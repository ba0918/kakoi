//! Collects the facts of `variables::WorkspaceFacts` from the file system. Reads only the
//! workspace's ancestors' `.git` entries and, for a `.git` file, `gitdir:`, `commondir`,
//! `gitdir`, and `config` under the directory it names, and the kind of `HEAD` under the
//! common dir (specification section 14). Runs no command.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::environment::{PathState, RealEntry};
use crate::regular_file::{read_regular_file, Links, ReadError};
use crate::variables::{
    core_worktree, worktree_marker, Ancestor, GitEntry, GitFileLinks, Reference, WorkspaceFacts,
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

/// What is at `path` itself: nothing by that name, something that exists but reaches no
/// real path, or the real entry behind it. Only `ErrorKind::NotFound` says the name is not
/// there; every other error (no search bit on the parent, a loop, a name too long) says the
/// name cannot be looked at, which specification section 5.3 counts as being there.
pub fn entry_state(path: &Path) -> PathState {
    match fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => return PathState::Absent,
        Err(_) => return PathState::Broken,
    }
    match real_entry(path) {
        RealEntry::Missing => PathState::Broken,
        RealEntry::Directory(real) => PathState::Directory(real),
        RealEntry::NotDirectory(real) => PathState::NotDirectory(real),
    }
}

/// The state of `path` with its components looked at from the root: the first component
/// that fails decides (specification section 5.3). Only a component that does not exist by
/// name makes the whole path absent; a dangling link or a regular file where a directory
/// must be makes it broken.
pub fn probe_path(path: &Path) -> PathState {
    let mut ancestors: Vec<&Path> = path.ancestors().skip(1).collect();
    ancestors.reverse();
    for ancestor in ancestors {
        match entry_state(ancestor) {
            PathState::Directory(_) => {}
            PathState::Absent => return PathState::Absent,
            PathState::Broken | PathState::NotDirectory(_) => return PathState::Broken,
        }
    }
    entry_state(path)
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
            dot_git: git_entry_kind(&path.join(".git")),
        })
        .collect();
    let links = worktree_marker(&ancestors)
        .filter(|marker| marker.dot_git == GitEntry::File)
        .map(|marker| read_links(&marker.path));
    WorkspaceFacts {
        workspace: RealEntry::Directory(workspace.clone()),
        ancestors,
        links,
    }
}

/// What is at `path`, looked at without following a symbolic link there.
fn git_entry_kind(path: &Path) -> GitEntry {
    match fs::symlink_metadata(path) {
        Err(_) => GitEntry::Absent,
        Ok(metadata) => {
            let kind = metadata.file_type();
            if kind.is_symlink() {
                GitEntry::Symlink
            } else if kind.is_dir() {
                GitEntry::Directory
            } else if kind.is_file() {
                GitEntry::File
            } else {
                GitEntry::Other
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
            common_head: GitEntry::Absent,
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
    let core_worktree = match &commondir {
        Reference::Absent => read_git_file(&gitdir.join("config"))
            .text()
            .and_then(|text| core_worktree(&text))
            .and_then(|value| resolve_from(&gitdir, &value)),
        _ => None,
    };
    let common_head = match &commondir {
        Reference::Resolved(common) => git_entry_kind(&common.join("HEAD")),
        _ => GitEntry::Absent,
    };
    GitFileLinks {
        gitdir: Some(gitdir),
        commondir,
        back_link,
        core_worktree,
        common_head,
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
