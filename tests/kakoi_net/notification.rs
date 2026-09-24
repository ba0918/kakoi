use kakoi_net::notification::{NetworkState, Notifications};

// @kotowari[REQ-067, EX-126, EX-127]
#[test]
fn network_notices_report_changes_without_repeating_the_same_failure() {
    let mut notices = Notifications::default();
    notices.update(NetworkState::Isolated, "forwarder stopped");
    notices.update(NetworkState::Isolated, "forwarder stopped");
    notices.update(NetworkState::Isolated, "firewall update failed");
    notices.update(NetworkState::Running, "restored");
    let lines: Vec<_> = std::iter::from_fn(|| notices.pop()).collect();
    assert_eq!(lines.len(), 3);
    assert!(lines[0].contains("forwarder stopped"));
    assert!(lines[1].contains("firewall update failed"));
    assert!(lines[2].contains("running: restored"));
    notices.update(NetworkState::Isolated, "forwarder stopped");
    assert!(notices.pop().unwrap().contains("forwarder stopped"));
}

// @kotowari[REQ-144]
#[test]
fn notification_overflow_keeps_bounds_and_prioritizes_loss_and_current_state() {
    let mut notices = Notifications::default();
    for index in 0..300 {
        notices.update(NetworkState::Isolated, &format!("failure {index}"));
        assert!(notices.pending() <= 256);
        assert!(notices.retained_bytes() <= 1024 * 1024);
    }
    notices.update(NetworkState::Running, "restored");
    let summary = notices.pop().unwrap();
    assert!(summary.contains("omitted"));
    assert!(summary.contains("running: restored"));
    assert!(
        notices.pop().is_none(),
        "old states must not follow the current-state summary"
    );
    notices.update(NetworkState::Isolated, "new failure");
    assert!(!notices.pop().unwrap().contains("omitted"));
}

// @kotowari[REQ-068, REQ-144]
#[test]
fn notifications_are_bounded_prefixed_single_utf8_lines_even_for_large_diagnostics() {
    let mut notices = Notifications::default();
    for index in 0..300 {
        notices.update(
            NetworkState::Unsafe,
            &format!("{index}:\n\r\x1b[31m{}", "あ".repeat(10000)),
        );
        assert!(notices.pending() <= 256);
        assert!(notices.retained_bytes() <= 1024 * 1024);
    }
    let line = notices.pop().unwrap();
    assert!(line.starts_with("kakoi: "));
    assert!(line.ends_with('\n'));
    assert_eq!(
        line.chars()
            .filter(|ch| ch.is_control())
            .collect::<Vec<_>>(),
        vec!['\n']
    );
    assert!(line.len() <= 4096);
    assert!(notices.retained_bytes() <= 4096);
}

// @kotowari[REQ-069, REQ-144, EX-321]
#[test]
fn a_full_output_pipe_does_not_block_notices_and_recovers_with_current_state() {
    use kakoi_net::notification::NotificationWriter;
    use std::{
        io::Read,
        os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd},
        time::{Duration, Instant},
    };
    let mut fds = [0; 2];
    assert_eq!(unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) }, 0);
    let mut read = std::fs::File::from(unsafe { OwnedFd::from_raw_fd(fds[0]) });
    let write = unsafe { OwnedFd::from_raw_fd(fds[1]) };
    let flags = unsafe { libc::fcntl(write.as_raw_fd(), libc::F_GETFL) };
    assert_eq!(
        unsafe { libc::fcntl(write.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) },
        0
    );
    let fill = [b'x'; 4096];
    while unsafe { libc::write(write.as_raw_fd(), fill.as_ptr().cast(), fill.len()) } > 0 {}
    assert_eq!(
        std::io::Error::last_os_error().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(
        unsafe { libc::fcntl(write.as_raw_fd(), libc::F_SETFL, flags) },
        0
    );
    {
        let mut blocked = NotificationWriter::new(write.as_fd()).unwrap();
        let pid = blocked.pid();
        let mut queue = Notifications::default();
        queue.update(NetworkState::Isolated, "blocked during shutdown");
        queue.flush_to(&mut blocked);
        std::thread::sleep(Duration::from_millis(20));
        let start = Instant::now();
        drop(blocked);
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    }
    let mut output = NotificationWriter::new(write.as_fd()).unwrap();
    let pid = output.pid();
    let mut notices = Notifications::default();
    notices.update(NetworkState::Isolated, "initial fault");
    notices.flush_to(&mut output);
    let start = Instant::now();
    for index in 0..1000 {
        notices.update(NetworkState::Isolated, &format!("failure {index}"));
        notices.flush_to(&mut output);
        assert!(notices.pending() <= 256);
        assert!(notices.retained_bytes() <= 1024 * 1024);
    }
    assert!(start.elapsed() < Duration::from_secs(1));
    assert_eq!(
        unsafe { libc::fcntl(write.as_raw_fd(), libc::F_GETFL) },
        flags,
        "application stderr flags changed"
    );
    notices.update(NetworkState::Running, "restored");
    assert_eq!(
        unsafe { libc::fcntl(read.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) },
        0
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut received = Vec::new();
    while !String::from_utf8_lossy(&received).contains("current running: restored\n") {
        notices.flush_to(&mut output);
        let mut bytes = [0; 8192];
        match read.read(&mut bytes) {
            Ok(count) => received.extend_from_slice(&bytes[..count]),
            Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock),
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(String::from_utf8(received)
        .unwrap()
        .contains("notifications omitted"));
    drop(output);
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
}

// @kotowari[REQ-069]
#[test]
fn closed_output_is_only_notification_loss_and_writer_can_be_reaped() {
    use kakoi_net::notification::NotificationWriter;
    use std::{
        os::fd::AsFd,
        os::unix::net::UnixStream,
        time::{Duration, Instant},
    };
    let (write, read) = UnixStream::pair().unwrap();
    drop(read);
    let mut output = NotificationWriter::new(write.as_fd()).unwrap();
    let pid = output.pid();
    let mut notices = Notifications::default();
    let start = Instant::now();
    for index in 0..300 {
        notices.update(NetworkState::Isolated, &format!("failure {index}"));
        notices.flush_to(&mut output);
    }
    drop(output);
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
}
