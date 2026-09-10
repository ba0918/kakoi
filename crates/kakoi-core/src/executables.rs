//! Which candidate path holds an executable regular file: the outer layer of the command
//! resolution and of the search for `bwrap` (specification sections 4.2 and 14).

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

/// The first of `candidates` that is a regular file this process may execute.
pub fn first_executable(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|candidate| is_executable_file(candidate))
        .cloned()
}

fn is_executable_file(path: &Path) -> bool {
    let is_file = std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file());
    let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `access` reads the NUL-terminated path and touches nothing else.
    is_file && unsafe { libc::access(c_path.as_ptr(), libc::X_OK) } == 0
}
