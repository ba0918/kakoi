//! filtered inside a nested isolation: an outer kakoi on the fake host runs a
//! Python program that starts the built kakoi again with `--nested=isolate`
//! and a filtered profile of its own.

use crate::fake_host::{FakeHost, CLIENT};

/// The outer application: starts the nested kakoi with `options` and
/// `inner_app` as its Python program, prints what it printed, then its exit
/// code, and passes its standard error on.
fn outer_app(options: &[&str], inner_app: &str) -> String {
    format!(
        r#"
import subprocess, sys
inner = subprocess.run([{kakoi:?}, '--nested=isolate', {options}'--', '/usr/bin/python3', '-c', {program:?}],
                       capture_output=True, text=True)
print(inner.stdout, end='')
print('inner exit', inner.returncode)
sys.stderr.write(inner.stderr)
"#,
        kakoi = env!("CARGO_BIN_EXE_kakoi"),
        options = options
            .iter()
            .map(|option| format!("{option:?}, "))
            .collect::<String>(),
        program = format!("{CLIENT}{inner_app}"),
    )
}

/// The profile `inner`: the workspace `rw`, filtered, and `network` in its
/// network table.
fn inner_profile(host: &FakeHost, network: &str) {
    host.write(
        ".config/kakoi/profile/inner.toml",
        &format!("[mounts]\nrw = [\"${{workspace}}\"]\n[network]\nmode = 'filtered'\n{network}"),
    );
}

const ALLOW_APP: &str =
    "\n[[network.allow]]\ndestination = { dns = 'app.example' }\nprotocol = 'tcp'\nports = ['8080']\n";
const ALLOW_OTHER: &str =
    "\n[[network.allow]]\ndestination = { dns = 'other.example' }\nprotocol = 'tcp'\nports = ['8080']\n";
const PLAIN_UPSTREAM: &str =
    "\n[[network.dns-upstream]]\ntransport = 'plain'\nip = '127.0.0.1'\nport = 53\n";

/// The nested application: the permitted name, then the address of a name it
/// does not permit.
const INNER_APP: &str = r#"
address = socket.getaddrinfo('app.example', 8080, socket.AF_INET, socket.SOCK_STREAM)[0][4][0]
print('permitted', address, exchange(address, 8080, 'tcp', PERMITTED))
print('not permitted', exchange('11.0.0.6', 8080, 'tcp', REFUSED))
"#;

/// Overwrites the default profile of `host` with a host-mode one that shows
/// the tun device or not.
fn outer_host_profile(host: &FakeHost, allow_nested_filtered: bool) {
    host.write(
        ".config/kakoi/profile/default.toml",
        &format!(
            "[mounts]\nrw = [\"${{workspace}}\"]\n[network]\nmode = 'host'\n\
             allow-nested-filtered = {allow_nested_filtered}\n"
        ),
    );
}

/// Whether the shared file place `place` holds a file with the resolver
/// configuration's content, which a launch that used it for filtered binds.
const RESOLVER_IN_PLACE: &str = r#"
def resolver_in_place(place):
    if not os.path.isdir(place):
        return False
    return any(open(os.path.join(place, name)).read() == 'nameserver 127.0.0.53\n'
               for name in os.listdir(place))
"#;

const SERVE_BOTH: &str = r#"
dns({('app.example', 1): [('11.0.0.5', 300)], ('other.example', 1): [('11.0.0.6', 300)]})
serve('11.0.0.5', 8080, 'tcp', 'app')
serve('11.0.0.6', 8080, 'tcp', 'other')
"#;

// @kotowari[EX-889, REQ-456]
#[test]
fn ex_889_an_outer_host_run_that_shows_tun_lets_a_nested_run_make_filtered() {
    let host = FakeHost::new("", &["11.0.0.5/32", "11.0.0.6/32"]);
    outer_host_profile(&host, true);
    inner_profile(&host, &format!("{PLAIN_UPSTREAM}{ALLOW_APP}"));
    let app = outer_app(&["--profile", "inner"], INNER_APP);

    let output = host.run(&format!(
        r#"{SERVE_BOTH}
process = kakoi({app:?})
print(process.stdout.read().decode(), end='')
print('outer exit', finish(process))
"#
    ));

    assert_eq!(
        output,
        "permitted 11.0.0.5 app\n\
         not permitted failed TimeoutError\n\
         inner exit 0\n\
         outer exit 0\n",
        "{output}"
    );
}

// @kotowari[EX-887]
#[test]
fn ex_887_without_tun_from_the_outer_run_a_nested_filtered_run_does_not_start() {
    let host = FakeHost::new("", &["11.0.0.5/32"]);
    outer_host_profile(&host, false);
    inner_profile(&host, &format!("{PLAIN_UPSTREAM}{ALLOW_APP}"));
    let app = outer_app(&["--profile", "inner"], "print('ran')\n");

    let output = host.run(&format!(
        r#"
process = kakoi({app:?})
print(process.stdout.read().decode(), end='')
print('outer exit', finish(process))
"#
    ));

    assert_eq!(output, "inner exit 125\nouter exit 0\n", "{output}");
}

/// The outer application's first step: the name the nested run does not permit,
/// reached by name, which the outer run permits.
const OUTER_REACHES_OTHER: &str = r#"
address = socket.getaddrinfo('other.example', 8080, socket.AF_INET, socket.SOCK_STREAM)[0][4][0]
print('outer permitted', address, exchange(address, 8080, 'tcp', PERMITTED))
"#;

/// The nested application: the permitted name, then the name it does not
/// permit, and the address the outer run reached under that name.
const INNER_APP_BY_NAME: &str = r#"
address = socket.getaddrinfo('app.example', 8080, socket.AF_INET, socket.SOCK_STREAM)[0][4][0]
print('permitted', address, exchange(address, 8080, 'tcp', PERMITTED))
try:
    other = socket.getaddrinfo('other.example', 8080, socket.AF_INET, socket.SOCK_STREAM)[0][4][0]
    reached = exchange(other, 8080, 'tcp', REFUSED) == 'other'
except socket.gaierror:
    reached = False
print('not permitted reached', reached)
print('not permitted address', exchange('11.0.0.6', 8080, 'tcp', REFUSED))
"#;

// @kotowari[EX-891, REQ-459, REQ-460]
#[test]
fn ex_891_filtered_inside_filtered_resolves_through_the_outer_resolver() {
    let host = FakeHost::following_host_dns(
        &format!("allow-nested-filtered = true\n{ALLOW_APP}{ALLOW_OTHER}"),
        &["11.0.0.5/32", "11.0.0.6/32"],
    );
    inner_profile(&host, ALLOW_APP);
    let app = format!(
        "{OUTER_REACHES_OTHER}{}",
        outer_app(&["--profile", "inner"], INNER_APP_BY_NAME)
    );

    let output = host.run(&format!(
        r#"{SERVE_BOTH}{RESOLVER_IN_PLACE}
resolv('nameserver 127.0.0.1\n')
os.makedirs(os.environ['HOME_DIR'] + '/run')
process = kakoi({app:?}, environment={{'XDG_RUNTIME_DIR': os.environ['HOME_DIR'] + '/run'}},
               unprivileged=True)
print(process.stdout.read().decode(), end='')
print('outer exit', finish(process))
print('place used', resolver_in_place(os.environ['HOME_DIR'] + '/run/kakoi'))
"#
    ));

    assert_eq!(
        output,
        "outer permitted 11.0.0.6 other\n\
         permitted 11.0.0.5 app\n\
         not permitted reached False\n\
         not permitted address failed TimeoutError\n\
         inner exit 0\n\
         outer exit 0\n\
         place used True\n",
        "{output}"
    );
}

// @kotowari[REQ-455, REQ-460]
#[test]
fn req_455_a_filtered_isolation_carries_the_mark_and_a_0600_resolver_configuration() {
    let host = FakeHost::new("", &["11.0.0.5/32"]);
    let app = r#"
import os, stat
mark = '/dev/kakoi-isolated'
print('mark', stat.S_ISREG(os.lstat(mark).st_mode))
try:
    open(mark, 'w').close()
    print('mark writable')
except OSError:
    print('mark not writable')
resolver = os.stat('/etc/resolv.conf')
print('resolv.conf', oct(stat.S_IMODE(resolver.st_mode)), stat.S_ISREG(resolver.st_mode),
      repr(open('/etc/resolv.conf').read()))
"#;

    let output = host.run(&format!(
        r#"{RESOLVER_IN_PLACE}
os.makedirs(os.environ['HOME_DIR'] + '/run')
process = kakoi({app:?}, environment={{'XDG_RUNTIME_DIR': os.environ['HOME_DIR'] + '/run'}})
print(process.stdout.read().decode(), end='')
print('exit', finish(process))
print('place used', resolver_in_place(os.environ['HOME_DIR'] + '/run/kakoi'))
"#
    ));

    assert_eq!(
        output,
        "mark True\n\
         mark not writable\n\
         resolv.conf 0o600 True 'nameserver 127.0.0.53\\n'\n\
         exit 0\n\
         place used True\n",
        "{output}"
    );
}
