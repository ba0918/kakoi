use std::path::Path;

mod common;

use std::path::PathBuf;

use common::fixture::{
    command_line_layer, home, layers, merged, variables, xdg_profile_layer, xdg_variables, Facts,
    CONFIG_DIR, POLICY_FILE, WORKTREE, XDG_CONFIG_DIR, XDG_PROFILE,
};
use process_wrap::diagnostic::{Diagnostic, Kind, Warning};
use process_wrap::layers::{Layer, LayerOrigin};
use process_wrap::mounts::{expand_policy, resolve_mounts, MountFacts};
use process_wrap::placement::{check_placement, protected_paths};
use process_wrap::policy::parse_policy;
use process_wrap::variables::Variables;

/// Resolves the mounts of the written layers against `facts` and checks their placement
/// with the current directory at `current_dir` and the configuration directory at
/// `config_dir`.
fn check_at(
    layers: &[Layer],
    variables: &Variables,
    facts: Facts,
    current_dir: &str,
    config_dir: &str,
) -> Result<Vec<Warning>, Diagnostic> {
    let policy = merged(layers);
    let expanded = expand_policy(&policy, variables, &home());
    let facts = MountFacts {
        paths: facts.0,
        scan_hits: Vec::new(),
        mounts: Vec::new(),
    };
    let resolved = resolve_mounts(&expanded, layers, variables, &facts)?;
    let protected = protected_paths(&expanded, layers, Path::new(config_dir));
    check_placement(
        &resolved,
        &protected,
        variables,
        &home(),
        Path::new(current_dir),
        &facts,
    )
}

/// `check_at` with the configuration directory under the home.
fn check(
    layers: &[Layer],
    variables: &Variables,
    facts: Facts,
    current_dir: &str,
) -> Result<Vec<Warning>, Diagnostic> {
    check_at(layers, variables, facts, current_dir, CONFIG_DIR)
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

#[test]
fn rw_on_home_is_rejected_from_any_layer() {
    // With the configuration directory outside the home, no protected path has the home
    // as a prefix, so only the rule about the width of `rw` can stop these.
    let facts = Facts::new()
        .dir_with_ancestors(WORKTREE)
        .file_with_ancestors(XDG_PROFILE)
        .file_with_ancestors("/etc/xdg/p.toml");
    let policy_file = |text: &str| Layer {
        origin: LayerOrigin::PolicyFile(PathBuf::from("/etc/xdg/p.toml")),
        policy: parse_policy(text, Path::new("/etc/xdg/p.toml")).unwrap(),
    };
    for (name, layers) in [
        (
            "rw ~ in the profile",
            vec![
                xdg_profile_layer("[mounts]\nrw = [\"~\"]"),
                command_line_layer(&[], &[]),
            ],
        ),
        (
            "rw /home in the policy file",
            vec![
                xdg_profile_layer(""),
                policy_file("[mounts]\nrw = [\"/home\"]"),
                command_line_layer(&[], &[]),
            ],
        ),
    ] {
        let diagnostic = check_at(
            &layers,
            &xdg_variables(),
            facts.clone(),
            WORKTREE,
            XDG_CONFIG_DIR,
        )
        .unwrap_err();

        assert_eq!(diagnostic.kind(), Kind::Path, "{name}: {diagnostic}");
        assert!(
            !diagnostic.description().contains("p.toml")
                && !diagnostic.description().contains("default.toml"),
            "{name} was stopped by a prefix check, not the width rule: {diagnostic}"
        );
    }
    // `/` is a prefix of every protected path, so the prefix check stops it first (the
    // order of specification section 13); the kind is the same.
    let diagnostic = check_at(
        &[xdg_profile_layer(""), command_line_layer(&["/"], &[])],
        &xdg_variables(),
        facts,
        WORKTREE,
        XDG_CONFIG_DIR,
    )
    .unwrap_err();
    assert_eq!(diagnostic.kind(), Kind::Path, "--rw /: {diagnostic}");
}

#[test]
fn a_worktree_at_home_is_a_path_diagnostic() {
    let at_home = Variables {
        workspace: PathBuf::from("/home/u/proj"),
        worktree: PathBuf::from("/home/u"),
        git_common_dir: Some(PathBuf::from("/home/u/.git")),
        ..variables()
    };

    let diagnostic = check(&layers("", None, &[], &[]), &at_home, host(), WORKTREE).unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Path, "{diagnostic}");
}

#[test]
fn a_worktree_or_workspace_at_an_ancestor_of_home_is_a_path_diagnostic() {
    for (name, variables) in [
        (
            "worktree at /home",
            Variables {
                worktree: PathBuf::from("/home"),
                git_common_dir: Some(PathBuf::from("/home/.git")),
                ..variables()
            },
        ),
        (
            "workspace at /",
            Variables {
                workspace: PathBuf::from("/"),
                worktree: PathBuf::from("/"),
                git_common_dir: None,
                ..variables()
            },
        ),
        (
            "workspace at home",
            Variables {
                workspace: PathBuf::from("/home/u"),
                worktree: PathBuf::from("/home/u"),
                git_common_dir: None,
                ..variables()
            },
        ),
    ] {
        let diagnostic =
            check(&layers("", None, &[], &[]), &variables, host(), WORKTREE).unwrap_err();

        assert_eq!(diagnostic.kind(), Kind::Path, "{name}: {diagnostic}");
    }
}

#[test]
fn a_cwd_under_a_hide_is_rejected() {
    let diagnostic = check(
        &layers(
            "[mounts]\nrw = [\"${worktree}\"]\nhide = [\"/tmp\"]",
            None,
            &[],
            &[],
        ),
        &variables(),
        host().dir_with_ancestors("/tmp/work"),
        "/tmp/work",
    )
    .unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::Path, "{diagnostic}");
}
