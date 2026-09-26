//! Which candidate path holds an executable regular file: the outer layer of the command
//! resolution and of the search for `bwrap` (specification sections 4.2 and 14).

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use crate::guard_placement::{FileId, ProgramFact};

/// The first of `candidates` that is a regular file this process may execute.
pub fn first_executable(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|candidate| is_executable_file(candidate))
        .cloned()
}

/// The first of `candidates` that names anything, looked at without following a link,
/// with what it resolves to: the program a guard is placed in front of (specification
/// REQ-449 skips it when it is not a regular file, rather than searching on).
pub fn first_named(candidates: &[PathBuf]) -> ProgramFact {
    candidates
        .iter()
        .find(|candidate| std::fs::symlink_metadata(candidate).is_ok())
        .map_or(ProgramFact::NotFound, |candidate| ProgramFact::Found {
            candidate: candidate.clone(),
            real: std::fs::canonicalize(candidate).ok(),
            regular: std::fs::metadata(candidate).is_ok_and(|metadata| metadata.is_file()),
            file: file_id(candidate),
        })
}

/// Which file `path` names, following a link; none when it cannot be read.
pub fn file_id(path: &Path) -> Option<FileId> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|metadata| FileId {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

fn is_executable_file(path: &Path) -> bool {
    let is_file = std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file());
    let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `access` reads the NUL-terminated path and touches nothing else.
    is_file && unsafe { libc::access(c_path.as_ptr(), libc::X_OK) } == 0
}
