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
