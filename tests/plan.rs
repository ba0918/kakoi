use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

mod common;

use common::TempDir;
use process_wrap::command::{command_candidates, resolve_command};
use process_wrap::diagnostic::Kind;
use process_wrap::executables::first_executable;

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
