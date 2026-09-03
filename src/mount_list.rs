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
    fs::read_to_string(path)
        .map(|text| parse_mount_list(&text))
        .unwrap_or_default()
}

/// The mount target and file system type of each well-formed line.
pub fn parse_mount_list(text: &str) -> Vec<Mount> {
    text.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<Mount> {
    let (before, after) = line.split_once(" - ")?;
    let target = before.split(' ').nth(4)?;
    let fstype = after.split(' ').next()?;
    Some(Mount {
        target: unescape(target),
        fstype: fstype.to_string(),
    })
}

/// The kernel writes space, tab, newline, and backslash in a path as `\ooo` octal escapes.
fn unescape(field: &str) -> PathBuf {
    let mut bytes = Vec::with_capacity(field.len());
    let mut rest = field.as_bytes();
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
