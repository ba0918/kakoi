//! The mount items of the merged policy: the expansion of `~` and the variables
//! (specification section 5.2) and their resolution to real paths against the facts the
//! outer layer collected (sections 5.4 and 6). Pure.

use std::collections::BTreeMap;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use crate::diagnostic::Diagnostic;
use crate::environment::{HomeDirectory, RealEntry};
use crate::layers::{Directive, Layer, LayerOrigin, Policy};
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
    pub path_prepend: Vec<Expansion>,
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
                Variable::ConfigDir => Some(variables.config_dir.as_path()),
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
        path_prepend: policy.path_prepend.iter().map(expand).collect(),
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
        .chain(expanded.path_prepend.iter());
    paths.extend(
        expanded_paths
            .filter_map(Expansion::path)
            .map(Path::to_path_buf),
    );
    let mut traversals: Vec<PathBuf> = layers
        .iter()
        .filter_map(|layer| match &layer.origin {
            LayerOrigin::Profile(path) | LayerOrigin::PolicyFile(path) => Some(path.as_path()),
            LayerOrigin::CommandLine => None,
        })
        .chain(std::iter::once(config_dir))
        .chain(expanded.secrets.values().filter_map(Expansion::path))
        .chain(expanded.path_prepend.iter().filter_map(Expansion::path))
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
    paths.push(variables.config_dir.join("secrets"));
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

/// One entry the scan found by name: where it was found and what is behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanHit {
    pub found_at: PathBuf,
    pub target: RealEntry,
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

/// The mount items after resolution: the ones that apply, in the order of specification
/// section 6.4, and the ones skipped.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolvedMounts {
    pub items: Vec<ResolvedItem>,
    pub skipped: Vec<SkippedItem>,
}

/// Resolves the expanded mount items to real paths: the written layers with the identity
/// of specification section 5.4 (`resolve_written`), then the generated items of
/// section 6.3 and the kinds of section 6.1 (`generate`).
pub fn resolve_mounts(
    expanded: &ExpandedPolicy,
    layers: &[Layer],
    variables: &Variables,
    facts: &MountFacts,
) -> Result<ResolvedMounts, Diagnostic> {
    let written = resolve_written(expanded, facts)?;
    generate(written, expanded, layers, variables, facts)
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

/// Applies the generated `hide` items of specification section 6.3 to the written items:
/// a generated item replaces the written item at the same real path (they are all `hide`,
/// so two of them on one path collapse), the order of section 6.4 is restored, and the
/// kinds of section 6.1 are checked.
pub fn generate(
    written: ResolvedMounts,
    expanded: &ExpandedPolicy,
    layers: &[Layer],
    variables: &Variables,
    facts: &MountFacts,
) -> Result<ResolvedMounts, Diagnostic> {
    let mut resolved = written;
    for generated in generated_items(expanded, layers, variables, facts) {
        match resolved
            .items
            .iter_mut()
            .find(|existing| existing.real == generated.real)
        {
            Some(existing) => *existing = generated,
            None => resolved.items.push(generated),
        }
    }
    resolved.items.sort_by(|a, b| byte_order(&a.real, &b.real));
    check_kinds(&resolved.items)?;
    Ok(resolved)
}

/// The real paths of the policy files that were read, for the scan to leave alone. A
/// file that no longer has a real path cannot be hit by the scan anyway.
pub fn loaded_policy_files(layers: &[Layer], facts: &MountFacts) -> Vec<PathBuf> {
    layers
        .iter()
        .filter_map(|layer| match &layer.origin {
            LayerOrigin::Profile(path) | LayerOrigin::PolicyFile(path) => Some(path),
            LayerOrigin::CommandLine => None,
        })
        .map(|path| facts.entry(path).path().unwrap_or(path).to_path_buf())
        .collect()
}

/// The `hide` items of specification section 6.3, from the facts.
fn generated_items(
    expanded: &ExpandedPolicy,
    layers: &[Layer],
    variables: &Variables,
    facts: &MountFacts,
) -> Vec<ResolvedItem> {
    let policy_files = loaded_policy_files(layers, facts);
    // The places the user chose to work in are never hidden by their mount type.
    let work_places = [
        Some(variables.workspace.as_path()),
        Some(variables.worktree.as_path()),
        variables.git_common_dir.as_deref(),
    ];
    let mut generated = Vec::new();
    // Hidden so that a secret file the policy does not name cannot be read from inside.
    let config_secrets = variables.config_dir.join("secrets");
    generated.extend(hidden(
        &config_secrets,
        ItemOrigin::ConfigSecrets,
        facts.entry(&config_secrets),
    ));
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
    for hit in &facts.scan_hits {
        // A directory, or a link whose target is a directory, is not hidden; a link to
        // nothing names nothing to hide.
        if let RealEntry::NotDirectory(real) = &hit.target {
            if policy_files.contains(real) {
                continue;
            }
            generated.extend(hidden(&hit.found_at, ItemOrigin::Scan, hit.target.clone()));
        }
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
