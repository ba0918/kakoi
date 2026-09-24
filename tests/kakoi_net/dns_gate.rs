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
    // No time to live: the same question afterwards is new work, not cached.
    answer.extend([0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 0, 0, 4, 1, 1, 1, 1]);
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

// A failed resolution is held: the same question is answered SERVFAIL without
// new work until the hold ends, and another question is not held.
// @kotowari[REQ-134, EX-301, EX-302]
#[test]
fn a_failed_resolution_is_held_for_the_failure_cache_time() {
    use kakoi_net::dns::{AcceptedRequest, DnsError, DnsRequests};
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let mut requests = DnsRequests::new(
        vec![rule("*.example.com")],
        7,
        4,
        4,
        Duration::from_secs(10),
    )
    .unwrap()
    .with_failure_hold(Duration::from_secs(5));
    let question = query("api.example.com", 1);
    let AcceptedRequest::Start(task) = requests.accept(&question, 1, now).unwrap() else {
        panic!("the first question starts a resolution")
    };
    let replies = requests.complete(task.id, Err(DnsError::IncompleteResponse), now);
    assert_eq!(replies[0].wire[3] & 15, 2);
    let AcceptedRequest::Answer(reply) = requests
        .accept(&question, 2, now + Duration::from_secs(2))
        .unwrap()
    else {
        panic!("a held failure started new work")
    };
    assert_eq!((reply.recipient, reply.wire[3] & 15), (2, 2));
    assert_eq!(&reply.wire[..2], &question[..2]);
    // Another type of the same name is another question.
    assert!(matches!(
        requests
            .accept(
                &query("api.example.com", 28),
                3,
                now + Duration::from_secs(2)
            )
            .unwrap(),
        AcceptedRequest::Start(_)
    ));
    assert!(matches!(
        requests
            .accept(&question, 4, now + Duration::from_secs(5))
            .unwrap(),
        AcceptedRequest::Start(_)
    ));
}

/// An answer to `question` with one A record of `ttl` seconds.
fn answered(question: &[u8], ttl: u32) -> Vec<u8> {
    let mut answer = question.to_vec();
    answer[2] |= 0x80;
    answer[7] = 1;
    answer.extend([0xc0, 0x0c, 0, 1, 0, 1]);
    answer.extend(ttl.to_be_bytes());
    answer.extend([0, 4, 1, 1, 1, 1]);
    answer
}

// A valid answer is used again for the same question until its time to live
// runs out, counted down, with the asker's own ID; an answer with no time to
// live is not kept, and new settings do not use what the old ones cached.
// @kotowari[REQ-133, REQ-389, EX-234]
#[test]
fn a_valid_answer_is_reused_until_its_time_to_live_runs_out() {
    use kakoi_net::dns::{AcceptedRequest, DnsRequests};
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let mut requests = DnsRequests::new(
        vec![rule("*.example.com")],
        7,
        4,
        4,
        Duration::from_secs(10),
    )
    .unwrap();
    let question = query("api.example.com", 1);
    let AcceptedRequest::Start(task) = requests.accept(&question, 1, now).unwrap() else {
        panic!("the first question starts a resolution")
    };
    requests.complete(task.id, Ok(answered(&question, 30)), now);
    let mut again = query("API.example.com", 1);
    again[..2].copy_from_slice(&[0x55, 0x66]);
    let AcceptedRequest::Answer(reply) = requests
        .accept(&again, 2, now + Duration::from_millis(10_500))
        .unwrap()
    else {
        panic!("a valid cached answer started new work")
    };
    let validated = kakoi_net::dns::Question::parse(&again)
        .unwrap()
        .validate_response(&reply.wire)
        .unwrap();
    assert_eq!(validated.wire()[3] & 15, 0);
    // 30 seconds less the 11 begun since the answer.
    let len = reply.wire.len();
    assert_eq!(&reply.wire[len - 10..len - 6], &19u32.to_be_bytes());
    assert_eq!(&reply.wire[len - 4..], &[1, 1, 1, 1]);
    assert!(matches!(
        requests
            .accept(&question, 3, now + Duration::from_secs(30))
            .unwrap(),
        AcceptedRequest::Start(_)
    ));
    // Nothing is kept of an answer without a time to live.
    let other = query("www.example.com", 1);
    let AcceptedRequest::Start(task) = requests.accept(&other, 4, now).unwrap() else {
        panic!()
    };
    requests.complete(task.id, Ok(answered(&other, 0)), now);
    assert!(matches!(
        requests.accept(&other, 5, now).unwrap(),
        AcceptedRequest::Start(_)
    ));
    // New settings: the cache of the old ones is not used.
    let third = query("cdn.example.com", 1);
    let AcceptedRequest::Start(task) = requests.accept(&third, 6, now).unwrap() else {
        panic!()
    };
    requests.complete(task.id, Ok(answered(&third, 30)), now);
    requests.advance_generation();
    assert!(matches!(
        requests.accept(&third, 7, now).unwrap(),
        AcceptedRequest::Start(_)
    ));
}

// Joining a resolution shares its first deadline: at that deadline every
// waiter, the one that joined late included, is answered.
// @kotowari[EX-287]
#[test]
fn a_late_join_keeps_the_shared_deadline() {
    use kakoi_net::dns::{AcceptedRequest, DnsRequests};
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let mut requests =
        DnsRequests::new(vec![rule("*.example.com")], 7, 4, 4, Duration::from_secs(2)).unwrap();
    let question = query("api.example.com", 1);
    assert!(matches!(
        requests.accept(&question, 1, now).unwrap(),
        AcceptedRequest::Start(_)
    ));
    assert!(matches!(
        requests
            .accept(&question, 2, now + Duration::from_millis(1900))
            .unwrap(),
        AcceptedRequest::Waiting
    ));
    assert!(requests
        .expire(now + Duration::from_millis(1999))
        .is_empty());
    let expired = requests.expire(now + Duration::from_secs(2));
    assert_eq!(
        expired
            .iter()
            .map(|reply| reply.recipient)
            .collect::<Vec<_>>(),
        [1, 2]
    );
}

// Two questions from one application count as two waiters.
// @kotowari[EX-292]
#[test]
fn each_question_of_one_application_is_a_waiter() {
    use kakoi_net::dns::{AcceptedRequest, DnsRequests};
    use std::time::{Duration, Instant};
    let now = Instant::now();
    // One recipient stands for one application's socket.
    let mut requests =
        DnsRequests::new(vec![rule("*.example.com")], 7, 4, 2, Duration::from_secs(2)).unwrap();
    let question = query("api.example.com", 1);
    assert!(matches!(
        requests.accept(&question, 1, now).unwrap(),
        AcceptedRequest::Start(_)
    ));
    assert!(matches!(
        requests.accept(&question, 1, now).unwrap(),
        AcceptedRequest::Waiting
    ));
    let AcceptedRequest::Answer(reply) = requests.accept(&question, 1, now).unwrap() else {
        panic!("a third waiter was admitted")
    };
    assert_eq!(reply.wire[3] & 15, 2);
}

// Each environment has its own requests: one full of work and one resolving
// the same name share neither slots nor resolutions.
// @kotowari[EX-283, EX-288]
#[test]
fn environments_share_neither_slots_nor_resolutions() {
    use kakoi_net::dns::{AcceptedRequest, DnsRequests};
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let mut first =
        DnsRequests::new(vec![rule("*.example.com")], 7, 1, 4, Duration::from_secs(2)).unwrap();
    let mut second =
        DnsRequests::new(vec![rule("*.example.com")], 7, 1, 4, Duration::from_secs(2)).unwrap();
    let question = query("api.example.com", 1);
    assert!(matches!(
        first.accept(&question, 1, now).unwrap(),
        AcceptedRequest::Start(_)
    ));
    // The first is full; the second starts the same name on its own.
    assert!(matches!(
        first.accept(&query("www.example.com", 1), 2, now).unwrap(),
        AcceptedRequest::Answer(_)
    ));
    assert!(matches!(
        second.accept(&question, 3, now).unwrap(),
        AcceptedRequest::Start(_)
    ));
}
