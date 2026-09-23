use crate::common::{binary, TempDir, RW_WORKSPACE};
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

    fn command(&self, path: &str, arguments: &[&str]) -> Command {
        let mut command = binary(self.home.path());
        command
            .env("PATH", path)
            .current_dir(&self.workspace)
            .arg("--")
            .args(arguments);
        command
    }

    fn with_pasta(&self, arguments: &[&str]) -> Command {
        self.command(
            &format!("{}:/usr/sbin:/usr/bin:/bin", self.bin.path().display()),
            arguments,
        )
    }
}

// @kotowari[REQ-068, REQ-148]
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

// @kotowari[REQ-057]
#[test]
fn a_missing_pasta_ends_the_start_before_the_application_runs() {
    let filtered = Filtered::new("");
    let marker = filtered.workspace.join("ran");
    // Only bwrap and nft are reachable, whatever else this machine has installed.
    let tools = TempDir::new();
    let bwrap = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join("bwrap"))
        .find(|candidate| candidate.is_file())
        .unwrap();
    std::os::unix::fs::symlink(bwrap, tools.path().join("bwrap")).unwrap();
    std::os::unix::fs::symlink("/usr/sbin/nft", tools.path().join("nft")).unwrap();
    let output = filtered
        .command(
            tools.path().to_str().unwrap(),
            &["/bin/sh", "-c", &format!("touch '{}'", marker.display())],
        )
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(125), "{stderr}");
    assert!(
        stderr.contains("kakoi: bwrap: ") && stderr.contains("pasta"),
        "{stderr}"
    );
    assert!(!marker.exists());
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

// @kotowari[EX-216, EX-217]
#[test]
fn ctrl_c_during_the_grace_ends_the_remaining_processes_and_keeps_the_result() {
    let filtered = Filtered::new("\n[process]\nshutdown-grace-seconds = 300\n");
    let (code, output, after) = on_terminal(
        filtered.with_pasta(&[
            "/usr/bin/python3",
            "-c",
            // The remaining child reports the grace's termination request and stays.
            "import subprocess, sys, time\nsubprocess.Popen([sys.executable, '-c', 'import signal, time; signal.signal(signal.SIGTERM, lambda *_: print(\"term-received\", flush=True)); print(\"child-ready\", flush=True); time.sleep(600)'], start_new_session=True)\ntime.sleep(0.5)",
        ]),
        "term-received",
        0.0,
    );
    assert_eq!(code, 0, "{output}");
    // Far below the 300 second grace.
    assert!(after < HANG, "{after:?}");
    assert!(output.contains("kakoi: grace interrupted"), "{output}");
}
