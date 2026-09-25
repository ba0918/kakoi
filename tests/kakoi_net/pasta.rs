use crate::common::TempDir;
use kakoi_net::{
    namespace::NetworkNamespace,
    pasta::{Pasta, PastaStage},
};
use std::{sync::Arc, time::Duration};

fn executable(home: &TempDir, script: &str) -> std::path::PathBuf {
    home.write_executable("pasta", script)
}

// @kotowari[REQ-057, REQ-150]
#[test]
fn pasta_readiness_owns_the_process_and_target_until_drop() {
    let directory = TempDir::new();
    let executable = executable(&directory, "#!/bin/sh\necho $$\nexec /bin/sleep 30\n");
    let target = Arc::new(NetworkNamespace::create().unwrap());
    let keeper = target.keeper_pid();
    let mut pasta = Pasta::start(
        &executable,
        None,
        target.clone(),
        PastaStage::Outer,
        &[],
        Duration::from_secs(1),
    )
    .unwrap();
    let pid = pasta.pid();
    drop(target);
    assert!(pasta.is_running().unwrap());
    assert!(std::path::Path::new(&format!("/proc/{keeper}")).exists());
    drop(pasta);
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    assert!(!std::path::Path::new(&format!("/proc/{keeper}")).exists());
}

// @kotowari[REQ-057]
#[test]
fn missing_or_wrong_readiness_fails_and_reaps_the_process() {
    for response in ["", "echo 1\n"] {
        let directory = TempDir::new();
        let marker = directory.path().join("pid");
        let executable = executable(
            &directory,
            &format!(
                "#!/bin/sh\necho $$ > '{}'\n{response}exec /bin/sleep 30\n",
                marker.display()
            ),
        );
        let target = Arc::new(NetworkNamespace::create().unwrap());
        assert!(Pasta::start(
            &executable,
            None,
            target,
            PastaStage::Outer,
            &[],
            Duration::from_millis(100)
        )
        .is_err());
        let pid = std::fs::read_to_string(marker).unwrap();
        assert!(!std::path::Path::new(&format!("/proc/{}", pid.trim())).exists());
    }
}

// @kotowari[REQ-057, REQ-150]
#[test]
fn second_pasta_failure_reaps_the_first_and_both_namespace_keepers() {
    use kakoi_net::transport::Transport;
    let directory = TempDir::new();
    let records = directory.path().join("processes");
    let executable = executable(
        &directory,
        &format!(
            r#"#!/usr/bin/python3
import os, sys, time
target = sys.argv[sys.argv.index('--netns') + 1].split('/')[2]
with open({:?}, 'a') as out:
    out.write(str(os.getpid()) + ' ' + target + '\n')
if 'app0' in sys.argv:
    sys.exit(17)
print(os.getpid(), flush=True)
time.sleep(30)
"#,
            records.to_str().unwrap()
        ),
    );
    assert!(Transport::start_closed(
        &executable,
        std::path::Path::new("/usr/sbin/nft"),
        &[],
        Duration::from_secs(1)
    )
    .is_err());
    let records = std::fs::read_to_string(records).unwrap();
    assert_eq!(records.lines().count(), 2);
    for pid in records.split_whitespace() {
        assert!(
            !std::path::Path::new(&format!("/proc/{pid}")).exists(),
            "leaked {pid}"
        );
    }
}

// @kotowari[REQ-057]
#[test]
fn startup_failure_preserves_bounded_diagnostics_without_blocking_on_a_full_pipe() {
    let directory = TempDir::new();
    let executable = executable(&directory, "#!/usr/bin/python3\nimport sys\nsys.stderr.write('x' * 100000 + 'TUN unavailable\\n')\nsys.exit(1)\n");
    let target = Arc::new(NetworkNamespace::create().unwrap());
    let error = match Pasta::start(
        &executable,
        None,
        target,
        PastaStage::Outer,
        &[],
        Duration::from_secs(2),
    ) {
        Ok(_) => panic!("failed program returned readiness"),
        Err(error) => error.to_string(),
    };
    assert!(error.contains("TUN unavailable"), "{error}");
    assert!(error.len() < 10000);
}

// @kotowari[REQ-058, REQ-150]
#[test]
fn stopped_pasta_is_not_healthy_and_is_reaped_on_drop() {
    let directory = TempDir::new();
    let executable = executable(&directory, "#!/bin/sh\necho $$\nexec /bin/sleep 30\n");
    let target = Arc::new(NetworkNamespace::create().unwrap());
    let mut pasta = Pasta::start(
        &executable,
        None,
        target,
        PastaStage::Outer,
        &[],
        Duration::from_secs(1),
    )
    .unwrap();
    let pid = pasta.pid();
    assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGSTOP) }, 0);
    let deadline = std::time::Instant::now() + Duration::from_millis(500);
    while pasta.is_running().unwrap() {
        assert!(
            std::time::Instant::now() < deadline,
            "stopped pasta was reported healthy"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        !pasta.is_running().unwrap(),
        "observing the stop must not consume it"
    );
    assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGCONT) }, 0);
    while !pasta.is_running().unwrap() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    drop(pasta);
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
}

// The usage text of the verified pasta, saved from its `--help`.
const VERIFIED_HELP: &str =
    include_str!("pasta_help/debian-trixie-backports-passt-0.0-git20260728.f8df3f1-1-bpo13+1.txt");
// The usage text of Ubuntu 24.04's pasta, saved from its `--help`. It is saved
// rather than run because the machines running the tests need not have it.
const UBUNTU_24_04_HELP: &str =
    include_str!("pasta_help/ubuntu-24.04-passt-0.0-git20240220.1e6f92b-1.txt");

// @kotowari[REQ-430]
#[test]
fn the_verified_pasta_lists_every_long_option_kakoi_passes() {
    assert_eq!(
        kakoi_net::pasta::missing_long_options(VERIFIED_HELP.as_bytes()),
        Vec::<&str>::new()
    );
}

// @kotowari[REQ-430]
#[test]
fn ubuntu_24_04_pasta_lacks_exactly_the_two_host_loopback_options() {
    assert_eq!(
        kakoi_net::pasta::missing_long_options(UBUNTU_24_04_HELP.as_bytes()),
        vec!["--host-lo-to-ns-lo", "--map-host-loopback"]
    );
}
