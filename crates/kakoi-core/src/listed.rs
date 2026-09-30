//! The root of the mount mode "listed" (specification core-listed-mounts.md): an empty
//! root with the base, the places the policy shows, and the directories and links that
//! lead to them, and nothing else of the host. Pure: what is on the host arrives as
//! `MountFacts`.

use std::path::{Path, PathBuf};

use crate::environment::RealEntry;
use crate::guard_placement::GUARD_ROOT;
use crate::layers::Directive;
use crate::mounts::{
    byte_order, ExpandedPolicy, Expansion, MountFacts, NotShown, ResolvedItem, ResolvedMounts,
    SkippedPath, SkippedRole,
};
use crate::plan::{NESTING_MARK, TUN_DEVICE};
use crate::policy::NetworkMode;

/// The base: the host's directories shown read-only as a whole while `mounts.system` is
/// true.
pub const BASE: [&str; 6] = ["/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc"];

/// The host's resolver configuration, whose target is shown when it points outside the
/// base.
pub const RESOLVER: &str = "/etc/resolv.conf";

/// What the isolation has whatever the policy writes: bwrap's `/dev` and `/proc`, and the
/// empty `/tmp` of its own. None of the host's content under them is there.
const PROVIDED: [&str; 3] = ["/dev", "/proc", "/tmp"];

/// What kakoi and bwrap put in the provided places: kakoi's own tmpfs of the command
/// guards (handed on to a nested run too), the nesting mark, the tunnel device, and the
/// nodes and links bwrap's `/dev` holds.
const PUT_INSIDE: [&str; 17] = [
    GUARD_ROOT,
    NESTING_MARK,
    TUN_DEVICE,
    "/dev/null",
    "/dev/zero",
    "/dev/full",
    "/dev/random",
    "/dev/urandom",
    "/dev/tty",
    "/dev/shm",
    "/dev/pts",
    "/dev/ptmx",
    "/dev/fd",
    "/dev/stdin",
    "/dev/stdout",
    "/dev/stderr",
    "/dev/core",
];

/// The root of a "listed" isolation besides the mount items: the directories made on the
/// way to what is shown (parents first), the base directories by their real paths, the
/// links on the written paths made again, the resolver configuration's target, where
/// filtered puts its own resolver configuration, and every place shown.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ListedRoot {
    pub directories: Vec<PathBuf>,
    pub base: Vec<PathBuf>,
    pub links: Vec<RebuiltLink>,
    pub resolver: Option<PathBuf>,
    /// Where filtered puts its resolver configuration: the real path the isolation's
    /// `/etc/resolv.conf` leads to, since bwrap follows a link at the destination as the
    /// host resolves it; none outside filtered.
    pub filtered_resolver: Option<PathBuf>,
    /// The places shown of the host's: the base, the items, and the resolver
    /// configuration's target.
    pub places: Vec<PathBuf>,
}

impl ListedRoot {
    /// Whether `path` is inside a place shown, is one of the places the isolation
    /// provides, or is what kakoi or bwrap puts in them.
    pub fn shows(&self, path: &Path) -> bool {
        covered(path, self.places.iter().map(PathBuf::as_path))
            || PROVIDED.iter().any(|provided| path == Path::new(provided))
            || covered(path, PUT_INSIDE.iter().map(Path::new))
    }

    /// Whether a path is there as written when resolving it passes through `links` to
    /// `real`: each link is inside a place shown or is made again, and `real` is shown.
    pub fn shows_through(&self, links: &[PathBuf], real: &Path) -> bool {
        self.shows(real)
            && links.iter().all(|link| {
                self.shows(link) || self.links.iter().any(|rebuilt| &rebuilt.place == link)
            })
    }

    /// Whether anything of the host's at or under `path` is shown.
    pub fn reaches(&self, path: &Path) -> bool {
        covered(path, self.places.iter().map(PathBuf::as_path))
            || self.places.iter().any(|place| place.starts_with(path))
    }
}

/// A symbolic link made again inside: at `place`, holding `target` as the host's does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebuiltLink {
    pub place: PathBuf,
    pub target: PathBuf,
}

/// The paths whose facts the "listed" root needs looked up and walked while `system` is
/// on: the base and the resolver configuration.
pub fn lookups(system: bool) -> Vec<PathBuf> {
    if !system {
        return Vec::new();
    }
    BASE.iter()
        .chain(std::iter::once(&RESOLVER))
        .map(PathBuf::from)
        .collect()
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
    let filtered_resolver =
        (network_mode == NetworkMode::Filtered).then(|| filtered_resolver(system, facts));
    let places: Vec<&Path> = places
        .into_iter()
        .chain(resolver.as_deref())
        .chain(filtered_resolver.as_deref())
        .collect();
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
    let places = base
        .iter()
        .map(PathBuf::as_path)
        .chain(shown)
        .chain(resolver.as_deref())
        .chain(filtered_resolver.as_deref())
        .map(Path::to_path_buf)
        .collect();
    (
        ListedRoot {
            directories,
            base,
            links,
            resolver,
            filtered_resolver,
            places,
        },
        skipped,
    )
}

/// Where filtered puts its resolver configuration: `/etc/resolv.conf` itself without the
/// base, else the real path it leads to, or the target of a link to nothing.
fn filtered_resolver(system: bool, facts: &MountFacts) -> PathBuf {
    let resolver = Path::new(RESOLVER);
    if !system {
        return resolver.to_path_buf();
    }
    if let Some(real) = facts.entry(resolver).path() {
        return real.to_path_buf();
    }
    facts
        .traversed_links(resolver)
        .last()
        .and_then(|link| facts.link_targets.get(link))
        .filter(|target| target.is_absolute())
        .cloned()
        .unwrap_or_else(|| resolver.to_path_buf())
}

/// Sets aside the `hide` items that name nothing shown, each with the reason: nothing
/// there is in the isolation to hide, and the mount would make the place appear.
pub fn set_aside_unshown(mounts: &mut ResolvedMounts, root: &ListedRoot) {
    let (kept, unshown): (Vec<_>, Vec<_>) = std::mem::take(&mut mounts.items)
        .into_iter()
        .partition(|item| item.directive != Directive::Hide || root.shows(&item.real));
    mounts.items = kept;
    mounts.not_shown = unshown
        .into_iter()
        .map(|item| NotShown {
            item,
            reason: "outside what the \"listed\" mount mode shows".to_string(),
        })
        .collect();
}

/// `expanded` without the scans and the `hide-mounts` whose root or `under` has nothing
/// shown at or under it, and those set aside, each with the reason.
pub fn shown_generators(
    expanded: &ExpandedPolicy,
    facts: &MountFacts,
    root: &ListedRoot,
) -> (ExpandedPolicy, Vec<SkippedPath>) {
    let unshown = |expansion: &Expansion| {
        expansion.path().is_some_and(|path| {
            facts
                .entry(path)
                .path()
                .is_some_and(|real| !root.reaches(real))
        })
    };
    let skip = |role, written: &crate::policy::PolicyPath| SkippedPath {
        role,
        written: written.to_string(),
        reason: "nothing under it is shown in the \"listed\" mount mode".to_string(),
    };
    let (scans, unshown_scans): (Vec<_>, Vec<_>) = expanded
        .scans
        .iter()
        .cloned()
        .partition(|scan| !unshown(&scan.root));
    let (hide_mounts, unshown_hide_mounts): (Vec<_>, Vec<_>) = expanded
        .hide_mounts
        .iter()
        .cloned()
        .partition(|hide| !unshown(&hide.under));
    let skipped = unshown_scans
        .iter()
        .map(|scan| skip(SkippedRole::ScanRoot, &scan.written))
        .chain(
            unshown_hide_mounts
                .iter()
                .map(|hide| skip(SkippedRole::HideMountsUnder, &hide.written)),
        )
        .collect();
    (
        ExpandedPolicy {
            scans,
            hide_mounts,
            ..expanded.clone()
        },
        skipped,
    )
}

/// Whether `path` is one of `places` or inside one.
fn covered<'a>(path: &Path, mut places: impl Iterator<Item = &'a Path>) -> bool {
    places.any(|place| path.starts_with(place))
}
