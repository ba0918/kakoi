//! Interpretation of the command line (specification section 4.1).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use clap::{Arg, ArgAction, CommandFactory, FromArgMatches, Parser, ValueEnum};

use kakoi_core::diagnostic::{is_control_character, Diagnostic};
use kakoi_core::layers::{LayerSelection, DEFAULT_PROFILE};

/// The interpreted command line. Option paths are as written until `anchored` joins the
/// relative ones to the current directory; `command` is passed through untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub profile: String,
    pub policy_file: Option<PathBuf>,
    pub workspace: Option<PathBuf>,
    pub rw: Vec<PathBuf>,
    pub hide: Vec<PathBuf>,
    /// The form of the plan to print; none when the command is to be run.
    pub print_plan: Option<PlanForm>,
    pub command: Vec<OsString>,
}

/// Which form `--print-plan` shows (specification section 13): the summary; the full
/// plan with the merged policy, the origin of every item, the whole environment, and the
/// bwrap argument list; or the same content as one line of JSON, for LLM agents and
/// tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PlanForm {
    Summary,
    Full,
    Json,
}

/// What the command line asks for: a run, the help text, or the version line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    Invocation(Invocation),
    /// `init [NAME]`: write the built-in default out as the profile `NAME`.
    Init(String),
    Help(String),
    Version(String),
}

#[derive(Debug, Parser)]
#[command(
    name = "kakoi",
    version,
    about = "Run a command inside a bubblewrap mount namespace shaped by a layered policy.",
    override_usage = "kakoi [OPTIONS] -- COMMAND [ARGS]...\n       \
                      kakoi [OPTIONS] --print-plan[=full|json] [-- COMMAND [ARGS]...]\n       \
                      kakoi init [NAME]\n       \
                      kakoi --version\n       \
                      kakoi --help",
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

    /// Print the plan and exit without running the command. `=full` adds the merged
    /// policy, the origin of every item, the whole environment, and the bwrap arguments;
    /// `=json` is the same as one line of JSON, for LLM agents and tools.
    #[arg(
        long,
        value_name = "FORM",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "summary",
        value_enum
    )]
    print_plan: Option<PlanForm>,

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

    /// What the written layers are read from: the profile, the `--policy-file`, and the
    /// command-line layer's `--rw` and `--hide`. Taken after `anchored`, so the paths are
    /// absolute.
    pub fn layer_selection(&self) -> LayerSelection {
        LayerSelection {
            profile: self.profile.clone(),
            policy_file: self.policy_file.clone(),
            rw: self.rw.clone(),
            hide: self.hide.clone(),
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
    if arguments.first().is_some_and(|word| word == "init") {
        return Ok(Parsed::Init(init_name(&arguments[1..])?));
    }
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
            std::iter::once(OsString::from("kakoi")).chain(arguments.iter().cloned()),
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
    if parsed.command.is_empty() && parsed.print_plan.is_none() {
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

/// The `NAME` of the `init` form, `default` when it is left out (specification
/// section 4.1). Read from the words as written, because the parser takes everything after
/// `--` as the command; `init` is exclusive, so any further word is an error. A word
/// starting with `-` is an option, which `init` does not combine with.
fn init_name(rest: &[OsString]) -> Result<String, Diagnostic> {
    let name = match rest {
        [] => return Ok(DEFAULT_PROFILE.to_string()),
        [name] => name,
        _ => return Err(Diagnostic::usage("init takes nothing but a profile name")),
    };
    if name.as_encoded_bytes().starts_with(b"-") {
        return Err(Diagnostic::usage("init cannot be combined with any option"));
    }
    let name = name
        .to_str()
        .ok_or_else(|| Diagnostic::usage("the profile name is not valid UTF-8"))?;
    if !is_single_path_component(name) {
        return Err(Diagnostic::usage(format!(
            "the profile name `{name}` is not a single path component"
        )));
    }
    Ok(name.to_string())
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

/// Whether `name` is one path component: not empty, without `/` and without a control
/// character, and neither `.` nor `..` (specification section 4.1). A control character is
/// refused because the path `init` writes is printed as it stands, unescaped, so a newline
/// in the name would break the one line the output is (section 13).
fn is_single_path_component(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.chars().any(is_control_character)
        && name != "."
        && name != ".."
}

fn anchor_all(paths: Vec<PathBuf>, current_dir: &Path) -> Vec<PathBuf> {
    paths
        .into_iter()
        .map(|path| current_dir.join(path))
        .collect()
}
