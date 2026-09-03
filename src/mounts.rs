//! The mount items of the merged policy: the expansion of `~` and the variables
//! (specification section 5.2) and their resolution to real paths against the facts the
//! outer layer collected (sections 5.4 and 6). Pure.

use std::collections::BTreeMap;
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

/// A mount item that applies: its real path and what is there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedItem {
    pub directive: Directive,
    pub real: PathBuf,
    pub kind: EntryKind,
    pub origin: LayerOrigin,
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
    _layers: &[Layer],
    _variables: &Variables,
    facts: &MountFacts,
) -> Result<ResolvedMounts, Diagnostic> {
    let mut resolved = ResolvedMounts::default();
    for item in &expanded.mounts {
        let skip = |reason: String| SkippedItem {
            directive: item.directive,
            written: item.written.to_string(),
            origin: item.origin.clone(),
            reason,
        };
        let path = match &item.path {
            Expansion::Valueless(variable) => {
                resolved
                    .skipped
                    .push(skip(format!("`${{{}}}` has no value", variable.name())));
                continue;
            }
            Expansion::Path(path) => path,
        };
        let (real, kind) = match facts.entry(path) {
            RealEntry::Missing => {
                resolved.skipped.push(skip("does not exist".to_string()));
                continue;
            }
            RealEntry::Directory(real) => (real, EntryKind::Directory),
            RealEntry::NotDirectory(real) => (real, EntryKind::NotDirectory),
        };
        resolved.items.push(ResolvedItem {
            directive: item.directive,
            real,
            kind,
            origin: item.origin.clone(),
            written: item.written.to_string(),
        });
    }
    Ok(resolved)
}
