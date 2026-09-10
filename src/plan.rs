//! The plan (specification section 2): the isolation resolved in the order of stage 7 of
//! section 13, and the bwrap argument list with the file descriptor positions as symbols
//! (section 14). Pure.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::copies::{CopiedEntry, CopySource, CopySources, FileContent, NotCopied};
use crate::diagnostic::{Diagnostic, Warning};
use crate::environment::HomeDirectory;
use crate::isolated_env::{
    assemble_environment, environment_changes, Environment, EnvironmentChanges, SecretFile,
};
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
    /// The configuration directory as derived, before realisation, whether or not
    /// anything is at the name (specification section 5.6).
    pub config_dir: &'a Path,
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
) -> Plan {
    let arguments = bwrap_arguments(
        inputs.policy.network_mode,
        inputs.current_dir,
        &isolation.mounts.items,
        &copies,
        command.as_ref(),
    );
    let environment_changes =
        environment_changes(inputs.policy, inputs.host, &isolation.environment);
    Plan {
        nested: is_nested(inputs.host),
        policy: inputs.policy.clone(),
        policy_sources: isolation.policy_sources,
        variables: inputs.variables.clone(),
        home: inputs.home.path().to_path_buf(),
        mounts: isolation.mounts,
        skipped_paths: isolation.skipped_paths,
        not_copied: copies.not_copied,
        environment: isolation.environment,
        environment_changes,
        warnings: isolation.warnings,
        bwrap,
        command,
        arguments,
    }
}

/// Whether `host` marks a nested run: `KAKOI` is `1` (specification section 12.1).
pub fn is_nested(host: &BTreeMap<OsString, OsString>) -> bool {
    host.get(OsStr::new("KAKOI"))
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
    /// The descriptor one file of an `rw-copy` item is filled from.
    CopiedFile(FileContent),
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
    copies: &CopySources,
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
            (Directive::RwCopy, _) => {
                arguments.extend(copy_arguments(item, copies.sources.get(&item.real)));
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
        (Some(CopySource::Directory(entries)), _) => {
            let mut arguments = vec![Argument::text("--tmpfs"), real()];
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

/// A mode as bwrap's `--perms` takes it.
fn octal(mode: u32) -> String {
    format!("{mode:04o}")
}
