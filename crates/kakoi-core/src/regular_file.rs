//! Reading a file the way specification section 14 requires: only a regular file, opened
//! without waiting (a FIFO does not stop the start-up), checked on the open descriptor, and
//! at most `READ_LIMIT` bytes. The caller turns a failure into the diagnostic of its own
//! kind (`policy`, `secret`, or `path`).

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// The most one file is read up to.
pub const READ_LIMIT: u64 = 1 << 20;

/// Whether a symbolic link at the path itself is followed. Links on the way there are
/// always followed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Links {
    Follow,
    DoNotFollow,
}

#[derive(Debug)]
pub enum ReadError {
    /// Nothing exists at the path.
    Absent,
    /// Something exists but is not a regular file (a directory, a FIFO, a socket, or a
    /// link that is not followed).
    NotRegular,
    /// The file is longer than `READ_LIMIT`.
    TooLarge,
    Unreadable(io::Error),
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::Absent => write!(f, "does not exist"),
            ReadError::NotRegular => write!(f, "is not a regular file"),
            ReadError::TooLarge => write!(f, "is larger than {READ_LIMIT} bytes"),
            ReadError::Unreadable(error) => write!(f, "cannot be read: {error}"),
        }
    }
}

/// Reads the whole file at `path`.
pub fn read_regular_file(path: &Path, links: Links) -> Result<Vec<u8>, ReadError> {
    let file = open_without_waiting(path, links)?;
    // Checked on the open descriptor: the entry can be swapped between a check by name
    // and the open.
    match file.metadata() {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => return Err(ReadError::NotRegular),
        Err(error) => return Err(ReadError::Unreadable(error)),
    }
    let mut bytes = Vec::new();
    file.take(READ_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(ReadError::Unreadable)?;
    if bytes.len() as u64 > READ_LIMIT {
        return Err(ReadError::TooLarge);
    }
    Ok(bytes)
}

fn open_without_waiting(path: &Path, links: Links) -> Result<File, ReadError> {
    let mut flags = libc::O_NONBLOCK;
    if links == Links::DoNotFollow {
        flags |= libc::O_NOFOLLOW;
    }
    OpenOptions::new()
        .read(true)
        .custom_flags(flags)
        .open(path)
        .map_err(|error| match error.raw_os_error() {
            Some(libc::ENOENT) | Some(libc::ENOTDIR) => ReadError::Absent,
            Some(libc::ELOOP) => ReadError::NotRegular,
            _ => ReadError::Unreadable(error),
        })
}
