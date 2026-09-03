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
