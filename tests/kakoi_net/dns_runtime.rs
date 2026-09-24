use kakoi_core::{
    network::{Allow, Destination, NetworkLimits, Protocol},
    policy::parse_policy,
};
use kakoi_net::{
    dns_runtime::{DnsRuntime, DnsRuntimeConfig},
    filter,
    namespace::NetworkNamespace,
    nft,
    scope::AddressContext,
};
use std::{
    net::UdpSocket,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

// @kotowari[REQ-116, REQ-131, REQ-058]
#[test]
fn runtime_stop_interrupts_pending_dns_and_kernel_fault_stops_further_work() {
    for mode in ["stop", "kernel"] {
        let ns = Arc::new(NetworkNamespace::create().unwrap());
        assert!(ns
            .command("/usr/sbin/ip")
            .unwrap()
            .args(["link", "set", "lo", "up"])
            .status()
            .unwrap()
            .success());
        nft::apply(
            &ns,
            Path::new("/usr/sbin/nft"),
            &filter::compile_static(&[], 120).unwrap(),
            Instant::now() + Duration::from_secs(2),
        )
        .unwrap();
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        let config = parse_policy(
            &format!(
                "[[network.dns-upstream]]\ntransport='plain'\nip='127.0.0.1'\nport={}",
                socket.local_addr().unwrap().port()
            ),
            Path::new("dns.toml"),
        )
        .unwrap();
        let mut runtime = DnsRuntime::new(
            Arc::clone(&ns),
            DnsRuntimeConfig {
                policy: vec![Allow {
                    destination: Destination::Dns("api.example.com".parse().unwrap()),
                    protocol: Protocol::Tcp,
                    ports: vec!["443".into()].try_into().unwrap(),
                }],
                upstreams: config.network.dns_upstream,
                limits: NetworkLimits {
                    dns_resolution_timeout_seconds: 30,
                    dns_server_timeout_seconds: 30,
                    dns_max_concurrent_resolutions: 1,
                    ..NetworkLimits::default()
                },
                nft: if mode == "kernel" {
                    "/bin/false".into()
                } else {
                    "/usr/sbin/nft".into()
                },
                trust: None,
                scope: AddressContext::default(),
                generation: 0,
                host_dns: None,
            },
            |_| None,
        )
        .unwrap();
        assert!(ns.command("/usr/bin/python3").unwrap().args(["-c",r#"
import socket
s=socket.socket(socket.AF_INET,socket.SOCK_DGRAM)
s.sendto(b'\x12\x34\x01\0\0\x01\0\0\0\0\0\0\x03api\x07example\x03com\0\0\x01\0\x01',('127.0.0.53',53))
"#]).status().unwrap().success());
        let deadline = Instant::now() + Duration::from_secs(2);
        let (mut answer, peer) = loop {
            runtime.poll(Instant::now()).unwrap();
            let mut bytes = [0; 512];
            match socket.recv_from(&mut bytes) {
                Ok((size, peer)) => break (bytes[..size].to_vec(), peer),
                Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock),
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        };
        if mode == "stop" {
            runtime.stop();
        } else {
            answer[2] |= 0x80;
            answer[7] = 1;
            answer.extend([0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 30, 0, 4, 1, 1, 1, 1]);
            socket.send_to(&answer, peer).unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut fault = false;
        while !runtime.is_finished() {
            if runtime.poll(Instant::now()).is_err() {
                assert_eq!(mode, "kernel");
                fault = true;
            }
            assert!(Instant::now() < deadline, "runtime did not stop promptly");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(fault, mode == "kernel");
        assert_eq!(runtime.is_faulted(), fault);
        let json = nft::inspect(
            &ns,
            Path::new("/usr/sbin/nft"),
            "kakoi_policy",
            Instant::now() + Duration::from_secs(2),
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert!(!value["nftables"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["rule"]["chain"] == "dns_permitted"));
    }
}

// The runtime holds a failure for the configured time: asked again at once,
// the same question gets SERVFAIL without reaching the upstream.
// @kotowari[REQ-134, EX-301]
#[test]
fn the_runtime_holds_a_failed_resolution_without_asking_again() {
    let ns = Arc::new(NetworkNamespace::create().unwrap());
    assert!(ns
        .command("/usr/sbin/ip")
        .unwrap()
        .args(["link", "set", "lo", "up"])
        .status()
        .unwrap()
        .success());
    nft::apply(
        &ns,
        Path::new("/usr/sbin/nft"),
        &filter::compile_static(&[], 120).unwrap(),
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.set_nonblocking(true).unwrap();
    let config = parse_policy(
        &format!(
            "[[network.dns-upstream]]\ntransport='plain'\nip='127.0.0.1'\nport={}",
            socket.local_addr().unwrap().port()
        ),
        Path::new("dns.toml"),
    )
    .unwrap();
    let mut runtime = DnsRuntime::new(
        Arc::clone(&ns),
        DnsRuntimeConfig {
            policy: vec![Allow {
                destination: Destination::Dns("api.example.com".parse().unwrap()),
                protocol: Protocol::Tcp,
                ports: vec!["443".into()].try_into().unwrap(),
            }],
            upstreams: config.network.dns_upstream,
            limits: NetworkLimits {
                dns_failure_cache_seconds: 60,
                ..NetworkLimits::default()
            },
            nft: "/usr/sbin/nft".into(),
            trust: None,
            scope: AddressContext::default(),
            generation: 0,
            host_dns: None,
        },
        |_| None,
    )
    .unwrap();
    let mut upstream_queries = 0;
    for _ in 0..2 {
        let mut client = ns
            .command("/usr/bin/python3")
            .unwrap()
            .args([
                "-c",
                r#"
import socket, sys
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
s.settimeout(20)
s.sendto(b'\x12\x34\x01\0\0\x01\0\0\0\0\0\0\x03api\x07example\x03com\0\0\x01\0\x01', ('127.0.0.53', 53))
sys.exit(s.recv(512)[3] & 15)
"#,
            ])
            .spawn()
            .unwrap();
        // Only guards against a hang.
        let deadline = Instant::now() + Duration::from_secs(30);
        let status = loop {
            runtime.poll(Instant::now()).unwrap();
            let mut bytes = [0; 512];
            if let Ok((size, peer)) = socket.recv_from(&mut bytes) {
                upstream_queries += 1;
                let mut answer = bytes[..size].to_vec();
                answer[2] |= 0x80;
                answer[3] = (answer[3] & 0xf0) | 2;
                socket.send_to(&answer, peer).unwrap();
            }
            if let Some(status) = client.try_wait().unwrap() {
                break status;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(status.code(), Some(2));
    }
    assert_eq!(upstream_queries, 1);
}

/// What a fake upstream does with each query.
#[derive(Clone, Copy)]
enum Upstream {
    /// One A record, 1.1.1.1, for 30 seconds.
    Answers,
    /// No records, NOERROR.
    NoData,
    /// An empty answer with this RCODE.
    Code(u8),
    /// Nothing.
    Silent,
}

/// Starts a runtime whose explicit upstreams are `upstreams`, in order, and
/// asks for each of `names` in turn, as the application would. Returns each
/// answer's RCODE and record count, and the names each upstream was asked.
fn resolve(
    upstreams: &[Upstream],
    limits: NetworkLimits,
    names: &[&str],
) -> (Vec<(u8, u16)>, Vec<Vec<String>>) {
    let ns = Arc::new(NetworkNamespace::create().unwrap());
    assert!(ns
        .command("/usr/sbin/ip")
        .unwrap()
        .args(["link", "set", "lo", "up"])
        .status()
        .unwrap()
        .success());
    nft::apply(
        &ns,
        Path::new("/usr/sbin/nft"),
        &filter::compile_static(&[], 120).unwrap(),
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    let sockets: Vec<_> = upstreams
        .iter()
        .map(|_| {
            let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
            socket.set_nonblocking(true).unwrap();
            socket
        })
        .collect();
    let text: String = sockets
        .iter()
        .map(|socket| {
            format!(
                "[[network.dns-upstream]]\ntransport='plain'\nip='127.0.0.1'\nport={}\n",
                socket.local_addr().unwrap().port()
            )
        })
        .collect();
    let config = parse_policy(&text, Path::new("dns.toml")).unwrap();
    let mut runtime = DnsRuntime::new(
        Arc::clone(&ns),
        DnsRuntimeConfig {
            policy: vec![Allow {
                destination: Destination::Dns("*.example.com".parse().unwrap()),
                protocol: Protocol::Tcp,
                ports: vec!["443".into()].try_into().unwrap(),
            }],
            upstreams: config.network.dns_upstream,
            limits,
            nft: "/usr/sbin/nft".into(),
            trust: None,
            scope: AddressContext::default(),
            generation: 0,
            host_dns: None,
        },
        |_| None,
    )
    .unwrap();
    let mut client = ns
        .command("/usr/bin/python3")
        .unwrap()
        .args([
            "-c",
            r#"
import socket, sys
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
s.settimeout(60)
for index, name in enumerate(sys.argv[1:]):
    wire = bytes([0x12, index, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0])
    for label in name.split('.'):
        wire += bytes([len(label)]) + label.encode()
    wire += b'\0\0\x01\0\x01'
    s.sendto(wire, ('127.0.0.53', 53))
    data = s.recv(512)
    print(data[3] & 15, int.from_bytes(data[6:8], 'big'), flush=True)
"#,
        ])
        .args(names)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut asked = vec![Vec::new(); upstreams.len()];
    // Only guards against a hang.
    let deadline = Instant::now() + Duration::from_secs(60);
    while client.try_wait().unwrap().is_none() {
        runtime.poll(Instant::now()).unwrap();
        for (index, (socket, behaviour)) in sockets.iter().zip(upstreams).enumerate() {
            let mut bytes = [0; 512];
            let Ok((size, peer)) = socket.recv_from(&mut bytes) else {
                continue;
            };
            let query = &bytes[..size];
            let mut at = 12;
            let mut labels = Vec::new();
            while query[at] != 0 {
                let length = query[at] as usize;
                labels.push(String::from_utf8_lossy(&query[at + 1..at + 1 + length]).into_owned());
                at += 1 + length;
            }
            asked[index].push(labels.join("."));
            let mut answer = query.to_vec();
            answer[2] |= 0x80;
            match behaviour {
                Upstream::Answers => {
                    answer[7] = 1;
                    answer.extend([0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 30, 0, 4, 1, 1, 1, 1]);
                }
                Upstream::NoData => {}
                Upstream::Code(code) => answer[3] = (answer[3] & 0xf0) | code,
                Upstream::Silent => continue,
            }
            socket.send_to(&answer, peer).unwrap();
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    let mut output = String::new();
    std::io::Read::read_to_string(&mut client.stdout.take().unwrap(), &mut output).unwrap();
    let answers = output
        .lines()
        .map(|line| {
            let (code, count) = line.split_once(' ').unwrap();
            (code.parse().unwrap(), count.parse().unwrap())
        })
        .collect();
    (answers, asked)
}

fn short_waits() -> NetworkLimits {
    NetworkLimits {
        dns_server_timeout_seconds: 1,
        dns_resolution_timeout_seconds: 10,
        ..NetworkLimits::default()
    }
}

// Two names, one explicit upstream for both.
// @kotowari[REQ-111, EX-242]
#[test]
fn every_name_uses_the_explicit_upstreams() {
    let (answers, asked) = resolve(
        &[Upstream::Answers],
        NetworkLimits::default(),
        &["a.example.com", "b.example.com"],
    );
    assert_eq!(answers, [(0, 1), (0, 1)]);
    assert_eq!(asked, [["a.example.com", "b.example.com"]]);
}

// A silent first candidate is given up after its wait, once, and the next
// answers; the next resolution starts again from the first.
// @kotowari[REQ-112, EX-244, EX-247, EX-303]
#[test]
fn a_silent_candidate_is_passed_over_and_each_resolution_starts_from_the_first() {
    let (answers, asked) = resolve(
        &[Upstream::Silent, Upstream::Answers],
        short_waits(),
        &["a.example.com", "b.example.com"],
    );
    assert_eq!(answers, [(0, 1), (0, 1)]);
    assert_eq!(
        asked,
        [
            ["a.example.com", "b.example.com"],
            ["a.example.com", "b.example.com"]
        ]
    );
}

// Failure, refusal, and silence from every candidate end in SERVFAIL, and
// nothing unlisted is asked.
// @kotowari[EX-245, EX-276]
#[test]
fn every_candidate_failing_ends_in_servfail() {
    let (answers, asked) = resolve(
        &[Upstream::Code(2), Upstream::Code(5), Upstream::Silent],
        short_waits(),
        &["a.example.com"],
    );
    assert_eq!(answers, [(2, 0)]);
    assert_eq!(
        asked,
        [["a.example.com"], ["a.example.com"], ["a.example.com"]]
    );
}

// NXDOMAIN and NODATA are answers: returned as they are, and no later
// candidate is asked.
// @kotowari[EX-249, EX-277]
#[test]
fn a_negative_answer_is_the_result() {
    for (first, expected) in [(Upstream::Code(3), (3, 0)), (Upstream::NoData, (0, 0))] {
        let (answers, asked) = resolve(
            &[first, Upstream::Answers],
            short_waits(),
            &["a.example.com"],
        );
        assert_eq!(answers, [expected]);
        assert_eq!(asked[1], Vec::<String>::new());
    }
}

// After a refusal or a failure the next candidate's answer is taken; the
// query count is the resolution's, not each candidate's: with two allowed, a
// third candidate is never asked.
// @kotowari[EX-250, EX-251]
#[test]
fn later_candidates_answer_within_one_query_count() {
    let (answers, _) = resolve(
        &[Upstream::Code(5), Upstream::Answers],
        short_waits(),
        &["a.example.com"],
    );
    assert_eq!(answers, [(0, 1)]);
    let (answers, asked) = resolve(
        &[Upstream::Code(2), Upstream::Code(2), Upstream::Answers],
        NetworkLimits {
            dns_max_upstream_queries: 2,
            ..short_waits()
        },
        &["a.example.com"],
    );
    assert_eq!(answers, [(2, 0)]);
    assert_eq!(asked[2], Vec::<String>::new());
}

// With the waits left at their defaults, a silent candidate is waited on for
// its 2 seconds before the next one answers.
// @kotowari[EX-252]
#[test]
fn the_default_wait_for_a_candidate_is_two_seconds() {
    let start = Instant::now();
    let (answers, _) = resolve(
        &[Upstream::Silent, Upstream::Answers],
        NetworkLimits::default(),
        &["a.example.com"],
    );
    assert_eq!(answers, [(0, 1)]);
    assert!(start.elapsed() >= Duration::from_millis(1900));
}
