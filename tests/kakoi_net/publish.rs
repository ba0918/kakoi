use crate::fake_host::FakeHost;

pub(crate) const TCP_V4: &str =
    "\n[[network.publish]]\nmode = 'fixed'\nprotocol = 'tcp'\nport = 8000\nhost-port = 18000\n";
const UDP_V6: &str = "\n[[network.publish]]\nmode = 'fixed'\nprotocol = 'udp'\nport = 8000\nhost-port = 18000\ntarget-family = 'ipv6'\nhost-family = 'ipv6'\n";

/// The application's services, started and stopped by commands on its standard
/// input (`tcp start`, `udp stop`, ...) and each acknowledged with `done`. A
/// service answers with its name and how many times it has been started.
/// `resolve NAME` answers with the name's IPv4 address, and
/// `remote ADDRESS permitted|refused` with an exchange with ADDRESS's TCP 8080.
pub(crate) const SERVICES: &str = r#"
import select, sys, threading
endpoints = {'tcp': ('127.0.0.1', 8000), 'udp': ('::1', 8000), 'other': ('127.0.0.1', 8001)}
running, starts = {}, {}

def start(name):
    address, port = endpoints[name]
    kind = socket.SOCK_DGRAM if name == 'udp' else socket.SOCK_STREAM
    server = socket.socket(socket.AF_INET6 if ':' in address else socket.AF_INET, kind)
    # A restarted service binds again while its last connection lingers.
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind((address, port))
    if kind == socket.SOCK_STREAM:
        server.listen()
    starts[name] = starts.get(name, 0) + 1
    reply = f'inside-{name}-{starts[name]}'.encode()
    stop = threading.Event()

    def loop():
        while not stop.is_set():
            if not select.select([server], [], [], .05)[0]:
                continue
            if kind == socket.SOCK_STREAM:
                connection, _ = server.accept()
                with connection:
                    connection.sendall(reply)
            else:
                _, peer = server.recvfrom(64)
                server.sendto(reply, peer)
        server.close()
    # Services still running when the input ends do not keep the application.
    thread = threading.Thread(target=loop, daemon=True)
    thread.start()
    running[name] = (stop, thread)

for command in sys.stdin:
    name, action, *rest = command.split()
    if name == 'resolve':
        print(socket.getaddrinfo(action, 8080, socket.AF_INET, socket.SOCK_STREAM)[0][4][0], flush=True)
    elif name == 'remote':
        print(exchange(action, 8080, 'tcp', PERMITTED if rest == ['permitted'] else REFUSED), flush=True)
    elif action == 'start':
        start(name)
        print('done', flush=True)
    else:
        stop, thread = running.pop(name)
        stop.set()
        thread.join()
        print('done', flush=True)
"#;

/// Host-side helpers for a kakoi that runs [`SERVICES`].
pub(crate) const CONTROL: &str = r#"
import errno

def ask(process, text):
    """The application's answer to the command `text`."""
    process.stdin.write((text + '\n').encode())
    process.stdin.flush()
    return line(process.stdout).strip()

def command(process, text):
    assert ask(process, text) == 'done'

def until(process, text):
    """Reads kakoi's standard error up to the next notice containing `text`."""
    while True:
        notice = line(process.stderr)
        assert notice, f'kakoi ended before a notice with {text}'
        if text in notice:
            return notice

def held(address, port, kind):
    """Whether the host's endpoint is taken, observed by binding it."""
    family = socket.AF_INET6 if ':' in address else socket.AF_INET
    with socket.socket(family, socket.SOCK_STREAM if kind == 'tcp' else socket.SOCK_DGRAM) as probe:
        if kind == 'tcp':
            # A listener still refuses the bind; the closed connections that
            # linger after an exchange do not.
            probe.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        try:
            probe.bind((address, port))
            return 'free'
        except OSError as error:
            assert error.errno == errno.EADDRINUSE, error
            return 'held'

def notices(process, count):
    """The next `count` publication notices on kakoi's standard error."""
    found = []
    while len(found) < count:
        text = line(process.stderr)
        assert text, 'kakoi ended before its notices'
        if 'published' in text:
            found.append(text)
    return ''.join(sorted(found))
"#;

// @kotowari[REQ-393, EX-725]
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

// @kotowari[EX-726]
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
