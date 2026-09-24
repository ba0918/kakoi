//! A separate writer process may block on stderr; its controller never does.
use crate::namespace::{child_pidfd, duplicate_above_stdio, kill_and_reap};
use std::{
    io,
    os::{
        fd::{AsRawFd, BorrowedFd, OwnedFd},
        unix::net::UnixDatagram,
    },
};

pub(super) enum Progress {
    Ready,
    Busy,
    Lost,
    Closed,
}

pub struct NotificationWriter {
    pid: libc::pid_t,
    pidfd: OwnedFd,
    control: UnixDatagram,
    busy: bool,
    reaped: bool,
}
impl NotificationWriter {
    /// The CLI supplies stderr. Its open-file-description flags are left intact,
    /// including when the application shares that descriptor.
    pub fn new(output: BorrowedFd<'_>) -> io::Result<Self> {
        let (parent, child) = UnixDatagram::pair()?;
        parent.set_nonblocking(true)?;
        let output_fd = duplicate_above_stdio(output.as_raw_fd())?;
        let control_fd = duplicate_above_stdio(child.as_raw_fd())?;
        let parent_pid = unsafe { libc::getpid() };
        // SAFETY: the child runs only `writer`, which allocates nothing and uses
        // raw syscalls on descriptors prepared before fork.
        let pid = unsafe { libc::fork() };
        if pid < 0 {
            return Err(io::Error::last_os_error());
        }
        if pid == 0 {
            unsafe { writer(output_fd.as_raw_fd(), control_fd.as_raw_fd(), parent_pid) }
        }
        drop(child);
        let pidfd = child_pidfd(pid)?;
        Ok(Self {
            pid,
            pidfd,
            control: parent,
            busy: false,
            reaped: false,
        })
    }
    pub fn pid(&self) -> libc::pid_t {
        self.pid
    }
    pub(super) fn poll(&mut self) -> Progress {
        if !self.reaped {
            let result = unsafe { libc::waitpid(self.pid, std::ptr::null_mut(), libc::WNOHANG) };
            let exited = result == self.pid;
            let already_reaped =
                result < 0 && io::Error::last_os_error().raw_os_error() == Some(libc::ECHILD);
            if exited || already_reaped {
                self.reaped = true;
            }
        }
        if self.reaped {
            return if std::mem::take(&mut self.busy) {
                Progress::Lost
            } else {
                Progress::Closed
            };
        }
        if !self.busy {
            return Progress::Ready;
        }
        let mut reply = [0; 2];
        match self.control.recv(&mut reply) {
            Ok(1) => {
                self.busy = false;
                if reply[0] == 1 {
                    Progress::Ready
                } else {
                    Progress::Lost
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Progress::Busy
            }
            _ => {
                self.busy = false;
                Progress::Lost
            }
        }
    }
    pub(super) fn send(&mut self, line: &str) -> io::Result<()> {
        if self.busy || self.reaped {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "notification writer unavailable",
            ));
        }
        if line.is_empty() || line.len() > super::MAX_LINE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid notification size",
            ));
        }
        let sent = unsafe {
            libc::send(
                self.control.as_raw_fd(),
                line.as_ptr().cast(),
                line.len(),
                libc::MSG_DONTWAIT | libc::MSG_NOSIGNAL,
            )
        };
        if sent < 0 {
            return Err(io::Error::last_os_error());
        }
        self.busy = true;
        Ok(())
    }
}
impl Drop for NotificationWriter {
    fn drop(&mut self) {
        if !self.reaped {
            kill_and_reap(&self.pidfd, self.pid);
        }
    }
}

/// # Safety
///
/// Call only in the child of a fork, which this never returns to: it uses only
/// async-signal-safe calls on descriptors prepared before the fork.
unsafe fn writer(output: libc::c_int, control: libc::c_int, parent: libc::pid_t) -> ! {
    // All allocations happen before fork. Child setup and writes use stack data
    // and raw syscalls only; never run inherited handlers, locks or destructors.
    if libc::dup2(output, 2) < 0
        || libc::dup2(control, 3) < 0
        || libc::syscall(libc::SYS_close_range, 4_u32, u32::MAX, 0_u32) != 0
    {
        libc::_exit(125);
    }
    libc::close(0);
    libc::close(1);
    let mut action: libc::sigaction = std::mem::zeroed();
    action.sa_sigaction = libc::SIG_DFL;
    libc::sigemptyset(&mut action.sa_mask);
    for signal in 1..=64 {
        libc::sigaction(signal, &action, std::ptr::null_mut());
    }
    action.sa_sigaction = libc::SIG_IGN;
    libc::sigaction(libc::SIGPIPE, &action, std::ptr::null_mut());
    libc::sigprocmask(libc::SIG_SETMASK, &action.sa_mask, std::ptr::null_mut());
    if libc::setsid() < 0
        || libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0
        || libc::getppid() != parent
    {
        libc::_exit(125);
    }
    let mut bytes = [0_u8; super::MAX_LINE + 1];
    loop {
        let count = libc::recv(3, bytes.as_mut_ptr().cast(), bytes.len(), 0);
        if count < 0 && *libc::__errno_location() == libc::EINTR {
            continue;
        }
        if count <= 0 || count as usize > super::MAX_LINE {
            libc::_exit(125);
        }
        let mut offset = 0;
        let mut success = 1_u8;
        while offset < count as usize {
            let written = libc::write(
                2,
                bytes.as_ptr().add(offset).cast(),
                count as usize - offset,
            );
            if written > 0 {
                offset += written as usize;
            } else if written < 0 && *libc::__errno_location() == libc::EINTR {
                continue;
            } else {
                success = 0;
                break;
            }
        }
        if libc::send(3, (&success as *const u8).cast(), 1, libc::MSG_NOSIGNAL) != 1 {
            libc::_exit(0);
        }
    }
}
