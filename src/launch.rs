//! The start of bwrap (specification section 14): the empty file each `hide` of a file is
//! bound from, the content of each file an `rw-copy` item starts the isolation with, and
//! the seccomp filter are handed over as file descriptors, the symbols of
//! the plan are replaced by their numbers, and the `bwrap` command line is assembled with
//! the plan's arguments (the command and its arguments included) and the assembled
//! environment. Executing it in place is left to the caller. The outer layer; nothing
//! here decides what the plan contains.

use std::ffi::{CString, OsString};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::process::Command;

use crate::diagnostic::Diagnostic;
use crate::plan::{Argument, Plan};
use crate::seccomp::filter_bytes;

/// The `bwrap` command line ready to be executed in place: the program at the plan's
/// path, the arguments with each descriptor's number in place of its symbol, and the
/// plan's environment in place of the host's. The descriptors stay open for as long as
/// this value lives, so that the exec inherits them.
#[derive(Debug)]
pub struct BwrapCommand {
    pub command: Command,
    /// The memory files the arguments refer to by number. Held, never read.
    pub descriptors: Vec<OwnedFd>,
}

/// Assembles the `bwrap` command line for `plan`, raising the soft limit on open files
/// first and making a descriptor for every symbol. Fails when a descriptor cannot be
/// made (the `bwrap` diagnostic of specification section 13).
pub fn assemble(plan: &Plan) -> Result<BwrapCommand, Diagnostic> {
    raise_open_file_limit();
    let mut descriptors = Vec::new();
    let arguments = numbered_arguments(&plan.arguments, &mut descriptors).map_err(|error| {
        Diagnostic::bwrap(format!(
            "a file descriptor for bwrap could not be prepared: {error}"
        ))
    })?;
    let mut command = Command::new(&plan.bwrap);
    command
        .args(arguments)
        .env_clear()
        .envs(plan.environment.values());
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
fn raise_open_file_limit() {
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
        };
        numbered.push(text);
    }
    Ok(numbered)
}

/// A file in memory holding `content`, positioned at its start, that survives the exec
/// (no close-on-exec flag).
fn memory_file(name: &str, content: &[u8]) -> io::Result<OwnedFd> {
    let name = CString::new(name).expect("the name has no NUL");
    // SAFETY: `memfd_create` reads the NUL-terminated name and creates a descriptor
    // that nothing else holds.
    let raw = unsafe { libc::memfd_create(name.as_ptr(), 0) };
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
