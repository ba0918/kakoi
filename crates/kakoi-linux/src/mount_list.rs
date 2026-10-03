use std::path::Path;

use crate::mounts::Mount;
pub use kakoi_plan::mount_list::{parse_mount_list, MOUNTINFO};

/// Reads the mounts listed in the file at `path`; none when the file cannot be read.
pub fn read_mount_list(path: &Path) -> Option<Vec<Mount>> {
    std::fs::read(path)
        .ok()
        .map(|bytes| parse_mount_list(&bytes))
}
