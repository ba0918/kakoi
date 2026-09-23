use kakoi_core::{
    network::{Allow, Destination, NetworkLimits, Protocol},
    policy::parse_policy,
};
use kakoi_net::{
    dns::{
        AcceptedRequest, DnsRequests, EnforcedDnsError, ExplicitResolver, PreparedAnswer,
        ResolutionId,
    },
    dns_adoption::DnsAdoption,
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

fn rules() -> Vec<Allow> {
    vec![Allow {
        destination: Destination::Dns("*.example.com".parse().unwrap()),
        protocol: Protocol::Tcp,
        ports: vec!["443".into()].try_into().unwrap(),
    }]
}
fn candidate(requests: &mut DnsRequests<()>, name: u8) -> (ResolutionId, PreparedAnswer) {
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
    let resolver = ExplicitResolver::new(
        rules(),
        config.network.dns_upstream,
        NetworkLimits::default(),
        None,
    )
    .unwrap();
    let mut wire = vec![name, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 1, name];
    wire.extend(b"\x07example\x03com\0\0\x01\0\x01");
    let AcceptedRequest::Start(task) = requests.accept(&wire, (), Instant::now()).unwrap() else {
        panic!()
    };
    let answer = resolver
        .prepare_until(
            &wire,
            task.deadline,
            None,
            &AddressContext::default(),
            |_| None,
        )
        .unwrap();
    upstream.join().unwrap();
    (task.id, answer)
}
fn namespace() -> Arc<NetworkNamespace> {
    let ns = Arc::new(NetworkNamespace::create().unwrap());
    nft::apply(
        &ns,
        Path::new("/usr/sbin/nft"),
        &filter::compile_static(&[], 120).unwrap(),
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    ns
}
/// Waits only guard against a hang; the properties are asserted separately.
const HANG: Duration = Duration::from_secs(10);

fn finish(owner: &mut DnsAdoption) {
    let deadline = Instant::now() + HANG;
    while !owner.is_finished() {
        owner.poll().unwrap();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}

// @kotowari[REQ-125, REQ-014]
#[test]
fn adoption_capacity_is_held_until_result_collection_and_stop_rejects_new_work() {
    let ns = namespace();
    let mut requests = DnsRequests::new(rules(), 0, 4, 1, Duration::from_secs(5)).unwrap();
    let first = candidate(&mut requests, b'a');
    let second = candidate(&mut requests, b'b');
    let mut owner = DnsAdoption::new(Arc::clone(&ns), "/usr/sbin/nft".into(), rules(), 1).unwrap();
    owner.submit(first.0, first.1).unwrap();
    let until = Instant::now() + HANG;
    loop {
        let json = nft::inspect(&ns, Path::new("/usr/sbin/nft"), "kakoi_policy", until).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
        let adopted = value["nftables"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["rule"]["chain"] == "dns_permitted");
        if adopted {
            break;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        owner.submit(second.0, second.1).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    let completion = loop {
        if let Some(done) = owner.poll().unwrap().pop() {
            break done;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(completion.id, first.0);
    assert_eq!(completion.answer.unwrap()[3] & 15, 0);
    let third = candidate(&mut requests, b'c');
    owner.stop();
    assert_eq!(
        owner.submit(third.0, third.1).unwrap_err().kind(),
        std::io::ErrorKind::BrokenPipe
    );
    finish(&mut owner);
    assert!(!owner.is_faulted());
}

// @kotowari[REQ-058]
#[test]
fn a_stalled_kernel_adoption_does_not_block_poll_or_stop_and_fault_is_preserved() {
    let ns = namespace();
    let temp = crate::common::TempDir::new();
    let marker = temp.path().join("activation-started");
    let wrapper = temp.path().join("nft");
    crate::common::write_executable(&wrapper,format!("#!/bin/sh\nif [ \"$1\" != -f ]; then exec /usr/sbin/nft \"$@\"; fi\nscript=$(cat)\ncase \"$script\" in\n  'flush chain'*) touch '{}'; exec /bin/sleep 10 ;;\n  *) printf '%s' \"$script\" | /usr/sbin/nft -f - ;;\nesac\n",marker.display()));
    let mut requests = DnsRequests::new(rules(), 0, 1, 1, Duration::from_secs(5)).unwrap();
    let task = candidate(&mut requests, b'a');
    let mut owner = DnsAdoption::new(ns, wrapper, rules(), 1).unwrap();
    owner.submit(task.0, task.1).unwrap();
    let until = Instant::now() + HANG;
    while !marker.exists() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(owner.poll().unwrap().is_empty());
    owner.stop();
    // The stalled nft ends only at the adoption's 2 second deadline; had the
    // controller waited for it, its result would already be collectable here.
    assert!(
        owner.poll().unwrap().is_empty(),
        "controller waited for nft"
    );
    let until = Instant::now() + HANG;
    let mut seen = false;
    while !owner.is_finished() {
        for done in owner.poll().unwrap() {
            assert_eq!(done.id, task.0);
            assert!(matches!(done.answer, Err(EnforcedDnsError::Enforcement(_))));
            seen = true;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(seen && owner.is_faulted());
}
