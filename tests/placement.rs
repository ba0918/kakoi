use std::path::Path;

mod common;

use common::fixture::{home, layers, merged, variables, Facts, CONFIG_DIR, POLICY_FILE, WORKTREE};
use process_wrap::diagnostic::{Diagnostic, Kind, Warning};
use process_wrap::layers::Layer;
use process_wrap::mounts::{expand_policy, resolve_mounts, MountFacts};
use process_wrap::placement::{check_placement, protected_paths};
use process_wrap::variables::Variables;

/// Resolves the mounts of the written layers against `facts` and checks their placement
/// with the current directory at `current_dir`.
fn check(
    layers: &[Layer],
    variables: &Variables,
    facts: Facts,
    current_dir: &str,
) -> Result<Vec<Warning>, Diagnostic> {
    let policy = merged(layers);
    let expanded = expand_policy(&policy, variables, &home());
    let facts = MountFacts {
        paths: facts.0,
        scan_hits: Vec::new(),
        mounts: Vec::new(),
    };
    let resolved = resolve_mounts(&expanded, layers, variables, &facts)?;
    let protected = protected_paths(&expanded, layers, Path::new(CONFIG_DIR));
    check_placement(
        &resolved,
        &protected,
        variables,
        &home(),
        Path::new(current_dir),
        &facts,
    )
}

/// Facts for the fixture host: the home, the worktree, the profile, and the configuration
/// directory all exist.
fn host() -> Facts {
    Facts::new()
        .dir_with_ancestors(WORKTREE)
        .dir_with_ancestors(CONFIG_DIR)
        .file_with_ancestors(common::fixture::PROFILE)
}

fn assert_path_diagnostic(diagnostic: &Diagnostic, mentions: &[&str]) {
    assert_eq!(diagnostic.kind(), Kind::Path, "{diagnostic}");
    for text in mentions {
        assert!(
            diagnostic.description().contains(text),
            "{diagnostic} does not mention {text}"
        );
    }
}

#[test]
fn a_policy_file_inside_a_writable_area_is_rejected_with_the_three_reasons() {
    let diagnostic = check(
        &layers("", Some("[mounts]\nrw = [\"/home/u/policies\"]"), &[], &[]),
        &variables(),
        host().file_with_ancestors(POLICY_FILE),
        WORKTREE,
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &[POLICY_FILE, "/home/u/policies"]);
}

#[test]
fn a_policy_file_reached_through_a_symlink_in_a_writable_area_is_rejected() {
    let diagnostic = check(
        &layers("", Some("[mounts]\nrw = [\"${worktree}\"]"), &[], &[]),
        &variables(),
        host()
            .link_to_dir("/home/u/policies", "/home/u/proj/policies")
            .link_to_file(POLICY_FILE, "/home/u/proj/policies/p.toml"),
        WORKTREE,
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &[POLICY_FILE, WORKTREE]);
}

#[test]
fn the_config_dir_inside_a_writable_area_is_rejected() {
    let diagnostic = check(
        &layers("[mounts]\nrw = [\"~/.config\"]", None, &[], &[]),
        &variables(),
        host(),
        WORKTREE,
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &[CONFIG_DIR, "/home/u/.config"]);
}

#[test]
fn a_secret_file_inside_a_writable_area_is_rejected() {
    let diagnostic = check(
        &layers(
            "[mounts]\nrw = [\"${worktree}\"]\n[secrets]\nT = \"${worktree}/token\"",
            None,
            &[],
            &[],
        ),
        &variables(),
        host().file_with_ancestors("/home/u/proj/token"),
        WORKTREE,
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &["/home/u/proj/token", WORKTREE]);
}

#[test]
fn a_missing_secret_file_under_a_writable_area_is_rejected() {
    let diagnostic = check(
        &layers(
            "[mounts]\nrw = [\"${worktree}\"]\n[secrets]\nT = \"${worktree}/missing/token\"",
            None,
            &[],
            &[],
        ),
        &variables(),
        host(),
        WORKTREE,
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &[WORKTREE]);
}

#[test]
fn the_first_placement_violation_follows_the_specified_order() {
    let diagnostic = check(
        &layers(
            "[mounts]\nrw = [\"/home/u/policies\", \"${worktree}\"]\n\
             [secrets]\nT = \"${worktree}/token\"",
            Some(""),
            &[],
            &[],
        ),
        &variables(),
        host()
            .file_with_ancestors(POLICY_FILE)
            .file_with_ancestors("/home/u/proj/token"),
        WORKTREE,
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &[POLICY_FILE]);
    assert!(!diagnostic.description().contains("token"), "{diagnostic}");
}

#[test]
fn path_prepend_inside_a_writable_area_is_rejected() {
    let diagnostic = check(
        &layers(
            "[mounts]\nrw = [\"${worktree}\"]\n[env]\npath-prepend = [\"${worktree}/bin\"]",
            None,
            &[],
            &[],
        ),
        &variables(),
        host().dir_with_ancestors("/home/u/proj/bin"),
        WORKTREE,
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &["/home/u/proj/bin", WORKTREE]);
}
