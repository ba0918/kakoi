//! Private user/network namespaces for trusted network control processes.
//! Applications must enter a further user namespace and receive no control handles.

use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Read};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::time::Duration;

mod dns;
mod watchdog;
pub use dns::DnsSockets;
pub use watchdog::{TransitWatchdog, WatchdogState};

pub struct NetworkNamespace {
    keeper: Keeper,
    user: File,
    net: File,
}

struct Keeper {
    pid: libc::pid_t,
    pidfd: OwnedFd,
    _control: UnixStream,
}

pub(crate) fn pidfd(pid: libc::pid_t) -> io::Result<OwnedFd> {
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: pidfd_open returned a new, uniquely owned descriptor.
    Ok(unsafe { OwnedFd::from_raw_fd(fd as RawFd) })
}

pub(crate) fn duplicate_above_stdio(fd: RawFd) -> io::Result<OwnedFd> {
    let fd = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 10) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

impl NetworkNamespace {
    pub fn create() -> io::Result<Self> {
        Self::create_in(None)
    }

    /// A descendant user namespace lets the transit controller enter the inner
    /// network, without giving the inner controller authority over the transit.
    pub fn create_within(parent: &Self) -> io::Result<Self> {
        Self::create_in(Some(parent))
    }

    fn create_in(outer: Option<&Self>) -> io::Result<Self> {
        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };
        let uid_map = format!("0 {} 1\n", if outer.is_some() { 0 } else { uid });
        let gid_map = format!("0 {} 1\n", if outer.is_some() { 0 } else { gid });
        let parent_pidfd = pidfd(unsafe { libc::getpid() })?;
        let parent_pidfd = duplicate_above_stdio(parent_pidfd.as_raw_fd())?;
        let (parent, child) = UnixStream::pair()?;
        parent.set_read_timeout(Some(Duration::from_secs(5)))?;
        let child = duplicate_above_stdio(child.as_raw_fd())?;
        // SAFETY: the child uses only raw syscalls, stack data and _exit. It never
        // allocates, takes a Rust lock, unwinds, or runs inherited destructors.
        let pid = unsafe { libc::fork() };
        if pid < 0 {
            return Err(io::Error::last_os_error());
        }
        if pid == 0 {
            unsafe {
                hold_namespace(
                    child.as_raw_fd(),
                    parent_pidfd.as_raw_fd(),
                    outer.map(|namespace| (namespace.user.as_raw_fd(), namespace.net.as_raw_fd())),
                    uid_map.as_bytes(),
                    gid_map.as_bytes(),
                )
            }
        }
        drop(child);
        drop(parent_pidfd);
        let child_pidfd = match pidfd(pid) {
            Ok(fd) => fd,
            Err(error) => {
                // The child is still ours and has not been reaped, so its PID cannot
                // yet be reused. Normal cleanup below uses pidfd to avoid PID races.
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }
                reap(pid);
                return Err(error);
            }
        };
        let mut keeper = Keeper {
            pid,
            pidfd: child_pidfd,
            _control: parent,
        };
        let mut result = [0_u8; 4];
        keeper._control.read_exact(&mut result)?;
        let errno = i32::from_ne_bytes(result);
        if errno != 0 {
            return Err(io::Error::new(
                io::Error::from_raw_os_error(errno).kind(),
                format!(
                    "create user/network namespace: {}",
                    io::Error::from_raw_os_error(errno)
                ),
            ));
        }
        let user = File::open(format!("/proc/{pid}/ns/user"))?;
        let net = File::open(format!("/proc/{pid}/ns/net"))?;
        Ok(Self { keeper, user, net })
    }

    pub fn keeper_pid(&self) -> libc::pid_t {
        self.keeper.pid
    }

    /// Builds a trusted controller command. This is not an application launcher:
    /// the command receives the network administrator's user namespace.
    pub fn command(&self, program: impl AsRef<OsStr>) -> io::Result<Command> {
        let mut command = Command::new(program);
        self.enter_before_exec(&mut command)?;
        Ok(command)
    }

    pub(crate) fn enter_before_exec(&self, command: &mut Command) -> io::Result<()> {
        let user = self.user.try_clone()?;
        let net = self.net.try_clone()?;
        // SAFETY: only async-signal-safe syscalls occur between fork and exec. Owned
        // duplicates prevent dangling/reused descriptors if the caller delays spawn.
        unsafe {
            command.pre_exec(move || {
                if libc::setns(user.as_raw_fd(), libc::CLONE_NEWUSER) != 0
                    || libc::setns(net.as_raw_fd(), libc::CLONE_NEWNET) != 0
                {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Ok(())
    }
}

pub(crate) fn reap(pid: libc::pid_t) {
    loop {
        let result = unsafe { libc::waitpid(pid, std::ptr::null_mut(), 0) };
        if result >= 0 || io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
            break;
        }
    }
}

impl Drop for Keeper {
    fn drop(&mut self) {
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

unsafe fn hold_namespace(
    control: RawFd,
    parent: RawFd,
    outer: Option<(RawFd, RawFd)>,
    uid_map: &[u8],
    gid_map: &[u8],
) -> ! {
    let mut result = 0_i32;
    if let Some((user, net)) = outer {
        if libc::setns(user, libc::CLONE_NEWUSER) != 0 || libc::setns(net, libc::CLONE_NEWNET) != 0
        {
            result = *libc::__errno_location();
        }
    }
    if libc::dup3(control, 3, libc::O_CLOEXEC) < 0
        || libc::dup3(parent, 4, libc::O_CLOEXEC) < 0
        || libc::syscall(libc::SYS_close_range, 5_u32, u32::MAX, 0) < 0
    {
        libc::_exit(125);
    }
    if result == 0 && libc::unshare(libc::CLONE_NEWUSER | libc::CLONE_NEWNET) != 0 {
        result = *libc::__errno_location();
    }
    if result == 0 {
        for (path, content) in [
            (c"/proc/self/setgroups", b"deny".as_slice()),
            (c"/proc/self/uid_map", uid_map),
            (c"/proc/self/gid_map", gid_map),
        ] {
            let fd = libc::open(path.as_ptr(), libc::O_WRONLY | libc::O_CLOEXEC);
            if fd < 0 {
                result = *libc::__errno_location();
                break;
            }
            let written = libc::write(fd, content.as_ptr().cast(), content.len());
            if written != content.len() as isize {
                result = if written < 0 {
                    *libc::__errno_location()
                } else {
                    libc::EIO
                };
            }
            libc::close(fd);
            if result != 0 {
                break;
            }
        }
    }
    if libc::write(3, (&result as *const i32).cast(), 4) != 4 || result != 0 {
        libc::_exit(125);
    }
    let mut descriptors = [
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
    loop {
        if libc::poll(descriptors.as_mut_ptr(), 2, -1) >= 0 {
            libc::_exit(0);
        }
        if *libc::__errno_location() != libc::EINTR {
            libc::_exit(125);
        }
    }
}
