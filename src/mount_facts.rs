//! Collects `mounts::MountFacts` from the file system: what is behind each candidate path,
//! what the resolution of each protected path passes through, the scan hits, and the mount
//! list (specification section 14). Runs no command.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::mount_list::{read_mount_list, MOUNTINFO};
use crate::mounts::{Candidates, MountFacts};
use crate::scan::scan;
use crate::workspace_facts::real_entry;

/// How many symbolic links one resolution follows before giving up: what the kernel
/// allows a path lookup.
const LINK_LIMIT: usize = 40;

/// Looks up every candidate path, walks the resolution of every path whose traversal is
/// asked for, walks every scan from its real root, and, when a `hide-mounts` asks for it,
/// reads the mount list and looks up each mount target under its `under`.
pub fn collect_mount_facts(candidates: &Candidates) -> MountFacts {
    let mut facts = MountFacts::default();
    for path in &candidates.paths {
        facts.paths.insert(path.clone(), real_entry(path));
    }
    for path in &candidates.traversals {
        let traversal = traverse(path);
        facts.links.insert(path.clone(), traversal.links);
        facts
            .directories
            .insert(path.clone(), traversal.directories);
    }
    for request in &candidates.scans {
        if let Some(root) = facts.entry(&request.root).path() {
            facts
                .scan_hits
                .extend(scan(root, &request.names, &request.exclude, &request.prune));
        }
    }
    if !candidates.hide_mounts_under.is_empty() {
        facts.mounts = read_mount_list(Path::new(MOUNTINFO));
        let unders: Vec<_> = candidates
            .hide_mounts_under
            .iter()
            .filter_map(|under| real_entry(under).path().map(Path::to_path_buf))
            .collect();
        for mount in &facts.mounts {
            if unders.iter().any(|under| mount.target.starts_with(under)) {
                facts
                    .paths
                    .insert(mount.target.clone(), real_entry(&mount.target));
            }
        }
    }
    facts
}

/// What resolving one path passed through: the symbolic links, each by its own place (its
/// parent's real path and its name), in the order they are followed, and the directories,
/// each by its real path, whose entries the walk looked up, each once.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Traversal {
    pub links: Vec<PathBuf>,
    pub directories: Vec<PathBuf>,
}

/// Walks the resolution of the absolute `path` as the kernel does: component by
/// component, a link's target spliced in where the link was and `..` taken against the
/// directory resolved so far, so a directory a target enters and leaves again is passed
/// through like any other. It stops where nothing exists, at a link that cannot be read,
/// or after `LINK_LIMIT` links, and still reports what was passed through up to there:
/// the existing part of a missing secret file's path is checked too (specification
/// section 5.6). A relative path is not walked.
pub fn traverse(path: &Path) -> Traversal {
    let mut traversal = Traversal::default();
    if !path.is_absolute() {
        return traversal;
    }
    let mut resolved = PathBuf::from("/");
    let mut remaining = names_of(path);
    let mut followed = 0;
    while let Some(name) = remaining.pop_front() {
        // `name` (`..` included) is looked up in `resolved`, so `resolved` is consulted.
        if !traversal.directories.contains(&resolved) {
            traversal.directories.push(resolved.clone());
        }
        if name == ".." {
            resolved.pop();
            continue;
        }
        let candidate = resolved.join(&name);
        let Ok(metadata) = fs::symlink_metadata(&candidate) else {
            break;
        };
        if !metadata.file_type().is_symlink() {
            resolved = candidate;
            continue;
        }
        followed += 1;
        if followed > LINK_LIMIT {
            break;
        }
        let Ok(target) = fs::read_link(&candidate) else {
            break;
        };
        traversal.links.push(candidate);
        if target.is_absolute() {
            resolved = PathBuf::from("/");
        }
        for name in names_of(&target).into_iter().rev() {
            remaining.push_front(name);
        }
    }
    traversal
}

/// The names of `path` in order, `..` included and `.` and the root left out.
fn names_of(path: &Path) -> VecDeque<OsString> {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name.to_os_string()),
            Component::ParentDir => Some(OsString::from("..")),
            Component::RootDir | Component::CurDir | Component::Prefix(_) => None,
        })
        .collect()
}
