use kakoi_core::network::{Allow, Destination, Protocol};
use kakoi_net::dns::{DnsError, DnsGate};

fn query(name: &str, kind: u16) -> Vec<u8> {
    let mut wire = vec![0x12, 0x34, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in name.split('.') {
        wire.push(label.len() as u8);
        wire.extend(label.as_bytes());
    }
    wire.push(0);
    wire.extend(kind.to_be_bytes());
    wire.extend([0, 1]);
    wire
}

fn rule(pattern: &str) -> Allow {
    Allow {
        destination: Destination::Dns(pattern.parse().unwrap()),
        protocol: Protocol::Tcp,
        ports: kakoi_core::network::Ports::try_from(vec!["443".into()]).unwrap(),
    }
}

// @kotowari[REQ-027, REQ-003, REQ-010, REQ-130, REQ-149, EX-273, EX-295, EX-044, EX-046, EX-231]
#[test]
fn only_authorized_names_reach_the_resolver_with_the_original_rule_indices() {
    let gate = DnsGate::new(vec![
        rule("*.example.com"),
        rule("API.Example.com"),
        rule("例え.テスト"),
    ]);
    for (name, indices) in [
        ("API.example.com", vec![0, 1]),
        ("nested.api.example.com", vec![0]),
        ("xn--r8jz45g.xn--zckzah", vec![2]),
    ] {
        for kind in [1, 28, 15, 16, 33, 65, 65280] {
            let wire = query(name, kind);
            let mut called = false;
            let answer = gate
                .dispatch(&wire, |request, matched| {
                    called = true;
                    assert_eq!(matched, indices);
                    assert_eq!(request, wire);
                    let mut response = request.to_vec();
                    response[2] |= 0x80;
                    Ok(response)
                })
                .unwrap();
            assert!(called);
            assert_eq!(answer[3] & 15, 0);
        }
    }
    for name in [
        "example.com",
        "evil-example.com",
        "api.example.com.attacker.net",
        "host-v4.kakoi.internal",
    ] {
        let wire = query(name, 1);
        let answer = gate
            .dispatch(&wire, |_, _| panic!("unauthorized name reached upstream"))
            .unwrap();
        assert_eq!(answer[3] & 15, 5);
        assert_eq!(&answer[..2], &wire[..2]);
        assert_eq!(&answer[12..], &wire[12..]);
    }
}

// @kotowari[REQ-123, REQ-021]
#[test]
fn gate_distinguishes_policy_refusal_from_resolution_failure_and_rejects_wrong_answers() {
    let gate = DnsGate::new(vec![rule("api.example.com")]);
    let wire = query("api.example.com", 1);
    for (error, code) in [
        (DnsError::PolicyDenied, 5),
        (DnsError::AliasCycle, 2),
        (DnsError::IncompleteResponse, 2),
    ] {
        let answer = gate.dispatch(&wire, |_, _| Err(error)).unwrap();
        assert_eq!(answer[3] & 15, code);
        assert_ne!(answer[2] & 0x80, 0);
        assert_ne!(answer[2] & 1, 0); // Preserve RD.
        assert_eq!(&answer[6..12], &[0; 6]);
    }
    let answer = gate
        .dispatch(&wire, |_, _| {
            let mut wrong = query("other.example.com", 1);
            wrong[2] |= 0x80;
            Ok(wrong)
        })
        .unwrap();
    assert_eq!(answer[3] & 15, 2);
}

// @kotowari[REQ-125, REQ-126, REQ-127, REQ-128, REQ-129, EX-291, EX-293]
#[test]
fn resolution_slots_and_waiter_limits_are_independent_and_completion_releases_a_slot() {
    use kakoi_net::dns::{Admission, CapacityError, ResolutionPool};
    let mut pool = ResolutionPool::new(1, 2).unwrap();
    let Admission::Start(id) = pool.admit("A:api.example.com:g1", 10).unwrap() else {
        panic!()
    };
    assert_eq!(
        pool.admit("A:api.example.com:g1", 11),
        Ok(Admission::Join(id))
    );
    assert_eq!(
        pool.admit("A:api.example.com:g1", 12),
        Err((CapacityError::Waiters, 12))
    );
    assert_eq!(
        pool.admit("AAAA:api.example.com:g1", 13),
        Err((CapacityError::Resolutions, 13))
    );
    assert_eq!(
        pool.admit("A:api.example.com:g2", 14),
        Err((CapacityError::Resolutions, 14))
    );
    assert_eq!(pool.complete(id), vec![10, 11]);
    let Admission::Start(next) = pool.admit("A:api.example.com:g1", 15).unwrap() else {
        panic!()
    };
    assert_ne!(id, next);
    assert!(pool.complete(id).is_empty());
    assert_eq!(pool.complete(next), vec![15]);
    assert!(ResolutionPool::<String, usize>::new(0, 1).is_err());
    assert!(ResolutionPool::<String, usize>::new(1, 0).is_err());
}

// @kotowari[REQ-127, REQ-124, REQ-108]
#[test]
fn resolution_keys_share_case_and_client_ids_but_separate_dns_conditions() {
    use kakoi_net::dns::Question;
    let wire = query("api.example.com", 1);
    let key = Question::parse(&wire).unwrap().resolution_key(4).unwrap();
    let mut same = query("API.EXAMPLE.COM", 1);
    same[..2].copy_from_slice(&[99, 88]);
    assert_eq!(
        key,
        Question::parse(&same).unwrap().resolution_key(4).unwrap()
    );
    assert_ne!(
        key,
        Question::parse(&wire).unwrap().resolution_key(5).unwrap()
    );
    assert_ne!(
        key,
        Question::parse(&query("api.example.com", 28))
            .unwrap()
            .resolution_key(4)
            .unwrap()
    );
    let mut no_recursion = wire.clone();
    no_recursion[2] &= !1;
    assert_ne!(
        key,
        Question::parse(&no_recursion)
            .unwrap()
            .resolution_key(4)
            .unwrap()
    );
    let mut checking_disabled = wire;
    checking_disabled[3] |= 0x10;
    assert_ne!(
        key,
        Question::parse(&checking_disabled)
            .unwrap()
            .resolution_key(4)
            .unwrap()
    );
}

// @kotowari[REQ-027, REQ-019, REQ-020, REQ-014, REQ-123, EX-030, EX-031, EX-045]
#[test]
fn explicit_resolution_installs_only_screened_rule_grants_before_answering() {
    use kakoi_core::{network::NetworkLimits, policy::parse_policy};
    use kakoi_net::{dns::ExplicitResolver, scope::AddressContext};
    use std::{
        net::UdpSocket,
        thread,
        time::{Duration, Instant},
    };
    for (install_succeeds, delay_ms) in [(true, 0), (false, 0), (true, 150)] {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let port = socket.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            for turn in 0..2 {
                let mut buffer = [0; 512];
                let (size, client) = socket.recv_from(&mut buffer).unwrap();
                let mut response = buffer[..size].to_vec();
                response[2] |= 0x80;
                let mut append = |kind: u16, data: &[u8]| {
                    response[7] += 1;
                    response.extend([0xc0, 0x0c]);
                    response.extend(kind.to_be_bytes());
                    response.extend([0, 1]);
                    response.extend(if delay_ms == 0 { 30u32 } else { 0u32 }.to_be_bytes());
                    response.extend((data.len() as u16).to_be_bytes());
                    response.extend(data);
                };
                if turn == 0 {
                    let target = query("unlisted.example.net", 1);
                    append(5, &target[12..target.len() - 4]);
                } else {
                    append(1, &[1, 1, 1, 1]);
                    append(1, &[10, 0, 0, 1]);
                }
                socket.send_to(&response, client).unwrap();
            }
        });
        let policy = parse_policy(
            &format!("[[network.dns-upstream]]\ntransport='plain'\nip='127.0.0.1'\nport={port}"),
            std::path::Path::new("upstream.toml"),
        )
        .unwrap();
        let resolver = ExplicitResolver::new(
            vec![rule("api.example.com")],
            policy.network.dns_upstream,
            NetworkLimits {
                dns_zero_ttl_grace_milliseconds: 100,
                ..NetworkLimits::default()
            },
            None,
        )
        .unwrap();
        let mut installed = false;
        let response = resolver
            .resolve(
                &query("api.example.com", 1),
                &AddressContext::default(),
                |_| None,
                |grants| {
                    installed = true;
                    assert_eq!(grants.len(), 1);
                    assert_eq!(grants[0].rule, 0);
                    assert_eq!(grants[0].address.to_string(), "1.1.1.1");
                    assert!(grants[0].deadline > Instant::now());
                    thread::sleep(Duration::from_millis(delay_ms));
                    if install_succeeds {
                        Ok(())
                    } else {
                        Err(DnsError::IncompleteResponse)
                    }
                },
            )
            .unwrap();
        assert!(installed);
        assert_eq!(
            response[3] & 15,
            if install_succeeds && delay_ms == 0 {
                0
            } else {
                2
            }
        );
        assert_eq!(
            response[7],
            if install_succeeds && delay_ms == 0 {
                2
            } else {
                0
            }
        ); // CNAME + allowed A.
        server.join().unwrap();
        let refused = resolver
            .resolve(
                &query("unlisted.example.net", 1),
                &AddressContext::default(),
                |_| None,
                |_| panic!("unauthorized install"),
            )
            .unwrap();
        assert_eq!(refused[3] & 15, 5);
    }
}

// @kotowari[REQ-027, REQ-123, REQ-125, REQ-126, REQ-127, REQ-128, REQ-129, EX-284]
#[test]
fn authorized_requests_share_work_and_return_individual_answers_without_resetting_deadlines() {
    use kakoi_net::dns::{AcceptedRequest, DnsRequests};
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let mut requests =
        DnsRequests::new(vec![rule("*.example.com")], 7, 1, 2, Duration::from_secs(1)).unwrap();
    let first = query("api.example.com", 1);
    let AcceptedRequest::Start(task) = requests.accept(&first, 10, now).unwrap() else {
        panic!()
    };
    assert_eq!(task.wire, first);
    assert_eq!(task.deadline, now + Duration::from_secs(1));
    let mut second = query("API.example.com", 1);
    second[..2].copy_from_slice(&[99, 88]);
    assert!(matches!(
        requests
            .accept(&second, 11, now + Duration::from_millis(900))
            .unwrap(),
        AcceptedRequest::Waiting
    ));
    for (wire, peer, code) in [
        (query("api.example.com", 1), 12, 2),
        (query("api.example.com", 28), 13, 2),
        (query("evil.invalid", 1), 14, 5),
    ] {
        let AcceptedRequest::Answer(reply) = requests.accept(&wire, peer, now).unwrap() else {
            panic!()
        };
        assert_eq!(reply.recipient, peer);
        assert_eq!(reply.wire[3] & 15, code);
    }
    let mut answer = first.clone();
    answer[2] |= 0x80;
    answer[7] = 1;
    answer.extend([0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 30, 0, 4, 1, 1, 1, 1]);
    let replies = requests.complete(task.id, Ok(answer), now + Duration::from_millis(950));
    assert_eq!(replies.len(), 2);
    for (reply, wire, peer) in [(&replies[0], &first, 10), (&replies[1], &second, 11)] {
        assert_eq!(reply.recipient, peer);
        assert_eq!(&reply.wire[..2], &wire[..2]);
        assert_eq!(&reply.wire[12..wire.len()], &wire[12..]);
        kakoi_net::dns::Question::parse(wire)
            .unwrap()
            .validate_response(&reply.wire)
            .unwrap();
        assert_eq!(reply.wire[7], 1);
        assert_eq!(&reply.wire[reply.wire.len() - 4..], &[1, 1, 1, 1]);
    }
    assert!(requests
        .complete(task.id, Err(DnsError::IncompleteResponse), now)
        .is_empty());
    let AcceptedRequest::Start(next) = requests.accept(&first, 15, now).unwrap() else {
        panic!()
    };
    assert_ne!(next.id, task.id);
    assert!(requests.expire(now + Duration::from_millis(999)).is_empty());
    let expired = requests.expire(now + Duration::from_secs(1));
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].recipient, 15);
    assert_eq!(expired[0].wire[3] & 15, 2);
    assert!(requests.complete(next.id, Ok(first), now).is_empty());
}

// @kotowari[REQ-021, REQ-123, REQ-127, EX-289]
#[test]
fn shared_resolution_rejects_wrong_or_late_answers_and_keeps_dns_conditions_separate() {
    use kakoi_net::dns::{AcceptedRequest, DnsRequests};
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let mut requests = DnsRequests::new(
        vec![rule("api.example.com")],
        0,
        4,
        2,
        Duration::from_secs(1),
    )
    .unwrap();
    let wire = query("api.example.com", 1);
    let mut other = wire.clone();
    other[3] |= 0x10;
    let AcceptedRequest::Start(first) = requests.accept(&wire, 1, now).unwrap() else {
        panic!()
    };
    let AcceptedRequest::Start(second) = requests.accept(&other, 2, now).unwrap() else {
        panic!()
    };
    let mut wrong = query("evil.invalid", 1);
    wrong[2] |= 0x80;
    let replies = requests.complete(first.id, Ok(wrong), now);
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0].wire[3] & 15, 2);
    let mut late = other;
    late[2] |= 0x80;
    let replies = requests.complete(second.id, Ok(late), now + Duration::from_secs(1));
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0].wire[3] & 15, 2);
    assert_ne!(replies[0].wire[3] & 0x10, 0);
    assert!(requests.accept(&[0, 1], 3, now).is_err());
}

// @kotowari[REQ-116, REQ-122, REQ-127]
#[test]
fn admitted_deadline_limits_real_upstream_io_and_expired_work_sends_nothing() {
    use kakoi_core::{network::NetworkLimits, policy::parse_policy};
    use kakoi_net::{
        dns::{AcceptedRequest, DnsRequests, ExplicitResolver},
        scope::AddressContext,
    };
    use std::{
        net::UdpSocket,
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Arc,
        },
        time::{Duration, Instant},
    };
    let resolver_for = |socket: &UdpSocket| {
        let policy = parse_policy(
            &format!(
                "[[network.dns-upstream]]\ntransport='plain'\nip='127.0.0.1'\nport={}",
                socket.local_addr().unwrap().port()
            ),
            std::path::Path::new("upstream.toml"),
        )
        .unwrap();
        ExplicitResolver::new(
            vec![rule("api.example.com")],
            policy.network.dns_upstream,
            NetworkLimits::default(),
            None,
        )
        .unwrap()
    };
    // Answers every query with SERVFAIL, so live work never waits on a timer.
    let answering = UdpSocket::bind("127.0.0.1:0").unwrap();
    answering
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let received = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let responder = {
        let socket = answering.try_clone().unwrap();
        let (received, stop) = (Arc::clone(&received), Arc::clone(&stop));
        std::thread::spawn(move || {
            let mut buffer = [0; 512];
            while !stop.load(Ordering::Relaxed) {
                let Ok((length, peer)) = socket.recv_from(&mut buffer) else {
                    continue;
                };
                received.fetch_add(1, Ordering::Relaxed);
                buffer[2] |= 0x80;
                buffer[3] = (buffer[3] & 0xf0) | 2;
                socket.send_to(&buffer[..length], peer).unwrap();
            }
        })
    };
    // Never answers: only an admission deadline can end the wait before the
    // per-server timeout of 2 seconds.
    let silent = UdpSocket::bind("127.0.0.1:0").unwrap();
    silent.set_nonblocking(true).unwrap();
    let (live, bounded) = (resolver_for(&answering), resolver_for(&silent));
    let mut roomy = DnsRequests::new(
        vec![rule("api.example.com")],
        0,
        1,
        2,
        Duration::from_secs(5),
    )
    .unwrap();
    let mut tight = DnsRequests::new(
        vec![rule("api.example.com")],
        0,
        1,
        2,
        Duration::from_millis(200),
    )
    .unwrap();
    let mut buffer = [0; 512];
    for kind in [1, 16] {
        let wire = query("api.example.com", kind);
        let before = received.load(Ordering::Relaxed);
        let AcceptedRequest::Start(task) = roomy.accept(&wire, 10, Instant::now()).unwrap() else {
            panic!()
        };
        let response = live
            .resolve_until(
                &task.wire,
                task.deadline,
                &AddressContext::default(),
                |_| None,
                |_| panic!("no upstream answer, no grants"),
            )
            .unwrap();
        assert_eq!(response[3] & 15, 2);
        assert!(
            received.load(Ordering::Relaxed) > before,
            "live worker did not try upstream"
        );
        assert_eq!(
            roomy.complete(task.id, Ok(response), Instant::now()).len(),
            1
        );

        let now = Instant::now();
        let AcceptedRequest::Start(task) = tight.accept(&wire, 10, now).unwrap() else {
            panic!()
        };
        let response = bounded
            .resolve_until(
                &task.wire,
                task.deadline,
                &AddressContext::default(),
                |_| None,
                |_| panic!("no upstream answer, no grants"),
            )
            .unwrap();
        assert_eq!(response[3] & 15, 2);
        // A reset deadline would wait for the 2 second server timeout instead.
        assert!(
            now.elapsed() < Duration::from_millis(1500),
            "worker reset the admission deadline"
        );
        assert_eq!(
            tight.complete(task.id, Ok(response), Instant::now()).len(),
            1
        );

        while silent.recv_from(&mut buffer).is_ok() {}
        let response = bounded
            .resolve_until(
                &wire,
                now,
                &AddressContext::default(),
                |_| None,
                |_| panic!("expired work installed grants"),
            )
            .unwrap();
        assert_eq!(response[3] & 15, 2);
        assert_eq!(
            silent.recv_from(&mut buffer).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
    stop.store(true, Ordering::Relaxed);
    responder.join().unwrap();
}
