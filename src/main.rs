use std::io::Write;
use std::process::ExitCode;

use process_wrap::cli::{self, Parsed};

fn main() -> ExitCode {
    let current_dir = match std::env::current_dir() {
        Ok(directory) => directory,
        Err(error) => {
            let _ = writeln!(
                std::io::stderr(),
                "process-wrap: path: the current directory cannot be determined: {error}"
            );
            return ExitCode::from(125);
        }
    };
    match cli::interpret(std::env::args_os().skip(1)).map(|parsed| match parsed {
        Parsed::Invocation(invocation) => Parsed::Invocation(invocation.anchored(&current_dir)),
        other => other,
    }) {
        Ok(Parsed::Help(text)) | Ok(Parsed::Version(text)) => {
            let _ = std::io::stdout().write_all(text.as_bytes());
            ExitCode::SUCCESS
        }
        // The launch itself is connected in a later plan; a valid invocation stops here.
        Ok(Parsed::Invocation(_)) => ExitCode::SUCCESS,
        Err(diagnostic) => {
            let _ = writeln!(std::io::stderr(), "{diagnostic}");
            ExitCode::from(diagnostic.exit_code() as u8)
        }
    }
}
