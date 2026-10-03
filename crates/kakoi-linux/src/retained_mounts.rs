//! O_PATH references retained independently of path names during preparation.

use std::ffi::CString;
use std::io;
use std::os::fd::{FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use crate::plan::Argument;

pub struct RetainedMount {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub descriptor: OwnedFd,
    pub read_only: bool,
}

pub fn retain(arguments: &[Argument]) -> io::Result<Vec<RetainedMount>> {
    let mut mounts = Vec::new();
    for arguments in arguments.windows(3) {
        let Argument::Literal(option) = &arguments[0] else {
            continue;
        };
        if option != "--bind" && option != "--ro-bind" && option != "--dev-bind" {
            continue;
        }
        // Fixed shared files are prepared only at spawn, not at prepare.
        let Argument::Literal(source) = &arguments[1] else {
            continue;
        };
        let Argument::Literal(destination) = &arguments[2] else {
            continue;
        };
        let name = CString::new(source.as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in mount source"))?;
        // SAFETY: name is terminated and the returned descriptor is owned exactly once.
        let fd = unsafe { libc::open(name.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        mounts.push(RetainedMount {
            source: source.into(),
            destination: destination.into(),
            descriptor: unsafe { OwnedFd::from_raw_fd(fd) },
            read_only: option == "--ro-bind",
        });
    }
    Ok(mounts)
}
