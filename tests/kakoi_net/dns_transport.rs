use kakoi_net::dns_transport::{exchange_plain, PlainTransport};
use std::{
    io::{Read, Write},
    net::{TcpListener, UdpSocket},
    thread,
    time::{Duration, Instant},
};

fn query() -> Vec<u8> {
    let mut wire = vec![0x12, 0x34, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    wire.extend(b"\x03api\x07example\x03com\0\0\x01\0\x01");
    wire
}

// @kotowari[REQ-131]
#[test]
fn udp_ignores_other_sources_and_mismatched_answers_before_accepting_the_peer() {
    let upstream = UdpSocket::bind("127.0.0.1:0").unwrap();
    upstream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let peer = upstream.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut data = [0; 512];
        let (size, client) = upstream.recv_from(&mut data).unwrap();
        let mut response = data[..size].to_vec();
        response[2] |= 0x80;
        let attacker = UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut fake = response.clone();
        fake[3] = 3; // NXDOMAIN would be observable if the wrong source were trusted.
        attacker.send_to(&fake, client).unwrap();
        fake[0] ^= 1;
        upstream.send_to(&fake, client).unwrap();
        upstream.send_to(&[0; 8], client).unwrap();
        upstream.send_to(&response, client).unwrap();
    });
    let answer = exchange_plain(
        &query(),
        peer,
        PlainTransport::Udp,
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    assert!(answer.received_at <= Instant::now());
    assert_eq!(&answer.wire()[..2], &[0x12, 0x34]);
    assert_eq!(answer.wire()[3], 0);
    server.join().unwrap();
}

// @kotowari[REQ-131]
#[test]
fn tcp_reads_a_fragmented_length_prefixed_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let peer = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut length = [0; 2];
        stream.read_exact(&mut length).unwrap();
        let mut response = vec![0; u16::from_be_bytes(length) as usize];
        stream.read_exact(&mut response).unwrap();
        response[2] |= 0x80;
        for byte in length.into_iter().chain(response) {
            stream.write_all(&[byte]).unwrap();
        }
    });
    let answer = exchange_plain(
        &query(),
        peer,
        PlainTransport::Tcp,
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    assert!(answer.received_at <= Instant::now());
    assert_eq!(&answer.wire()[..2], &[0x12, 0x34]);
    server.join().unwrap();
}

// @kotowari[REQ-116]
#[test]
fn stalled_and_trickling_servers_cannot_extend_the_absolute_deadline() {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let start = Instant::now();
    let error = exchange_plain(
        &query(),
        socket.local_addr().unwrap(),
        PlainTransport::Udp,
        start + Duration::from_millis(100),
    )
    .err()
    .unwrap();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(start.elapsed() < Duration::from_secs(1));

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let peer = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut length = [0; 2];
        stream.read_exact(&mut length).unwrap();
        let mut request = vec![0; u16::from_be_bytes(length) as usize];
        stream.read_exact(&mut request).unwrap();
        for byte in [0, 100, 0, 0, 0, 0] {
            if stream.write_all(&[byte]).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(40));
        }
    });
    let start = Instant::now();
    let error = exchange_plain(
        &query(),
        peer,
        PlainTransport::Tcp,
        start + Duration::from_millis(110),
    )
    .err()
    .unwrap();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(start.elapsed() < Duration::from_millis(500));
    server.join().unwrap();
}

// @kotowari[REQ-114, REQ-115, REQ-113, REQ-135, EX-248, EX-246]
#[test]
fn candidates_follow_order_skip_duplicates_and_stop_on_negative_answers() {
    use kakoi_core::network::NetworkLimits;
    use kakoi_net::{dns_transport::exchange_plain_candidates, resolution::ResolutionBudget};
    let first = UdpSocket::bind("127.0.0.1:0").unwrap();
    let second = UdpSocket::bind("127.0.0.1:0").unwrap();
    let unused = UdpSocket::bind("127.0.0.1:0").unwrap();
    let peers = [
        first.local_addr().unwrap(),
        first.local_addr().unwrap(),
        second.local_addr().unwrap(),
        unused.local_addr().unwrap(),
    ];
    let servers: Vec<_> = [(first, 2), (second, 3)]
        .into_iter()
        .map(|(socket, code)| {
            thread::spawn(move || {
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut buffer = [0; 512];
                let (size, peer) = socket.recv_from(&mut buffer).unwrap();
                buffer[2] |= 0x80;
                buffer[3] = code;
                socket.send_to(&buffer[..size], peer).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_millis(200)))
                    .unwrap();
                assert!(socket.recv_from(&mut buffer).is_err(), "duplicate query");
            })
        })
        .collect();
    let limits = NetworkLimits {
        dns_max_upstream_queries: 2,
        ..NetworkLimits::default()
    };
    let mut budget = ResolutionBudget::new(Instant::now(), &limits).unwrap();
    let answer = exchange_plain_candidates(&query(), &peers, &mut budget).unwrap();
    assert_eq!(answer.wire()[3] & 15, 3);
    unused.set_nonblocking(true).unwrap();
    assert!(unused.recv_from(&mut [0; 512]).is_err());
    for server in servers {
        server.join().unwrap();
    }
}

// @kotowari[REQ-122, REQ-131]
#[test]
fn truncated_udp_uses_tcp_only_with_another_query_reservation() {
    use kakoi_core::network::NetworkLimits;
    use kakoi_net::{dns_transport::exchange_plain_candidates, resolution::ResolutionBudget};
    for maximum in [1, 2] {
        let (tcp, udp) = crate::common::tcp_and_udp_on_one_port();
        let peer = tcp.local_addr().unwrap();
        let server = thread::spawn(move || {
            udp.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut buffer = [0; 512];
            let (size, client) = udp.recv_from(&mut buffer).unwrap();
            buffer[2] |= 0x82;
            udp.send_to(&buffer[..size], client).unwrap();
            if maximum == 2 {
                let (mut stream, _) = tcp.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut length = [0; 2];
                stream.read_exact(&mut length).unwrap();
                let mut request = vec![0; u16::from_be_bytes(length) as usize];
                stream.read_exact(&mut request).unwrap();
                request[2] |= 0x80;
                stream.write_all(&length).unwrap();
                stream.write_all(&request).unwrap();
            } else {
                thread::sleep(Duration::from_millis(100));
                tcp.set_nonblocking(true).unwrap();
                assert!(
                    tcp.accept().is_err(),
                    "TCP started without a query reservation"
                );
            }
        });
        let limits = NetworkLimits {
            dns_max_upstream_queries: maximum,
            ..NetworkLimits::default()
        };
        let mut budget = ResolutionBudget::new(Instant::now(), &limits).unwrap();
        let result = exchange_plain_candidates(&query(), &[peer], &mut budget);
        assert_eq!(result.is_ok(), maximum == 2);
        server.join().unwrap();
    }
}

// @kotowari[REQ-131]
#[test]
fn tls_checks_the_certificate_name_and_chain_without_plaintext_fallback() {
    use kakoi_net::dns_transport::TlsClient;
    use std::{
        fs,
        io::{BufRead, BufReader},
        process::{Command, Stdio},
    };
    let temp = crate::common::TempDir::new();
    let path = temp.path();
    let commands: &[&[&str]] = &[
        &[
            "req",
            "-x509",
            "-newkey",
            "ec",
            "-pkeyopt",
            "ec_paramgen_curve:P-256",
            "-nodes",
            "-keyout",
            "ca.key",
            "-out",
            "ca.pem",
            "-days",
            "1",
            "-subj",
            "/CN=kakoi test CA",
        ],
        &[
            "req",
            "-newkey",
            "ec",
            "-pkeyopt",
            "ec_paramgen_curve:P-256",
            "-nodes",
            "-keyout",
            "server.key",
            "-out",
            "server.csr",
            "-subj",
            "/CN=dns.example.com",
        ],
        &[
            "x509",
            "-req",
            "-in",
            "server.csr",
            "-CA",
            "ca.pem",
            "-CAkey",
            "ca.key",
            "-set_serial",
            "1",
            "-out",
            "server.pem",
            "-days",
            "1",
            "-extfile",
            "server.ext",
        ],
        &["x509", "-in", "ca.pem", "-outform", "DER", "-out", "ca.der"],
    ];
    fs::write(path.join("server.ext"), "subjectAltName=DNS:dns.example.com\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=serverAuth\n").unwrap();
    for args in commands {
        let output = Command::new("/usr/bin/openssl")
            .args(*args)
            .current_dir(path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let trusted =
        TlsClient::from_root_certificates([fs::read(path.join("ca.der")).unwrap()]).unwrap();
    let untrusted = TlsClient::from_root_certificates([]).unwrap();
    for (client, name, success) in [
        (&trusted, "dns.example.com", true),
        (&trusted, "wrong.example.com", false),
        (&untrusted, "dns.example.com", false),
    ] {
        let mut server = Command::new("/usr/bin/python3")
            .args([
                "-u",
                "-c",
                r#"
import socket, ssl
context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain('server.pem', 'server.key')
listener = socket.socket()
listener.bind(('127.0.0.1', 0)); listener.listen(); listener.settimeout(3)
print(listener.getsockname()[1], flush=True)
connection, _ = listener.accept(); connection.settimeout(2)
try:
    with context.wrap_socket(connection, server_side=True) as tls:
        def read(n):
            data = b''
            while len(data) < n:
                part = tls.recv(n-len(data))
                if not part: raise EOFError()
                data += part
            return data
        size = read(2)
        reply = bytearray(read(int.from_bytes(size, 'big')))
        reply[2] |= 128
        tls.sendall(size + reply)
except ssl.SSLError:
    pass
listener.settimeout(.1)
try:
    listener.accept()
    raise RuntimeError('unexpected plaintext fallback')
except socket.timeout:
    pass
"#,
            ])
            .current_dir(path)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut port = String::new();
        BufReader::new(server.stdout.take().unwrap())
            .read_line(&mut port)
            .unwrap();
        let peer: std::net::SocketAddr = format!("127.0.0.1:{}", port.trim()).parse().unwrap();
        let text = format!(
            "[[network.dns-upstream]]\ntransport='tls'\nip='127.0.0.1'\nport={}\ntls-name='{name}'",
            peer.port()
        );
        let policy =
            kakoi_core::policy::parse_policy(&text, std::path::Path::new("tls.toml")).unwrap();
        let mut budget = kakoi_net::resolution::ResolutionBudget::new(
            Instant::now(),
            &kakoi_core::network::NetworkLimits::default(),
        )
        .unwrap();
        let result = kakoi_net::dns_transport::exchange_upstreams(
            &query(),
            &policy.network.dns_upstream,
            Some(client),
            &mut budget,
        );
        assert_eq!(result.is_ok(), success, "TLS outcome differs for {name}");
        assert!(server.wait().unwrap().success());
    }
}

// @kotowari[REQ-116, REQ-131]
#[test]
fn a_stalled_tls_handshake_obeys_the_resolution_deadline() {
    use kakoi_net::dns_transport::TlsClient;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let peer = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (_stream, _) = listener.accept().unwrap();
        thread::sleep(Duration::from_millis(300));
    });
    let client = TlsClient::from_root_certificates([]).unwrap();
    let start = Instant::now();
    let error = client
        .exchange(
            &query(),
            peer,
            "dns.example.com",
            start + Duration::from_millis(100),
        )
        .err()
        .unwrap();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(start.elapsed() < Duration::from_millis(250));
    server.join().unwrap();
}
