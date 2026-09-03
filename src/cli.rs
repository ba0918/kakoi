//! Interpretation of the command line (specification section 4.1).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use clap::error::ErrorKind;
use clap::{Arg, ArgAction, CommandFactory, FromArgMatches, Parser};

use crate::diagnostic::Diagnostic;

/// The interpreted command line, with option paths made absolute against the current
/// directory. `command` is passed through untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub profile: String,
    pub policy_file: Option<PathBuf>,
    pub workspace: Option<PathBuf>,
    pub rw: Vec<PathBuf>,
    pub hide: Vec<PathBuf>,
    pub print_plan: bool,
    pub command: Vec<OsString>,
}

/// What the command line asks for: a run, the help text, or the version line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    Invocation(Invocation),
    Help(String),
    Version(String),
}

#[derive(Debug, Parser)]
#[command(
    name = "process-wrap",
    version,
    about = "Run a command inside a bubblewrap mount namespace shaped by a layered policy.",
    override_usage = "process-wrap [OPTIONS] -- COMMAND [ARGS]...\n       \
                      process-wrap [OPTIONS] --print-plan [-- COMMAND [ARGS]...]\n       \
                      process-wrap --version\n       \
                      process-wrap --help",
    color = clap::ColorChoice::Never,
    disable_help_flag = true,
    disable_version_flag = true
)]
struct Arguments {
    /// Profile name for the global scope.
    #[arg(long, value_name = "NAME", default_value = "default")]
    profile: String,

    /// Policy file for the process scope.
    #[arg(long, value_name = "PATH")]
    policy_file: Option<PathBuf>,

    /// Workspace. Defaults to the current directory.
    #[arg(long, value_name = "PATH")]
    workspace: Option<PathBuf>,

    /// An `rw` directive on the command-line layer. Repeatable.
    #[arg(long, value_name = "PATH", action = ArgAction::Append)]
    rw: Vec<PathBuf>,

    /// A `hide` directive on the command-line layer. Repeatable.
    #[arg(long, value_name = "PATH", action = ArgAction::Append)]
    hide: Vec<PathBuf>,

    /// Print the plan to standard output and exit without running the command.
    #[arg(long)]
    print_plan: bool,

    /// The command and its arguments, after `--`.
    #[arg(last = true, allow_hyphen_values = true, value_name = "COMMAND")]
    command: Vec<OsString>,
}

/// Interprets `arguments` (without the program name). Relative option paths are joined to
/// `current_dir`; `~` and variables are left as written.
pub fn interpret<I>(arguments: I, current_dir: &Path) -> Result<Parsed, Diagnostic>
where
    I: IntoIterator<Item = OsString>,
{
    let arguments: Vec<OsString> = arguments.into_iter().collect();
    let parser = Arguments::command()
        .arg(
            Arg::new("help")
                .long("help")
                .action(ArgAction::Help)
                .help("Print the usage and exit"),
        )
        .arg(
            Arg::new("version")
                .long("version")
                .action(ArgAction::Version)
                .help("Print the version and exit"),
        );
    let matches = match parser.try_get_matches_from(
        std::iter::once(OsString::from("process-wrap")).chain(arguments.iter().cloned()),
    ) {
        Ok(matches) => matches,
        Err(error) if error.kind() == ErrorKind::DisplayHelp => {
            return Ok(Parsed::Help(error.render().to_string()));
        }
        Err(error) if error.kind() == ErrorKind::DisplayVersion => {
            return Ok(Parsed::Version(error.render().to_string()));
        }
        Err(_) => return Err(Diagnostic::usage("invalid command line")),
    };
    let parsed = Arguments::from_arg_matches(&matches).expect("matches follow the definition");
    if parsed.command.is_empty() {
        if has_end_of_options(&arguments) {
            return Err(Diagnostic::usage("nothing follows `--`"));
        }
        if !parsed.print_plan {
            return Err(Diagnostic::usage(
                "COMMAND is required unless --print-plan is given",
            ));
        }
    }
    Ok(Parsed::Invocation(Invocation {
        profile: parsed.profile,
        policy_file: parsed.policy_file.map(|path| current_dir.join(path)),
        workspace: parsed.workspace.map(|path| current_dir.join(path)),
        rw: resolve_all(parsed.rw, current_dir),
        hide: resolve_all(parsed.hide, current_dir),
        print_plan: parsed.print_plan,
        command: parsed.command,
    }))
}

/// Whether a bare `--` appears. No option takes a value that may start with a hyphen, so the
/// first `--` is always the end-of-options marker.
fn has_end_of_options(arguments: &[OsString]) -> bool {
    arguments.iter().any(|argument| argument == "--")
}

fn resolve_all(paths: Vec<PathBuf>, current_dir: &Path) -> Vec<PathBuf> {
    paths
        .into_iter()
        .map(|path| current_dir.join(path))
        .collect()
}
