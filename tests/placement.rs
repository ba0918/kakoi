use std::path::Path;

mod common;

use std::path::PathBuf;

use common::fixture::{
    command_line_layer, home, layers, merged, variables, xdg_profile_layer, xdg_variables, Facts,
    CONFIG_DIR, POLICY_FILE, WORKTREE, XDG_CONFIG_DIR, XDG_PROFILE,
};
use kakoi::diagnostic::{Diagnostic, Kind, Warning};
use kakoi::layers::{Layer, LayerOrigin};
use kakoi::mounts::{expand_policy, generate, resolve_written};
use kakoi::placement::{
    check_origins, check_placement, protected_paths, swappable_ro_items, written_paths,
};
use kakoi::policy::parse_policy;
use kakoi::variables::Variables;

/// Resolves the mounts of the written layers against `facts` and checks their placement
/// with the current directory at `current_dir`, the configuration directory at
/// `config_dir`, and `--workspace` given as `workspace`, in the order of stage 7 of
/// specification section 13: the scan roots and the `hide-mounts` `under`s against the
/// written items before generation, then the rest against the generated set.
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
    let written = written_paths(&expanded, workspace.map(Path::new));
    let before_generation = resolve_written(&expanded, &facts)?;
    check_origins(
        &expanded,
        &before_generation,
        &written,
        variables,
        Path::new(current_dir),
        &facts,
    )?;
    let swappable_ro = swappable_ro_items(
        &before_generation,
        &written,
        variables,
        Path::new(current_dir),
        &facts,
    );
    let resolved = generate(
        before_generation,
        &expanded,
        layers,
        variables,
        &facts,
        &swappable_ro,
    )?;
    let protected = protected_paths(&expanded, layers, Path::new(config_dir));
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
    // The `rw` and `rw-file` rows are held to the root items and the diagnostic names the
    // writable item passed through; the `ro` row is not (an `ro` exposes nothing new by
    // landing outside), and is refused as an exposing pair naming the `hide` it lands on.
    for (name, profile, policy_file, facts, partner) in [
        (
            "rw nested under rw, redirected to a directory",
            "[mounts]\nrw = [\"~/cache\", \"~/cache/pip/http\"]",
            None,
            rewired_through_cache(host().dir("/home/u/victim"), "/home/u/victim", true),
            "/home/u/cache",
        ),
        (
            "ro in the upper layer, redirected onto a hidden directory",
            "[mounts]\nrw = [\"~/cache\"]\nhide = [\"/home/u/secret\"]",
            Some("[mounts]\nro = [\"~/cache/pip/http\"]"),
            rewired_through_cache(host().dir("/home/u/secret"), "/home/u/secret", true),
            "/home/u/secret",
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
            "/home/u/cache",
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
        assert_path_diagnostic(&diagnostic, &["/home/u/cache/pip/http", partner]);
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
fn a_workspace_reached_through_a_link_must_land_in_a_root_not_derived_from_itself() {
    // `rw = ["${worktree}"]` after `~/proj/sub` was replaced by a link to `~/victim`:
    // `--workspace ~/proj/sub/inner` from elsewhere derives the worktree as
    // `~/victim/inner`, so `~/proj` is writable no more and the workspace's resolution
    // referenced nothing, yet the only root is the item derived from the redirected
    // workspace itself. The diagnostic names the link followed, apart from the workspace.
    let given = "/home/u/proj/sub/inner";
    let link = "/home/u/proj/sub";
    let real = "/home/u/victim/inner";
    let redirected = Variables {
        workspace: PathBuf::from(real),
        worktree: PathBuf::from(real),
        git_common_dir: None,
        ..variables()
    };

    let diagnostic = check_at(
        &layers("[mounts]\nrw = [\"${worktree}\"]", None, &[], &[]),
        &redirected,
        host()
            .dir_with_ancestors(real)
            .link_to_dir(link, "/home/u/victim")
            .link_to_dir(given, real)
            .links_traversed(given, &[link])
            .directories_visited(
                given,
                &["/", "/home", "/home/u", "/home/u/proj", "/home/u/victim"],
            )
            .directories_visited(real, &["/", "/home", "/home/u", "/home/u/victim"]),
        "/home/u/elsewhere",
        CONFIG_DIR,
        Some(given),
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &[given]);
    let apart_from_the_workspace = diagnostic.description().replace(given, "");
    assert!(
        apart_from_the_workspace.contains(link),
        "{diagnostic} does not mention the link followed apart from the workspace"
    );
}

#[test]
fn a_workspace_reached_through_a_link_landing_in_a_root_written_by_path_is_accepted() {
    // `rw = ["~/work"]` with `~/work` itself a link to `~/data/work`: `--workspace
    // ~/work/proj` follows that link, and lands inside an item whose written form does not
    // depend on the workspace.
    let given = "/home/u/work/proj";
    let behind_the_link = Variables {
        workspace: PathBuf::from("/home/u/data/work/proj"),
        worktree: PathBuf::from("/home/u/data/work/proj"),
        git_common_dir: None,
        ..variables()
    };

    let warnings = check_at(
        &layers("[mounts]\nrw = [\"~/work\"]", None, &[], &[]),
        &behind_the_link,
        host()
            .dir_with_ancestors("/home/u/data/work/proj")
            .link_to_dir("/home/u/work", "/home/u/data/work")
            .link_to_dir(given, "/home/u/data/work/proj")
            .links_traversed(given, &["/home/u/work"])
            .directories_visited(
                given,
                &["/", "/home", "/home/u", "/home/u/data", "/home/u/data/work"],
            ),
        "/home/u/elsewhere",
        CONFIG_DIR,
        Some(given),
    )
    .unwrap();

    assert!(warnings.is_empty(), "{warnings:?}");
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
fn a_referenced_item_replacing_a_lower_layer_hide_is_rejected() {
    // `--rw ~/a/link` resolves through `~/a`, a root item. Pointing at `~/a/real` it lands
    // in that root and replaces nothing. Re-pointed at `~/b/creds`, it still lands in a
    // root (`~/b`), but by the identity of section 5.4 it would replace the profile's
    // `hide` there and expose what the lower layer hid.
    let profile = "[mounts]\nrw = [\"~/a\", \"~/b\"]\nhide = [\"~/b/creds\"]";
    let facts = || {
        host()
            .dir("/home/u/a")
            .dir("/home/u/b")
            .dir("/home/u/b/creds")
            .links_traversed("/home/u/a/link", &["/home/u/a/link"])
    };

    let honest = check(
        &layers(profile, None, &["/home/u/a/link"], &[]),
        &variables(),
        facts()
            .dir("/home/u/a/real")
            .link_to_dir("/home/u/a/link", "/home/u/a/real"),
        WORKTREE,
    );
    assert!(honest.is_ok(), "{honest:?}");

    let diagnostic = check(
        &layers(profile, None, &["/home/u/a/link"], &[]),
        &variables(),
        facts().link_to_dir("/home/u/a/link", "/home/u/b/creds"),
        WORKTREE,
    )
    .unwrap_err();
    assert_path_diagnostic(&diagnostic, &["/home/u/a/link", "/home/u/b/creds"]);
}

#[test]
fn a_referenced_item_replacing_a_lower_layer_ro_is_rejected() {
    // The same shape with `ro` in the lower layer: replacing it would make the read-only
    // place writable.
    let diagnostic = check(
        &layers(
            "[mounts]\nrw = [\"~/a\", \"~/b\"]\nro = [\"~/b/creds\"]",
            None,
            &["/home/u/a/link"],
            &[],
        ),
        &variables(),
        host()
            .dir("/home/u/a")
            .dir("/home/u/b")
            .dir("/home/u/b/creds")
            .link_to_dir("/home/u/a/link", "/home/u/b/creds")
            .links_traversed("/home/u/a/link", &["/home/u/a/link"]),
        WORKTREE,
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &["/home/u/a/link", "/home/u/b/creds"]);
}

#[test]
fn a_referenced_item_landing_inside_a_lower_layer_hide_is_rejected() {
    // Re-pointed at `~/b/creds/sub`, `--rw ~/a/link` replaces nothing by the identity of
    // section 5.4, but it lands inside the profile's `hide` and section 6.4 mounts the
    // narrower item after it, exposing what the lower layer hid.
    let diagnostic = check(
        &layers(
            "[mounts]\nrw = [\"~/a\", \"~/b\"]\nhide = [\"~/b/creds\"]",
            None,
            &["/home/u/a/link"],
            &[],
        ),
        &variables(),
        host()
            .dir("/home/u/a")
            .dir("/home/u/b")
            .dir("/home/u/b/creds")
            .dir("/home/u/b/creds/sub")
            .link_to_dir("/home/u/a/link", "/home/u/b/creds/sub")
            .links_traversed("/home/u/a/link", &["/home/u/a/link"]),
        WORKTREE,
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &["/home/u/a/link", "/home/u/b/creds"]);
}

#[test]
fn a_lower_layer_rw_written_as_the_real_path_shields_the_hide_it_replaced() {
    // The policy file's `rw ~/b/creds`, written as the real path, replaces the profile's
    // `hide` there (section 5.4) and is what is left in force when `--rw ~/a/link` lands
    // inside; it already exposes the place, so the link exposes nothing new.
    let result = check(
        &layers(
            "[mounts]\nrw = [\"~/a\"]\nhide = [\"~/b/creds\"]",
            Some("[mounts]\nrw = [\"~/b/creds\"]"),
            &["/home/u/a/link"],
            &[],
        ),
        &variables(),
        host()
            .dir("/home/u/a")
            .dir("/home/u/b")
            .dir("/home/u/b/creds")
            .dir("/home/u/b/creds/sub")
            .link_to_dir("/home/u/a/link", "/home/u/b/creds/sub")
            .links_traversed("/home/u/a/link", &["/home/u/a/link"]),
        WORKTREE,
    );

    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn a_referenced_item_merging_with_an_ro_of_its_own_layer_is_accepted() {
    // Two `ro` forms of one layer that resolve to the same place merge into one item
    // (section 5.4); neither replaces the other, so the rule on lower layers does not
    // apply even though the second form resolves through `~/a`. (A `hide` in the linked
    // form would fall to the link rule of section 5.6 instead.)
    let result = check(
        &layers(
            "[mounts]\nrw = [\"~/a\", \"~/b\"]\nro = [\"~/b/creds\", \"~/a/link\"]",
            None,
            &[],
            &[],
        ),
        &variables(),
        host()
            .dir("/home/u/a")
            .dir("/home/u/b")
            .dir("/home/u/b/creds")
            .link_to_dir("/home/u/a/link", "/home/u/b/creds")
            .links_traversed("/home/u/a/link", &["/home/u/a/link"]),
        WORKTREE,
    );

    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn a_referenced_lower_layer_rw_landing_inside_an_upper_layer_hide_is_rejected() {
    // The profile's `rw ~/a/link` resolves through `~/a` and lands in `~/b/creds/sub`;
    // `--hide ~/b/creds` sits above it in the layers, but section 6.4 still mounts the
    // narrower `rw` after the `hide`, so the link exposes what the command line hid.
    let diagnostic = check(
        &layers(
            "[mounts]\nrw = [\"~/a\", \"~/b\", \"~/a/link\"]",
            None,
            &[],
            &["/home/u/b/creds"],
        ),
        &variables(),
        host()
            .dir("/home/u/a")
            .dir("/home/u/b")
            .dir("/home/u/b/creds")
            .dir("/home/u/b/creds/sub")
            .link_to_dir("/home/u/a/link", "/home/u/b/creds/sub")
            .links_traversed("/home/u/a/link", &["/home/u/a/link"])
            .directories_visited("/home/u/b/creds", &["/", "/home", "/home/u", "/home/u/b"]),
        WORKTREE,
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &["/home/u/a/link", "/home/u/b/creds"]);
}

#[test]
fn a_referenced_rw_landing_inside_a_hide_of_its_own_layer_is_rejected() {
    // The same shape with the `hide` written in the profile itself: the layer makes no
    // difference to the order of section 6.4.
    let diagnostic = check(
        &layers(
            "[mounts]\nrw = [\"~/a\", \"~/b\", \"~/a/link\"]\nhide = [\"~/b/creds\"]",
            None,
            &[],
            &[],
        ),
        &variables(),
        host()
            .dir("/home/u/a")
            .dir("/home/u/b")
            .dir("/home/u/b/creds")
            .dir("/home/u/b/creds/sub")
            .link_to_dir("/home/u/a/link", "/home/u/b/creds/sub")
            .links_traversed("/home/u/a/link", &["/home/u/a/link"])
            .directories_visited("/home/u/b/creds", &["/", "/home", "/home/u", "/home/u/b"]),
        WORKTREE,
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &["/home/u/a/link", "/home/u/b/creds"]);
}

#[test]
fn a_referenced_hide_restating_or_narrowing_a_lower_layer_hide_is_accepted() {
    // `~/b/creds` and `~/b/creds/sub` resolve through the profile's `rw ~/b`, so a `--hide`
    // on either is a referenced item, but a `hide` exposes nothing whatever it lands on.
    let through_b = &["/", "/home", "/home/u", "/home/u/b"];
    for hide in ["/home/u/b/creds", "/home/u/b/creds/sub"] {
        let result = check(
            &layers(
                "[mounts]\nrw = [\"~/b\"]\nhide = [\"~/b/creds\"]",
                None,
                &[],
                &[hide],
            ),
            &variables(),
            host()
                .dir("/home/u/b")
                .dir("/home/u/b/creds")
                .dir("/home/u/b/creds/sub")
                .directories_visited("/home/u/b/creds", through_b)
                .directories_visited("/home/u/b/creds/sub", through_b),
            WORKTREE,
        );

        assert!(result.is_ok(), "{hide}: {result:?}");
    }
}

#[test]
fn a_referenced_ro_replacing_a_lower_layer_ro_is_accepted() {
    // The policy file's `ro ~/a/link`, re-pointed at `~/b/creds`, replaces the profile's
    // `ro` there by the identity of section 5.4, but `ro` over `ro` exposes nothing.
    let result = check(
        &layers(
            "[mounts]\nrw = [\"~/a\", \"~/b\"]\nro = [\"~/b/creds\"]",
            Some("[mounts]\nro = [\"~/a/link\"]"),
            &[],
            &[],
        ),
        &variables(),
        host()
            .dir("/home/u/a")
            .dir("/home/u/b")
            .dir("/home/u/b/creds")
            .link_to_dir("/home/u/a/link", "/home/u/b/creds")
            .links_traversed("/home/u/a/link", &["/home/u/a/link"])
            .directories_visited("/home/u/b/creds", &["/", "/home", "/home/u", "/home/u/b"]),
        WORKTREE,
    );

    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn a_referenced_ro_landing_inside_a_lower_layer_hide_is_rejected() {
    // Re-pointed at `~/b/creds/sub`, the policy file's `ro ~/a/link` lands inside the
    // profile's `hide`, and section 6.4 mounts it after the `hide`: what was hidden
    // becomes readable.
    let diagnostic = check(
        &layers(
            "[mounts]\nrw = [\"~/a\", \"~/b\"]\nhide = [\"~/b/creds\"]",
            Some("[mounts]\nro = [\"~/a/link\"]"),
            &[],
            &[],
        ),
        &variables(),
        host()
            .dir("/home/u/a")
            .dir("/home/u/b")
            .dir("/home/u/b/creds")
            .dir("/home/u/b/creds/sub")
            .link_to_dir("/home/u/a/link", "/home/u/b/creds/sub")
            .links_traversed("/home/u/a/link", &["/home/u/a/link"])
            .directories_visited("/home/u/b/creds", &["/", "/home", "/home/u", "/home/u/b"]),
        WORKTREE,
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &["/home/u/a/link", "/home/u/b/creds"]);
}

/// The facts of `~/a/l/y` with `~/a/l` a link to the directory `target`, which holds `y`:
/// the resolution passes through `a` and follows a link that sits inside it.
fn a_l_pointing_at(facts: Facts, target: &str) -> Facts {
    let given = "/home/u/a/l/y";
    let real = format!("{target}/y");
    facts
        .dir("/home/u/a")
        .dir("/home/u/b")
        .dir_with_ancestors(&real)
        .link_to_dir(given, &real)
        .links_traversed(given, &["/home/u/a/l"])
        .directories_visited(given, &["/", "/home", "/home/u", "/home/u/a", target])
}

#[test]
fn an_ro_written_through_a_writable_link_is_accepted_wherever_it_lands_outside_hide_and_ro() {
    // `rw = ["~/a", "~/b"]` with `a/l -> b/x`: `ro ~/a/l/y` resolves through `a`. Wherever
    // the link is pointed, an `ro` makes nothing newly visible unless it lands on a `hide`
    // or an `ro`, so it is not held to the root items: landing in `b` and landing in a
    // place that is no item at all are both accepted.
    let profile = "[mounts]\nrw = [\"~/a\", \"~/b\"]\nro = [\"~/a/l/y\"]";

    for (name, target) in [
        ("into another rw", "/home/u/b/x"),
        ("into no item", "/home/u/elsewhere/x"),
    ] {
        let result = check(
            &layers(profile, None, &[], &[]),
            &variables(),
            a_l_pointing_at(host(), target),
            WORKTREE,
        );

        assert!(result.is_ok(), "{name}: {result:?}");
    }
}

#[test]
fn an_ro_written_through_a_writable_link_landing_inside_a_hide_is_rejected() {
    // The same `ro ~/a/l/y` with `l` pointed into the profile's `hide ~/b/creds`: the
    // narrower `ro` mounts after the `hide` (section 6.4) and shows what it hid.
    let diagnostic = check(
        &layers(
            "[mounts]\nrw = [\"~/a\", \"~/b\"]\nro = [\"~/a/l/y\"]\nhide = [\"~/b/creds\"]",
            None,
            &[],
            &[],
        ),
        &variables(),
        a_l_pointing_at(host(), "/home/u/b/creds"),
        WORKTREE,
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &["/home/u/a/l/y", "/home/u/b/creds"]);
}

#[test]
fn a_hide_whose_resolution_follows_a_link_inside_a_writable_item_is_rejected_wherever_it_lands() {
    // A `hide` written as a link inside `rw` is skipped by the next start once the link is
    // removed from inside the isolation, and what it hid shows through. That holds whether
    // the link lands outside every writable item (`~/proj/secrets -> ~/vault`) or inside a
    // root item (`~/a/l -> ~/b/creds`), where the root-item check alone would let it pass.
    for (name, profile, given, facts) in [
        (
            "landing outside every writable item",
            "[mounts]\nrw = [\"~/proj\"]\nhide = [\"~/proj/secrets\"]",
            "/home/u/proj/secrets",
            host()
                .dir("/home/u/vault")
                .link_to_dir("/home/u/proj/secrets", "/home/u/vault")
                .links_traversed("/home/u/proj/secrets", &["/home/u/proj/secrets"])
                .directories_visited(
                    "/home/u/proj/secrets",
                    &["/", "/home", "/home/u", "/home/u/proj"],
                ),
        ),
        (
            "landing inside a root item",
            "[mounts]\nrw = [\"~/a\", \"~/b\"]\nhide = [\"~/a/l\"]",
            "/home/u/a/l",
            host()
                .dir("/home/u/a")
                .dir("/home/u/b")
                .dir("/home/u/b/creds")
                .link_to_dir("/home/u/a/l", "/home/u/b/creds")
                .links_traversed("/home/u/a/l", &["/home/u/a/l"])
                .directories_visited("/home/u/a/l", &["/", "/home", "/home/u", "/home/u/a"]),
        ),
    ] {
        let diagnostic = check(
            &layers(profile, None, &[], &[]),
            &variables(),
            facts,
            WORKTREE,
        )
        .unwrap_err();

        assert_eq!(diagnostic.kind(), Kind::Path, "{name}: {diagnostic}");
        assert_path_diagnostic(&diagnostic, &[given]);
    }
}

#[test]
fn a_hide_written_by_real_path_inside_an_rw_item_is_accepted() {
    // `rw = ["~/cache"]` with `hide = ["~/cache/x"]`, `x` a real directory: the resolution
    // passes through `cache` but follows no link, and a mount point one level deep cannot
    // be renamed from inside, so the `hide` stays where it was written.
    let result = check(
        &layers(
            "[mounts]\nrw = [\"~/cache\"]\nhide = [\"~/cache/x\"]",
            None,
            &[],
            &[],
        ),
        &variables(),
        host()
            .dir("/home/u/cache")
            .dir("/home/u/cache/x")
            .directories_visited(
                "/home/u/cache/x",
                &["/", "/home", "/home/u", "/home/u/cache"],
            ),
        WORKTREE,
    );

    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn a_hide_that_fails_both_rules_names_the_followed_link() {
    // `hide = ["~/x"]` with `~/x -> ~/proj/link/y` and `~/proj/link -> ~/vault`: the item
    // lands outside every root (the root-item check) and follows a link inside `rw ~/proj`
    // (the link rule). The link rule's diagnostic is the one shown, since the link it names
    // is what the user has to change, so `~/proj/link` appears though it is no prefix of
    // the item's own path.
    let given = "/home/u/x";
    let diagnostic = check(
        &layers(
            "[mounts]\nrw = [\"~/proj\"]\nhide = [\"~/x\"]",
            None,
            &[],
            &[],
        ),
        &variables(),
        host()
            .dir_with_ancestors("/home/u/vault/y")
            .link_to_dir("/home/u/proj/link", "/home/u/vault")
            .link_to_dir(given, "/home/u/vault/y")
            .links_traversed(given, &[given, "/home/u/proj/link"])
            .directories_visited(
                given,
                &["/", "/home", "/home/u", "/home/u/proj", "/home/u/vault"],
            ),
        WORKTREE,
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &[given, "/home/u/proj/link"]);
}

#[test]
fn a_hide_written_with_a_workspace_variable_does_not_count_inherited_references() {
    // `rw = ["${worktree}", "~/cache"]` with `hide = ["${worktree}/x"]`, `x` a real path,
    // and `--workspace ~/cache/proj` given from elsewhere with `proj` a link to
    // `~/cache/real`. The hide's own resolution follows no link; the link the workspace
    // followed is inherited as a reference for the root-item check but is not the hide's
    // to fail on, and the workspace itself lands inside `~/cache`, a root written by path.
    let behind_the_link = Variables {
        workspace: PathBuf::from("/home/u/cache/real"),
        worktree: PathBuf::from("/home/u/cache/real"),
        git_common_dir: None,
        ..variables()
    };
    let through_cache = &["/", "/home", "/home/u", "/home/u/cache"];

    let result = check_at(
        &layers(
            "[mounts]\nrw = [\"${worktree}\", \"~/cache\"]\nhide = [\"${worktree}/x\"]",
            None,
            &[],
            &[],
        ),
        &behind_the_link,
        host()
            .dir("/home/u/cache")
            .dir("/home/u/cache/real")
            .dir("/home/u/cache/real/x")
            .link_to_dir("/home/u/cache/proj", "/home/u/cache/real")
            .links_traversed("/home/u/cache/proj", &["/home/u/cache/proj"])
            .directories_visited("/home/u/cache/proj", through_cache)
            .directories_visited("/home/u/cache/real", through_cache)
            .directories_visited(
                "/home/u/cache/real/x",
                &[
                    "/",
                    "/home",
                    "/home/u",
                    "/home/u/cache",
                    "/home/u/cache/real",
                ],
            ),
        "/home/u/elsewhere",
        CONFIG_DIR,
        Some("/home/u/cache/proj"),
    );

    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn an_item_landing_inside_a_generated_hide_is_rejected_as_an_exposing_pair() {
    // `ro ~/a/link` resolves through `rw ~/a` and lands inside a `hide` the generation
    // step made: the configuration directory's `secrets/` (a file `secrets` does not name),
    // or a mount `hide-mounts` hides. The narrower `ro` would mount after the generated
    // `hide` (section 6.4) and show what it hid, so the generated `hide` counts as a
    // partner of the exposing pair; the diagnostic names it.
    let secrets_dir = format!("{CONFIG_DIR}/secrets");
    let other_secret = format!("{secrets_dir}/other");
    for (name, profile, facts, partner) in [
        (
            "into the configuration directory's secrets/",
            "[mounts]\nrw = [\"~/a\"]\nro = [\"~/a/link\"]".to_string(),
            host()
                .dir(&secrets_dir)
                .file(&other_secret)
                .link_to_file("/home/u/a/link", &other_secret),
            secrets_dir.as_str(),
        ),
        (
            "into a mount hidden by hide-mounts",
            "[mounts]\nrw = [\"~/a\"]\nro = [\"~/a/link\"]\n\
             [[mounts.hide-mounts]]\nunder = \"/mnt\"\nfstype = [\"9p\"]"
                .to_string(),
            host()
                .dir("/mnt")
                .mount("/mnt/c", "9p")
                .dir("/mnt/c/x")
                .link_to_dir("/home/u/a/link", "/mnt/c/x"),
            "/mnt/c",
        ),
    ] {
        let diagnostic = check(
            &layers(&profile, None, &[], &[]),
            &variables(),
            facts
                .dir("/home/u/a")
                .links_traversed("/home/u/a/link", &["/home/u/a/link"]),
            WORKTREE,
        )
        .unwrap_err();

        assert_eq!(diagnostic.kind(), Kind::Path, "{name}: {diagnostic}");
        assert_path_diagnostic(&diagnostic, &["/home/u/a/link", partner]);
    }
}

#[test]
fn an_item_landing_on_the_fixed_mount_targets_is_rejected() {
    // The fixed part of the bwrap arguments mounts `/`, `/dev`, and `/proc` itself
    // (section 14). An item over `/proc` covers the isolation's `/proc` with the host's and
    // shows the host's processes; over `/dev` or `/` the ground of the isolation is gone.
    // The directive and whether the item referenced anything make no difference. The
    // diagnostic names the item as written and where it lands.
    for (name, profile, facts, mentions) in [
        (
            "hide identical to /",
            "[mounts]\nhide = [\"/\"]",
            host(),
            &["/"][..],
        ),
        (
            "hide inside /dev",
            "[mounts]\nhide = [\"/dev/shm\"]",
            host().dir("/dev").dir("/dev/shm"),
            &["/dev/shm"],
        ),
        (
            "ro identical to /proc",
            "[mounts]\nro = [\"/proc\"]",
            host().dir("/proc"),
            &["/proc"],
        ),
        (
            "ro through a link pointed at /proc",
            "[mounts]\nrw = [\"~/a\"]\nro = [\"~/a/link\"]",
            host()
                .dir("/home/u/a")
                .dir("/proc")
                .link_to_dir("/home/u/a/link", "/proc")
                .links_traversed("/home/u/a/link", &["/home/u/a/link"]),
            &["~/a/link", "/proc"],
        ),
    ] {
        let diagnostic = check(
            &layers(profile, None, &[], &[]),
            &variables(),
            facts,
            WORKTREE,
        )
        .unwrap_err();

        assert_eq!(diagnostic.kind(), Kind::Path, "{name}: {diagnostic}");
        assert_path_diagnostic(&diagnostic, mentions);
    }
}

#[test]
fn an_item_under_root_but_outside_dev_and_proc_is_accepted() {
    let result = check(
        &layers("[mounts]\nro = [\"/etc/hosts\"]", None, &[], &[]),
        &variables(),
        host().file_with_ancestors("/etc/hosts"),
        WORKTREE,
    );

    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn a_scan_root_resolving_through_a_writable_item_must_be_a_writable_mount_point() {
    // A scan root is not mounted, so a root below an `rw` item can be renamed from inside
    // the isolation, and the next start walks an empty tree and hides no `.env`. The mount
    // point of an `rw` item itself cannot be renamed, and a root that referenced nothing
    // writable is not at issue. The diagnostic names the root and the `rw` item it
    // resolved through. (A root behind a link inside `rw` is the link rule's, below.)
    let through_b = &["/", "/home", "/home/u", "/home/u/b"];
    for (name, profile, facts, mentions) in [
        (
            "a real directory below rw",
            "[mounts]\nrw = [\"~/b\"]\n[[mounts.scan]]\nroot = \"~/b/tree\"\nnames = [\".env\"]",
            host()
                .dir("/home/u/b")
                .dir("/home/u/b/tree")
                .directories_visited("/home/u/b/tree", through_b),
            Some(&["/home/u/b/tree", "/home/u/b"][..]),
        ),
        (
            "the mount point of rw itself",
            "[mounts]\nrw = [\"~/b\"]\n[[mounts.scan]]\nroot = \"~/b\"\nnames = [\".env\"]",
            host()
                .dir("/home/u/b")
                .directories_visited("/home/u/b", &["/", "/home", "/home/u"]),
            None,
        ),
        (
            "the worktree, referencing nothing writable",
            "[mounts]\nrw = [\"~/b\"]\n[[mounts.scan]]\nroot = \"${worktree}\"\nnames = [\".env\"]",
            host()
                .dir("/home/u/b")
                .directories_visited(WORKTREE, &["/", "/home", "/home/u"]),
            None,
        ),
    ] {
        let result = check(
            &layers(profile, None, &[], &[]),
            &variables(),
            facts,
            WORKTREE,
        );

        match mentions {
            Some(mentions) => {
                let diagnostic = result.expect_err(name);
                assert_eq!(diagnostic.kind(), Kind::Path, "{name}: {diagnostic}");
                assert_path_diagnostic(&diagnostic, mentions);
            }
            None => assert!(result.is_ok(), "{name}: {result:?}"),
        }
    }
}

#[test]
fn a_scan_origin_following_a_link_inside_a_writable_item_is_rejected_wherever_it_lands() {
    // A scan `root` or a `hide-mounts` `under` written as a link inside `rw` is skipped by
    // the next start once the link is removed from inside the isolation, and nothing under
    // it is hidden. That holds even where it lands on the mount point of `rw ~/b`, which
    // the mount-point rule alone would let through. The diagnostic names the origin and
    // the link followed; the mount point written as its real path, and the worktree, which
    // follow no link, pass.
    let through_a = &["/", "/home", "/home/u", "/home/u/a"];
    let two_roots = "[mounts]\nrw = [\"~/a\", \"~/b\"]\n";
    let link_to_b = || {
        host()
            .dir("/home/u/a")
            .dir("/home/u/b")
            .link_to_dir("/home/u/a/link", "/home/u/b")
            .links_traversed("/home/u/a/link", &["/home/u/a/link"])
            .directories_visited("/home/u/a/link", through_a)
    };
    for (name, profile, facts, mentions) in [
        (
            "a root through a link inside rw landing on the mount point of another rw",
            format!("{two_roots}[[mounts.scan]]\nroot = \"~/a/link\"\nnames = [\".env\"]"),
            link_to_b(),
            Some(&["/home/u/a/link"][..]),
        ),
        (
            "an under through a link inside rw landing on the mount point of another rw",
            format!("{two_roots}[[mounts.hide-mounts]]\nunder = \"~/a/link\"\nfstype = [\"9p\"]"),
            link_to_b(),
            Some(&["/home/u/a/link"][..]),
        ),
        (
            "a root that is no link itself but follows one inside rw",
            format!("{two_roots}[[mounts.scan]]\nroot = \"~/x\"\nnames = [\".env\"]"),
            link_to_b()
                .link_to_dir("/home/u/x", "/home/u/b")
                .links_traversed("/home/u/x", &["/home/u/x", "/home/u/a/link"])
                .directories_visited("/home/u/x", through_a),
            Some(&["/home/u/x", "/home/u/a/link"][..]),
        ),
        (
            "the mount point of rw itself",
            format!("{two_roots}[[mounts.scan]]\nroot = \"~/b\"\nnames = [\".env\"]"),
            host()
                .dir("/home/u/a")
                .dir("/home/u/b")
                .directories_visited("/home/u/b", &["/", "/home", "/home/u"]),
            None,
        ),
        (
            "the worktree, referencing nothing writable",
            format!("{two_roots}[[mounts.scan]]\nroot = \"${{worktree}}\"\nnames = [\".env\"]"),
            host()
                .dir("/home/u/a")
                .dir("/home/u/b")
                .directories_visited(WORKTREE, &["/", "/home", "/home/u"]),
            None,
        ),
    ] {
        let result = check(
            &layers(&profile, None, &[], &[]),
            &variables(),
            facts,
            WORKTREE,
        );

        match mentions {
            Some(mentions) => {
                let diagnostic = result.expect_err(name);
                assert_eq!(diagnostic.kind(), Kind::Path, "{name}: {diagnostic}");
                assert_path_diagnostic(&diagnostic, mentions);
            }
            None => assert!(result.is_ok(), "{name}: {result:?}"),
        }
    }
}

#[test]
fn a_scan_origin_that_fails_both_rules_names_the_followed_link() {
    // `root = "~/x"` with `~/x -> ~/a/link/y` and `~/a/link -> ~/vault`: the root lands
    // outside every writable item and is no mount point (the mount-point rule) and follows
    // a link inside `rw ~/a` (the link rule). The link rule's diagnostic is the one shown,
    // since the link it names is what the user has to change, so `~/a/link` appears though
    // it is no prefix of the root's own path.
    let given = "/home/u/x";
    let diagnostic = check(
        &layers(
            "[mounts]\nrw = [\"~/a\"]\n[[mounts.scan]]\nroot = \"~/x\"\nnames = [\".env\"]",
            None,
            &[],
            &[],
        ),
        &variables(),
        host()
            .dir("/home/u/a")
            .dir_with_ancestors("/home/u/vault/y")
            .link_to_dir("/home/u/a/link", "/home/u/vault")
            .link_to_dir(given, "/home/u/vault/y")
            .links_traversed(given, &[given, "/home/u/a/link"])
            .directories_visited(
                given,
                &["/", "/home", "/home/u", "/home/u/a", "/home/u/vault"],
            ),
        WORKTREE,
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &[given, "/home/u/a/link"]);
}

#[test]
fn a_hide_mounts_under_resolving_through_a_writable_item_must_be_a_writable_mount_point() {
    // The same rule for the `under` of `hide-mounts`: below `rw ~/b` it could be renamed
    // from inside and the next start would hide no mount; the mount point itself passes.
    let through_b = &["/", "/home", "/home/u", "/home/u/b"];
    let profile = |under: &str| {
        format!("[mounts]\nrw = [\"~/b\"]\n[[mounts.hide-mounts]]\nunder = \"{under}\"\nfstype = [\"9p\"]")
    };

    let below = check(
        &layers(&profile("~/b/mnt"), None, &[], &[]),
        &variables(),
        host()
            .dir("/home/u/b")
            .dir("/home/u/b/mnt")
            .directories_visited("/home/u/b/mnt", through_b),
        WORKTREE,
    )
    .unwrap_err();
    assert_path_diagnostic(&below, &["/home/u/b/mnt", "/home/u/b"]);

    let at_the_mount_point = check(
        &layers(&profile("~/b"), None, &[], &[]),
        &variables(),
        host()
            .dir("/home/u/b")
            .directories_visited("/home/u/b", &["/", "/home", "/home/u"]),
        WORKTREE,
    );
    assert!(at_the_mount_point.is_ok(), "{at_the_mount_point:?}");
}

#[test]
fn a_scan_root_is_judged_against_the_writable_items_before_generation() {
    // `hide-mounts` hides `~/b`, a 9p mount, and that generated `hide` replaces `rw ~/b`
    // (section 6.3): after generation nothing is writable and `root = "~/b/tree"` would
    // pass. The origins are judged before generation, where `~/b` is still writable, so
    // the root below it is refused.
    let diagnostic = check(
        &layers(
            "[mounts]\nrw = [\"~/b\"]\n\
             [[mounts.scan]]\nroot = \"~/b/tree\"\nnames = [\".env\"]\n\
             [[mounts.hide-mounts]]\nunder = \"/home/u\"\nfstype = [\"9p\"]",
            None,
            &[],
            &[],
        ),
        &variables(),
        host()
            .mount("/home/u/b", "9p")
            .dir("/home/u/b/tree")
            .directories_visited("/home/u/b/tree", &["/", "/home", "/home/u", "/home/u/b"])
            .directories_visited("/home/u", &["/", "/home"]),
        WORKTREE,
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &["/home/u/b/tree", "/home/u/b"]);
}

#[test]
fn a_scan_root_diagnostic_precedes_a_written_item_diagnostic() {
    // `rw ~/cache/pip/http` rewired to outside every writable item and `root = "~/b/tree"`
    // below `rw ~/b` are both refused; the root is judged first (specification
    // section 13, stage 7), so its diagnostic is the one shown.
    let diagnostic = check(
        &layers(
            "[mounts]\nrw = [\"~/b\", \"~/cache\", \"~/cache/pip/http\"]\n\
             [[mounts.scan]]\nroot = \"~/b/tree\"\nnames = [\".env\"]",
            None,
            &[],
            &[],
        ),
        &variables(),
        rewired_through_cache(host().dir("/home/u/victim"), "/home/u/victim", true)
            .dir("/home/u/b")
            .dir("/home/u/b/tree")
            .directories_visited("/home/u/b/tree", &["/", "/home", "/home/u", "/home/u/b"]),
        WORKTREE,
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &["/home/u/b/tree"]);
    assert!(
        !diagnostic.description().contains("/home/u/cache"),
        "{diagnostic} is about the written item, not the scan root"
    );
}

#[test]
fn a_scan_root_diagnostic_precedes_a_hide_mounts_under_diagnostic() {
    // Both origins lie below `rw ~/b`; the roots come before the `under`s.
    let through_b = &["/", "/home", "/home/u", "/home/u/b"];
    let diagnostic = check(
        &layers(
            "[mounts]\nrw = [\"~/b\"]\n\
             [[mounts.hide-mounts]]\nunder = \"~/b/mnt\"\nfstype = [\"9p\"]\n\
             [[mounts.scan]]\nroot = \"~/b/tree\"\nnames = [\".env\"]",
            None,
            &[],
            &[],
        ),
        &variables(),
        host()
            .dir("/home/u/b")
            .dir("/home/u/b/tree")
            .dir("/home/u/b/mnt")
            .directories_visited("/home/u/b/tree", through_b)
            .directories_visited("/home/u/b/mnt", through_b),
        WORKTREE,
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &["/home/u/b/tree"]);
    assert!(
        !diagnostic.description().contains("/home/u/b/mnt"),
        "{diagnostic} is about the `under`, not the scan root"
    );
}

#[test]
fn a_scan_root_written_with_a_workspace_variable_inherits_the_workspace_references() {
    // `rw = ["~/cache"]` with `root = "${worktree}"` and `--workspace ~/cache/proj` given
    // from elsewhere after `proj` became a link to `~/victim`. The root expands to the
    // real path `~/victim` and its own resolution references nothing writable, but the
    // workspace it came from was reached through `~/cache`, so the root inherits that
    // reference and is refused as no mount point of a writable item. The diagnostic names
    // the scan root, not the `--workspace` path.
    let (redirected, facts) = workspace_rewired_through_cache();

    let diagnostic = check_at(
        &layers(
            "[mounts]\nrw = [\"~/cache\"]\n[[mounts.scan]]\nroot = \"${worktree}\"\nnames = [\".env\"]",
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
    assert!(
        !diagnostic.description().contains("/home/u/cache/proj"),
        "{diagnostic} is about the workspace, not the scan root"
    );
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
        warnings[0].to_string().starts_with("kakoi: warning: "),
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

// The `rw-copy` directive against the placement rules: nothing written inside one reaches
// the host, so it is not a writable item, and it is exempt from the landing rule for the
// same reason `ro` is. What it can still do is show the host's content where a `hide`
// covered it, and what an `rw` can still do to it is let the writes through.

#[test]
fn a_policy_file_inside_an_rw_copy_area_is_accepted() {
    // The policy file's own place is protected because a writable item there lets the next
    // start read something else. An `rw-copy` gives the isolation a copy: the rewrite dies
    // with the tmpfs, the host's file and the names on the way to it are untouched, and the
    // next start reads what it read this time.
    let result = check(
        &layers(
            &format!("[mounts]\nrw-copy = [\"{}\"]", "/home/u/policies"),
            Some(""),
            &[],
            &[],
        ),
        &variables(),
        host().file_with_ancestors(POLICY_FILE),
        WORKTREE,
    );

    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn the_configuration_directory_and_a_secret_file_inside_an_rw_copy_area_are_accepted() {
    let result = check(
        &layers(
            "[mounts]\nrw-copy = [\"~/.config\"]\n\
             [secrets]\nT = \"${config_dir}/secrets/t\"",
            None,
            &[],
            &[],
        ),
        &variables(),
        host().file_with_ancestors("/home/u/.config/kakoi/secrets/t"),
        WORKTREE,
    );

    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn a_referenced_rw_copy_landing_inside_a_hide_is_rejected() {
    // `rw-copy ~/a/link` resolves through the `rw ~/a` and lands on the hidden `~/b/creds`.
    // Section 6.4 mounts the narrower item after the `hide`, and an `rw-copy` starts from
    // the host's content, so the link would show what the `hide` covered.
    let diagnostic = check(
        &layers(
            "[mounts]\nrw = [\"~/a\", \"~/b\"]\nhide = [\"~/b/creds\"]",
            Some("[mounts]\nrw-copy = [\"~/a/link\"]"),
            &[],
            &[],
        ),
        &variables(),
        host()
            .dir("/home/u/a")
            .dir("/home/u/b")
            .dir("/home/u/b/creds")
            .link_to_dir("/home/u/a/link", "/home/u/b/creds")
            .links_traversed("/home/u/a/link", &["/home/u/a/link"])
            .directories_visited("/home/u/b/creds", &["/", "/home", "/home/u", "/home/u/b"]),
        WORKTREE,
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &["/home/u/a/link", "/home/u/b/creds"]);
}

#[test]
fn a_referenced_rw_copy_landing_inside_an_ro_is_accepted() {
    // The same shape over an `ro`: the copy shows the bytes the `ro` already showed and
    // writes none of them back, so nothing is newly readable or writable. This is the rule
    // `ro` itself is held to.
    let result = check(
        &layers(
            "[mounts]\nrw = [\"~/a\", \"~/b\"]\nro = [\"~/b/creds\"]",
            Some("[mounts]\nrw-copy = [\"~/a/link\"]"),
            &[],
            &[],
        ),
        &variables(),
        host()
            .dir("/home/u/a")
            .dir("/home/u/b")
            .dir("/home/u/b/creds")
            .link_to_dir("/home/u/a/link", "/home/u/b/creds")
            .links_traversed("/home/u/a/link", &["/home/u/a/link"])
            .directories_visited("/home/u/b/creds", &["/", "/home", "/home/u", "/home/u/b"]),
        WORKTREE,
    );

    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn a_referenced_rw_landing_inside_an_rw_copy_is_rejected() {
    // The other direction: the user asked for a place whose writes end with the run, and a
    // redirectable `rw` over it is mounted afterwards and lets them through to the host.
    let diagnostic = check(
        &layers(
            "[mounts]\nrw = [\"~/a\", \"~/b\"]\nrw-copy = [\"~/b/creds\"]",
            Some("[mounts]\nrw-file = [\"~/a/link\"]"),
            &[],
            &[],
        ),
        &variables(),
        host()
            .dir("/home/u/a")
            .dir("/home/u/b")
            .dir("/home/u/b/creds")
            .file("/home/u/b/creds/k")
            .link_to_file("/home/u/a/link", "/home/u/b/creds/k")
            .links_traversed("/home/u/a/link", &["/home/u/a/link"])
            .directories_visited("/home/u/b/creds", &["/", "/home", "/home/u", "/home/u/b"]),
        WORKTREE,
    )
    .unwrap_err();

    assert_path_diagnostic(&diagnostic, &["/home/u/a/link", "/home/u/b/creds"]);
}

#[test]
fn an_rw_copy_over_the_work_place_still_warns_that_no_rw_covers_it() {
    // The warning of section 6.5 is about work that survives the run. An `rw-copy` over the
    // worktree gives the command a copy it can write and lose, which is exactly the case
    // the warning is for.
    let warnings = check(
        &layers("[mounts]\nrw-copy = [\"${worktree}\"]", None, &[], &[]),
        &variables(),
        host(),
        WORKTREE,
    )
    .unwrap();

    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].to_string().contains(WORKTREE), "{warnings:?}");
}
