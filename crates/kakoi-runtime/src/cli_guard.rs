//! kakoi started as a command guard (specification REQ-446 to REQ-448 and REQ-453): when
//! the path this executable was started from is in the table the plan placed in the
//! isolation, the arguments and the environment are matched against the rules there, and
//! the run is either denied with one line and exit code 126 or handed to the real program
//! by exec. Role detection differs from embedded images; execution is shared below.

use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, ExitCode};

use crate::cli::diagnostic::{Diagnostic, Kind};
use crate::cli::guard::evaluate;
use crate::cli::guard_placement::{GuardTable, TableEntry, GUARD_TABLE};

/// Acts as the guard when this executable was started from a path the table lists, and
/// returns the exit code; `None` otherwise, and kakoi goes on as itself.
pub fn run_if_guard() -> Option<ExitCode> {
    let started_from = std::fs::read_link("/proc/self/exe").ok()?;
    let table = std::fs::read(GUARD_TABLE)
        .ok()
        .and_then(|bytes| GuardTable::from_bytes(&bytes));
    let Some(table) = table else {
        // A guard is started from the place of the program it stands for, so under
        // another name, and inside an isolation, which sets `KAKOI`; kakoi itself is
        // started as `kakoi`, or anywhere outside. Going on as kakoi would answer
        // `git --version` with kakoi's version (specification REQ-453).
        if started_from.file_name() == Some(OsStr::new("kakoi"))
            || std::env::var_os("KAKOI").as_deref() != Some(OsStr::new("1"))
        {
            return None;
        }
        return Some(report(Diagnostic::new(
            Kind::Guard,
            format!(
                "{}: the table of the command guards at {GUARD_TABLE} cannot be read, so \
                 this guard cannot tell what to run; a sandbox that makes /dev anew inside \
                 the isolation hides it",
                started_from.display()
            ),
        )));
    };
    let entry = table.entry(&started_from)?;
    let mut command = match command(entry) {
        Ok(command) => command,
        Err(diagnostic) => return Some(report(diagnostic)),
    };
    let error = command.exec();
    Some(report(Diagnostic::new(
        Kind::CommandNotExecutable,
        format!(
            "{}: {error}",
            Path::new(OsStr::from_bytes(&entry.execute)).display()
        ),
    )))
}

pub(crate) fn command(entry: &TableEntry) -> Result<Command, Diagnostic> {
    let mut arguments = std::env::args_os();
    let name = arguments.next().unwrap_or_default();
    let arguments: Vec<OsString> = arguments.collect();
    let environment: Vec<OsString> = std::env::vars_os().map(|(name, _)| name).collect();
    if let Some(denial) = evaluate(&entry.rules, &arguments, &environment) {
        return Err(Diagnostic::new(
            Kind::Guard,
            format!("{} {}: {}", denial.program, denial.matched, denial.reason),
        ));
    }
    // The real program replaces this process with the same name, arguments,
    // environment, and working directory; this returns only when the exec fails.
    let real = Path::new(OsStr::from_bytes(&entry.execute));
    let mut command = Command::new(real);
    command.arg0(name).args(arguments);
    Ok(command)
}

fn report(diagnostic: Diagnostic) -> ExitCode {
    let _ = writeln!(std::io::stderr(), "{diagnostic}");
    ExitCode::from(diagnostic.exit_code() as u8)
}
