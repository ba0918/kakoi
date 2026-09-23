//! The application side of a filtered run: bwrap with the supervisor as process 1.
//! The caller blocks the network before asking for termination or grace; this
//! owner only relays those decisions and never changes network permissions.

use crate::{
    application::{self, Supervision},
    init,
    namespace::NetworkNamespace,
};
use kakoi_core::{diagnostic::Diagnostic, plan::Plan};
use std::{
    ffi::CString,
    io,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::{ffi::OsStrExt, process::CommandExt},
    },
    path::Path,
    process::{Child, ChildStdout, ExitStatus, Stdio},
    time::Duration,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationEvent {
    /// The confirmed result of the main command, `128 + signal` for a signal.
    MainExited(u8),
    /// Ctrl+C ended the grace after the main command had exited.
    GraceInterrupted,
    /// bwrap, and with it every process of the isolation, has ended.
    Finished(ExitStatus),
}

pub struct Application {
    child: Child,
    control: OwnedFd,
    finished: bool,
}

impl Application {
    /// `init` must be an executable whose `main` first calls
    /// [`crate::init::run_if_requested`]. Start this only after the network
    /// enforcement for `namespace` is installed and verified.
    pub fn spawn(
        plan: &Plan,
        namespace: &NetworkNamespace,
        init: &Path,
        grace: Duration,
        stdout: Stdio,
    ) -> Result<Self, Diagnostic> {
        let failure = |what: &str, error: io::Error| Diagnostic::bwrap(format!("{what}: {error}"));
        let executable = open_path(init)
            .map_err(|error| failure(&format!("open supervisor {}", init.display()), error))?;
        let (control, inherited) =
            control_pair().map_err(|error| failure("create supervisor channel", error))?;
        let supervision = Supervision {
            init: executable.as_raw_fd(),
            control: inherited.as_raw_fd(),
            grace,
            interrupt_ignored: interrupt_ignored(),
        };
        let mut bwrap = application::prepare_supervised(plan, namespace, &supervision)?;
        let shared = [supervision.init, supervision.control];
        // SAFETY: only async-signal-safe calls between fork and exec.
        unsafe {
            bwrap.command.pre_exec(move || {
                for fd in shared {
                    if libc::fcntl(fd, libc::F_SETFD, 0) != 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
                // The terminal's Ctrl+C belongs to the application; bwrap must
                // survive it. The supervisor restores the original disposition.
                libc::signal(libc::SIGINT, libc::SIG_IGN);
                Ok(())
            })
        };
        let child = bwrap
            .command
            .stdout(stdout)
            .spawn()
            .map_err(|error| failure("start bwrap", error))?;
        Ok(Self {
            child,
            control,
            finished: false,
        })
    }

    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout.take()
    }

    /// Never blocks. Supervisor reports are returned before `Finished`.
    pub fn poll(&mut self) -> io::Result<Option<ApplicationEvent>> {
        let mut message = [0u8; 2];
        // SAFETY: reads one message into a local buffer.
        let read = unsafe {
            libc::recv(
                self.control.as_raw_fd(),
                message.as_mut_ptr().cast(),
                message.len(),
                libc::MSG_DONTWAIT,
            )
        };
        match (read, message[0]) {
            (2, init::MAIN_EXITED) => return Ok(Some(ApplicationEvent::MainExited(message[1]))),
            (1, init::GRACE_INTERRUPTED) => return Ok(Some(ApplicationEvent::GraceInterrupted)),
            (read, _) if read > 0 => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unexpected supervisor message",
                ))
            }
            _ => {}
        }
        if self.finished {
            return Ok(None);
        }
        let status = self.child.try_wait()?;
        self.finished = status.is_some();
        Ok(status.map(ApplicationEvent::Finished))
    }

    /// After the main command exited and the network is blocked: request every
    /// remaining process to end within the one grace.
    pub fn begin_grace(&mut self) -> io::Result<()> {
        self.send(init::BEGIN_GRACE)
    }

    /// After an external termination request, once the network is blocked.
    pub fn terminate(&mut self) -> io::Result<()> {
        self.send(init::TERMINATE)
    }

    /// Ends the whole isolation at once, without any grace.
    pub fn kill(&mut self) {
        if !self.finished {
            let _ = self.child.kill();
        }
    }

    fn send(&self, message: u8) -> io::Result<()> {
        // SAFETY: sends a one-byte local buffer.
        let sent = unsafe {
            libc::send(
                self.control.as_raw_fd(),
                (&message as *const u8).cast(),
                1,
                libc::MSG_NOSIGNAL | libc::MSG_DONTWAIT,
            )
        };
        if sent == 1 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

impl Drop for Application {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn open_path(path: &Path) -> io::Result<OwnedFd> {
    let path = CString::new(path.as_os_str().as_bytes())?;
    // SAFETY: opens a NUL-terminated path; the result is owned below.
    let fd = unsafe { libc::open(path.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `fd` was just opened and is owned by nothing else.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn control_pair() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0; 2];
    // SAFETY: writes two new descriptors into the local array.
    if unsafe {
        libc::socketpair(
            libc::AF_UNIX,
            libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC,
            0,
            fds.as_mut_ptr(),
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: both descriptors were just created and are owned by nothing else.
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

fn interrupt_ignored() -> bool {
    // SAFETY: queries the current disposition without changing it.
    unsafe {
        let mut current: libc::sigaction = std::mem::zeroed();
        libc::sigaction(libc::SIGINT, std::ptr::null(), &mut current);
        current.sa_sigaction == libc::SIG_IGN
    }
}
