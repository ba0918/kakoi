//! Collects the facts of `variables::WorkspaceFacts` from the file system. Reads only the
//! workspace's ancestors' `.git` entries and, for a `.git` file, `gitdir:`, `commondir`,
//! `gitdir`, and `config` under the directory it names (specification section 14). Runs no
//! command.

use std::fs::{self, OpenOptions};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
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

/// The most a file git wrote is read up to. Git writes one path per file, or a small
/// configuration; anything larger was not written by git.
const GIT_FILE_LIMIT: u64 = 1 << 20;

/// What is found at the name of a file git may have written.
enum GitFile {
    /// Nothing exists at the name.
    Absent,
    /// Something exists there but is not a regular file within `GIT_FILE_LIMIT`, or could
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
/// the isolation, so nothing that is not a regular file is used, symbolic links are not
/// followed, and the length is bounded. The entry can be swapped for a FIFO between the
/// type check and the open, and an open that waits for a writer would stop the start-up,
/// so the open itself refuses to follow a link and does not wait; a regular file is read
/// the same way either way.
fn read_git_file(path: &Path) -> GitFile {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return GitFile::Absent;
    };
    if !metadata.file_type().is_file() {
        return GitFile::Unusable;
    }
    let Ok(file) = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    else {
        return GitFile::Unusable;
    };
    // Checked again on the open descriptor: the entry may have been replaced since.
    match file.metadata() {
        Ok(metadata) if metadata.file_type().is_file() => {}
        _ => return GitFile::Unusable,
    }
    let mut text = String::new();
    if file
        .take(GIT_FILE_LIMIT + 1)
        .read_to_string(&mut text)
        .is_err()
        || text.len() as u64 > GIT_FILE_LIMIT
    {
        return GitFile::Unusable;
    }
    GitFile::Text(text)
}

/// The real path of `target` taken relative to `base`; `None` when nothing exists there.
fn resolve_from(base: &Path, target: &str) -> Option<PathBuf> {
    fs::canonicalize(base.join(target)).ok()
}
