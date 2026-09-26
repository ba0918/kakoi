//! kakoi started as a command guard (specification REQ-446 to REQ-448 and REQ-453): when
//! the path this executable was started from is in the table the plan placed in the
//! isolation, the arguments and the environment are matched against the rules there, and
//! the run is either denied with one line and exit code 126 or handed to the real program
//! by exec. This is the outer layer; the rules and the matching are `kakoi-core`'s.

use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, ExitCode};

use kakoi_core::diagnostic::{Diagnostic, Kind};
use kakoi_core::guard::evaluate;
use kakoi_core::guard_placement::{GuardTable, GUARD_TABLE};

/// Acts as the guard when this executable was started from a path the table lists, and
/// returns the exit code; `None` otherwise, and kakoi goes on as itself.
pub fn run_if_guard() -> Option<ExitCode> {
    let started_from = std::fs::read_link("/proc/self/exe").ok()?;
    let table = GuardTable::from_bytes(&std::fs::read(GUARD_TABLE).ok()?)?;
    let entry = table.entry(&started_from)?;
    let mut arguments = std::env::args_os();
    let name = arguments.next().unwrap_or_default();
    let arguments: Vec<OsString> = arguments.collect();
    let environment: Vec<OsString> = std::env::vars_os().map(|(name, _)| name).collect();
    if let Some(denial) = evaluate(&entry.rules, &arguments, &environment) {
        return Some(report(Diagnostic::new(
            Kind::Guard,
            format!("{} {}: {}", denial.program, denial.matched, denial.reason),
        )));
    }
    // The real program replaces this process with the same name, arguments,
    // environment, and working directory; this returns only when the exec fails.
    let real = Path::new(OsStr::from_bytes(&entry.execute));
    let error = Command::new(real).arg0(name).args(arguments).exec();
    Some(report(Diagnostic::new(
        Kind::CommandNotExecutable,
        format!("{}: {error}", real.display()),
    )))
}

fn report(diagnostic: Diagnostic) -> ExitCode {
    let _ = writeln!(std::io::stderr(), "{diagnostic}");
    ExitCode::from(diagnostic.exit_code() as u8)
}
