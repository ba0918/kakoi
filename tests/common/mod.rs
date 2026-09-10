#![allow(dead_code)]

pub mod fixture;

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// The built binary, for tests that also set arguments piecewise, the environment, or the
/// working directory. The environment is emptied except for the test process's `PATH`, so
/// nothing of the developer's environment reaches the binary, the isolation, or the message
/// of a failed assertion (which quotes the plan in full). `HOME` and `XDG_CONFIG_HOME` point
/// into `home` so that no test reads the developer's real configuration directory.
pub fn binary(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kakoi"));
    command.env_clear();
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"));
    command
}

/// Runs the built binary with `arguments` from `home` and waits for it.
pub fn run<I, S>(home: &Path, arguments: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    binary(home).args(arguments).output().unwrap()
}

/// Runs the built binary with `arguments` from a directory that no longer exists.
pub fn run_from_deleted_dir<I, S>(home: &Path, arguments: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    run_command_from_deleted_dir(binary(home), home, arguments)
}

/// Runs `command` (the built binary, with its environment as set) with `arguments` from a
/// directory that no longer exists: a child shell enters a fresh directory under `home`,
/// removes it, and then executes the binary. `Command::current_dir` cannot do this (the
/// spawn fails), and changing the test process's own directory would race with the other
/// tests.
pub fn run_command_from_deleted_dir<I, S>(command: Command, home: &Path, arguments: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let doomed = home.join("doomed");
    fs::create_dir(&doomed).unwrap();
    let program = command.get_program().to_os_string();
    Command::new("sh")
        .arg("-c")
        .arg("doomed=\"$1\"; shift; cd \"$doomed\" && rmdir \"$doomed\" && exec \"$0\" \"$@\"")
        .arg(program)
        .arg(&doomed)
        .args(arguments)
        .env_clear()
        .envs(
            command
                .get_envs()
                .filter_map(|(key, value)| value.map(|value| (key, value))),
        )
        .output()
        .unwrap()
}

/// Runs `command` (the built binary, with its environment as set) with `arguments` from a
/// child shell whose soft limit on open files is lowered to `limit` first. The test
/// process's own limit is left alone: lowering it would race with the other tests.
pub fn run_command_with_soft_fd_limit<I, S>(command: Command, limit: u32, arguments: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let program = command.get_program().to_os_string();
    // By absolute path: the child's `PATH` is whatever the test gave the binary.
    let mut shell = Command::new("/bin/sh");
    shell
        .arg("-c")
        .arg("limit=\"$1\"; shift; ulimit -Sn \"$limit\" && exec \"$0\" \"$@\"")
        .arg(program)
        .arg(limit.to_string())
        .args(arguments)
        .env_clear()
        .envs(
            command
                .get_envs()
                .filter_map(|(key, value)| value.map(|value| (key, value))),
        );
    if let Some(dir) = command.get_current_dir() {
        shell.current_dir(dir);
    }
    shell.output().unwrap()
}

/// Everything the binary left behind, for the message of a failed assertion.
pub fn output_report(output: &Output) -> String {
    format!(
        "exit code {:?}, stdout {:?}, stderr {:?}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    )
}

/// Exit `code`, nothing on standard output, and one diagnostic line on standard error of the
/// form `kakoi: <kind>: <description>`. Returns the line so a test can check the
/// description.
pub fn assert_diagnostic(output: &Output, code: i32, kind: &str) -> String {
    let report = output_report(output);
    assert_eq!(output.status.code(), Some(code), "{report}");
    assert!(output.stdout.is_empty(), "{report}");
    let diagnostic = String::from_utf8(output.stderr.clone()).unwrap();
    assert_eq!(diagnostic.matches('\n').count(), 1, "{report}");
    assert!(diagnostic.ends_with('\n'), "{report}");
    assert!(
        diagnostic.starts_with(&format!("kakoi: {kind}: ")),
        "{report}"
    );
    diagnostic
}

/// The profile of a binary test that reaches the plan without a warning: the workspace is
/// `rw` (specification section 6.5).
pub const RW_WORKSPACE: &str = "[mounts]\nrw = [\"${workspace}\"]\n";

/// A home for a binary test that reaches the plan: the `RW_WORKSPACE` profile and a
/// workspace directory `ws` under it.
pub fn home_with_workspace() -> (TempDir, PathBuf) {
    let home = TempDir::new();
    home.write(".config/kakoi/profile/default.toml", RW_WORKSPACE);
    let workspace = home.path().join("ws");
    fs::create_dir(&workspace).unwrap();
    (home, workspace)
}

/// A fresh directory, removed when dropped.
pub struct TempDir(PathBuf);

impl TempDir {
    /// A directory under the system temporary directory (`TMPDIR` when set).
    pub fn new() -> Self {
        Self::under(&std::env::temp_dir())
    }

    /// A directory directly under `parent`, for a test whose scene names the parent.
    pub fn under(parent: &Path) -> Self {
        let path = parent.join(format!(
            "kakoi-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Writes `body` to `relative` under the directory, creating parent directories.
    pub fn write(&self, relative: impl AsRef<Path>, body: impl AsRef<[u8]>) -> PathBuf {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, body).unwrap();
        path
    }
}

impl Default for TempDir {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
