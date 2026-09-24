use crate::fake_host::FakeHost;

/// The application tries each exchange at once, since every refused one
/// waits out its whole wait, and prints the results in order. A refused TCP
/// connection times out; a refused UDP message fails to send, since the
/// sandbox's own output hook drops it.
fn attempts(exchanges: &[(&str, u16, &str, &str)]) -> String {
    let calls: Vec<String> = exchanges
        .iter()
        .map(|(address, port, kind, wait)| format!("({address:?}, {port}, {kind:?}, {wait})"))
        .collect();
    format!(
        r#"
from concurrent.futures import ThreadPoolExecutor
calls = [{}]
with ThreadPoolExecutor(len(calls)) as pool:
    for (address, port, kind, _), reply in zip(calls, pool.map(lambda call: exchange(*call), calls)):
        print(address, port, kind, reply)
"#,
        calls.join(", ")
    )
}

// @kotowari[EX-001, EX-002, EX-003, EX-051, EX-052, EX-048, EX-049, EX-310, EX-311]
#[test]
fn only_the_permitted_ip_protocol_and_port_reach_a_remote_service() {
    let host = FakeHost::new(
        "\n[[network.allow]]\ndestination = { ip = '11.0.0.5' }\nprotocol = 'tcp'\nports = ['8080']\n\
         \n[[network.allow]]\ndestination = { cidr = '2a00:5::/64' }\nprotocol = 'udp'\nports = ['8080']\n",
        &[
            "11.0.0.5/32",
            "11.0.0.6/32",
            "2a00:5::5/128",
            "2a00:6::5/128",
        ],
    );
    let app = attempts(&[
        ("11.0.0.5", 8080, "tcp", "PERMITTED"),
        ("11.0.0.5", 8080, "udp", "REFUSED"),
        ("11.0.0.5", 8081, "tcp", "REFUSED"),
        ("11.0.0.6", 8080, "tcp", "REFUSED"),
        ("2a00:5::5", 8080, "udp", "PERMITTED"),
        ("2a00:5::5", 8080, "tcp", "REFUSED"),
        ("2a00:5::5", 8081, "udp", "REFUSED"),
        ("2a00:6::5", 8080, "udp", "REFUSED"),
    ]);
    let output = host.run(&format!(
        r#"
for address, port, kind in [
        ('11.0.0.5', 8080, 'tcp'), ('11.0.0.5', 8080, 'udp'),
        ('11.0.0.5', 8081, 'tcp'), ('11.0.0.6', 8080, 'tcp'),
        ('2a00:5::5', 8080, 'udp'), ('2a00:5::5', 8080, 'tcp'),
        ('2a00:5::5', 8081, 'udp'), ('2a00:6::5', 8080, 'udp')]:
    serve(address, port, kind, f'{{address}}/{{kind}}/{{port}}')
process = kakoi({app:?})
print(process.stdout.read().decode(), end='')
assert finish(process) == 0
print('received', sorted(received))
"#
    ));
    assert_eq!(
        output,
        "11.0.0.5 8080 tcp 11.0.0.5/tcp/8080\n\
         11.0.0.5 8080 udp failed PermissionError\n\
         11.0.0.5 8081 tcp failed TimeoutError\n\
         11.0.0.6 8080 tcp failed TimeoutError\n\
         2a00:5::5 8080 udp 2a00:5::5/udp/8080\n\
         2a00:5::5 8080 tcp failed TimeoutError\n\
         2a00:5::5 8081 udp failed PermissionError\n\
         2a00:6::5 8080 udp failed PermissionError\n\
         received ['11.0.0.5/tcp/8080', '2a00:5::5/udp/8080']\n",
        "{output}"
    );
}

// A time to live long enough that the first exchanges start well within it
// even on a loaded host.
const TTL: u32 = 8;

// @kotowari[REQ-002, EX-023, EX-024, EX-025, EX-026, EX-027]
#[test]
fn a_resolved_name_permits_its_addresses_until_the_answer_expires() {
    let host = FakeHost::new(
        "\n[[network.allow]]\ndestination = { dns = 'app.example' }\nprotocol = 'tcp'\nports = ['8080']\n\
         \n[[network.allow]]\ndestination = { dns = 'app.example' }\nprotocol = 'udp'\nports = ['8080']\n",
        &["11.0.0.5/32", "2a00:5::5/128"],
    );
    let app = format!(
        r#"
import time
v4 = socket.getaddrinfo('app.example', 8080, socket.AF_INET, socket.SOCK_STREAM)[0][4][0]
v6 = socket.getaddrinfo('app.example', 8080, socket.AF_INET6, socket.SOCK_DGRAM)[0][4][0]
# Both answers arrived before this point, so both expire by its end.
expired = time.monotonic() + {TTL} + 1
tcp = socket.create_connection((v4, 8080), timeout=PERMITTED)
print('tcp', v4, tcp.recv(64).decode())
udp = socket.socket(socket.AF_INET6, socket.SOCK_DGRAM)
udp.connect((v6, 8080))
print('udp', v6, exchange(v6, 8080, 'udp', PERMITTED, udp))
time.sleep(max(0, expired - time.monotonic()))
tcp.send(b'again')
print('tcp kept', tcp.recv(64).decode())
print('udp kept', exchange(v6, 8080, 'udp', PERMITTED, udp))
print('tcp new', exchange(v4, 8080, 'tcp', REFUSED))
print('udp new', exchange(v6, 8080, 'udp', REFUSED))
"#
    );
    let output = host.run(&format!(
        r#"
dns({{('app.example', 1): [('11.0.0.5', {TTL})], ('app.example', 28): [('2a00:5::5', {TTL})]}})
serve('11.0.0.5', 8080, 'tcp', 'v4-tcp')
serve('2a00:5::5', 8080, 'udp', 'v6-udp')
process = kakoi({app:?})
print(process.stdout.read().decode(), end='')
assert finish(process) == 0
"#
    ));
    assert_eq!(
        output,
        "tcp 11.0.0.5 v4-tcp\n\
         udp 2a00:5::5 v6-udp\n\
         tcp kept v4-tcp\n\
         udp kept v6-udp\n\
         tcp new failed TimeoutError\n\
         udp new failed PermissionError\n",
        "{output}"
    );
}
