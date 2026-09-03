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

/// One `mounts.scan` entry with its root expanded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedScan {
    pub root: Expansion,
    pub names: Vec<String>,
    pub exclude: Vec<String>,
    pub prune: Vec<String>,
}

/// One `mounts.hide-mounts` entry with its `under` expanded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedHideMounts {
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
/// kind, and real path are needed, the scans to walk, and the `under` of each
/// `hide-mounts` (the mount list is read only when there is one).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Candidates {
    pub paths: Vec<PathBuf>,
    pub scans: Vec<ScanRequest>,
    pub hide_mounts_under: Vec<PathBuf>,
}

/// The candidate paths of `expanded`: every expanded path, plus every prefix (each
/// ancestor and the path itself) of the paths specification section 5.6 protects — the
/// policy files read, the configuration directory, and the secret files — and the
/// configuration directory's `secrets/`.
pub fn candidates(
    expanded: &ExpandedPolicy,
    layers: &[Layer],
    variables: &Variables,
    config_dir: &Path,
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
    let protected = layers
        .iter()
        .filter_map(|layer| match &layer.origin {
            LayerOrigin::Profile(path) | LayerOrigin::PolicyFile(path) => Some(path.as_path()),
            LayerOrigin::CommandLine => None,
        })
        .chain(std::iter::once(config_dir))
        .chain(expanded.secrets.values().filter_map(Expansion::path));
    for path in protected {
        paths.extend(path.ancestors().map(Path::to_path_buf));
    }
    paths.push(variables.config_dir.join("secrets"));
    paths.sort();
    paths.dedup();
    Candidates {
        paths,
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
    pub scan_hits: Vec<ScanHit>,
    pub mounts: Vec<Mount>,
}

impl MountFacts {
    /// What exists behind `path`; a path the outer layer did not look up is missing.
    pub fn entry(&self, path: &Path) -> RealEntry {
        self.paths.get(path).cloned().unwrap_or(RealEntry::Missing)
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

/// Resolves the expanded mount items to real paths.
pub fn resolve_mounts(
    expanded: &ExpandedPolicy,
    layers: &[Layer],
    variables: &Variables,
    facts: &MountFacts,
) -> Result<ResolvedMounts, Diagnostic> {
    let mut resolved = ResolvedMounts::default();
    let mut merged: Vec<Candidate> = Vec::new();
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
                return Err(match item.origin {
                    LayerOrigin::CommandLine => Diagnostic::usage(description),
                    _ => Diagnostic::policy(description),
                });
            }
            Some(_) => {}
            None => merged.push(candidate),
        }
    }
    for generated in generated_items(expanded, layers, variables, facts) {
        match merged
            .iter_mut()
            .find(|existing| existing.key == generated.key)
        {
            // A generated item replaces a written one; generated items are all `hide`, so
            // two of them on one path collapse.
            Some(existing) => *existing = generated,
            None => merged.push(generated),
        }
    }
    for candidate in merged {
        let (real, kind) = match candidate.entry {
            RealEntry::Missing => {
                let ItemOrigin::Written(origin) = candidate.origin else {
                    unreachable!("generated items come from things that exist");
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
) -> Vec<Candidate> {
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
    generated.extend(Candidate::hidden(
        &config_secrets,
        ItemOrigin::ConfigSecrets,
        facts.entry(&config_secrets),
    ));
    // A secret file that does not exist is the warning of section 9, not a hide.
    for (name, path) in &expanded.secrets {
        if let Some(path) = path.path() {
            generated.extend(Candidate::hidden(
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
            generated.extend(Candidate::hidden(
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
            generated.extend(Candidate::hidden(
                &hit.found_at,
                ItemOrigin::Scan,
                hit.target.clone(),
            ));
        }
    }
    generated
}

/// An item with its identity: the real path when something exists, else the expanded path
/// as written (specification section 5.4).
struct Candidate {
    directive: Directive,
    written: String,
    origin: ItemOrigin,
    key: PathBuf,
    entry: RealEntry,
}

impl Candidate {
    /// A generated `hide` on what exists at `written`; nothing when nothing exists.
    fn hidden(written: &Path, origin: ItemOrigin, entry: RealEntry) -> Option<Self> {
        let key = entry.path()?.to_path_buf();
        Some(Self {
            directive: Directive::Hide,
            written: written.display().to_string(),
            origin,
            key,
            entry,
        })
    }
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
fn byte_order(a: &Path, b: &Path) -> std::cmp::Ordering {
    a.as_os_str().as_bytes().cmp(b.as_os_str().as_bytes())
}
