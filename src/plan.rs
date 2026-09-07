//! The plan (specification section 2): the isolation resolved in the order of stage 7 of
//! section 13, and the bwrap argument list with the file descriptor positions as symbols
//! (section 14). Pure.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::diagnostic::{Diagnostic, Warning};
use crate::environment::HomeDirectory;
use crate::isolated_env::{assemble_environment, Environment, SecretFile};
use crate::layers::{Directive, Layer, Policy, PolicySource};
use crate::mounts::{
    generate, policy_sources, resolve_written, skipped_paths, EntryKind, ExpandedPolicy,
    MountFacts, ResolvedItem, ResolvedMounts, SkippedPath,
};
use crate::placement::{
    check_origins, check_placement, protected_paths, swappable_ro_items, written_paths,
};
use crate::policy::NetworkMode;
use crate::variables::Variables;

/// Everything stage 7 decides from besides the facts: the layers and their merge, the
/// expansion, the variables, the host.
#[derive(Debug, Clone, Copy)]
pub struct Inputs<'a> {
    pub layers: &'a [Layer],
    pub policy: &'a Policy,
    pub expanded: &'a ExpandedPolicy,
    pub variables: &'a Variables,
    pub home: &'a HomeDirectory,
    /// The configuration directory as derived, before realisation; none when it does not
    /// exist (specification section 5.6).
    pub config_dir: Option<&'a Path>,
    /// The `--workspace` path as given (made absolute), before resolution; none when it
    /// was omitted.
    pub workspace: Option<&'a Path>,
    pub current_dir: &'a Path,
    pub host: &'a BTreeMap<OsString, OsString>,
}

/// The facts stage 7 needs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IsolationFacts {
    pub mounts: MountFacts,
    pub secrets: BTreeMap<String, SecretFile>,
}

/// The isolation as resolved: the mount items, the scan roots, `hide-mounts` `under`s,
/// and `path-prepend` entries skipped, the environment, the warnings so far, and where the
/// policies read came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Isolation {
    pub mounts: ResolvedMounts,
    pub skipped_paths: Vec<SkippedPath>,
    pub environment: Environment,
    pub warnings: Vec<Warning>,
    pub policy_sources: Vec<PolicySource>,
}

/// Stage 7 of specification section 13, in its order: the identity of the written mount
/// items, the scan roots and the `hide-mounts` `under`s against the written items before
/// generation, the generation and the kinds, the placement rules, then the secrets and the
/// git count while assembling the environment. The first diagnostic ends it.
pub fn resolve_isolation(inputs: &Inputs, facts: &IsolationFacts) -> Result<Isolation, Diagnostic> {
    let written = written_paths(inputs.expanded, inputs.workspace);
    let before_generation = resolve_written(inputs.expanded, &facts.mounts)?;
    check_origins(
        inputs.expanded,
        &before_generation,
        &written,
        inputs.variables,
        inputs.current_dir,
        &facts.mounts,
    )?;
    let swappable_ro = swappable_ro_items(
        &before_generation,
        &written,
        inputs.variables,
        inputs.current_dir,
        &facts.mounts,
    );
    let mounts = generate(
        before_generation,
        inputs.expanded,
        inputs.layers,
        inputs.variables,
        &facts.mounts,
        &swappable_ro,
    )?;
    let protected = protected_paths(inputs.expanded, inputs.layers, inputs.config_dir);
    let mut warnings = check_placement(
        &mounts,
        &protected,
        &written,
        inputs.variables,
        inputs.home,
        inputs.current_dir,
        &facts.mounts,
    )?;
    // A `path-prepend` entry enters `PATH` as its real path; one that names nothing is
    // skipped like a mount item would be, and reported (specification section 5.2).
    let path_prepend: Vec<PathBuf> = inputs
        .expanded
        .path_prepend
        .iter()
        .filter_map(|entry| entry.path.path())
        .filter_map(|entry| facts.mounts.entry(entry).path().map(Path::to_path_buf))
        .collect();
    let assembled =
        assemble_environment(inputs.policy, inputs.host, &facts.secrets, &path_prepend)?;
    warnings.extend(assembled.warnings);
    Ok(Isolation {
        mounts,
        skipped_paths: skipped_paths(inputs.expanded, &facts.mounts),
        environment: assembled.environment,
        warnings,
        policy_sources: policy_sources(inputs.layers, &facts.mounts),
    })
}

/// The plan: what `--print-plan` shows and what the start uses (specification
/// sections 2 and 13).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Whether the run is nested (specification section 12.1): the plan is shown, not
    /// applied.
    pub nested: bool,
    pub policy: Policy,
    /// Where the policies read came from: files at their real paths, or the built-in
    /// default.
    pub policy_sources: Vec<PolicySource>,
    pub variables: Variables,
    pub mounts: ResolvedMounts,
    pub skipped_paths: Vec<SkippedPath>,
    pub environment: Environment,
    pub warnings: Vec<Warning>,
    pub bwrap: PathBuf,
    /// The command as given and resolved; none when `--print-plan` was given without one.
    pub command: Option<ResolvedCommand>,
    pub arguments: Vec<Argument>,
}

/// The plan of `isolation` with `bwrap` at `bwrap` and the command as given and resolved.
pub fn plan(
    inputs: &Inputs,
    isolation: Isolation,
    bwrap: PathBuf,
    command: Option<ResolvedCommand>,
) -> Plan {
    let arguments = bwrap_arguments(
        inputs.policy.network_mode,
        inputs.current_dir,
        &isolation.mounts.items,
        command.as_ref(),
    );
    Plan {
        nested: is_nested(inputs.host),
        policy: inputs.policy.clone(),
        policy_sources: isolation.policy_sources,
        variables: inputs.variables.clone(),
        mounts: isolation.mounts,
        skipped_paths: isolation.skipped_paths,
        environment: isolation.environment,
        warnings: isolation.warnings,
        bwrap,
        command,
        arguments,
    }
}

/// Whether `host` marks a nested run: `PROCESS_WRAP` is `1` (specification section 12.1).
pub fn is_nested(host: &BTreeMap<OsString, OsString>) -> bool {
    host.get(OsStr::new("PROCESS_WRAP"))
        .is_some_and(|value| value == "1")
}

/// The command as given on the command line (`COMMAND` and `ARGS`) and where `COMMAND`
/// resolved to (specification section 4.2). The process sees `command` as its argv[0]
/// and `path` is only what is executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCommand {
    pub command: OsString,
    pub arguments: Vec<OsString>,
    pub path: PathBuf,
}

/// One bwrap argument. The descriptors are symbols: their numbers are assigned right
/// before the start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Argument {
    Literal(OsString),
    /// The descriptor of the empty file a `hide` of a file is bound from.
    EmptyFile,
    /// The descriptor the seccomp filter is read from.
    Seccomp,
}

impl Argument {
    fn text(text: impl Into<OsString>) -> Self {
        Argument::Literal(text.into())
    }
}

/// The bwrap arguments: the fixed part in the order of specification section 14, ending
/// with `--argv0 <COMMAND as given>` so that the process sees the name it was called by;
/// then the mount items in the order they were resolved; then `--`, the resolved path, and
/// `ARGS`. The `--` keeps the path from being read as an option of bwrap (specification
/// section 4.2: a `COMMAND` with `/` is used as that path, and such a path can start with
/// `-`). Without a command (`--print-plan` alone) neither `--argv0` nor `--` appears. No
/// argument sets an environment variable.
pub fn bwrap_arguments(
    network_mode: NetworkMode,
    current_dir: &Path,
    items: &[ResolvedItem],
    command: Option<&ResolvedCommand>,
) -> Vec<Argument> {
    let mut arguments = vec![
        Argument::text("--ro-bind"),
        Argument::text("/"),
        Argument::text("/"),
        Argument::text("--dev"),
        Argument::text("/dev"),
        Argument::text("--proc"),
        Argument::text("/proc"),
        Argument::text("--unshare-all"),
    ];
    if network_mode == NetworkMode::Host {
        arguments.push(Argument::text("--share-net"));
    }
    arguments.extend([
        Argument::text("--die-with-parent"),
        Argument::text("--chdir"),
        Argument::text(current_dir),
        Argument::text("--seccomp"),
        Argument::Seccomp,
    ]);
    if let Some(command) = command {
        arguments.extend([
            Argument::text("--argv0"),
            Argument::text(command.command.as_os_str()),
        ]);
    }
    for item in items {
        let real = Argument::text(item.real.as_os_str());
        match (item.directive, item.kind) {
            (Directive::Rw | Directive::RwFile, _) => {
                arguments.extend([Argument::text("--bind"), real.clone(), real]);
            }
            (Directive::Ro, _) => {
                arguments.extend([Argument::text("--ro-bind"), real.clone(), real]);
            }
            (Directive::Hide, EntryKind::Directory) => {
                arguments.extend([Argument::text("--tmpfs"), real]);
            }
            (Directive::Hide, EntryKind::NotDirectory) => {
                arguments.extend([Argument::text("--ro-bind-data"), Argument::EmptyFile, real]);
            }
        }
    }
    if let Some(command) = command {
        arguments.push(Argument::text("--"));
        arguments.push(Argument::text(command.path.as_os_str()));
        arguments.extend(command.arguments.iter().map(Argument::text));
    }
    arguments
}
