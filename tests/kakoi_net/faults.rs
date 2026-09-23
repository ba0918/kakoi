use crate::{
    fake_host::{FakeHost, CLIENT},
    publish::{CONTROL, SERVICES, TCP_V4},
};

/// Host-side helpers that break the transport: the outer pasta is killed, and
/// the name kakoi starts pasta by is pointed at a program that fails at once,
/// until `repair` points it back.
const BREAK: &str = r#"
import signal

def outer_pasta():
    [pid] = [pid for pid, words in pastas() if b'--map-host-loopback' in words]
    return pid

pasta = os.environ['BIN'] + '/pasta'
original = os.readlink(pasta)

def break_transport():
    os.remove(pasta)
    os.symlink('/bin/false', pasta)
    os.kill(outer_pasta(), signal.SIGKILL)

def repair():
    os.remove(pasta)
    os.symlink(original, pasta)
"#;

// A time to live long enough that the first exchanges start well within it
// even on a loaded host.
const TTL: u32 = 8;

// @kotowari[REQ-059, REQ-393, EX-107, EX-109, EX-111, EX-128]
#[test]
fn a_transport_fault_blocks_until_recovery_which_keeps_publications_and_dns_deadlines() {
    let host = FakeHost::new(
        &format!(
            "\n[[network.allow]]\ndestination = {{ ip = '11.0.0.5' }}\nprotocol = 'tcp'\nports = ['8080']\n\
             \n[[network.allow]]\ndestination = {{ dns = 'app.example' }}\nprotocol = 'tcp'\nports = ['8080']\n\
             {TCP_V4}"
        ),
        &["11.0.0.5/32", "11.0.0.6/32"],
    );
    let output = host.run(&format!(
        r#"{CONTROL}
{BREAK}
import time
dns({{('app.example', 1): [('11.0.0.6', {TTL})]}})
serve('11.0.0.5', 8080, 'tcp', 'static')
serve('11.0.0.6', 8080, 'tcp', 'resolved')
process = kakoi({SERVICES:?}, stdin=subprocess.PIPE)
until(process, 'published')
command(process, 'tcp start')
resolved = ask(process, 'resolve app.example')
# The answer arrived before this point, so it expires by its end.
expired = time.monotonic() + {TTL} + 1
print('running', ask(process, 'remote 11.0.0.5 permitted'), ask(process, f'remote {{resolved}} permitted'),
      exchange('127.0.0.1', 18000, 'tcp', PERMITTED))
break_transport()
print(until(process, 'network isolated').split(':')[1].strip())
# How an attempt fails depends on how far the recovery has gone; that nothing
# arrives does not.
before = len(received)
attempts = [ask(process, 'remote 11.0.0.5 refused'), exchange('127.0.0.1', 18000, 'tcp', REFUSED)]
print('isolated', all(attempt.startswith('failed') for attempt in attempts), len(received) - before)
# The resolved permission expires while the transport is down.
time.sleep(max(0, expired - time.monotonic()))
repair()
# The retries back off (1, 2, 4, 8 seconds): the repair comes before the fifth,
# so the recovery comes well within the wait for its notice.
print(until(process, 'network running').strip())
print('restored', ask(process, 'remote 11.0.0.5 permitted'), ask(process, f'remote {{resolved}} refused'),
      exchange('127.0.0.1', 18000, 'tcp', PERMITTED))
process.stdin.close()
assert finish(process) == 0
"#
    ));
    assert_eq!(
        output,
        "running static resolved inside-tcp-1\n\
         network isolated\n\
         isolated True 0\n\
         kakoi: network running: restored\n\
         restored static failed TimeoutError inside-tcp-1\n",
        "{output}"
    );
}

/// A process the main command leaves behind. It serves the published port,
/// and on the grace's termination request reports whether it can still reach
/// the remote service, then waits for a line on its input before it ends.
const LEFT_BEHIND: &str = r#"
import os, signal, threading, time
server = socket.socket()
server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
server.bind(('127.0.0.1', 8000))
server.listen()

def serve():
    while True:
        connection, _ = server.accept()
        with connection:
            connection.sendall(b'left-behind')
threading.Thread(target=serve, daemon=True).start()

def terminate(*_):
    print('terminating', exchange('11.0.0.5', 8080, 'tcp', REFUSED).startswith('failed'), flush=True)
    os.read(0, 16)
    os._exit(0)
signal.signal(signal.SIGTERM, terminate)
print('ready', exchange('11.0.0.5', 8080, 'tcp', PERMITTED), flush=True)
while True:
    time.sleep(1)
"#;

// @kotowari[EX-112, EX-114, EX-115, EX-118, EX-121]
#[test]
fn block_precedes_cleanup_grace() {
    let host = FakeHost::new(
        &format!(
            "\n[[network.allow]]\ndestination = {{ ip = '11.0.0.5' }}\nprotocol = 'tcp'\nports = ['8080']\n\
             {TCP_V4}\n[process]\nshutdown-grace-seconds = 300\n"
        ),
        &["11.0.0.5/32"],
    );
    let child = format!("{CLIENT}{LEFT_BEHIND}");
    let main = format!(
        "import subprocess, sys\nsubprocess.Popen([sys.executable, '-c', {child:?}])\nsys.stdin.readline()\nsys.exit(7)\n"
    );
    let output = host.run(&format!(
        r#"{CONTROL}
import time
serve('11.0.0.5', 8080, 'tcp', 'static')
process = kakoi({main:?}, stdin=subprocess.PIPE)
until(process, 'published')
print(line(process.stdout).strip(), exchange('127.0.0.1', 18000, 'tcp', PERMITTED))
# The main command ends; the process it left behind stays.
before = len(received)
process.stdin.write(b'exit\n')
process.stdin.flush()
print(line(process.stdout).strip())
print('published', exchange('127.0.0.1', 18000, 'tcp', REFUSED).startswith('failed'), len(received) - before)
started = time.monotonic()
process.stdin.write(b'end\n')
process.stdin.flush()
print('exit', finish(process), 'within the grace', time.monotonic() - started < 60)
print('released', held('127.0.0.1', 18000, 'tcp'), 'pasta left', len(pastas()))
"#
    ));
    assert_eq!(
        output,
        "ready static left-behind\n\
         terminating True\n\
         published True 0\n\
         exit 7 within the grace True\n\
         released free pasta left 0\n",
        "{output}"
    );
}

/// A main command that serves the published port and, on the termination
/// request, reports whether it can still reach the remote service, then ends
/// with 0 once a line arrives on its input.
const TERMINATED: &str = r#"
import os, signal, threading, time
server = socket.socket()
server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
server.bind(('127.0.0.1', 8000))
server.listen()

def serve():
    while True:
        connection, _ = server.accept()
        with connection:
            connection.sendall(b'main')
threading.Thread(target=serve, daemon=True).start()

def terminate(*_):
    print('terminating', exchange('11.0.0.5', 8080, 'tcp', REFUSED).startswith('failed'), flush=True)
    os.read(0, 16)
    os._exit(0)
signal.signal(signal.SIGTERM, terminate)
print('ready', exchange('11.0.0.5', 8080, 'tcp', PERMITTED), flush=True)
while True:
    time.sleep(1)
"#;

// @kotowari[EX-219, EX-221]
#[test]
fn sigterm_blocks_before_asking_the_main_command_to_end() {
    let host = FakeHost::new(
        &format!(
            "\n[[network.allow]]\ndestination = {{ ip = '11.0.0.5' }}\nprotocol = 'tcp'\nports = ['8080']\n\
             {TCP_V4}\n[process]\nshutdown-grace-seconds = 300\n"
        ),
        &["11.0.0.5/32"],
    );
    let output = host.run(&format!(
        r#"{CONTROL}
import signal, time
serve('11.0.0.5', 8080, 'tcp', 'static')
process = kakoi({TERMINATED:?}, stdin=subprocess.PIPE)
until(process, 'published')
print(line(process.stdout).strip(), exchange('127.0.0.1', 18000, 'tcp', PERMITTED))
before = len(received)
process.send_signal(signal.SIGTERM)
print(line(process.stdout).strip())
print('published', exchange('127.0.0.1', 18000, 'tcp', REFUSED).startswith('failed'), len(received) - before)
started = time.monotonic()
process.stdin.write(b'end\n')
process.stdin.flush()
print('exit', finish(process), 'within the grace', time.monotonic() - started < 60)
print('released', held('127.0.0.1', 18000, 'tcp'), 'pasta left', len(pastas()))
"#
    ));
    assert_eq!(
        output,
        "ready static main\n\
         terminating True\n\
         published True 0\n\
         exit 143 within the grace True\n\
         released free pasta left 0\n",
        "{output}"
    );
}
