//! The plan (specification section 2): the isolation resolved in the order of stage 7 of
//! section 13, and the bwrap argument list with the file descriptor positions as symbols
//! (section 14). Pure.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::command_limits::{AllowedList, CommandLimits, ALLOWED_LIST, FIRST_PROCESS, FIRST_ROOT};
use crate::copies::{CopiedEntry, CopySource, CopySources, FileContent, NotCopied};
use crate::diagnostic::{Diagnostic, Warning};
use crate::environment::HomeDirectory;
use crate::guard_placement::{GuardPlan, GUARD_LOCATION, GUARD_ROOT, GUARD_TABLE};
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

/// Everything stage 7 decides from besides the facts: the layers and their merge, the
/// expansion, the variables, the host.
#[derive(Debug, Clone, Copy)]
pub struct Inputs<'a> {
    pub layers: &'a [Layer],
    pub policy: &'a Policy,
    pub expanded: &'a ExpandedPolicy,
    pub variables: &'a Variables,
    pub home: &'a HomeDirectory,
    /// The configuration directory as derived, before realisation, whether or not
    /// anything is at the name (specification section 5.6).
    pub config_dir: &'a Path,
    /// The `--workspace` path as given (made absolute), before resolution; none when it
    /// was omitted.
    pub workspace: Option<&'a Path>,
    pub current_dir: &'a Path,
    pub host: &'a BTreeMap<OsString, OsString>,
    /// Whether the run is nested: the nesting mark was there when it started
    /// (specification REQ-455).
    pub nested: bool,
    /// Whether the plan is used: outside an isolation, or inside one with
    /// `--nested=isolate` (specification REQ-457).
    pub applied: bool,
    /// The shared file place the run plans with; none when it makes those files from data
    /// (specification REQ-460).
    pub shared_files: Option<&'a Path>,
    /// Whether the run hands the command guards of the run around it on (specification
    /// REQ-465).
    pub outer_guard: bool,
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
    /// The root of the "listed" mount mode; none under "host".
    pub listed: Option<ListedRoot>,
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
        skipped_paths: skipped,
        environment: assembled.environment,
        warnings,
        policy_sources: policy_sources(inputs.layers, &facts.mounts),
        listed,
    })
}

/// The policy whose scans and `hide-mounts` generate `hide` items: under the "listed"
/// mount mode, those rooted where nothing is shown are set aside, with the reason, by the
/// places the written items show, before anything is walked or the mount list is read
/// for them (specification REQ-468); otherwise the policy as it is. The outer layer
/// collects the facts of the scans and the `hide-mounts` of this policy only.
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

/// The current directory of a "listed" isolation is one of the places it shows
/// (specification REQ-483): any other is not there, and bwrap would fail to enter it.
fn check_current_dir_shown(current_dir: &Path, root: &ListedRoot) -> Result<(), Diagnostic> {
    if root.shows(current_dir) {
        return Ok(());
    }
    Err(Diagnostic::path(format!(
        "the current directory {} is not among the places the \"listed\" mount mode shows",
        current_dir.display()
    )))
}

/// The shared file place when the run puts anything from it: what the placement check
/// protects and what the arguments bind from are the same place, or none (specification
/// REQ-460 and REQ-462).
fn used_shared_files(inputs: &Inputs, items: &[ResolvedItem]) -> Option<PathBuf> {
    inputs
        .shared_files
        .filter(|_| places_shared_files(inputs.policy.network_mode, items))
        .map(Path::to_path_buf)
}

/// Whether a run puts anything from the shared file place: the empty file of a `hide` of a
/// file, or the resolver configuration of filtered (specification REQ-460). Only such a
/// run uses the place, and only its place is protected (specification REQ-462).
fn places_shared_files(network_mode: NetworkMode, items: &[ResolvedItem]) -> bool {
    network_mode == NetworkMode::Filtered
        || items
            .iter()
            .any(|item| item.directive == Directive::Hide && item.kind == EntryKind::NotDirectory)
}

/// The plan: what `--print-plan` shows and what the start uses (specification
/// sections 2 and 13).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Whether the run is nested (specification REQ-455).
    pub nested: bool,
    /// Whether the plan is used; a nested run without `--nested=isolate` only shows it
    /// (specification REQ-457).
    pub applied: bool,
    pub policy: Policy,
    /// Where the policies read came from: files at their real paths, or the built-in
    /// default.
    pub policy_sources: Vec<PolicySource>,
    pub variables: Variables,
    /// The home directory, which the summary shortens to `~`.
    pub home: PathBuf,
    pub mounts: ResolvedMounts,
    pub skipped_paths: Vec<SkippedPath>,
    /// The entries an `rw-copy` item could not take from the host, with the reason.
    pub not_copied: Vec<NotCopied>,
    pub environment: Environment,
    /// How `environment` differs from the host's.
    pub environment_changes: EnvironmentChanges,
    pub warnings: Vec<Warning>,
    pub bwrap: PathBuf,
    /// The command as given and resolved; none when `--print-plan` was given without one.
    pub command: Option<ResolvedCommand>,
    /// The command guards placed and skipped.
    pub guards: GuardPlan,
    /// The programs the "listed" command mode lets start; none under "host".
    pub commands: Option<CommandLimits>,
    pub arguments: Vec<Argument>,
}

/// The plan of `isolation`, with what each `rw-copy` item starts from in `copies`, `bwrap`
/// at `bwrap`, and the command as given and resolved. `copies` is taken by value: its file
/// content moves into the arguments rather than being held a second time.
pub fn plan(
    inputs: &Inputs,
    isolation: Isolation,
    copies: CopySources,
    bwrap: PathBuf,
    command: Option<ResolvedCommand>,
    guards: GuardPlan,
    commands: Option<CommandLimits>,
) -> Plan {
    let provisions = Provisions {
        shared_files: used_shared_files(inputs, &isolation.mounts.items),
        tun: inputs.policy.allow_nested_filtered,
        outer_guard: inputs.outer_guard,
        listed: isolation.listed.clone(),
        commands: commands.clone(),
    };
    let arguments = bwrap_arguments(
        inputs.policy.network_mode,
        inputs.current_dir,
        &isolation.mounts.items,
        &copies,
        &guards,
        &provisions,
        command.as_ref(),
    );
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
        bwrap,
        command,
        guards,
        commands,
        arguments,
    }
}

/// The nesting mark: an empty file every isolation carries, read-only, under bwrap's own
/// `/dev`. A kakoi started where it exists is nested (specification REQ-455).
pub const NESTING_MARK: &str = "/dev/kakoi-isolated";

/// The tun device a kakoi nested inside needs for filtered (specification REQ-458).
pub const TUN_DEVICE: &str = "/dev/net/tun";

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
    /// The descriptor one file of an `rw-copy` item is filled from.
    CopiedFile(FileContent),
    /// A file of the shared file place at `path`, bound read-only. Right before the start
    /// it becomes `path`, or the same file put from data when the place does not hold
    /// (specification REQ-460).
    SharedFile {
        file: SharedFile,
        path: PathBuf,
    },
}

/// What the host provides the isolation with besides the mount items.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Provisions {
    /// The shared file place the files that are always the same are bound from; none
    /// when they are made from data (specification REQ-460).
    pub shared_files: Option<PathBuf>,
    /// Whether the host's tun device is shown inside (specification REQ-458).
    pub tun: bool,
    /// Whether the command guards of the run around a nested one are shown inside as
    /// they are (specification REQ-465).
    pub outer_guard: bool,
    /// The root of the "listed" mount mode; none under "host", where the host's root is
    /// shown read-only.
    pub listed: Option<ListedRoot>,
    /// The programs the "listed" command mode lets start: the isolation's first process
    /// is kakoi, which restricts execution to them and then starts the command.
    pub commands: Option<CommandLimits>,
}

impl Provisions {
    /// The arguments that put `file` at `destination`, read-only.
    fn put(&self, file: SharedFile, destination: impl Into<OsString>) -> [Argument; 3] {
        match &self.shared_files {
            Some(place) => [
                Argument::text("--ro-bind"),
                Argument::SharedFile {
                    file,
                    path: place.join(file.name()),
                },
                Argument::text(destination),
            ],
            None => [
                Argument::text("--ro-bind-data"),
                file.from_data(),
                Argument::text(destination),
            ],
        }
    }
}

impl Argument {
    fn text(text: impl Into<OsString>) -> Self {
        Argument::Literal(text.into())
    }
}

/// The bwrap arguments: the fixed part in the order of specification section 14 (the
/// nesting mark right after `/dev`), ending
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
    copies: &CopySources,
    guards: &GuardPlan,
    provisions: &Provisions,
    command: Option<&ResolvedCommand>,
) -> Vec<Argument> {
    let mut arguments = match provisions.listed {
        Some(_) => vec![Argument::text("--tmpfs"), Argument::text("/")],
        None => vec![
            Argument::text("--ro-bind"),
            Argument::text("/"),
            Argument::text("/"),
        ],
    };
    arguments.extend([
        Argument::text("--dev"),
        Argument::text("/dev"),
        Argument::text("--ro-bind-data"),
        Argument::EmptyFile,
        Argument::text(NESTING_MARK),
    ]);
    if provisions.tun {
        arguments.extend([
            Argument::text("--dev-bind"),
            Argument::text(TUN_DEVICE),
            Argument::text(TUN_DEVICE),
        ]);
    }
    if provisions.outer_guard {
        arguments.extend([
            Argument::text("--ro-bind"),
            Argument::text(GUARD_ROOT),
            Argument::text(GUARD_ROOT),
        ]);
    }
    arguments.extend([
        Argument::text("--proc"),
        Argument::text("/proc"),
        Argument::text("--unshare-all"),
    ]);
    if matches!(network_mode, NetworkMode::Host | NetworkMode::Filtered) {
        arguments.push(Argument::text("--share-net"));
    }
    if network_mode == NetworkMode::Filtered {
        arguments.extend([Argument::text("--cap-drop"), Argument::text("ALL")]);
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
    if let Some(root) = &provisions.listed {
        arguments.extend(listed_arguments(root));
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
            (Directive::RwCopy, _) => {
                arguments.extend(copy_arguments(item, copies.sources.get(&item.real)));
            }
            (Directive::Hide, EntryKind::Directory) => {
                arguments.extend([Argument::text("--tmpfs"), real]);
            }
            (Directive::Hide, EntryKind::NotDirectory) => {
                arguments.extend(provisions.put(SharedFile::Empty, item.real.as_os_str()));
            }
        }
    }
    if network_mode == NetworkMode::Filtered {
        // Apply after user mounts so a copied/hidden host resolver file cannot
        // silently redirect the application's ordinary DNS lookups.
        let destination = provisions
            .listed
            .as_ref()
            .and_then(|root| root.filtered_resolver.as_deref())
            .unwrap_or(Path::new("/etc/resolv.conf"));
        arguments.extend(provisions.put(SharedFile::Resolver, destination.as_os_str()));
    }
    arguments.extend(guard_arguments(guards, provisions.commands.as_ref()));
    if provisions.listed.is_some() {
        arguments.extend([Argument::text("--remount-ro"), Argument::text("/")]);
    }
    if let Some(command) = command {
        arguments.push(Argument::text("--"));
        if provisions.commands.is_some() {
            arguments.push(Argument::text(FIRST_PROCESS));
        }
        arguments.push(Argument::text(command.path.as_os_str()));
        arguments.extend(command.arguments.iter().map(Argument::text));
    }
    arguments
}

/// The arguments that lay out the root of the "listed" mount mode before the mount items:
/// the isolation's own `/tmp`, first so that what is made under it stays and a written
/// item at `/tmp` covers it; the directories on the way to what is shown, made by kakoi
/// rather than by bwrap, which would make them readable by their owner only; the base;
/// the links on the written paths; and the resolver configuration's target. The root is
/// made read-only after everything else.
fn listed_arguments(root: &ListedRoot) -> Vec<Argument> {
    let mut arguments = vec![
        Argument::text("--perms"),
        Argument::text("1777"),
        Argument::text("--tmpfs"),
        Argument::text("/tmp"),
    ];
    for directory in &root.directories {
        arguments.extend([
            Argument::text("--perms"),
            Argument::text("0755"),
            Argument::text("--dir"),
            Argument::text(directory.as_os_str()),
        ]);
    }
    for directory in &root.base {
        arguments.extend([
            Argument::text("--ro-bind"),
            Argument::text(directory.as_os_str()),
            Argument::text(directory.as_os_str()),
        ]);
    }
    for link in &root.links {
        arguments.extend([
            Argument::text("--symlink"),
            Argument::text(link.target.as_os_str()),
            Argument::text(link.place.as_os_str()),
        ]);
    }
    if let Some(resolver) = &root.resolver {
        arguments.extend([
            Argument::text("--ro-bind"),
            Argument::text(resolver.as_os_str()),
            Argument::text(resolver.as_os_str()),
        ]);
    }
    arguments
}

/// The arguments of one `rw-copy` item. A directory is a tmpfs of its own, filled from
/// `source` entry by entry: the tmpfs holds what the isolation writes and is gone with the
/// mount namespace, and each entry is made inside it, so `--file` writes into that tmpfs
/// and never through to the host. A regular file is `--bind-data`, which puts the bytes in
/// a file of bwrap's own and binds that over the host's, rather than `--file`, which would
/// write the copy at the path itself and reach the host whenever an `rw` item covers the
/// directory it sits in (measured with bwrap 0.9.0).
///
/// Every item that applies has a source: `copy_facts` reads one for each of them. Without
/// one there is nothing of the host to start from, and the item falls back to what `hide`
/// does, so a mistake leaves the place empty rather than showing the host's own.
fn copy_arguments(item: &ResolvedItem, source: Option<&CopySource>) -> Vec<Argument> {
    let real = || Argument::text(item.real.as_os_str());
    match (source, item.kind) {
        (Some(CopySource::File { mode, content }), _) => vec![
            Argument::text("--perms"),
            Argument::text(octal(*mode)),
            Argument::text("--bind-data"),
            Argument::CopiedFile(content.clone()),
            real(),
        ],
        (Some(CopySource::Directory { mode, entries }), _) => {
            let mut arguments = vec![
                Argument::text("--perms"),
                Argument::text(octal(*mode)),
                Argument::text("--tmpfs"),
                real(),
            ];
            for entry in entries {
                let destination = Argument::text(item.real.join(entry.relative()).into_os_string());
                match entry {
                    CopiedEntry::Directory { mode, .. } => arguments.extend([
                        Argument::text("--perms"),
                        Argument::text(octal(*mode)),
                        Argument::text("--dir"),
                        destination,
                    ]),
                    CopiedEntry::File { mode, content, .. } => arguments.extend([
                        Argument::text("--perms"),
                        Argument::text(octal(*mode)),
                        Argument::text("--file"),
                        Argument::CopiedFile(content.clone()),
                        destination,
                    ]),
                    CopiedEntry::Symlink { target, .. } => arguments.extend([
                        Argument::text("--symlink"),
                        Argument::text(target.as_os_str()),
                        destination,
                    ]),
                }
            }
            arguments
        }
        (None, EntryKind::Directory) => vec![Argument::text("--tmpfs"), real()],
        (None, EntryKind::NotDirectory) => vec![
            Argument::text("--ro-bind-data"),
            Argument::EmptyFile,
            real(),
        ],
    }
}

/// The arguments that place the command guards and the first process of the "listed"
/// command mode, after the user's mounts so that none of them covers what kakoi places:
/// each in a tmpfs of kakoi's own under bwrap's `/dev`, made read-only once filled.
/// Nothing when there is neither. No argument is a bare `--`.
fn guard_arguments(guards: &GuardPlan, commands: Option<&CommandLimits>) -> Vec<Argument> {
    let mut arguments = Vec::new();
    if let Some(kakoi) = guards
        .executable
        .as_deref()
        .filter(|_| !guards.placed.is_empty())
    {
        arguments.extend(guard_tmpfs(kakoi, guards));
    }
    if let Some(limits) = commands {
        arguments.extend(first_process_tmpfs(limits));
    }
    arguments
}

/// The tmpfs of the command guards: a bind of kakoi's executable for each guard (each
/// its own, so that the guard tells where it was started from), each program of
/// `guard-absolute-path` bound again inside the tmpfs and a guard bound over its own
/// path, and the table.
fn guard_tmpfs(kakoi: &Path, guards: &GuardPlan) -> Vec<Argument> {
    let mut arguments = Vec::from(tmpfs(GUARD_ROOT));
    arguments.extend(directory(Path::new(GUARD_LOCATION)));
    for guard in &guards.placed {
        arguments.extend(read_only_bind(kakoi, &guard.guard()));
    }
    for (real, relocated) in &guards.overlaid {
        // bwrap would make the missing directories itself, readable by their owner only.
        let parent = relocated
            .parent()
            .expect("a relocated program is in a directory");
        for directory_path in [parent.parent(), Some(parent)].into_iter().flatten() {
            arguments.extend(directory(directory_path));
        }
        arguments.extend(read_only_bind(real, relocated));
        arguments.extend(read_only_bind(kakoi, real));
    }
    arguments.extend(read_only_data(guards.table.to_bytes(), GUARD_TABLE));
    arguments.extend(remount_read_only(GUARD_ROOT));
    arguments
}

/// The tmpfs of the first process: kakoi's executable and the list of the programs it
/// allows.
fn first_process_tmpfs(limits: &CommandLimits) -> Vec<Argument> {
    let mut arguments = Vec::from(tmpfs(FIRST_ROOT));
    arguments.extend(read_only_bind(&limits.executable, Path::new(FIRST_PROCESS)));
    let allowed: Vec<PathBuf> = limits
        .allowed
        .iter()
        .chain(&limits.relocated)
        .chain(&limits.outer_guards)
        .cloned()
        .collect();
    arguments.extend(read_only_data(
        AllowedList::new(&allowed).to_bytes(),
        ALLOWED_LIST,
    ));
    arguments.extend(remount_read_only(FIRST_ROOT));
    arguments
}

fn tmpfs(path: &str) -> [Argument; 4] {
    [
        Argument::text("--perms"),
        Argument::text("0755"),
        Argument::text("--tmpfs"),
        Argument::text(path),
    ]
}

fn directory(path: &Path) -> [Argument; 4] {
    [
        Argument::text("--perms"),
        Argument::text("0755"),
        Argument::text("--dir"),
        Argument::text(path.as_os_str()),
    ]
}

fn read_only_bind(from: &Path, to: &Path) -> [Argument; 3] {
    [
        Argument::text("--ro-bind"),
        Argument::text(from.as_os_str()),
        Argument::text(to.as_os_str()),
    ]
}

fn read_only_data(content: Vec<u8>, to: &str) -> [Argument; 5] {
    [
        Argument::text("--perms"),
        Argument::text("0444"),
        Argument::text("--ro-bind-data"),
        Argument::CopiedFile(FileContent::new(content)),
        Argument::text(to),
    ]
}

fn remount_read_only(path: &str) -> [Argument; 2] {
    [Argument::text("--remount-ro"), Argument::text(path)]
}

/// A mode as bwrap's `--perms` takes it.
fn octal(mode: u32) -> String {
    format!("{mode:04o}")
}
