use std::path::PathBuf;

mod common;

use common::fixture::{home, layers, merged, variables, variables_without_git, Facts};
use process_wrap::diagnostic::{Diagnostic, Kind};
use process_wrap::environment::{HostEnvironment, RealEntry};
use process_wrap::layers::{Directive, Layer};
use process_wrap::mounts::{expand_policy, resolve_mounts, Expansion, MountFacts, ResolvedMounts};
use process_wrap::variables::Variables;

/// Resolves the mount items of the written layers against `facts`, with no scan hits and
/// no mount list.
fn resolve(
    layers: &[Layer],
    variables: &Variables,
    facts: Facts,
) -> Result<ResolvedMounts, Diagnostic> {
    let policy = merged(layers);
    let expanded = expand_policy(&policy, variables, &home());
    let facts = MountFacts {
        paths: facts.0,
        scan_hits: Vec::new(),
        mounts: Vec::new(),
    };
    resolve_mounts(&expanded, layers, variables, &facts)
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
