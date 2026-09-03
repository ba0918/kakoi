//! Collects the facts of `variables::WorkspaceFacts` from the file system. Reads only the
//! workspace's ancestors' `.git` entries and, for a `.git` file, `gitdir:`, `commondir`,
//! `gitdir`, and `config` under the directory it names (specification section 14). Runs no
//! command.

use std::fs;
use std::path::{Path, PathBuf};

use crate::environment::RealEntry;
use crate::regular_file::{read_regular_file, Links, ReadError};
use crate::variables::{
    core_worktree, worktree_marker, Ancestor, DotGit, GitFileLinks, Reference, WorkspaceFacts,
};

/// What exists behind `path`, following symbolic links.
pub fn real_entry(path: &Path) -> RealEntry {
    let Ok(real) = fs::canonicalize(path) else {
        return RealEntry::Missing;
    };
    match fs::metadata(&real) {
        Ok(metadata) if metadata.is_dir() => RealEntry::Directory(real),
        Ok(_) => RealEntry::NotDirectory(real),
        Err(_) => RealEntry::Missing,
    }
}

/// Reads the facts for `workspace`.
pub fn collect_workspace_facts(workspace: &Path) -> WorkspaceFacts {
    let workspace = real_entry(workspace);
    let RealEntry::Directory(workspace) = &workspace else {
        return WorkspaceFacts {
            workspace,
            ancestors: Vec::new(),
            links: None,
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
        workspace: RealEntry::Directory(workspace.clone()),
        ancestors,
        links,
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
    let gitdir = read_git_file(&worktree.join(".git"))
        .text()
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
    // A `commondir` that exists but cannot be used is not "no commondir" (specification
    // section 5.2), so it does not open the submodule case.
    let commondir = match read_git_file(&gitdir.join("commondir")) {
        GitFile::Absent => Reference::Absent,
        GitFile::Unusable => Reference::Unresolvable,
        GitFile::Text(text) => match resolve_from(&gitdir, text.trim_end()) {
            Some(path) => Reference::Resolved(path),
            None => Reference::Unresolvable,
        },
    };
    let back_link = read_git_file(&gitdir.join("gitdir"))
        .text()
        .and_then(|text| resolve_from(&gitdir, text.trim_end()));
    // Only a submodule's config is read (specification section 14); a linked worktree is
    // recognised by its commondir and needs no config.
    let core_worktree = match commondir {
        Reference::Absent => read_git_file(&gitdir.join("config"))
            .text()
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

/// What is found at the name of a file git may have written.
enum GitFile {
    /// Nothing exists at the name.
    Absent,
    /// Something exists there but is not a regular file within the reading limit, or could
    /// not be read.
    Unusable,
    Text(String),
}

impl GitFile {
    fn text(self) -> Option<String> {
        match self {
            GitFile::Text(text) => Some(text),
            _ => None,
        }
    }
}

/// Reads a file git may have written. The directory it sits in can be written from inside
/// the isolation, so a link at the name is not followed (specification section 5.2), and
/// the reading rules of section 14 apply.
fn read_git_file(path: &Path) -> GitFile {
    match read_regular_file(path, Links::DoNotFollow) {
        Ok(bytes) => String::from_utf8(bytes).map_or(GitFile::Unusable, GitFile::Text),
        Err(ReadError::Absent) => GitFile::Absent,
        Err(_) => GitFile::Unusable,
    }
}

/// The real path of `target` taken relative to `base`; `None` when nothing exists there.
fn resolve_from(base: &Path, target: &str) -> Option<PathBuf> {
    fs::canonicalize(base.join(target)).ok()
}
