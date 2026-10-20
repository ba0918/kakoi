//! The isolation's process 1 of a library run: it checks the mounts and generated files,
//! starts the command, reports its result, and reaps every descendant before it lets go.

use std::ffi::OsString;
use std::io;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use super::protocol::{verify_data, DataBatch, InitReply, InitRequest, INIT};
use super::{failure, poll, strongest_reason};
use crate::running::Control;
use crate::{
    ipc, Cleanup, ErrorKind, ExitReason, MainOutcome, NetworkCleanup, ProcessCleanup, RunOutcome,
    StartError,
};
use kakoi_linux::retained_mounts::Identity;

pub(super) fn init(control: &UnixStream) -> io::Result<()> {
    let (request, fds): (InitRequest, _) = ipc::recv(control)?;
    if !fds.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unexpected init request descriptors",
        ));
    }
    // PID 1's default dispositions survive terminal signals. Probes and the target
    // inherit the original mask, not bwrap's supervisor-only blocking.
    kakoi_linux::supervisor_signals::restore(request.terminal_blocked)?;
    // ELF initializers can create descendants before dispatch. None survive release.
    reap_before_release()?;
    let validation = (|| -> io::Result<()> {
        let mut data = Vec::new();
        while data.len() < request.data.len() {
            let (header, batch): (DataBatch, _) = ipc::recv(control)?;
            if header.offset != data.len()
                || batch.is_empty()
                || batch.len() > request.data.len() - data.len()
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid generated data descriptors",
                ));
            }
            data.extend(batch);
        }
        for (check, expected) in request.data.iter().zip(data) {
            verify_data(check, expected)?;
        }
        for check in &request.checks {
            let path = PathBuf::from(OsString::from_vec(check.path.clone()));
            let expected = Identity {
                device: check.device,
                inode: check.inode,
                kind: check.kind,
                rdev: check.rdev,
            };
            if Identity::of_path(&path)? != expected {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "visible mount identity changed",
                ));
            }
        }
        Ok(())
    })();
    if let Err(cause) = validation {
        return ipc::send(
            control,
            &InitReply::Failed(failure(
                ErrorKind::PlanChanged,
                cause,
                MainOutcome::NotStarted,
            )),
            &[],
        );
    }
    let readiness = (|| -> io::Result<()> {
        if let Some(paths) = &request.allowed {
            let mut allowed = Vec::new();
            for bytes in paths.iter().chain(&request.guards) {
                let path = PathBuf::from(OsString::from_vec(bytes.clone()));
                if let Ok(fd) = kakoi_linux::landlock::open_path(&path) {
                    allowed.push(fd);
                }
            }
            allowed.push(kakoi_linux::landlock::open_path(Path::new(INIT))?);
            allowed.extend(
                kakoi_linux::landlock::open_path(Path::new(
                    kakoi_plan::command_limits::DYNAMIC_LINKER,
                ))
                .ok(),
            );
            kakoi_linux::landlock::restrict_execution(&allowed)?;
        }
        for bytes in &request.guards {
            let path = PathBuf::from(OsString::from_vec(bytes.clone()));
            let confirmed = crate::helper_image::probe(&path);
            reap_before_release()?;
            confirmed?;
        }
        Ok(())
    })();
    if let Err(cause) = readiness {
        return ipc::send(
            control,
            &InitReply::Failed(failure(
                ErrorKind::HelperFailure,
                cause,
                MainOutcome::NotStarted,
            )),
            &[],
        );
    }
    let program = OsString::from_vec(request.program);
    let mut command = Command::new(program);
    crate::preparation::remove_helper_environment(&mut command);
    command.arg0(OsString::from_vec(request.argv0));
    command.args(request.arguments.into_iter().map(OsString::from_vec));
    unsafe {
        command.pre_exec(|| kakoi_linux::launch::inherit_only(&[]));
    }
    let child = match command.spawn() {
        Ok(child) => child,
        Err(cause) => {
            ipc::send(
                control,
                &InitReply::Failed(failure(ErrorKind::Io, cause, MainOutcome::NotStarted)),
                &[],
            )?;
            return Ok(());
        }
    };
    let main_pid = child.id() as i32;
    // waitpid below owns this child and every orphan adopted by namespace PID 1.
    drop(child);
    let early_status = match confirm_exec(control, main_pid) {
        Ok(status) => status,
        Err(error) => {
            reap_before_release()?;
            return ipc::send(control, &InitReply::Failed(error), &[]);
        }
    };
    ipc::send(control, &InitReply::Ready, &[])?;
    let mut main = early_status
        .map(main_status)
        .unwrap_or(MainOutcome::Unknown);
    let mut reason = ExitReason::Completed;
    if early_status.is_some() && retain_main(control, main)? {
        reason = ExitReason::StopRequested;
    }
    let mut stopping: Option<Instant> = early_status.map(|_| Instant::now());
    if stopping.is_some() {
        unsafe {
            libc::kill(-1, libc::SIGTERM);
        }
    }
    let mut killed = false;
    let mut control_lost = false;
    loop {
        loop {
            let mut status = 0;
            let pid = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
            if pid == main_pid {
                main = main_status(status);
                if retain_main(control, main)? {
                    reason = ExitReason::StopRequested;
                }
                if stopping.is_none() {
                    stopping = Some(Instant::now());
                    unsafe {
                        libc::kill(-1, libc::SIGTERM);
                    }
                }
            }
            if pid > 0 {
                continue;
            }
            if pid < 0 {
                let cause = io::Error::last_os_error();
                if cause.raw_os_error() == Some(libc::ECHILD) {
                    return ipc::send(
                        control,
                        &InitReply::Finished(RunOutcome {
                            main,
                            reason,
                            network: NetworkCleanup::NotApplicable,
                            processes: ProcessCleanup::ConfirmedReaped,
                            diagnostics: vec![],
                        }),
                        &[],
                    );
                }
                if cause.kind() != io::ErrorKind::Interrupted {
                    return Err(cause);
                }
            }
            break;
        }
        if !control_lost && poll(control, 20)? {
            let observed = match ipc::recv::<Control>(control) {
                Ok((Control::Stop, fds)) if fds.is_empty() => ExitReason::StopRequested,
                _ => {
                    control_lost = true;
                    ExitReason::OwnerLost
                }
            };
            reason = strongest_reason(reason, observed);
            if stopping.is_none() {
                stopping = Some(Instant::now());
                unsafe {
                    libc::kill(-1, libc::SIGTERM);
                }
            }
        }
        if let Some(start) = stopping {
            if !killed && start.elapsed() >= Duration::from_millis(request.grace_ms) {
                unsafe {
                    libc::kill(-1, libc::SIGKILL);
                }
                killed = true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn retain_main(control: &UnixStream, main: MainOutcome) -> io::Result<bool> {
    // Acknowledgement comes only after the caller's independent reaper retained the fact.
    // Descendant shutdown may fail after this point without erasing a known main result.
    ipc::send(control, &InitReply::Main(main), &[])?;
    let mut stopped = false;
    loop {
        let (reply, fds): (Control, _) = ipc::recv(control)?;
        if !fds.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected main acknowledgement descriptors",
            ));
        }
        match reply {
            Control::MainRetained => return Ok(stopped),
            Control::Stop => stopped = true,
            Control::Start => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unexpected start during execution",
                ))
            }
        }
    }
}

fn main_status(status: i32) -> MainOutcome {
    if libc::WIFEXITED(status) {
        MainOutcome::Exited(libc::WEXITSTATUS(status))
    } else {
        MainOutcome::Signaled(libc::WTERMSIG(status), libc::WCOREDUMP(status))
    }
}

fn confirm_exec(control: &UnixStream, pid: i32) -> Result<Option<i32>, StartError> {
    let init_identity = Identity::of_path(Path::new("/proc/self/exe"))
        .map_err(|cause| failure(ErrorKind::Io, cause, MainOutcome::Unknown))?;
    let executable = PathBuf::from(format!("/proc/{pid}/exe"));
    loop {
        // The inherited init inode proves only fork. A different executable inode
        // proves exec, including a shebang interpreter; error-pipe EOF does not.
        if Identity::of_path(&executable).is_ok_and(|id| id != init_identity) {
            return Ok(None);
        }
        let mut status = 0;
        let waited = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
        if waited == pid {
            if libc::WIFEXITED(status) {
                return Ok(Some(status));
            }
            let mut error = failure(
                ErrorKind::HelperFailure,
                io::Error::other(format!(
                    "command died before exec confirmation: signal {}, core={}",
                    libc::WTERMSIG(status),
                    libc::WCOREDUMP(status)
                )),
                MainOutcome::Unknown,
            );
            error.cleanup = Cleanup::Confirmed;
            return Err(error);
        }
        if waited < 0 {
            let cause = io::Error::last_os_error();
            if cause.kind() != io::ErrorKind::Interrupted {
                return Err(failure(ErrorKind::Io, cause, MainOutcome::Unknown));
            }
        }
        if poll(control, 0).map_err(|cause| failure(ErrorKind::Io, cause, MainOutcome::Unknown))? {
            return Err(failure(
                ErrorKind::Protocol,
                io::Error::new(io::ErrorKind::ConnectionAborted, "startup control lost"),
                MainOutcome::Unknown,
            ));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn reap_before_release() -> io::Result<()> {
    // This function is called only after dispatch proved that we are namespace PID 1.
    unsafe {
        libc::kill(-1, libc::SIGKILL);
    }
    loop {
        let mut status = 0;
        let pid = unsafe { libc::waitpid(-1, &mut status, 0) };
        if pid > 0 {
            continue;
        }
        let cause = io::Error::last_os_error();
        if cause.raw_os_error() == Some(libc::ECHILD) {
            return Ok(());
        }
        if cause.kind() != io::ErrorKind::Interrupted {
            return Err(cause);
        }
    }
}
