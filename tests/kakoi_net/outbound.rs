use crate::{fake_host::FakeHost, publish::CONTROL};

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

// The pasta here is the stock Debian build, started with ordinary options
// only: no experimental control interface and no patched build.
// @kotowari[EX-001, EX-002, EX-003, EX-051, EX-052, EX-048, EX-049, EX-310, EX-311, EX-722]
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

// @kotowari[REQ-002, REQ-133, EX-023, EX-024, EX-025, EX-026, EX-027, EX-299]
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
# Asked again within the time to live: answered without the upstream.
assert socket.getaddrinfo('app.example', 8080, socket.AF_INET, socket.SOCK_STREAM)[0][4][0] == v4
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
# One query per type: none for the cached answer, none ahead of expiry.
print('upstream queries', len(asked))
"#
    ));
    assert_eq!(
        output,
        "tcp 11.0.0.5 v4-tcp\n\
         udp 2a00:5::5 v6-udp\n\
         tcp kept v4-tcp\n\
         udp kept v6-udp\n\
         tcp new failed TimeoutError\n\
         udp new failed PermissionError\n\
         upstream queries 2\n",
        "{output}"
    );
}

// Nothing but the mode: the command runs, and reaches neither a remote
// service, nor the host's loopback (by its own localhost or by the host's
// name), nor is anything inside published.
// @kotowari[REQ-085, EX-175, EX-054]
#[test]
fn filtered_alone_starts_and_opens_nothing() {
    let host = FakeHost::new("", &["11.0.0.5/32"]);
    let app = r#"
import sys
print('remote', exchange('11.0.0.5', 8080, 'tcp', REFUSED))
print('localhost', exchange('127.0.0.1', 5432, 'tcp', REFUSED))
address = socket.getaddrinfo('host-v4.kakoi.internal', 5432, socket.AF_INET)[0][4][0]
print('host name', exchange(address, 5432, 'tcp', REFUSED))
server = socket.socket()
server.bind(('127.0.0.1', 8000))
server.listen()
print('listening', flush=True)
sys.stdin.readline()
"#;
    let output = host.run(&format!(
        r#"
serve('11.0.0.5', 8080, 'tcp', 'remote')
serve('127.0.0.1', 5432, 'tcp', 'host-db')
process = kakoi({app:?}, stdin=subprocess.PIPE)
for _ in range(4):
    print(line(process.stdout), end='')
print('published', exchange('127.0.0.1', 8000, 'tcp', REFUSED), exchange('127.0.0.1', 18000, 'tcp', REFUSED))
process.stdin.write(b'end\n')
process.stdin.flush()
print('exit', finish(process), 'received', received)
"#
    ));
    assert_eq!(
        output,
        "remote failed TimeoutError\n\
         localhost failed ConnectionRefusedError\n\
         host name failed TimeoutError\n\
         listening\n\
         published failed ConnectionRefusedError failed ConnectionRefusedError\n\
         exit 0 received []\n",
        "{output}"
    );
}

/// The application's side of the host DNS tests: `resolve` prints the
/// name's address or the failure, `open` makes TCP and UDP exchanges it keeps,
/// `kept` uses them again, and `new ADDRESS` tries a new TCP connection
/// (`refused ADDRESS` when it is expected to fail, with a shorter wait).
const RESOLVER_APP: &str = r#"
import sys
tcp = udp = None
for command in sys.stdin:
    words = command.split()
    if words[0] == 'resolve':
        try:
            print(socket.getaddrinfo('app.example', 8080, socket.AF_INET, socket.SOCK_STREAM)[0][4][0], flush=True)
        except OSError as error:
            print('failed', type(error).__name__, flush=True)
    elif words[0] == 'open':
        tcp = socket.create_connection((words[1], 8080), timeout=PERMITTED)
        udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        udp.connect((words[1], 8080))
        print(tcp.recv(64).decode(), exchange(words[1], 8080, 'udp', PERMITTED, udp), flush=True)
    elif words[0] == 'kept':
        tcp.send(b'again')
        print(tcp.recv(64).decode(), exchange(words[1], 8080, 'udp', PERMITTED, udp), flush=True)
    elif words[0] == 'new':
        print(exchange(words[1], 8080, 'tcp', PERMITTED), flush=True)
    elif words[0] == 'refused':
        print(exchange(words[1], 8080, 'tcp', REFUSED), flush=True)
"#;

const RESOLVER_HOST: &str = r#"
def until_resolved(process, expected):
    """Asks again until the new settings take effect; a hang ends the test."""
    deadline = time.monotonic() + PERMITTED
    while True:
        answer = ask(process, 'resolve')
        if answer == expected:
            return answer
        assert time.monotonic() < deadline, answer
        time.sleep(.2)
"#;

// With no upstream written, the host's resolv.conf names it. A change of the
// file takes effect while running; what was granted and opened before it
// stays, also when the new file names no usable upstream.
// @kotowari[EX-042, EX-235, EX-236, EX-241]
#[test]
fn the_host_resolver_is_followed_without_touching_what_was_granted() {
    let host = FakeHost::following_host_dns(
        "\n[[network.allow]]\ndestination = { dns = 'app.example' }\nprotocol = 'tcp'\nports = ['8080']\n\
         \n[[network.allow]]\ndestination = { dns = 'app.example' }\nprotocol = 'udp'\nports = ['8080']\n",
        &["11.0.0.5/32", "11.0.0.6/32"],
    );
    let output = host.run(&format!(
        r#"{CONTROL}
{RESOLVER_HOST}
import time
dns({{('app.example', 1): [('11.0.0.5', 300)]}}, '127.0.0.1')
dns({{('app.example', 1): [('11.0.0.6', 300)]}}, '127.0.0.2')
for address in ('11.0.0.5', '11.0.0.6'):
    serve(address, 8080, 'tcp', address)
    serve(address, 8080, 'udp', address)
resolv('nameserver 127.0.0.1\n')
process = kakoi({RESOLVER_APP:?}, stdin=subprocess.PIPE)
print('first', ask(process, 'resolve'), ask(process, 'open 11.0.0.5'))
resolv('nameserver 127.0.0.2\n')
print('changed', until_resolved(process, '11.0.0.6'), ask(process, 'kept 11.0.0.5'), ask(process, 'new 11.0.0.5'))
resolv('nothing usable\n')
print('unusable', until_resolved(process, 'failed gaierror'), ask(process, 'kept 11.0.0.5'), ask(process, 'new 11.0.0.5'))
process.stdin.close()
assert finish(process) == 0
"#
    ));
    assert_eq!(
        output,
        "first 11.0.0.5 11.0.0.5 11.0.0.5\n\
         changed 11.0.0.6 11.0.0.5 11.0.0.5 11.0.0.5\n\
         unusable failed gaierror 11.0.0.5 11.0.0.5 11.0.0.5\n",
        "{output}"
    );
}

// An upstream written in the policy is the only one asked: a change of the
// host's resolv.conf does not move it.
// @kotowari[EX-233]
#[test]
fn a_written_upstream_ignores_the_host_resolver() {
    let host = FakeHost::new(
        "\n[[network.allow]]\ndestination = { dns = 'app.example' }\nprotocol = 'tcp'\nports = ['8080']\n",
        &["11.0.0.5/32"],
    );
    let output = host.run(&format!(
        r#"{CONTROL}
import time
dns({{('app.example', 1): [('11.0.0.5', 1)]}}, '127.0.0.1')
dns({{('app.example', 1): [('11.0.0.6', 1)]}}, '127.0.0.2')
resolv('nameserver 127.0.0.1\n')
process = kakoi({RESOLVER_APP:?}, stdin=subprocess.PIPE)
print('first', ask(process, 'resolve'))
resolv('nameserver 127.0.0.2\n')
# Past the reading interval and the answer's time to live.
time.sleep(3)
print('after', ask(process, 'resolve'))
process.stdin.close()
assert finish(process) == 0
"#
    ));
    assert_eq!(output, "first 11.0.0.5\nafter 11.0.0.5\n", "{output}");
}

// A time to live of 0 permits new connections for the grace (5 seconds
// here) from the answer; after it a new one is refused, while those begun
// within it carry on.
// @kotowari[EX-719]
#[test]
fn a_zero_ttl_answer_permits_only_within_its_grace() {
    let host = FakeHost::new(
        "dns-zero-ttl-grace-milliseconds = 5000\n\
         \n[[network.allow]]\ndestination = { dns = 'app.example' }\nprotocol = 'tcp'\nports = ['8080']\n\
         \n[[network.allow]]\ndestination = { dns = 'app.example' }\nprotocol = 'udp'\nports = ['8080']\n",
        &["11.0.0.5/32"],
    );
    let output = host.run(&format!(
        r#"{CONTROL}
import time
dns({{('app.example', 1): [('11.0.0.5', 0)]}})
serve('11.0.0.5', 8080, 'tcp', 'tcp')
serve('11.0.0.5', 8080, 'udp', 'udp')
process = kakoi({RESOLVER_APP:?}, stdin=subprocess.PIPE)
print(ask(process, 'resolve'), ask(process, 'open 11.0.0.5'))
time.sleep(6)
print(ask(process, 'kept 11.0.0.5'), ask(process, 'refused 11.0.0.5').startswith('failed'))
process.stdin.close()
assert finish(process) == 0
"#
    ));
    assert_eq!(output, "11.0.0.5 tcp udp\ntcp udp True\n", "{output}");
}

// Only what kakoi's own resolver answered opens an address: the application
// knowing the address by other means (its own table, its own lookup) gets
// nothing, before and after asking kakoi for another name.
// @kotowari[REQ-029, EX-047, EX-050]
#[test]
fn an_address_the_application_knows_by_itself_is_not_opened() {
    let host = FakeHost::new(
        "\n[[network.allow]]\ndestination = { dns = 'app.example' }\nprotocol = 'tcp'\nports = ['8080']\n",
        &["11.0.0.5/32", "11.0.0.6/32"],
    );
    let output = host.run(&format!(
        r#"{CONTROL}
dns({{('app.example', 1): [('11.0.0.5', 300)]}})
serve('11.0.0.5', 8080, 'tcp', 'resolved')
serve('11.0.0.6', 8080, 'tcp', 'claimed')
process = kakoi({RESOLVER_APP:?}, stdin=subprocess.PIPE)
print('before', ask(process, 'refused 11.0.0.5').startswith('failed'), ask(process, 'refused 11.0.0.6').startswith('failed'))
print('resolved', ask(process, 'resolve'), ask(process, 'new 11.0.0.5'), ask(process, 'refused 11.0.0.6').startswith('failed'))
process.stdin.close()
assert finish(process) == 0
print('received', sorted(received))
"#
    ));
    assert_eq!(
        output, "before True True\nresolved 11.0.0.5 resolved True\nreceived ['resolved']\n",
        "{output}"
    );
}
