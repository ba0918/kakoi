use kakoi_core::network::{Ports, Protocol};
use kakoi_net::{
    filter::{self, FilterRule},
    namespace::NetworkNamespace,
    nft,
};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, Stdio},
    time::{Duration, Instant},
};

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn ip(namespace: &NetworkNamespace, args: &[&str]) {
    let output = namespace
        .command("/usr/sbin/ip")
        .unwrap()
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// @kotowari[REQ-001, REQ-030, REQ-015, REQ-016, REQ-017, EX-079, EX-313, EX-028]
#[test]
fn kernel_filter_matches_address_protocol_and_port_in_both_families() {
    let client = NetworkNamespace::create().unwrap();
    let server = NetworkNamespace::create_within(&client).unwrap();
    ip(
        &client,
        &[
            "link",
            "add",
            "app0",
            "type",
            "veth",
            "peer",
            "name",
            "srv0",
            "netns",
            &server.keeper_pid().to_string(),
        ],
    );
    for (namespace, interface, suffix) in [(&client, "app0", "1"), (&server, "srv0", "2")] {
        ip(namespace, &["link", "set", "lo", "up"]);
        ip(namespace, &["link", "set", interface, "up"]);
        ip(
            namespace,
            &[
                "addr",
                "add",
                &format!("198.18.0.{suffix}/24"),
                "dev",
                interface,
            ],
        );
        ip(
            namespace,
            &[
                "-6",
                "addr",
                "add",
                &format!("fd00:1::{suffix}/64"),
                "dev",
                interface,
                "nodad",
            ],
        );
    }
    ip(&server, &["addr", "add", "198.18.0.3/24", "dev", "srv0"]);
    ip(
        &server,
        &["-6", "addr", "add", "fd00:1::3/64", "dev", "srv0", "nodad"],
    );
    ip(&server, &["addr", "add", "1.1.1.1/32", "dev", "lo"]);
    ip(
        &server,
        &[
            "-6",
            "addr",
            "add",
            "2606:4700:4700::1111/128",
            "dev",
            "lo",
            "nodad",
        ],
    );
    ip(
        &client,
        &["route", "add", "1.1.1.1/32", "via", "198.18.0.2"],
    );
    ip(
        &client,
        &[
            "-6",
            "route",
            "add",
            "2606:4700:4700::1111/128",
            "via",
            "fd00:1::2",
        ],
    );
    let script = filter::compile_static(
        &[
            FilterRule {
                network: "198.18.0.2/32".parse().unwrap(),
                protocol: Protocol::Tcp,
                ports: Ports::try_from(vec!["8080".into()]).unwrap(),
            },
            FilterRule {
                network: "fd00:1::2/128".parse().unwrap(),
                protocol: Protocol::Udp,
                ports: Ports::try_from(vec!["8080".into()]).unwrap(),
            },
        ],
        1,
    )
    .unwrap();
    nft::apply(
        &client,
        Path::new("/usr/sbin/nft"),
        &script,
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    let mut server = Server(server.command("/usr/bin/python3").unwrap().args(["-c", r#"
import socket, socketserver, threading, sys
class TCP(socketserver.BaseRequestHandler):
    def handle(self):
        while True:
            data = self.request.recv(64)
            if not data: return
            self.request.sendall(data)
class UDP(socketserver.BaseRequestHandler):
    def handle(self): self.request[1].sendto(self.request[0], self.client_address)
servers = []
for af, address in [(socket.AF_INET, '198.18.0.2'), (socket.AF_INET, '198.18.0.3'), (socket.AF_INET6, 'fd00:1::2'), (socket.AF_INET6, 'fd00:1::3'), (socket.AF_INET, '1.1.1.1'), (socket.AF_INET6, '2606:4700:4700::1111')]:
    for base, handler in [(socketserver.ThreadingTCPServer, TCP), (socketserver.ThreadingUDPServer, UDP)]:
        class Service(base):
            address_family = af
            allow_reuse_address = True
            daemon_threads = True
        for port in [8080, 8081]:
            service = Service((address, port), handler)
            servers.append(service)
            threading.Thread(target=service.serve_forever, daemon=True).start()
print('ready', flush=True)
sys.stdin.read()
"#]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap());
    let mut ready = String::new();
    BufReader::new(server.0.stdout.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    assert_eq!(ready, "ready\n");
    let output = client.command("/usr/bin/python3").unwrap().args(["-c", r#"
import socket
for af, address in [(socket.AF_INET, '198.18.0.2'), (socket.AF_INET, '198.18.0.3'), (socket.AF_INET6, 'fd00:1::2'), (socket.AF_INET6, 'fd00:1::3')]:
    for kind in [socket.SOCK_STREAM, socket.SOCK_DGRAM]:
        for port in [8080, 8081]:
            expected = address.endswith('2') and port == 8080 and ((af == socket.AF_INET and kind == socket.SOCK_STREAM) or (af == socket.AF_INET6 and kind == socket.SOCK_DGRAM))
            with socket.socket(af, kind) as sock:
                sock.settimeout(.2)
                try:
                    sock.connect((address, port))
                    sock.sendall(b'payload')
                    actual = sock.recv(64) == b'payload'
                except (TimeoutError, PermissionError): actual = False
                assert actual == expected, (address, kind, port, actual, expected)
"#]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut ongoing = Server(client.command("/usr/bin/python3").unwrap().args(["-c", r#"
import socket, sys
with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as tcp, socket.socket(socket.AF_INET6, socket.SOCK_DGRAM) as udp:
    tcp.settimeout(.3)
    udp.settimeout(.3)
    tcp.connect(('198.18.0.2', 8080))
    udp.bind(('fd00:1::1', 20001))
    udp.connect(('fd00:1::2', 8080))
    for phase in range(3):
        if phase: assert sys.stdin.readline()
        tcp.sendall(b'tcp')
        assert tcp.recv(64) == b'tcp'
        try:
            udp.send(b'udp')
            received = udp.recv(64) == b'udp'
        except (TimeoutError, PermissionError): received = False
        assert received == (phase < 2), (phase, received)
        if phase == 1:
            with socket.socket(socket.AF_INET6, socket.SOCK_DGRAM) as fresh:
                fresh.bind(('fd00:1::1', 20002))
                fresh.settimeout(.2)
                try:
                    fresh.sendto(b'new', ('fd00:1::2', 8080))
                    fresh.recv(64)
                    raise AssertionError('new UDP tuple was allowed')
                except (TimeoutError, PermissionError): pass
        print('phase' + str(phase), flush=True)
"#]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap());
    let mut replies = BufReader::new(ongoing.0.stdout.take().unwrap());
    let mut line = String::new();
    replies.read_line(&mut line).unwrap();
    assert_eq!(line, "phase0\n");
    nft::apply(
        &client,
        Path::new("/usr/sbin/nft"),
        "flush chain inet kakoi_policy permitted\n",
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    ongoing
        .0
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"continue\n")
        .unwrap();
    line.clear();
    replies.read_line(&mut line).unwrap();
    assert_eq!(line, "phase1\n");
    std::thread::sleep(Duration::from_millis(1500));
    ongoing
        .0
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"after idle\n")
        .unwrap();
    line.clear();
    replies.read_line(&mut line).unwrap();
    assert_eq!(line, "phase2\n");
    assert!(ongoing.0.wait().unwrap().success());

    use kakoi_core::network::{Allow, Destination};
    use kakoi_net::{dynamic::DynamicPermissions, leases::ActiveGrant};
    nft::apply(
        &client,
        Path::new("/usr/sbin/nft"),
        "add rule inet kakoi_policy permitted jump dns_permitted\n",
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    let rules: Vec<Allow> = [Protocol::Tcp, Protocol::Udp]
        .into_iter()
        .map(|protocol| Allow {
            destination: Destination::Dns("api.example.com".parse().unwrap()),
            protocol,
            ports: Ports::try_from(vec!["8081".into()]).unwrap(),
        })
        .collect();
    let mut dynamic = DynamicPermissions::new(&client, Path::new("/usr/sbin/nft"), rules.clone());
    let sample = |expected: bool, ipv4: &str, ipv6: &str| {
        let output = client.command("/usr/bin/python3").unwrap().args(["-c", r#"
import socket, sys
expected = sys.argv[1] == 'true'
for af, address, kind in [(socket.AF_INET, sys.argv[2], socket.SOCK_STREAM), (socket.AF_INET6, sys.argv[3], socket.SOCK_DGRAM)]:
    with socket.socket(af, kind) as sock:
        sock.settimeout(.15)
        try:
            sock.connect((address, 8081))
            sock.sendall(b'dynamic')
            actual = sock.recv(64) == b'dynamic'
        except (TimeoutError, PermissionError): actual = False
        assert actual == expected, (address, actual, expected)
"#, if expected { "true" } else { "false" }, ipv4, ipv6]).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    sample(false, "198.18.0.3", "fd00:1::3");
    let grants = |duration| {
        let deadline = Instant::now() + duration;
        vec![
            ActiveGrant {
                rule: 0,
                address: "198.18.0.3".parse().unwrap(),
                deadline,
            },
            ActiveGrant {
                rule: 1,
                address: "fd00:1::3".parse().unwrap(),
                deadline,
            },
        ]
    };
    let prepared = dynamic.stage(&grants(Duration::from_secs(2))).unwrap();
    sample(false, "198.18.0.3", "fd00:1::3"); // An installed but unreferenced set permits nothing.
    prepared.activate().unwrap();
    sample(true, "198.18.0.3", "fd00:1::3");
    std::thread::sleep(Duration::from_secs(2));
    sample(false, "198.18.0.3", "fd00:1::3"); // No updater is running: kernel timeouts remove new permissions.
    let prepared = dynamic.stage(&grants(Duration::from_millis(500))).unwrap();
    std::thread::sleep(Duration::from_millis(600));
    prepared.activate().unwrap();
    sample(false, "198.18.0.3", "fd00:1::3"); // Referencing an expired set must never reinsert its elements.

    use kakoi_core::{network::NetworkLimits, policy::parse_policy};
    use kakoi_net::{dns::UpstreamResolver, scope::AddressContext};
    use std::net::UdpSocket;
    sample(false, "1.1.1.1", "2606:4700:4700::1111");
    let upstream = UdpSocket::bind("127.0.0.1:0").unwrap();
    upstream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let port = upstream.local_addr().unwrap().port();
    let dns = std::thread::spawn(move || {
        for _ in 0..2 {
            let mut bytes = [0; 512];
            let (size, peer) = upstream.recv_from(&mut bytes).unwrap();
            let kind = u16::from_be_bytes([bytes[size - 4], bytes[size - 3]]);
            let mut reply = bytes[..size].to_vec();
            reply[2] |= 0x80;
            reply[7] = 1;
            reply.extend([0xc0, 0x0c]);
            reply.extend(kind.to_be_bytes());
            reply.extend([0, 1, 0, 0, 0, 1]);
            let address = if kind == 1 {
                vec![1, 1, 1, 1]
            } else {
                "2606:4700:4700::1111"
                    .parse::<std::net::Ipv6Addr>()
                    .unwrap()
                    .octets()
                    .to_vec()
            };
            reply.extend((address.len() as u16).to_be_bytes());
            reply.extend(address);
            upstream.send_to(&reply, peer).unwrap();
        }
    });
    let config = parse_policy(
        &format!("[[network.dns-upstream]]\ntransport='plain'\nip='127.0.0.1'\nport={port}"),
        Path::new("dns.toml"),
    )
    .unwrap();
    let resolver = UpstreamResolver::new(
        rules,
        config.network.dns_upstream,
        NetworkLimits::default(),
        None,
    )
    .unwrap();
    for kind in [1u16, 28u16] {
        let mut question = vec![0x12, 0x34, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        question.extend(b"\x03api\x07example\x03com\0");
        question.extend(kind.to_be_bytes());
        question.extend([0, 1]);
        let expired = resolver
            .resolve_enforced_until(
                &question,
                Instant::now(),
                &AddressContext::default(),
                |_| None,
                &mut dynamic,
            )
            .unwrap();
        assert_eq!(expired[3] & 15, 2);
        let prepared = resolver
            .prepare_until(
                &question,
                Instant::now() + Duration::from_secs(1),
                None,
                &AddressContext::default(),
                |_| None,
            )
            .unwrap();
        if kind == 1 {
            sample(false, "1.1.1.1", "2606:4700:4700::1111");
        }
        let reply = prepared.adopt(&mut dynamic).unwrap();
        assert_eq!(reply[3] & 15, 0);
    }
    dns.join().unwrap();
    sample(true, "1.1.1.1", "2606:4700:4700::1111");
    std::thread::sleep(Duration::from_millis(1200));
    sample(false, "1.1.1.1", "2606:4700:4700::1111");
}

// @kotowari[REQ-014, REQ-389]
#[test]
fn a_late_staging_acknowledgement_never_activates_candidate_permissions() {
    use kakoi_core::network::{Allow, Destination};
    use kakoi_net::{dynamic::DynamicPermissions, leases::ActiveGrant};
    use std::fs;
    let namespace = NetworkNamespace::create().unwrap();
    nft::apply(
        &namespace,
        Path::new("/usr/sbin/nft"),
        &filter::compile_static(&[], 120).unwrap(),
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    let temp = crate::common::TempDir::new();
    let executable = temp.path().join("delayed-nft");
    let commits = temp.path().join("commits");
    // Every change commits, then its acknowledgement arrives after any deadline,
    // including the one for discarding the late staging.
    crate::common::write_executable(
        &executable,
        format!(
            "#!/bin/sh\n/usr/sbin/nft \"$@\" || exit $?\necho >> '{}'\nexec /bin/sleep 3\n",
            commits.display()
        ),
    );
    let rules = vec![Allow {
        destination: Destination::Dns("api.example.com".parse().unwrap()),
        protocol: Protocol::Tcp,
        ports: Ports::try_from(vec!["443".into()]).unwrap(),
    }];
    let mut owner = DynamicPermissions::new(&namespace, &executable, rules);
    let grant = ActiveGrant {
        rule: 0,
        address: "1.1.1.1".parse().unwrap(),
        deadline: Instant::now() + Duration::from_secs(3),
    };
    assert!(owner.stage(&[grant]).is_err());
    assert!(
        fs::read_to_string(&commits).is_ok_and(|log| !log.is_empty()),
        "fixture did not commit a candidate"
    );
    let output = namespace
        .command("/usr/sbin/nft")
        .unwrap()
        .args(["list", "chain", "inet", "kakoi_policy", "dns_permitted"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let chain = String::from_utf8(output.stdout).unwrap();
    assert!(
        !chain.contains("daddr"),
        "late candidate became reachable: {chain}"
    );
    assert!(
        owner.stage(&[grant]).is_err(),
        "uncertain owner was reused without reconciliation"
    );
}

// @kotowari[REQ-014, REQ-389]
#[test]
fn staging_rejects_elements_without_a_kernel_expiration() {
    use kakoi_core::network::{Allow, Destination};
    use kakoi_net::{dynamic::DynamicPermissions, leases::ActiveGrant};
    let namespace = NetworkNamespace::create().unwrap();
    nft::apply(
        &namespace,
        Path::new("/usr/sbin/nft"),
        &filter::compile_static(&[], 120).unwrap(),
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    let temp = crate::common::TempDir::new();
    let executable = temp.path().join("zero-timeout-nft");
    crate::common::write_executable(&executable, "#!/bin/sh\ncase \"$1\" in\n-f) /usr/bin/sed -E 's/timeout [0-9]+ms/timeout 0s/g' | /usr/sbin/nft \"$@\" ;;\n*) exec /usr/sbin/nft \"$@\" ;;\nesac\n");
    let rules = vec![Allow {
        destination: Destination::Dns("api.example.com".parse().unwrap()),
        protocol: Protocol::Tcp,
        ports: Ports::try_from(vec!["443".into()]).unwrap(),
    }];
    let mut owner = DynamicPermissions::new(&namespace, &executable, rules);
    let result = owner.stage(&[ActiveGrant {
        rule: 0,
        address: "1.1.1.1".parse().unwrap(),
        deadline: Instant::now() + Duration::from_secs(3),
    }]);
    assert!(
        result.is_err(),
        "an immortal element was accepted for activation"
    );
}

// @kotowari[REQ-133, REQ-014]
#[test]
fn an_old_grant_near_expiry_does_not_reduce_the_new_updates_staging_budget() {
    use kakoi_core::network::{Allow, Destination};
    use kakoi_net::{dynamic::DynamicPermissions, leases::ActiveGrant};
    let namespace = NetworkNamespace::create().unwrap();
    nft::apply(
        &namespace,
        Path::new("/usr/sbin/nft"),
        &filter::compile_static(&[], 120).unwrap(),
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    let rules = vec![Allow {
        destination: Destination::Dns("api.example.com".parse().unwrap()),
        protocol: Protocol::Tcp,
        ports: Ports::try_from(vec!["443".into()]).unwrap(),
    }];
    let mut owner = DynamicPermissions::new(&namespace, Path::new("/usr/sbin/nft"), rules);
    let expiry = Instant::now() + Duration::from_millis(300);
    owner
        .install(&[ActiveGrant {
            rule: 0,
            address: "1.1.1.1".parse().unwrap(),
            deadline: expiry,
        }])
        .unwrap();
    std::thread::sleep(
        (expiry - Duration::from_millis(4)).saturating_duration_since(Instant::now()),
    );
    owner
        .install(&[ActiveGrant {
            rule: 0,
            address: "8.8.8.8".parse().unwrap(),
            deadline: Instant::now() + Duration::from_secs(3),
        }])
        .unwrap();
    let data = nft::inspect(
        &namespace,
        Path::new("/usr/sbin/nft"),
        "kakoi_policy",
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    let data = String::from_utf8(data).unwrap();
    assert!(data.contains("8.8.8.8"));
    assert!(!data.contains("1.1.1.1"));
}

// @kotowari[REQ-014]
#[test]
fn enforced_dns_reports_an_uncertain_kernel_owner_before_contacting_upstream() {
    use kakoi_core::{
        network::{Allow, Destination, NetworkLimits},
        policy::parse_policy,
    };
    use kakoi_net::{
        dns::{EnforcedDnsError, UpstreamResolver},
        dynamic::DynamicPermissions,
        leases::ActiveGrant,
        scope::AddressContext,
    };
    let namespace = NetworkNamespace::create().unwrap();
    let rules = vec![Allow {
        destination: Destination::Dns("api.example.com".parse().unwrap()),
        protocol: Protocol::Tcp,
        ports: Ports::try_from(vec!["443".into()]).unwrap(),
    }];
    let mut permissions =
        DynamicPermissions::new(&namespace, Path::new("/bin/false"), rules.clone());
    assert!(permissions
        .install(&[ActiveGrant {
            rule: 0,
            address: "1.1.1.1".parse().unwrap(),
            deadline: Instant::now() + Duration::from_secs(3)
        }])
        .is_err());
    assert!(permissions.requires_reconciliation());
    let upstream = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let config = parse_policy(
        &format!(
            "[[network.dns-upstream]]\ntransport='plain'\nip='127.0.0.1'\nport={}",
            upstream.local_addr().unwrap().port()
        ),
        Path::new("dns.toml"),
    )
    .unwrap();
    let limits = NetworkLimits {
        dns_resolution_timeout_seconds: 1,
        dns_server_timeout_seconds: 1,
        ..NetworkLimits::default()
    };
    let resolver = UpstreamResolver::new(rules, config.network.dns_upstream, limits, None).unwrap();
    let mut question = vec![0x12, 0x34, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    question.extend(b"\x03api\x07example\x03com\0\0\x01\0\x01");
    assert!(matches!(
        resolver.resolve_enforced(
            &question,
            &AddressContext::default(),
            |_| None,
            &mut permissions
        ),
        Err(EnforcedDnsError::Enforcement(_))
    ));
    upstream.set_nonblocking(true).unwrap();
    assert_eq!(
        upstream.recv(&mut [0; 512]).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

// @kotowari[REQ-014, REQ-116]
#[test]
fn prepared_answers_reject_stopped_expired_or_mismatched_adoption_and_surface_kernel_faults() {
    use kakoi_core::{
        network::{Allow, Destination, NetworkLimits},
        policy::parse_policy,
    };
    use kakoi_net::{
        dns::{AcceptedRequest, DnsRequests, EnforcedDnsError, UpstreamResolver},
        dns_workers::{DnsWorkers, WorkResult},
        dynamic::DynamicPermissions,
        scope::AddressContext,
    };
    use std::net::UdpSocket;
    for mode in ["cancel", "expiry", "mismatch", "kernel"] {
        let namespace = NetworkNamespace::create().unwrap();
        nft::apply(
            &namespace,
            Path::new("/usr/sbin/nft"),
            &filter::compile_static(&[], 120).unwrap(),
            Instant::now() + Duration::from_secs(2),
        )
        .unwrap();
        let inspect = || {
            nft::inspect(
                &namespace,
                Path::new("/usr/sbin/nft"),
                "kakoi_policy",
                Instant::now() + Duration::from_secs(2),
            )
            .unwrap()
        };
        let before = inspect();
        let rules = vec![Allow {
            destination: Destination::Dns("api.example.com".parse().unwrap()),
            protocol: Protocol::Tcp,
            ports: vec!["443".into()].try_into().unwrap(),
        }];
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let config = parse_policy(
            &format!(
                "[[network.dns-upstream]]\ntransport='plain'\nip='127.0.0.1'\nport={}",
                socket.local_addr().unwrap().port()
            ),
            Path::new("dns.toml"),
        )
        .unwrap();
        let upstream = std::thread::spawn(move || {
            let mut bytes = [0; 512];
            let (size, peer) = socket.recv_from(&mut bytes).unwrap();
            let mut answer = bytes[..size].to_vec();
            answer[2] |= 0x80;
            answer[7] = 1;
            answer.extend([0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 30, 0, 4, 1, 1, 1, 1]);
            socket.send_to(&answer, peer).unwrap();
        });
        let resolver = UpstreamResolver::new(
            rules.clone(),
            config.network.dns_upstream,
            NetworkLimits::default(),
            None,
        )
        .unwrap();
        let mut requests =
            DnsRequests::new(rules.clone(), 0, 1, 1, Duration::from_millis(500)).unwrap();
        let wire = b"\x12\x34\x01\0\0\x01\0\0\0\0\0\0\x03api\x07example\x03com\0\0\x01\0\x01";
        let AcceptedRequest::Start(task) = requests.accept(wire, (), Instant::now()).unwrap()
        else {
            panic!()
        };
        let expires = task.deadline;
        let mut workers = DnsWorkers::new(1).unwrap();
        workers
            .start(task, move |task, cancel| {
                resolver.prepare_until(
                    &task.wire,
                    task.deadline,
                    Some(&cancel),
                    &AddressContext::default(),
                    |_| None,
                )
            })
            .unwrap();
        let wait_until = Instant::now() + Duration::from_secs(1);
        let answer = loop {
            if let Some(result) = workers.collect().pop() {
                let WorkResult::Finished(answer) = result.result else {
                    panic!()
                };
                break answer.unwrap();
            }
            assert!(Instant::now() < wait_until);
            std::thread::sleep(Duration::from_millis(1));
        };
        upstream.join().unwrap();
        assert_eq!(before, inspect(), "worker changed kernel permissions");
        if mode == "cancel" {
            workers.stop();
        }
        if mode == "expiry" {
            std::thread::sleep(
                expires.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
            );
        }
        let policy = if mode == "mismatch" {
            Vec::new()
        } else {
            rules
        };
        let path = if mode == "kernel" {
            "/bin/false"
        } else {
            "/usr/sbin/nft"
        };
        let mut permissions = DynamicPermissions::new(&namespace, Path::new(path), policy);
        let result = answer.adopt(&mut permissions);
        if matches!(mode, "kernel" | "mismatch") {
            assert!(matches!(result, Err(EnforcedDnsError::Enforcement(_))));
        } else {
            assert!(matches!(result, Err(EnforcedDnsError::Query(_))));
        }
        assert_eq!(permissions.requires_reconciliation(), mode == "kernel");
        assert_eq!(
            before,
            inspect(),
            "invalid candidate changed kernel permissions"
        );
    }
}

/// An nft that takes `delay` before every change while `slow` exists, like one
/// scheduled late on a loaded host. It is killed during the delay on a deadline,
/// before the real nft could commit anything.
fn slow_nft(temp: &crate::common::TempDir) -> (std::path::PathBuf, std::path::PathBuf) {
    use std::fs;
    let slow = temp.path().join("slow");
    let wrapper = temp.path().join("nft");
    crate::common::write_executable(
        &wrapper,
        format!(
            "#!/bin/sh\nif [ -e '{}' ]; then /bin/sleep 0.12; fi\nexec /usr/sbin/nft \"$@\"\n",
            slow.display()
        ),
    );
    fs::write(&slow, "").unwrap();
    (wrapper, slow)
}

fn dns_sets(namespace: &NetworkNamespace) -> Vec<String> {
    let json = nft::inspect(
        namespace,
        Path::new("/usr/sbin/nft"),
        "kakoi_policy",
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
    value["nftables"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| entry["set"]["name"].as_str())
        .filter(|name| name.starts_with("dns_"))
        .map(str::to_owned)
        .collect()
}

fn dns_owner_namespace() -> NetworkNamespace {
    let namespace = NetworkNamespace::create().unwrap();
    nft::apply(
        &namespace,
        Path::new("/usr/sbin/nft"),
        &filter::compile_static(&[], 120).unwrap(),
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    namespace
}

fn api_rule() -> Vec<kakoi_core::network::Allow> {
    use kakoi_core::network::{Allow, Destination};
    vec![Allow {
        destination: Destination::Dns("api.example.com".parse().unwrap()),
        protocol: Protocol::Tcp,
        ports: Ports::try_from(vec!["443".into()]).unwrap(),
    }]
}

/// One staging of DNS permissions that ran out its reserve: the time left to
/// the permission's expiry when it began, and how long it was given before
/// it was discarded.
struct LateStaging {
    left: Duration,
    reserve: Duration,
}

/// Installs one permission expiring `left` from now through an nft that never
/// finishes a staging, so that every staging runs out its reserve and is
/// discarded. Returns each staging, in order, and how the install ended.
fn late_stagings(left: Duration) -> (Vec<LateStaging>, std::io::ErrorKind) {
    use kakoi_net::{dynamic::DynamicPermissions, leases::ActiveGrant};
    use std::time::SystemTime;
    let namespace = dns_owner_namespace();
    let temp = crate::common::TempDir::new();
    let log = temp.path().join("log");
    let wrapper = temp.path().join("nft");
    crate::common::write_executable(
        &wrapper,
        format!(
            r#"#!/bin/sh
if [ "$1" = "-f" ]; then
    input='{directory}/input.'$$
    /bin/cat > "$input"
    if /bin/grep -q '^add element' "$input"; then
        echo "stage $(/bin/date +%s%N)" >> '{log}'
        exec /bin/sleep 60
    fi
    if /bin/grep -q '^delete set' "$input"; then
        echo "discard $(/bin/date +%s%N)" >> '{log}'
    fi
    exec /usr/sbin/nft "$@" < "$input"
fi
exec /usr/sbin/nft "$@"
"#,
            directory = temp.path().display(),
            log = log.display(),
        ),
    );
    let mut owner = DynamicPermissions::new(&namespace, &wrapper, api_rule());
    let expiry = SystemTime::now() + left;
    let error = owner
        .install(&[ActiveGrant {
            rule: 0,
            address: "1.1.1.1".parse().unwrap(),
            deadline: Instant::now() + left,
        }])
        .unwrap_err();
    assert!(!owner.requires_reconciliation());
    let events: Vec<(String, SystemTime)> = std::fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(|line| {
            let (kind, nanos) = line.split_once(' ').unwrap();
            (
                kind.to_owned(),
                SystemTime::UNIX_EPOCH + Duration::from_nanos(nanos.parse().unwrap()),
            )
        })
        .collect();
    let stagings = events
        .chunks(2)
        .map(|pair| {
            assert_eq!(
                [pair[0].0.as_str(), pair[1].0.as_str()],
                ["stage", "discard"]
            );
            LateStaging {
                left: expiry.duration_since(pair[0].1).unwrap(),
                reserve: pair[1].1.duration_since(pair[0].1).unwrap(),
            }
        })
        .collect();
    (stagings, error.kind())
}

// While half the time left allows, each late staging is tried again with
// twice the reserve. Reserves too short for the doubling to show above the
// cost of starting nft are not compared.
// @kotowari[REQ-420, EX-804]
#[test]
fn a_late_staging_is_tried_again_with_twice_the_reserve() {
    let (stagings, error) = late_stagings(Duration::from_secs(10));
    assert_eq!(error, std::io::ErrorKind::TimedOut);
    let compared: Vec<f64> = stagings
        .windows(2)
        .filter(|pair| {
            pair[0].reserve >= Duration::from_millis(90)
                && pair[0].reserve * 2 + Duration::from_millis(100) < pair[1].left / 2
        })
        .map(|pair| pair[1].reserve.as_secs_f64() / pair[0].reserve.as_secs_f64())
        .collect();
    assert!(compared.len() >= 3, "{compared:?}");
    assert!(
        compared.iter().all(|ratio| (1.6..=2.4).contains(ratio)),
        "{compared:?}"
    );
}

// With 1 second left, a doubled reserve past 0.5 seconds is cut to half the
// time left; once no longer reserve fits, the install gives up.
// @kotowari[REQ-420, EX-805]
#[test]
fn a_staging_reserve_never_exceeds_half_the_time_left() {
    let (stagings, error) = late_stagings(Duration::from_millis(1750));
    assert_eq!(error, std::io::ErrorKind::TimedOut);
    let slack = Duration::from_millis(60);
    for staging in &stagings {
        assert!(
            staging.reserve <= staging.left / 2 + slack,
            "reserve {:?} with {:?} left",
            staging.reserve,
            staging.left
        );
    }
    assert!(
        stagings
            .windows(2)
            .any(|pair| pair[0].reserve * 2 > pair[1].left / 2 + Duration::from_millis(100)),
        "the doubling never reached half the time left"
    );
}

// @kotowari[REQ-014, REQ-420]
#[test]
fn a_late_nft_widens_the_staging_reserve_instead_of_faulting_the_owner() {
    use kakoi_net::{dynamic::DynamicPermissions, leases::ActiveGrant};
    let namespace = dns_owner_namespace();
    let temp = crate::common::TempDir::new();
    let (wrapper, _slow) = slow_nft(&temp);
    let mut owner = DynamicPermissions::new(&namespace, &wrapper, api_rule());
    owner
        .install(&[ActiveGrant {
            rule: 0,
            address: "1.1.1.1".parse().unwrap(),
            deadline: Instant::now() + Duration::from_secs(3),
        }])
        .unwrap();
    assert!(!owner.requires_reconciliation());
    // Only the activated set remains; abandoned staging attempts were removed.
    assert_eq!(dns_sets(&namespace).len(), 1);
}

// @kotowari[REQ-014]
#[test]
fn staging_that_cannot_finish_before_the_answer_expires_fails_only_that_answer() {
    use kakoi_net::{dynamic::DynamicPermissions, leases::ActiveGrant};
    let namespace = dns_owner_namespace();
    let temp = crate::common::TempDir::new();
    let (wrapper, slow) = slow_nft(&temp);
    let mut owner = DynamicPermissions::new(&namespace, &wrapper, api_rule());
    let error = owner
        .install(&[ActiveGrant {
            rule: 0,
            address: "1.1.1.1".parse().unwrap(),
            deadline: Instant::now() + Duration::from_millis(150),
        }])
        .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(!owner.requires_reconciliation());
    assert!(dns_sets(&namespace).is_empty());
    std::fs::remove_file(slow).unwrap();
    owner
        .install(&[ActiveGrant {
            rule: 0,
            address: "8.8.8.8".parse().unwrap(),
            deadline: Instant::now() + Duration::from_secs(3),
        }])
        .unwrap();
    assert_eq!(dns_sets(&namespace).len(), 1);
}

/// The time a DNS permission has left in the kernel, read from `nft`'s text
/// listing, which unlike its JSON keeps the milliseconds.
fn dns_permission_expires(namespace: &NetworkNamespace) -> Duration {
    let output = namespace
        .command("/usr/sbin/nft")
        .unwrap()
        .args(["list", "table", "inet", "kakoi_policy"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    let expires = text
        .split(" expires ")
        .nth(1)
        .and_then(|rest| rest.split([' ', ',', '\n']).next())
        .unwrap_or_else(|| panic!("no expiring element in {text}"));
    let mut total = Duration::ZERO;
    let mut digits = String::new();
    let mut unit = String::new();
    for character in expires.chars().chain([' ']) {
        if character.is_ascii_digit() || character == ' ' {
            if !unit.is_empty() {
                let value: u64 = digits.parse().unwrap();
                total += match unit.as_str() {
                    "d" => Duration::from_secs(value * 86400),
                    "h" => Duration::from_secs(value * 3600),
                    "m" => Duration::from_secs(value * 60),
                    "s" => Duration::from_secs(value),
                    "ms" => Duration::from_millis(value),
                    other => panic!("unknown unit {other} in {expires}"),
                };
                digits.clear();
                unit.clear();
            }
            digits.push(character);
        } else {
            unit.push(character);
        }
    }
    total
}

// A staging retried with a longer reserve still counts the permission's
// kernel lifetime from the end of that reserve, never from its start: the
// kernel removes the permission no later than the answer's expiry.
// @kotowari[REQ-397, EX-733]
#[test]
fn a_late_staging_never_leaves_the_permission_past_the_answer_expiry() {
    use kakoi_net::{dynamic::DynamicPermissions, leases::ActiveGrant};
    let namespace = dns_owner_namespace();
    let temp = crate::common::TempDir::new();
    let (wrapper, _slow) = slow_nft(&temp);
    let mut owner = DynamicPermissions::new(&namespace, &wrapper, api_rule());
    let deadline = Instant::now() + Duration::from_secs(3);
    owner
        .install(&[ActiveGrant {
            rule: 0,
            address: "1.1.1.1".parse().unwrap(),
            deadline,
        }])
        .unwrap();
    // Taken before the listing, so the sum never exceeds the real expiry and
    // load cannot make this fail. Counting from the start of the reserve would
    // exceed the deadline by the wrapper's 120 ms; 20 ms covers kernel rounding.
    let listed = Instant::now();
    let expires = dns_permission_expires(&namespace);
    assert!(
        listed + expires <= deadline + Duration::from_millis(20),
        "the permission outlives the answer by {:?}",
        (listed + expires).saturating_duration_since(deadline)
    );
}

// A rule for all of IPv6 keeps its port, and does not reach a link-local
// neighbour: only a rule naming the interface could, and the initial release
// has none.
// @kotowari[REQ-141, EX-314, EX-085]
#[test]
fn a_rule_for_all_of_ipv6_does_not_reach_link_local() {
    let client = NetworkNamespace::create().unwrap();
    let server = NetworkNamespace::create_within(&client).unwrap();
    ip(
        &client,
        &[
            "link",
            "add",
            "app0",
            "type",
            "veth",
            "peer",
            "name",
            "srv0",
            "netns",
            &server.keeper_pid().to_string(),
        ],
    );
    for (namespace, interface, suffix) in [(&client, "app0", "1"), (&server, "srv0", "2")] {
        ip(namespace, &["link", "set", "lo", "up"]);
        ip(namespace, &["link", "set", interface, "up"]);
        for address in [format!("fd00:1::{suffix}/64"), format!("fe80::{suffix}/64")] {
            ip(
                namespace,
                &["-6", "addr", "add", &address, "dev", interface, "nodad"],
            );
        }
    }
    let script = filter::compile_static(
        &[FilterRule {
            network: "::/0".parse().unwrap(),
            protocol: Protocol::Tcp,
            ports: Ports::try_from(vec!["8080".into()]).unwrap(),
        }],
        1,
    )
    .unwrap();
    nft::apply(
        &client,
        Path::new("/usr/sbin/nft"),
        &script,
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    let mut server = Server(
        server
            .command("/usr/bin/python3")
            .unwrap()
            .args([
                "-c",
                r#"
import socket, sys, threading
def serve(listener):
    while True:
        connection, _ = listener.accept()
        with connection:
            connection.sendall(b'served')
for port in (8080, 8081):
    listener = socket.socket(socket.AF_INET6)
    listener.bind(('::', port))
    listener.listen()
    threading.Thread(target=serve, args=(listener,), daemon=True).start()
print('ready', flush=True)
sys.stdin.read()
"#,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut ready = String::new();
    BufReader::new(server.0.stdout.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    assert_eq!(ready, "ready\n");
    let output = client
        .command("/usr/bin/python3")
        .unwrap()
        .args([
            "-c",
            r#"
import socket
for address in [('fd00:1::2', 8080), ('fd00:1::2', 8081), ('fe80::2%app0', 8080)]:
    with socket.socket(socket.AF_INET6) as sock:
        sock.settimeout(1)
        try:
            sock.connect(socket.getaddrinfo(*address, socket.AF_INET6, socket.SOCK_STREAM)[0][4])
            print(*address, sock.recv(16).decode())
        except OSError as error:
            print(*address, 'failed', type(error).__name__)
"#,
        ])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "fd00:1::2 8080 served\nfd00:1::2 8081 failed TimeoutError\nfe80::2%app0 8080 failed TimeoutError\n",
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A client namespace joined to a server namespace by a veth pair, with
/// fd00:1::1 and fd00:1::2, and a UDP echo on the server's port 8080.
fn udp_echo_pair() -> (NetworkNamespace, NetworkNamespace, Server) {
    let client = NetworkNamespace::create().unwrap();
    let server = NetworkNamespace::create_within(&client).unwrap();
    ip(
        &client,
        &[
            "link",
            "add",
            "app0",
            "type",
            "veth",
            "peer",
            "name",
            "srv0",
            "netns",
            &server.keeper_pid().to_string(),
        ],
    );
    for (namespace, interface, suffix) in [(&client, "app0", "1"), (&server, "srv0", "2")] {
        ip(namespace, &["link", "set", "lo", "up"]);
        ip(namespace, &["link", "set", interface, "up"]);
        ip(
            namespace,
            &[
                "-6",
                "addr",
                "add",
                &format!("fd00:1::{suffix}/64"),
                "dev",
                interface,
                "nodad",
            ],
        );
    }
    let mut echo = Server(
        server
            .command("/usr/bin/python3")
            .unwrap()
            .args([
                "-c",
                "import socket, sys\ns = socket.socket(socket.AF_INET6, socket.SOCK_DGRAM)\ns.bind(('fd00:1::2', 8080))\nprint('ready', flush=True)\nwhile True:\n    data, peer = s.recvfrom(64)\n    s.sendto(data, peer)\n",
            ])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut ready = String::new();
    BufReader::new(echo.0.stdout.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    assert_eq!(ready, "ready\n");
    (client, server, echo)
}

// With a 3 second idle limit and the permission withdrawn, an exchange that
// keeps going outlives the limit many times over, and another process taking
// up the same pair of addresses and ports within the limit continues it.
// @kotowari[REQ-018, EX-029, EX-200]
#[test]
fn a_udp_flow_lives_while_it_is_used_whoever_uses_it() {
    let (client, _server, _echo) = udp_echo_pair();
    let script = filter::compile_static(
        &[FilterRule {
            network: "fd00:1::2/128".parse().unwrap(),
            protocol: Protocol::Udp,
            ports: Ports::try_from(vec!["8080".into()]).unwrap(),
        }],
        3,
    )
    .unwrap();
    nft::apply(
        &client,
        Path::new("/usr/sbin/nft"),
        &script,
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    let exchange = r#"
import socket, sys, time
s = socket.socket(socket.AF_INET6, socket.SOCK_DGRAM)
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(('fd00:1::1', 20001))
s.connect(('fd00:1::2', 8080))
s.settimeout(.5)
for _ in range(int(sys.argv[1])):
    try:
        s.send(b'x')
        s.recv(64)
    except OSError as error:
        print('failed', type(error).__name__)
        sys.exit()
    time.sleep(.3)
print('ok')
"#;
    let run = |count: &str| {
        let output = client
            .command("/usr/bin/python3")
            .unwrap()
            .args(["-c", exchange, count])
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    assert_eq!(run("1"), "ok\n");
    nft::apply(
        &client,
        Path::new("/usr/sbin/nft"),
        "flush chain inet kakoi_policy permitted\n",
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    // Another process, the same pair, within the idle limit: 20 exchanges
    // 0.3 seconds apart span 6 seconds.
    assert_eq!(run("20"), "ok\n");
    // Idle past the limit: a new flow, which nothing permits.
    std::thread::sleep(Duration::from_secs(4));
    assert_eq!(run("1"), "failed PermissionError\n");
}
