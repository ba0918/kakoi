//! The resolution of the command (specification section 4.2). Pure: which candidate is
//! executable is a fact from `executables`.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use crate::diagnostic::{Diagnostic, Kind};

/// The paths where `command` may be: itself when it contains `/`, else `command` under
/// each entry of `path` in order. Without a `PATH` there is nowhere to look.
pub fn command_candidates(command: &OsStr, path: Option<&OsStr>) -> Vec<PathBuf> {
    if command.as_bytes().contains(&b'/') {
        return vec![PathBuf::from(command)];
    }
    path.map(|path| {
        path.as_bytes()
            .split(|byte| *byte == b':')
            .map(|entry| PathBuf::from(OsStr::from_bytes(entry)).join(command))
            .collect()
    })
    .unwrap_or_default()
}

/// The resolved command, or the `command not found` diagnostic whose description is the
/// command name.
pub fn resolve_command(command: &OsStr, found: Option<PathBuf>) -> Result<PathBuf, Diagnostic> {
    found.ok_or_else(|| Diagnostic::new(Kind::CommandNotFound, command.to_string_lossy()))
}
