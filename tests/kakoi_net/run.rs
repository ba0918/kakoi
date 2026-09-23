use crate::{
    common::TempDir,
    session::{assert_transit, config, transport, transport_with_nft},
    supervisor::{filtered_plan, INIT},
};
use kakoi_net::{
    notification::NotificationWriter, run::run, session::Session, supervisor::Application,
};
use std::{
    io::{BufRead, BufReader, Read},
    os::{fd::AsFd, unix::net::UnixStream},
    path::Path,
    process::{ChildStdout, Stdio},
    sync::atomic::AtomicBool,
    thread::JoinHandle,
    time::{Duration, Instant},
};

fn start(
    directory: &TempDir,
    nft: &Path,
    script: &str,
    grace: Duration,
) -> (Session, Application, i32, TempDir) {
    let (transport, records) = if nft == Path::new("/usr/sbin/nft") {
        transport(directory)
    } else {
        transport_with_nft(directory, nft)
    };
    let mut session =
        Session::prepare(transport, config(nft.to_str().unwrap()), &[], |_| None).unwrap();
    session.activate().unwrap();
    let (home, _workspace, plan) = filtered_plan(script);
    let application = Application::spawn(
        &plan,
        session.namespace(),
        Path::new(INIT),
        grace,
        Stdio::piped(),
    )
    .unwrap();
    // The home holds the workspace, so it must outlive the application.
    (session, application, records[1].0, home)
}

/// Checks transit when the application reports a line, then returns its output.
fn observe(
    stdout: ChildStdout,
    keeper: i32,
    checks: &[(&'static str, bool)],
) -> JoinHandle<String> {
    let checks = checks.to_vec();
    std::thread::spawn(move || {
        let mut text = String::new();
        for line in BufReader::new(stdout).lines() {
            let line = line.unwrap();
            if let Some((_, open)) = checks.iter().find(|(name, _)| *name == line) {
                assert_transit(keeper, *open);
            }
            text.push_str(&line);
            text.push('\n');
        }
        text
    })
}

fn notifications() -> (NotificationWriter, JoinHandle<String>) {
    let (write, mut read) = UnixStream::pair().unwrap();
    let writer = NotificationWriter::new(write.as_fd()).unwrap();
    drop(write);
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = read.read_to_string(&mut text);
        text
    });
    (writer, reader)
}

const LEFTOVER: &str = "import signal, time\ndef term(*_):\n    print('term', flush=True)\n    time.sleep(1)\n    raise SystemExit(0)\nsignal.signal(signal.SIGTERM, term)\nwhile True:\n    time.sleep(60)";

// @kotowari[REQ-061, REQ-062, EX-115]
#[test]
fn main_exit_blocks_the_network_before_the_grace_and_returns_the_main_result() {
    let directory = TempDir::new();
    let script = format!(
        "import subprocess, sys, time\nsubprocess.Popen([sys.executable, '-c', {LEFTOVER:?}], start_new_session=True)\ntime.sleep(0.5)\nprint('ready', flush=True)\ntime.sleep(1.5)\nsys.exit(23)\n"
    );
    let (mut session, mut application, _, _home) = start(
        &directory,
        Path::new("/usr/sbin/nft"),
        &script,
        Duration::from_secs(5),
    );
    let keeper = transport_keeper(&directory);
    let output = observe(
        application.take_stdout().unwrap(),
        keeper,
        &[("ready", true), ("term", false)],
    );
    let (mut writer, notices) = notifications();
    let code = run(
        &mut session,
        &mut application,
        &mut writer,
        &AtomicBool::new(false),
    );
    assert_eq!(code, 23);
    assert_eq!(output.join().unwrap(), "ready\nterm\n");
    assert!(session.is_drained());
    drop(writer);
    assert!(notices
        .join()
        .unwrap()
        .contains("kakoi: network running: ready"));
}

// @kotowari[REQ-101, EX-219, EX-221]
#[test]
fn termination_request_blocks_the_network_then_ends_with_143() {
    let directory = TempDir::new();
    let script = "import signal, sys, time\ndef term(*_):\n    print('term', flush=True)\n    time.sleep(1)\n    sys.exit(0)\nsignal.signal(signal.SIGTERM, term)\nprint('ready', flush=True)\nwhile True:\n    time.sleep(60)\n";
    let (mut session, mut application, _, _home) = start(
        &directory,
        Path::new("/usr/sbin/nft"),
        script,
        Duration::from_secs(5),
    );
    let keeper = transport_keeper(&directory);
    let output = observe(
        application.take_stdout().unwrap(),
        keeper,
        &[("ready", true), ("term", false)],
    );
    let (mut writer, _notices) = notifications();
    let terminate = AtomicBool::new(false);
    let code = std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_secs(2));
            terminate.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        run(&mut session, &mut application, &mut writer, &terminate)
    });
    assert_eq!(code, 143);
    assert_eq!(output.join().unwrap(), "ready\nterm\n");
}

// @kotowari[REQ-064, REQ-065]
#[test]
fn unconfirmed_blocking_ends_every_process_at_once_with_125_and_its_reason() {
    use std::fs;
    let directory = TempDir::new();
    let trigger = directory.path().join("stall");
    let nft = directory.write_executable(
        "nft",
        format!(
            "#!/bin/sh\nif [ -e '{}' ] && [ \"$1\" = -f ]; then exec /bin/sleep 10; fi\nexec /usr/sbin/nft \"$@\"\n",
            trigger.display()
        ),
    );
    let script = "import signal, time\nsignal.signal(signal.SIGTERM, signal.SIG_IGN)\nprint('ready', flush=True)\ntime.sleep(60)\n";
    let (mut session, mut application, pasta, _home) =
        start(&directory, &nft, script, Duration::from_secs(300));
    let output = observe(application.take_stdout().unwrap(), 0, &[]);
    let (mut writer, notices) = notifications();
    let started = Instant::now();
    let code = std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_secs(1));
            fs::write(&trigger, "").unwrap();
            assert_eq!(unsafe { libc::kill(pasta, libc::SIGKILL) }, 0);
        });
        run(
            &mut session,
            &mut application,
            &mut writer,
            &AtomicBool::new(false),
        )
    });
    assert_eq!(code, 125);
    // Far below the 300 second grace: nothing waited for the application.
    assert!(started.elapsed() < Duration::from_secs(30));
    // End of output: no process of the isolation holds it any more.
    assert_eq!(output.join().unwrap(), "ready\n");
    drop(writer);
    assert!(notices.join().unwrap().contains("kakoi: network unsafe: "));
}

fn transport_keeper(directory: &TempDir) -> i32 {
    std::fs::read_to_string(directory.path().join("processes"))
        .unwrap()
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap()
        .parse()
        .unwrap()
}
