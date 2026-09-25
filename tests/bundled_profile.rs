//! The bundled profile `examples/profile/default.toml` (specification section 16), read
//! by the built binary as the `default` profile of a temporary home.

mod common;

use common::{binary, output_report, TempDir};

// @kotowari[REQ-151]
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

// @kotowari[REQ-283]
#[test]
fn the_bundled_profile_cuts_the_display_server_sockets_and_variables() {
    let home = TempDir::new();
    let bundled = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/profile/default.toml"
    ))
    .unwrap();
    home.write(".config/kakoi/profile/default.toml", bundled);
    let workspace = home.path().join("ws");
    std::fs::create_dir(&workspace).unwrap();
    // A stand-in for the X11 socket directory under /tmp, where the display server keeps
    // its sockets.
    let display = TempDir::under(std::path::Path::new("/tmp"));
    let socket = display.path().join("X0");
    let _listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let variables = ["DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY"];

    let mut command = binary(home.path());
    for name in variables {
        command.env(name, "from-the-host");
    }
    let output = command
        .current_dir(&workspace)
        .args([
            "--",
            "/bin/sh",
            "-c",
            &format!(
                "test -e {} && echo socket-visible; /usr/bin/env",
                socket.display()
            ),
        ])
        .output()
        .unwrap();

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains("socket-visible"), "{report}");
    for name in variables {
        assert!(
            !stdout
                .lines()
                .any(|line| line.starts_with(&format!("{name}="))),
            "{name}: {report}"
        );
    }

    let plan = binary(home.path())
        .current_dir(&workspace)
        .args(["--print-plan=json"])
        .output()
        .unwrap();
    let report = output_report(&plan);
    assert_eq!(plan.status.code(), Some(0), "{report}");
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    // The Wayland socket lives under /run/user, hidden like /tmp.
    assert!(
        plan["policy"]["mounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["directive"] == "hide" && entry["path"] == "/run/user"),
        "{report}"
    );
}
