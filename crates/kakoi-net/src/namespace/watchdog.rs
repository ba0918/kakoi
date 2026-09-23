use super::{duplicate_above_stdio, pidfd, reap, NetworkNamespace};
use crate::health::CLOSE_RULES;
use std::{
    ffi::CString,
    fs::File,
    io::{self, Seek, SeekFrom, Write},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::{ffi::OsStrExt, net::UnixDatagram},
    },
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchdogState {
    Healthy,
    Unresponsive,
    Closed,
}

pub struct TransitWatchdog {
    pid: libc::pid_t,
    pidfd: OwnedFd,
    control: UnixDatagram,
    last_ack: u64,
    timeout: u64,
    status: Option<i32>,
    guard_table: String,
    _namespace: Arc<NetworkNamespace>,
}

impl TransitWatchdog {
    pub(crate) fn start(
        namespace: Arc<NetworkNamespace>,
        nft: &Path,
        timeout: Duration,
    ) -> io::Result<Self> {
        let timeout = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
        if !(100..=60000).contains(&timeout) || !nft.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "watchdog needs 100..60000ms and an absolute nft path",
            ));
        }
        let executable = CString::new(nft.as_os_str().as_bytes()).map_err(io::Error::other)?;
        let raw = unsafe { libc::memfd_create(c"kakoi-guard".as_ptr(), libc::MFD_CLOEXEC) };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut script = unsafe { File::from_raw_fd(raw) };
        static NEXT_GUARD: AtomicU64 = AtomicU64::new(0);
        let generation = NEXT_GUARD
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| io::Error::other("watchdog generation exhausted"))?;
        let guard_table = format!(
            "kakoi_watchdog_{}_{}",
            unsafe { libc::getpid() },
            generation
        );
        script.write_all(CLOSE_RULES.as_bytes())?;
        // Reopening the common guard cannot erase a concurrent watchdog close.
        script.write_all(
            CLOSE_RULES
                .replacen("kakoi_guard", &guard_table, 1)
                .as_bytes(),
        )?;
        script.seek(SeekFrom::Start(0))?;
        let null = File::options().read(true).write(true).open("/dev/null")?;
        let (parent, child) = UnixDatagram::pair()?;
        parent.set_read_timeout(Some(Duration::from_secs(2)))?;
        child.set_nonblocking(true)?;
        let parent_pidfd = pidfd(unsafe { libc::getpid() })?;
        let sources = [
            script.as_raw_fd(),
            null.as_raw_fd(),
            child.as_raw_fd(),
            parent_pidfd.as_raw_fd(),
            namespace.user.as_raw_fd(),
            namespace.net.as_raw_fd(),
        ];
        let descriptors: Vec<_> = sources
            .into_iter()
            .map(duplicate_above_stdio)
            .collect::<io::Result<_>>()?;
        let sent = monotonic_millis()?;
        parent.send(&sent.to_ne_bytes())?;
        // After fork the child uses raw syscalls/stack data only, never Rust
        // allocation, locks, unwinding or inherited destructors.
        let pid = unsafe { libc::fork() };
        if pid < 0 {
            return Err(io::Error::last_os_error());
        }
        if pid == 0 {
            unsafe { monitor(&descriptors, executable.as_ptr(), timeout) }
        }
        drop(child);
        drop(descriptors);
        let child_pidfd = match pidfd(pid) {
            Ok(fd) => fd,
            Err(error) => {
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }
                reap(pid);
                return Err(error);
            }
        };
        let watchdog = Self {
            pid,
            pidfd: child_pidfd,
            control: parent,
            last_ack: sent,
            timeout,
            status: None,
            guard_table,
            _namespace: namespace,
        };
        let mut ack = [0; 8];
        if watchdog.control.recv(&mut ack)? != 8 || u64::from_ne_bytes(ack) != sent {
            return Err(io::Error::other(
                "invalid watchdog readiness acknowledgement",
            ));
        }
        watchdog.control.set_nonblocking(true)?;
        Ok(watchdog)
    }

    pub fn pid(&self) -> libc::pid_t {
        self.pid
    }

    /// Drop kills/reaps this monitor before its private guard can be removed.
    /// The caller must already hold the common transit guard closed.
    pub(crate) fn retire(self) -> String {
        self.guard_table.clone()
    }

    /// Only the trusted controller owns this socket. Absolute timestamps prevent
    /// a backlog of old heartbeats from extending the monitor's lease.
    pub fn heartbeat(&mut self) -> io::Result<()> {
        self.control.send(&monotonic_millis()?.to_ne_bytes())?;
        Ok(())
    }

    pub fn poll(&mut self) -> io::Result<WatchdogState> {
        if self.status.is_none() {
            let mut status = 0;
            let result = unsafe { libc::waitpid(self.pid, &mut status, libc::WNOHANG) };
            if result < 0 {
                return Err(io::Error::last_os_error());
            }
            if result == self.pid {
                self.status = Some(status);
            }
        }
        if let Some(status) = self.status {
            return if libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0 {
                Ok(WatchdogState::Closed)
            } else {
                Err(io::Error::other(format!(
                    "transit watchdog failed (wait status {status})"
                )))
            };
        }
        let now = monotonic_millis()?;
        for _ in 0..64 {
            let mut bytes = [0; 9];
            match self.control.recv(&mut bytes) {
                Ok(8) => {
                    let stamp = u64::from_ne_bytes(bytes[..8].try_into().expect("eight bytes"));
                    if stamp > now {
                        return Err(io::Error::other("invalid watchdog timestamp"));
                    }
                    self.last_ack = self.last_ack.max(stamp);
                }
                Ok(_) => return Err(io::Error::other("invalid watchdog acknowledgement")),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(if now >= self.last_ack.saturating_add(self.timeout) {
            WatchdogState::Unresponsive
        } else {
            WatchdogState::Healthy
        })
    }
}

impl Drop for TransitWatchdog {
    fn drop(&mut self) {
        // The session closes transit before retiring its monitor. PID ownership
        // stays pinned even if it exited between the last poll and this cleanup.
        if self.status.is_none() {
            unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    self.pidfd.as_raw_fd(),
                    libc::SIGKILL,
                    std::ptr::null::<libc::siginfo_t>(),
                    0,
                );
            }
            reap(self.pid);
        }
    }
}

fn monotonic_millis() -> io::Result<u64> {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((time.tv_sec as u64)
        .saturating_mul(1000)
        .saturating_add(time.tv_nsec as u64 / 1_000_000))
}

unsafe fn monitor(fds: &[OwnedFd], executable: *const libc::c_char, timeout: u64) -> ! {
    // Duplicate only from reserved high descriptors so setup cannot clobber a
    // later source, then discard every unrelated inherited capability.
    let targets = [0, 1, 3, 4, 5, 6];
    for (fd, target) in fds.iter().zip(targets) {
        if libc::dup2(fd.as_raw_fd(), target) < 0 {
            libc::_exit(125);
        }
    }
    if libc::dup2(1, 2) < 0 || libc::syscall(libc::SYS_close_range, 7_u32, u32::MAX, 0_u32) != 0 {
        libc::_exit(125);
    }
    if libc::setsid() < 0
        || libc::setns(5, libc::CLONE_NEWUSER) != 0
        || libc::setns(6, libc::CLONE_NEWNET) != 0
    {
        libc::_exit(125);
    }
    libc::close(5);
    libc::close(6);
    let mut action: libc::sigaction = std::mem::zeroed();
    action.sa_sigaction = libc::SIG_DFL;
    libc::sigemptyset(&mut action.sa_mask);
    for signal in 1..=64 {
        libc::sigaction(signal, &action, std::ptr::null_mut());
    }
    libc::sigprocmask(libc::SIG_SETMASK, &action.sa_mask, std::ptr::null_mut());
    let now = raw_millis();
    let mut deadline = now.saturating_add(timeout);
    loop {
        let now = raw_millis();
        if now >= deadline {
            break;
        }
        let mut polls = [
            libc::pollfd {
                fd: 3,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: 4,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let count = libc::poll(
            polls.as_mut_ptr(),
            2,
            (deadline - now).min(i32::MAX as u64) as i32,
        );
        if count < 0 {
            if *libc::__errno_location() == libc::EINTR {
                continue;
            }
            break;
        }
        if polls[1].revents != 0 || raw_millis() >= deadline {
            break;
        }
        if polls[0].revents != 0 {
            let mut bytes = [0_u8; 9];
            let size = libc::recv(3, bytes.as_mut_ptr().cast(), bytes.len(), 0);
            if size != 8 {
                break;
            }
            let stamp = u64::from_ne_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]);
            let now = raw_millis();
            if stamp > now || now >= stamp.saturating_add(timeout) {
                break;
            }
            deadline = deadline.max(stamp.saturating_add(timeout));
            if libc::send(3, bytes.as_ptr().cast(), 8, libc::MSG_NOSIGNAL) != 8 {
                break;
            }
        }
    }
    libc::close(3);
    libc::close(4);
    // Bound a stuck nft invocation independently of the stopped controller.
    libc::alarm(3);
    let args = [executable, c"-f".as_ptr(), c"-".as_ptr(), std::ptr::null()];
    let env = [
        c"PATH=/usr/sbin:/usr/bin:/sbin:/bin".as_ptr(),
        c"LC_ALL=C".as_ptr(),
        std::ptr::null(),
    ];
    libc::execve(executable, args.as_ptr(), env.as_ptr());
    libc::_exit(125)
}

unsafe fn raw_millis() -> u64 {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    if libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) != 0 {
        libc::_exit(125);
    }
    (time.tv_sec as u64)
        .saturating_mul(1000)
        .saturating_add(time.tv_nsec as u64 / 1_000_000)
}
