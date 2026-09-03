//! The mount list of the host, read from `/proc/self/mountinfo` (specification
//! section 14). Each line is `id parent major:minor root target options [optional...] -
//! fstype source super-options`; the optional fields vary in number, so the file system
//! type is found after the ` - ` separator.

use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};

use crate::mounts::Mount;

pub const MOUNTINFO: &str = "/proc/self/mountinfo";

/// Reads the mounts listed in the file at `path`. A file that cannot be read lists nothing.
pub fn read_mount_list(path: &Path) -> Vec<Mount> {
    fs::read(path)
        .map(|bytes| parse_mount_list(&bytes))
        .unwrap_or_default()
}

/// The mount target and file system type of each well-formed line. The file is handled as
/// bytes: the kernel escapes only space, tab, newline, and backslash in a path, so a mount
/// point may hold any other byte, and one such line must not take the others with it.
pub fn parse_mount_list(bytes: &[u8]) -> Vec<Mount> {
    bytes
        .split(|byte| *byte == b'\n')
        .filter_map(parse_line)
        .collect()
}

fn parse_line(line: &[u8]) -> Option<Mount> {
    let separator = line.windows(3).position(|window| window == b" - ")?;
    let (before, after) = (&line[..separator], &line[separator + 3..]);
    let target = before.split(|byte| *byte == b' ').nth(4)?;
    let fstype = after.split(|byte| *byte == b' ').next()?;
    Some(Mount {
        target: unescape(target),
        fstype: std::str::from_utf8(fstype).ok()?.to_string(),
    })
}

/// The kernel writes space, tab, newline, and backslash in a path as `\ooo` octal escapes.
fn unescape(field: &[u8]) -> PathBuf {
    let mut bytes = Vec::with_capacity(field.len());
    let mut rest = field;
    while let Some(backslash) = rest.iter().position(|byte| *byte == b'\\') {
        bytes.extend_from_slice(&rest[..backslash]);
        let tail = &rest[backslash + 1..];
        match tail
            .get(..3)
            .and_then(|octal| std::str::from_utf8(octal).ok())
            .and_then(|octal| u8::from_str_radix(octal, 8).ok())
        {
            Some(byte) => {
                bytes.push(byte);
                rest = &tail[3..];
            }
            None => {
                bytes.push(b'\\');
                rest = tail;
            }
        }
    }
    bytes.extend_from_slice(rest);
    PathBuf::from(OsString::from_vec(bytes))
}
