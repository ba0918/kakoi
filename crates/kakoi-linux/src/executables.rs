//! Which candidate path holds an executable regular file: the outer layer of the command
//! resolution and of the search for `bwrap` (specification sections 4.2 and 14).

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use crate::guard_placement::{FileId, NameFact};

/// The first of `candidates` that is a regular file this process may execute.
pub fn first_executable(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|candidate| is_executable_file(candidate))
        .cloned()
}

/// Each of `candidates` that names anything, looked at without following a link, in
/// order, with what it resolves to: the names the real program a guard is placed in front
/// of is chosen from (specification REQ-446).
pub fn named(candidates: &[PathBuf]) -> Vec<NameFact> {
    candidates
        .iter()
        .filter(|candidate| std::fs::symlink_metadata(candidate).is_ok())
        .map(|candidate| {
            let real = crate::mount_facts::real_path(candidate);
            // Looked at through `real` rather than `candidate`, so that the answer agrees
            // with the real path found: asking the kernel to walk the chain again gave
            // "not a program" now and then for a chain of exactly 40 links (seen on CI).
            NameFact {
                candidate: candidate.clone(),
                name: resolved_name(candidate),
                links: followed_links(candidate),
                executable: real.as_deref().is_some_and(is_executable_file),
                file: real.as_deref().and_then(file_id),
                real,
            }
        })
        .collect()
}

/// The places of the links resolving `path` follows, taken against the current directory
/// when relative.
fn followed_links(path: &Path) -> Vec<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(directory) => directory.join(path),
            Err(_) => return Vec::new(),
        }
    };
    crate::mount_facts::traverse(&absolute).links
}

/// `path` with the links of its directory resolved and its own name kept.
fn resolved_name(path: &Path) -> Option<PathBuf> {
    let directory = crate::mount_facts::real_path(path.parent()?)?;
    Some(directory.join(path.file_name()?))
}

/// Which file `path` names, following a link; none when it cannot be read.
pub fn file_id(path: &Path) -> Option<FileId> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|metadata| FileId {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

/// Whether `path`, following a link, is a regular file.
fn is_file(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `access` reads the NUL-terminated path and touches nothing else.
    is_file(path) && unsafe { libc::access(c_path.as_ptr(), libc::X_OK) } == 0
}
