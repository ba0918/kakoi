use kakoi_net::namespace::NetworkNamespace;

// @kotowari[REQ-060, REQ-150]
#[test]
fn transit_controller_can_enter_a_nested_application_network() {
    let transit = NetworkNamespace::create().unwrap();
    let app = NetworkNamespace::create_within(&transit).unwrap();
    let pid = app.keeper_pid();
    let output = transit
        .command("/usr/bin/python3")
        .unwrap()
        .args([
            "-c",
            r#"
import ctypes, os, sys
libc = ctypes.CDLL(None, use_errno=True)
for kind, flag in [('user', 0x10000000), ('net', 0x40000000)]:
    fd = os.open('/proc/' + sys.argv[1] + '/ns/' + kind, os.O_RDONLY)
    assert libc.setns(fd, flag) == 0, (kind, ctypes.get_errno())
    os.close(fd)
print(os.readlink('/proc/self/ns/net'))
"#,
            &pid.to_string(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        std::fs::read_link(format!("/proc/{pid}/ns/net"))
            .unwrap()
            .to_str()
            .unwrap()
    );
    let denied = app
        .command("/usr/bin/python3")
        .unwrap()
        .args([
            "-c",
            r#"
import ctypes, errno, os, sys
libc = ctypes.CDLL(None, use_errno=True)
for kind, flag in [('user', 0x10000000), ('net', 0x40000000)]:
    try:
        fd = os.open('/proc/' + sys.argv[1] + '/ns/' + kind, os.O_RDONLY)
    except PermissionError:
        continue
    try:
        assert libc.setns(fd, flag) == -1
        assert ctypes.get_errno() == errno.EPERM
    finally:
        os.close(fd)
"#,
            &transit.keeper_pid().to_string(),
        ])
        .output()
        .unwrap();
    assert!(
        denied.status.success(),
        "{}",
        String::from_utf8_lossy(&denied.stderr)
    );
}

// @kotowari[REQ-060, REQ-150]
#[test]
fn namespace_outlives_its_creating_thread_and_keeps_command_descriptors_owned() {
    let namespace = std::thread::spawn(NetworkNamespace::create)
        .join()
        .unwrap()
        .unwrap();
    let pid = namespace.keeper_pid();
    let mut command = namespace.command("/bin/true").unwrap();
    assert!(command.status().unwrap().success());
    drop(command);
    drop(namespace);
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
}

// @kotowari[REQ-060, REQ-150]
#[test]
fn rust_caller_owns_independent_network_namespaces_and_reaps_their_keepers() {
    let host = std::fs::read_link("/proc/self/ns/net").unwrap();
    let first = NetworkNamespace::create().unwrap();
    let second = NetworkNamespace::create().unwrap();
    let first_pid = first.keeper_pid();
    let second_pid = second.keeper_pid();
    let first_identity = std::fs::read_link(format!("/proc/{first_pid}/ns/net")).unwrap();
    let second_identity = std::fs::read_link(format!("/proc/{second_pid}/ns/net")).unwrap();
    assert_ne!(host, first_identity);
    assert_ne!(first_identity, second_identity);
    let output = first
        .command("/usr/bin/readlink")
        .unwrap()
        .arg("/proc/self/ns/net")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        first_identity.to_str().unwrap()
    );
    drop(first);
    assert!(!std::path::Path::new(&format!("/proc/{first_pid}")).exists());
    assert!(second
        .command("/bin/true")
        .unwrap()
        .status()
        .unwrap()
        .success());
    assert_eq!(std::fs::read_link("/proc/self/ns/net").unwrap(), host);
    drop(second);
    assert!(!std::path::Path::new(&format!("/proc/{second_pid}")).exists());
}

// @kotowari[REQ-027, REQ-150]
#[test]
fn dns_sockets_accept_isolated_clients_without_moving_the_controller_namespace() {
    use kakoi_net::namespace::DnsSockets;
    use std::{
        io::{Read, Write},
        os::fd::AsRawFd,
        sync::Arc,
        time::{Duration, Instant},
    };
    let before = std::fs::read_link("/proc/self/ns/net").unwrap();
    let transit = NetworkNamespace::create().unwrap();
    let namespace = Arc::new(NetworkNamespace::create_within(&transit).unwrap());
    assert!(namespace
        .command("/usr/sbin/ip")
        .unwrap()
        .args(["link", "set", "lo", "up"])
        .status()
        .unwrap()
        .success());
    let sockets = DnsSockets::bind(Arc::clone(&namespace)).unwrap();
    assert_eq!(std::fs::read_link("/proc/self/ns/net").unwrap(), before);
    for fd in [sockets.udp.as_raw_fd(), sockets.tcp.as_raw_fd()] {
        assert_ne!(
            unsafe { libc::fcntl(fd, libc::F_GETFD) } & libc::FD_CLOEXEC,
            0
        );
    }
    assert!(DnsSockets::bind(Arc::clone(&namespace)).is_err());
    let mut client = namespace
        .command("/usr/bin/python3")
        .unwrap()
        .args([
            "-c",
            r#"
import socket
for kind, payload in [(socket.SOCK_DGRAM, b'udp'), (socket.SOCK_STREAM, b'tcp')]:
    with socket.socket(socket.AF_INET, kind) as sock:
        sock.settimeout(2)
        sock.connect(('127.0.0.53', 53))
        sock.sendall(payload)
        assert sock.recv(16) == payload
"#,
        ])
        .spawn()
        .unwrap();
    sockets
        .udp
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut bytes = [0; 16];
    let (size, peer) = sockets.udp.recv_from(&mut bytes).unwrap();
    assert_eq!(&bytes[..size], b"udp");
    sockets.udp.send_to(&bytes[..size], peer).unwrap();
    sockets.tcp.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut stream = loop {
        match sockets.tcp.accept() {
            Ok((stream, _)) => break stream,
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(error) => panic!("{error}"),
        }
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut payload = [0; 3];
    stream.read_exact(&mut payload).unwrap();
    assert_eq!(&payload, b"tcp");
    stream.write_all(&payload).unwrap();
    assert!(client.wait().unwrap().success());
    drop(stream);
    drop(sockets);
    // Another test's child, forked but not yet executing, may briefly hold a
    // copy of the dropped sockets; the port must come free, not at once.
    let deadline = Instant::now() + Duration::from_secs(10);
    let rebound = loop {
        match DnsSockets::bind(Arc::clone(&namespace)) {
            Ok(rebound) => break rebound,
            Err(error)
                if error.kind() == std::io::ErrorKind::AddrInUse && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(10))
            }
            Err(error) => panic!("{error}"),
        }
    };
    assert_eq!(rebound.udp.local_addr().unwrap().port(), 53);
}
