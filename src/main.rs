use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, ExitCode};

use kakoi::diagnostic::{Diagnostic, Kind, Warning};
use kakoi::init;
use kakoi::launch::{self, BwrapCommand};
use kakoi::plan_text;
use kakoi::startup::{self, Outcome};

fn main() -> ExitCode {
    match startup::prepare(std::env::args_os().skip(1)) {
        Ok(Outcome::Text(text)) => {
            let _ = std::io::stdout().write_all(text.as_bytes());
            ExitCode::SUCCESS
        }
        Ok(Outcome::Init(request)) => match init::write_built_in_default(&request) {
            Ok(path) => {
                // The path as assembled, byte for byte, so that `$EDITOR "$(kakoi
                // init)"` opens it (specification section 4.1).
                let mut stdout = std::io::stdout();
                let _ = stdout.write_all(path.as_os_str().as_bytes());
                let _ = stdout.write_all(b"\n");
                ExitCode::SUCCESS
            }
            Err(diagnostic) => exit_with(diagnostic),
        },
        Ok(Outcome::Nested(nested)) => {
            print_warnings(std::slice::from_ref(&nested.warning));
            let command = match nested.command {
                Ok(command) => command,
                Err(diagnostic) => return exit_with(diagnostic),
            };
            // The process sees `COMMAND` as given as its argv[0]; the resolved path is only
            // what is executed (specification section 4.2). Only reached when the exec
            // fails; the command was found a moment ago, so this is `command not
            // executable`, exit code 126 (section 12.1).
            let error = Command::new(&command)
                .arg0(&nested.given)
                .args(&nested.arguments)
                .exec();
            exit_with(Diagnostic::new(
                Kind::CommandNotExecutable,
                format!("{}: {error}", command.display()),
            ))
        }
        Ok(Outcome::Prepared(prepared)) => {
            print_warnings(&prepared.plan.warnings);
            if let Some(form) = prepared.invocation.print_plan {
                let text = plan_text::render(&prepared.plan, form);
                let _ = std::io::stdout().write_all(text.as_bytes());
                return ExitCode::SUCCESS;
            }
            match launch::assemble(&prepared.plan) {
                Ok(bwrap) => exit_with(execute(bwrap)),
                Err(diagnostic) => exit_with(diagnostic),
            }
        }
        Err(diagnostic) => exit_with(diagnostic),
    }
}

/// Executes `bwrap` in place (specification section 13, stage 10). Returns only when the
/// exec itself fails, with the `bwrap` diagnostic; the descriptors stay open until then.
fn execute(mut bwrap: BwrapCommand) -> Diagnostic {
    let error = bwrap.command.exec();
    Diagnostic::bwrap(format!(
        "{} could not be executed: {error}",
        Path::new(bwrap.command.get_program()).display()
    ))
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
