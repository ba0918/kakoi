//! Collects `mounts::MountFacts` from the file system: what is behind each candidate path,
//! the scan hits, and the mount list (specification section 14). Runs no command.

use std::path::Path;

use crate::mount_list::{read_mount_list, MOUNTINFO};
use crate::mounts::{Candidates, MountFacts};
use crate::scan::scan;
use crate::workspace_facts::real_entry;

/// Looks up every candidate path, walks every scan from its real root, and, when a
/// `hide-mounts` asks for it, reads the mount list and looks up each mount target under
/// its `under`.
pub fn collect_mount_facts(candidates: &Candidates) -> MountFacts {
    let mut facts = MountFacts::default();
    for path in &candidates.paths {
        facts.paths.insert(path.clone(), real_entry(path));
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
