//! Stages 4 to 9 of specification section 13 from values: the home directory, the policy
//! files and their merge, the workspace and the variables, the mount resolution, the
//! whereabouts of `bwrap`, the command, and the plan. The environment and the current
//! directory arrive as arguments, so nothing here asks the process for them; the file
//! system is read here and the facts are handed to the pure functions. The first
//! diagnostic ends the run.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use crate::command::{command_candidates, resolve_command};
use crate::copy_facts::read_copy_sources;
use crate::diagnostic::Diagnostic;
use crate::environment::{HostEnvironment, RealEntry};
use crate::executables::{file_id, first_executable, named};
use crate::guard_placement::{place_guards, real_program, GuardPlan, PlacedGuard, GUARD_LOCATION};
use crate::layers::{load_layers, merge, LayerSelection, Policy};
use crate::mount_facts::collect_mount_facts;
use crate::mounts::{candidates, expand_policy, ResolvedItem};
use crate::plan::{
    self, resolve_isolation, Inputs, IsolationFacts, Plan, ResolvedCommand, TUN_DEVICE,
};
use crate::policy::ListMode;
use crate::secret_facts::read_secret_files;
use crate::shared_files;
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
    /// The path of kakoi's own executable, which each guard is; none when it cannot be
    /// told.
    pub executable: Option<PathBuf>,
    /// Whether the run is nested: the nesting mark was there when it started
    /// (specification REQ-455).
    pub nested: bool,
    /// Whether the plan is used: outside an isolation, or inside one with
    /// `--nested=isolate` (specification REQ-457).
    pub applied: bool,
    /// Whether the command guards of the run around this nested one are handed on: they
    /// were there when it started (specification REQ-465).
    pub outer_guard: bool,
}

/// Runs stages 4 to 9 for `request` and returns the plan.
pub fn plan_for(request: &Request) -> Result<Plan, Diagnostic> {
    let env = HostEnvironment::from_variables(&request.host);
    let home = env.home_directory(&env.home.as_deref().map_or(RealEntry::Missing, real_entry))?;
    let config_dir = env.config_dir(&home);
    let layers = load_layers(&request.layers, &config_dir)?;
    let policy = merge(&layers)?;
    // The guards of both runs cannot be laid one over the other (specification REQ-466).
    if request.outer_guard && !policy.guards.is_empty() {
        return Err(Diagnostic::policy(
            "the command guards of the isolation around this one are handed on, so the \
             policy of a nested isolation cannot place guards of its own",
        ));
    }
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
    let shared_files = shared_files::place(&request.host, request.nested);
    let wanted = candidates(
        &expanded,
        &layers,
        &variables,
        protected_config_dir,
        request.workspace.as_deref(),
    )
    .with_protected(shared_files.as_deref())
    .with_walked(&listed_lookups(&policy));
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
        nested: request.nested,
        applied: request.applied,
        shared_files: shared_files.as_deref(),
        outer_guard: request.outer_guard,
    };
    let mut isolation = resolve_isolation(&inputs, &facts)?;
    let guards = plan_guards(
        &policy,
        &mut isolation,
        request.executable.as_deref(),
        request.outer_guard,
    )?;
    // The last of stage 7: what each `rw-copy` item that applies starts the isolation
    // with, read only for the items that survived the resolution and the checks.
    let copies = read_copy_sources(&isolation.mounts.items)?;
    let bwrap = locate_bwrap(&request.host)?;
    // With `bwrap`, since the device is shown by it (specification REQ-458).
    if policy.allow_nested_filtered && Path::new(TUN_DEVICE).symlink_metadata().is_err() {
        return Err(Diagnostic::bwrap(format!(
            "network.allow-nested-filtered shows {TUN_DEVICE} inside, but the host has none"
        )));
    }
    // A plan that is only shown, that of a nested run without `--nested=isolate`,
    // resolves on the host's `PATH` as that run would (specification REQ-261).
    let search_in = if request.applied {
        isolation.environment.values()
    } else {
        &request.host
    };
    let command = match request.command.split_first() {
        None => None,
        Some((command, arguments)) => Some(ResolvedCommand {
            command: command.clone(),
            arguments: arguments.to_vec(),
            path: through_guard(
                command,
                locate_command(command, search_in)?,
                path_of(search_in),
                &isolation.mounts.items,
                &guards,
            ),
        }),
    };
    Ok(plan::plan(
        &inputs, isolation, copies, bwrap, command, guards,
    ))
}

/// The paths the "listed" mount mode looks up besides the policy's own.
fn listed_lookups(policy: &Policy) -> Vec<PathBuf> {
    if policy.mounts_mode != ListMode::Listed {
        return Vec::new();
    }
    crate::listed::lookups(policy.mounts_system, policy.network_mode)
}

/// The guards of the merged rules, found on the isolation's `PATH` after the mounts are
/// resolved; the guard location goes first on that `PATH` when a guard is placed, or when
/// the guards of the run around a nested one are handed on, since the nested policy may
/// have set `PATH` afresh or put entries before it (specification REQ-446, REQ-449, and
/// REQ-465).
fn plan_guards(
    policy: &Policy,
    isolation: &mut plan::Isolation,
    executable: Option<&Path>,
    outer_guard: bool,
) -> Result<GuardPlan, Diagnostic> {
    let path = path_of(isolation.environment.values());
    let facts = policy
        .guards
        .iter()
        .map(|entry| {
            let program = &entry.rule.program;
            let names = named(&real_candidates(OsStr::new(program), path));
            (program.clone(), names)
        })
        .collect();
    let kakoi = executable.and_then(|path| std::fs::canonicalize(path).ok());
    let guards = place_guards(
        &policy.guards,
        path.is_some(),
        &facts,
        &isolation.mounts.items,
        kakoi.as_deref(),
        kakoi.as_deref().and_then(file_id),
    );
    if !guards.placed.is_empty() && kakoi.is_none() {
        return Err(Diagnostic::bwrap(
            "the executable of kakoi itself, which each command guard is, cannot be located",
        ));
    }
    if !guards.placed.is_empty() || (outer_guard && !guard_location_is_first(path)) {
        isolation
            .environment
            .put_first_on_path(Path::new(GUARD_LOCATION));
    }
    Ok(guards)
}

/// Whether `path` already starts with the guard location, as the `PATH` a nested run
/// inherits from the run around it does.
fn guard_location_is_first(path: Option<&OsStr>) -> bool {
    path.and_then(|path| path.as_bytes().split(|&byte| byte == b':').next())
        == Some(GUARD_LOCATION.as_bytes())
}

/// Where the real `program` is looked for: the absolute entries of `path` other than the
/// guard location, which a nested run inherits from the run around it (specification
/// REQ-446). A relative entry, the empty one included, names a different place for every
/// directory the program is started from, so no one real program stands behind it.
fn real_candidates(program: &OsStr, path: Option<&OsStr>) -> Vec<PathBuf> {
    command_candidates(program, path)
        .into_iter()
        .filter(|candidate| {
            candidate.is_absolute() && candidate.parent() != Some(Path::new(GUARD_LOCATION))
        })
        .collect()
}

/// The command a run starts: a name whose real program, looked up on `path` as a guard
/// looks it up, has a guard is started through the guard, as the same name started
/// inside would be; otherwise `found`. A path with `/` is left as it is.
fn through_guard(
    command: &OsStr,
    found: PathBuf,
    path: Option<&OsStr>,
    mounts: &[ResolvedItem],
    guards: &GuardPlan,
) -> PathBuf {
    if command.as_bytes().contains(&b'/') {
        return found;
    }
    let names = named(&real_candidates(command, path));
    let Some(real) = real_program(&names, mounts) else {
        return found;
    };
    guards
        .placed
        .iter()
        .find(|guard| guard.found == real.candidate)
        .map_or(found, PlacedGuard::guard)
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
