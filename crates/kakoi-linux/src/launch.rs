//! The start of bwrap (specification section 14): the files of the shared file place are
//! checked and made again, or else put from data (specification REQ-461); the empty files
//! and the content of each file an `rw-copy` item starts the isolation with, and the
//! seccomp filter, are handed over as file descriptors, the symbols of
//! the plan are replaced by their numbers, and the `bwrap` command line is assembled with
//! the plan's arguments (the command and its arguments included) and the assembled
//! environment. Executing it in place is left to the caller. The outer layer; nothing
//! here decides what the plan contains.

use std::ffi::{CString, OsString};
use std::fmt;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::diagnostic::Diagnostic;
use crate::plan::{Argument, Plan};
use crate::policy::{ListMode, NetworkMode};
use crate::seccomp::filter_bytes;
use crate::shared_files;

/// The `bwrap` command line ready to be executed in place: the program at the plan's
/// path, the arguments with each descriptor's number in place of its symbol, and the
/// plan's environment in place of the host's. The descriptors stay open for as long as
/// this value lives, so that the exec inherits them.
pub struct BwrapCommand {
    pub command: Command,
    /// The memory files the arguments refer to by number. Held, never read.
    pub descriptors: Vec<OwnedFd>,
}

/// In a freshly forked helper, preserve only its assigned non-standard capabilities.
/// This function uses only async-signal-safe syscalls and must run before exec.
pub fn inherit_only(descriptors: &[libc::c_int]) -> io::Result<()> {
    // SAFETY: close_range changes only this process's private descriptor table.
    if unsafe {
        libc::syscall(
            libc::SYS_close_range,
            3u32,
            u32::MAX,
            libc::CLOSE_RANGE_CLOEXEC,
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    for &fd in descriptors {
        // SAFETY: fcntl changes flags of the specified inherited descriptor only.
        if unsafe { libc::fcntl(fd, libc::F_SETFD, 0) } < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Written by hand rather than derived: the environment's values have been moved into
/// the `Command`, whose `Debug` prints them, secrets included, so the masking of
/// `Environment` does not reach here. Only the names of the variables are shown
/// (specification section 9).
impl fmt::Debug for BwrapCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let variables: Vec<_> = self.command.get_envs().map(|(name, _)| name).collect();
        f.debug_struct("BwrapCommand")
            .field("program", &self.command.get_program())
            .field("arguments", &self.command.get_args().collect::<Vec<_>>())
            .field("variables", &variables)
            .field("descriptors", &self.descriptors)
            .finish()
    }
}

/// Assembles the `bwrap` command line for `plan`, raising the soft limit on open files
/// first and making a descriptor for every symbol. Fails when a descriptor cannot be
/// made (the `bwrap` diagnostic of specification section 13).
pub fn assemble(plan: &Plan) -> Result<BwrapCommand, Diagnostic> {
    if plan.policy.network_mode == crate::policy::NetworkMode::Filtered {
        return Err(Diagnostic::bwrap(
            "filtered launch requires a prepared kakoi-net session",
        ));
    }
    assemble_with_prefix(plan, &[])
}

/// Assembly entry for the network executor, which must enter the prepared user
/// and network namespaces before executing bwrap. This function performs no
/// namespace operation and does not establish or verify network permissions.
pub fn assemble_for_network_executor(
    plan: &Plan,
    uid: u32,
    gid: u32,
) -> Result<BwrapCommand, Diagnostic> {
    assemble_network(plan, uid, gid, None, None)
}

/// A supervisor replaces only the command tail; mount choices stay in the plan.
pub struct ProcessSupervisor {
    pub argv0: OsString,
    pub program: PathBuf,
    pub arguments: Vec<OsString>,
}

/// The observed destination of the resolver mount for the legacy host-root launch.
pub fn network_resolver_target(plan: &Plan) -> Result<Option<PathBuf>, Diagnostic> {
    if plan.policy.network_mode != NetworkMode::Filtered
        || plan.policy.mounts_mode != ListMode::Host
    {
        return Ok(None);
    }
    std::fs::canonicalize("/etc/resolv.conf")
        .map(Some)
        .map_err(|error| {
            Diagnostic::bwrap(format!(
                "locate application resolver configuration: {error}"
            ))
        })
}

pub fn assemble_network(
    plan: &Plan,
    uid: u32,
    gid: u32,
    resolver: Option<&Path>,
    supervisor: Option<&ProcessSupervisor>,
) -> Result<BwrapCommand, Diagnostic> {
    if plan.policy.network_mode != crate::policy::NetworkMode::Filtered {
        return Err(Diagnostic::bwrap("network executor requires filtered mode"));
    }
    let mut arguments = plan.arguments.clone();
    if let Some(resolver) = resolver {
        let index = plan.launch_layout.resolver_destination.ok_or_else(|| {
            Diagnostic::bwrap("filtered plan lacks managed resolver configuration")
        })?;
        let destination = arguments.get_mut(index).ok_or_else(|| {
            Diagnostic::bwrap("filtered plan lacks managed resolver configuration")
        })?;
        *destination = Argument::Literal(resolver.as_os_str().into());
    }
    if let Some(supervisor) = supervisor {
        let missing = || Diagnostic::bwrap("filtered plan lacks a command to supervise");
        let argv0 = plan.launch_layout.argv0.ok_or_else(missing)?;
        let separator = plan.launch_layout.command_separator.ok_or_else(missing)?;
        if separator >= arguments.len() {
            return Err(missing());
        }
        let given = arguments.get_mut(argv0).ok_or_else(missing)?;
        let Argument::Literal(given) =
            std::mem::replace(given, Argument::Literal(supervisor.argv0.clone()))
        else {
            return Err(missing());
        };
        let command = arguments.split_off(separator + 1);
        arguments.insert(separator, Argument::Literal("--as-pid-1".into()));
        arguments.push(Argument::Literal(supervisor.program.as_os_str().into()));
        arguments.extend(supervisor.arguments.iter().cloned().map(Argument::Literal));
        arguments.push(Argument::Literal(given));
        arguments.extend(command);
    }
    assemble_with_arguments(
        plan,
        &[
            "--uid".into(),
            uid.to_string().into(),
            "--gid".into(),
            gid.to_string().into(),
        ],
        &arguments,
    )
}

fn assemble_with_prefix(plan: &Plan, prefix: &[OsString]) -> Result<BwrapCommand, Diagnostic> {
    assemble_with_arguments(plan, prefix, &plan.arguments)
}

fn assemble_with_arguments(
    plan: &Plan,
    prefix: &[OsString],
    arguments: &[Argument],
) -> Result<BwrapCommand, Diagnostic> {
    raise_open_file_limit();
    let mut descriptors = Vec::new();
    let arguments = shared_files::settle(arguments);
    let arguments = numbered_arguments(&arguments, &mut descriptors).map_err(|error| {
        Diagnostic::bwrap(format!(
            "a file descriptor for bwrap could not be prepared: {error}"
        ))
    })?;
    let mut command = Command::new(&plan.bwrap);
    command
        .args(prefix)
        .args(arguments)
        .env_clear()
        .envs(plan.environment.values());
    // Close-on-exec is cleared only in this command's own child, so another launch
    // prepared at the same time in this process never inherits these files.
    let inherited: Vec<_> = descriptors.iter().map(AsRawFd::as_raw_fd).collect();
    // SAFETY: only `fcntl`, which is async-signal-safe, runs between fork and exec.
    unsafe {
        command.pre_exec(move || {
            for &fd in &inherited {
                if libc::fcntl(fd, libc::F_SETFD, 0) != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    if plan.policy.mounts_mode == ListMode::Listed && plan.policy.network_mode == NetworkMode::Host
    {
        // Before bwrap rather than inside: Landlock's scopes do not forbid mounting, and
        // what bwrap starts inherits them (specification REQ-472).
        // SAFETY: only system calls run between fork and exec.
        unsafe {
            command.pre_exec(crate::landlock::scope_abstract_unix_sockets);
        }
    }
    Ok(BwrapCommand {
        command,
        descriptors,
    })
}

/// Raises the soft limit on open files to the hard limit before any descriptor is made
/// (specification section 14): a monorepo with thousands of hidden files needs one
/// descriptor each, and the usual soft limit of 1024 would stop the start. Always raised,
/// so that the same input gives the same result, and inherited by the command. A failure
/// here is not reported on its own: if the descriptors then cannot be made, that is the
/// `bwrap` diagnostic.
pub fn raise_open_file_limit() {
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: `getrlimit` writes the current limits into the struct given, which is valid
    // for the call.
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) } != 0 {
        return;
    }
    limit.rlim_cur = limit.rlim_max;
    // SAFETY: `setrlimit` reads the struct given, which is valid for the call.
    unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limit) };
}

/// The arguments with each symbol replaced by the number of a descriptor made for it.
/// bwrap closes a data descriptor after reading it, so every occurrence gets its own.
/// The descriptors are kept in `descriptors` for the caller to hold until the exec, which
/// inherits them.
fn numbered_arguments(
    arguments: &[Argument],
    descriptors: &mut Vec<OwnedFd>,
) -> io::Result<Vec<OsString>> {
    let mut numbered = Vec::with_capacity(arguments.len());
    for argument in arguments {
        let text = match argument {
            Argument::Literal(text) => text.clone(),
            Argument::EmptyFile => {
                let fd = memory_file("kakoi-empty", &[])?;
                let number = OsString::from(fd.as_raw_fd().to_string());
                descriptors.push(fd);
                number
            }
            Argument::Seccomp => {
                let fd = memory_file("kakoi-seccomp", &filter_bytes())?;
                let number = OsString::from(fd.as_raw_fd().to_string());
                descriptors.push(fd);
                number
            }
            Argument::CopiedFile(content) => {
                let fd = memory_file("kakoi-copy", content.bytes())?;
                let number = OsString::from(fd.as_raw_fd().to_string());
                descriptors.push(fd);
                number
            }
            Argument::SharedFile { path, .. } => path.clone().into_os_string(),
        };
        numbered.push(text);
    }
    Ok(numbered)
}

/// A file in memory holding `content`, positioned at its start. It is created
/// close-on-exec; only the `bwrap` command it was made for clears the flag.
fn memory_file(name: &str, content: &[u8]) -> io::Result<OwnedFd> {
    let name = CString::new(name).expect("the name has no NUL");
    // SAFETY: `memfd_create` reads the NUL-terminated name and creates a descriptor
    // that nothing else holds.
    let raw = unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC) };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `raw` is a descriptor this function alone owns.
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    let mut written = 0;
    while written < content.len() {
        let rest = &content[written..];
        // SAFETY: `rest` is a valid buffer of `rest.len()` bytes for the whole call.
        let count = unsafe { libc::write(fd.as_raw_fd(), rest.as_ptr().cast(), rest.len()) };
        if count < 0 {
            return Err(io::Error::last_os_error());
        }
        written += count as usize;
    }
    // SAFETY: a plain seek on a descriptor this function owns.
    if unsafe { libc::lseek(fd.as_raw_fd(), 0, libc::SEEK_SET) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(fd)
}
