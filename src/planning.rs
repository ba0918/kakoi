//! Stages 4 to 9 of specification section 13 from values: the home directory, the policy
//! files and their merge, the workspace and the variables, the mount resolution, the
//! whereabouts of `bwrap`, the command, and the plan. The environment and the current
//! directory arrive as arguments, so nothing here asks the process for them; the file
//! system is read here and the facts are handed to the pure functions. The first
//! diagnostic ends the run.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use crate::command::{command_candidates, resolve_command};
use crate::copy_facts::read_copy_sources;
use crate::diagnostic::Diagnostic;
use crate::environment::{HostEnvironment, RealEntry};
use crate::executables::first_executable;
use crate::layers::{load_layers, merge, LayerSelection};
use crate::mount_facts::collect_mount_facts;
use crate::mounts::{candidates, expand_policy};
use crate::plan::{
    self, is_nested, resolve_isolation, Inputs, IsolationFacts, Plan, ResolvedCommand,
};
use crate::secret_facts::read_secret_files;
use crate::variables::derive_variables;
use crate::workspace_facts::{collect_workspace_facts, probe_path, real_entry};

/// What a run asks for once the command line is interpreted and its paths are anchored:
/// where the written layers come from, the `--workspace` path as given (made absolute)
/// or none, the command and its arguments (empty when `--print-plan` leaves the command
/// out), the current directory, and a copy of the host's environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub layers: LayerSelection,
    pub workspace: Option<PathBuf>,
    pub command: Vec<OsString>,
    pub current_dir: PathBuf,
    pub host: BTreeMap<OsString, OsString>,
}

/// Runs stages 4 to 9 for `request` and returns the plan.
pub fn plan_for(request: &Request) -> Result<Plan, Diagnostic> {
    let env = HostEnvironment::from_variables(&request.host);
    let home = env.home_directory(&env.home.as_deref().map_or(RealEntry::Missing, real_entry))?;
    let config_dir = env.config_dir(&home);
    let layers = load_layers(&request.layers, &config_dir)?;
    let policy = merge(&layers)?;
    let workspace = request
        .workspace
        .clone()
        .unwrap_or_else(|| request.current_dir.clone());
    let facts = collect_workspace_facts(&workspace);
    let variables = derive_variables(&probe_path(&config_dir), &facts)?;
    // The configuration directory is a protected path whether or not anything is at the
    // name: what does not exist yet can be made from inside the isolation and a profile
    // planted there, so its existing prefixes are checked (specification section 5.6).
    let protected_config_dir = config_dir.as_path();
    // Stage 7: the core names the paths to look up, the outer layer looks them up.
    let expanded = expand_policy(&policy, &variables, &home);
    let wanted = candidates(
        &expanded,
        &layers,
        &variables,
        protected_config_dir,
        request.workspace.as_deref(),
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
        config_dir: protected_config_dir,
        workspace: request.workspace.as_deref(),
        current_dir: &request.current_dir,
        host: &request.host,
    };
    let isolation = resolve_isolation(&inputs, &facts)?;
    // The last of stage 7: what each `rw-copy` item that applies starts the isolation
    // with, read only for the items that survived the resolution and the checks.
    let copies = read_copy_sources(&isolation.mounts.items)?;
    let bwrap = locate_bwrap(&request.host)?;
    // A nested run resolves on the host's `PATH` rather than the isolation's
    // (specification section 4.2).
    let search_in = if is_nested(&request.host) {
        &request.host
    } else {
        isolation.environment.values()
    };
    let command = match request.command.split_first() {
        None => None,
        Some((command, arguments)) => Some(ResolvedCommand {
            command: command.clone(),
            arguments: arguments.to_vec(),
            path: locate_command(command, search_in)?,
        }),
    };
    Ok(plan::plan(&inputs, isolation, copies, bwrap, command))
}

/// Stage 8: `bwrap` on the host's `PATH` (specification section 14).
fn locate_bwrap(host: &BTreeMap<OsString, OsString>) -> Result<PathBuf, Diagnostic> {
    first_executable(&command_candidates(OsStr::new("bwrap"), path_of(host)))
        .ok_or_else(|| Diagnostic::bwrap("bwrap is not on the host's PATH"))
}

/// Stage 9: the command on the `PATH` of `environment` (specification section 4.2).
pub fn locate_command(
    command: &OsStr,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<PathBuf, Diagnostic> {
    let found = first_executable(&command_candidates(command, path_of(environment)));
    resolve_command(command, found)
}

fn path_of(environment: &BTreeMap<OsString, OsString>) -> Option<&OsStr> {
    environment.get(OsStr::new("PATH")).map(OsString::as_os_str)
}
