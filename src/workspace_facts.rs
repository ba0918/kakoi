//! Collects the facts of `variables::WorkspaceFacts` from the file system. Reads only the
//! workspace's ancestors' `.git` entries and, for a `.git` file, `gitdir:`, `commondir`,
//! `gitdir`, and `config` under the directory it names (specification section 14). Runs no
//! command.

use std::fs;
use std::path::{Path, PathBuf};

use crate::environment::HostEnvironment;
use crate::variables::{
    core_worktree, worktree_marker, Ancestor, DotGit, GitFileLinks, Reference, WorkspaceFacts,
};

/// Reads the facts for `workspace`.
pub fn collect_workspace_facts(workspace: &Path, env: &HostEnvironment) -> WorkspaceFacts {
    let home = env
        .home
        .as_ref()
        .map(|home| fs::canonicalize(home).unwrap_or_else(|_| home.clone()));
    let Ok(workspace) = fs::canonicalize(workspace) else {
        return WorkspaceFacts {
            workspace: None,
            ancestors: Vec::new(),
            links: None,
            home,
        };
    };
    let ancestors: Vec<Ancestor> = workspace
        .ancestors()
        .map(|path| Ancestor {
            path: path.to_path_buf(),
            dot_git: dot_git_kind(&path.join(".git")),
        })
        .collect();
    let links = worktree_marker(&ancestors)
        .filter(|marker| marker.dot_git == DotGit::File)
        .map(|marker| read_links(&marker.path));
    WorkspaceFacts {
        workspace: Some(workspace),
        ancestors,
        links,
        home,
    }
}

fn dot_git_kind(path: &Path) -> DotGit {
    match fs::symlink_metadata(path) {
        Err(_) => DotGit::Absent,
        Ok(metadata) => {
            let kind = metadata.file_type();
            if kind.is_symlink() {
                DotGit::Symlink
            } else if kind.is_dir() {
                DotGit::Directory
            } else if kind.is_file() {
                DotGit::File
            } else {
                DotGit::Other
            }
        }
    }
}

/// Reads the `.git` file of `worktree` and the links under the directory it names.
fn read_links(worktree: &Path) -> GitFileLinks {
    let gitdir = fs::read_to_string(worktree.join(".git"))
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|line| line.strip_prefix("gitdir:"))
                .map(|rest| rest.trim().to_string())
        })
        .and_then(|target| fs::canonicalize(worktree.join(target)).ok());
    let Some(gitdir) = gitdir else {
        return GitFileLinks {
            gitdir: None,
            commondir: Reference::Absent,
            back_link: None,
            core_worktree: None,
        };
    };
    let commondir = match fs::read_to_string(gitdir.join("commondir")) {
        Err(_) => Reference::Absent,
        Ok(text) => match resolve_from(&gitdir, text.trim_end()) {
            Some(path) => Reference::Resolved(path),
            None => Reference::Unresolvable,
        },
    };
    let back_link = fs::read_to_string(gitdir.join("gitdir"))
        .ok()
        .and_then(|text| resolve_from(&gitdir, text.trim_end()));
    // Only a submodule's config is read (specification section 14); a linked worktree is
    // recognised by its commondir and needs no config.
    let core_worktree = match commondir {
        Reference::Absent => fs::read_to_string(gitdir.join("config"))
            .ok()
            .and_then(|text| core_worktree(&text))
            .and_then(|value| resolve_from(&gitdir, &value)),
        _ => None,
    };
    GitFileLinks {
        gitdir: Some(gitdir),
        commondir,
        back_link,
        core_worktree,
    }
}

/// The real path of `target` taken relative to `base`; `None` when nothing exists there.
fn resolve_from(base: &Path, target: &str) -> Option<PathBuf> {
    fs::canonicalize(base.join(target)).ok()
}
