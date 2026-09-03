use std::path::{Path, PathBuf};

mod common;

use common::fixture::{
    home, layers, merged, variables, variables_without_git, Facts, POLICY_FILE, PROFILE,
};
use common::TempDir;
use process_wrap::diagnostic::{Diagnostic, Kind};
use process_wrap::environment::{HostEnvironment, RealEntry};
use process_wrap::layers::{Directive, Layer, LayerOrigin};
use process_wrap::mounts::{
    expand_policy, resolve_mounts, Expansion, ItemOrigin, MountFacts, ResolvedMounts, ScanHit,
};
use process_wrap::scan::scan;
use process_wrap::variables::Variables;

/// Resolves the mount items of the written layers against `facts`.
fn resolve_with(
    layers: &[Layer],
    variables: &Variables,
    facts: MountFacts,
) -> Result<ResolvedMounts, Diagnostic> {
    let policy = merged(layers);
    let expanded = expand_policy(&policy, variables, &home());
    resolve_mounts(&expanded, layers, variables, &facts)
}

/// Resolves with path facts only: no scan hits and no mount list.
fn resolve(
    layers: &[Layer],
    variables: &Variables,
    facts: Facts,
) -> Result<ResolvedMounts, Diagnostic> {
    resolve_with(
        layers,
        variables,
        MountFacts {
            paths: facts.0,
            scan_hits: Vec::new(),
            mounts: Vec::new(),
        },
    )
}

fn hit(found_at: &str, target: RealEntry) -> ScanHit {
    ScanHit {
        found_at: PathBuf::from(found_at),
        target,
    }
}

fn file_at(path: &str) -> RealEntry {
    RealEntry::NotDirectory(PathBuf::from(path))
}

fn dir_at(path: &str) -> RealEntry {
    RealEntry::Directory(PathBuf::from(path))
}

#[test]
fn tilde_expands_to_the_real_home_directory() {
    let home_behind_a_link = HostEnvironment {
        home: Some(PathBuf::from("/home/link")),
        xdg_config_home: None,
    }
    .home_directory(&RealEntry::Directory(PathBuf::from("/home/u")))
    .unwrap();
    let policy = merged(&layers(
        "[mounts]\nrw = [\"~\", \"~/.cache\"]",
        None,
        &[],
        &[],
    ));

    let expanded = expand_policy(&policy, &variables(), &home_behind_a_link);

    let paths: Vec<&Expansion> = expanded.mounts.iter().map(|item| &item.path).collect();
    assert_eq!(
        paths,
        [
            &Expansion::Path(PathBuf::from("/home/u")),
            &Expansion::Path(PathBuf::from("/home/u/.cache")),
        ]
    );
}

#[test]
fn variables_expand_only_in_path_values() {
    let policy = merged(&layers(
        "[mounts]\nro = [\"${config_dir}/agents.md\"]\n\
         [[mounts.scan]]\nroot = \"${worktree}\"\nnames = [\".env\"]\n\
         [[mounts.hide-mounts]]\nunder = \"${workspace}/mnt\"\nfstype = [\"9p\"]\n\
         [env]\npath-prepend = [\"${git_common_dir}/bin\"]\nset = { X = \"${worktree}\" }\n\
         [secrets]\nT = \"${config_dir}/secrets/t\"",
        None,
        &[],
        &[],
    ));

    let expanded = expand_policy(&policy, &variables(), &home());

    let path = |text: &str| Expansion::Path(PathBuf::from(text));
    assert_eq!(
        expanded.mounts[0].path,
        path("/home/u/.config/process-wrap/agents.md")
    );
    assert_eq!(expanded.scans[0].root, path("/home/u/proj"));
    assert_eq!(expanded.hide_mounts[0].under, path("/home/u/proj/mnt"));
    assert_eq!(expanded.path_prepend, [path("/home/u/proj/.git/bin")]);
    assert_eq!(
        expanded.secrets["T"],
        path("/home/u/.config/process-wrap/secrets/t")
    );
    assert_eq!(policy.env_set["X"], "${worktree}");
}

#[test]
fn an_item_with_a_valueless_variable_is_skipped() {
    let resolved = resolve(
        &layers("[mounts]\nrw = [\"${git_common_dir}\"]", None, &[], &[]),
        &variables_without_git(),
        Facts::new(),
    )
    .unwrap();

    assert!(resolved.items.is_empty(), "{resolved:?}");
    assert_eq!(resolved.skipped.len(), 1, "{resolved:?}");
    assert_eq!(resolved.skipped[0].directive, Directive::Rw);
    assert!(!resolved.skipped[0].reason.is_empty());
}

#[test]
fn a_missing_real_path_is_skipped_with_a_reason() {
    let resolved = resolve(
        &layers("[mounts]\nrw = [\"/home/u/gone\"]", None, &[], &[]),
        &variables(),
        Facts::new(),
    )
    .unwrap();

    assert!(resolved.items.is_empty(), "{resolved:?}");
    assert_eq!(resolved.skipped.len(), 1, "{resolved:?}");
    assert_eq!(resolved.skipped[0].written, "/home/u/gone");
    assert!(!resolved.skipped[0].reason.is_empty());
}

#[test]
fn the_same_directive_twice_in_one_layer_collapses() {
    let resolved = resolve(
        &layers(
            "[mounts]\nrw = [\"/home/u/proj\", \"~/proj\"]",
            None,
            &[],
            &[],
        ),
        &variables(),
        Facts::new().dir("/home/u/proj"),
    )
    .unwrap();

    assert_eq!(resolved.items.len(), 1, "{resolved:?}");
    assert_eq!(resolved.items[0].directive, Directive::Rw);
    assert_eq!(resolved.items[0].real, PathBuf::from("/home/u/proj"));
    assert!(resolved.skipped.is_empty(), "{resolved:?}");
}

#[test]
fn conflicting_directives_in_one_layer_are_a_policy_diagnostic() {
    let diagnostic = resolve(
        &layers(
            "[mounts]\nrw = [\"/home/u/a\"]\nhide = [\"/home/u/link\"]",
            None,
            &[],
            &[],
        ),
        &variables(),
        Facts::new()
            .dir("/home/u/a")
            .link_to_dir("/home/u/link", "/home/u/a"),
    )
    .unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Policy, "{diagnostic}");
}

#[test]
fn conflicting_directives_on_the_command_line_are_a_usage_diagnostic() {
    let diagnostic = resolve(
        &layers("", None, &["/x"], &["/x"]),
        &variables(),
        Facts::new(),
    )
    .unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Usage, "{diagnostic}");
}

#[test]
fn an_upper_layer_directive_replaces_the_same_real_path() {
    let resolved = resolve(
        &layers(
            "[mounts]\nhide = [\"/home/u/a\"]",
            Some("[mounts]\nrw = [\"/home/u/a\"]"),
            &[],
            &[],
        ),
        &variables(),
        Facts::new().dir("/home/u/a"),
    )
    .unwrap();

    assert_eq!(resolved.items.len(), 1, "{resolved:?}");
    assert_eq!(resolved.items[0].directive, Directive::Rw);
    assert_eq!(
        resolved.items[0].origin,
        ItemOrigin::Written(LayerOrigin::PolicyFile(PathBuf::from(POLICY_FILE)))
    );
}

#[test]
fn rw_on_a_regular_file_is_a_path_diagnostic_naming_rw_file() {
    let diagnostic = resolve(
        &layers("[mounts]\nrw = [\"/home/u/file\"]", None, &[], &[]),
        &variables(),
        Facts::new().file("/home/u/file"),
    )
    .unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Path, "{diagnostic}");
    assert!(diagnostic.description().contains("rw-file"), "{diagnostic}");
}

#[test]
fn rw_file_on_a_directory_is_a_path_diagnostic() {
    let diagnostic = resolve(
        &layers("[mounts]\nrw-file = [\"/home/u/dir\"]", None, &[], &[]),
        &variables(),
        Facts::new().dir("/home/u/dir"),
    )
    .unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Path, "{diagnostic}");
}

/// The directives and real paths of the items, in resolved order.
fn order(resolved: &ResolvedMounts) -> Vec<(Directive, &Path)> {
    resolved
        .items
        .iter()
        .map(|item| (item.directive, item.real.as_path()))
        .collect()
}

#[test]
fn ancestors_come_before_descendants() {
    let resolved = resolve(
        &layers(
            "[mounts]\nrw = [\"/home/u/proj/a/b\", \"/home/u/proj/a\"]\nhide = [\"/home/u/proj\"]",
            None,
            &[],
            &[],
        ),
        &variables(),
        Facts::new()
            .dir("/home/u/proj")
            .dir("/home/u/proj/a")
            .dir("/home/u/proj/a/b"),
    )
    .unwrap();

    assert_eq!(
        order(&resolved),
        [
            (Directive::Hide, Path::new("/home/u/proj")),
            (Directive::Rw, Path::new("/home/u/proj/a")),
            (Directive::Rw, Path::new("/home/u/proj/a/b")),
        ]
    );
}

#[test]
fn siblings_are_ordered_by_bytes() {
    let resolved = resolve(
        &layers(
            "[mounts]\nro = [\"/home/u/proj/a/b\", \"/home/u/proj/a-x\", \"/home/u/proj/B\"]",
            None,
            &[],
            &[],
        ),
        &variables(),
        Facts::new()
            .dir("/home/u/proj/a/b")
            .dir("/home/u/proj/a-x")
            .dir("/home/u/proj/B"),
    )
    .unwrap();

    assert_eq!(
        order(&resolved),
        [
            (Directive::Ro, Path::new("/home/u/proj/B")),
            (Directive::Ro, Path::new("/home/u/proj/a-x")),
            (Directive::Ro, Path::new("/home/u/proj/a/b")),
        ]
    );
}

const SCAN_ENV: &str = "[[mounts.scan]]\nroot = \"${worktree}\"\nnames = [\".env*\"]";

#[test]
fn scan_hides_matching_non_directory_entries() {
    let resolved = resolve_with(
        &layers(SCAN_ENV, None, &[], &[]),
        &variables(),
        MountFacts {
            paths: Facts::new().dir("/home/u/proj").0,
            scan_hits: vec![
                hit("/home/u/proj/.env", file_at("/home/u/proj/.env")),
                hit("/home/u/proj/.env.d", dir_at("/home/u/proj/.env.d")),
            ],
            mounts: Vec::new(),
        },
    )
    .unwrap();

    assert_eq!(
        order(&resolved),
        [(Directive::Hide, Path::new("/home/u/proj/.env"))]
    );
    assert_eq!(resolved.items[0].origin, ItemOrigin::Scan);
}

#[test]
fn scan_skips_loaded_policy_files() {
    let resolved = resolve_with(
        &layers(
            "[[mounts.scan]]\nroot = \"/home/u\"\nnames = [\"*.toml\"]",
            Some(""),
            &[],
            &[],
        ),
        &variables(),
        MountFacts {
            paths: Facts::new()
                .dir("/home/u")
                .link_to_file(PROFILE, "/home/u/dotfiles/default.toml")
                .file(POLICY_FILE)
                .0,
            scan_hits: vec![
                hit(
                    "/home/u/dotfiles/default.toml",
                    file_at("/home/u/dotfiles/default.toml"),
                ),
                hit(POLICY_FILE, file_at(POLICY_FILE)),
                hit("/home/u/other.toml", file_at("/home/u/other.toml")),
            ],
            mounts: Vec::new(),
        },
    )
    .unwrap();

    assert_eq!(
        order(&resolved),
        [(Directive::Hide, Path::new("/home/u/other.toml"))]
    );
}

#[test]
fn scan_hides_the_target_of_a_matching_symlink() {
    let resolved = resolve_with(
        &layers(SCAN_ENV, None, &[], &[]),
        &variables(),
        MountFacts {
            paths: Facts::new().dir("/home/u/proj").0,
            scan_hits: vec![hit("/home/u/proj/.env", file_at("/home/u/secrets/env"))],
            mounts: Vec::new(),
        },
    )
    .unwrap();

    assert_eq!(
        order(&resolved),
        [(Directive::Hide, Path::new("/home/u/secrets/env"))]
    );
}

#[test]
fn scan_skips_a_matching_symlink_to_a_directory() {
    let resolved = resolve_with(
        &layers(SCAN_ENV, None, &[], &[]),
        &variables(),
        MountFacts {
            paths: Facts::new().dir("/home/u/proj").0,
            scan_hits: vec![
                hit("/home/u/proj/.env", dir_at("/home/u/envs")),
                hit("/home/u/proj/.env.broken", RealEntry::Missing),
            ],
            mounts: Vec::new(),
        },
    )
    .unwrap();

    assert!(resolved.items.is_empty(), "{resolved:?}");
}

/// The scan hits as (path found, what is behind it), sorted for comparison.
fn hits(mut found: Vec<ScanHit>) -> Vec<(PathBuf, RealEntry)> {
    found.sort_by(|a, b| a.found_at.cmp(&b.found_at));
    found
        .into_iter()
        .map(|hit| (hit.found_at, hit.target))
        .collect()
}

#[test]
fn scan_walks_a_real_tree_with_prune_and_exclude() {
    let tree = TempDir::new();
    let root = tree.path().canonicalize().unwrap();
    tree.write(".env", "");
    tree.write(".env.example", "");
    tree.write("node_modules/.env", "");
    tree.write("sub/deeper/.env.local", "");
    std::fs::create_dir(root.join(".env.d")).unwrap();

    let found = scan(
        &root,
        &[".env*".to_string()],
        &["*.example".to_string()],
        &["node_modules".to_string()],
    );

    assert_eq!(
        hits(found),
        [
            (
                root.join(".env"),
                file_at(root.join(".env").to_str().unwrap())
            ),
            (
                root.join(".env.d"),
                dir_at(root.join(".env.d").to_str().unwrap())
            ),
            (
                root.join("sub/deeper/.env.local"),
                file_at(root.join("sub/deeper/.env.local").to_str().unwrap()),
            ),
        ]
    );
}

#[test]
fn scan_does_not_enter_symlinked_directories() {
    let tree = TempDir::new();
    let root = tree.path().canonicalize().unwrap();
    tree.write("outside/.env", "");
    std::fs::create_dir(root.join("inside")).unwrap();
    std::os::unix::fs::symlink(root.join("outside"), root.join("inside/linked")).unwrap();

    let found = scan(&root.join("inside"), &[".env".to_string()], &[], &[]);

    assert!(found.is_empty(), "{found:?}");
}
