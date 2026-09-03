use std::ffi::{OsStr, OsString};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

mod common;

use common::TempDir;
use process_wrap::command::{command_candidates, resolve_command};
use process_wrap::diagnostic::Kind;
use process_wrap::executables::first_executable;
use process_wrap::layers::{Directive, LayerOrigin};
use process_wrap::mounts::{EntryKind, ItemOrigin, ResolvedItem};
use process_wrap::plan::{bwrap_arguments, Argument};
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
