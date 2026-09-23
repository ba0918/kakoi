use kakoi_net::{
    dns_front::DnsFront,
    namespace::{DnsSockets, NetworkNamespace},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

// @kotowari[REQ-131, REQ-130]
#[test]
fn frontend_handles_udp_limits_and_fragmented_pipelined_tcp_without_blocking() {
    let namespace = Arc::new(NetworkNamespace::create().unwrap());
    assert!(namespace
        .command("/usr/sbin/ip")
        .unwrap()
        .args(["link", "set", "lo", "up"])
        .status()
        .unwrap()
        .success());
    let mut front = DnsFront::new(
        DnsSockets::bind(Arc::clone(&namespace)).unwrap(),
        4,
        Duration::from_secs(2),
    )
    .unwrap();
    let mut client = namespace
        .command("/usr/bin/python3")
        .unwrap()
        .args([
            "-c",
            r#"
import socket, struct, time
query = b'\x12\x34\x01\0\0\x01\0\0\0\0\0\0\x03api\x07example\x03com\0\0\x10\0\x01'
with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as udp:
    udp.settimeout(2)
    udp.sendto(query, ('127.0.0.53', 53))
    reply = udp.recv(65535)
    assert len(reply) <= 512 and reply[2] & 2 and reply[6:8] == b'\0\0'
with socket.create_connection(('127.0.0.53', 53), timeout=2) as tcp:
    second = b'\x56\x78' + query[2:]
    frame = struct.pack('!H', len(query)) + query
    tcp.sendall(frame[:1]); time.sleep(.02)
    tcp.sendall(frame[1:] + struct.pack('!H', len(second)) + second)
    def read(n):
        data = b''
        while len(data) < n:
            part = tcp.recv(n-len(data))
            assert part
            data += part
        return data
    for identity in [query[:2], second[:2]]:
        reply = read(struct.unpack('!H', read(2))[0])
        assert len(reply) > 512 and not reply[2] & 2 and reply[:2] == identity
"#,
        ])
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut handled = 0;
    loop {
        assert!(
            Instant::now() < deadline,
            "frontend failed to make progress"
        );
        for request in front.poll(Instant::now()).unwrap() {
            let mut answer = request.wire;
            answer[2] |= 0x80;
            answer[7] = 1;
            answer.extend([0xc0, 0x0c, 0, 16, 0, 1, 0, 0, 0, 30]);
            answer.extend(753_u16.to_be_bytes());
            for _ in 0..3 {
                answer.push(250);
                answer.extend([b'x'; 250]);
            }
            assert!(front
                .respond(request.reply, &answer, Instant::now())
                .unwrap());
            handled += 1;
        }
        if let Some(status) = client.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(handled, 3);
}

// @kotowari[REQ-131]
#[test]
fn tcp_capacity_and_expiration_do_not_block_udp_or_deliver_stale_replies() {
    let namespace = Arc::new(NetworkNamespace::create().unwrap());
    assert!(namespace
        .command("/usr/sbin/ip")
        .unwrap()
        .args(["link", "set", "lo", "up"])
        .status()
        .unwrap()
        .success());
    let mut front = DnsFront::new(
        DnsSockets::bind(Arc::clone(&namespace)).unwrap(),
        1,
        Duration::from_secs(1),
    )
    .unwrap();
    let mut client = namespace
        .command("/usr/bin/python3")
        .unwrap()
        .args([
            "-c",
            r#"
import socket, struct
base = b'\0\0\x01\0\0\x01\0\0\0\0\0\0\x03api\x07example\x03com\0\0\x01\0\x01'
def query(identity): return bytes([identity, 0]) + base[2:]
def send(sock, identity):
    data = query(identity)
    sock.sendall(struct.pack('!H', len(data)) + data)
with socket.create_connection(('127.0.0.53', 53), timeout=2) as old:
    send(old, 10)
    with socket.create_connection(('127.0.0.53', 53), timeout=2) as excess:
        try:
            send(excess, 99)
            assert excess.recv(2) == b''
        except ConnectionResetError: pass
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as udp:
        udp.settimeout(2)
        udp.sendto(query(20), ('127.0.0.53', 53))
        assert udp.recv(512)[0] == 20
    assert old.recv(2) == b''
with socket.create_connection(('127.0.0.53', 53), timeout=2) as new:
    send(new, 11)
    size = new.recv(2)
    assert len(size) == 2
    payload = b''
    while len(payload) < int.from_bytes(size, 'big'):
        payload += new.recv(512)
    assert payload[0] == 11
"#,
        ])
        .spawn()
        .unwrap();
    let start = Instant::now();
    let mut offset = Duration::ZERO;
    let mut old = None;
    let mut saw_udp = false;
    let mut saw_new = false;
    loop {
        assert!(start.elapsed() < Duration::from_secs(3));
        let now = Instant::now() + offset;
        for request in front.poll(now).unwrap() {
            let mut answer = request.wire;
            answer[2] |= 0x80;
            match answer[0] {
                10 => old = Some(request.reply),
                20 => {
                    assert!(front.respond(request.reply, &answer, now).unwrap());
                    saw_udp = true;
                }
                11 => {
                    let stale = old.take().unwrap();
                    assert!(!front.respond(stale, &answer, now).unwrap());
                    assert!(front.respond(request.reply, &answer, now).unwrap());
                    saw_new = true;
                }
                other => panic!("capacity was bypassed by {other}"),
            }
        }
        if saw_udp {
            offset = Duration::from_secs(3);
        }
        if let Some(status) = client.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(saw_udp && saw_new);
}

// @kotowari[REQ-027, REQ-116, REQ-127, REQ-130, REQ-131, EX-043, EX-286]
#[test]
fn service_shares_udp_tcp_resolution_while_rejections_remain_responsive() {
    use kakoi_core::{
        network::{Allow, Destination, NetworkLimits, Protocol},
        policy::parse_policy,
    };
    use kakoi_net::{
        dns_runtime::{DnsRuntime, DnsRuntimeConfig},
        scope::AddressContext,
    };
    use std::{net::UdpSocket, thread};
    let namespace = Arc::new(NetworkNamespace::create().unwrap());
    assert!(namespace
        .command("/usr/sbin/ip")
        .unwrap()
        .args(["link", "set", "lo", "up"])
        .status()
        .unwrap()
        .success());
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let parsed = parse_policy(
        &format!(
            "[[network.dns-upstream]]\ntransport='plain'\nip='127.0.0.1'\nport={}",
            socket.local_addr().unwrap().port()
        ),
        std::path::Path::new("upstream.toml"),
    )
    .unwrap();
    let upstream = thread::spawn(move || {
        let mut wire = [0; 512];
        let (size, peer) = socket.recv_from(&mut wire).unwrap();
        thread::sleep(Duration::from_millis(300));
        wire[2] |= 0x80;
        socket.send_to(&wire[..size], peer).unwrap();
        socket.set_nonblocking(true).unwrap();
        assert_eq!(
            socket.recv_from(&mut wire).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    });
    let policy = vec![Allow {
        destination: Destination::Dns("api.example.com".parse().unwrap()),
        protocol: Protocol::Tcp,
        ports: vec!["443".into()].try_into().unwrap(),
    }];
    kakoi_net::nft::apply(
        &namespace,
        std::path::Path::new("/usr/sbin/nft"),
        &kakoi_net::filter::compile_static(&[], 120).unwrap(),
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    let mut runtime = DnsRuntime::new(
        Arc::clone(&namespace),
        DnsRuntimeConfig {
            policy,
            upstreams: parsed.network.dns_upstream,
            limits: NetworkLimits {
                dns_max_concurrent_resolutions: 1,
                ..NetworkLimits::default()
            },
            trust: None,
            nft: "/usr/sbin/nft".into(),
            scope: AddressContext::default(),
            generation: 0,
            host_dns: None,
        },
        |_| None,
    )
    .unwrap();
    let mut client = namespace.command("/usr/bin/python3").unwrap().args(["-c", r#"
import socket, struct
base = b'\x12\x34\x01\0\0\x01\0\0\0\0\0\0\x03api\x07example\x03com\0\0\x10\0\x01'
with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as udp, socket.create_connection(('127.0.0.53',53),timeout=2) as tcp:
    udp.settimeout(2)
    udp.sendto(base, ('127.0.0.53',53))
    second = b'\x56\x78' + base[2:]
    tcp.sendall(struct.pack('!H',len(second))+second)
    denied = b'\x99\x88'+base[2:].replace(b'api',b'bad')
    udp.sendto(denied, ('127.0.0.53',53))
    answer = udp.recv(512)
    assert answer[:2] == denied[:2] and answer[3]&15 == 5, answer
    malformed = b'\xaa\xbb\x01\0\0\x01\0\0\0\0\0\0'
    udp.sendto(malformed, ('127.0.0.53',53))
    answer = udp.recv(512)
    assert answer[:2] == malformed[:2] and answer[3]&15 == 1, answer
    unsupported = b'\xcc\xdd' + base[2:-4] + b'\0\xff\0\x01'
    udp.sendto(unsupported, ('127.0.0.53',53))
    answer = udp.recv(512)
    assert answer[:2] == unsupported[:2] and answer[3]&15 == 4, answer
    answer = udp.recv(512)
    assert answer[:2] == base[:2] and answer[3]&15 == 0, answer
    def read(n):
        data=b''
        while len(data)<n:
            part=tcp.recv(n-len(data)); assert part
            data+=part
        return data
    answer=read(struct.unpack('!H',read(2))[0])
    assert answer[:2] == second[:2] and answer[3]&15 == 0, answer
"#]).spawn().unwrap();
    let start = Instant::now();
    loop {
        assert!(start.elapsed() < Duration::from_secs(4));
        runtime.poll(Instant::now()).unwrap();
        if let Some(status) = client.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    runtime.stop();
    let deadline = Instant::now() + Duration::from_secs(1);
    while !runtime.is_finished() {
        runtime.poll(Instant::now()).unwrap();
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    upstream.join().unwrap();
}

// @kotowari[REQ-116, REQ-123, REQ-131]
#[test]
fn service_returns_tcp_servfail_when_resolution_deadline_expires() {
    use kakoi_core::network::{Allow, Destination, Protocol};
    use kakoi_net::{dns::DnsRequests, dns_service::DnsService};
    let namespace = Arc::new(NetworkNamespace::create().unwrap());
    assert!(namespace
        .command("/usr/sbin/ip")
        .unwrap()
        .args(["link", "set", "lo", "up"])
        .status()
        .unwrap()
        .success());
    let front = DnsFront::new(
        DnsSockets::bind(Arc::clone(&namespace)).unwrap(),
        1,
        Duration::from_secs(1),
    )
    .unwrap();
    let rules = vec![Allow {
        destination: Destination::Dns("api.example.com".parse().unwrap()),
        protocol: Protocol::Tcp,
        ports: vec!["443".into()].try_into().unwrap(),
    }];
    let requests = DnsRequests::new(rules, 0, 1, 1, Duration::from_secs(1)).unwrap();
    let mut service = DnsService::new(front, requests);
    let mut client = namespace
        .command("/usr/bin/python3")
        .unwrap()
        .args([
            "-c",
            r#"
import socket,struct
query=b'\x12\x34\x01\0\0\x01\0\0\0\0\0\0\x03api\x07example\x03com\0\0\x01\0\x01'
with socket.create_connection(('127.0.0.53',53),timeout=2) as s:
    s.sendall(struct.pack('!H',len(query))+query)
    def read(n):
        data=b''
        while len(data)<n:
            part=s.recv(n-len(data)); assert part, 'deadline closed TCP without SERVFAIL'
            data+=part
        return data
    reply=read(struct.unpack('!H',read(2))[0])
    assert reply[:2]==query[:2] and reply[3]&15==2
"#,
        ])
        .spawn()
        .unwrap();
    let start = Instant::now();
    let mut clock = None;
    loop {
        assert!(start.elapsed() < Duration::from_secs(3));
        for task in service.poll(clock.unwrap_or_else(Instant::now)).unwrap() {
            assert!(clock.is_none());
            clock = Some(task.deadline);
        }
        if let Some(status) = client.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(clock.is_some());
}
