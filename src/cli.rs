//! Interpretation of the command line (specification section 4.1).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use clap::{Arg, ArgAction, CommandFactory, FromArgMatches, Parser};

use crate::diagnostic::Diagnostic;

/// The profile of the global scope when `--profile` is omitted (specification
/// section 4.1). Only this name falls back to the built-in default (section 5.3).
pub const DEFAULT_PROFILE: &str = "default";

/// The interpreted command line. Option paths are as written until `anchored` joins the
/// relative ones to the current directory; `command` is passed through untouched.
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
    #[arg(long, value_name = "NAME", default_value = DEFAULT_PROFILE)]
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

impl Invocation {
    /// Joins the relative option paths to `current_dir`. `~` and variables are left as
    /// written (specification section 4.1).
    pub fn anchored(self, current_dir: &Path) -> Self {
        Self {
            policy_file: self.policy_file.map(|path| current_dir.join(path)),
            workspace: self.workspace.map(|path| current_dir.join(path)),
            rw: anchor_all(self.rw, current_dir),
            hide: anchor_all(self.hide, current_dir),
            ..self
        }
    }
}

/// Interprets `arguments` (without the program name). Needs nothing from the host, so the
/// grammar is checked before the current directory is looked up (specification
/// section 13, stages 1 and 2).
pub fn interpret<I>(arguments: I) -> Result<Parsed, Diagnostic>
where
    I: IntoIterator<Item = OsString>,
{
    let arguments: Vec<OsString> = arguments.into_iter().collect();
    check_option_values(&arguments)?;
    // `--help` and `--version` are plain flags, so that the whole command line is checked
    // before either is answered (specification section 4.1).
    let mut parser = Arguments::command()
        .arg(
            Arg::new("help")
                .long("help")
                .action(ArgAction::SetTrue)
                .help("Print the usage and exit"),
        )
        .arg(
            Arg::new("version")
                .long("version")
                .action(ArgAction::SetTrue)
                .help("Print the version and exit"),
        );
    let matches = parser
        .try_get_matches_from_mut(
            std::iter::once(OsString::from("process-wrap")).chain(arguments.iter().cloned()),
        )
        .map_err(|_| Diagnostic::usage("invalid command line"))?;
    let parsed = Arguments::from_arg_matches(&matches).expect("matches follow the definition");
    if parsed.command.is_empty() && has_end_of_options(&arguments) {
        return Err(Diagnostic::usage("nothing follows `--`"));
    }
    if matches.get_flag("help") || matches.get_flag("version") {
        if arguments.len() != 1 {
            return Err(Diagnostic::usage(
                "--help and --version cannot be combined with any other argument",
            ));
        }
        if matches.get_flag("help") {
            return Ok(Parsed::Help(parser.render_help().to_string()));
        }
        return Ok(Parsed::Version(parser.render_version()));
    }
    if parsed.command.is_empty() && !parsed.print_plan {
        return Err(Diagnostic::usage(
            "COMMAND is required unless --print-plan is given",
        ));
    }
    if !is_single_path_component(&parsed.profile) {
        return Err(Diagnostic::usage(format!(
            "the profile name `{}` is not a single path component",
            parsed.profile
        )));
    }
    Ok(Parsed::Invocation(Invocation {
        profile: parsed.profile,
        policy_file: parsed.policy_file,
        workspace: parsed.workspace,
        rw: parsed.rw,
        hide: parsed.hide,
        print_plan: parsed.print_plan,
        command: parsed.command,
    }))
}

const OPTIONS_WITH_A_VALUE: [&str; 5] = [
    "--profile",
    "--policy-file",
    "--workspace",
    "--rw",
    "--hide",
];

/// In the separated form `--opt VALUE`, the value must follow and must not start with `-`;
/// in either form it must not be empty (specification section 4.1). Checked on the words as
/// written, because after parsing `--opt=-x` and `--opt -x` look the same.
fn check_option_values(arguments: &[OsString]) -> Result<(), Diagnostic> {
    let mut words = arguments.iter().take_while(|word| *word != "--");
    while let Some(word) = words.next() {
        let Some(word) = word.to_str() else {
            continue;
        };
        if OPTIONS_WITH_A_VALUE.contains(&word) {
            match words.next() {
                Some(value) if !value.is_empty() && !value.as_encoded_bytes().starts_with(b"-") => {
                }
                _ => {
                    return Err(Diagnostic::usage(format!(
                        "{word} needs a value that is not empty and does not start with `-`"
                    )));
                }
            }
            continue;
        }
        if let Some((option, value)) = word.split_once('=') {
            if OPTIONS_WITH_A_VALUE.contains(&option) && value.is_empty() {
                return Err(Diagnostic::usage(format!(
                    "{option} needs a value that is not empty"
                )));
            }
        }
    }
    Ok(())
}

/// Whether a bare `--` appears. No option takes a value that may start with a hyphen, so the
/// first `--` is always the end-of-options marker.
fn has_end_of_options(arguments: &[OsString]) -> bool {
    arguments.iter().any(|argument| argument == "--")
}

/// Whether `name` is one path component: not empty, without `/`, and neither `.` nor `..`
/// (specification section 4.1).
fn is_single_path_component(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && name != "." && name != ".."
}

fn anchor_all(paths: Vec<PathBuf>, current_dir: &Path) -> Vec<PathBuf> {
    paths
        .into_iter()
        .map(|path| current_dir.join(path))
        .collect()
}
