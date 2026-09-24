//! A private stand-in for the host: a user, network, and PID namespace of its
//! own, where kakoi runs with the real pasta. Its addresses and ports belong to
//! one test only, and everything in it ends with the namespace.

use crate::common::{TempDir, RW_WORKSPACE};
use std::{
    path::PathBuf,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

/// The real pasta, required: these tests observe the traffic it carries.
pub(crate) fn pasta() -> PathBuf {
    std::env::var_os("KAKOI_TEST_PASTA")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::split_paths(&std::env::var_os("PATH")?)
                .map(|directory| directory.join("pasta"))
                .find(|candidate| candidate.is_file())
        })
        .expect("the real pasta tests need pasta: set KAKOI_TEST_PASTA or put it on PATH")
}

/// Exchanges one message and reports the reply, or the kind of failure. Shared
/// by the host side and the application.
pub(crate) const CLIENT: &str = r#"
import socket

def exchange(address, port, kind, wait, sock=None):
    family = socket.AF_INET6 if ':' in address else socket.AF_INET
    own = sock is None
    if own:
        sock = socket.socket(family, socket.SOCK_STREAM if kind == 'tcp' else socket.SOCK_DGRAM)
    try:
        sock.settimeout(wait)
        if own:
            sock.connect((address, port))
        if kind == 'udp':
            sock.send(b'probe')
        reply = sock.recv(64).decode()
        return reply if reply else 'failed closed'
    except OSError as error:
        return 'failed ' + type(error).__name__
    finally:
        if own:
            sock.close()

# A permitted exchange may take long on a loaded host: its wait only guards
# against a hang. A refused one is dropped, so any wait ends the same way.
PERMITTED = 20
REFUSED = 3
"#;

/// The namespace's process 1 reaps orphans as a host's init does, so that an
/// ended process leaves no entry in `/proc`; the script runs in its child.
const INIT: &str = r#"
import os
script = os.fork()
if script:
    while True:
        pid, status = os.wait()
        if pid == script:
            os._exit(os.waitstatus_to_exitcode(status))
"#;

/// The host side's preparation and helpers. The remote services live in a
/// network namespace of their own, reached over a veth pair: an address the
/// host itself holds is not remote, and kakoi treats it as the host's.
const HOST: &str = r#"
import ipaddress, os, select, subprocess, sys, threading

def ip(*arguments):
    subprocess.run(['/usr/sbin/ip', *arguments], check=True)

ip('link', 'set', 'lo', 'up')
ip('link', 'add', 'probe0', 'type', 'dummy')
ip('link', 'set', 'probe0', 'up')
ip('-4', 'addr', 'add', '198.18.0.1/24', 'dev', 'probe0')
ip('-6', 'addr', 'add', '2001:db8::1/64', 'dev', 'probe0', 'nodad')
ip('-6', 'addr', 'add', 'fe80::1/64', 'dev', 'probe0', 'nodad')
ip('-4', 'route', 'add', 'default', 'via', '198.18.0.254', 'dev', 'probe0')
ip('-6', 'route', 'add', 'default', 'via', '2001:db8::fe', 'dev', 'probe0')

HOST_NS = os.open('/proc/self/ns/net', os.O_RDONLY)
REMOTE_ADDRESSES = {address.split('/')[0] for address in REMOTE}
if REMOTE:
    # Duplicate address detection would hold the link-local addresses that
    # neighbour discovery on the veth pair needs.
    def no_dad():
        for name in ('default', 'all'):
            with open(f'/proc/sys/net/ipv6/conf/{name}/accept_dad', 'w') as knob:
                knob.write('0')
    no_dad()
    remote_holder = subprocess.Popen(['/usr/bin/unshare', '--net', '/usr/bin/sleep', 'infinity'])
    while os.readlink(f'/proc/{remote_holder.pid}/ns/net') == os.readlink('/proc/self/ns/net'):
        pass
    REMOTE_NS = os.open(f'/proc/{remote_holder.pid}/ns/net', os.O_RDONLY)

    def remote_ip(*arguments):
        subprocess.run(['/usr/bin/nsenter', '--net=/proc/%d/ns/net' % remote_holder.pid,
                        '/usr/sbin/ip', *arguments], check=True)

    os.setns(REMOTE_NS, os.CLONE_NEWNET)
    try:
        no_dad()
    finally:
        os.setns(HOST_NS, os.CLONE_NEWNET)
    ip('link', 'add', 'rhost0', 'type', 'veth', 'peer', 'name', 'remote0', 'netns', str(remote_holder.pid))
    ip('-6', 'addr', 'add', 'fe80::fe/64', 'dev', 'rhost0', 'nodad')
    ip('link', 'set', 'rhost0', 'up')
    remote_ip('link', 'set', 'lo', 'up')
    remote_ip('link', 'set', 'remote0', 'up')
    for address in REMOTE:
        remote_ip('addr', 'add', address, 'dev', 'remote0', *(['nodad'] if ':' in address else []))
        ip('route', 'add', str(ipaddress.ip_interface(address).network), 'dev', 'rhost0')
    remote_ip('-4', 'route', 'add', 'default', 'dev', 'remote0')
    remote_ip('-6', 'route', 'add', 'default', 'via', 'fe80::fe', 'dev', 'remote0')

def new_socket(address, family, kind):
    """A socket in the namespace that holds `address`."""
    remote = address in REMOTE_ADDRESSES
    if remote:
        os.setns(REMOTE_NS, os.CLONE_NEWNET)
    try:
        return socket.socket(family, kind)
    finally:
        if remote:
            os.setns(HOST_NS, os.CLONE_NEWNET)

received = []
# The queries the fake upstream DNS received.
asked = []

def serve(address, port, kind, reply):
    """Binds before returning, so that a later exchange cannot race the bind.
    Records each message it receives."""
    family = socket.AF_INET6 if ':' in address else socket.AF_INET
    server = new_socket(address, family, socket.SOCK_STREAM if kind == 'tcp' else socket.SOCK_DGRAM)
    server.bind((address, port))
    if kind == 'tcp':
        server.listen()

    def answer(connection):
        # Replies on connecting and to each later message, until closed.
        with connection:
            received.append(reply)
            connection.sendall(reply.encode())
            while connection.recv(64):
                received.append(reply)
                connection.sendall(reply.encode())

    def loop():
        while True:
            if kind == 'tcp':
                connection, _ = server.accept()
                threading.Thread(target=answer, args=(connection,), daemon=True).start()
            else:
                _, peer = server.recvfrom(64)
                received.append(reply)
                server.sendto(reply.encode(), peer)
    threading.Thread(target=loop, daemon=True).start()
    return server

def dns(records, address='127.0.0.1'):
    """Answers plain queries on `address`:53 from `records`, which maps a
    name and a record type (1 or 28) to its addresses and time to live. An
    unknown name does not exist."""
    server = new_socket(address, socket.AF_INET, socket.SOCK_DGRAM)
    server.bind((address, 53))

    def loop():
        while True:
            query, peer = server.recvfrom(512)
            asked.append(query)
            at, labels = 12, []
            while query[at]:
                labels.append(query[at + 1:at + 1 + query[at]].decode().lower())
                at += 1 + query[at]
            kind = int.from_bytes(query[at + 1:at + 3], 'big')
            name = '.'.join(labels)
            known = any(known_name == name for known_name, _ in records)
            answers = records.get((name, kind), [])
            flags = 0x8180 if known else 0x8183
            response = query[:2] + flags.to_bytes(2, 'big') + (1).to_bytes(2, 'big')
            response += len(answers).to_bytes(2, 'big') + bytes(4) + query[12:at + 5]
            for address, ttl in answers:
                if address == 'rrsig':
                    # A syntactic signature, not a cryptographic one.
                    record, data = 46, bytes([0, kind, 8, 3, 0, 0, 0, 30, 255, 255, 255, 255, 0, 0, 0, 1, 0, 1, 0, 1, 2, 3, 4])
                else:
                    record, data = kind, socket.inet_pton(socket.AF_INET6 if kind == 28 else socket.AF_INET, address)
                response += b'\xc0\x0c' + record.to_bytes(2, 'big') + (1).to_bytes(2, 'big')
                response += ttl.to_bytes(4, 'big') + len(data).to_bytes(2, 'big') + data
            server.sendto(response, peer)
    threading.Thread(target=loop, daemon=True).start()

RESOLV = os.environ['HOME_DIR'] + '/resolv.conf'

def resolv(text):
    """Sets the host's /etc/resolv.conf, as this namespace sees it. The file
    is bound in place, so a change rewrites it: always the same length, in one
    write, so that a reader never sees it half written."""
    padded = text.ljust(256, '#').encode()
    assert len(padded) == 256
    if not os.path.exists(RESOLV):
        with open(RESOLV, 'wb') as new:
            new.write(padded)
        subprocess.run(['/usr/bin/mount', '--bind', RESOLV, '/etc/resolv.conf'], check=True)
    else:
        with open(RESOLV, 'r+b', buffering=0) as current:
            current.write(padded)

def kakoi(app, stdin=subprocess.DEVNULL, options=(), environment=None):
    """Starts kakoi with `options` and `app` as the sandbox's Python program,
    adding `environment` to kakoi's own."""
    return subprocess.Popen(
        [os.environ['KAKOI'], *options, '--', '/usr/bin/python3', '-c', CLIENT + app],
        cwd=os.environ['WORKSPACE'], stdin=stdin, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, bufsize=0,
        env={'PATH': os.environ['BIN'] + ':/usr/sbin:/usr/bin:/bin',
             'HOME': os.environ['HOME_DIR'],
             'XDG_CONFIG_HOME': os.environ['HOME_DIR'] + '/.config',
             **(environment or {})})

def line(stream, wait=PERMITTED):
    """The next line of the unbuffered `stream`, or '' at its end; a hang ends
    the test. Reads a byte at a time, so that no later line waits in a buffer
    that select cannot see."""
    text = b''
    while not text.endswith(b'\n'):
        if not select.select([stream], [], [], wait)[0]:
            raise AssertionError('no line within the wait')
        byte = stream.read(1)
        if not byte:
            break
        text += byte
    return text.decode()

def pastas():
    """The pasta processes in the namespace, by process ID and arguments.
    Only kakoi starts pasta here."""
    found = []
    for entry in os.listdir('/proc'):
        try:
            with open(f'/proc/{entry}/cmdline', 'rb') as arguments:
                words = arguments.read().split(b'\0')
        except (FileNotFoundError, NotADirectoryError, ProcessLookupError):
            continue
        if b'--config-net' in words:
            found.append((int(entry), words))
    return found

def finish(process):
    """Waits for kakoi and passes its standard error on for the report."""
    code = process.wait(timeout=60)
    sys.stderr.write(process.stderr.read().decode())
    return code
"#;

/// A test's own host: the profile of its kakoi, and the directories it uses.
pub(crate) struct FakeHost {
    home: TempDir,
    workspace: PathBuf,
    bin: TempDir,
    remote: Vec<&'static str>,
}

impl FakeHost {
    /// `network` follows the mode in a filtered profile's `[network]` table,
    /// so that its plain keys belong to that table; the
    /// managed resolver's upstream is the host's 127.0.0.1:53, where the
    /// script can start its `dns` helper.
    /// `remote` are the host's addresses, with prefixes, that stand for remote
    /// services. They are global unicast addresses: documentation ranges would
    /// need an explicit permission when a name resolves to them. Nothing leaves
    /// the namespace, so they reach no real host.
    pub(crate) fn new(network: &str, remote: &[&'static str]) -> Self {
        Self::with_profile(
            &format!(
                "{network}\n[[network.dns-upstream]]\ntransport = 'plain'\nip = '127.0.0.1'\nport = 53\n"
            ),
            remote,
        )
    }

    /// As [`Self::new`], but the managed resolver follows the host's
    /// `/etc/resolv.conf`, which the script sets with its `resolv` helper.
    pub(crate) fn following_host_dns(network: &str, remote: &[&'static str]) -> Self {
        Self::with_profile(network, remote)
    }

    fn with_profile(network: &str, remote: &[&'static str]) -> Self {
        let home = TempDir::new();
        let workspace = home.path().join("ws");
        std::fs::create_dir(&workspace).unwrap();
        home.write(
            ".config/kakoi/profile/default.toml",
            format!("{RW_WORKSPACE}\n[network]\nmode = 'filtered'\n\n{network}"),
        );
        let bin = TempDir::new();
        // pasta selects its mode by the name it is started under.
        std::os::unix::fs::symlink(pasta(), bin.path().join("pasta")).unwrap();
        Self {
            home,
            workspace,
            bin,
            remote: remote.to_vec(),
        }
    }

    /// Writes a file under the home directory, outside the workspace, and
    /// returns its path.
    pub(crate) fn write(&self, relative: &str, body: &str) -> PathBuf {
        self.home.write(relative, body)
    }

    /// Puts an executable ahead of the host's own on kakoi's `PATH`.
    pub(crate) fn command(&self, name: &str, body: &str) {
        self.bin.write_executable(name, body);
    }

    /// Runs `script`, a Python program on the host side after the helpers, in
    /// a fresh namespace, and returns its output. A failure of the script fails
    /// the test.
    pub(crate) fn run(&self, script: &str) -> String {
        let program = format!(
            "{INIT}\nREMOTE = {:?}\nCLIENT = {CLIENT:?}\n{CLIENT}\n{HOST}\n{script}",
            self.remote
        );
        let mut child = Command::new("unshare")
            .args([
                "--user",
                "--map-root-user",
                "--net",
                "--pid",
                "--fork",
                "--mount-proc",
                "--kill-child",
                "/usr/bin/python3",
                "-c",
                &program,
            ])
            .env_clear()
            .env("PATH", "/usr/sbin:/usr/bin:/bin")
            .env("KAKOI", env!("CARGO_BIN_EXE_kakoi"))
            .env("HOME_DIR", self.home.path())
            .env("WORKSPACE", &self.workspace)
            .env("BIN", self.bin.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the real pasta tests need unshare from util-linux");
        // Only a hang reaches this limit; the script's own waits are shorter.
        let deadline = Instant::now() + Duration::from_secs(180);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() > deadline {
                // --kill-child ends the namespace with unshare.
                child.kill().unwrap();
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let output = child.wait_with_output().unwrap();
        let report = report(&output);
        assert!(output.status.success(), "{report}");
        // kakoi's notifications help to explain an unexpected result.
        eprintln!("{report}");
        String::from_utf8(output.stdout).unwrap()
    }
}

fn report(output: &Output) -> String {
    format!(
        "fake host exit {:?}\n--- stdout\n{}--- stderr\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
