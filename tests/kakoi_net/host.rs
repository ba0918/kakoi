use crate::common::{binary, TempDir, RW_WORKSPACE};
use kakoi_net::{
    dns::{AcceptedRequest, DnsRequests},
    host::{HOST_LOOPBACK_V4, HOST_LOOPBACK_V6},
};
use std::{
    net::{IpAddr, TcpListener, UdpSocket},
    path::PathBuf,
    time::{Duration, Instant},
};

fn query(name: &str, kind: u16) -> Vec<u8> {
    let mut wire = vec![0x43, 0x21, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in name.trim_end_matches('.').split('.') {
        wire.push(label.len() as u8);
        wire.extend(label.as_bytes());
    }
    wire.push(0);
    wire.extend(kind.to_be_bytes());
    wire.extend([0, 1]);
    wire
}

/// The addresses in the answer section of a response to `query`.
fn answered(response: &[u8], question: usize) -> (u8, Vec<IpAddr>) {
    let rcode = response[3] & 15;
    let count = u16::from_be_bytes([response[6], response[7]]);
    let mut at = question;
    let mut addresses = Vec::new();
    for _ in 0..count {
        // A compression pointer to the question name.
        assert_eq!(response[at] & 0xc0, 0xc0);
        at += 2;
        let kind = u16::from_be_bytes([response[at], response[at + 1]]);
        let length = u16::from_be_bytes([response[at + 8], response[at + 9]]) as usize;
        let data = &response[at + 10..at + 10 + length];
        addresses.push(match kind {
            1 => IpAddr::from(<[u8; 4]>::try_from(data).unwrap()),
            28 => IpAddr::from(<[u8; 16]>::try_from(data).unwrap()),
            other => panic!("unexpected record type {other}"),
        });
        at += 10 + length;
    }
    (rcode, addresses)
}

// @kotowari[REQ-090]
#[test]
fn reserved_host_names_are_answered_locally_whatever_the_policy() {
    let mut requests = DnsRequests::new(vec![], 0, 4, 4, Duration::from_secs(5)).unwrap();
    for (name, kind, expected) in [
        (
            "host-v4.kakoi.internal",
            1,
            vec![IpAddr::V4(HOST_LOOPBACK_V4)],
        ),
        (
            "HOST-V4.kakoi.internal.",
            1,
            vec![IpAddr::V4(HOST_LOOPBACK_V4)],
        ),
        ("host-v4.kakoi.internal", 28, vec![]),
        (
            "host-v6.kakoi.internal",
            28,
            vec![IpAddr::V6(HOST_LOOPBACK_V6)],
        ),
        ("host-v6.kakoi.internal", 1, vec![]),
    ] {
        let wire = query(name, kind);
        let Ok(AcceptedRequest::Answer(reply)) = requests.accept(&wire, (), Instant::now()) else {
            panic!("{name} was not answered locally");
        };
        assert_eq!(&reply.wire[..2], &wire[..2]);
        assert_eq!(answered(&reply.wire, wire.len()), (0, expected), "{name}");
    }
    // Other names under the reserved domain are not served.
    let Ok(AcceptedRequest::Answer(reply)) =
        requests.accept(&query("other.kakoi.internal", 1), (), Instant::now())
    else {
        panic!("an unauthorized name reached resolution");
    };
    assert_eq!(reply.wire[3] & 15, 5);
}

/// The real pasta, required: these tests observe the traffic it carries.
fn pasta() -> PathBuf {
    std::env::var_os("KAKOI_TEST_PASTA")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::split_paths(&std::env::var_os("PATH")?)
                .map(|directory| directory.join("pasta"))
                .find(|candidate| candidate.is_file())
        })
        .expect("the host-loopback tests need pasta: set KAKOI_TEST_PASTA or put it on PATH")
}

fn filtered(allow: &str) -> (TempDir, PathBuf, TempDir) {
    let home = TempDir::new();
    let workspace = home.path().join("ws");
    std::fs::create_dir(&workspace).unwrap();
    home.write(
        ".config/kakoi/profile/default.toml",
        format!(
            "{RW_WORKSPACE}\n[network]\nmode = 'filtered'\n\n[[network.dns-upstream]]\ntransport = 'plain'\nip = '127.0.0.1'\nport = 9\n{allow}"
        ),
    );
    let bin = TempDir::new();
    // pasta selects its mode by the name it is started under.
    std::os::unix::fs::symlink(pasta(), bin.path().join("pasta")).unwrap();
    (home, workspace, bin)
}

/// Runs `script` with the sandbox's Python and returns its standard output.
fn inside(home: &TempDir, workspace: &PathBuf, bin: &TempDir, script: &str) -> String {
    let output = binary(home.path())
        .env(
            "PATH",
            format!("{}:/usr/sbin:/usr/bin:/bin", bin.path().display()),
        )
        .current_dir(workspace)
        .args(["--", "/usr/bin/python3", "-c", script])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    // kakoi's notifications help to explain an unexpected result.
    eprintln!("{stderr}");
    String::from_utf8(output.stdout).unwrap()
}

fn tcp_echo(address: &str) -> (TcpListener, u16, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind((address, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = listener.try_clone().unwrap();
    let handle = std::thread::spawn(move || {
        use std::io::Write;
        // Gives up after a while, so that a refused connection cannot hang the test.
        server.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if let Ok((mut stream, _)) = server.accept() {
                let _ = stream.write_all(b"host-tcp");
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    (listener, port, handle)
}

const CONNECT: &str = r#"
import socket, sys
name, port, kind, wait = sys.argv[1], int(sys.argv[2]), sys.argv[3], float(sys.argv[4])
family = socket.AF_INET6 if 'v6' in name else socket.AF_INET
try:
    address = socket.getaddrinfo(name, port, family)[0][4][0]
    with socket.socket(family, socket.SOCK_STREAM if kind == 'tcp' else socket.SOCK_DGRAM) as s:
        s.settimeout(wait)
        s.connect((address, port))
        if kind == 'udp':
            s.send(b'probe')
        print(address, s.recv(32).decode())
except OSError as error:
    print('failed', type(error).__name__)
"#;

/// A permitted exchange may take long on a loaded host: its wait only guards
/// against a hang. A refused one is dropped, so any wait ends the same way.
fn connect(name: &str, port: u16, kind: &str) -> String {
    exchange(name, port, kind, 20)
}

fn refused(name: &str, port: u16, kind: &str) -> String {
    exchange(name, port, kind, 3)
}

fn exchange(name: &str, port: u16, kind: &str, wait: u32) -> String {
    format!("import sys\nsys.argv = ['connect', {name:?}, '{port}', {kind:?}, '{wait}']\n{CONNECT}")
}

// @kotowari[EX-190, EX-192]
#[test]
fn the_host_v4_name_reaches_only_the_permitted_host_loopback_port() {
    let (listener, port, server) = tcp_echo("127.0.0.1");
    // A listening service on a port the policy does not name.
    let other = TcpListener::bind("127.0.0.1:0").unwrap();
    let other_port = other.local_addr().unwrap().port();
    let (home, workspace, bin) = filtered(&format!(
        "\n[[network.allow]]\ndestination = {{ host-loopback = 'ipv4' }}\nprotocol = 'tcp'\nports = ['{port}']\n"
    ));
    let output = inside(
        &home,
        &workspace,
        &bin,
        &format!(
            "{}\n{}",
            connect("host-v4.kakoi.internal", port, "tcp"),
            refused("host-v4.kakoi.internal", other_port, "tcp")
        ),
    );
    assert_eq!(
        output,
        format!("{HOST_LOOPBACK_V4} host-tcp\nfailed TimeoutError\n"),
        "{output}"
    );
    server.join().unwrap();
    drop((listener, other));
}

// @kotowari[EX-191]
#[test]
fn the_host_v6_name_reaches_a_permitted_udp_service() {
    let socket = UdpSocket::bind("[::1]:0").unwrap();
    let port = socket.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let mut buffer = [0; 64];
        socket
            .set_read_timeout(Some(Duration::from_secs(30)))
            .unwrap();
        if let Ok((_, peer)) = socket.recv_from(&mut buffer) {
            socket.send_to(b"host-udp", peer).unwrap();
        }
    });
    let (home, workspace, bin) = filtered(&format!(
        "\n[[network.allow]]\ndestination = {{ host-loopback = 'ipv6' }}\nprotocol = 'udp'\nports = ['{port}']\n"
    ));
    let output = inside(
        &home,
        &workspace,
        &bin,
        &connect("host-v6.kakoi.internal", port, "udp"),
    );
    assert_eq!(output, format!("{HOST_LOOPBACK_V6} host-udp\n"), "{output}");
    server.join().unwrap();
}

// @kotowari[EX-194]
#[test]
fn a_dns_wildcard_does_not_permit_the_host_loopback() {
    let (listener, port, _) = tcp_echo("127.0.0.1");
    let (home, workspace, bin) = filtered(&format!(
        "\n[[network.allow]]\ndestination = {{ dns = '*.kakoi.internal' }}\nprotocol = 'tcp'\nports = ['{port}']\n"
    ));
    let output = inside(
        &home,
        &workspace,
        &bin,
        &refused("host-v4.kakoi.internal", port, "tcp"),
    );
    assert_eq!(output, "failed TimeoutError\n", "{output}");
    drop(listener);
}
