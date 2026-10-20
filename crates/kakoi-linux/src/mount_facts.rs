//! Collects `mounts::MountFacts` from the file system: what is behind each candidate path,
//! what the resolution of each protected path passes through, the scan hits, and the mount
//! list or the fact that it could not be read (specification section 14). Runs no command.

use std::path::Path;

use crate::mount_list::{read_mount_list, MOUNTINFO};
use crate::mounts::{Candidates, MountFacts};
use crate::real_paths::{real_entry, traverse};
use crate::scan::scan;

/// Looks up every candidate path and walks the resolution of every path whose traversal
/// is asked for: the facts the resolution needs before it knows which scans and
/// `hide-mounts` apply.
pub fn collect_path_facts(candidates: &Candidates) -> MountFacts {
    let mut facts = MountFacts::default();
    for path in &candidates.paths {
        facts.paths.insert(path.clone(), real_entry(path));
    }
    for path in &candidates.traversals {
        let traversal = traverse(path);
        facts.link_targets.extend(traversal.targets);
        facts.links.insert(path.clone(), traversal.links);
        facts
            .directories
            .insert(path.clone(), traversal.directories);
    }
    facts
}

/// Adds to `facts`, which hold the real paths of the scan roots and the `under`s, what
/// every scan of `candidates` finds from its real root and, when a `hide-mounts` asks for
/// it, the mount list and each mount target under its `under`.
pub fn collect_generator_facts(candidates: &Candidates, facts: &mut MountFacts) {
    for request in &candidates.scans {
        if let Some(root) = facts.entry(&request.root).path() {
            facts
                .scan_hits
                .extend(scan(root, &request.names, &request.exclude, &request.prune));
        }
    }
    if !candidates.hide_mounts_under.is_empty() {
        match read_mount_list(Path::new(MOUNTINFO)) {
            Some(mounts) => facts.mounts = mounts,
            None => facts.mount_list_unreadable = true,
        }
        let unders: Vec<_> = candidates
            .hide_mounts_under
            .iter()
            .filter_map(|under| facts.entry(under).path().map(Path::to_path_buf))
            .collect();
        for mount in &facts.mounts {
            if unders.iter().any(|under| mount.target.starts_with(under)) {
                facts
                    .paths
                    .insert(mount.target.clone(), real_entry(&mount.target));
            }
        }
    }
}
