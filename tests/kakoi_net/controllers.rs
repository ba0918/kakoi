use crate::{
    fake_host::FakeHost,
    publish::{CONTROL, SERVICES, TCP_V4},
};

fn host() -> FakeHost {
    FakeHost::new(
        &format!(
            "\n[[network.allow]]\ndestination = {{ ip = '11.0.0.5' }}\nprotocol = 'tcp'\nports = ['8080']\n\
             {TCP_V4}\n[process]\nshutdown-grace-seconds = 300\n"
        ),
        &["11.0.0.5/32"],
    )
}

/// Host-side helpers: a kakoi running [`SERVICES`] with its published TCP
/// service, the processes that control its network, and whether traffic
/// passes in either direction.
const FAULTS: &str = r#"
import signal, time
serve('11.0.0.5', 8080, 'tcp', 'static')

def start():
    process = kakoi(SERVICES, stdin=subprocess.PIPE)
    until(process, 'published')
    command(process, 'tcp start')
    return process

def monitor(process):
    """The watchdog: the process that holds its closing rules in memory."""
    found = []
    for entry in os.listdir('/proc'):
        try:
            if any(os.readlink(f'/proc/{entry}/fd/{fd}').startswith('/memfd:kakoi-guard')
                   for fd in os.listdir(f'/proc/{entry}/fd')):
                found.append(int(entry))
        except (FileNotFoundError, NotADirectoryError, ProcessLookupError):
            pass
    [pid] = found
    return pid

def passing(process):
    return [ask(process, 'remote 11.0.0.5 permitted'), exchange('127.0.0.1', 18000, 'tcp', PERMITTED)]

def blocked(process=None):
    """Whether nothing arrives either way. How an attempt fails depends on
    where it is stopped; that nothing arrives does not."""
    before = len(received)
    attempts = [exchange('127.0.0.1', 18000, 'tcp', REFUSED)]
    if process:
        attempts.append(ask(process, 'remote 11.0.0.5 refused'))
    return all(attempt.startswith('failed') for attempt in attempts) and len(received) == before

# The health lease lasts 5 seconds and the watchdog waits 3 for a heartbeat;
# a stopped controller is blocked well within this wait.
LAPSED = 8
"#;

fn run(script: &str) -> String {
    run_on(&host(), script)
}

fn run_on(host: &FakeHost, script: &str) -> String {
    host.run(&format!(
        "{CONTROL}\nSERVICES = {SERVICES:?}\n{FAULTS}\n{script}"
    ))
}

// @kotowari[EX-107, EX-109]
#[test]
fn a_stopped_controller_is_blocked_and_recovers_once_it_resumes() {
    let output = run(r#"
process = start()
print('running', *passing(process))
os.kill(process.pid, signal.SIGSTOP)
time.sleep(LAPSED)
print('stopped', blocked(process))
os.kill(process.pid, signal.SIGCONT)
print(until(process, 'network isolated').split(':')[1].strip())
print(until(process, 'network running').strip())
print('resumed', *passing(process))
process.stdin.close()
print('exit', finish(process))
"#);
    assert_eq!(
        output,
        "running static inside-tcp-1\n\
         stopped True\n\
         network isolated\n\
         kakoi: network running: restored\n\
         resumed static inside-tcp-1\n\
         exit 0\n",
        "{output}"
    );
}

// @kotowari[EX-107, EX-109]
#[test]
fn a_stopped_or_killed_monitor_is_replaced_behind_the_block() {
    let output = run(r#"
process = start()
for fault in (signal.SIGSTOP, signal.SIGKILL):
    watchdog = monitor(process)
    os.kill(watchdog, fault)
    print(until(process, 'network isolated').split(':')[1].strip())
    print(until(process, 'network running').strip())
    print('replaced', monitor(process) != watchdog, *passing(process))
process.stdin.close()
print('exit', finish(process))
"#);
    assert_eq!(
        output,
        "network isolated\n\
         kakoi: network running: restored\n\
         replaced True static inside-tcp-1\n\
         network isolated\n\
         kakoi: network running: restored\n\
         replaced True static inside-tcp-1\n\
         exit 0\n",
        "{output}"
    );
}

// Neither the controller nor the watchdog can act: only the kernel's lease on
// the transit remains.
// @kotowari[EX-107]
#[test]
fn the_lease_blocks_when_the_controller_and_monitor_both_stop() {
    let output = run(r#"
process = start()
watchdog = monitor(process)
# The watchdog notices a missing heartbeat only after 3 seconds, and the
# controller one missing acknowledgement likewise: both stop well before.
os.kill(watchdog, signal.SIGSTOP)
os.kill(process.pid, signal.SIGSTOP)
time.sleep(LAPSED)
print('stopped', blocked(process))
os.kill(process.pid, signal.SIGCONT)
os.kill(watchdog, signal.SIGCONT)
print(until(process, 'network isolated').split(':')[1].strip())
print(until(process, 'network running').strip())
print('resumed', *passing(process))
process.stdin.close()
print('exit', finish(process))
"#);
    assert_eq!(
        output,
        "stopped True\n\
         network isolated\n\
         kakoi: network running: restored\n\
         resumed static inside-tcp-1\n\
         exit 0\n",
        "{output}"
    );
}

// @kotowari[EX-112]
#[test]
fn a_killed_controller_leaves_no_transport_behind() {
    let output = run(r#"
process = start()
os.kill(process.pid, signal.SIGKILL)
print('exit', process.wait(timeout=60))
# Until the watchdog acts, the rules in force still hold; only what they
# permit can pass meanwhile.
time.sleep(LAPSED)
print('later', blocked(), 'pasta left', len(pastas()))
"#);
    assert_eq!(output, "exit -9\nlater True pasta left 0\n", "{output}");
}

// Both the transport and the means to block fail: nft fails from the moment
// the outer pasta is killed.
// @kotowari[EX-108, EX-120, EX-122]
#[test]
fn an_unconfirmed_block_ends_the_environment_at_once_with_125() {
    let host = host();
    host.command(
        "nft",
        "#!/bin/sh\n[ -e \"${0%/*}/nft-broken\" ] && exit 1\nexec /usr/sbin/nft \"$@\"\n",
    );
    let output = run_on(
        &host,
        r#"
process = start()
print('running', *passing(process))
open(os.environ['BIN'] + '/nft-broken', 'w').close()
[outer] = [pid for pid, words in pastas() if b'--map-host-loopback' in words]
started = time.monotonic()
os.kill(outer, signal.SIGKILL)
print(until(process, 'network unsafe').split(':')[1].strip())
# Far below the 300 second grace.
print('exit', process.wait(timeout=60), 'at once', time.monotonic() - started < 60)
print('pasta left', len(pastas()))
"#,
    );
    assert_eq!(
        output,
        "running static inside-tcp-1\n\
         network unsafe\n\
         exit 125 at once True\n\
         pasta left 0\n",
        "{output}"
    );
}
