use crate::{common::TempDir, health::assert_loopback_command};
use kakoi_core::network::NetworkLimits;
use kakoi_net::{
    dns_runtime::DnsRuntimeConfig,
    scope::AddressContext,
    session::{Session, SessionState},
    transport::Transport,
};
use std::{
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

// The pasta substitute only supplies readiness and process failure. All namespaces,
// DNS sockets, nft rules and packet observations below use the real kernel.
pub(crate) fn transport(directory: &TempDir) -> (Transport, Vec<(i32, i32)>) {
    transport_with_nft(directory, Path::new("/usr/sbin/nft"))
}

pub(crate) fn transport_with_nft(directory: &TempDir, nft: &Path) -> (Transport, Vec<(i32, i32)>) {
    let records = directory.path().join("processes");
    let executable = directory.write_executable(
        "pasta",
        format!(
            r#"#!/usr/bin/python3
import os, sys, time
target = sys.argv[sys.argv.index('--netns') + 1].split('/')[2]
with open({:?}, 'a') as out:
    out.write(str(os.getpid()) + ' ' + target + '\n')
print(os.getpid(), flush=True)
time.sleep(60)
"#,
            records.to_str().unwrap()
        ),
    );
    let transport = Transport::start_closed(&executable, nft, &[], Duration::from_secs(2)).unwrap();
    let records = std::fs::read_to_string(records)
        .unwrap()
        .lines()
        .map(|line| {
            let mut fields = line.split_whitespace().map(|value| value.parse().unwrap());
            (fields.next().unwrap(), fields.next().unwrap())
        })
        .collect();
    (transport, records)
}

pub(crate) fn config(nft: &str) -> DnsRuntimeConfig {
    DnsRuntimeConfig {
        policy: vec![],
        upstreams: vec![serde_json::from_value(
            serde_json::json!({"transport":"plain", "ip":"127.0.0.1", "port":9}),
        )
        .unwrap()],
        limits: NetworkLimits::default(),
        trust: None,
        nft: nft.into(),
        scope: AddressContext::default(),
        generation: 0,
        host_dns: None,
    }
}

pub(crate) fn assert_transit(keeper: i32, expected: bool) {
    let mut command = Command::new("/usr/bin/nsenter");
    command.args([
        "--preserve-credentials",
        &format!("--user=/proc/{keeper}/ns/user"),
        &format!("--net=/proc/{keeper}/ns/net"),
        "--",
        "/usr/bin/python3",
    ]);
    assert_loopback_command(command, expected);
}

// @kotowari[REQ-057, REQ-058, REQ-150]
#[test]
fn session_opens_only_after_preparation_and_latches_closed_on_transport_failure() {
    for signal in [libc::SIGSTOP, libc::SIGKILL] {
        let directory = TempDir::new();
        let (transport, records) = transport(&directory);
        let mut session =
            Session::prepare(transport, config("/usr/sbin/nft"), &[], |_| None).unwrap();
        assert_eq!(session.state(), SessionState::Prepared);
        assert_transit(records[0].1, false);
        session.activate().unwrap();
        assert_eq!(session.state(), SessionState::Running);
        assert_transit(records[0].1, true);
        assert_eq!(unsafe { libc::kill(records[1].0, signal) }, 0);
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if session.poll().is_err() {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(session.state(), SessionState::Isolated);
        assert_transit(records[0].1, false);
        crate::health::assert_loopback(session.namespace(), true);
        assert!(
            session.activate().is_err(),
            "fault recovery must revalidate before opening"
        );
        assert_transit(records[0].1, false);
        session
            .close_until(Instant::now() + Duration::from_secs(1))
            .unwrap();
        while !session.is_drained() {
            let _ = session.poll();
            assert!(Instant::now() < deadline + Duration::from_secs(2));
        }
        drop(session);
        for (process, keeper) in records {
            for pid in [process, keeper] {
                assert!(!Path::new(&format!("/proc/{pid}")).exists());
            }
        }
    }
}

// @kotowari[REQ-057, REQ-150]
#[test]
fn failed_session_policy_preparation_reaps_transport_without_opening_it() {
    let directory = TempDir::new();
    let (transport, records) = transport(&directory);
    assert!(Session::prepare(transport, config("/bin/false"), &[], |_| None).is_err());
    for (process, keeper) in records {
        for pid in [process, keeper] {
            assert!(!Path::new(&format!("/proc/{pid}")).exists());
        }
    }
}

// @kotowari[REQ-116, REQ-131, REQ-058]
#[test]
fn session_services_dns_and_closes_before_draining_its_controllers() {
    use kakoi_core::network::{Allow, Destination, Protocol};
    use std::{net::UdpSocket, process::Stdio};
    let directory = TempDir::new();
    let (transport, records) = transport(&directory);
    let upstream = UdpSocket::bind("127.0.0.1:0").unwrap();
    upstream.set_nonblocking(true).unwrap();
    let mut dns = config("/usr/sbin/nft");
    dns.upstreams = vec![serde_json::from_value(serde_json::json!({
        "transport":"plain", "ip":"127.0.0.1", "port":upstream.local_addr().unwrap().port()
    }))
    .unwrap()];
    dns.policy.push(Allow {
        destination: Destination::Dns("api.example.com".parse().unwrap()),
        protocol: Protocol::Tcp,
        ports: vec!["443".into()].try_into().unwrap(),
    });
    let mut session = Session::prepare(transport, dns, &[], |_| None).unwrap();
    crate::health::assert_loopback(session.namespace(), true);
    session.activate().unwrap();
    let mut client = session.namespace().command("/usr/bin/python3").unwrap()
        .args(["-c", r#"
import socket
s=socket.socket(socket.AF_INET,socket.SOCK_DGRAM)
s.settimeout(2)
s.sendto(b'\x12\x34\x01\0\0\x01\0\0\0\0\0\0\x03api\x07example\x03com\0\0\x01\0\x01',('127.0.0.53',53))
answer=s.recv(65535)
assert answer[:2] == b'\x12\x34' and answer[3] & 15 == 0, answer
assert answer[-4:] == b'\x01\x01\x01\x01', answer
"#]).stderr(Stdio::piped()).spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut queries = 0;
    loop {
        session.poll().unwrap();
        let mut wire = [0; 512];
        match upstream.recv_from(&mut wire) {
            Ok((size, peer)) => {
                queries += 1;
                let mut answer = wire[..size].to_vec();
                answer[2] |= 0x80;
                answer[7] = 1;
                answer.extend([0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 30, 0, 4, 1, 1, 1, 1]);
                upstream.send_to(&answer, peer).unwrap();
            }
            Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock),
        }
        if let Some(status) = client.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(queries, 1);
    let rules = kakoi_net::nft::inspect(
        session.namespace(),
        Path::new("/usr/sbin/nft"),
        "kakoi_policy",
        Instant::now() + Duration::from_secs(1),
    )
    .unwrap();
    assert!(String::from_utf8(rules).unwrap().contains("1.1.1.1"));
    session
        .close_until(Instant::now() + Duration::from_secs(1))
        .unwrap();
    assert_eq!(session.state(), SessionState::Isolated);
    assert_transit(records[0].1, false);
    while !session.is_drained() {
        session.poll().unwrap();
        assert!(Instant::now() < deadline);
    }
    drop(session);
    for (process, keeper) in records {
        for pid in [process, keeper] {
            assert!(!Path::new(&format!("/proc/{pid}")).exists());
        }
    }
}

// @kotowari[REQ-058]
#[test]
fn unconfirmed_session_closure_requires_termination_instead_of_claiming_isolation() {
    let directory = TempDir::new();
    let (transport, _) = transport(&directory);
    let mut session = Session::prepare(transport, config("/usr/sbin/nft"), &[], |_| None).unwrap();
    session.activate().unwrap();
    let expired = Instant::now() - Duration::from_secs(1);
    assert_eq!(
        session.close_until(expired).unwrap_err().kind(),
        std::io::ErrorKind::TimedOut
    );
    assert_eq!(session.state(), SessionState::Unsafe);
    assert!(session.activate().is_err());
    session
        .close_until(Instant::now() + Duration::from_secs(1))
        .unwrap();
    assert_eq!(session.state(), SessionState::Isolated);
    let deadline = Instant::now() + Duration::from_secs(1);
    while !session.is_drained() {
        session.poll().unwrap();
        assert!(Instant::now() < deadline);
    }
}

// @kotowari[REQ-058, REQ-150]
#[test]
fn dropping_a_closed_session_does_not_restart_a_blocking_closure_command() {
    let directory = TempDir::new();
    let counter = directory.path().join("closures");
    let nft = directory.write_executable(
        "nft",
        format!(
            r#"#!/usr/bin/python3
import os, subprocess, sys
rules = sys.stdin.read()
if 'kakoi_guard' in rules:
    try:
        with open({:?}, 'x') as out:
            out.write('closed')
    except FileExistsError:
        os.execl('/bin/sleep', 'sleep', '30')
sys.exit(subprocess.run(['/usr/sbin/nft', '-f', '-'], input=rules, text=True).returncode)
"#,
            counter.to_str().unwrap()
        ),
    );
    let (transport, _) = transport_with_nft(&directory, &nft);
    let mut session =
        Session::prepare(transport, config(nft.to_str().unwrap()), &[], |_| None).unwrap();
    session.activate().unwrap();
    session
        .close_until(Instant::now() + Duration::from_secs(1))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while !session.is_drained() {
        session.poll().unwrap();
        assert!(Instant::now() < deadline);
    }
    let start = Instant::now();
    drop(session);
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "drop started another closure budget"
    );
}

// @kotowari[REQ-058, REQ-150]
#[test]
fn transport_replacement_preserves_application_namespace_and_reaps_old_forwarders() {
    let directory = TempDir::new();
    let (mut transport, records) = transport(&directory);
    let app = transport.controller_namespace().keeper_pid();
    let net = std::fs::read_link(format!("/proc/{app}/ns/net")).unwrap();
    transport
        .restart_closed_until(Instant::now() + Duration::from_secs(1))
        .unwrap();
    assert_eq!(transport.controller_namespace().keeper_pid(), app);
    assert_eq!(
        std::fs::read_link(format!("/proc/{app}/ns/net")).unwrap(),
        net
    );
    for (pid, _) in &records {
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }
    assert!(transport.is_running().unwrap());
    assert_transit(records[0].1, false);
    crate::health::assert_loopback(transport.controller_namespace(), true);
    let all_records = std::fs::read_to_string(directory.path().join("processes")).unwrap();
    assert_eq!(all_records.lines().count(), 4);
    drop(transport);
    for pid in all_records.split_whitespace() {
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }
}

// @kotowari[REQ-058, REQ-066, REQ-150]
#[test]
fn partial_transport_replacement_failure_is_reaped_and_retry_keeps_the_same_namespace() {
    let directory = TempDir::new();
    let (mut transport, original) = transport(&directory);
    let executable = directory.path().join("pasta");
    let script = std::fs::read_to_string(&executable).unwrap();
    crate::common::write_executable(
        &executable,
        script.replace(
            "print(os.getpid(), flush=True)",
            "if 'app0' in sys.argv:\n    sys.exit(17)\nprint(os.getpid(), flush=True)",
        ),
    );
    let app = transport.controller_namespace().keeper_pid();
    assert!(transport
        .restart_closed_until(Instant::now() + Duration::from_secs(1))
        .is_err());
    assert!(!transport.is_running().unwrap());
    assert_eq!(transport.controller_namespace().keeper_pid(), app);
    let failed = std::fs::read_to_string(directory.path().join("processes")).unwrap();
    assert_eq!(failed.lines().count(), 4);
    for line in failed.lines() {
        let process = line.split_whitespace().next().unwrap();
        assert!(!Path::new(&format!("/proc/{process}")).exists());
    }
    assert_transit(original[0].1, false);
    crate::health::assert_loopback(transport.controller_namespace(), true);
    crate::common::write_executable(&executable, script);
    transport
        .restart_closed_until(Instant::now() + Duration::from_secs(1))
        .unwrap();
    assert!(transport.is_running().unwrap());
    assert_eq!(transport.controller_namespace().keeper_pid(), app);
    assert_transit(original[0].1, false);
    let all = std::fs::read_to_string(directory.path().join("processes")).unwrap();
    assert_eq!(all.lines().count(), 6);
    drop(transport);
    for pid in all.split_whitespace() {
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }
}

// @kotowari[REQ-143, REQ-150]
#[test]
fn both_transport_stages_share_one_recovery_deadline() {
    let directory = TempDir::new();
    let (mut transport, original) = transport(&directory);
    let executable = directory.path().join("pasta");
    let script = std::fs::read_to_string(&executable).unwrap();
    crate::common::write_executable(
        &executable,
        script.replace(
            "print(os.getpid(), flush=True)",
            "time.sleep(0.4 if 'app0' not in sys.argv else 30)\nprint(os.getpid(), flush=True)",
        ),
    );
    let start = Instant::now();
    assert_eq!(
        transport
            .restart_closed_until(start + Duration::from_millis(600))
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::TimedOut
    );
    assert!(
        start.elapsed() < Duration::from_millis(850),
        "second stage restarted the timeout"
    );
    assert!(!transport.is_running().unwrap());
    assert_transit(original[0].1, false);
    let records = std::fs::read_to_string(directory.path().join("processes")).unwrap();
    assert_eq!(records.lines().count(), 4);
    for line in records.lines() {
        let pid = line.split_whitespace().next().unwrap();
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }
}

// @kotowari[REQ-058, REQ-066, REQ-067, REQ-143, REQ-150]
#[test]
fn session_automatically_rebuilds_original_policy_without_reviving_old_dns_permissions() {
    let directory = TempDir::new();
    let (transport, original) = transport(&directory);
    let mut session = Session::prepare(transport, config("/usr/sbin/nft"), &[], |_| None).unwrap();
    let app = session.namespace().keeper_pid();
    session.activate().unwrap();
    assert!(session
        .take_notification()
        .unwrap()
        .contains("running: ready"));
    kakoi_net::nft::apply(session.namespace(), Path::new("/usr/sbin/nft"),
        "add set inet kakoi_policy stale { type ipv4_addr; }\nadd element inet kakoi_policy stale { 1.1.1.1 }\n",
        Instant::now() + Duration::from_secs(1)).unwrap();
    assert_eq!(unsafe { libc::kill(original[1].0, libc::SIGKILL) }, 0);
    let deadline = Instant::now() + Duration::from_secs(3);
    while session.state() == SessionState::Running {
        let _ = session.poll();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(session.state(), SessionState::Isolated);
    assert_transit(original[0].1, false);
    while session.state() != SessionState::Running {
        let _ = session.poll();
        assert_ne!(session.state(), SessionState::Unsafe);
        assert!(
            Instant::now() < deadline,
            "session did not recover automatically"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(session.namespace().keeper_pid(), app);
    assert_transit(original[0].1, true);
    let notices: Vec<_> = std::iter::from_fn(|| session.take_notification()).collect();
    assert_eq!(notices.len(), 2);
    assert!(notices[0].contains("isolated:"));
    assert!(notices[1].contains("running: restored"));
    let rules = kakoi_net::nft::inspect(
        session.namespace(),
        Path::new("/usr/sbin/nft"),
        "kakoi_policy",
        Instant::now() + Duration::from_secs(1),
    )
    .unwrap();
    assert!(!String::from_utf8(rules).unwrap().contains("stale"));
    session
        .close_until(Instant::now() + Duration::from_secs(1))
        .unwrap();
    while !session.is_drained() {
        session.poll().unwrap();
        assert!(Instant::now() < deadline);
    }
    let records = std::fs::read_to_string(directory.path().join("processes")).unwrap();
    assert_eq!(records.lines().count(), 4);
    drop(session);
    for pid in records.split_whitespace() {
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }
}

// @kotowari[REQ-058, REQ-143, REQ-150]
#[test]
fn explicit_shutdown_cancels_a_stalled_recovery_and_never_reopens_transit() {
    let directory = TempDir::new();
    let (transport, original) = transport(&directory);
    let mut session = Session::prepare(transport, config("/usr/sbin/nft"), &[], |_| None).unwrap();
    session.activate().unwrap();
    let executable = directory.path().join("pasta");
    let script = std::fs::read_to_string(&executable).unwrap();
    crate::common::write_executable(
        &executable,
        script.replace(
            "print(os.getpid(), flush=True)",
            "time.sleep(30)\nprint(os.getpid(), flush=True)",
        ),
    );
    assert_eq!(unsafe { libc::kill(original[1].0, libc::SIGKILL) }, 0);
    let deadline = Instant::now() + Duration::from_secs(2);
    let blocked_pid = loop {
        let start = Instant::now();
        let _ = session.poll();
        assert!(
            start.elapsed() < Duration::from_millis(300),
            "recovery blocked supervisor"
        );
        let records = std::fs::read_to_string(directory.path().join("processes")).unwrap();
        if let Some(line) = records.lines().nth(2) {
            break line
                .split_whitespace()
                .next()
                .unwrap()
                .parse::<i32>()
                .unwrap();
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    };
    session
        .close_until(Instant::now() + Duration::from_secs(1))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while !session.is_drained() && Instant::now() < deadline {
        let _ = session.poll();
        std::thread::sleep(Duration::from_millis(2));
    }
    let cancelled_promptly = session.is_drained();
    // Clean up even on RED so a deliberately stalled test process cannot leak.
    if !cancelled_promptly {
        unsafe {
            libc::kill(blocked_pid, libc::SIGKILL);
        }
        let cleanup = Instant::now() + Duration::from_secs(2);
        while !session.is_drained() && Instant::now() < cleanup {
            let _ = session.poll();
        }
    }
    assert_transit(original[0].1, false);
    assert_eq!(session.state(), SessionState::Isolated);
    assert!(
        cancelled_promptly,
        "shutdown waited for the recovery-attempt timeout"
    );
}

// @kotowari[REQ-058, REQ-143, REQ-150]
#[test]
fn shutdown_cancels_a_stalled_policy_rebuild_without_waiting_for_nft_timeout() {
    let directory = TempDir::new();
    let marker = directory.path().join("blocked-nft");
    let nft = directory.write_executable(
        "nft",
        format!(
            r#"#!/usr/bin/python3
import os, subprocess, sys
rules = sys.stdin.read()
if 'delete table inet kakoi_policy' in rules:
    # Renamed into place, so that the marker never appears without the PID.
    with open({marker:?} + '.partial', 'w') as out:
        out.write(str(os.getpid()))
    os.rename({marker:?} + '.partial', {marker:?})
    os.execl('/bin/sleep', 'sleep', '30')
sys.exit(subprocess.run(['/usr/sbin/nft', '-f', '-'], input=rules, text=True).returncode)
"#,
            marker = marker.to_str().unwrap()
        ),
    );
    let (transport, original) = transport_with_nft(&directory, &nft);
    let mut session =
        Session::prepare(transport, config(nft.to_str().unwrap()), &[], |_| None).unwrap();
    session.activate().unwrap();
    assert_eq!(unsafe { libc::kill(original[1].0, libc::SIGKILL) }, 0);
    let deadline = Instant::now() + Duration::from_secs(2);
    while !marker.exists() {
        let _ = session.poll();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    let pid: i32 = std::fs::read_to_string(marker).unwrap().parse().unwrap();
    session
        .close_until(Instant::now() + Duration::from_secs(1))
        .unwrap();
    let deadline = Instant::now() + Duration::from_millis(500);
    while !session.is_drained() && Instant::now() < deadline {
        let _ = session.poll();
        std::thread::sleep(Duration::from_millis(2));
    }
    let timely = session.is_drained();
    if !timely {
        unsafe {
            libc::kill(pid, libc::SIGKILL);
        }
        let cleanup = Instant::now() + Duration::from_secs(2);
        while !session.is_drained() && Instant::now() < cleanup {
            let _ = session.poll();
        }
    }
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    assert_transit(original[0].1, false);
    assert!(timely, "cancelled rebuild waited for nft timeout");
}

// @kotowari[REQ-058, REQ-066, REQ-067, REQ-143, EX-123]
#[test]
fn automatic_recovery_waits_after_failure_and_retries_without_user_input() {
    let directory = TempDir::new();
    let (transport, original) = transport(&directory);
    let executable = directory.path().join("pasta");
    let script = std::fs::read_to_string(&executable).unwrap();
    let mut session = Session::prepare(transport, config("/usr/sbin/nft"), &[], |_| None).unwrap();
    session.activate().unwrap();
    crate::common::write_executable(
        &executable,
        script.replace(
            "print(os.getpid(), flush=True)",
            "if 'app0' in sys.argv:\n    sys.exit(17)\nprint(os.getpid(), flush=True)",
        ),
    );
    assert_eq!(unsafe { libc::kill(original[1].0, libc::SIGKILL) }, 0);
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let result = session.poll();
        let records = std::fs::read_to_string(directory.path().join("processes")).unwrap();
        if records.lines().count() == 4 && result.is_err() {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(session.state(), SessionState::Isolated);
    crate::common::write_executable(&executable, script);
    let waiting = Instant::now() + Duration::from_millis(900);
    while Instant::now() < waiting {
        session.poll().unwrap();
        assert_eq!(
            std::fs::read_to_string(directory.path().join("processes"))
                .unwrap()
                .lines()
                .count(),
            4
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    // Still blocked while waiting for the next attempt.
    assert_transit(original[0].1, false);
    while session.state() != SessionState::Running {
        session.poll().unwrap();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_transit(original[0].1, true);
    let notices: Vec<_> = std::iter::from_fn(|| session.take_notification()).collect();
    assert_eq!(notices.len(), 4);
    assert!(notices[1].contains("network controller stopped"));
    assert!(
        notices[2].contains("pasta exited during startup"),
        "{notices:?}"
    );
    assert!(notices[3].contains("running: restored"));
    session
        .close_until(Instant::now() + Duration::from_secs(1))
        .unwrap();
    while !session.is_drained() {
        session.poll().unwrap();
        assert!(Instant::now() < deadline);
    }
    let records = std::fs::read_to_string(directory.path().join("processes")).unwrap();
    assert_eq!(records.lines().count(), 6);
    drop(session);
    for pid in records.split_whitespace() {
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }
}

// @kotowari[REQ-058, REQ-066]
#[test]
fn recovery_removes_only_a_retired_watchdogs_guard_before_opening() {
    let directory = TempDir::new();
    let (transport, records) = transport(&directory);
    let mut session = Session::prepare(transport, config("/usr/sbin/nft"), &[], |_| None).unwrap();
    session.activate().unwrap();
    // Simulate a controller pause long enough for the independent monitor to close.
    std::thread::sleep(Duration::from_millis(3200));
    assert!(session.poll().is_err());
    assert_eq!(session.state(), SessionState::Isolated);
    assert_transit(records[0].1, false);
    let deadline = Instant::now() + Duration::from_secs(2);
    while session.state() != SessionState::Running {
        session.poll().unwrap();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_transit(records[0].1, true);
    session
        .close_until(Instant::now() + Duration::from_secs(1))
        .unwrap();
    while !session.is_drained() {
        session.poll().unwrap();
        assert!(Instant::now() < deadline);
    }
}

// @kotowari[REQ-058, REQ-066, REQ-069]
#[test]
fn blocked_notification_output_does_not_delay_isolation_recovery_or_shutdown() {
    use kakoi_net::notification::NotificationWriter;
    use std::{
        os::fd::{AsFd, AsRawFd},
        os::unix::net::UnixStream,
    };
    let (write, _read) = UnixStream::pair().unwrap();
    let fill = [b'x'; 4096];
    while unsafe {
        libc::send(
            write.as_raw_fd(),
            fill.as_ptr().cast(),
            fill.len(),
            libc::MSG_DONTWAIT | libc::MSG_NOSIGNAL,
        )
    } > 0
    {}
    assert_eq!(
        std::io::Error::last_os_error().kind(),
        std::io::ErrorKind::WouldBlock
    );
    let mut output = NotificationWriter::new(write.as_fd()).unwrap();
    let directory = TempDir::new();
    let (transport, original) = transport(&directory);
    let mut session = Session::prepare(transport, config("/usr/sbin/nft"), &[], |_| None).unwrap();
    session.activate().unwrap();
    session.flush_notifications(&mut output);
    assert_eq!(unsafe { libc::kill(original[1].0, libc::SIGKILL) }, 0);
    let deadline = Instant::now() + Duration::from_secs(3);
    while session.state() == SessionState::Running {
        let _ = session.poll();
        session.flush_notifications(&mut output);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(session.state(), SessionState::Isolated);
    assert_transit(original[0].1, false);
    while session.state() != SessionState::Running {
        session.poll().unwrap();
        session.flush_notifications(&mut output);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_transit(original[0].1, true);
    session
        .close_until(Instant::now() + Duration::from_secs(1))
        .unwrap();
    while !session.is_drained() {
        session.poll().unwrap();
        session.flush_notifications(&mut output);
        assert!(Instant::now() < deadline);
    }
    let start = Instant::now();
    drop(session);
    drop(output);
    assert!(start.elapsed() < Duration::from_secs(1));
}

// The rebuild stalls: the transit stays blocked while it is pending, and the
// attempt is cut at the default 10 seconds, not before, still blocked.
// @kotowari[EX-110, EX-318]
#[test]
fn a_stalled_rebuild_stays_blocked_and_is_cut_at_the_default_attempt_timeout() {
    let directory = TempDir::new();
    let (transport, original) = transport(&directory);
    let mut session = Session::prepare(transport, config("/usr/sbin/nft"), &[], |_| None).unwrap();
    session.activate().unwrap();
    let executable = directory.path().join("pasta");
    let script = std::fs::read_to_string(&executable).unwrap();
    crate::common::write_executable(
        &executable,
        script.replace(
            "print(os.getpid(), flush=True)",
            "time.sleep(60)\nprint(os.getpid(), flush=True)",
        ),
    );
    assert_eq!(unsafe { libc::kill(original[1].0, libc::SIGKILL) }, 0);
    // Only guards against a hang.
    let deadline = Instant::now() + Duration::from_secs(30);
    let (stalled, started) = loop {
        let _ = session.poll();
        let records = std::fs::read_to_string(directory.path().join("processes")).unwrap();
        if let Some(line) = records.lines().nth(2) {
            let pid: i32 = line.split_whitespace().next().unwrap().parse().unwrap();
            break (pid, Instant::now());
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    };
    assert_eq!(session.state(), SessionState::Isolated);
    assert_transit(original[0].1, false);
    while Path::new(&format!("/proc/{stalled}")).exists() {
        let _ = session.poll();
        assert!(
            Instant::now() < deadline,
            "the stalled attempt was never cut"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let cut = started.elapsed();
    assert!(cut >= Duration::from_millis(9500), "{cut:?}");
    assert_eq!(session.state(), SessionState::Isolated);
    assert_transit(original[0].1, false);
    session
        .close_until(Instant::now() + Duration::from_secs(1))
        .unwrap();
}
