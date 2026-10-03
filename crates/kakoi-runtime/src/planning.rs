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
use crate::command_limits::{
    self, allowed_programs, outer_guards, outer_overlaid, relocated_programs, CommandLimits,
};
use crate::copy_facts::read_copy_sources;
use crate::diagnostic::Diagnostic;
use crate::environment::{HostEnvironment, RealEntry};
use crate::executables::{file_id, first_executable, named};
use crate::guard_placement::{
    name_hidden, place_guards, real_program, GuardPlan, GuardTable, NameFact, PlacedGuard,
    GUARD_LOCATION, GUARD_TABLE,
};
use crate::layers::{load_layers, merge, LayerSelection, Policy};
use crate::listed::ListedRoot;
use crate::mount_facts::{collect_generator_facts, collect_path_facts};
use crate::mounts::{candidates, expand_policy, ResolvedItem};
use crate::placement::{protected_paths, ProtectedPaths};
use crate::plan::{
    self, resolve_isolation, Inputs, IsolationFacts, Plan, ResolvedCommand, TUN_DEVICE,
};
use crate::policy::{ListMode, NetworkMode};
use crate::secret_facts::read_secret_files;
use crate::shared_files;
use crate::variables::derive_variables;
use crate::workspace_facts::{collect_workspace_facts, probe_path, real_entry};
pub use kakoi_linux::command_location::locate_command;

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
    /// What the table of those guards holds; none when it cannot be read.
    pub outer_table: Option<Vec<u8>>,
    /// The host's Landlock ABI version; none when it has no Landlock.
    pub landlock_abi: Option<u32>,
}

/// Runs stages 4 to 9 for `request` and returns the plan.
pub fn plan_for(request: &Request) -> Result<Plan, Diagnostic> {
    let env = HostEnvironment::from_variables(&request.host);
    let home = env.home_directory(&env.home.as_deref().map_or(RealEntry::Missing, real_entry))?;
    let config_dir = env.config_dir(&home);
    let layers = load_layers(&request.layers, &config_dir)?;
    let policy = merge(&layers)?;
    plan_with_context(request, &layers, &policy, home, config_dir, true)
}

pub(crate) fn plan_with_policy(
    request: &Request,
    layers: &[crate::layers::Layer],
    policy: &Policy,
) -> Result<Plan, Diagnostic> {
    let env = HostEnvironment::from_variables(&request.host);
    let home = env.home_directory(&env.home.as_deref().map_or(RealEntry::Missing, real_entry))?;
    let config_dir = env.config_dir(&home);
    plan_with_context(request, layers, policy, home, config_dir, false)
}

fn plan_with_context(
    request: &Request,
    layers: &[crate::layers::Layer],
    policy: &Policy,
    home: crate::environment::HomeDirectory,
    config_dir: PathBuf,
    exclude_self: bool,
) -> Result<Plan, Diagnostic> {
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
    let expanded = expand_policy(policy, &variables, &home);
    let shared_files = shared_files::place(&request.host, request.nested);
    let protected = ProtectedPaths {
        shared_files: shared_files.clone(),
        ..protected_paths(&expanded, layers, protected_config_dir)
    };
    let wanted = candidates(
        &expanded,
        &protected.paths(),
        &variables,
        request.workspace.as_deref(),
    )
    .with_walked(&listed_lookups(policy))
    .with_walked(&command_limits::lookups(policy, &variables, &home));
    let inputs = Inputs {
        layers,
        policy,
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
    // What the scans find and the mount list are gathered only for the scans and the
    // `hide-mounts` that apply (specification REQ-468).
    let mut mounts = collect_path_facts(&wanted);
    let (generating, _) = plan::generating_policy(&inputs, &mounts)?;
    collect_generator_facts(&wanted.for_generators(&generating), &mut mounts);
    let facts = IsolationFacts {
        mounts,
        secrets: read_secret_files(&expanded.secrets),
    };
    let mut isolation = resolve_isolation(&inputs, &facts)?;
    let guards = plan_guards(
        policy,
        &mut isolation,
        request.executable.as_deref(),
        request.outer_guard,
        exclude_self,
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
    // With `bwrap`, since the scope is a fact of the host as bwrap is (specification
    // REQ-472).
    if policy.mounts_mode == ListMode::Listed
        && policy.network_mode == NetworkMode::Host
        && !request
            .landlock_abi
            .is_some_and(|abi| abi >= crate::landlock::SCOPE_ABI)
    {
        return Err(Diagnostic::bwrap(format!(
            "the \"listed\" mount mode with the host network needs the Landlock scope on \
             abstract UNIX sockets (ABI {}), which the host does not have",
            crate::landlock::SCOPE_ABI
        )));
    }
    // With `bwrap`, as the scope above (specification REQ-477).
    if policy.commands_mode == ListMode::Listed && request.landlock_abi.is_none() {
        return Err(Diagnostic::bwrap(
            "the \"listed\" command mode needs Landlock, which the host does not have",
        ));
    }
    let commands = (policy.commands_mode == ListMode::Listed)
        .then(|| {
            command_limits(
                policy,
                &variables,
                &home,
                &facts.mounts,
                &isolation,
                &guards,
                request,
            )
        })
        .transpose()?;
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
                if request.applied {
                    locate_shown_command(
                        command,
                        search_in,
                        &isolation.mounts.items,
                        isolation.listed.as_ref(),
                    )?
                } else {
                    locate_command(command, search_in)?
                },
                path_of(search_in),
                &isolation.mounts.items,
                isolation.listed.as_ref(),
                &guards,
            ),
        }),
    };
    let provisions = plan::provisions(&inputs, &isolation, commands.clone());
    let (arguments, layout) = kakoi_linux::bwrap_arguments::bwrap_arguments_with_layout(
        policy.network_mode,
        &request.current_dir,
        &isolation.mounts.items,
        &copies,
        &guards,
        &provisions,
        command.as_ref(),
    );
    let launch = plan::LaunchDescription {
        program: bwrap,
        arguments,
        layout,
    };
    Ok(plan::plan(
        &inputs, isolation, copies, launch, command, guards, commands,
    ))
}

/// The programs the "listed" command mode lets start, and kakoi's own executable, which
/// the isolation's first process is (specification REQ-475 and REQ-476). Under the guards
/// of the run around a nested one, those guards are allowed too, and an allowed item a
/// guard of theirs overlays stands for the real program they relocated (specification
/// REQ-485).
fn command_limits(
    policy: &Policy,
    variables: &crate::variables::Variables,
    home: &crate::environment::HomeDirectory,
    facts: &crate::mounts::MountFacts,
    isolation: &plan::Isolation,
    guards: &GuardPlan,
    request: &Request,
) -> Result<CommandLimits, Diagnostic> {
    let executable = request
        .executable
        .as_deref()
        .and_then(crate::mount_facts::real_path)
        .ok_or_else(|| {
            Diagnostic::bwrap(
                "the executable of kakoi itself, which the isolation's first process is, \
                 cannot be located",
            )
        })?;
    let (allowed, skipped) = allowed_programs(
        policy,
        variables,
        home,
        facts,
        &isolation.mounts.items,
        isolation.listed.as_ref(),
    );
    let reals: Vec<PathBuf> = allowed
        .iter()
        .filter_map(|path| facts.entry(path).path().map(Path::to_path_buf))
        .collect();
    let outer_table = if request.outer_guard {
        outer_guard_table(request.outer_table.as_deref())?
    } else {
        GuardTable::default()
    };
    let overlaid = [guards.overlaid.clone(), outer_overlaid(&outer_table)].concat();
    let relocated = relocated_programs(&reals, &overlaid);
    Ok(CommandLimits {
        allowed,
        skipped,
        relocated,
        outer_guards: outer_guards(&outer_table),
        executable,
    })
}

/// The table of the guards of the run around a nested one, from what it holds (`bytes`,
/// none when it cannot be read). Without it the outer guards are denied inside and a
/// shell would start the next program on `PATH`, the real one, past their rules, so the
/// run stops (specification REQ-485).
fn outer_guard_table(bytes: Option<&[u8]>) -> Result<GuardTable, Diagnostic> {
    bytes.and_then(GuardTable::from_bytes).ok_or_else(|| {
        Diagnostic::bwrap(format!(
            "the table of the command guards of the isolation around this one, \
             {GUARD_TABLE}, cannot be read or interpreted; the kakoi inside may be of \
             another version than the one outside"
        ))
    })
}

/// The paths the "listed" mount mode looks up besides the policy's own.
fn listed_lookups(policy: &Policy) -> Vec<PathBuf> {
    if policy.mounts_mode != ListMode::Listed {
        return Vec::new();
    }
    crate::listed::lookups(policy.mounts_system)
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
    exclude_self: bool,
) -> Result<GuardPlan, Diagnostic> {
    let path = path_of(isolation.environment.values());
    let facts = policy
        .guards
        .iter()
        .map(|entry| {
            let program = &entry.rule.program;
            let names = shown_names(
                named(&real_candidates(OsStr::new(program), path)),
                isolation.listed.as_ref(),
            );
            (program.clone(), names)
        })
        .collect();
    let kakoi = executable.and_then(crate::mount_facts::real_path);
    let guards = place_guards(
        &policy.guards,
        path.is_some(),
        &facts,
        &isolation.mounts.items,
        kakoi.as_deref().filter(|_| exclude_self),
        kakoi.as_deref().filter(|_| exclude_self).and_then(file_id),
    );
    let mut guards = guards;
    guards.executable = kakoi.clone();
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
    listed: Option<&ListedRoot>,
    guards: &GuardPlan,
) -> PathBuf {
    if command.as_bytes().contains(&b'/') {
        return found;
    }
    let names = shown_names(named(&real_candidates(command, path)), listed);
    let Some(real) = real_program(&names, mounts) else {
        return found;
    };
    guards
        .placed
        .iter()
        .find(|guard| guard.found == real.candidate)
        .map_or(found, PlacedGuard::guard)
}

/// The names of `names` a "listed" isolation has as they are written on `PATH`: where
/// each is, what it resolves to, and every link on the way shown or made again
/// (specification REQ-468 and REQ-484); all of them under "host".
fn shown_names(names: Vec<NameFact>, listed: Option<&ListedRoot>) -> Vec<NameFact> {
    let Some(root) = listed else {
        return names;
    };
    names
        .into_iter()
        .filter(|fact| {
            fact.name.as_deref().is_some_and(|name| root.shows(name))
                && fact
                    .real
                    .as_deref()
                    .is_some_and(|real| root.shows_through(&fact.links, real))
        })
        .collect()
}

/// Stage 9 as the isolation sees it: a name on `PATH` whose place or real file `mounts`
/// hides, or that a "listed" isolation does not show, is passed over, and a path with `/`
/// naming such a file is not found (specification REQ-260 and REQ-484).
fn locate_shown_command(
    command: &OsStr,
    environment: &BTreeMap<OsString, OsString>,
    mounts: &[ResolvedItem],
    listed: Option<&ListedRoot>,
) -> Result<PathBuf, Diagnostic> {
    let candidates: Vec<PathBuf> = shown_names(
        named(&command_candidates(command, path_of(environment))),
        listed,
    )
    .into_iter()
    .filter(|fact| !name_hidden(fact, mounts))
    .map(|fact| fact.candidate)
    .collect();
    resolve_command(command, first_executable(&candidates))
}

/// Stage 8: `bwrap` on the host's `PATH` (specification section 14).
fn locate_bwrap(host: &BTreeMap<OsString, OsString>) -> Result<PathBuf, Diagnostic> {
    first_executable(&command_candidates(OsStr::new("bwrap"), path_of(host)))
        .ok_or_else(|| Diagnostic::bwrap("bwrap is not on the host's PATH"))
}

fn path_of(environment: &BTreeMap<OsString, OsString>) -> Option<&OsStr> {
    environment.get(OsStr::new("PATH")).map(OsString::as_os_str)
}
