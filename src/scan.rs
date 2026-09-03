//! The walk of `mounts.scan` (specification section 6.3): the outer layer that reads
//! directories and reports what it found by name. What becomes a `hide` is decided in
//! `mounts`.

use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use crate::mounts::ScanHit;
use crate::wildcard::matches;
use crate::workspace_facts::real_entry;

/// Walks `root` and reports every entry whose name matches one of `names` and none of
/// `exclude`, with what is behind it. Does not enter a directory whose name matches
/// `prune`, nor a symbolic link to a directory. A directory that cannot be read is left
/// out. `root` that is not a directory yields nothing.
pub fn scan(root: &Path, names: &[String], exclude: &[String], prune: &[String]) -> Vec<ScanHit> {
    let mut found = Vec::new();
    walk(root, names, exclude, prune, &mut found);
    found
}

fn walk(
    directory: &Path,
    names: &[String],
    exclude: &[String],
    prune: &[String],
    found: &mut Vec<ScanHit>,
) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name();
        let name = name.as_bytes();
        let path = entry.path();
        if names.iter().any(|pattern| matches(pattern, name))
            && !exclude.iter().any(|pattern| matches(pattern, name))
        {
            found.push(ScanHit {
                target: real_entry(&path),
                found_at: path.clone(),
            });
        }
        let is_directory = entry.file_type().is_ok_and(|kind| kind.is_dir());
        if is_directory && !prune.iter().any(|pattern| matches(pattern, name)) {
            walk(&path, names, exclude, prune, found);
        }
    }
}
