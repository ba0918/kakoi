use kakoi_core::network::{Allow, Destination, Protocol};
use kakoi_net::{
    dns::{AcceptedRequest, DnsRequests, ResolutionTask},
    dns_workers::{DnsWorkers, WorkResult},
};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

fn request(id: u8, requests: &mut DnsRequests<()>, now: Instant) -> ResolutionTask {
    let mut wire = vec![id, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    wire.extend(b"\x03api\x07example\x03com\0\0\x10\0\x01");
    let AcceptedRequest::Start(task) = requests.accept(&wire, (), now).unwrap() else {
        panic!()
    };
    task
}
fn requests() -> DnsRequests<()> {
    DnsRequests::new(
        vec![Allow {
            destination: Destination::Dns("api.example.com".parse().unwrap()),
            protocol: Protocol::Tcp,
            ports: vec!["443".into()].try_into().unwrap(),
        }],
        0,
        1,
        1,
        Duration::from_secs(1),
    )
    .unwrap()
}

// @kotowari[REQ-125, REQ-126]
#[test]
fn expired_request_slots_do_not_free_running_threads_and_stop_discards_late_results() {
    let mut requests = requests();
    let now = Instant::now();
    let mut workers = DnsWorkers::new(1).unwrap();
    let task = request(1, &mut requests, now);
    let id = task.id;
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    workers
        .start(task, move |_, cancel| {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            assert!(cancel.is_cancelled());
            42
        })
        .unwrap();
    started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(requests.expire(now + Duration::from_secs(1)).len(), 1);
    let next = request(2, &mut requests, now + Duration::from_secs(1));
    let (_, error) = workers
        .start(next, |_, _| panic!("capacity bypass"))
        .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
    workers.stop();
    assert!(!workers.is_idle());
    assert!(workers.collect().is_empty());
    release_tx.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let completed = workers.collect();
        if !completed.is_empty() {
            assert_eq!(completed.len(), 1);
            assert_eq!(completed[0].id, id);
            assert!(matches!(completed[0].result, WorkResult::Cancelled));
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(workers.is_idle());
    requests.expire(now + Duration::from_secs(2));
    let task = request(3, &mut requests, now + Duration::from_secs(2));
    assert_eq!(
        workers.start(task, |_, _| 43).unwrap_err().1.kind(),
        std::io::ErrorKind::BrokenPipe
    );
}

// @kotowari[REQ-125]
#[test]
fn worker_completion_and_panic_are_collected_without_blocking_other_work() {
    let mut requests = requests();
    let now = Instant::now();
    let mut workers = DnsWorkers::new(1).unwrap();
    for panic in [false, true] {
        let task = request(1, &mut requests, now);
        let id = task.id;
        workers
            .start(task, move |_, _| {
                assert!(!panic, "deliberate worker fault");
                7
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if let Some(completed) = workers.collect().pop() {
                assert_eq!(completed.id, id);
                assert!(if panic {
                    matches!(completed.result, WorkResult::Panicked)
                } else {
                    matches!(completed.result, WorkResult::Finished(7))
                });
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        requests.expire(now + Duration::from_secs(1));
    }
    assert!(workers.is_idle());
}

// @kotowari[REQ-116, REQ-131]
#[test]
fn stopping_workers_interrupts_udp_tcp_and_tls_waits_without_waiting_for_dns_deadlines() {
    use kakoi_core::{network::NetworkLimits, policy::parse_policy};
    use kakoi_net::{dns::ExplicitResolver, dns_transport::TlsClient, scope::AddressContext};
    use std::io::Read;
    for mode in ["udp", "tcp", "tls", "connect"] {
        let (listener, udp) = crate::common::tcp_and_udp_on_one_port();
        let port = listener.local_addr().unwrap().port();
        let filler = if mode == "connect" {
            use std::os::fd::AsRawFd;
            assert_eq!(unsafe { libc::listen(listener.as_raw_fd(), 0) }, 0);
            let peer = listener.local_addr().unwrap();
            // Successful connect can precede the server's final ACK processing;
            // do not assume exactly one call has already filled its accept queue.
            let mut fillers = Vec::new();
            let mut saturated = false;
            for _ in 0..64 {
                match std::net::TcpStream::connect_timeout(&peer, Duration::from_millis(100)) {
                    Ok(stream) => fillers.push(stream),
                    Err(error) => {
                        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
                        saturated = true;
                        break;
                    }
                }
            }
            assert!(
                saturated && !fillers.is_empty(),
                "accept queue was not filled"
            );
            Some(fillers)
        } else {
            None
        };

        udp.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let _filler = filler;
            if mode != "tls" {
                let mut bytes = [0; 512];
                let (size, peer) = udp.recv_from(&mut bytes).unwrap();
                if matches!(mode, "tcp" | "connect") {
                    bytes[2] |= 0x82;
                    udp.send_to(&bytes[..size], peer).unwrap();
                } else {
                    ready_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
                    return;
                }
            }
            if mode == "connect" {
                std::thread::sleep(Duration::from_millis(100));
                ready_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
                return;
            }
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            ready_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        });
        let extra = if mode == "tls" {
            "tls-name='dns.example.com'"
        } else {
            ""
        };
        let config = parse_policy(
            &format!(
                "[[network.dns-upstream]]\ntransport='{}'\nip='127.0.0.1'\nport={port}\n{extra}",
                if mode == "tls" { "tls" } else { "plain" }
            ),
            std::path::Path::new("dns.toml"),
        )
        .unwrap();
        let rules = vec![Allow {
            destination: Destination::Dns("api.example.com".parse().unwrap()),
            protocol: Protocol::Tcp,
            ports: vec!["443".into()].try_into().unwrap(),
        }];
        let resolver = ExplicitResolver::new(
            rules,
            config.network.dns_upstream,
            NetworkLimits {
                dns_resolution_timeout_seconds: 30,
                dns_server_timeout_seconds: 30,
                ..NetworkLimits::default()
            },
            if mode == "tls" {
                Some(TlsClient::from_root_certificates(Vec::new()).unwrap())
            } else {
                None
            },
        )
        .unwrap();
        let mut requests = requests();
        let mut task = request(1, &mut requests, Instant::now());
        task.deadline = Instant::now() + Duration::from_secs(30);
        let mut workers = DnsWorkers::new(1).unwrap();
        workers
            .start(task, move |task, cancel| {
                resolver.resolve_cancellable_until(
                    &task.wire,
                    task.deadline,
                    &cancel,
                    &AddressContext::default(),
                    |_| None,
                    |_| panic!("cancelled resolution must not grant IPs"),
                )
            })
            .unwrap();
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        workers.stop();
        let start = Instant::now();
        loop {
            if let Some(done) = workers.collect().pop() {
                assert!(matches!(done.result, WorkResult::Cancelled));
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(1),
                "{mode} cancellation waited for DNS timeout"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        release_tx.send(()).unwrap();
        server.join().unwrap();
    }
}
