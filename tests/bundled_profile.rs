//! The bundled profile `examples/profile/default.toml` (specification section 16), read
//! by the built binary as the `default` profile of a temporary home.

mod common;

use common::{binary, output_report, TempDir};

#[test]
fn the_bundled_default_profile_loads_and_passes_the_placement_checks() {
    let home = TempDir::new();
    let bundled = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/profile/default.toml"
    ))
    .unwrap();
    home.write(".config/kakoi/profile/default.toml", bundled);
    let workspace = home.path().join("ws");
    std::fs::create_dir(&workspace).unwrap();

    let output = binary(home.path())
        .current_dir(&workspace)
        .args(["--print-plan", "--", "/bin/true"])
        .output()
        .unwrap();

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr
            .lines()
            .all(|line| line.starts_with("kakoi: warning: ")),
        "{report}"
    );
    let plan = String::from_utf8(output.stdout).unwrap();
    assert!(plan.contains(workspace.to_str().unwrap()), "{report}");
}
