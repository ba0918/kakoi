//! Pure isolation decisions from explicit input and observed facts.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::command_limits::CommandLimits;
use crate::copies::{CopySources, FileContent, NotCopied};
use crate::diagnostic::{Diagnostic, Warning};
use crate::environment::HomeDirectory;
use crate::guard_placement::GuardPlan;
use crate::isolated_env::{
    assemble_environment, environment_changes, Environment, EnvironmentChanges, SecretFile,
};
use crate::layers::{Directive, Layer, Policy, PolicySource};
use crate::listed::{listed_root, set_aside_unshown, shown_generators, ListedRoot};
use crate::mounts::{
    generate, policy_sources, resolve_written, skipped_paths, EntryKind, ExpandedPolicy,
    MountFacts, ResolvedItem, ResolvedMounts, SkippedPath,
};
use crate::placement::{
    check_origins, check_placement, protected_paths, swappable_ro_items, written_paths,
    ProtectedPaths,
};
use crate::policy::{ListMode, NetworkMode};
use crate::shared_files::SharedFile;
use crate::variables::Variables;

/// Values used by stage 7; host state is observed by the outer layer.
#[derive(Debug, Clone, Copy)]
pub struct Inputs<'a> {
    pub layers: &'a [Layer],
    pub policy: &'a Policy,
    pub expanded: &'a ExpandedPolicy,
    pub variables: &'a Variables,
    pub home: &'a HomeDirectory,
    /// Derived configuration path, including prefixes that do not yet exist.
    pub config_dir: &'a Path,
    pub workspace: Option<&'a Path>,
    pub current_dir: &'a Path,
    pub host: &'a BTreeMap<OsString, OsString>,
    pub nested: bool,
    pub applied: bool,
    pub shared_files: Option<&'a Path>,
    pub outer_guard: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IsolationFacts {
    pub mounts: MountFacts,
    pub secrets: BTreeMap<String, SecretFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Isolation {
    pub mounts: ResolvedMounts,
    pub skipped_paths: Vec<SkippedPath>,
    pub environment: Environment,
    pub warnings: Vec<Warning>,
    pub policy_sources: Vec<PolicySource>,
    pub listed: Option<ListedRoot>,
}

/// Stage 7 in diagnostic order: origins, generation, placement, then environment.
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
    let (generating, skipped_generators) = generating_policy(inputs, &facts.mounts)?;
    let mut mounts = generate(
        before_generation,
        &generating,
        inputs.layers,
        inputs.variables,
        &facts.mounts,
        &swappable_ro,
    )?;
    let mut skipped = skipped_paths(inputs.expanded, &facts.mounts);
    let listed = (inputs.policy.mounts_mode == ListMode::Listed).then(|| {
        let (root, skipped_base) = listed_root(
            inputs.policy.mounts_system,
            inputs.policy.network_mode,
            &mounts.items,
            inputs.expanded,
            &facts.mounts,
        );
        skipped.extend(skipped_base);
        skipped.extend(skipped_generators);
        set_aside_unshown(&mut mounts, &root);
        root
    });
    let protected = ProtectedPaths {
        shared_files: used_shared_files(inputs, &mounts.items),
        ..protected_paths(inputs.expanded, inputs.layers, inputs.config_dir)
    };
    let mut warnings = check_placement(
        &mounts,
        &protected,
        &written,
        inputs.variables,
        inputs.home,
        inputs.current_dir,
        &facts.mounts,
    )?;
    if let Some(root) = &listed {
        check_current_dir_shown(inputs.current_dir, root)?;
    }
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
        skipped_paths: skipped,
        environment: assembled.environment,
        warnings,
        policy_sources: policy_sources(inputs.layers, &facts.mounts),
        listed,
    })
}

/// Only selected generators are observed by the outer layer (REQ-468).
pub fn generating_policy(
    inputs: &Inputs,
    facts: &MountFacts,
) -> Result<(ExpandedPolicy, Vec<SkippedPath>), Diagnostic> {
    if inputs.policy.mounts_mode != ListMode::Listed {
        return Ok((inputs.expanded.clone(), Vec::new()));
    }
    let written = resolve_written(inputs.expanded, facts)?;
    let (root, _) = listed_root(
        inputs.policy.mounts_system,
        inputs.policy.network_mode,
        &written.items,
        inputs.expanded,
        facts,
    );
    Ok(shown_generators(inputs.expanded, facts, &root))
}

fn check_current_dir_shown(current_dir: &Path, root: &ListedRoot) -> Result<(), Diagnostic> {
    if root.shows(current_dir) {
        return Ok(());
    }
    Err(Diagnostic::path(format!(
        "the current directory {} is not among the places the \"listed\" mount mode shows",
        current_dir.display()
    )))
}

fn used_shared_files(inputs: &Inputs, items: &[ResolvedItem]) -> Option<PathBuf> {
    inputs
        .shared_files
        .filter(|_| places_shared_files(inputs.policy.network_mode, items))
        .map(Path::to_path_buf)
}

fn places_shared_files(network_mode: NetworkMode, items: &[ResolvedItem]) -> bool {
    network_mode == NetworkMode::Filtered
        || items
            .iter()
            .any(|item| item.directive == Directive::Hide && item.kind == EntryKind::NotDirectory)
}

/// Internal CLI plan. The public runtime API does not expose this mutable value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub nested: bool,
    pub applied: bool,
    pub policy: Policy,
    pub policy_sources: Vec<PolicySource>,
    pub variables: Variables,
    pub home: PathBuf,
    pub mounts: ResolvedMounts,
    pub skipped_paths: Vec<SkippedPath>,
    pub not_copied: Vec<NotCopied>,
    pub environment: Environment,
    pub environment_changes: EnvironmentChanges,
    pub warnings: Vec<Warning>,
    pub bwrap: PathBuf,
    pub command: Option<ResolvedCommand>,
    pub guards: GuardPlan,
    pub commands: Option<CommandLimits>,
    pub arguments: Vec<Argument>,
    pub launch_layout: LaunchLayout,
}

/// Symbolic launch description supplied by the Linux argument generator.
pub struct LaunchDescription {
    pub program: PathBuf,
    pub arguments: Vec<Argument>,
    pub layout: LaunchLayout,
}

pub fn provisions(
    inputs: &Inputs,
    isolation: &Isolation,
    commands: Option<CommandLimits>,
) -> Provisions {
    Provisions {
        shared_files: used_shared_files(inputs, &isolation.mounts.items),
        tun: inputs.policy.allow_nested_filtered,
        outer_guard: inputs.outer_guard,
        listed: isolation.listed.clone(),
        commands,
    }
}

/// Combines the pure decisions with the independently generated launch description.
pub fn plan(
    inputs: &Inputs,
    isolation: Isolation,
    copies: CopySources,
    launch: LaunchDescription,
    command: Option<ResolvedCommand>,
    guards: GuardPlan,
    commands: Option<CommandLimits>,
) -> Plan {
    let environment_changes =
        environment_changes(inputs.policy, inputs.host, &isolation.environment);
    let mut warnings = isolation.warnings;
    if inputs.policy.network_settings_present && inputs.policy.network_mode != NetworkMode::Filtered
    {
        warnings.push(Warning::new(
            "network/process settings are unused outside filtered mode",
        ));
    }
    Plan {
        nested: inputs.nested,
        applied: inputs.applied,
        policy: inputs.policy.clone(),
        policy_sources: isolation.policy_sources,
        variables: inputs.variables.clone(),
        home: inputs.home.path().to_path_buf(),
        mounts: isolation.mounts,
        skipped_paths: isolation.skipped_paths,
        not_copied: copies.not_copied,
        environment: isolation.environment,
        environment_changes,
        warnings,
        bwrap: launch.program,
        command,
        guards,
        commands,
        arguments: launch.arguments,
        launch_layout: launch.layout,
    }
}

pub const NESTING_MARK: &str = "/dev/kakoi-isolated";
pub const TUN_DEVICE: &str = "/dev/net/tun";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCommand {
    pub command: OsString,
    pub arguments: Vec<OsString>,
    pub path: PathBuf,
}

/// Symbols, not live descriptor numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Argument {
    Literal(OsString),
    EmptyFile,
    Seccomp,
    CopiedFile(FileContent),
    SharedFile { file: SharedFile, path: PathBuf },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchLayout {
    pub argv0: Option<usize>,
    pub command_separator: Option<usize>,
    pub resolver_destination: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Provisions {
    pub shared_files: Option<PathBuf>,
    pub tun: bool,
    pub outer_guard: bool,
    pub listed: Option<ListedRoot>,
    pub commands: Option<CommandLimits>,
}
