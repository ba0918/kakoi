//! Nesting: the mark an isolation carries, how kakoi tells a nested run by it, and what a
//! kakoi started inside an isolation of the built kakoi does.

use std::path::Path;
use std::process::Output;

mod common;

use common::{binary, home_with_workspace, output_report, TempDir, RW_WORKSPACE};

/// The name of the nesting mark inside an isolation.
const MARK: &str = "/dev/kakoi-isolated";

/// Overwrites the `default` profile of `home` with `text`.
fn profile(home: &TempDir, text: &str) {
    home.write(".config/kakoi/profile/default.toml", text);
}

/// Runs `script` with `/bin/sh -c` inside the isolation of `home`'s profile, with the
/// workspace at `workspace`, which is also the current directory.
fn run_script(home: &TempDir, workspace: &Path, script: &str) -> Output {
    binary(home.path())
        .current_dir(workspace)
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--",
            "/bin/sh",
            "-c",
            script,
        ])
        .output()
        .unwrap()
}

// @kotowari[REQ-455]
#[test]
fn req_455_the_mark_is_a_regular_file_the_isolation_cannot_write_or_remove() {
    for mode in ["host", "none"] {
        let (home, workspace) = home_with_workspace();
        profile(
            &home,
            &format!("{RW_WORKSPACE}[network]\nmode = \"{mode}\"\n"),
        );

        let output = run_script(
            &home,
            &workspace,
            &format!(
                "test -f {MARK} && test ! -L {MARK} && echo regular; \
                 (echo x > {MARK}) 2>/dev/null || echo not-writable; \
                 rm -f {MARK} 2>/dev/null || echo not-removable; \
                 mv {MARK} /dev/moved 2>/dev/null || echo not-movable; \
                 test -f {MARK} && test ! -s {MARK} && echo still-there"
            ),
        );

        let report = output_report(&output);
        assert_eq!(output.status.code(), Some(0), "{mode}: {report}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "regular\nnot-writable\nnot-removable\nnot-movable\nstill-there\n",
            "{mode}: {report}"
        );
    }
}

// @kotowari[REQ-315]
#[test]
fn req_315_the_mark_follows_the_dev_argument_in_the_plan() {
    let (home, workspace) = home_with_workspace();

    let output = binary(home.path())
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--print-plan=json",
            "--",
            "/bin/true",
        ])
        .output()
        .unwrap();

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let arguments = plan["bwrap_arguments"].as_array().unwrap();
    let dev = arguments
        .iter()
        .position(|argument| argument["value"] == "--dev")
        .unwrap_or_else(|| panic!("no --dev: {report}"));
    assert_eq!(arguments[dev + 1]["value"], "/dev", "{report}");
    assert_eq!(arguments[dev + 2]["value"], "--ro-bind-data", "{report}");
    assert_eq!(arguments[dev + 3]["kind"], "empty-file", "{report}");
    assert_eq!(arguments[dev + 4]["value"], MARK, "{report}");
}
