use crate::fake_host::{FakeHost, CONTROL, SERVICES, TCP_V4};

const UDP_V6: &str = "\n[[network.publish]]\nmode = 'fixed'\nprotocol = 'udp'\nport = 8000\nhost-port = 18000\ntarget-family = 'ipv6'\nhost-family = 'ipv6'\n";

// @kotowari[REQ-393, EX-725, REQ-421, EX-808, EX-809, REQ-422, EX-811]
#[test]
fn fixed_publish_lifetime() {
    let host = FakeHost::new(&format!("{TCP_V4}{UDP_V6}"), &[]);
    let output = host.run(&format!(
        r#"{CONTROL}
process = kakoi({SERVICES:?}, stdin=subprocess.PIPE)
print(notices(process, 2), end='')
# Held before the services start, and while they are stopped.
print('before', held('127.0.0.1', 18000, 'tcp'), held('::1', 18000, 'udp'))
command(process, 'tcp start')
command(process, 'udp start')
command(process, 'other start')
print('started', exchange('127.0.0.1', 18000, 'tcp', PERMITTED), exchange('::1', 18000, 'udp', PERMITTED))
# Neither an unnamed service nor the host's other addresses are published.
print('unnamed', exchange('127.0.0.1', 18001, 'tcp', REFUSED), exchange('198.18.0.1', 18000, 'tcp', REFUSED))
command(process, 'tcp stop')
command(process, 'udp stop')
print('stopped', held('127.0.0.1', 18000, 'tcp'), held('::1', 18000, 'udp'))
command(process, 'tcp start')
command(process, 'udp start')
print('restarted', exchange('127.0.0.1', 18000, 'tcp', PERMITTED), exchange('::1', 18000, 'udp', PERMITTED))
process.stdin.close()
assert finish(process) == 0
print('ended', held('127.0.0.1', 18000, 'tcp'), held('::1', 18000, 'udp'))
"#
    ));
    assert_eq!(
        output,
        "kakoi: network published: tcp 127.0.0.1:18000 -> sandbox 127.0.0.1:8000\n\
         kakoi: network published: udp [::1]:18000 -> sandbox [::1]:8000\n\
         before held held\n\
         started inside-tcp-1 inside-udp-1\n\
         unnamed failed ConnectionRefusedError failed ConnectionRefusedError\n\
         stopped held held\n\
         restarted inside-tcp-2 inside-udp-2\n\
         ended free free\n",
        "{output}"
    );
}

/// kakoi's end when the host's UDP endpoint is already taken: its exit code,
/// whether the application ran, and whether the diagnostic names the endpoint.
const TAKEN: &str = r#"
taken = socket.socket(socket.AF_INET6, socket.SOCK_DGRAM)
taken.bind(('::1', 18000))
process = kakoi("open('ran', 'w').close()")
code = process.wait(timeout=60)
diagnostic = process.stderr.read().decode()
sys.stderr.write(diagnostic)
print('exit', code, 'ran', os.path.exists(os.environ['WORKSPACE'] + '/ran'))
print('named', 'UDP port ::1/18000: Address already in use' in diagnostic)
"#;

// @kotowari[EX-726]
#[test]
fn fixed_publish_conflict_prevents_launch() {
    let host = FakeHost::new(UDP_V6, &[]);
    let output = host.run(TAKEN);
    assert_eq!(output, "exit 125 ran False\nnamed True\n", "{output}");
}

// @kotowari[EX-726, REQ-422, EX-810]
#[test]
fn fixed_publish_partial_failure_prevents_launch() {
    let host = FakeHost::new(&format!("{TCP_V4}{UDP_V6}"), &[]);
    let output = host.run(&format!(
        "{CONTROL}\n{TAKEN}\nprint('other', held('127.0.0.1', 18000, 'tcp'))\n"
    ));
    assert_eq!(
        output, "exit 125 ran False\nnamed True\nother free\n",
        "{output}"
    );
}

// Standard output is a pipe to another program: it carries the
// application's output alone, and the publication is told on standard error.
// @kotowari[REQ-423, EX-812, EX-813]
#[test]
fn a_publication_leaves_the_applications_standard_output_as_it_is() {
    let host = FakeHost::new(TCP_V4, &[]);
    let output = host.run(
        r#"
process = kakoi("print('hello')")
out = process.stdout.read().decode()
code = process.wait(timeout=60)
err = process.stderr.read().decode()
sys.stderr.write(err)
print(code, repr(out), 'kakoi: network published: tcp 127.0.0.1:18000' in err)
"#,
    );
    assert_eq!(output, "0 'hello\\n' True\n", "{output}");
}

// The second environment also starts while the first one's TCP connections
// still linger on the host's endpoint.
// @kotowari[REQ-393]
#[test]
fn fixed_udp_service_restart_and_environment_reuse() {
    let host = FakeHost::new(&format!("{TCP_V4}{UDP_V6}"), &[]);
    let output = host.run(&format!(
        r#"{CONTROL}
# One host peer throughout: the same address and port each time.
peer = socket.socket(socket.AF_INET6, socket.SOCK_DGRAM)
peer.connect(('::1', 18000))
for environment in (1, 2):
    process = kakoi({SERVICES:?}, stdin=subprocess.PIPE)
    notices(process, 2)
    command(process, 'tcp start')
    command(process, 'udp start')
    print(environment, exchange('::1', 18000, 'udp', PERMITTED, peer), exchange('127.0.0.1', 18000, 'tcp', PERMITTED))
    command(process, 'udp stop')
    command(process, 'udp start')
    print(environment, exchange('::1', 18000, 'udp', PERMITTED, peer))
    process.stdin.close()
    assert finish(process) == 0
"#
    ));
    assert_eq!(
        output,
        "1 inside-udp-1 inside-tcp-1\n1 inside-udp-2\n2 inside-udp-1 inside-tcp-1\n2 inside-udp-2\n",
        "{output}"
    );
}

// Two environments on one host, each with its own publication and its own
// permission: the end of one leaves the other's traffic in both directions.
// @kotowari[EX-113]
#[test]
fn another_environment_keeps_its_publication_when_one_ends() {
    let host = FakeHost::new(
        "\n[[network.allow]]\ndestination = { ip = '11.0.0.5' }\nprotocol = 'tcp'\nports = ['8080']\n",
        &["11.0.0.5/32"],
    );
    let publish = |host_port| {
        format!(
            "[network]\nmode = 'filtered'\n\n[[network.publish]]\nmode = 'fixed'\nprotocol = 'tcp'\nport = 8000\nhost-port = {host_port}\n"
        )
    };
    let x = host.write("x.toml", &publish(18000));
    let y = host.write("y.toml", &publish(18001));
    let output = host.run(&format!(
        r#"{CONTROL}
serve('11.0.0.5', 8080, 'tcp', 'static')
x = kakoi({SERVICES:?}, stdin=subprocess.PIPE, options=['--policy-file', {x:?}])
y = kakoi({SERVICES:?}, stdin=subprocess.PIPE, options=['--policy-file', {y:?}])
for process in (x, y):
    until(process, 'published')
    command(process, 'tcp start')
print('both', exchange('127.0.0.1', 18000, 'tcp', PERMITTED), exchange('127.0.0.1', 18001, 'tcp', PERMITTED))
x.stdin.close()
assert finish(x) == 0
print('x ended', held('127.0.0.1', 18000, 'tcp'), exchange('127.0.0.1', 18001, 'tcp', PERMITTED),
      ask(y, 'remote 11.0.0.5 permitted'))
y.stdin.close()
assert finish(y) == 0
"#
    ));
    assert_eq!(
        output, "both inside-tcp-1 inside-tcp-1\nx ended free inside-tcp-1 static\n",
        "{output}"
    );
}
