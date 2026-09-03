#![allow(dead_code)]

pub mod fixture;

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// The built binary, for tests that also set arguments piecewise, the environment, or the
/// working directory. `HOME` and `XDG_CONFIG_HOME` point into `home` so that no test reads
/// the developer's real configuration directory.
pub fn binary(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_process-wrap"));
    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env_remove("PROCESS_WRAP");
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

/// Runs the built binary with `arguments` from a directory that no longer exists: a child
/// shell enters a fresh directory under `home`, removes it, and then executes the binary.
/// `Command::current_dir` cannot do this (the spawn fails), and changing the test process's
/// own directory would race with the other tests.
pub fn run_from_deleted_dir<I, S>(home: &Path, arguments: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let doomed = home.join("doomed");
    fs::create_dir(&doomed).unwrap();
    let command = binary(home);
    let program = command.get_program().to_os_string();
    Command::new("sh")
        .arg("-c")
        .arg("doomed=\"$1\"; shift; cd \"$doomed\" && rmdir \"$doomed\" && exec \"$0\" \"$@\"")
        .arg(program)
        .arg(&doomed)
        .args(arguments)
        .envs(
            command
                .get_envs()
                .filter_map(|(key, value)| value.map(|value| (key, value))),
        )
        .env_remove("PROCESS_WRAP")
        .output()
        .unwrap()
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
/// form `process-wrap: <kind>: <description>`. Returns the line so a test can check the
/// description.
pub fn assert_diagnostic(output: &Output, code: i32, kind: &str) -> String {
    let report = output_report(output);
    assert_eq!(output.status.code(), Some(code), "{report}");
    assert!(output.stdout.is_empty(), "{report}");
    let diagnostic = String::from_utf8(output.stderr.clone()).unwrap();
    assert_eq!(diagnostic.matches('\n').count(), 1, "{report}");
    assert!(diagnostic.ends_with('\n'), "{report}");
    assert!(
        diagnostic.starts_with(&format!("process-wrap: {kind}: ")),
        "{report}"
    );
    diagnostic
}

/// A directory under the system temporary directory, removed when dropped.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "process-wrap-{}-{}",
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
