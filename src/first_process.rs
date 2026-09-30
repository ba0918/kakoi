//! kakoi started as the isolation's first process of the "listed" command mode
//! (specification REQ-475): when this executable was started from the place the plan put
//! it at, it restricts execution with Landlock to the programs of the list placed beside
//! it, the dynamic linker, and kakoi itself, then replaces itself with the command. bwrap
//! starts it with the command's name as argv[0] and the command's path first among the
//! arguments. This is the outer layer; the list and the system calls are `kakoi-core`'s.

use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::os::fd::OwnedFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, ExitCode};

use kakoi_core::command_limits::{AllowedList, ALLOWED_LIST, DYNAMIC_LINKER, FIRST_PROCESS};
use kakoi_core::diagnostic::{Diagnostic, Kind, Warning};
use kakoi_core::landlock::{open_path, restrict_execution};

/// Acts as the first process when this executable was started from its place, and
/// returns the exit code; `None` otherwise, and kakoi goes on as itself.
pub fn run_if_first_process() -> Option<ExitCode> {
    if std::fs::read_link("/proc/self/exe").ok()? != Path::new(FIRST_PROCESS) {
        return None;
    }
    let mut arguments = std::env::args_os();
    let given = arguments.next().unwrap_or_default();
    let Some(path) = arguments.next() else {
        return Some(report(Diagnostic::bwrap(
            "the isolation's first process was started without a command",
        )));
    };
    let list = std::fs::read(ALLOWED_LIST)
        .ok()
        .and_then(|bytes| AllowedList::from_bytes(&bytes));
    let Some(list) = list else {
        return Some(report(Diagnostic::bwrap(format!(
            "the list of the programs to allow cannot be read from {ALLOWED_LIST}"
        ))));
    };
    let allowed = open_allowed(&list);
    if let Err(error) = restrict_execution(&allowed) {
        return Some(report(Diagnostic::bwrap(format!(
            "execution cannot be restricted with Landlock: {error}"
        ))));
    }
    Some(start(&given, &path, arguments.collect()))
}

/// The files to allow, each opened as a path: the listed ones, with a warning for each
/// that cannot be opened, the dynamic linker when it is there, and kakoi itself.
fn open_allowed(list: &AllowedList) -> Vec<OwnedFd> {
    let mut allowed = Vec::new();
    let mut stderr = std::io::stderr().lock();
    for path in &list.paths {
        let path = Path::new(OsStr::from_bytes(path));
        match open_path(path) {
            Ok(file) => allowed.push(file),
            Err(error) => {
                let warning = Warning::new(format!(
                    "commands.allow `{}` is not allowed: {error}",
                    path.display()
                ));
                let _ = writeln!(stderr, "{warning}");
            }
        }
    }
    allowed.extend(open_path(Path::new(DYNAMIC_LINKER)).ok());
    allowed.extend(open_path(Path::new(FIRST_PROCESS)).ok());
    allowed
}

/// Replaces this process with the command, which sees `given` as its argv[0]; returns
/// only when that fails.
fn start(given: &OsString, path: &OsString, arguments: Vec<OsString>) -> ExitCode {
    let error = Command::new(path).arg0(given).args(arguments).exec();
    report(Diagnostic::new(
        Kind::CommandNotExecutable,
        format!("{}: {error}", Path::new(path).display()),
    ))
}

fn report(diagnostic: Diagnostic) -> ExitCode {
    let _ = writeln!(std::io::stderr(), "{diagnostic}");
    ExitCode::from(diagnostic.exit_code() as u8)
}
