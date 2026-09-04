use std::path::Path;

mod common;

use std::path::PathBuf;

use common::fixture::{
    command_line_layer, home, layers, merged, variables, xdg_profile_layer, xdg_variables, Facts,
    CONFIG_DIR, POLICY_FILE, WORKTREE, XDG_CONFIG_DIR, XDG_PROFILE,
};
use process_wrap::diagnostic::{Diagnostic, Kind, Warning};
use process_wrap::layers::{Layer, LayerOrigin};
use process_wrap::mounts::{expand_policy, resolve_mounts};
use process_wrap::placement::{check_placement, protected_paths, written_paths};
use process_wrap::policy::parse_policy;
use process_wrap::variables::Variables;

/// Resolves the mounts of the written layers against `facts` and checks their placement
/// with the current directory at `current_dir`, the configuration directory at
/// `config_dir`, and `--workspace` given as `workspace`.
fn check_at(
    layers: &[Layer],
    variables: &Variables,
    facts: Facts,
    current_dir: &str,
    config_dir: &str,
    workspace: Option<&str>,
) -> Result<Vec<Warning>, Diagnostic> {
    let policy = merged(layers);
    let expanded = expand_policy(&policy, variables, &home());
    let facts = facts.mount_facts();
    let resolved = resolve_mounts(&expanded, layers, variables, &facts)?;
    let protected = protected_paths(&expanded, layers, Path::new(config_dir));
    let written = written_paths(&expanded, workspace.map(Path::new));
    check_placement(
        &resolved,
        &protected,
        &written,
        variables,
        &home(),
        Path::new(current_dir),
        &facts,
    )
}

/// `check_at` with the configuration directory under the home and `--workspace` omitted.
fn check(
    layers: &[Layer],
    variables: &Variables,
    facts: Facts,
    current_dir: &str,
) -> Result<Vec<Warning>, Diagnostic> {
    check_at(layers, variables, facts, current_dir, CONFIG_DIR, None)
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
fn a_symlink_on_the_resolution_chain_inside_a_writable_area_is_rejected() {
    // `/home/u/policies` points at `/home/u/cache/link/pol` and `/home/u/cache/link` at
    // `/home/u/real`: every prefix of the policy file resolves outside `~/cache`, but the
    // chain passes through a link that sits inside it.
    let chain = |profile: &str, policy_file: &str| {
        check(
            &layers(profile, Some(policy_file), &[], &[]),
            &variables(),
            host()
                .dir("/home/u/cache")
                .dir_with_ancestors("/home/u/real/pol")
                .link_to_dir("/home/u/policies", "/home/u/real/pol")
                .link_to_file(POLICY_FILE, "/home/u/real/pol/p.toml")
                .links_traversed(POLICY_FILE, &["/home/u/policies", "/home/u/cache/link"]),
            WORKTREE,
        )
    };
    let rw_cache = "[mounts]\nrw = [\"~/cache\"]";

    for (name, diagnostic) in [
        ("rw in the profile", chain(rw_cache, "")),
        ("rw in the policy file itself", chain("", rw_cache)),
    ] {
        let diagnostic = diagnostic.unwrap_err();
        assert_eq!(diagnostic.kind(), Kind::Path, "{name}: {diagnostic}");
        assert_path_diagnostic(&diagnostic, &["/home/u/cache/link", "/home/u/cache"]);
    }
}

#[test]
fn a_directory_on_the_resolution_chain_inside_a_writable_area_is_rejected() {
    // `/home/u/policies` points at `/home/u/cache/x/../../real/pol` with `/home/u/cache/x`
    // a real directory: every prefix of the policy file resolves outside `~/cache` and the
    // only link sits in the home, but the walk passes through a directory inside it.
    let chain = |profile: &str, policy_file: &str| {
        check(
            &layers(profile, Some(policy_file), &[], &[]),
            &variables(),
            host()
                .dir("/home/u/cache")
                .dir_with_ancestors("/home/u/real/pol")
                .link_to_dir("/home/u/policies", "/home/u/real/pol")
                .link_to_file(POLICY_FILE, "/home/u/real/pol/p.toml")
                .links_traversed(POLICY_FILE, &["/home/u/policies"])
                .directories_visited(POLICY_FILE, &["/home/u/cache/x"]),
            WORKTREE,
        )
    };
    let rw_cache = "[mounts]\nrw = [\"~/cache\"]";

    for (name, diagnostic) in [
        ("rw in the profile", chain(rw_cache, "")),
        ("rw in the policy file itself", chain("", rw_cache)),
    ] {
        let diagnostic = diagnostic.unwrap_err();
        assert_eq!(diagnostic.kind(), Kind::Path, "{name}: {diagnostic}");
        assert_path_diagnostic(&diagnostic, &["/home/u/cache/x", "/home/u/cache"]);
    }
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

/// The facts of `~/cache/pip/http` rewired from inside the isolation: `cache/pip` became a
/// link to `cache/evil`, and `cache/evil/http` a link to `target`. The given path resolves
/// to `target` after passing through `cache` and `cache/evil` and following two links
/// that sit inside `cache`.
fn rewired_through_cache(facts: Facts, target: &str, target_is_dir: bool) -> Facts {
    let given = "/home/u/cache/pip/http";
    let facts = facts.dir("/home/u/cache").dir("/home/u/cache/evil");
    let facts = if target_is_dir {
        facts.link_to_dir(given, target)
    } else {
        facts.link_to_file(given, target)
    };
    facts
        .links_traversed(given, &["/home/u/cache/pip", "/home/u/cache/evil/http"])
        .directories_visited(
            given,
            &[
                "/",
                "/home",
                "/home/u",
                "/home/u/cache",
                "/home/u/cache/evil",
            ],
        )
}

#[test]
fn a_written_item_resolving_through_a_writable_item_to_outside_every_writable_item_is_rejected() {
    for (name, profile, policy_file, facts) in [
        (
            "rw nested under rw, redirected to a directory",
            "[mounts]\nrw = [\"~/cache\", \"~/cache/pip/http\"]",
            None,
            rewired_through_cache(host().dir("/home/u/victim"), "/home/u/victim", true),
        ),
        (
            "ro in the upper layer, redirected onto a hidden directory",
            "[mounts]\nrw = [\"~/cache\"]\nhide = [\"/home/u/secret\"]",
            Some("[mounts]\nro = [\"~/cache/pip/http\"]"),
            rewired_through_cache(host().dir("/home/u/secret"), "/home/u/secret", true),
        ),
        (
            "rw-file nested under rw, redirected to a file",
            "[mounts]\nrw = [\"~/cache\"]\nrw-file = [\"~/cache/pip/http\"]",
            None,
            rewired_through_cache(
                host().file("/home/u/victim.txt"),
                "/home/u/victim.txt",
                false,
            ),
        ),
    ] {
        let diagnostic = check(
            &layers(profile, policy_file, &[], &[]),
            &variables(),
            facts,
            WORKTREE,
        )
        .unwrap_err();

        assert_eq!(diagnostic.kind(), Kind::Path, "{name}: {diagnostic}");
        assert_path_diagnostic(&diagnostic, &["/home/u/cache/pip/http", "/home/u/cache"]);
    }
}

#[test]
fn a_workspace_resolving_through_a_writable_item_to_outside_every_writable_item_is_rejected() {
    // `--workspace ~/cache/ws` with `cache/ws` a link to `~/victim`: the workspace's real
    // path is `~/victim`, reached through a link that sits inside `~/cache`.
    let redirected = Variables {
        workspace: PathBuf::from("/home/u/victim"),
        worktree: PathBuf::from("/home/u/victim"),
        git_common_dir: None,
        ..variables()
    };

    let diagnostic = check_at(
        &layers("[mounts]\nrw = [\"~/cache\"]", None, &[], &[]),
        &redirected,
        host()
            .dir("/home/u/cache")
            .dir("/home/u/victim")
            .link_to_dir("/home/u/cache/ws", "/home/u/victim")
            .links_traversed("/home/u/cache/ws", &["/home/u/cache/ws"])
            .directories_visited(
                "/home/u/cache/ws",
                &["/", "/home", "/home/u", "/home/u/cache"],
            ),
        "/home/u/elsewhere",
        CONFIG_DIR,
        Some("/home/u/cache/ws"),
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &["/home/u/cache/ws", "/home/u/cache"]);
}

/// The facts of `--workspace /home/u/cache/proj` after `cache/proj` was replaced from
/// inside the isolation by a link to `/home/u/victim`, and the variables of that redirected
/// workspace (no `.git` anywhere: the worktree is the workspace).
fn workspace_rewired_through_cache() -> (Variables, Facts) {
    let given = "/home/u/cache/proj";
    let redirected = Variables {
        workspace: PathBuf::from("/home/u/victim"),
        worktree: PathBuf::from("/home/u/victim"),
        git_common_dir: None,
        ..variables()
    };
    let facts = host()
        .dir("/home/u/cache")
        .dir("/home/u/victim")
        .link_to_dir(given, "/home/u/victim")
        .links_traversed(given, &[given])
        .directories_visited(given, &["/", "/home", "/home/u", "/home/u/cache"])
        .directories_visited("/home/u/victim", &["/", "/home", "/home/u"]);
    (redirected, facts)
}

#[test]
fn an_item_expanded_from_a_workspace_variable_inherits_what_the_workspace_referenced() {
    // `rw ${worktree}` expands to the real path `/home/u/victim`, which on its own
    // references nothing writable; the workspace it came from was reached through
    // `~/cache`, so the item is not a root and cannot carry the workspace.
    let (redirected, facts) = workspace_rewired_through_cache();

    let diagnostic = check_at(
        &layers(
            "[mounts]\nrw = [\"${worktree}\", \"~/cache\"]",
            None,
            &[],
            &[],
        ),
        &redirected,
        facts,
        "/home/u/elsewhere",
        CONFIG_DIR,
        Some("/home/u/cache/proj"),
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &["/home/u/victim", "/home/u/cache"]);
}

#[test]
fn an_explicit_workspace_at_the_current_directory_is_exempt_and_hands_down_no_reference() {
    // `--workspace ~/proj/sub` under `rw ${worktree}` resolves through the worktree itself.
    // Started from `sub`, the process already sits there and nothing can move it; started
    // from elsewhere, the worktree item inherits that reference and no root remains.
    let under_worktree = Variables {
        workspace: PathBuf::from("/home/u/proj/sub"),
        ..variables()
    };
    let facts = || {
        host()
            .dir("/home/u/proj/sub")
            .directories_visited(
                "/home/u/proj/sub",
                &["/", "/home", "/home/u", "/home/u/proj"],
            )
            .directories_visited(WORKTREE, &["/", "/home", "/home/u"])
    };
    let rw_worktree = layers("[mounts]\nrw = [\"${worktree}\"]", None, &[], &[]);

    let from_the_workspace = check_at(
        &rw_worktree,
        &under_worktree,
        facts(),
        "/home/u/proj/sub",
        CONFIG_DIR,
        Some("/home/u/proj/sub"),
    );
    assert!(from_the_workspace.is_ok(), "{from_the_workspace:?}");

    let from_elsewhere = check_at(
        &rw_worktree,
        &under_worktree,
        facts(),
        "/home/u/elsewhere",
        CONFIG_DIR,
        Some("/home/u/proj/sub"),
    )
    .unwrap_err();
    assert_path_diagnostic(&from_elsewhere, &[WORKTREE]);
}

#[test]
fn two_nested_items_redirected_to_the_same_outside_place_do_not_vouch_for_each_other() {
    let facts = rewired_through_cache(host().dir("/home/u/victim"), "/home/u/victim", true)
        .link_to_dir("/home/u/cache/npm/x", "/home/u/victim")
        .links_traversed("/home/u/cache/npm/x", &["/home/u/cache/npm/x"])
        .directories_visited(
            "/home/u/cache/npm/x",
            &[
                "/",
                "/home",
                "/home/u",
                "/home/u/cache",
                "/home/u/cache/npm",
            ],
        );

    let diagnostic = check(
        &layers(
            "[mounts]\nrw = [\"~/cache\", \"~/cache/pip/http\", \"~/cache/npm/x\"]",
            None,
            &[],
            &[],
        ),
        &variables(),
        facts,
        WORKTREE,
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &["/home/u/cache"]);
}

#[test]
fn a_nested_item_resolving_inside_the_writable_item_it_passes_through_is_accepted() {
    let warnings = check(
        &layers(
            "[mounts]\nrw = [\"${worktree}\", \"~/cache\", \"~/cache/pip/http\"]",
            None,
            &[],
            &[],
        ),
        &variables(),
        host()
            .dir_with_ancestors("/home/u/cache/pip/http")
            .directories_visited(
                "/home/u/cache/pip/http",
                &[
                    "/",
                    "/home",
                    "/home/u",
                    "/home/u/cache",
                    "/home/u/cache/pip",
                ],
            ),
        WORKTREE,
    )
    .unwrap();

    assert!(warnings.is_empty(), "{warnings:?}");
}

const RW_WITH_A_NESTED_ITEM_AND_ANOTHER: &str =
    "[mounts]\nrw = [\"${worktree}\", \"~/cache\", \"~/cache/pip/http\", \"~/other\"]";

#[test]
fn a_writable_item_whose_own_resolution_passes_through_its_inside_is_not_a_root() {
    // `~/cache/sub/..` resolves to `~/cache` by looking `..` up in `cache/sub`, a directory
    // inside the item itself: replacing `sub` from inside the isolation moves the item.
    // The honest nested `~/cache/pip/http` then has no root to lie in.
    let diagnostic = check(
        &layers(
            "[mounts]\nrw = [\"~/cache/sub/..\", \"~/cache/pip/http\"]",
            None,
            &[],
            &[],
        ),
        &variables(),
        host()
            .dir_with_ancestors("/home/u/cache/pip/http")
            .dir("/home/u/cache/sub")
            .link_to_dir("/home/u/cache/sub/..", "/home/u/cache")
            .directories_visited(
                "/home/u/cache/sub/..",
                &[
                    "/",
                    "/home",
                    "/home/u",
                    "/home/u/cache",
                    "/home/u/cache/sub",
                ],
            )
            .directories_visited(
                "/home/u/cache/pip/http",
                &[
                    "/",
                    "/home",
                    "/home/u",
                    "/home/u/cache",
                    "/home/u/cache/pip",
                ],
            ),
        WORKTREE,
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &["/home/u/cache"]);
}

#[test]
fn a_written_item_redirected_under_another_root_item_is_accepted() {
    let warnings = check(
        &layers(RW_WITH_A_NESTED_ITEM_AND_ANOTHER, None, &[], &[]),
        &variables(),
        rewired_through_cache(
            host().dir_with_ancestors("/home/u/other/http"),
            "/home/u/other/http",
            true,
        ),
        WORKTREE,
    )
    .unwrap();

    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn a_writable_item_one_of_whose_written_forms_was_redirected_onto_it_is_not_a_root() {
    // `~/other` is written honestly, but `~/cache/pip/http` now resolves to the same real
    // path through `~/cache`: the two forms merge into one item, and one of them
    // references something writable, so the item vouches for nothing, itself included.
    let diagnostic = check(
        &layers(RW_WITH_A_NESTED_ITEM_AND_ANOTHER, None, &[], &[]),
        &variables(),
        rewired_through_cache(host().dir("/home/u/other"), "/home/u/other", true),
        WORKTREE,
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &["/home/u/cache/pip/http", "/home/u/cache"]);
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
            None,
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
        None,
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

#[test]
fn a_cwd_re_exposed_by_a_descendant_rw_is_accepted() {
    let warnings = check(
        &layers(
            "[mounts]\nrw = [\"${worktree}\", \"/tmp/work\"]\nhide = [\"/tmp\"]",
            None,
            &[],
            &[],
        ),
        &variables(),
        host().dir_with_ancestors("/tmp/work/x"),
        "/tmp/work/x",
    )
    .unwrap();

    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn no_rw_over_the_workspace_yields_a_warning() {
    let warnings = check(
        &layers("[mounts]\nro = [\"${worktree}\"]", None, &[], &[]),
        &variables(),
        host(),
        WORKTREE,
    )
    .unwrap();

    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0]
            .to_string()
            .starts_with("process-wrap: warning: "),
        "{warnings:?}"
    );
}

#[test]
fn rw_over_the_workspace_alone_yields_no_warning() {
    for profile in [
        "[mounts]\nrw = [\"${worktree}\"]",
        "[mounts]\nrw = [\"/home/u/proj/sub\"]",
    ] {
        let variables = Variables {
            workspace: PathBuf::from("/home/u/proj/sub"),
            ..variables()
        };
        let warnings = check(
            &layers(profile, None, &[], &[]),
            &variables,
            host().dir("/home/u/proj/sub"),
            WORKTREE,
        )
        .unwrap();

        assert!(warnings.is_empty(), "{profile}: {warnings:?}");
    }
}
