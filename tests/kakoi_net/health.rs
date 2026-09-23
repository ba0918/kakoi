use kakoi_net::{health::HealthGate, namespace::NetworkNamespace};
use std::num::NonZeroU16;
use std::time::Duration;

pub(super) fn assert_loopback(namespace: &NetworkNamespace, expected: bool) {
    assert_loopback_command(namespace.command("/usr/bin/python3").unwrap(), expected);
}

pub(super) fn assert_loopback_command(mut command: std::process::Command, expected: bool) {
    let output = command
        .args([
            "-c",
            r#"
import fcntl, socket, struct
with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as control:
    fcntl.ioctl(control, 0x8914, struct.pack('16sH14x', b'lo', 1))
for af, address in [(socket.AF_INET, '127.0.0.1'), (socket.AF_INET6, '::1')]:
    for kind in [socket.SOCK_STREAM, socket.SOCK_DGRAM]:
        with socket.socket(af, kind) as server, socket.socket(af, kind) as client:
            server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            server.bind((address, 8080))
            server.settimeout(.1)
            client.settimeout(.1)
            if kind == socket.SOCK_STREAM:
                server.listen(1)
            try:
                client.connect(server.getsockname())
                client.send(b'packet')
                if kind == socket.SOCK_STREAM:
                    accepted, _ = server.accept()
                    with accepted:
                        accepted.settimeout(.1)
                        assert accepted.recv(64) == b'packet'
                else:
                    assert server.recv(64) == b'packet'
                print('yes')
            except (TimeoutError, PermissionError):
                print('no')
"#,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        actual,
        if expected {
            "yes\nyes\nyes\nyes\n"
        } else {
            "no\nno\nno\nno\n"
        }
    );
}

// @kotowari[REQ-057, REQ-058]
#[test]
fn transit_gate_starts_closed_expires_without_userspace_and_leaves_app_loopback_untouched() {
    let app = NetworkNamespace::create().unwrap();
    let mut gate = HealthGate::create("/usr/sbin/nft").unwrap();
    assert_loopback(gate.namespace(), false);
    assert_loopback(&app, true);
    gate.renew_for(NonZeroU16::new(1000).unwrap()).unwrap();
    assert_loopback(gate.namespace(), true);
    std::thread::sleep(Duration::from_millis(1200));
    assert_loopback(gate.namespace(), false);
    assert_loopback(&app, true);
    gate.renew_for(NonZeroU16::new(1000).unwrap()).unwrap();
    assert_loopback(gate.namespace(), true);
    gate.close().unwrap();
    assert_loopback(gate.namespace(), false);
    assert_loopback(&app, true);
    // An update already in flight when shutdown began must not reopen transit.
    gate.renew_for(NonZeroU16::new(1000).unwrap()).unwrap();
    assert_loopback(gate.namespace(), false);
    gate.close().unwrap();
    assert_loopback(gate.namespace(), false);
    gate.reopen_after_validation(NonZeroU16::new(1000).unwrap())
        .unwrap();
    assert_loopback(gate.namespace(), true);
    gate.close().unwrap();
    let pid = gate.namespace().keeper_pid();
    drop(gate);
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
}

// @kotowari[REQ-057]
#[test]
fn failed_firewall_installation_does_not_return_a_usable_gate() {
    assert!(HealthGate::create("/bin/false").is_err());
}

// @kotowari[REQ-057]
#[test]
fn stalled_firewall_command_is_killed_and_reaped() {
    use crate::common::TempDir;
    let directory = TempDir::new();
    let marker = directory.path().join("pid");
    let script = directory.write_executable(
        "nft",
        format!(
            "#!/bin/sh\necho $$ > '{}'\nexec /bin/sleep 30\n",
            marker.display()
        ),
    );
    let error = match HealthGate::create(script) {
        Ok(_) => panic!("stalled command returned a usable gate"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    let pid = std::fs::read_to_string(marker).unwrap();
    assert!(!std::path::Path::new(&format!("/proc/{}", pid.trim())).exists());
}

// @kotowari[REQ-057, REQ-058]
#[test]
fn subsecond_health_leases_are_rejected_before_the_active_gate_changes() {
    let mut gate = HealthGate::create("/usr/sbin/nft").unwrap();
    for milliseconds in [1, 10, 100, 999] {
        assert_eq!(
            gate.renew_for(NonZeroU16::new(milliseconds).unwrap())
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::InvalidInput
        );
    }
    assert_loopback(gate.namespace(), false);
}

// @kotowari[REQ-058]
#[test]
fn stalled_closure_obeys_the_controllers_deadline_and_reaps_its_command() {
    use crate::common::TempDir;
    use std::{io, time::Instant};
    let directory = TempDir::new();
    let marker = directory.path().join("closing-pid");
    let script = directory.write_executable(
        "nft",
        format!(
            r#"#!/usr/bin/python3
import os, subprocess, sys
rules = sys.stdin.read()
if 'kakoi_guard' in rules:
    with open({:?}, 'w') as out:
        out.write(str(os.getpid()))
    os.execl('/bin/sleep', 'sleep', '30')
sys.exit(subprocess.run(['/usr/sbin/nft', '-f', '-'], input=rules, text=True).returncode)
"#,
            marker.to_str().unwrap()
        ),
    );
    let mut gate = HealthGate::create(script).unwrap();
    let start = Instant::now();
    let deadline = start + Duration::from_millis(200);
    assert_eq!(
        gate.close_until(deadline).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    assert!(start.elapsed() < Duration::from_secs(1));
    let pid = std::fs::read_to_string(&marker).unwrap();
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    std::fs::remove_file(&marker).unwrap();
    // Reusing the expired overall deadline cannot buy another command budget.
    assert_eq!(
        gate.close_until(deadline).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    assert!(!marker.exists());
    assert_loopback(gate.namespace(), false);
}
