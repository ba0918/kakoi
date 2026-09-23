use crate::common::{home_with_workspace, TempDir, RW_WORKSPACE};
use kakoi_core::{
    layers::LayerSelection,
    plan::Plan,
    planning::{plan_for, Request},
};
use kakoi_net::{
    exit::exit_code,
    namespace::NetworkNamespace,
    supervisor::{Application, ApplicationEvent},
};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
    sync::mpsc,
    time::{Duration, Instant},
};

pub(crate) const INIT: &str = env!("CARGO_BIN_EXE_kakoi");
/// Waits that only guard against a hang; the properties are asserted separately.
const HANG: Duration = Duration::from_secs(30);

pub(crate) fn filtered_plan(script: &str) -> (TempDir, PathBuf, Plan) {
    let (home, workspace) = home_with_workspace();
    home.write(
        ".config/kakoi/profile/default.toml",
        format!("{RW_WORKSPACE}\n[network]\nmode='filtered'\n"),
    );
    let mut host = BTreeMap::new();
    host.insert("HOME".into(), home.path().as_os_str().to_owned());
    host.insert(
        "XDG_CONFIG_HOME".into(),
        home.path().join(".config").into_os_string(),
    );
    host.insert("PATH".into(), "/usr/bin:/bin".into());
    let plan = plan_for(&Request {
        layers: LayerSelection {
            profile: "default".into(),
            policy_file: None,
            rw: vec![],
            hide: vec![],
        },
        workspace: Some(workspace.clone()),
        command: vec!["python3".into(), "-c".into(), script.into()],
        current_dir: workspace.clone(),
        host,
    })
    .unwrap();
    (home, workspace, plan)
}

/// Standard output reaches end of file only when every process holding it is gone.
fn read_until_all_exit(mut stdout: std::process::ChildStdout) -> mpsc::Receiver<(String, Instant)> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        let _ = sender.send((text, Instant::now()));
    });
    receiver
}

fn spawn(plan: &Plan, grace: Duration) -> (Application, NetworkNamespace) {
    let namespace = NetworkNamespace::create().unwrap();
    let application =
        Application::spawn(plan, &namespace, Path::new(INIT), grace, Stdio::piped()).unwrap();
    (application, namespace)
}

fn next_event(application: &mut Application, limit: Duration) -> ApplicationEvent {
    let deadline = Instant::now() + limit;
    loop {
        if let Some(event) = application.poll().unwrap() {
            return event;
        }
        assert!(Instant::now() < deadline, "no application event");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn finish(application: &mut Application, limit: Duration) -> Option<i32> {
    loop {
        match next_event(application, limit) {
            ApplicationEvent::Finished(status) => return status.code(),
            ApplicationEvent::MainExited(_) | ApplicationEvent::GraceInterrupted => {}
        }
    }
}

const LEFTOVER: &str = r#"
import signal, subprocess, sys, time
def term(*_):
    time.sleep(1)
    subprocess.Popen([sys.executable, '-c', 'import signal, time; signal.signal(signal.SIGTERM, signal.SIG_IGN); print("late", flush=True); time.sleep(60)'])
signal.signal(signal.SIGTERM, term)
print('armed', flush=True)
while True:
    time.sleep(60)
"#;

// @kotowari[REQ-105, EX-114, EX-116, EX-228, EX-229]
#[test]
fn main_exit_waits_for_blocking_then_one_grace_covers_every_remaining_process() {
    let script = format!(
        "import subprocess, sys, time\nsubprocess.Popen([sys.executable, '-c', {LEFTOVER:?}], start_new_session=True)\ntime.sleep(0.5)\nsys.exit(23)\n"
    );
    let (_home, _workspace, plan) = filtered_plan(&script);
    let (mut application, _namespace) = spawn(&plan, Duration::from_secs(2));
    let output = read_until_all_exit(application.take_stdout().unwrap());
    // A grace request before the main command's result is not a blocking confirmation.
    application.begin_grace().unwrap();
    assert_eq!(
        next_event(&mut application, HANG),
        ApplicationEvent::MainExited(23)
    );
    // The supervisor withholds termination until the caller confirms blocking.
    std::thread::sleep(Duration::from_millis(700));
    assert!(application.poll().unwrap().is_none());
    assert!(output.try_recv().is_err(), "remaining process ended early");
    let started = Instant::now();
    application.begin_grace().unwrap();
    assert_eq!(finish(&mut application, HANG), Some(23));
    // Extending the 2 second grace for the child that appears after 1 second
    // would end it at 3 seconds.
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_millis(1900) && elapsed < Duration::from_millis(2700),
        "{elapsed:?}"
    );
    let (text, ended) = output.recv_timeout(HANG).unwrap();
    assert!(ended.duration_since(started) < Duration::from_millis(2800));
    assert_eq!(text, "armed\nlate\n");
}

// @kotowari[EX-118]
#[test]
fn grace_ends_as_soon_as_every_remaining_process_exits() {
    let script = "import subprocess, sys, time\nsubprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'], start_new_session=True)\nsys.exit(0)\n";
    let (_home, _workspace, plan) = filtered_plan(script);
    let (mut application, _namespace) = spawn(&plan, Duration::from_secs(300));
    assert_eq!(
        next_event(&mut application, HANG),
        ApplicationEvent::MainExited(0)
    );
    let started = Instant::now();
    application.begin_grace().unwrap();
    assert_eq!(finish(&mut application, HANG), Some(0));
    // Far below the 300 second grace it would otherwise use up.
    assert!(started.elapsed() < Duration::from_secs(30));
}

// @kotowari[EX-220]
#[test]
fn termination_request_reaches_running_processes_and_forces_them_after_grace() {
    let script = r#"
import signal, subprocess, sys, time
subprocess.Popen([sys.executable, '-c', 'import signal, sys, time\nsignal.signal(signal.SIGTERM, lambda *_: (print("cleanup", flush=True), sys.exit(0)))\nwhile True:\n    time.sleep(60)'], start_new_session=True)
signal.signal(signal.SIGTERM, signal.SIG_IGN)
time.sleep(0.3)
print('running', flush=True)
while True:
    time.sleep(60)
"#;
    let (_home, _workspace, plan) = filtered_plan(script);
    let (mut application, _namespace) = spawn(&plan, Duration::from_secs(1));
    let output = read_until_all_exit(application.take_stdout().unwrap());
    std::thread::sleep(Duration::from_millis(700));
    assert!(application.poll().unwrap().is_none());
    let started = Instant::now();
    application.terminate().unwrap();
    // The main command ignores the request, so only the grace's end stops it.
    assert_eq!(
        next_event(&mut application, HANG),
        ApplicationEvent::MainExited(137)
    );
    let elapsed = started.elapsed();
    assert!(elapsed >= Duration::from_millis(900), "{elapsed:?}");
    assert_eq!(finish(&mut application, HANG), Some(137));
    let (text, _) = output.recv_timeout(HANG).unwrap();
    assert_eq!(text, "running\ncleanup\n");
}

// @kotowari[EX-122]
#[test]
fn safety_kill_ends_every_process_without_the_grace() {
    let script = r#"
import signal, subprocess, sys, time
signal.signal(signal.SIGTERM, signal.SIG_IGN)
subprocess.Popen([sys.executable, '-c', 'import signal, time; signal.signal(signal.SIGTERM, signal.SIG_IGN); time.sleep(60)'], start_new_session=True)
print('running', flush=True)
time.sleep(60)
"#;
    let (_home, _workspace, plan) = filtered_plan(script);
    let (mut application, _namespace) = spawn(&plan, Duration::from_secs(300));
    let output = read_until_all_exit(application.take_stdout().unwrap());
    std::thread::sleep(Duration::from_millis(700));
    let started = Instant::now();
    application.kill();
    finish(&mut application, HANG);
    let (text, ended) = output.recv_timeout(HANG).unwrap();
    assert_eq!(text, "running\n");
    // Far below the 300 second grace.
    assert!(ended.duration_since(started) < Duration::from_secs(30));
}

// @kotowari[EX-328]
#[test]
fn supervisor_stays_as_process_one_without_exposing_its_channel_or_ignored_interrupt() {
    let script = r#"
import os, signal
assert os.getpid() != 1
with open('/proc/self/status') as status:
    fields = dict(line.rstrip().split(':', 1) for line in status if ':' in line)
assert int(fields['SigIgn'].strip(), 16) & (1 << (signal.SIGINT - 1)) == 0
for fd in os.listdir('/proc/self/fd'):
    try:
        target = os.readlink('/proc/self/fd/' + fd)
    except FileNotFoundError:
        continue
    # Standard streams are the caller's; the channel would be above them.
    assert int(fd) < 3 or 'socket:' not in target, target
try:
    os.listdir('/proc/1/fd')
except PermissionError:
    pass
else:
    raise AssertionError('supervisor descriptors are visible')
print('bounded', flush=True)
"#;
    let (_home, _workspace, plan) = filtered_plan(script);
    let (mut application, _namespace) = spawn(&plan, Duration::from_secs(1));
    let output = read_until_all_exit(application.take_stdout().unwrap());
    assert_eq!(
        next_event(&mut application, HANG),
        ApplicationEvent::MainExited(0)
    );
    application.begin_grace().unwrap();
    assert_eq!(finish(&mut application, HANG), Some(0));
    let (text, _) = output.recv_timeout(HANG).unwrap();
    assert_eq!(text, "bounded\n");
}

// @kotowari[REQ-142]
#[test]
fn exit_priority_is_safety_fault_then_external_termination_then_main_result() {
    for (fault, terminated, main, expected) in [
        (false, false, 0, 0),
        (false, false, 7, 7),
        (false, true, 0, 143),
        (false, true, 137, 143),
        (true, true, 0, 125),
        (true, false, 23, 125),
    ] {
        assert_eq!(exit_code(fault, terminated, main), expected);
    }
}
