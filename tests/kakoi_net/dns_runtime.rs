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

/// One upstream reply: the RCODE, the answer records (type, TTL, data) owned
/// by the question's name, and how long the upstream takes; `None` is silence.
struct Reply {
    code: u8,
    records: Vec<(u16, u32, Vec<u8>)>,
    delay: Duration,
}

impl Reply {
    fn records(records: Vec<(u16, u32, Vec<u8>)>) -> Option<Self> {
        Some(Self {
            code: 0,
            records,
            delay: Duration::ZERO,
        })
    }
}

fn encode_name(name: &str) -> Vec<u8> {
    let mut wire = Vec::new();
    for label in name.split('.') {
        wire.push(label.len() as u8);
        wire.extend(label.as_bytes());
    }
    wire.push(0);
    wire
}

/// What happened to each question, in order: its RCODE and record count, and
/// the kakoi_policy table afterwards.
struct Resolved {
    answers: Vec<(u8, u16)>,
    rules: String,
}

/// A runtime with one explicit upstream that answers with `upstream`, for a
/// policy of one DNS rule `pattern`. Each question is (name, type, offset):
/// the application sends it `offset` after starting, without waiting for the
/// others' answers.
fn resolve_questions(
    pattern: &str,
    limits: NetworkLimits,
    upstream: impl Fn(&str, u16) -> Option<Reply>,
    questions: &[(&str, u16, u64)],
) -> Resolved {
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
            "[[network.dns-upstream]]\ntransport='plain'\nip='127.0.0.1'\nport={}\n",
            socket.local_addr().unwrap().port()
        ),
        Path::new("dns.toml"),
    )
    .unwrap();
    let mut runtime = DnsRuntime::new(
        Arc::clone(&ns),
        DnsRuntimeConfig {
            policy: vec![Allow {
                destination: Destination::Dns(pattern.parse().unwrap()),
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
    let arguments: Vec<String> = questions
        .iter()
        .map(|(name, kind, offset)| format!("{name}/{kind}/{offset}"))
        .collect();
    let mut client = ns
        .command("/usr/bin/python3")
        .unwrap()
        .args([
            "-c",
            r#"
import select, socket, sys, time
questions = [argument.split('/') for argument in sys.argv[1:]]
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
start = time.monotonic()
pending = sorted(range(len(questions)), key=lambda index: int(questions[index][2]))
results = {}
deadline = start + 60
while len(results) < len(questions) and time.monotonic() < deadline:
    while pending and time.monotonic() - start >= int(questions[pending[0]][2]) / 1000:
        index = pending.pop(0)
        name, kind, _ = questions[index]
        wire = bytes([0x12, index, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0])
        for label in name.split('.'):
            wire += bytes([len(label)]) + label.encode()
        wire += b'\0' + int(kind).to_bytes(2, 'big') + b'\0\x01'
        s.sendto(wire, ('127.0.0.53', 53))
    if select.select([s], [], [], .01)[0]:
        data = s.recv(4096)
        results[data[1]] = (data[3] & 15, int.from_bytes(data[6:8], 'big'))
for index in range(len(questions)):
    print(*results.get(index, ('none', 0)), flush=True)
"#,
        ])
        .args(&arguments)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut delayed: Vec<(Instant, Vec<u8>, std::net::SocketAddr)> = Vec::new();
    // Only guards against a hang.
    let deadline = Instant::now() + Duration::from_secs(90);
    while client.try_wait().unwrap().is_none() {
        runtime.poll(Instant::now()).unwrap();
        let mut bytes = [0; 512];
        if let Ok((size, peer)) = socket.recv_from(&mut bytes) {
            let query = &bytes[..size];
            let mut at = 12;
            let mut labels = Vec::new();
            while query[at] != 0 {
                let length = query[at] as usize;
                labels.push(String::from_utf8_lossy(&query[at + 1..at + 1 + length]).into_owned());
                at += 1 + length;
            }
            let kind = u16::from_be_bytes([query[at + 1], query[at + 2]]);
            if let Some(reply) = upstream(&labels.join("."), kind) {
                let mut answer = query.to_vec();
                answer[2] |= 0x80;
                answer[3] = (answer[3] & 0xf0) | reply.code;
                answer[7] = reply.records.len() as u8;
                for (record, ttl, data) in reply.records {
                    answer.extend([0xc0, 0x0c]);
                    answer.extend(record.to_be_bytes());
                    answer.extend([0, 1]);
                    answer.extend(ttl.to_be_bytes());
                    answer.extend((data.len() as u16).to_be_bytes());
                    answer.extend(data);
                }
                delayed.push((Instant::now() + reply.delay, answer, peer));
            }
        }
        let now = Instant::now();
        for (_, answer, peer) in delayed.iter().filter(|(at, _, _)| *at <= now) {
            socket.send_to(answer, peer).unwrap();
        }
        delayed.retain(|(at, _, _)| *at > now);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    let mut output = String::new();
    std::io::Read::read_to_string(&mut client.stdout.take().unwrap(), &mut output).unwrap();
    let answers = output
        .lines()
        .map(|line| {
            let (code, count) = line.split_once(' ').unwrap();
            (code.parse().unwrap_or(255), count.parse().unwrap())
        })
        .collect();
    let rules = String::from_utf8(
        nft::inspect(
            &ns,
            Path::new("/usr/sbin/nft"),
            "kakoi_policy",
            Instant::now() + Duration::from_secs(2),
        )
        .unwrap(),
    )
    .unwrap();
    Resolved { answers, rules }
}

fn a_record(address: [u8; 4]) -> (u16, u32, Vec<u8>) {
    (1, 30, address.to_vec())
}

fn alias(target: &str) -> (u16, u32, Vec<u8>) {
    (5, 30, encode_name(target))
}

/// `c0.example.com` is an alias of `c1.example.com`, and so on down to
/// `c{depth}.example.com`, which has 1.1.1.1 and 2606:4700::1111.
fn chain(depth: u32, delay: Duration) -> impl Fn(&str, u16) -> Option<Reply> {
    move |name: &str, kind: u16| {
        let step: u32 = name.strip_prefix('c')?.split('.').next()?.parse().ok()?;
        let records = if step < depth {
            vec![alias(&format!("c{}.example.com", step + 1))]
        } else if kind == 28 {
            vec![(
                28,
                30,
                "2606:4700::1111"
                    .parse::<std::net::Ipv6Addr>()
                    .unwrap()
                    .octets()
                    .to_vec(),
            )]
        } else {
            vec![a_record([1, 1, 1, 1])]
        };
        Some(Reply {
            code: 0,
            records,
            delay,
        })
    }
}

// An answer whose every address is refused by the policy is REFUSED, and
// grants nothing; the same holds for a name written in Japanese, which is
// resolved and checked as its ASCII form.
// @kotowari[REQ-012, EX-274]
#[test]
fn an_answer_of_refused_addresses_only_is_refused() {
    for (pattern, name) in [
        ("*.example.com", "api.example.com"),
        ("*.テスト", "xn--r8jz45g.xn--zckzah"),
    ] {
        let private = resolve_questions(
            pattern,
            NetworkLimits::default(),
            |_, _| Reply::records(vec![a_record([192, 168, 1, 5])]),
            &[(name, 1, 0)],
        );
        assert_eq!(private.answers, [(5, 0)], "{name}");
        assert!(!private.rules.contains("192.168.1.5"), "{name}");
        let public = resolve_questions(
            pattern,
            NetworkLimits::default(),
            |_, _| Reply::records(vec![a_record([1, 1, 1, 1])]),
            &[(name, 1, 0)],
        );
        assert_eq!(public.answers, [(0, 1)], "{name}");
        assert!(public.rules.contains("1.1.1.1"), "{name}");
    }
}

// A resolution cut by a limit, the hops, the queries, or the time, ends in
// SERVFAIL and grants nothing.
// @kotowari[EX-275, EX-040]
#[test]
fn a_resolution_cut_by_a_limit_is_servfail_without_grants() {
    for (limits, delay) in [
        (
            NetworkLimits {
                dns_max_cname_hops: 2,
                ..NetworkLimits::default()
            },
            Duration::ZERO,
        ),
        (
            NetworkLimits {
                dns_max_upstream_queries: 2,
                ..NetworkLimits::default()
            },
            Duration::ZERO,
        ),
        (
            NetworkLimits {
                dns_server_timeout_seconds: 1,
                dns_resolution_timeout_seconds: 1,
                ..NetworkLimits::default()
            },
            Duration::from_millis(400),
        ),
    ] {
        let resolved = resolve_questions(
            "*.example.com",
            limits,
            chain(4, delay),
            &[("c0.example.com", 1, 0)],
        );
        assert_eq!(resolved.answers, [(2, 0)]);
        assert!(!resolved.rules.contains("1.1.1.1"));
    }
}

// A and AAAA of one name are separate resolutions: each has its own hops,
// its own queries, and its own deadline.
// @kotowari[EX-278, EX-279, EX-280]
#[test]
fn a_and_aaaa_are_separate_resolutions() {
    // Each type follows three aliases under a limit of three, and sends four
    // queries under a limit of four: together they would exceed both.
    let resolved = resolve_questions(
        "*.example.com",
        NetworkLimits {
            dns_max_cname_hops: 3,
            dns_max_upstream_queries: 4,
            ..NetworkLimits::default()
        },
        chain(3, Duration::ZERO),
        &[("c0.example.com", 1, 0), ("c0.example.com", 28, 0)],
    );
    assert_eq!(resolved.answers, [(0, 4), (0, 4)]);
    // The A resolution times out at 3 seconds; the AAAA one, asked a second
    // later and answered at 3.5 seconds, still has its own time.
    let resolved = resolve_questions(
        "*.example.com",
        NetworkLimits {
            dns_server_timeout_seconds: 3,
            dns_resolution_timeout_seconds: 3,
            ..NetworkLimits::default()
        },
        |_, kind| {
            (kind == 28).then(|| Reply {
                code: 0,
                records: vec![(
                    28,
                    30,
                    "2606:4700::1111"
                        .parse::<std::net::Ipv6Addr>()
                        .unwrap()
                        .octets()
                        .to_vec(),
                )],
                delay: Duration::from_millis(2500),
            })
        },
        &[("api.example.com", 1, 0), ("api.example.com", 28, 1000)],
    );
    assert_eq!(resolved.answers, [(2, 0), (0, 1)]);
}

// Addresses inside other record types grant nothing: an HTTPS record's
// ipv4hint, and IP-like bytes in a type kakoi does not know.
// @kotowari[EX-296, EX-331]
#[test]
fn addresses_inside_other_records_grant_nothing() {
    // HTTPS: priority 1, target ".", ipv4hint (key 4) 1.1.1.1.
    let https = vec![0, 1, 0, 0, 4, 0, 4, 1, 1, 1, 1];
    for (kind, data) in [(65u16, https), (65280, vec![1, 1, 1, 1])] {
        let resolved = resolve_questions(
            "*.example.com",
            NetworkLimits::default(),
            |_, asked| Reply::records(vec![(asked, 30, data.clone())]),
            &[("api.example.com", kind, 0)],
        );
        assert_eq!(resolved.answers, [(0, 1)], "{kind}");
        assert!(!resolved.rules.contains("1.1.1.1"), "{kind}");
    }
}
