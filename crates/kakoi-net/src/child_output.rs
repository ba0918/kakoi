//! A helper process's standard error, read without ever blocking on it, and its
//! tail kept for a diagnostic.

use std::io::{self, Read};
use std::os::fd::RawFd;

pub(crate) fn nonblocking(fd: RawFd) -> io::Result<()> {
    // SAFETY: F_GETFL/F_SETFL only read and update the status flags of a
    // descriptor the caller keeps open.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Reads what `reader` has now into `tail`, keeping its last 8 KiB. Bounds the
/// work per call as well as the memory; drains a full Linux pipe, so a tool that
/// just exited does not lose the end of its diagnostic.
pub(crate) fn drain(reader: &mut impl Read, tail: &mut Vec<u8>) -> io::Result<()> {
    drain_keeping(reader, tail, 8192)
}

/// As [`drain`], keeping the last `limit` bytes.
pub(crate) fn drain_keeping(
    reader: &mut impl Read,
    tail: &mut Vec<u8>,
    limit: usize,
) -> io::Result<()> {
    let mut buffer = [0; 1024];
    for _ in 0..1024 {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                tail.extend_from_slice(&buffer[..count]);
                let excess = tail.len().saturating_sub(limit);
                tail.drain(..excess);
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// `error`, followed by the helper's standard error with control characters escaped.
pub(crate) fn with_diagnostic(error: io::Error, diagnostic: &[u8]) -> io::Error {
    io::Error::new(error.kind(), described(&error, diagnostic))
}

/// `what`, followed by the helper's standard error with control characters escaped.
pub(crate) fn described(what: &dyn std::fmt::Display, diagnostic: &[u8]) -> String {
    let message = kakoi_core::diagnostic::escape_control(&String::from_utf8_lossy(diagnostic));
    format!("{what}: {message}")
}
