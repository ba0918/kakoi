use crate::fake_host::FakeHost;
use kakoi_net::{
    dns::{AcceptedRequest, DnsRequests},
    host::{HOST_LOOPBACK_V4, HOST_LOOPBACK_V6},
};
use std::{
    net::IpAddr,
    time::{Duration, Instant},
};

fn query(name: &str, kind: u16) -> Vec<u8> {
    let mut wire = vec![0x43, 0x21, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in name.trim_end_matches('.').split('.') {
        wire.push(label.len() as u8);
        wire.extend(label.as_bytes());
    }
    wire.push(0);
    wire.extend(kind.to_be_bytes());
    wire.extend([0, 1]);
    wire
}

/// The addresses in the answer section of a response to `query`.
fn answered(response: &[u8], question: usize) -> (u8, Vec<IpAddr>) {
    let rcode = response[3] & 15;
    let count = u16::from_be_bytes([response[6], response[7]]);
    let mut at = question;
    let mut addresses = Vec::new();
    for _ in 0..count {
        // A compression pointer to the question name.
        assert_eq!(response[at] & 0xc0, 0xc0);
        at += 2;
        let kind = u16::from_be_bytes([response[at], response[at + 1]]);
        let length = u16::from_be_bytes([response[at + 8], response[at + 9]]) as usize;
        let data = &response[at + 10..at + 10 + length];
        addresses.push(match kind {
            1 => IpAddr::from(<[u8; 4]>::try_from(data).unwrap()),
            28 => IpAddr::from(<[u8; 16]>::try_from(data).unwrap()),
            other => panic!("unexpected record type {other}"),
        });
        at += 10 + length;
    }
    (rcode, addresses)
}

// @kotowari[REQ-090]
#[test]
fn reserved_host_names_are_answered_locally_whatever_the_policy() {
    let mut requests = DnsRequests::new(vec![], 0, 4, 4, Duration::from_secs(5)).unwrap();
    for (name, kind, expected) in [
        (
            "host-v4.kakoi.internal",
            1,
            vec![IpAddr::V4(HOST_LOOPBACK_V4)],
        ),
        (
            "HOST-V4.kakoi.internal.",
            1,
            vec![IpAddr::V4(HOST_LOOPBACK_V4)],
        ),
        ("host-v4.kakoi.internal", 28, vec![]),
        (
            "host-v6.kakoi.internal",
            28,
            vec![IpAddr::V6(HOST_LOOPBACK_V6)],
        ),
        ("host-v6.kakoi.internal", 1, vec![]),
    ] {
        let wire = query(name, kind);
        let Ok(AcceptedRequest::Answer(reply)) = requests.accept(&wire, (), Instant::now()) else {
            panic!("{name} was not answered locally");
        };
        assert_eq!(&reply.wire[..2], &wire[..2]);
        assert_eq!(answered(&reply.wire, wire.len()), (0, expected), "{name}");
    }
    // Other names under the reserved domain are not served.
    let Ok(AcceptedRequest::Answer(reply)) =
        requests.accept(&query("other.kakoi.internal", 1), (), Instant::now())
    else {
        panic!("an unauthorized name reached resolution");
    };
    assert_eq!(reply.wire[3] & 15, 5);
}

/// The application resolves `name` for its family and exchanges with the
/// address; it prints the address and the reply.
fn reach(name: &str, port: u16, kind: &str, wait: &str) -> String {
    format!(
        "family = socket.AF_INET6 if 'v6' in {name:?} else socket.AF_INET\n\
         address = socket.getaddrinfo({name:?}, {port}, family)[0][4][0]\n\
         print(address, exchange(address, {port}, {kind:?}, {wait}))\n"
    )
}

/// Runs `app` in kakoi on a fake host whose own loopback serves `services`
/// (address, port, protocol, reply), and prints the application's output.
fn on_host(network: &str, services: &[(&str, u16, &str, &str)], app: &str) -> String {
    let host = FakeHost::new(network, &[]);
    let services: String = services
        .iter()
        .map(|(address, port, kind, reply)| {
            format!("serve({address:?}, {port}, {kind:?}, {reply:?})\n")
        })
        .collect();
    host.run(&format!(
        "{services}process = kakoi({app:?})\nprint(process.stdout.read().decode(), end='')\nassert finish(process) == 0\n"
    ))
}

// @kotowari[EX-190, EX-192, EX-055, EX-056, EX-187]
#[test]
fn the_host_v4_name_reaches_only_the_permitted_host_loopback_port() {
    let output = on_host(
        "\n[[network.allow]]\ndestination = { host-loopback = 'ipv4' }\nprotocol = 'tcp'\nports = ['8080']\n",
        // A listening service on a port the policy does not name.
        &[
            ("127.0.0.1", 8080, "tcp", "host-tcp"),
            ("127.0.0.1", 8081, "tcp", "other"),
        ],
        &format!(
            "{}{}",
            reach("host-v4.kakoi.internal", 8080, "tcp", "PERMITTED"),
            reach("host-v4.kakoi.internal", 8081, "tcp", "REFUSED")
        ),
    );
    assert_eq!(
        output,
        format!("{HOST_LOOPBACK_V4} host-tcp\n{HOST_LOOPBACK_V4} failed TimeoutError\n"),
        "{output}"
    );
}

// @kotowari[EX-191, EX-188]
#[test]
fn the_host_v6_name_reaches_a_permitted_udp_service() {
    let output = on_host(
        "\n[[network.allow]]\ndestination = { host-loopback = 'ipv6' }\nprotocol = 'udp'\nports = ['8080']\n",
        &[("::1", 8080, "udp", "host-udp")],
        &reach("host-v6.kakoi.internal", 8080, "udp", "PERMITTED"),
    );
    assert_eq!(output, format!("{HOST_LOOPBACK_V6} host-udp\n"), "{output}");
}

// @kotowari[EX-194]
#[test]
fn a_dns_wildcard_does_not_permit_the_host_loopback() {
    let output = on_host(
        "\n[[network.allow]]\ndestination = { dns = '*.kakoi.internal' }\nprotocol = 'tcp'\nports = ['8080']\n",
        &[("127.0.0.1", 8080, "tcp", "host-tcp")],
        &reach("host-v4.kakoi.internal", 8080, "tcp", "REFUSED"),
    );
    assert_eq!(
        output,
        format!("{HOST_LOOPBACK_V4} failed TimeoutError\n"),
        "{output}"
    );
}

// An address the host itself holds, outside loopback, is reached only through
// an IP permission: a name permitted by `dns` that resolves to it opens nothing,
// whereas the same answer for a remote address does.
// @kotowari[REQ-139]
#[test]
fn a_dns_permission_does_not_open_an_address_the_host_holds() {
    let dns_only =
        "\n[[network.allow]]\ndestination = { dns = '*.example' }\nprotocol = 'tcp'\nports = ['8080']\n";
    let script = |network: &str| {
        FakeHost::new(network, &["11.0.0.5/32"]).run(&format!(
            r#"
ip('link', 'add', 'lan0', 'type', 'dummy')
ip('link', 'set', 'lan0', 'up')
ip('-4', 'addr', 'add', '11.0.0.7/32', 'dev', 'lan0')
dns({{('own.example', 1): [('11.0.0.7', 300)], ('far.example', 1): [('11.0.0.5', 300)]}})
serve('11.0.0.7', 8080, 'tcp', 'host-lan')
serve('11.0.0.5', 8080, 'tcp', 'remote')
process = kakoi({app:?})
print(process.stdout.read().decode(), end='')
assert finish(process) == 0
"#,
            app = r#"
for name in ('own.example', 'far.example'):
    try:
        address = socket.getaddrinfo(name, 8080, socket.AF_INET, socket.SOCK_STREAM)[0][4][0]
    except OSError:
        print(name, 'not resolved')
        continue
    print(name, exchange(address, 8080, 'tcp', PERMITTED if name == 'far.example' else REFUSED))
"#
        ))
    };
    assert_eq!(
        script(dns_only),
        "own.example not resolved\nfar.example remote\n"
    );
    // With the address permitted as an IP, the same name reaches it.
    assert_eq!(
        script(&format!(
            "{dns_only}\n[[network.allow]]\ndestination = {{ ip = '11.0.0.7' }}\nprotocol = 'tcp'\nports = ['8080']\n"
        )),
        "own.example host-lan\nfar.example remote\n"
    );
}
