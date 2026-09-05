//! What the built binary does with the real bwrap (specification section 15.2): the rows
//! of the tables of section 13, and what is seen from inside the isolation.

use std::path::PathBuf;

mod common;

use common::{binary, output_report, TempDir};

/// The profile of a launch that raises no warning: the workspace is `rw`.
const RW_WORKSPACE: &str = "[mounts]\nrw = [\"${workspace}\"]\n";

/// A home with the `RW_WORKSPACE` profile and a workspace directory under it.
fn home_with_workspace() -> (TempDir, PathBuf) {
    let home = TempDir::new();
    home.write(".config/process-wrap/profile/default.toml", RW_WORKSPACE);
    let workspace = home.path().join("ws");
    std::fs::create_dir(&workspace).unwrap();
    (home, workspace)
}

#[test]
fn a_command_exit_code_passes_through() {
    let (home, workspace) = home_with_workspace();

    for code in [0, 3, 42] {
        let output = binary(home.path())
            .args([
                "--workspace",
                workspace.to_str().unwrap(),
                "--",
                "/bin/sh",
                "-c",
                &format!("echo out; echo err >&2; exit {code}"),
            ])
            .output()
            .unwrap();

        let report = output_report(&output);
        assert_eq!(output.status.code(), Some(code), "{report}");
        assert_eq!(output.stdout, b"out\n", "{report}");
        assert_eq!(output.stderr, b"err\n", "{report}");
    }
}
