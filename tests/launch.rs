//! What the built binary does with the real bwrap (specification section 15.2): the rows
//! of the tables of section 13, and what is seen from inside the isolation.

mod common;

use common::{binary, home_with_workspace, output_report};

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
