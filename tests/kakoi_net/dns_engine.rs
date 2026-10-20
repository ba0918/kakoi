use crate::dns_fixture::{dns_rule, query};
use kakoi_net::dns::DnsError;

// @kotowari[REQ-027, REQ-019, REQ-020, REQ-014, REQ-123, EX-030, EX-031, EX-045]
#[test]
fn explicit_resolution_installs_only_screened_rule_grants_before_answering() {
    use kakoi_net::{dns::UpstreamResolver, scope::AddressContext};
    use kakoi_runtime::cli::{network::NetworkLimits, policy::parse_policy};
    use std::{
        net::UdpSocket,
        thread,
        time::{Duration, Instant},
    };
    // The third case answers with no lifetime, and the install outlasts the grace it
    // gives: the answer must not be returned once its permission is no longer live.
    for (install_succeeds, outlasted) in [(true, false), (false, false), (true, true)] {
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
                    response.extend(if outlasted { 0u32 } else { 30u32 }.to_be_bytes());
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
        let resolver = UpstreamResolver::new(
            vec![dns_rule("api.example.com")],
            policy.network.dns_upstream,
            NetworkLimits {
                dns_zero_ttl_grace_milliseconds: 1000,
                ..NetworkLimits::default()
            },
            None,
        )
        .unwrap();
        let mut installed = false;
        let response = resolver
            .resolve_until(
                &query("api.example.com", 1),
                // Far off: the configured resolution timeout decides.
                Instant::now() + Duration::from_secs(3600),
                &AddressContext::default(),
                |_| None,
                |grants| {
                    installed = true;
                    assert_eq!(grants.len(), 1);
                    assert_eq!(grants[0].rule, 0);
                    assert_eq!(grants[0].address.to_string(), "1.1.1.1");
                    assert!(grants[0].deadline > Instant::now());
                    if outlasted {
                        thread::sleep(
                            grants[0].deadline.saturating_duration_since(Instant::now())
                                + Duration::from_millis(1),
                        );
                    }
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
            if install_succeeds && !outlasted { 0 } else { 2 }
        );
        assert_eq!(
            response[7],
            if install_succeeds && !outlasted { 2 } else { 0 }
        ); // CNAME + allowed A.
        server.join().unwrap();
        let refused = resolver
            .resolve_until(
                &query("unlisted.example.net", 1),
                Instant::now() + Duration::from_secs(3600),
                &AddressContext::default(),
                |_| None,
                |_| panic!("unauthorized install"),
            )
            .unwrap();
        assert_eq!(refused[3] & 15, 5);
    }
}

// @kotowari[REQ-116, REQ-122, REQ-127]
#[test]
fn admitted_deadline_limits_real_upstream_io_and_expired_work_sends_nothing() {
    use kakoi_net::{
        dns::{AcceptedRequest, DnsRequests, UpstreamResolver},
        scope::AddressContext,
    };
    use kakoi_runtime::cli::{network::NetworkLimits, policy::parse_policy};
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
        UpstreamResolver::new(
            vec![dns_rule("api.example.com")],
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
        vec![dns_rule("api.example.com")],
        0,
        1,
        2,
        Duration::from_secs(5),
    )
    .unwrap();
    let mut tight = DnsRequests::new(
        vec![dns_rule("api.example.com")],
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
