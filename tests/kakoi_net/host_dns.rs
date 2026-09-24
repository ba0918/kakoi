use kakoi_net::host_dns::upstreams_from_resolv_conf;

fn addresses(text: &str) -> Vec<String> {
    upstreams_from_resolv_conf(text)
        .unwrap()
        .iter()
        .map(|upstream| {
            assert_eq!(upstream.port().get(), 53);
            assert!(upstream.tls_name().is_none());
            upstream.address.to_string()
        })
        .collect()
}

// @kotowari[REQ-396, EX-731]
#[test]
fn the_host_nameservers_are_the_upstreams_in_their_order() {
    assert_eq!(
        addresses("nameserver 10.255.255.254\nsearch example.test\nnameserver 192.0.2.53\n"),
        ["10.255.255.254", "192.0.2.53"]
    );
    assert_eq!(
        addresses("# comment\nnameserver ::1\noptions ndots:2\nnameserver 127.0.0.53\n"),
        ["::1", "127.0.0.53"]
    );
}

// @kotowari[EX-732]
#[test]
fn the_systemd_resolved_stub_alone_is_reached_through_its_proxy() {
    assert_eq!(
        addresses("nameserver 127.0.0.53\noptions edns0 trust-ad\nsearch .\n"),
        ["127.0.0.54"]
    );
}

// The systemd-resolved proxy chooses among the host's servers by itself, so kakoi gives
// it the whole resolution rather than one candidate's wait; nameservers kakoi tries in
// turn keep the per-candidate wait.
// @kotowari[REQ-116]
#[test]
fn only_the_systemd_resolved_proxy_is_given_the_whole_resolution_wait() {
    use kakoi_net::{host_dns::wait_for, resolution::UpstreamWait};
    let wait = |text| wait_for(&upstreams_from_resolv_conf(text).unwrap());
    assert_eq!(wait("nameserver 127.0.0.53\n"), UpstreamWait::HostResolver);
    assert_eq!(wait("nameserver 127.0.0.54\n"), UpstreamWait::HostResolver);
    assert_eq!(wait("nameserver 10.255.255.254\n"), UpstreamWait::Explicit);
    assert_eq!(
        wait("nameserver 127.0.0.53\nnameserver 192.0.2.53\n"),
        UpstreamWait::Explicit
    );
}

// @kotowari[REQ-145, EX-322]
#[test]
fn a_host_configuration_without_a_nameserver_asks_for_an_explicit_upstream() {
    for text in ["search example.test\n", "", "nameserver not-an-address\n"] {
        let error = upstreams_from_resolv_conf(text).unwrap_err();
        assert!(error.contains("network.dns-upstream"), "{error}");
    }
}

mod following {
    use crate::common::TempDir;
    use kakoi_core::network::{Allow, Destination, DnsUpstream, NetworkLimits, Protocol};
    use kakoi_net::{
        dns_runtime::{DnsRuntime, DnsRuntimeConfig, HostDns},
        filter,
        namespace::NetworkNamespace,
        nft,
        resolution::UpstreamWait,
        scope::AddressContext,
    };
    use std::{
        io::Read,
        net::UdpSocket,
        num::NonZeroU16,
        path::Path,
        process::{Child, Stdio},
        sync::Arc,
        time::{Duration, Instant},
    };

    const HANG: Duration = Duration::from_secs(30);

    /// Test fixtures cannot listen on port 53: "upstream PORT" names a loopback
    /// upstream on that port instead.
    fn fixture_parse(text: &str) -> Result<Vec<DnsUpstream>, String> {
        let ports: Vec<_> = text
            .lines()
            .filter_map(|line| line.strip_prefix("upstream ")?.trim().parse().ok())
            .collect();
        if ports.is_empty() {
            return Err("no upstream".into());
        }
        Ok(ports
            .into_iter()
            .map(|port| {
                DnsUpstream::plain("127.0.0.1".parse().unwrap(), NonZeroU16::new(port).unwrap())
            })
            .collect())
    }

    /// "relay" in the fixture stands for the systemd-resolved proxy.
    fn fixture_wait(upstreams: &[DnsUpstream]) -> UpstreamWait {
        if upstreams.len() == 1 && RELAY.with(|relay| relay.get()) == upstreams[0].port().get() {
            UpstreamWait::HostResolver
        } else {
            UpstreamWait::Explicit
        }
    }

    thread_local! {
        /// The port a test's fixture treats as the systemd-resolved proxy.
        static RELAY: std::cell::Cell<u16> = const { std::cell::Cell::new(0) };
    }

    fn upstream() -> (UdpSocket, u16) {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        let port = socket.local_addr().unwrap().port();
        (socket, port)
    }

    fn runtime(file: &Path) -> (Arc<NetworkNamespace>, DnsRuntime) {
        runtime_with(
            file,
            NetworkLimits {
                dns_resolution_timeout_seconds: 30,
                dns_server_timeout_seconds: 30,
                ..NetworkLimits::default()
            },
        )
    }

    fn runtime_with(file: &Path, limits: NetworkLimits) -> (Arc<NetworkNamespace>, DnsRuntime) {
        start(file, limits, None)
    }

    /// Reads `file` for the upstreams, then, when `change` is given, rewrites it
    /// before the runtime starts: a change made while kakoi is still starting.
    fn start(
        file: &Path,
        limits: NetworkLimits,
        change: Option<&str>,
    ) -> (Arc<NetworkNamespace>, DnsRuntime) {
        let read = std::fs::read_to_string(file).unwrap();
        if let Some(change) = change {
            std::fs::write(file, change).unwrap();
        }
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
            Instant::now() + HANG,
        )
        .unwrap();
        let host_dns = HostDns {
            path: file.to_owned(),
            parse: fixture_parse,
            read: Some(read.clone()),
            wait: fixture_wait,
        };
        let runtime = DnsRuntime::new(
            Arc::clone(&ns),
            DnsRuntimeConfig {
                policy: vec![Allow {
                    destination: Destination::Dns("api.example.com".parse().unwrap()),
                    protocol: Protocol::Tcp,
                    ports: vec!["443".into()].try_into().unwrap(),
                }],
                upstreams: fixture_parse(&read).unwrap(),
                limits,
                nft: "/usr/sbin/nft".into(),
                trust: None,
                scope: AddressContext::default(),
                generation: 0,
                host_dns: Some(host_dns),
            },
            |_| None,
        )
        .unwrap();
        (ns, runtime)
    }

    /// An application's resolver query; it prints the answer's RCODE and addresses.
    fn client(ns: &NetworkNamespace) -> Child {
        ns.command("/usr/bin/python3")
            .unwrap()
            .args(["-c", r#"
import socket
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
s.settimeout(25)
s.sendto(b'\x12\x34\x01\0\0\x01\0\0\0\0\0\0\x03api\x07example\x03com\0\0\x01\0\x01', ('127.0.0.53', 53))
data = s.recv(512)
count = int.from_bytes(data[6:8], 'big')
print(data[3] & 15, '.'.join(str(b) for b in data[-4:]) if count else '-')
"#])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap()
    }

    fn received(socket: &UdpSocket, runtime: &mut DnsRuntime) -> (Vec<u8>, std::net::SocketAddr) {
        let deadline = Instant::now() + HANG;
        loop {
            runtime.poll(Instant::now()).unwrap();
            let mut bytes = [0; 512];
            match socket.recv_from(&mut bytes) {
                Ok((size, peer)) => return (bytes[..size].to_vec(), peer),
                Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock),
            }
            assert!(Instant::now() < deadline, "no upstream query");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn answer(query: &[u8], address: [u8; 4]) -> Vec<u8> {
        let mut answer = query.to_vec();
        answer[2] |= 0x80;
        answer[7] = 1;
        answer.extend([0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 30, 0, 4]);
        answer.extend(address);
        answer
    }

    fn finish(mut child: Child, runtime: &mut DnsRuntime) -> String {
        let deadline = Instant::now() + HANG;
        while child.try_wait().unwrap().is_none() {
            runtime.poll(Instant::now()).unwrap();
            assert!(Instant::now() < deadline, "the application got no answer");
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut output = String::new();
        child
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut output)
            .unwrap();
        output
    }

    // @kotowari[EX-232, EX-237]
    #[test]
    fn a_host_dns_change_redoes_pending_queries_and_ignores_old_answers() {
        let temp = TempDir::new();
        let (old, old_port) = upstream();
        let (new, new_port) = upstream();
        let file = temp.write("resolv.conf", format!("upstream {old_port}\n"));
        let (ns, mut runtime) = runtime(&file);
        let app = client(&ns);
        let (stale_query, stale_peer) = received(&old, &mut runtime);
        std::fs::write(&file, format!("upstream {new_port}\n")).unwrap();
        // The pending query is sent again, to the new upstream only.
        let (query, peer) = received(&new, &mut runtime);
        old.send_to(&answer(&stale_query, [2, 2, 2, 2]), stale_peer)
            .unwrap();
        for _ in 0..20 {
            runtime.poll(Instant::now()).unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
        new.send_to(&answer(&query, [1, 1, 1, 1]), peer).unwrap();
        assert_eq!(finish(app, &mut runtime), "0 1.1.1.1\n");
        let mut bytes = [0; 512];
        assert!(
            old.recv_from(&mut bytes).is_err(),
            "the old upstream was asked again"
        );
        let rules = String::from_utf8(
            nft::inspect(
                &ns,
                Path::new("/usr/sbin/nft"),
                "kakoi_policy",
                Instant::now() + HANG,
            )
            .unwrap(),
        )
        .unwrap();
        assert!(
            !rules.contains("2.2.2.2"),
            "an answer from before the change was adopted"
        );
    }

    // The upstreams in force came from the content read at start-up; a change
    // made before the runtime started is a change to follow like any other.
    // @kotowari[REQ-107]
    #[test]
    fn a_host_dns_change_during_start_up_is_followed() {
        let temp = TempDir::new();
        let (_old, old_port) = upstream();
        let (new, new_port) = upstream();
        let file = temp.write("resolv.conf", format!("upstream {old_port}\n"));
        let (ns, mut runtime) = start(
            &file,
            NetworkLimits {
                dns_resolution_timeout_seconds: 30,
                dns_server_timeout_seconds: 30,
                ..NetworkLimits::default()
            },
            Some(&format!("upstream {new_port}\n")),
        );
        // Let the content be read again before the application asks.
        let noticed = Instant::now() + Duration::from_secs(2);
        while Instant::now() < noticed {
            runtime.poll(Instant::now()).unwrap();
            std::thread::sleep(Duration::from_millis(10));
        }
        let app = client(&ns);
        let (query, peer) = received(&new, &mut runtime);
        new.send_to(&answer(&query, [1, 1, 1, 1]), peer).unwrap();
        assert_eq!(finish(app, &mut runtime), "0 1.1.1.1\n");
    }

    // An upstream that answers after 3 seconds is past one candidate's 2 but
    // well inside the resolution's 10: the relay is waited for, a nameserver
    // kakoi tries itself is not.
    // @kotowari[REQ-116]
    #[test]
    fn the_resolved_proxy_is_waited_for_beyond_one_candidates_wait() {
        for relay in [true, false] {
            let temp = TempDir::new();
            let (slow, port) = upstream();
            RELAY.with(|cell| cell.set(if relay { port } else { 0 }));
            let file = temp.write("resolv.conf", format!("upstream {port}\n"));
            let (ns, mut runtime) = runtime_with(&file, NetworkLimits::default());
            let app = client(&ns);
            let (query, peer) = received(&slow, &mut runtime);
            let answer_at = Instant::now() + Duration::from_secs(3);
            while Instant::now() < answer_at {
                runtime.poll(Instant::now()).unwrap();
                std::thread::sleep(Duration::from_millis(10));
            }
            let _ = slow.send_to(&answer(&query, [1, 1, 1, 1]), peer);
            let expected = if relay { "0 1.1.1.1\n" } else { "2 -\n" };
            assert_eq!(finish(app, &mut runtime), expected, "relay: {relay}");
        }
    }

    // @kotowari[EX-239, EX-240]
    #[test]
    fn an_unreadable_new_host_dns_fails_queries_without_falling_back() {
        let temp = TempDir::new();
        let (old, old_port) = upstream();
        let file = temp.write("resolv.conf", format!("upstream {old_port}\n"));
        let (ns, mut runtime) = runtime(&file);
        std::fs::write(&file, "nothing usable\n").unwrap();
        // Let the change be noticed before the application asks.
        let noticed = Instant::now() + Duration::from_secs(3);
        while Instant::now() < noticed {
            runtime.poll(Instant::now()).unwrap();
            std::thread::sleep(Duration::from_millis(10));
        }
        let app = client(&ns);
        assert_eq!(finish(app, &mut runtime), "2 -\n");
        let mut bytes = [0; 512];
        assert!(
            old.recv_from(&mut bytes).is_err(),
            "the old upstream was used as a fallback"
        );
    }

    // The query sent before the change counts. With two allowed, the redo
    // under the new settings sends one, and fails rather than send another.
    // @kotowari[EX-238, EX-270, EX-334, EX-335]
    #[test]
    fn a_redo_after_a_host_dns_change_keeps_the_query_count() {
        let temp = TempDir::new();
        let (old, old_port) = upstream();
        let (first, first_port) = upstream();
        let (second, second_port) = upstream();
        let file = temp.write("resolv.conf", format!("upstream {old_port}\n"));
        let (ns, mut runtime) = runtime_with(
            &file,
            NetworkLimits {
                dns_resolution_timeout_seconds: 30,
                dns_server_timeout_seconds: 30,
                dns_max_upstream_queries: 2,
                ..NetworkLimits::default()
            },
        );
        let app = client(&ns);
        received(&old, &mut runtime);
        std::fs::write(
            &file,
            format!("upstream {first_port}\nupstream {second_port}\n"),
        )
        .unwrap();
        // The one query left goes to the first new upstream, which fails.
        let (query, peer) = received(&first, &mut runtime);
        let mut failure = query.clone();
        failure[2] |= 0x80;
        failure[3] = (failure[3] & 0xf0) | 2;
        first.send_to(&failure, peer).unwrap();
        assert_eq!(finish(app, &mut runtime), "2 -\n");
        let mut bytes = [0; 512];
        assert!(
            second.recv_from(&mut bytes).is_err(),
            "the redo was given a fresh query count"
        );
    }

    // Six silent upstreams from the host at 2 seconds each would take 12: the
    // default 10 seconds in all ends the resolution before the sixth.
    // @kotowari[EX-255]
    #[test]
    fn the_overall_limit_applies_to_the_host_upstreams() {
        let temp = TempDir::new();
        let silent: Vec<_> = (0..6).map(|_| upstream()).collect();
        let file = temp.write(
            "resolv.conf",
            silent
                .iter()
                .map(|(_, port)| format!("upstream {port}\n"))
                .collect::<String>(),
        );
        let (ns, mut runtime) = runtime_with(&file, NetworkLimits::default());
        let app = client(&ns);
        assert_eq!(finish(app, &mut runtime), "2 -\n");
        let mut bytes = [0; 512];
        assert!(
            silent[5].0.recv_from(&mut bytes).is_err(),
            "the sixth upstream was asked"
        );
        assert!(silent[0].0.recv_from(&mut bytes).is_ok());
    }
}
