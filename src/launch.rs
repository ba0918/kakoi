//! The start of bwrap (specification section 14): the empty file each `hide` of a file is
//! bound from and the seccomp filter are handed over as file descriptors, the symbols of
//! the plan are replaced by their numbers, and `bwrap` is executed in place with the
//! plan's arguments (the command and its arguments included) and the assembled
//! environment. The outer layer; nothing here decides what the plan contains.

use std::ffi::{CString, OsString};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::Command;

use crate::diagnostic::Diagnostic;
use crate::plan::Argument;
use crate::seccomp::filter_bytes;
use crate::startup::Prepared;

/// Executes `bwrap` for `prepared`. Returns only when the exec itself fails.
pub fn launch(prepared: &Prepared) -> Diagnostic {
    let plan = &prepared.plan;
    let mut descriptors = Vec::new();
    let arguments = match numbered_arguments(&plan.arguments, &mut descriptors) {
        Ok(arguments) => arguments,
        Err(error) => {
            return Diagnostic::bwrap(format!(
                "a file descriptor for bwrap could not be prepared: {error}"
            ))
        }
    };
    let error = Command::new(&plan.bwrap)
        .args(arguments)
        .env_clear()
        .envs(plan.environment.values())
        .exec();
    Diagnostic::bwrap(format!(
        "{} could not be executed: {error}",
        plan.bwrap.display()
    ))
}

/// The arguments with each symbol replaced by the number of a descriptor made for it.
/// bwrap closes a data descriptor after reading it, so every occurrence gets its own.
/// The descriptors are kept in `descriptors` until the exec, which inherits them.
fn numbered_arguments(
    arguments: &[Argument],
    descriptors: &mut Vec<OwnedFd>,
) -> io::Result<Vec<OsString>> {
    let mut numbered = Vec::with_capacity(arguments.len());
    for argument in arguments {
        let text = match argument {
            Argument::Literal(text) => text.clone(),
            Argument::EmptyFile => {
                let fd = memory_file("process-wrap-empty", &[])?;
                let number = OsString::from(fd.as_raw_fd().to_string());
                descriptors.push(fd);
                number
            }
            Argument::Seccomp => {
                let fd = memory_file("process-wrap-seccomp", &filter_bytes())?;
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
