use crate::health::assert_loopback;
use kakoi_net::{
    health::HealthGate,
    namespace::{NetworkNamespace, WatchdogState},
};
use std::{
    num::NonZeroU16,
    time::{Duration, Instant},
};

// @kotowari[REQ-058]
#[test]
fn independent_watchdog_latches_transit_closed_after_heartbeats_stop() {
    let app = NetworkNamespace::create().unwrap();
    let mut gate = HealthGate::create("/usr/sbin/nft").unwrap();
    gate.renew_for(NonZeroU16::new(10000).unwrap()).unwrap();
    let mut watchdog = gate.watchdog(Duration::from_millis(400)).unwrap();
    let pid = watchdog.pid();
    for _ in 0..6 {
        watchdog.heartbeat().unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(watchdog.poll().unwrap(), WatchdogState::Healthy);
    }
    assert_loopback(gate.namespace(), true);
    let deadline = Instant::now() + Duration::from_secs(2);
    while watchdog.poll().unwrap() != WatchdogState::Closed {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_loopback(gate.namespace(), false);
    gate.renew_for(NonZeroU16::new(10000).unwrap()).unwrap();
    assert_loopback(gate.namespace(), false);
    assert_loopback(&app, true);
    drop(watchdog);
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
}

// @kotowari[REQ-058]
#[test]
fn a_stopped_watchdog_is_reported_unresponsive_and_cannot_revive_on_resume() {
    let mut gate = HealthGate::create("/usr/sbin/nft").unwrap();
    let mut watchdog = gate.watchdog(Duration::from_millis(300)).unwrap();
    assert_eq!(unsafe { libc::kill(watchdog.pid(), libc::SIGSTOP) }, 0);
    std::thread::sleep(Duration::from_millis(350));
    assert_eq!(watchdog.poll().unwrap(), WatchdogState::Unresponsive);
    gate.close().unwrap();
    watchdog.heartbeat().unwrap();
    assert_eq!(unsafe { libc::kill(watchdog.pid(), libc::SIGCONT) }, 0);
    let deadline = Instant::now() + Duration::from_secs(2);
    while watchdog.poll().unwrap() != WatchdogState::Closed {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    gate.renew_for(NonZeroU16::new(1000).unwrap()).unwrap();
    assert_loopback(gate.namespace(), false);
}

// @kotowari[REQ-058]
#[test]
fn controller_process_loss_is_independently_blocked() {
    use std::{
        io::{BufRead, BufReader, Write},
        process::{Child, Command, Stdio},
    };
    const ROLE: &str = "KAKOI_TEST_WATCHDOG_CONTROLLER";
    if std::env::var_os(ROLE).is_some() {
        let mut gate = HealthGate::create("/usr/sbin/nft").unwrap();
        gate.renew_for(NonZeroU16::new(10000).unwrap()).unwrap();
        let mut watchdog = gate.watchdog(Duration::from_millis(500)).unwrap();
        println!("KAKOI_GUARD_READY {}", gate.namespace().keeper_pid());
        std::io::stdout().flush().unwrap();
        loop {
            match watchdog.poll().unwrap() {
                WatchdogState::Closed => break,
                WatchdogState::Healthy => watchdog.heartbeat().unwrap(),
                WatchdogState::Unresponsive => {}
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        return;
    }
    // A separate test process is stopped so the rest of the test runner and its
    // time-sensitive namespace tests continue normally.
    struct Fixture(Child);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for loss in [libc::SIGSTOP, libc::SIGKILL] {
        let mut child = Fixture(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "watchdog::controller_process_loss_is_independently_blocked",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(ROLE, "1")
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let mut reader = BufReader::new(child.0.stdout.take().unwrap());
        let keeper = loop {
            let mut line = String::new();
            assert!(
                reader.read_line(&mut line).unwrap() > 0,
                "controller failed before readiness"
            );
            if let Some((_, pid)) = line.split_once("KAKOI_GUARD_READY ") {
                break pid.trim().parse::<u32>().unwrap();
            }
        };
        use std::os::fd::AsRawFd;
        let user_namespace = std::fs::File::open(format!("/proc/{keeper}/ns/user")).unwrap();
        let net_namespace = std::fs::File::open(format!("/proc/{keeper}/ns/net")).unwrap();
        let observer = std::process::id();
        let enter = |program: &str| {
            let mut command = Command::new("/usr/bin/nsenter");
            command.args([
                "--preserve-credentials".into(),
                format!("--user=/proc/{observer}/fd/{}", user_namespace.as_raw_fd()),
                format!("--net=/proc/{observer}/fd/{}", net_namespace.as_raw_fd()),
                "--".into(),
                program.into(),
            ]);
            command
        };
        assert_eq!(unsafe { libc::kill(child.0.id() as i32, loss) }, 0);
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let output = enter("/usr/sbin/nft")
                .args(["list", "table", "inet", "kakoi_guard"])
                .output()
                .unwrap();
            if output.status.success() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "independent process did not close transit: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        crate::health::assert_loopback_command(enter("/usr/bin/python3"), false);
        if loss == libc::SIGSTOP {
            assert_eq!(unsafe { libc::kill(child.0.id() as i32, libc::SIGCONT) }, 0);
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                if loss == libc::SIGSTOP {
                    assert!(status.success());
                } else {
                    use std::os::unix::process::ExitStatusExt;
                    assert_eq!(status.signal(), Some(libc::SIGKILL));
                }
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

// @kotowari[REQ-058]
#[test]
fn watchdog_reports_a_failed_closure_instead_of_claiming_transit_is_closed() {
    let temp = crate::common::TempDir::new();
    let wrapper = temp.write_executable(
        "nft",
        r#"#!/bin/sh
if [ "$1" != -f ]; then exec /usr/sbin/nft "$@"; fi
script=$(cat)
case "$script" in
  *'table inet kakoi_guard'*) exit 23 ;;
  *) printf '%s' "$script" | /usr/sbin/nft -f - ;;
esac
"#,
    );
    let gate = HealthGate::create(&wrapper).unwrap();
    let mut watchdog = gate.watchdog(Duration::from_millis(100)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match watchdog.poll() {
            Err(error) => {
                assert!(error.to_string().contains("watchdog failed"));
                break;
            }
            Ok(state) => assert_ne!(state, WatchdogState::Closed),
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
}

// @kotowari[REQ-058]
#[test]
fn late_reopen_cannot_erase_a_watchdogs_independent_closure() {
    let mut gate = HealthGate::create("/usr/sbin/nft").unwrap();
    let mut watchdog = gate.watchdog(Duration::from_millis(100)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while watchdog.poll().unwrap() != WatchdogState::Closed {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    gate.reopen_after_validation(std::num::NonZeroU16::new(5000).unwrap())
        .unwrap();
    super::health::assert_loopback(gate.namespace(), false);
}
