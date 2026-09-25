use crate::common::{assert_diagnostic, binary, TempDir, RW_WORKSPACE};
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Waits that only guard against a hang; the properties are asserted separately.
const HANG: Duration = Duration::from_secs(30);

/// The pasta substitute only supplies readiness and a process to supervise; the
/// namespaces, nft rules and the isolation are real.
const PASTA: &str = r#"#!/usr/bin/python3
import os, time
print(os.getpid(), flush=True)
time.sleep(600)
"#;

struct Filtered {
    home: TempDir,
    workspace: PathBuf,
    bin: TempDir,
}

impl Filtered {
    fn new(extra: &str) -> Self {
        let home = TempDir::new();
        let workspace = home.path().join("ws");
        std::fs::create_dir(&workspace).unwrap();
        home.write(
            ".config/kakoi/profile/default.toml",
            format!(
                "{RW_WORKSPACE}\n[network]\nmode = 'filtered'\n\n[[network.dns-upstream]]\ntransport = 'plain'\nip = '127.0.0.1'\nport = 9\n{extra}"
            ),
        );
        let bin = TempDir::new();
        bin.write_executable("pasta", PASTA);
        Self {
            home,
            workspace,
            bin,
        }
    }

    fn command(&self, path: &str, options: &[&str], arguments: &[&str]) -> Command {
        let mut command = binary(self.home.path());
        command
            .env("PATH", path)
            .current_dir(&self.workspace)
            .args(options)
            .arg("--")
            .args(arguments);
        command
    }

    fn with_pasta(&self, arguments: &[&str]) -> Command {
        self.with_pasta_and_options(&[], arguments)
    }

    fn with_pasta_and_options(&self, options: &[&str], arguments: &[&str]) -> Command {
        self.command(
            &format!("{}:/usr/sbin:/usr/bin:/bin", self.bin.path().display()),
            options,
            arguments,
        )
    }
}

// @kotowari[REQ-068, REQ-148, EX-129, EX-130]
#[test]
fn filtered_run_keeps_supervising_and_returns_the_main_result() {
    let filtered = Filtered::new("");
    let output = filtered
        .with_pasta(&["/bin/sh", "-c", "echo out; exit 7"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(7), "{stderr}");
    // Notifications go to standard error only; the application's output is untouched.
    assert_eq!(output.stdout, b"out\n");
    assert!(stderr.contains("kakoi: network running: ready"), "{stderr}");
}

// @kotowari[EX-323]
#[test]
fn a_policy_without_dns_names_needs_no_dns_upstream() {
    let filtered = Filtered::new("");
    // The same environment without the explicit upstream.
    filtered.home.write(
        ".config/kakoi/profile/default.toml",
        format!("{RW_WORKSPACE}\n[network]\nmode = 'filtered'\n"),
    );
    let output = filtered
        .with_pasta(&["/bin/sh", "-c", "echo ran"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    assert_eq!(output.stdout, b"ran\n");
}

/// Where the diagnostics send a user whose pasta is missing or too old.
const PASTA_GUIDE: &str = "https://github.com/ba0918/kakoi/blob/main/docs/pasta.md";

/// A directory holding only `bwrap` and the given tools, whatever else this
/// machine has installed.
fn only_bwrap_and(tools: &[(&str, &str)]) -> TempDir {
    let directory = TempDir::new();
    let bwrap = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join("bwrap"))
        .find(|candidate| candidate.is_file())
        .unwrap();
    std::os::unix::fs::symlink(bwrap, directory.path().join("bwrap")).unwrap();
    for (name, target) in tools {
        std::os::unix::fs::symlink(target, directory.path().join(name)).unwrap();
    }
    directory
}

// @kotowari[REQ-057, EX-106, REQ-429, EX-824, EX-825]
#[test]
fn a_missing_pasta_ends_the_start_before_the_application_runs() {
    let filtered = Filtered::new("");
    let marker = filtered.workspace.join("ran");
    let tools = only_bwrap_and(&[("nft", "/usr/sbin/nft")]);
    let output = filtered
        .command(
            tools.path().to_str().unwrap(),
            &[],
            &["/bin/sh", "-c", &format!("touch '{}'", marker.display())],
        )
        .output()
        .unwrap();
    let diagnostic = assert_diagnostic(&output, 125, "bwrap");
    assert!(diagnostic.contains("pasta"), "{diagnostic}");
    assert!(diagnostic.contains(PASTA_GUIDE), "{diagnostic}");
    assert!(!marker.exists());
}

// @kotowari[REQ-429, EX-839]
#[test]
fn a_missing_pasta_is_reported_before_a_missing_nft() {
    let filtered = Filtered::new("");
    let tools = only_bwrap_and(&[]);
    let output = filtered
        .command(tools.path().to_str().unwrap(), &[], &["/bin/true"])
        .output()
        .unwrap();
    let diagnostic = assert_diagnostic(&output, 125, "bwrap");
    assert!(
        diagnostic.contains("pasta") && !diagnostic.contains("nft"),
        "{diagnostic}"
    );
    assert!(diagnostic.contains(PASTA_GUIDE), "{diagnostic}");
}

// @kotowari[REQ-102, EX-221]
#[test]
fn sigterm_during_the_main_command_blocks_then_ends_with_143() {
    let filtered = Filtered::new("");
    let mut child = filtered
        .with_pasta(&[
            "/bin/sh",
            "-c",
            "trap 'echo cleanup; exit 0' TERM; echo ready; while :; do sleep 0.1; done",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    assert_eq!(lines.next().unwrap().unwrap(), "ready");
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGTERM) }, 0);
    let deadline = Instant::now() + HANG;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "kakoi did not end");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(status.code(), Some(143));
    assert_eq!(lines.next().unwrap().unwrap(), "cleanup");
}

/// Runs kakoi on a pseudo-terminal, sends Ctrl+C once `trigger` appears in its
/// output, and reports the exit status and the whole output.
fn on_terminal(command: Command, trigger: &str, delay: f64) -> (i32, String, Duration) {
    let program = command.get_program().to_owned();
    let arguments: Vec<_> = command.get_args().map(|a| a.to_owned()).collect();
    let mut harness = Command::new("/usr/bin/python3");
    harness.env_clear();
    for (name, value) in command.get_envs() {
        if let Some(value) = value {
            harness.env(name, value);
        }
    }
    let output = harness
        .current_dir(command.get_current_dir().unwrap_or(Path::new("/")))
        .arg("-c")
        .arg(
            r#"
import os, pty, signal, sys, time
# Only guards against a hang: the test then fails on the missing report.
signal.alarm(120)
trigger, delay = sys.argv[1].encode(), float(sys.argv[2])
pid, fd = pty.fork()
if pid == 0:
    os.execv(sys.argv[3], sys.argv[3:])
out, sent, sent_at = b'', False, None
while True:
    try:
        data = os.read(fd, 4096)
    except OSError:
        break
    if not data:
        break
    out += data
    if not sent and trigger in out:
        time.sleep(delay)
        sent_at = time.monotonic()
        os.write(fd, b'\x03')
        sent = True
_, status = os.waitpid(pid, 0)
ended = time.monotonic()
sys.stdout.write(f"{os.waitstatus_to_exitcode(status)} {ended - (sent_at or ended):.3f}\n")
sys.stdout.write(out.decode(errors='replace'))
"#,
        )
        .arg(trigger)
        .arg(delay.to_string())
        .arg(program)
        .args(arguments)
        .output()
        .unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    let (first, rest) = text.split_once('\n').unwrap();
    let (code, after) = first.split_once(' ').unwrap();
    (
        code.parse().unwrap(),
        rest.to_owned(),
        Duration::from_secs_f64(after.parse().unwrap()),
    )
}

// @kotowari[EX-214]
#[test]
fn ctrl_c_reaches_the_application_which_may_continue() {
    let filtered = Filtered::new("");
    let (code, output, _) = on_terminal(
        filtered.with_pasta(&[
            "/usr/bin/python3",
            "-c",
            // The handler only records: printing from it could reenter a print.
            "import signal, sys, time\ncaught = []\nsignal.signal(signal.SIGINT, lambda *_: caught.append(1))\nprint('app-ready', flush=True)\ntime.sleep(2)\nprint('caught' if caught else 'missed', flush=True)\nprint('continued', flush=True)\nsys.exit(23)",
        ]),
        "app-ready",
        0.0,
    );
    assert_eq!(code, 23, "{output}");
    assert!(
        output.contains("caught") && output.contains("continued"),
        "{output}"
    );
    // The terminal's Ctrl+C reaches none of the network controllers.
    assert!(!output.contains("kakoi: network isolated"), "{output}");
}

// @kotowari[EX-216, EX-217, EX-218]
#[test]
fn ctrl_c_during_the_grace_ends_the_remaining_processes_and_keeps_the_result() {
    let filtered = Filtered::new("\n[process]\nshutdown-grace-seconds = 300\n");
    for result in [0, 7] {
        let (code, output, after) = on_terminal(
            filtered.with_pasta(&[
                "/usr/bin/python3",
                "-c",
                // The remaining child reports the grace's termination request and stays.
                &format!("import subprocess, sys, time\nsubprocess.Popen([sys.executable, '-c', 'import signal, time; signal.signal(signal.SIGTERM, lambda *_: print(\"term-received\", flush=True)); print(\"child-ready\", flush=True); time.sleep(600)'], start_new_session=True)\ntime.sleep(0.5)\nsys.exit({result})"),
            ]),
            "term-received",
            0.0,
        );
        assert_eq!(code, result, "{output}");
        // Far below the 300 second grace.
        assert!(after < HANG, "{after:?}");
        assert!(output.contains("kakoi: grace interrupted"), "{output}");
    }
}

// The main command ends on Ctrl+C: the environment ends as after any main
// exit, the remaining child being asked to end.
// @kotowari[EX-215]
#[test]
fn a_main_command_ended_by_ctrl_c_is_cleaned_up_like_any_main_exit() {
    let filtered = Filtered::new("");
    let (code, output, _) = on_terminal(
        filtered.with_pasta(&[
            "/usr/bin/python3",
            "-c",
            // The child marks itself ready once its handler is in place, so the
            // termination request cannot arrive before the handler does.
            "import os, subprocess, sys, time\nsubprocess.Popen([sys.executable, '-c', 'import signal, sys, time; signal.signal(signal.SIGTERM, lambda *_: (print(\"term-received\", flush=True), sys.exit(0))); open(\"child-ready\", \"w\").close(); time.sleep(600)'], start_new_session=True)\nwhile not os.path.exists('child-ready'):\n    time.sleep(0.01)\nprint('app-ready', flush=True)\ntime.sleep(600)",
        ]),
        "app-ready",
        0.0,
    );
    assert_eq!(code, 130, "{output}");
    assert!(output.contains("term-received"), "{output}");
}

// Neither pasta nor nft is on PATH: only what host and none always needed.
// The unused filtered settings are reported and change nothing.
// @kotowari[EX-166, EX-168, EX-730]
#[test]
fn host_and_none_need_neither_pasta_nor_nft() {
    let bin = TempDir::new();
    for tool in ["bwrap", "git"] {
        let found = ["/usr/bin", "/bin", "/usr/local/bin"]
            .iter()
            .map(|directory| Path::new(directory).join(tool))
            .find(|candidate| candidate.is_file())
            .unwrap_or_else(|| panic!("the test needs {tool}"));
        std::os::unix::fs::symlink(found, bin.path().join(tool)).unwrap();
    }
    for mode in ["host", "none"] {
        let home = TempDir::new();
        let workspace = home.path().join("ws");
        std::fs::create_dir(&workspace).unwrap();
        home.write(
            ".config/kakoi/profile/default.toml",
            format!(
                "{RW_WORKSPACE}\n[network]\nmode = '{mode}'\n\n[[network.allow]]\ndestination = {{ dns = 'example.com' }}\nprotocol = 'tcp'\nports = ['443']\n\n[[network.publish]]\nmode = 'fixed'\nprotocol = 'tcp'\nport = 8000\nhost-port = 18000\n"
            ),
        );
        let output = binary(home.path())
            .env("PATH", bin.path())
            .current_dir(&workspace)
            .args(["--", "/bin/sh", "-c", "echo ran"])
            .output()
            .unwrap();
        assert_eq!(
            (
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
            (
                Some(0),
                "ran\n".into(),
                "kakoi: warning: network/process settings are unused outside filtered mode\n"
                    .into()
            ),
            "{mode}"
        );
    }
}

/// A main command for timing tests: it ignores termination requests, leaves
/// `children` more processes that ignore them too, prints `ready`, and then
/// either keeps running or, given an exit code, ends with it.
fn stubborn(children: usize, exit: Option<i32>) -> String {
    let mut script = String::from("trap '' TERM\n");
    for _ in 0..children {
        script.push_str("(trap '' TERM; while :; do sleep 0.1; done) &\n");
    }
    script.push_str("echo ready\n");
    match exit {
        Some(code) => script.push_str(&format!("exit {code}\n")),
        None => script.push_str("while :; do sleep 0.1; done\n"),
    }
    script
}

/// Starts `script` under kakoi and waits for its `ready`.
fn started(filtered: &Filtered, script: &str) -> (std::process::Child, Instant) {
    let mut child = filtered
        .with_pasta(&["/bin/sh", "-c", script])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    assert_eq!(lines.next().unwrap().unwrap(), "ready");
    (child, Instant::now())
}

fn sigterm(child: &std::process::Child) {
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGTERM) }, 0);
}

/// Waits for kakoi's end and returns its exit code and when it came.
fn ended(child: &mut std::process::Child) -> (i32, Instant) {
    let deadline = Instant::now() + HANG;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return (status.code().unwrap(), Instant::now());
        }
        assert!(Instant::now() < deadline, "kakoi did not end");
        std::thread::sleep(Duration::from_millis(20));
    }
}

// A main command that ignores the request is killed when the grace ends,
// and the result is still the termination request's.
// @kotowari[EX-222]
#[test]
fn a_main_command_killed_after_sigterm_still_ends_with_143() {
    let filtered = Filtered::new("\n[process]\nshutdown-grace-seconds = 1\n");
    let (mut child, _) = started(&filtered, &stubborn(0, None));
    sigterm(&child);
    assert_eq!(ended(&mut child).0, 143);
}

// The grace is 6 seconds from the first request. A second request 3 seconds
// in neither restarts it (it would end at 9) nor cuts it (at 3).
// @kotowari[REQ-103, EX-223, EX-224]
#[test]
fn a_repeated_sigterm_keeps_the_first_deadline() {
    let filtered = Filtered::new("\n[process]\nshutdown-grace-seconds = 6\n");
    let (mut child, _) = started(&filtered, &stubborn(1, None));
    sigterm(&child);
    let first = Instant::now();
    std::thread::sleep(Duration::from_secs(3));
    sigterm(&child);
    let (code, at) = ended(&mut child);
    let after = at - first;
    assert_eq!(code, 143);
    assert!(
        (Duration::from_millis(4500)..Duration::from_secs(8)).contains(&after),
        "{after:?}"
    );
}

// The main command has ended with its result, and the grace of 6 seconds
// runs; a SIGTERM a second in changes neither the result nor the deadline.
// @kotowari[REQ-104, EX-225, EX-226]
#[test]
fn a_sigterm_after_the_main_exit_keeps_the_result_and_the_deadline() {
    let filtered = Filtered::new("\n[process]\nshutdown-grace-seconds = 6\n");
    for result in [0, 7] {
        let (mut child, exited) = started(&filtered, &stubborn(1, Some(result)));
        std::thread::sleep(Duration::from_secs(1));
        sigterm(&child);
        let (code, at) = ended(&mut child);
        let after = at - exited;
        assert_eq!(code, result);
        assert!(
            (Duration::from_millis(4500)..Duration::from_millis(8500)).contains(&after),
            "{after:?}"
        );
    }
}

// The grace starts with the request; the main command ending 3 seconds later
// does not start another for the child left (it would end at 9).
// @kotowari[EX-227]
#[test]
fn a_main_exit_during_the_sigterm_grace_does_not_restart_it() {
    let filtered = Filtered::new("\n[process]\nshutdown-grace-seconds = 6\n");
    let script = "(trap '' TERM; while :; do sleep 0.1; done) &\ntrap 'sleep 3; exit 0' TERM\necho ready\nwhile :; do sleep 0.1; done\n";
    let (mut child, _) = started(&filtered, script);
    sigterm(&child);
    let first = Instant::now();
    let (code, at) = ended(&mut child);
    let after = at - first;
    assert_eq!(code, 143);
    assert!(
        (Duration::from_millis(4500)..Duration::from_secs(8)).contains(&after),
        "{after:?}"
    );
}

// Three processes left, no grace written: one common deadline of 5 seconds,
// not one per process (15).
// @kotowari[EX-117]
#[test]
fn three_remaining_processes_share_the_default_grace() {
    let filtered = Filtered::new("");
    let (mut child, exited) = started(&filtered, &stubborn(3, Some(0)));
    let (code, at) = ended(&mut child);
    let after = at - exited;
    assert_eq!(code, 0);
    assert!(
        (Duration::from_secs(4)..Duration::from_secs(9)).contains(&after),
        "{after:?}"
    );
}

// The written grace of 10 seconds applies, not the default 5.
// @kotowari[EX-207]
#[test]
fn a_written_grace_applies_in_filtered_mode() {
    let filtered = Filtered::new("\n[process]\nshutdown-grace-seconds = 10\n");
    let (mut child, exited) = started(&filtered, &stubborn(1, Some(0)));
    let (code, at) = ended(&mut child);
    let after = at - exited;
    assert_eq!(code, 0);
    assert!(
        (Duration::from_millis(8500)..Duration::from_secs(14)).contains(&after),
        "{after:?}"
    );
}

// The block cannot be confirmed when the termination request comes: the
// safety fault's 125 wins over the request's 143, with its cause.
// @kotowari[EX-316]
#[test]
fn a_safety_fault_during_sigterm_ends_with_125_and_its_cause() {
    let filtered = Filtered::new("\n[process]\nshutdown-grace-seconds = 300\n");
    let flag = filtered.bin.path().join("nft-broken");
    filtered.bin.write_executable(
        "nft",
        "#!/bin/sh\n[ -e \"${0%/*}/nft-broken\" ] && exit 1\nexec /usr/sbin/nft \"$@\"\n",
    );
    let mut child = filtered
        .with_pasta(&["/bin/sh", "-c", "echo ready; while :; do sleep 0.1; done"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    assert_eq!(lines.next().unwrap().unwrap(), "ready");
    std::fs::write(&flag, "").unwrap();
    sigterm(&child);
    let (code, _) = ended(&mut child);
    let mut stderr = String::new();
    std::io::Read::read_to_string(&mut child.stderr.take().unwrap(), &mut stderr).unwrap();
    assert_eq!(code, 125, "{stderr}");
    assert!(stderr.contains("kakoi: network unsafe: "), "{stderr}");
}

// A name that fails to resolve is not a safety fault: the main result stays.
// @kotowari[EX-317]
#[test]
fn a_dns_failure_alone_does_not_replace_the_main_result() {
    let filtered = Filtered::new(
        "\n[[network.allow]]\ndestination = { dns = 'app.example.com' }\nprotocol = 'tcp'\nports = ['443']\n",
    );
    let output = filtered
        .with_pasta(&[
            "/usr/bin/python3",
            "-c",
            "import socket\ntry:\n    socket.getaddrinfo('app.example.com', 443)\n    print('resolved')\nexcept OSError:\n    print('failed')\n",
        ])
        .output()
        .unwrap();
    assert_eq!(
        (
            output.status.code(),
            String::from_utf8_lossy(&output.stdout).into_owned()
        ),
        (Some(0), "failed\n".into()),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// host and none still hand the process over to bwrap, whatever the written
// grace: kakoi becomes bwrap, and the processes left end with the command.
// @kotowari[EX-329, EX-208, EX-209]
#[test]
fn host_and_none_keep_the_exec_contract_and_ignore_the_grace() {
    for mode in ["host", "none"] {
        let home = TempDir::new();
        let workspace = home.path().join("ws");
        std::fs::create_dir(&workspace).unwrap();
        home.write(
            ".config/kakoi/profile/default.toml",
            format!("{RW_WORKSPACE}\n[network]\nmode = '{mode}'\n\n[process]\nshutdown-grace-seconds = 10\n"),
        );
        let mut child = binary(home.path())
            .current_dir(&workspace)
            .args([
                "--",
                "/bin/sh",
                "-c",
                "(trap '' TERM; while :; do sleep 0.1; done) &\necho ready\nread line\nexit 0\n",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        assert_eq!(lines.next().unwrap().unwrap(), "ready");
        let exe = std::fs::read_link(format!("/proc/{}/exe", child.id())).unwrap();
        assert_eq!(exe.file_name().unwrap(), "bwrap", "{mode}");
        drop(child.stdin.take());
        let exited = Instant::now();
        let (code, at) = ended(&mut child);
        let mut stderr = String::new();
        std::io::Read::read_to_string(&mut child.stderr.take().unwrap(), &mut stderr).unwrap();
        assert_eq!(code, 0, "{mode}: {stderr}");
        // No grace: the child left ends with the command, well before 10 seconds.
        assert!(at - exited < Duration::from_secs(5), "{mode}");
        assert_eq!(
            stderr, "kakoi: warning: network/process settings are unused outside filtered mode\n",
            "{mode}"
        );
    }
}

const UNUSED: &str = "kakoi: warning: network/process settings are unused outside filtered mode\n";

/// Runs `sh -c script` under kakoi with the default profile `profile` and,
/// when given, a policy file holding `upper`.
fn launched(profile: &str, upper: Option<&str>, script: &str) -> std::process::Output {
    let home = TempDir::new();
    let workspace = home.path().join("ws");
    std::fs::create_dir(&workspace).unwrap();
    home.write(
        ".config/kakoi/profile/default.toml",
        format!("{RW_WORKSPACE}\n{profile}"),
    );
    let mut command = binary(home.path());
    command.current_dir(&workspace);
    if let Some(upper) = upper {
        command
            .arg("--policy-file")
            .arg(home.write("upper.toml", upper));
    }
    command
        .args(["--", "/bin/sh", "-c", script])
        .output()
        .unwrap()
}

// none written in a lower layer is still a written mode: a publication above
// it is only warned about.
// @kotowari[EX-170]
#[test]
fn an_inherited_none_ignores_a_publication_above_it() {
    let output = launched(
        "[network]\nmode = 'none'\n",
        Some("[[network.publish]]\nmode = 'fixed'\nprotocol = 'tcp'\nport = 8000\nhost-port = 18000\n"),
        "echo ran",
    );
    assert_eq!(
        (
            output.status.code(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned()
        ),
        (Some(0), "ran\n".into(), UNUSED.into())
    );
}

// A host-interface destination is refused while the policy is merged, before
// anything starts: the launch and the plan alike end with a policy diagnostic.
// @kotowari[REQ-428, EX-820, EX-822]
#[test]
fn filtered_refuses_a_host_interface_destination_as_a_policy_error() {
    let filtered = Filtered::new(
        "\n[[network.allow]]\ndestination = { ip = 'fe80::1', host-interface = 'eth0' }\nprotocol = 'tcp'\nports = ['443']\n",
    );
    for options in [&[][..], &["--print-plan"][..]] {
        let output = filtered
            .with_pasta_and_options(options, &["/bin/sh", "-c", "echo ran"])
            .output()
            .unwrap();
        assert_diagnostic(&output, 125, "policy");
    }
}

// An upstream error in one file is found while the file is read, and a mix of
// transports only once the layers are merged: both are policy errors, and
// nothing starts.
// @kotowari[REQ-426, EX-818, EX-819]
#[test]
fn upstream_errors_found_while_reading_or_merging_are_policy_errors() {
    let written = Filtered::new("tls-name = 'resolver.example.com'\n");
    let output = written
        .with_pasta(&["/bin/sh", "-c", "echo ran"])
        .output()
        .unwrap();
    assert_diagnostic(&output, 125, "policy");

    let merged = Filtered::new("");
    let upper = merged.home.write(
        "upper.toml",
        "[[network.dns-upstream]]\ntransport = 'tls'\nip = '192.0.2.53'\nport = 853\ntls-name = 'resolver.example.com'\n",
    );
    let output = merged
        .with_pasta_and_options(
            &["--policy-file", upper.to_str().unwrap()],
            &["/bin/sh", "-c", "echo ran"],
        )
        .output()
        .unwrap();
    assert_diagnostic(&output, 125, "policy");
}

// Unused settings are not looked into: an interface that does not exist is
// not checked in host mode, and a name is not asked about in none mode.
// @kotowari[EX-173, EX-174, EX-823]
#[test]
fn unused_settings_are_neither_checked_nor_resolved() {
    let output = launched(
        "[network]\nmode = 'host'\n\n[[network.allow]]\ndestination = { ip = 'fe80::1', host-interface = 'kakoi-absent0' }\nprotocol = 'tcp'\nports = ['443']\n",
        None,
        "echo ran",
    );
    assert_eq!(
        (output.status.code(), output.stderr.as_slice()),
        (Some(0), UNUSED.as_bytes()),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let upstream = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    upstream.set_nonblocking(true).unwrap();
    let output = launched(
        &format!(
            "[network]\nmode = 'none'\n\n[[network.dns-upstream]]\ntransport = 'plain'\nip = '127.0.0.1'\nport = {}\n\n[[network.allow]]\ndestination = {{ dns = 'app.example.com' }}\nprotocol = 'tcp'\nports = ['443']\n",
            upstream.local_addr().unwrap().port()
        ),
        None,
        "echo ran",
    );
    assert_eq!(
        (output.status.code(), output.stderr.as_slice()),
        (Some(0), UNUSED.as_bytes())
    );
    let mut bytes = [0; 512];
    assert!(upstream.recv_from(&mut bytes).is_err(), "a name was asked");
}

// Standard error is closed from the start: the notices are lost, and the
// command runs and ends as usual.
// @kotowari[EX-131]
#[test]
fn a_closed_standard_error_loses_notices_without_ending_the_command() {
    let filtered = Filtered::new("");
    let mut command = filtered.with_pasta(&["/bin/sh", "-c", "sleep 1; echo out"]);
    // SAFETY: only closes a descriptor between fork and exec.
    unsafe {
        std::os::unix::process::CommandExt::pre_exec(&mut command, || {
            libc::close(2);
            Ok(())
        });
    }
    let output = command.stderr(Stdio::null()).output().unwrap();
    assert_eq!(
        (output.status.code(), output.stdout.as_slice()),
        (Some(0), b"out\n".as_slice())
    );
}
