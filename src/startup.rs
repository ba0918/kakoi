//! The start-up, in the order of the checks of specification section 13: the `--help` and
//! `--version` forms, the grammar, the current directory, the home directory, the policy
//! files and their merge, the workspace and the variables, the mount resolution, the
//! whereabouts of `bwrap`, and the command. The first diagnostic ends the run. This is
//! the outer layer: it reads the environment and the file system and hands the facts to
//! the pure functions.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use crate::cli::{self, Invocation, Parsed};
use crate::command::{command_candidates, resolve_command};
use crate::diagnostic::{Diagnostic, Warning};
use crate::environment::{HostEnvironment, RealEntry};
use crate::executables::first_executable;
use crate::layers::{load_layers, merge};
use crate::mount_facts::collect_mount_facts;
use crate::mounts::{candidates, expand_policy};
use crate::plan::{self, is_nested, resolve_isolation, Inputs, IsolationFacts, Plan};
use crate::secret_facts::read_secret_files;
use crate::variables::derive_variables;
use crate::workspace_facts::{collect_workspace_facts, real_entry};

/// What the start-up ends with: text to print (the usage or the version), a nested run
/// that goes straight to the command, or everything the start needs.
#[derive(Debug)]
pub enum Outcome {
    Text(String),
    Nested(Nested),
    Prepared(Box<Prepared>),
}

/// A nested run (specification section 12.1): the warning to print first, then the
/// command resolved on the host's `PATH` or the `command not found` diagnostic, and the
/// arguments as given. The environment is left as it is.
#[derive(Debug)]
pub struct Nested {
    pub warning: Warning,
    pub command: Result<PathBuf, Diagnostic>,
    pub arguments: Vec<OsString>,
}

/// The results of stages 3 to 9.
#[derive(Debug)]
pub struct Prepared {
    pub invocation: Invocation,
    pub current_dir: PathBuf,
    pub plan: Plan,
}

/// Runs stages 1 to 9 on `arguments` (without the program name).
pub fn prepare<I>(arguments: I) -> Result<Outcome, Diagnostic>
where
    I: IntoIterator<Item = OsString>,
{
    // Stages 1 and 2 need nothing from the host.
    let invocation = match cli::interpret(arguments)? {
        Parsed::Help(text) | Parsed::Version(text) => return Ok(Outcome::Text(text)),
        Parsed::Invocation(invocation) => invocation,
    };
    let host: BTreeMap<OsString, OsString> = std::env::vars_os().collect();
    // Stage 2, nested without `--print-plan`: stages 3 to 8 are skipped, nothing is read
    // and nothing changed, and the command is resolved on the host's `PATH`
    // (specification sections 12.1 and 13).
    if is_nested(&host) && !invocation.print_plan {
        return Ok(Outcome::Nested(Nested {
            warning: nested_warning(),
            command: locate_command(&invocation.command[0], &host),
            arguments: invocation.command[1..].to_vec(),
        }));
    }
    let current_dir = std::env::current_dir().map_err(|error| {
        Diagnostic::path(format!(
            "the current directory cannot be determined: {error}"
        ))
    })?;
    let invocation = invocation.anchored(&current_dir);
    let env = HostEnvironment::from_process();
    let home = env.home_directory(&env.home.as_deref().map_or(RealEntry::Missing, real_entry))?;
    let config_dir = env.config_dir(&home);
    let layers = load_layers(&invocation, &config_dir)?;
    let policy = merge(&layers)?;
    let workspace = invocation
        .workspace
        .clone()
        .unwrap_or_else(|| current_dir.clone());
    let facts = collect_workspace_facts(&workspace);
    let variables = derive_variables(&real_entry(&config_dir), &facts)?;
    // Stage 7: the core names the paths to look up, the outer layer looks them up.
    let expanded = expand_policy(&policy, &variables, &home);
    let wanted = candidates(
        &expanded,
        &layers,
        &variables,
        &config_dir,
        invocation.workspace.as_deref(),
    );
    let facts = IsolationFacts {
        mounts: collect_mount_facts(&wanted),
        secrets: read_secret_files(&expanded.secrets),
    };
    let inputs = Inputs {
        layers: &layers,
        policy: &policy,
        expanded: &expanded,
        variables: &variables,
        home: &home,
        config_dir: &config_dir,
        workspace: invocation.workspace.as_deref(),
        current_dir: &current_dir,
        host: &host,
    };
    let isolation = resolve_isolation(&inputs, &facts)?;
    let bwrap = locate_bwrap(&host)?;
    // A nested run resolves on the host's `PATH` rather than the isolation's
    // (specification section 4.2).
    let search_in = if is_nested(&host) {
        &host
    } else {
        isolation.environment.values()
    };
    let command = match invocation.command.first() {
        None => None,
        Some(command) => Some(locate_command(command, search_in)?),
    };
    let plan = plan::plan(&inputs, isolation, bwrap, command);
    Ok(Outcome::Prepared(Box::new(Prepared {
        invocation,
        current_dir,
        plan,
    })))
}

/// The one line a nested run prints (specification section 12.1).
fn nested_warning() -> Warning {
    Warning::new(
        "PROCESS_WRAP=1: already inside an isolation, so the policy is not applied and the \
         command runs under the outer boundary",
    )
}

/// Stage 8: `bwrap` on the host's `PATH` (specification section 14).
fn locate_bwrap(host: &BTreeMap<OsString, OsString>) -> Result<PathBuf, Diagnostic> {
    first_executable(&command_candidates(OsStr::new("bwrap"), path_of(host)))
        .ok_or_else(|| Diagnostic::bwrap("bwrap is not on the host's PATH"))
}

/// Stage 9: the command on the `PATH` of `environment` (specification section 4.2).
fn locate_command(
    command: &OsStr,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<PathBuf, Diagnostic> {
    let found = first_executable(&command_candidates(command, path_of(environment)));
    resolve_command(command, found)
}

fn path_of(environment: &BTreeMap<OsString, OsString>) -> Option<&OsStr> {
    environment.get(OsStr::new("PATH")).map(OsString::as_os_str)
}
