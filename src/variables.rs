//! The four policy variables, derived from the workspace and the facts git left on disk
//! (specification sections 2, 5.2, and 6.5). Pure: the facts come from
//! `workspace_facts`.

use std::path::{Path, PathBuf};

use crate::diagnostic::Diagnostic;
use crate::environment::{HomeDirectory, RealEntry};

/// What the `.git` entry of a directory is, looked at without following symbolic links.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DotGit {
    Absent,
    Directory,
    File,
    Symlink,
    Other,
}

/// A directory on the way from the workspace up to `/`, with its `.git` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ancestor {
    pub path: PathBuf,
    pub dot_git: DotGit,
}

/// A file git wrote that names a path: absent, present but unusable (naming nothing that
/// exists, not a regular file, or too large to be git's), or resolved to a real path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reference {
    Absent,
    Unresolvable,
    Resolved(PathBuf),
}

/// The links git wrote for a `.git` file, each resolved to a real path by the outer layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitFileLinks {
    /// G: the directory named by `gitdir:`; `None` when the line is missing or the
    /// directory does not exist.
    pub gitdir: Option<PathBuf>,
    /// `G/commondir`, resolved relative to G.
    pub commondir: Reference,
    /// `G/gitdir`, the back link; `None` when absent or naming nothing that exists.
    pub back_link: Option<PathBuf>,
    /// `core.worktree` of `G/config`, resolved relative to G; `None` when absent or naming
    /// nothing that exists.
    pub core_worktree: Option<PathBuf>,
}

/// The facts about the workspace and its git metadata that the variables are derived from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceFacts {
    /// What exists behind the workspace path.
    pub workspace: RealEntry,
    /// The workspace and each of its ancestors up to `/`, nearest first.
    pub ancestors: Vec<Ancestor>,
    /// The links of the worktree's `.git` file, when that marker is a regular file.
    pub links: Option<GitFileLinks>,
}

/// The values of `${workspace}`, `${worktree}`, `${git_common_dir}`, and `${config_dir}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variables {
    pub workspace: PathBuf,
    pub worktree: PathBuf,
    pub git_common_dir: Option<PathBuf>,
    pub config_dir: PathBuf,
}

/// The first ancestor whose `.git` is a directory or a regular file, if any.
pub fn worktree_marker(ancestors: &[Ancestor]) -> Option<&Ancestor> {
    ancestors
        .iter()
        .find(|ancestor| matches!(ancestor.dot_git, DotGit::Directory | DotGit::File))
}

/// The value of `core.worktree` in the text of a git configuration file: the `worktree` key
/// of the `[core]` section, the last one when repeated, without surrounding double quotes.
pub fn core_worktree(config: &str) -> Option<String> {
    let mut in_core = false;
    let mut value = None;
    for line in config.lines() {
        let line = line.trim();
        if let Some(section) = line.strip_prefix('[') {
            in_core = section.trim_end_matches(']').trim() == "core";
            continue;
        }
        if !in_core {
            continue;
        }
        let Some((key, rest)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "worktree" {
            continue;
        }
        let rest = rest.trim();
        let unquoted = rest
            .strip_prefix('"')
            .and_then(|inner| inner.strip_suffix('"'))
            .unwrap_or(rest);
        value = Some(unquoted.to_string());
    }
    value
}

/// Derives the variables, or the diagnostic that stops the run. `config_dir` is what the
/// outer layer found behind the configuration directory.
pub fn derive_variables(
    home: &HomeDirectory,
    config_dir: &RealEntry,
    facts: &WorkspaceFacts,
) -> Result<Variables, Diagnostic> {
    let home = home.path();
    let config_dir = config_dir
        .path()
        .ok_or_else(|| Diagnostic::path("the configuration directory has no real path"))?;
    let workspace = match &facts.workspace {
        RealEntry::Directory(path) => path.clone(),
        RealEntry::NotDirectory(path) => {
            return Err(Diagnostic::path(format!(
                "the workspace {} is not a directory",
                path.display()
            )))
        }
        RealEntry::Missing => return Err(Diagnostic::path("the workspace does not exist")),
    };
    let marker = worktree_marker(&facts.ancestors);
    let worktree = marker.map_or(workspace.clone(), |marker| marker.path.clone());
    for (name, path) in [("worktree", &worktree), ("workspace", &workspace)] {
        if is_or_ancestor_of(path, home) {
            return Err(Diagnostic::path(format!(
                "the {name} {} is `/`, the home directory, or an ancestor of it",
                path.display()
            )));
        }
    }
    let git_common_dir = match marker.map(|marker| marker.dot_git) {
        None => None,
        Some(DotGit::Directory) => Some(worktree.join(".git")),
        Some(_) => Some(verified_common_dir(&worktree, facts.links.as_ref())?),
    };
    Ok(Variables {
        workspace,
        worktree,
        git_common_dir,
        config_dir: config_dir.to_path_buf(),
    })
}

/// The shared `.git` a `.git` file points to, accepted only when git's own back link agrees
/// (specification section 5.2).
fn verified_common_dir(
    worktree: &Path,
    links: Option<&GitFileLinks>,
) -> Result<PathBuf, Diagnostic> {
    let reject = |reason: &str| {
        Diagnostic::path(format!("the .git file of {} {reason}", worktree.display()))
    };
    let links = links.ok_or_else(|| reject("could not be read"))?;
    let gitdir = links
        .gitdir
        .as_deref()
        .ok_or_else(|| reject("names a gitdir that does not exist"))?;
    match &links.commondir {
        Reference::Resolved(common) => {
            let under_worktrees = gitdir.parent() == Some(common.join("worktrees").as_path());
            let links_back = links.back_link.as_deref() == Some(worktree.join(".git").as_path());
            if under_worktrees && links_back {
                return Ok(common.clone());
            }
            Err(reject(
                "is not a linked worktree that its gitdir links back to",
            ))
        }
        Reference::Absent => {
            if links.core_worktree.as_deref() == Some(worktree) {
                return Ok(gitdir.to_path_buf());
            }
            Err(reject(
                "is not a submodule whose core.worktree points back to it",
            ))
        }
        Reference::Unresolvable => Err(reject("has a commondir that names nothing")),
    }
}

/// Whether `candidate` is `/`, `path`, or an ancestor of `path`. `/` is named on its own so
/// that the rule holds whatever `path` looks like.
fn is_or_ancestor_of(candidate: &Path, path: &Path) -> bool {
    candidate == Path::new("/") || path.ancestors().any(|ancestor| ancestor == candidate)
}
