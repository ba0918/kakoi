//! The root of the mount mode "listed" (specification core-listed-mounts.md): an empty
//! root with the base, the places the policy shows, and the directories and links that
//! lead to them, and nothing else of the host. Pure: what is on the host arrives as
//! `MountFacts`.

use std::path::{Path, PathBuf};

use crate::environment::RealEntry;
use crate::layers::Directive;
use crate::mounts::{
    byte_order, ExpandedPolicy, MountFacts, ResolvedItem, SkippedPath, SkippedRole,
};
use crate::policy::NetworkMode;

/// The base: the host's directories shown read-only as a whole while `mounts.system` is
/// true.
pub const BASE: [&str; 6] = ["/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc"];

/// The host's resolver configuration, whose target is shown when it points outside the
/// base.
pub const RESOLVER: &str = "/etc/resolv.conf";

/// What the isolation has whatever the policy writes: bwrap's `/dev` and `/proc`, and the
/// empty `/tmp` of its own.
const PROVIDED: [&str; 3] = ["/dev", "/proc", "/tmp"];

/// The root of a "listed" isolation besides the mount items: the directories made on the
/// way to what is shown (parents first), the base directories by their real paths, the
/// links on the written paths made again, and the resolver configuration's target.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ListedRoot {
    pub directories: Vec<PathBuf>,
    pub base: Vec<PathBuf>,
    pub links: Vec<RebuiltLink>,
    pub resolver: Option<PathBuf>,
}

/// A symbolic link made again inside: at `place`, holding `target` as the host's does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebuiltLink {
    pub place: PathBuf,
    pub target: PathBuf,
}

/// The paths whose facts the "listed" root needs looked up and walked: the base while
/// `system` is on, and the resolver configuration when its target may be shown.
pub fn lookups(system: bool, network_mode: NetworkMode) -> Vec<PathBuf> {
    if !system {
        return Vec::new();
    }
    let mut paths: Vec<PathBuf> = BASE.iter().map(PathBuf::from).collect();
    if network_mode != NetworkMode::Filtered {
        paths.push(PathBuf::from(RESOLVER));
    }
    paths
}

/// The root of a "listed" isolation for the resolved `items`, and the base directories
/// and the resolver target skipped with their reasons.
pub fn listed_root(
    system: bool,
    network_mode: NetworkMode,
    items: &[ResolvedItem],
    expanded: &ExpandedPolicy,
    facts: &MountFacts,
) -> (ListedRoot, Vec<SkippedPath>) {
    let mut skipped = Vec::new();
    let mut base = Vec::new();
    // The paths as written (or as kakoi names them) whose links are made again.
    let mut walked: Vec<&Path> = Vec::new();
    if system {
        for directory in BASE {
            match facts.entry(Path::new(directory)) {
                RealEntry::Directory(real) => {
                    base.push(real);
                    walked.push(Path::new(directory));
                }
                entry => skipped.push(SkippedPath {
                    role: SkippedRole::Base,
                    written: directory.to_string(),
                    reason: match entry {
                        RealEntry::Missing => "does not exist",
                        _ => "is not a directory",
                    }
                    .to_string(),
                }),
            }
        }
    }
    let shown: Vec<&Path> = items
        .iter()
        .filter(|item| item.directive != Directive::Hide)
        .map(|item| item.real.as_path())
        .collect();
    walked.extend(
        expanded
            .mounts
            .iter()
            .filter(|item| item.directive != Directive::Hide)
            .filter_map(|item| item.path.path())
            .filter(|path| {
                facts
                    .entry(path)
                    .path()
                    .is_some_and(|real| shown.contains(&real))
            }),
    );
    let mut resolver = None;
    if system
        && network_mode != NetworkMode::Filtered
        && !facts.traversed_links(Path::new(RESOLVER)).is_empty()
    {
        match facts.entry(Path::new(RESOLVER)) {
            RealEntry::Missing => skipped.push(SkippedPath {
                role: SkippedRole::ResolverTarget,
                written: RESOLVER.to_string(),
                reason: "points at nothing".to_string(),
            }),
            entry => {
                let real = entry.path().expect("an entry that exists").to_path_buf();
                if !covered(&real, base.iter().map(PathBuf::as_path)) {
                    walked.push(Path::new(RESOLVER));
                    resolver = Some(real);
                }
            }
        }
    }
    let base: Vec<PathBuf> = base
        .iter()
        .filter(|directory| {
            !covered(
                directory,
                base.iter()
                    .filter(|other| other != directory)
                    .map(PathBuf::as_path),
            )
        })
        .cloned()
        .collect();
    let places: Vec<&Path> = base
        .iter()
        .map(PathBuf::as_path)
        .chain(shown.iter().copied())
        .chain(PROVIDED.iter().map(Path::new))
        .collect();
    let resolver = resolver.filter(|real| !covered(real, places.iter().copied()));
    let places: Vec<&Path> = places.into_iter().chain(resolver.as_deref()).collect();
    let mut links: Vec<RebuiltLink> = Vec::new();
    for path in walked {
        for place in facts.traversed_links(path) {
            if covered(place, places.iter().copied())
                || links.iter().any(|link| &link.place == place)
            {
                continue;
            }
            if let Some(target) = facts.link_targets.get(place) {
                links.push(RebuiltLink {
                    place: place.clone(),
                    target: target.clone(),
                });
            }
        }
    }
    let mut directories: Vec<PathBuf> = places
        .iter()
        .copied()
        .chain(links.iter().map(|link| link.place.as_path()))
        .flat_map(|place| place.ancestors().skip(1))
        .filter(|ancestor| {
            *ancestor != Path::new("/") && !covered(ancestor, places.iter().copied())
        })
        .map(Path::to_path_buf)
        .collect();
    directories.sort_by(|a, b| byte_order(a, b));
    directories.dedup();
    (
        ListedRoot {
            directories,
            base,
            links,
            resolver,
        },
        skipped,
    )
}

/// Whether `path` is one of `places` or inside one.
fn covered<'a>(path: &Path, mut places: impl Iterator<Item = &'a Path>) -> bool {
    places.any(|place| path.starts_with(place))
}
