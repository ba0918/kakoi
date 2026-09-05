use std::io::Write;
use std::process::ExitCode;

use process_wrap::plan_text;
use process_wrap::startup::{self, Outcome};

fn main() -> ExitCode {
    match startup::prepare(std::env::args_os().skip(1)) {
        Ok(Outcome::Text(text)) => {
            let _ = std::io::stdout().write_all(text.as_bytes());
            ExitCode::SUCCESS
        }
        Ok(Outcome::Prepared(prepared)) if prepared.invocation.print_plan => {
            let _ = std::io::stdout().write_all(plan_text::render(&prepared.plan).as_bytes());
            ExitCode::SUCCESS
        }
        // The launch itself is connected next; a prepared run stops here.
        Ok(Outcome::Prepared(_)) => ExitCode::SUCCESS,
        Err(diagnostic) => {
            let _ = writeln!(std::io::stderr(), "{diagnostic}");
            ExitCode::from(diagnostic.exit_code() as u8)
        }
    }
}
