use std::io::Write;
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitCode};

use process_wrap::diagnostic::{Diagnostic, Kind, Warning};
use process_wrap::plan_text;
use process_wrap::startup::{self, Outcome};

fn main() -> ExitCode {
    match startup::prepare(std::env::args_os().skip(1)) {
        Ok(Outcome::Text(text)) => {
            let _ = std::io::stdout().write_all(text.as_bytes());
            ExitCode::SUCCESS
        }
        Ok(Outcome::Nested(nested)) => {
            print_warnings(std::slice::from_ref(&nested.warning));
            let command = match nested.command {
                Ok(command) => command,
                Err(diagnostic) => return exit_with(diagnostic),
            };
            // Only reached when the exec fails; the command was found a moment ago.
            let error = Command::new(&command).args(&nested.arguments).exec();
            exit_with(Diagnostic::new(
                Kind::CommandNotFound,
                format!("{}: {error}", command.display()),
            ))
        }
        Ok(Outcome::Prepared(prepared)) => {
            print_warnings(&prepared.plan.warnings);
            if prepared.invocation.print_plan {
                let _ = std::io::stdout().write_all(plan_text::render(&prepared.plan).as_bytes());
                return ExitCode::SUCCESS;
            }
            // The launch itself is connected next; a prepared run stops here.
            ExitCode::SUCCESS
        }
        Err(diagnostic) => exit_with(diagnostic),
    }
}

/// The diagnostic on standard error and its exit code (specification section 13).
fn exit_with(diagnostic: Diagnostic) -> ExitCode {
    let _ = writeln!(std::io::stderr(), "{diagnostic}");
    ExitCode::from(diagnostic.exit_code() as u8)
}

/// Warnings precede whatever follows them, one line each (specification section 13).
fn print_warnings(warnings: &[Warning]) {
    let mut stderr = std::io::stderr().lock();
    for warning in warnings {
        let _ = writeln!(stderr, "{warning}");
    }
}
