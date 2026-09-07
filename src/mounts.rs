//! The mount items of the merged policy: the expansion of `~` and the variables
//! (specification section 5.2) and their resolution to real paths against the facts the
//! outer layer collected (sections 5.4 and 6). Pure.

use std::collections::BTreeMap;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use crate::diagnostic::Diagnostic;
use crate::environment::{HomeDirectory, RealEntry};
use crate::layers::{Directive, Layer, LayerOrigin, Policy, PolicySource};
use crate::policy::{PolicyPath, Variable};
use crate::variables::Variables;

/// A policy path after expansion: a path, or a variable that has no value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expansion {
    Path(PathBuf),
    Valueless(Variable),
}

impl Expansion {
    /// The expanded path, when the value has one.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Expansion::Path(path) => Some(path),
            Expansion::Valueless(_) => None,
        }
    }
}

/// One written mount item with its path expanded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedItem {
    pub directive: Directive,
    pub written: PolicyPath,
    pub origin: LayerOrigin,
    pub path: Expansion,
}

/// One path-taking value that is neither a mount item nor a scan or `hide-mounts` entry
/// (an `env.path-prepend` entry), with its path expanded and as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedPath {
    pub written: PolicyPath,
    pub path: Expansion,
}

/// One `mounts.scan` entry with its root expanded; `written` is the root as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedScan {
    pub written: PolicyPath,
    pub root: Expansion,
    pub names: Vec<String>,
    pub exclude: Vec<String>,
    pub prune: Vec<String>,
}

/// One `mounts.hide-mounts` entry with its `under` expanded; `written` is the `under` as
/// written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedHideMounts {
    pub written: PolicyPath,
    pub under: Expansion,
    pub fstype: Vec<String>,
}

/// The merged policy with every path-taking value expanded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedPolicy {
    pub mounts: Vec<ExpandedItem>,
    pub scans: Vec<ExpandedScan>,
    pub hide_mounts: Vec<ExpandedHideMounts>,
    pub secrets: BTreeMap<String, Expansion>,
    pub path_prepend: Vec<ExpandedPath>,
}

/// Expands `~` to the home directory and the variables to their values.
pub fn expand(path: &PolicyPath, variables: &Variables, home: &HomeDirectory) -> Expansion {
    let (base, rest) = match path {
        PolicyPath::Absolute(path) => return Expansion::Path(path.clone()),
        PolicyPath::Home(rest) => (home.path(), rest),
        PolicyPath::Variable(variable, rest) => {
            let value = match variable {
                Variable::Workspace => Some(variables.workspace.as_path()),
                Variable::Worktree => Some(variables.worktree.as_path()),
                Variable::GitCommonDir => variables.git_common_dir.as_deref(),
                Variable::ConfigDir => variables.config_dir.as_deref(),
            };
            match value {
                Some(value) => (value, rest),
                None => return Expansion::Valueless(*variable),
            }
        }
    };
    let mut expanded = base.as_os_str().to_os_string();
    expanded.push(rest);
    Expansion::Path(PathBuf::from(expanded))
}

/// Expands every path-taking value of `policy`.
pub fn expand_policy(
    policy: &Policy,
    variables: &Variables,
    home: &HomeDirectory,
) -> ExpandedPolicy {
    let expand = |path: &PolicyPath| expand(path, variables, home);
    ExpandedPolicy {
        mounts: policy
            .mounts
            .iter()
            .map(|item| ExpandedItem {
                directive: item.directive,
                written: item.path.clone(),
                origin: item.origin.clone(),
                path: expand(&item.path),
            })
            .collect(),
        scans: policy
            .scan
            .iter()
            .map(|scan| ExpandedScan {
                written: scan.root.clone(),
                root: expand(&scan.root),
                names: scan.names.clone(),
                exclude: scan.exclude.clone(),
                prune: scan.prune.clone(),
            })
            .collect(),
        hide_mounts: policy
            .hide_mounts
            .iter()
            .map(|hide_mounts| ExpandedHideMounts {
                written: hide_mounts.under.clone(),
                under: expand(&hide_mounts.under),
                fstype: hide_mounts.fstype.clone(),
            })
            .collect(),
        secrets: policy
            .secrets
            .iter()
            .map(|(name, path)| (name.clone(), expand(path)))
            .collect(),
        path_prepend: policy
            .path_prepend
            .iter()
            .map(|entry| ExpandedPath {
                written: entry.clone(),
                path: expand(entry),
            })
            .collect(),
    }
}

/// One scan the outer layer is asked to walk, with its root as expanded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanRequest {
    pub root: PathBuf,
    pub names: Vec<String>,
    pub exclude: Vec<String>,
    pub prune: Vec<String>,
}

/// What the outer layer must look up for the mount resolution: the paths whose existence,
/// kind, and real path are needed, the paths whose resolution must report the symbolic
/// links and directories it passes through (the ones specification section 5.6 protects,
/// the written mount items, the scan roots, the `hide-mounts` `under`s, and the given
/// `--workspace`), the scans to walk, and the `under` of each `hide-mounts` (the mount
/// list is read only when there is one).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Candidates {
    pub paths: Vec<PathBuf>,
    pub traversals: Vec<PathBuf>,
    pub scans: Vec<ScanRequest>,
    pub hide_mounts_under: Vec<PathBuf>,
}

/// The candidate paths of `expanded`: every expanded path, plus every prefix (each
/// ancestor and the path itself) of the paths specification section 5.6 protects — the
/// policy files read, the configuration directory, the secret files, and the
/// `path-prepend` entries — and the configuration directory's `secrets/`. The traversals
/// are those protected paths, the written mount items, the scan roots, the `hide-mounts`
/// `under`s, and `workspace`, the `--workspace` path as given.
pub fn candidates(
    expanded: &ExpandedPolicy,
    layers: &[Layer],
    variables: &Variables,
    config_dir: &Path,
    workspace: Option<&Path>,
) -> Candidates {
    let mut paths = Vec::new();
    let expanded_paths = expanded
        .mounts
        .iter()
        .map(|item| &item.path)
        .chain(expanded.scans.iter().map(|scan| &scan.root))
        .chain(expanded.hide_mounts.iter().map(|hide| &hide.under))
        .chain(expanded.path_prepend.iter().map(|entry| &entry.path));
    paths.extend(
        expanded_paths
            .filter_map(Expansion::path)
            .map(Path::to_path_buf),
    );
    let mut traversals: Vec<PathBuf> = layers
        .iter()
        .filter_map(|layer| match &layer.origin {
            LayerOrigin::Profile(path) | LayerOrigin::PolicyFile(path) => Some(path.as_path()),
            LayerOrigin::BuiltInDefault | LayerOrigin::CommandLine => None,
        })
        .chain(std::iter::once(config_dir))
        .chain(expanded.secrets.values().filter_map(Expansion::path))
        .chain(
            expanded
                .path_prepend
                .iter()
                .filter_map(|entry| entry.path.path()),
        )
        .map(Path::to_path_buf)
        .collect();
    for path in &traversals {
        paths.extend(path.ancestors().map(Path::to_path_buf));
    }
    traversals.extend(
        expanded
            .mounts
            .iter()
            .filter_map(|item| item.path.path())
            .chain(expanded.scans.iter().filter_map(|scan| scan.root.path()))
            .chain(
                expanded
                    .hide_mounts
                    .iter()
                    .filter_map(|hide| hide.under.path()),
            )
            .chain(workspace)
            .map(Path::to_path_buf),
    );
    if let Some(config_dir) = &variables.config_dir {
        paths.push(config_dir.join("secrets"));
    }
    paths.sort();
    paths.dedup();
    traversals.sort();
    traversals.dedup();
    Candidates {
        paths,
        traversals,
        scans: expanded
            .scans
            .iter()
            .filter_map(|scan| match &scan.root {
                Expansion::Path(root) => Some(ScanRequest {
                    root: root.clone(),
                    names: scan.names.clone(),
                    exclude: scan.exclude.clone(),
                    prune: scan.prune.clone(),
                }),
                Expansion::Valueless(_) => None,
            })
            .collect(),
        hide_mounts_under: expanded
            .hide_mounts
            .iter()
            .filter_map(|hide| hide.under.path().map(Path::to_path_buf))
            .collect(),
    }
}

/// One entry the scan found by name: where it was found, what is behind it, and whether
/// the entry itself is a symbolic link (so that `target` is somewhere else).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanHit {
    pub found_at: PathBuf,
    pub target: RealEntry,
    pub is_link: bool,
}

/// One mount of the host: its mount point and its file system type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub target: PathBuf,
    pub fstype: String,
}

/// The facts the outer layer collected for the mount resolution.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MountFacts {
    /// What exists behind each candidate path, keyed by the expanded path.
    pub paths: BTreeMap<PathBuf, RealEntry>,
    /// The symbolic links passed through while resolving each path specification
    /// section 5.6 protects, keyed by the given path. Each link is named by its own place:
    /// its parent's real path and its name.
    pub links: BTreeMap<PathBuf, Vec<PathBuf>>,
    /// The directories, each by its real path, whose entries were consulted while
    /// resolving each path specification section 5.6 protects, keyed by the given path:
    /// the ones a link target enters and leaves again through `..` included.
    pub directories: BTreeMap<PathBuf, Vec<PathBuf>>,
    pub scan_hits: Vec<ScanHit>,
    pub mounts: Vec<Mount>,
    /// Whether the mount list was asked for and could not be read (specification
    /// section 6.3): `mounts` is then empty for the wrong reason.
    pub mount_list_unreadable: bool,
}

impl MountFacts {
    /// What exists behind `path`; a path the outer layer did not look up is missing.
    pub fn entry(&self, path: &Path) -> RealEntry {
        self.paths.get(path).cloned().unwrap_or(RealEntry::Missing)
    }

    /// The symbolic links resolving `path` passes through; none for a path the outer
    /// layer did not walk.
    pub fn traversed_links(&self, path: &Path) -> &[PathBuf] {
        self.links.get(path).map_or(&[], Vec::as_slice)
    }

    /// The directories resolving `path` passes through; none for a path the outer layer
    /// did not walk.
    pub fn visited_directories(&self, path: &Path) -> &[PathBuf] {
        self.directories.get(path).map_or(&[], Vec::as_slice)
    }
}

/// Whether a real path is a directory (specification section 6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Directory,
    NotDirectory,
}

/// Where a resolved item came from: a written layer, or one of the generators of
/// specification section 6.3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemOrigin {
    Written(LayerOrigin),
    Scan,
    HideMounts,
    /// The file of the secret with this environment variable name.
    SecretFile(String),
    /// The `secrets/` directory of the configuration directory.
    ConfigSecrets,
}

/// A mount item that applies: its real path and what is there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedItem {
    pub directive: Directive,
    pub real: PathBuf,
    pub kind: EntryKind,
    pub origin: ItemOrigin,
    pub written: String,
}

/// A written mount item that does not apply, with the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedItem {
    pub directive: Directive,
    pub written: String,
    pub origin: LayerOrigin,
    pub reason: String,
}

/// A scan hit the generation step left visible on purpose, with the reason (specification
/// section 6.3): the link found at `link` points into an `ro` item that cannot be swapped
/// from inside the isolation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftVisible {
    pub link: PathBuf,
    pub reason: String,
}

/// Which path-taking value a skipped path came from: a scan `root`, a `hide-mounts`
/// `under`, or an `env.path-prepend` entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkippedRole {
    ScanRoot,
    HideMountsUnder,
    PathPrepend,
}

/// A path-taking value that is not a mount item and was skipped, with the reason
/// (specification section 5.2): a variable without a value, or nothing at the path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedPath {
    pub role: SkippedRole,
    pub written: String,
    pub reason: String,
}

/// The scan roots, `hide-mounts` `under`s, and `path-prepend` entries of `expanded` that
/// are skipped (specification section 5.2): a variable without a value, or nothing at the
/// expanded path. Each is reported in the plan with its reason, as a mount item would be.
pub fn skipped_paths(expanded: &ExpandedPolicy, facts: &MountFacts) -> Vec<SkippedPath> {
    let values = expanded
        .scans
        .iter()
        .map(|scan| (SkippedRole::ScanRoot, &scan.written, &scan.root))
        .chain(
            expanded
                .hide_mounts
                .iter()
                .map(|hide| (SkippedRole::HideMountsUnder, &hide.written, &hide.under)),
        )
        .chain(
            expanded
                .path_prepend
                .iter()
                .map(|entry| (SkippedRole::PathPrepend, &entry.written, &entry.path)),
        );
    values
        .filter_map(|(role, written, expansion)| {
            let reason = match expansion {
                Expansion::Valueless(variable) => {
                    format!("`${{{}}}` has no value", variable.name())
                }
                Expansion::Path(path) if facts.entry(path) == RealEntry::Missing => {
                    "does not exist".to_string()
                }
                Expansion::Path(_) => return None,
            };
            Some(SkippedPath {
                role,
                written: written.to_string(),
                reason,
            })
        })
        .collect()
}

/// The mount items after resolution: the ones that apply, in the order of specification
/// section 6.4, the ones skipped, and the scan hits left visible.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolvedMounts {
    pub items: Vec<ResolvedItem>,
    pub skipped: Vec<SkippedItem>,
    pub left_visible: Vec<LeftVisible>,
}

/// The written mount items resolved to real paths, before the generated items of
/// specification section 6.3 are applied: the identity of section 5.4 within and across
/// the written layers, with the conflicts it finds, and the items that resolve to nothing
/// skipped. The items are in the order of section 6.4. This is the set the checks made
/// before generation (the scan `root` and the `hide-mounts` `under`, section 5.6) look at.
pub fn resolve_written(
    expanded: &ExpandedPolicy,
    facts: &MountFacts,
) -> Result<ResolvedMounts, Diagnostic> {
    let mut resolved = ResolvedMounts::default();
    let mut merged: Vec<Candidate> = Vec::new();
    // Conflicts are gathered rather than returned at once: the one reported is the first
    // in the order of section 6.4 (specification section 13), not the first written.
    let mut conflicts: Vec<(PathBuf, Diagnostic)> = Vec::new();
    for item in &expanded.mounts {
        let path = match &item.path {
            Expansion::Valueless(variable) => {
                resolved.skipped.push(SkippedItem {
                    directive: item.directive,
                    written: item.written.to_string(),
                    origin: item.origin.clone(),
                    reason: format!("`${{{}}}` has no value", variable.name()),
                });
                continue;
            }
            Expansion::Path(path) => path,
        };
        let entry = facts.entry(path);
        let candidate = Candidate {
            directive: item.directive,
            written: item.written.to_string(),
            origin: ItemOrigin::Written(item.origin.clone()),
            key: entry.path().unwrap_or(path).to_path_buf(),
            entry,
        };
        match merged
            .iter_mut()
            .find(|existing| existing.key == candidate.key)
        {
            Some(existing) if existing.origin != candidate.origin => {
                // Lower layers come first, so a later item is the upper layer replacing.
                *existing = candidate;
            }
            Some(existing) if existing.directive != candidate.directive => {
                let description = format!(
                    "`{}` and `{}` name the same path {} with different directives",
                    existing.written,
                    candidate.written,
                    candidate.key.display()
                );
                let diagnostic = match item.origin {
                    LayerOrigin::CommandLine => Diagnostic::usage(description),
                    _ => Diagnostic::policy(description),
                };
                conflicts.push((candidate.key, diagnostic));
            }
            Some(_) => {}
            None => merged.push(candidate),
        }
    }
    if let Some((_, diagnostic)) = conflicts
        .into_iter()
        .min_by(|(a, _), (b, _)| byte_order(a, b))
    {
        return Err(diagnostic);
    }
    for candidate in merged {
        let (real, kind) = match candidate.entry {
            RealEntry::Missing => {
                let ItemOrigin::Written(origin) = candidate.origin else {
                    unreachable!("the written items are all written");
                };
                resolved.skipped.push(SkippedItem {
                    directive: candidate.directive,
                    written: candidate.written,
                    origin,
                    reason: "does not exist".to_string(),
                });
                continue;
            }
            RealEntry::Directory(real) => (real, EntryKind::Directory),
            RealEntry::NotDirectory(real) => (real, EntryKind::NotDirectory),
        };
        resolved.items.push(ResolvedItem {
            directive: candidate.directive,
            real,
            kind,
            origin: candidate.origin,
            written: candidate.written,
        });
    }
    resolved.items.sort_by(|a, b| byte_order(&a.real, &b.real));
    Ok(resolved)
}

/// Applies the generated `hide` items of specification section 6.3 to the written items,
/// in the order the section gives: the scan, `hide-mounts`, the secret files, the
/// configuration directory's `secrets/`. A generated item replaces the written item at the
/// same real path (they are all `hide`, so two of them on one path collapse), the order
/// of section 6.4 is restored, and the kinds of section 6.1 are checked. `swappable_ro`
/// names, by real path, the written `ro` items whose resolution referenced something
/// inside a writable item, which the placement rules decided; the scan needs it for a link
/// pointing into an `ro` item.
pub fn generate(
    written: ResolvedMounts,
    expanded: &ExpandedPolicy,
    layers: &[Layer],
    variables: &Variables,
    facts: &MountFacts,
    swappable_ro: &[PathBuf],
) -> Result<ResolvedMounts, Diagnostic> {
    let mut resolved = written;
    let scanned = scan_items(&resolved.items, layers, facts, swappable_ro)?;
    resolved.left_visible = scanned.left_visible;
    let generated = scanned
        .hidden
        .into_iter()
        .chain(hide_mounts_items(expanded, variables, facts)?)
        .chain(secret_items(expanded, variables, facts));
    for item in generated {
        match resolved
            .items
            .iter_mut()
            .find(|existing| existing.real == item.real)
        {
            Some(existing) => *existing = item,
            None => resolved.items.push(item),
        }
    }
    resolved.items.sort_by(|a, b| byte_order(&a.real, &b.real));
    check_kinds(&resolved.items)?;
    Ok(resolved)
}

/// The real paths of the policy files that were read, for the scan to leave alone. A
/// file that no longer has a real path cannot be hit by the scan anyway.
pub fn policy_sources(layers: &[Layer], facts: &MountFacts) -> Vec<PolicySource> {
    layers
        .iter()
        .filter_map(|layer| match &layer.origin {
            LayerOrigin::Profile(path) | LayerOrigin::PolicyFile(path) => Some(PolicySource::File(
                facts.entry(path).path().unwrap_or(path).to_path_buf(),
            )),
            LayerOrigin::BuiltInDefault => Some(PolicySource::BuiltInDefault),
            LayerOrigin::CommandLine => None,
        })
        .collect()
}

/// What the scan of specification section 6.3 makes of its hits.
struct Scanned {
    hidden: Vec<ResolvedItem>,
    left_visible: Vec<LeftVisible>,
}

/// The `hide` items of the scan (specification section 6.3): each hit that is not a
/// directory nor a link to one, nor a policy file that was read. A hit that is a link
/// pointing at or into a written `ro` item is not hidden: hiding it would empty the user's
/// own read-only file. When that `ro` item could be swapped from inside the isolation
/// (`swappable_ro`), the scan could be made to empty something else next time, so the run
/// stops, naming the first such link in byte order whatever order the walk found them in;
/// otherwise the link is left visible with the reason.
fn scan_items(
    written: &[ResolvedItem],
    layers: &[Layer],
    facts: &MountFacts,
    swappable_ro: &[PathBuf],
) -> Result<Scanned, Diagnostic> {
    let sources = policy_sources(layers, facts);
    let policy_files: Vec<&Path> = sources.iter().filter_map(PolicySource::path).collect();
    let mut hits: Vec<&ScanHit> = facts.scan_hits.iter().collect();
    hits.sort_by(|a, b| byte_order(&a.found_at, &b.found_at));
    let mut scanned = Scanned {
        hidden: Vec::new(),
        left_visible: Vec::new(),
    };
    for hit in hits {
        // A directory, or a link whose target is a directory, is not hidden; a link to
        // nothing names nothing to hide.
        let RealEntry::NotDirectory(real) = &hit.target else {
            continue;
        };
        if policy_files.contains(&real.as_path()) {
            continue;
        }
        let into_ro = written
            .iter()
            .filter(|item| item.directive == Directive::Ro)
            .find(|item| hit.is_link && real.starts_with(&item.real));
        match into_ro {
            Some(ro) if swappable_ro.contains(&ro.real) => {
                return Err(Diagnostic::path(format!(
                    "the scan found the symbolic link {} pointing at {}, inside the `ro` item \
                     `{}` at {}, whose resolution passes through a writable item, so the link \
                     and the item could be re-pointed from inside the isolation and the next \
                     start made to empty another file",
                    hit.found_at.display(),
                    real.display(),
                    ro.written,
                    ro.real.display()
                )));
            }
            Some(ro) => scanned.left_visible.push(LeftVisible {
                link: hit.found_at.clone(),
                reason: format!(
                    "points at {}, inside the `ro` item `{}` at {}, which cannot be swapped \
                     from inside the isolation; hiding it would empty the read-only file",
                    real.display(),
                    ro.written,
                    ro.real.display()
                ),
            }),
            None => {
                scanned
                    .hidden
                    .extend(hidden(&hit.found_at, ItemOrigin::Scan, hit.target.clone()))
            }
        }
    }
    Ok(scanned)
}

/// The `hide` items of `hide-mounts` (specification section 6.3): each mount under an
/// `under` whose file system type is listed, except the places the user chose to work in.
/// With a `hide-mounts` written and no mount list to read, a mount to hide might be there
/// unseen (a `/proc` restricted by another sandbox), so the run does not start.
fn hide_mounts_items(
    expanded: &ExpandedPolicy,
    variables: &Variables,
    facts: &MountFacts,
) -> Result<Vec<ResolvedItem>, Diagnostic> {
    if facts.mount_list_unreadable && !expanded.hide_mounts.is_empty() {
        return Err(Diagnostic::path(
            "`mounts.hide-mounts` is written but the mount list could not be read, so a \
             mount to hide might be missed",
        ));
    }
    // The places the user chose to work in are never hidden by their mount type.
    let work_places = [
        Some(variables.workspace.as_path()),
        Some(variables.worktree.as_path()),
        variables.git_common_dir.as_deref(),
    ];
    let mut generated = Vec::new();
    for hide_mounts in &expanded.hide_mounts {
        let Some(under) = hide_mounts.under.path() else {
            continue;
        };
        let Some(under) = facts.entry(under).path().map(Path::to_path_buf) else {
            continue;
        };
        for mount in &facts.mounts {
            if !mount.target.starts_with(&under)
                || !hide_mounts.fstype.contains(&mount.fstype)
                || work_places.contains(&Some(mount.target.as_path()))
            {
                continue;
            }
            generated.extend(hidden(
                &mount.target,
                ItemOrigin::HideMounts,
                facts.entry(&mount.target),
            ));
        }
    }
    Ok(generated)
}

/// The `hide` items of the secrets (specification sections 6.3 and 9): each secret file
/// that exists, then the configuration directory's `secrets/`, hidden so that a secret
/// file the policy does not name cannot be read from inside.
fn secret_items(
    expanded: &ExpandedPolicy,
    variables: &Variables,
    facts: &MountFacts,
) -> Vec<ResolvedItem> {
    let mut generated = Vec::new();
    // A secret file that does not exist is the warning of section 9, not a hide.
    for (name, path) in &expanded.secrets {
        if let Some(path) = path.path() {
            generated.extend(hidden(
                path,
                ItemOrigin::SecretFile(name.clone()),
                facts.entry(path),
            ));
        }
    }
    if let Some(config_dir) = &variables.config_dir {
        let config_secrets = config_dir.join("secrets");
        generated.extend(hidden(
            &config_secrets,
            ItemOrigin::ConfigSecrets,
            facts.entry(&config_secrets),
        ));
    }
    generated
}

/// A written item with its identity: the real path when something exists, else the
/// expanded path as written (specification section 5.4).
struct Candidate {
    directive: Directive,
    written: String,
    origin: ItemOrigin,
    key: PathBuf,
    entry: RealEntry,
}

/// A generated `hide` on what exists at `written`; nothing when nothing exists.
fn hidden(written: &Path, origin: ItemOrigin, entry: RealEntry) -> Option<ResolvedItem> {
    let (real, kind) = match entry {
        RealEntry::Missing => return None,
        RealEntry::Directory(real) => (real, EntryKind::Directory),
        RealEntry::NotDirectory(real) => (real, EntryKind::NotDirectory),
    };
    Some(ResolvedItem {
        directive: Directive::Hide,
        real,
        kind,
        origin,
        written: written.display().to_string(),
    })
}

/// `rw` takes a directory and `rw-file` anything else (specification section 6.1).
fn check_kinds(items: &[ResolvedItem]) -> Result<(), Diagnostic> {
    for item in items {
        match (item.directive, item.kind) {
            (Directive::Rw, EntryKind::NotDirectory) => {
                return Err(Diagnostic::path(format!(
                    "`rw` needs a directory but `{}` is {}; use `rw-file` for a file",
                    item.written,
                    item.real.display()
                )));
            }
            (Directive::RwFile, EntryKind::Directory) => {
                return Err(Diagnostic::path(format!(
                    "`rw-file` takes a file but `{}` is the directory {}",
                    item.written,
                    item.real.display()
                )));
            }
            _ => {}
        }
    }
    Ok(())
}

/// The order of specification section 6.4. An ancestor is a proper prefix of its
/// descendants' bytes, so the byte order of the real path already puts ancestors first and
/// unrelated paths in byte order. `Path`'s own order compares by component and would put
/// `/a/b` before `/a-x`.
pub(crate) fn byte_order(a: &Path, b: &Path) -> std::cmp::Ordering {
    a.as_os_str().as_bytes().cmp(b.as_os_str().as_bytes())
}
