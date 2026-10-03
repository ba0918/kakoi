//! Command lookup shared by network setup and the runtime planning procedure.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use crate::command::{command_candidates, resolve_command};
use crate::diagnostic::Diagnostic;
use crate::executables::first_executable;

pub fn locate_command(
    command: &OsStr,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<PathBuf, Diagnostic> {
    let path = environment.get(OsStr::new("PATH")).map(OsString::as_os_str);
    resolve_command(
        command,
        first_executable(&command_candidates(command, path)),
    )
}
