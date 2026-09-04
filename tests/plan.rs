use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

mod common;

use common::fixture::{
    home, layers, merged, variables, Facts, CONFIG_DIR, POLICY_FILE, PROFILE, WORKTREE,
};
use common::{assert_diagnostic, binary, output_report, TempDir};
use process_wrap::command::{command_candidates, resolve_command};
use process_wrap::diagnostic::{Diagnostic, Kind};
use process_wrap::executables::first_executable;
use process_wrap::isolated_env::SecretFile;
use process_wrap::layers::{Directive, LayerOrigin};
use process_wrap::mounts::{expand_policy, EntryKind, ItemOrigin, ResolvedItem};
use process_wrap::plan::{
    bwrap_arguments, resolve_isolation, Argument, Inputs, Isolation, IsolationFacts,
};
use process_wrap::policy::NetworkMode;

/// Writes an executable script at `relative` under `dir`.
fn executable(dir: &TempDir, relative: &str) -> PathBuf {
    let path = dir.write(relative, "#!/bin/sh\n");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn a_command_with_a_slash_must_exist_and_be_executable() {
    let dir = TempDir::new();
    let exe = executable(&dir, "bin/exe");
    let plain = dir.write("bin/plain", "#!/bin/sh\n");
    let missing = dir.path().join("bin/missing");

    let candidates =
        |path: &Path| command_candidates(path.as_os_str(), Some(OsStr::new("/usr/bin")));
    assert_eq!(candidates(&exe), std::slice::from_ref(&exe));
    assert_eq!(first_executable(&candidates(&exe)), Some(exe));
    assert_eq!(first_executable(&candidates(&plain)), None);
    assert_eq!(first_executable(&candidates(&missing)), None);
    assert_eq!(first_executable(&candidates(dir.path())), None);
}

#[test]
fn a_command_is_searched_on_the_isolated_path() {
    let dir = TempDir::new();
    let tool = executable(&dir, "second/tool");
    dir.write("first/tool", "");
    let path = format!(
        "{}:{}",
        dir.path().join("first").display(),
        dir.path().join("second").display()
    );

    let candidates = command_candidates(OsStr::new("tool"), Some(OsStr::new(&path)));

    assert_eq!(
        candidates,
        [
            dir.path().join("first/tool"),
            dir.path().join("second/tool")
        ]
    );
    assert_eq!(first_executable(&candidates), Some(tool));
    assert!(command_candidates(OsStr::new("tool"), None).is_empty());
}

#[test]
fn an_unresolvable_command_is_a_command_not_found_diagnostic_naming_the_command() {
    let diagnostic = resolve_command(OsStr::new("no-such-tool"), None).unwrap_err();

    assert_eq!(diagnostic.kind(), Kind::CommandNotFound);
    assert_eq!(diagnostic.description(), "no-such-tool");
    assert_eq!(diagnostic.exit_code(), 127);

    let found = resolve_command(OsStr::new("tool"), Some(PathBuf::from("/opt/bin/tool"))).unwrap();
    assert_eq!(found, PathBuf::from("/opt/bin/tool"));
}

fn item(directive: Directive, real: &str, kind: EntryKind) -> ResolvedItem {
    ResolvedItem {
        directive,
        real: PathBuf::from(real),
        kind,
        origin: ItemOrigin::Written(LayerOrigin::CommandLine),
        written: real.to_string(),
    }
}

fn literal(text: &str) -> Argument {
    Argument::Literal(OsString::from(text))
}

#[test]
fn fixed_arguments_come_first_in_the_specified_order() {
    let items = [
        item(Directive::Hide, "/tmp", EntryKind::Directory),
        item(Directive::Rw, "/tmp/process-wrap", EntryKind::Directory),
        item(
            Directive::Ro,
            "/home/u/.codex/AGENTS.md",
            EntryKind::NotDirectory,
        ),
        item(
            Directive::RwFile,
            "/home/u/.claude.json",
            EntryKind::NotDirectory,
        ),
        item(
            Directive::Hide,
            "/home/u/proj/.env",
            EntryKind::NotDirectory,
        ),
    ];

    let arguments = bwrap_arguments(NetworkMode::Host, Path::new("/home/u/proj"), &items);

    assert_eq!(
        arguments,
        [
            literal("--ro-bind"),
            literal("/"),
            literal("/"),
            literal("--dev"),
            literal("/dev"),
            literal("--proc"),
            literal("/proc"),
            literal("--unshare-all"),
            literal("--share-net"),
            literal("--die-with-parent"),
            literal("--chdir"),
            literal("/home/u/proj"),
            literal("--seccomp"),
            Argument::Seccomp,
            literal("--tmpfs"),
            literal("/tmp"),
            literal("--bind"),
            literal("/tmp/process-wrap"),
            literal("/tmp/process-wrap"),
            literal("--ro-bind"),
            literal("/home/u/.codex/AGENTS.md"),
            literal("/home/u/.codex/AGENTS.md"),
            literal("--bind"),
            literal("/home/u/.claude.json"),
            literal("/home/u/.claude.json"),
            literal("--ro-bind-data"),
            Argument::EmptyFile,
            literal("/home/u/proj/.env"),
        ]
    );
}

#[test]
fn share_net_is_present_only_for_host_mode() {
    let host = bwrap_arguments(NetworkMode::Host, Path::new("/home/u/proj"), &[]);
    let none = bwrap_arguments(NetworkMode::None, Path::new("/home/u/proj"), &[]);

    assert!(host.contains(&literal("--share-net")));
    assert!(!none.contains(&literal("--share-net")));
    assert!(none.contains(&literal("--unshare-all")));
}

#[test]
fn the_argument_list_carries_no_environment_flags() {
    let items = [item(Directive::Rw, "/home/u/proj", EntryKind::Directory)];

    let arguments = bwrap_arguments(NetworkMode::Host, Path::new("/home/u/proj"), &items);

    for flag in ["--setenv", "--unsetenv", "--clearenv"] {
        assert!(!arguments.contains(&literal(flag)), "{flag}");
    }
}

/// The stage-seven checks on the fixture host: the profile `profile` with the policy
/// file at the fixture path, the facts, and the secret files given.
fn isolation(
    profile: &str,
    facts: Facts,
    secrets: &[(&str, SecretFile)],
) -> Result<Isolation, Diagnostic> {
    let layers = layers(profile, Some(""), &[], &[]);
    let policy = merged(&layers);
    let variables = variables();
    let home = home();
    let expanded = expand_policy(&policy, &variables, &home);
    let inputs = Inputs {
        layers: &layers,
        policy: &policy,
        expanded: &expanded,
        variables: &variables,
        home: &home,
        config_dir: Path::new(CONFIG_DIR),
        workspace: None,
        current_dir: Path::new(WORKTREE),
        host: &BTreeMap::new(),
    };
    let facts = IsolationFacts {
        mounts: facts.mount_facts(),
        secrets: secrets
            .iter()
            .map(|(name, file)| (name.to_string(), file.clone()))
            .collect(),
    };
    resolve_isolation(&inputs, &facts)
}

#[test]
fn stage_seven_checks_stop_at_the_first_diagnostic_in_the_specified_order() {
    let host = Facts::new()
        .dir_with_ancestors(WORKTREE)
        .file_with_ancestors(PROFILE)
        .file_with_ancestors(POLICY_FILE)
        .file("/home/u/tokens/s");
    let collision = "hide = [\"/home/u/link\"]\n";
    let policy_file_inside_rw = "\"/home/u/policies\", ";
    let empty_secret = [("S", SecretFile::Bytes(Vec::new()))];
    let profile = |collision: &str, inside: &str| {
        format!(
            "[mounts]\nrw = [{inside}\"${{worktree}}\"]\n{collision}\
             [secrets]\nS = \"/home/u/tokens/s\""
        )
    };
    let facts = host.clone().link_to_dir("/home/u/link", WORKTREE);

    let all_three = isolation(
        &profile(collision, policy_file_inside_rw),
        facts.clone(),
        &empty_secret,
    )
    .unwrap_err();
    assert_eq!(all_three.kind(), Kind::Policy, "{all_three}");

    let without_collision = isolation(
        &profile("", policy_file_inside_rw),
        facts.clone(),
        &empty_secret,
    )
    .unwrap_err();
    assert_eq!(without_collision.kind(), Kind::Path, "{without_collision}");

    let only_the_secret = isolation(&profile("", ""), facts, &empty_secret).unwrap_err();
    assert_eq!(only_the_secret.kind(), Kind::Secret, "{only_the_secret}");
}

/// A home for a binary test: an empty profile and a workspace directory under it.
fn home_with_workspace() -> (TempDir, PathBuf) {
    let home = TempDir::new();
    home.write(".config/process-wrap/profile/default.toml", "");
    let workspace = home.path().join("ws");
    std::fs::create_dir(&workspace).unwrap();
    (home, workspace)
}

#[test]
fn a_missing_bwrap_is_a_bwrap_diagnostic() {
    let (home, workspace) = home_with_workspace();
    let empty_path = TempDir::new();

    let output = binary(home.path())
        .env("PATH", empty_path.path())
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--",
            "/bin/true",
        ])
        .output()
        .unwrap();

    assert_diagnostic(&output, 125, "bwrap");

    // Stage 8 comes before stage 9: an unresolvable command does not change the answer.
    let with_a_missing_command = binary(home.path())
        .env("PATH", empty_path.path())
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--",
            "no-such-tool",
        ])
        .output()
        .unwrap();
    assert_diagnostic(&with_a_missing_command, 125, "bwrap");
}

/// A `PATH` holding only a stand-in `bwrap`, so that the whereabouts check passes
/// without the real one and nothing else is on it.
fn path_with_a_fake_bwrap() -> TempDir {
    let bin = TempDir::new();
    executable(&bin, "bwrap");
    bin
}

#[test]
fn print_plan_without_a_command_skips_resolution() {
    let (home, workspace) = home_with_workspace();
    let bin = path_with_a_fake_bwrap();
    let workspace = workspace.to_str().unwrap();

    let without = binary(home.path())
        .env("PATH", bin.path())
        .args(["--workspace", workspace, "--print-plan"])
        .output()
        .unwrap();
    assert_eq!(
        without.status.code(),
        Some(0),
        "{}",
        output_report(&without)
    );

    let with = binary(home.path())
        .env("PATH", bin.path())
        .args([
            "--workspace",
            workspace,
            "--print-plan",
            "--",
            "no-such-tool",
        ])
        .output()
        .unwrap();
    assert_diagnostic(&with, 127, "command not found");
}

/// A policy file at `real/pol/p.toml` reached through `policies -> cache/link/pol` and
/// `cache/link -> real`: no prefix of the given path resolves into `cache`, but the chain
/// passes through a link that sits inside it.
fn home_with_a_policy_file_behind_a_chain_of_links(profile: &str, policy_file: &str) -> TempDir {
    let (home, _) = home_with_workspace();
    home.write(".config/process-wrap/profile/default.toml", profile);
    home.write("real/pol/p.toml", policy_file);
    std::fs::create_dir(home.path().join("cache")).unwrap();
    std::os::unix::fs::symlink(home.path().join("real"), home.path().join("cache/link")).unwrap();
    std::os::unix::fs::symlink(
        home.path().join("cache/link/pol"),
        home.path().join("policies"),
    )
    .unwrap();
    home
}

#[test]
fn a_policy_file_behind_a_link_inside_a_writable_area_is_a_path_diagnostic() {
    let rw_cache = "[mounts]\nrw = [\"~/cache\"]";

    for (name, profile, policy_file) in [
        ("rw in the profile", rw_cache, ""),
        ("rw in the policy file itself", "", rw_cache),
    ] {
        let home = home_with_a_policy_file_behind_a_chain_of_links(profile, policy_file);
        let workspace = home.path().join("ws");

        let output = binary(home.path())
            .args([
                "--policy-file",
                home.path().join("policies/p.toml").to_str().unwrap(),
                "--workspace",
                workspace.to_str().unwrap(),
                "--",
                "true",
            ])
            .output()
            .unwrap();

        let diagnostic = assert_diagnostic(&output, 125, "path");
        let real_home = home.path().canonicalize().unwrap();
        for element in [real_home.join("cache/link"), real_home.join("cache")] {
            assert!(
                diagnostic.contains(element.to_str().unwrap()),
                "{name}: {diagnostic} does not mention {}",
                element.display()
            );
        }
    }
}

/// A policy file at `real/pol/p.toml` reached through `policies -> <through>/../../real/pol`,
/// with `<through>` a real directory two levels under the home and `cache` a directory an
/// `rw` can name: every prefix of the given path resolves outside `cache` and the only
/// link sits in the home, but the walk passes through `<through>` before stepping back.
fn home_with_a_policy_file_behind_a_link_stepping_back_through(
    through: &str,
    profile: &str,
    policy_file: &str,
) -> TempDir {
    let (home, _) = home_with_workspace();
    home.write(".config/process-wrap/profile/default.toml", profile);
    home.write("real/pol/p.toml", policy_file);
    std::fs::create_dir_all(home.path().join("cache")).unwrap();
    std::fs::create_dir_all(home.path().join(through)).unwrap();
    std::os::unix::fs::symlink(
        home.path().join(through).join("../../real/pol"),
        home.path().join("policies"),
    )
    .unwrap();
    home
}

#[test]
fn a_policy_file_behind_a_link_stepping_back_through_a_writable_area_is_a_path_diagnostic() {
    let rw_cache = "[mounts]\nrw = [\"~/cache\"]";

    for (name, profile, policy_file) in [
        ("rw in the profile", rw_cache, ""),
        ("rw in the policy file itself", "", rw_cache),
    ] {
        let home = home_with_a_policy_file_behind_a_link_stepping_back_through(
            "cache/x",
            profile,
            policy_file,
        );
        let workspace = home.path().join("ws");

        let output = binary(home.path())
            .args([
                "--policy-file",
                home.path().join("policies/p.toml").to_str().unwrap(),
                "--workspace",
                workspace.to_str().unwrap(),
                "--",
                "true",
            ])
            .output()
            .unwrap();

        let diagnostic = assert_diagnostic(&output, 125, "path");
        let real_home = home.path().canonicalize().unwrap();
        assert!(
            diagnostic.contains(real_home.join("cache").to_str().unwrap()),
            "{name}: {diagnostic} does not mention the writable area passed through"
        );
    }
}

#[test]
fn a_link_stepping_back_through_a_directory_outside_writable_areas_is_accepted() {
    let home = home_with_a_policy_file_behind_a_link_stepping_back_through(
        "other/x",
        "[mounts]\nrw = [\"~/cache\"]",
        "",
    );
    let workspace = home.path().join("ws");

    let output = binary(home.path())
        .args([
            "--policy-file",
            home.path().join("policies/p.toml").to_str().unwrap(),
            "--workspace",
            workspace.to_str().unwrap(),
            "--",
            "true",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0), "{}", output_report(&output));
}

/// A home with the profile `profile`, the directory `target` under it, and `cache/pip/http`
/// as a process inside the isolation could leave it: `cache/pip` a link to `cache/evil`,
/// and `cache/evil/http` a link to `target`. The next start resolves `~/cache/pip/http`
/// to `target` through `cache`.
fn home_with_a_rewired_cache(profile: &str, target: &str) -> TempDir {
    let (home, _) = home_with_workspace();
    home.write(".config/process-wrap/profile/default.toml", profile);
    std::fs::create_dir_all(home.path().join(target)).unwrap();
    std::fs::create_dir_all(home.path().join("cache/evil")).unwrap();
    std::os::unix::fs::symlink("evil", home.path().join("cache/pip")).unwrap();
    std::os::unix::fs::symlink(
        home.path().join(target),
        home.path().join("cache/evil/http"),
    )
    .unwrap();
    home
}

/// Runs the binary from `home` with `--workspace workspace` and the command `true`.
fn run_with_workspace(home: &TempDir, workspace: &Path) -> std::process::Output {
    binary(home.path())
        .args(["--workspace", workspace.to_str().unwrap(), "--", "true"])
        .output()
        .unwrap()
}

const NESTED_RW: &str = "[mounts]\nrw = [\"~/cache\", \"~/cache/pip/http\"]";

#[test]
fn a_nested_item_rewired_to_outside_every_writable_item_is_a_path_diagnostic() {
    let home = home_with_a_rewired_cache(NESTED_RW, "victim");
    let workspace = home.path().join("ws");

    let output = run_with_workspace(&home, &workspace);

    let diagnostic = assert_diagnostic(&output, 125, "path");
    let real_home = home.path().canonicalize().unwrap();
    for element in [real_home.join("cache/pip/http"), real_home.join("cache")] {
        assert!(
            diagnostic.contains(element.to_str().unwrap()),
            "{diagnostic} does not mention {}",
            element.display()
        );
    }
}

#[test]
fn a_nested_item_that_is_a_real_directory_is_accepted() {
    let (home, workspace) = home_with_workspace();
    home.write(".config/process-wrap/profile/default.toml", NESTED_RW);
    std::fs::create_dir_all(home.path().join("cache/pip/http")).unwrap();

    let output = run_with_workspace(&home, &workspace);

    assert_eq!(output.status.code(), Some(0), "{}", output_report(&output));
}

#[test]
fn a_nested_item_rewired_into_another_writable_item_is_accepted() {
    let home = home_with_a_rewired_cache(
        "[mounts]\nrw = [\"~/cache\", \"~/cache/pip/http\", \"~/other\"]",
        "other/http",
    );
    let workspace = home.path().join("ws");

    let output = run_with_workspace(&home, &workspace);

    assert_eq!(output.status.code(), Some(0), "{}", output_report(&output));
}

#[test]
fn a_workspace_rewired_to_outside_every_writable_item_is_a_path_diagnostic() {
    let (home, _) = home_with_workspace();
    home.write(
        ".config/process-wrap/profile/default.toml",
        "[mounts]\nrw = [\"~/cache\"]",
    );
    let victim = home.path().join("victim");
    std::fs::create_dir(&victim).unwrap();
    std::fs::create_dir(home.path().join("cache")).unwrap();
    std::os::unix::fs::symlink(&victim, home.path().join("cache/ws")).unwrap();

    let output = run_with_workspace(&home, &home.path().join("cache/ws"));

    let diagnostic = assert_diagnostic(&output, 125, "path");
    let real_home = home.path().canonicalize().unwrap();
    assert!(
        diagnostic.contains(real_home.join("cache").to_str().unwrap()),
        "{diagnostic} does not mention the writable area passed through"
    );
}

const RW_WORKTREE_AND_CACHE: &str = "[mounts]\nrw = [\"${worktree}\", \"~/cache\"]";

/// A home with `cache/proj` as a workspace under an `rw` cache, and `victim` beside it.
fn home_with_a_project_under_the_cache(profile: &str) -> TempDir {
    let home = TempDir::new();
    home.write(".config/process-wrap/profile/default.toml", profile);
    std::fs::create_dir_all(home.path().join("cache/proj")).unwrap();
    std::fs::create_dir(home.path().join("victim")).unwrap();
    home
}

/// `cache/proj` as a process inside the isolation could leave it: a link to `victim`.
fn rewire_the_project_to_the_victim(home: &TempDir) {
    std::fs::remove_dir(home.path().join("cache/proj")).unwrap();
    std::os::unix::fs::symlink(home.path().join("victim"), home.path().join("cache/proj")).unwrap();
}

#[test]
fn a_workspace_under_an_rw_cache_given_from_elsewhere_is_accepted_until_rewired() {
    let home = home_with_a_project_under_the_cache(RW_WORKTREE_AND_CACHE);
    let workspace = home.path().join("cache/proj");

    let honest = run_with_workspace(&home, &workspace);
    assert_eq!(honest.status.code(), Some(0), "{}", output_report(&honest));

    rewire_the_project_to_the_victim(&home);
    let rewired = run_with_workspace(&home, &workspace);
    let diagnostic = assert_diagnostic(&rewired, 125, "path");
    let real_home = home.path().canonicalize().unwrap();
    for element in [real_home.join("victim"), real_home.join("cache")] {
        assert!(
            diagnostic.contains(element.to_str().unwrap()),
            "{diagnostic} does not mention {}",
            element.display()
        );
    }
}

#[test]
fn a_workspace_under_an_rw_worktree_is_accepted_only_from_inside_it() {
    let home = TempDir::new();
    home.write(
        ".config/process-wrap/profile/default.toml",
        "[mounts]\nrw = [\"${worktree}\"]",
    );
    std::fs::create_dir_all(home.path().join("proj/.git")).unwrap();
    std::fs::create_dir_all(home.path().join("proj/sub")).unwrap();
    std::fs::create_dir(home.path().join("elsewhere")).unwrap();
    let sub = home.path().join("proj/sub").canonicalize().unwrap();

    let from_inside = binary(home.path())
        .current_dir(&sub)
        .args(["--workspace", sub.to_str().unwrap(), "--", "true"])
        .output()
        .unwrap();
    assert_eq!(
        from_inside.status.code(),
        Some(0),
        "{}",
        output_report(&from_inside)
    );

    let from_elsewhere = binary(home.path())
        .current_dir(home.path().join("elsewhere"))
        .args(["--workspace", sub.to_str().unwrap(), "--", "true"])
        .output()
        .unwrap();
    assert_diagnostic(&from_elsewhere, 125, "path");
}

#[test]
fn a_workspace_under_an_rw_worktree_rewired_to_elsewhere_is_a_path_diagnostic_naming_the_link() {
    // After `proj/sub` is replaced by a link to `victim`, `--workspace proj/sub` from
    // elsewhere derives the worktree as `victim`, so `proj` is writable no more and the
    // resolution referenced nothing writable; only the link it followed gives it away.
    for (name, with_git) in [("with .git", true), ("without .git", false)] {
        let home = TempDir::new();
        home.write(
            ".config/process-wrap/profile/default.toml",
            "[mounts]\nrw = [\"${worktree}\"]",
        );
        std::fs::create_dir_all(home.path().join("proj/sub")).unwrap();
        if with_git {
            std::fs::create_dir(home.path().join("proj/.git")).unwrap();
        }
        std::fs::create_dir(home.path().join("elsewhere")).unwrap();
        std::fs::create_dir(home.path().join("victim")).unwrap();
        std::fs::remove_dir(home.path().join("proj/sub")).unwrap();
        std::os::unix::fs::symlink(home.path().join("victim"), home.path().join("proj/sub"))
            .unwrap();
        let real_home = home.path().canonicalize().unwrap();
        let sub = real_home.join("proj/sub");

        let output = binary(home.path())
            .current_dir(home.path().join("elsewhere"))
            .args(["--workspace", sub.to_str().unwrap(), "--", "true"])
            .output()
            .unwrap();

        let diagnostic = assert_diagnostic(&output, 125, "path");
        assert!(
            diagnostic.contains(sub.to_str().unwrap()),
            "{name}: {diagnostic} does not mention the link followed"
        );
    }
}

#[test]
fn a_workspace_behind_a_link_landing_in_an_rw_item_written_by_path_is_accepted() {
    let home = TempDir::new();
    home.write(
        ".config/process-wrap/profile/default.toml",
        "[mounts]\nrw = [\"~/work\"]",
    );
    std::fs::create_dir_all(home.path().join("data/work/proj")).unwrap();
    std::fs::create_dir(home.path().join("elsewhere")).unwrap();
    std::os::unix::fs::symlink(home.path().join("data/work"), home.path().join("work")).unwrap();

    let output = binary(home.path())
        .current_dir(home.path().join("elsewhere"))
        .args([
            "--workspace",
            home.path().join("work/proj").to_str().unwrap(),
            "--",
            "true",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0), "{}", output_report(&output));
}

#[test]
fn an_item_reached_through_a_link_between_two_rw_items_is_accepted_until_the_link_moves() {
    // `rw = ["~/a", "~/b"]` with `a/l -> b/x`: `ro ~/a/l/y` resolves through `a` and lands
    // in `b`; re-pointing `l` at a third place lands it outside every root.
    let (home, workspace) = home_with_workspace();
    home.write(
        ".config/process-wrap/profile/default.toml",
        "[mounts]\nrw = [\"~/a\", \"~/b\"]\nro = [\"~/a/l/y\"]",
    );
    std::fs::create_dir_all(home.path().join("a")).unwrap();
    std::fs::create_dir_all(home.path().join("b/x/y")).unwrap();
    std::fs::create_dir_all(home.path().join("elsewhere/x/y")).unwrap();
    std::os::unix::fs::symlink(home.path().join("b/x"), home.path().join("a/l")).unwrap();

    let honest = run_with_workspace(&home, &workspace);
    assert_eq!(honest.status.code(), Some(0), "{}", output_report(&honest));

    std::fs::remove_file(home.path().join("a/l")).unwrap();
    std::os::unix::fs::symlink(home.path().join("elsewhere/x"), home.path().join("a/l")).unwrap();
    let moved = run_with_workspace(&home, &workspace);
    let diagnostic = assert_diagnostic(&moved, 125, "path");
    let real_home = home.path().canonicalize().unwrap();
    assert!(
        diagnostic.contains(real_home.join("a").to_str().unwrap()),
        "{diagnostic} does not mention the writable area passed through"
    );
}

#[test]
fn a_workspace_resolved_only_through_places_that_are_no_item_is_accepted() {
    let home = home_with_a_project_under_the_cache("[mounts]\nrw = [\"${worktree}\"]");

    let output = run_with_workspace(&home, &home.path().join("cache/proj"));

    assert_eq!(output.status.code(), Some(0), "{}", output_report(&output));
}

#[test]
fn a_hide_or_rw_file_nested_under_an_rw_item_is_accepted_until_rewired() {
    for (name, directive) in [("hide", "hide"), ("rw-file", "rw-file")] {
        let (home, workspace) = home_with_workspace();
        home.write(
            ".config/process-wrap/profile/default.toml",
            format!("[mounts]\nrw = [\"~/cache\"]\n{directive} = [\"~/cache/x/state\"]"),
        );
        home.write("cache/x/state", "");
        home.write("elsewhere/state", "");

        let honest = run_with_workspace(&home, &workspace);
        assert_eq!(
            honest.status.code(),
            Some(0),
            "{name}: {}",
            output_report(&honest)
        );

        std::fs::remove_dir_all(home.path().join("cache/x")).unwrap();
        std::os::unix::fs::symlink(home.path().join("elsewhere"), home.path().join("cache/x"))
            .unwrap();
        let rewired = run_with_workspace(&home, &workspace);
        let diagnostic = assert_diagnostic(&rewired, 125, "path");
        let real_home = home.path().canonicalize().unwrap();
        for element in [real_home.join("cache/x/state"), real_home.join("cache")] {
            assert!(
                diagnostic.contains(element.to_str().unwrap()),
                "{name}: {diagnostic} does not mention {}",
                element.display()
            );
        }
    }
}

#[test]
fn a_command_line_collision_beside_a_home_workspace_is_a_usage_diagnostic() {
    let (home, _) = home_with_workspace();
    let collision = home.path().join("x");

    let output = binary(home.path())
        .args([
            "--workspace",
            home.path().to_str().unwrap(),
            "--rw",
            collision.to_str().unwrap(),
            "--hide",
            collision.to_str().unwrap(),
            "--",
            "/bin/true",
        ])
        .output()
        .unwrap();

    assert_diagnostic(&output, 125, "usage");

    let home_alone = binary(home.path())
        .args([
            "--workspace",
            home.path().to_str().unwrap(),
            "--",
            "/bin/true",
        ])
        .output()
        .unwrap();
    assert_diagnostic(&home_alone, 125, "path");
}
