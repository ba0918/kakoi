use std::path::Path;
use std::process::Command;

mod common;
use common::{output_report, TempDir};

fn consumer() -> std::path::PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target = root.join("target/library-sync-consumer");
    let output = Command::new(env!("CARGO"))
        .args(["build", "--locked", "--manifest-path"])
        .arg(root.join("examples/library-sync/Cargo.toml"))
        .env("CARGO_TARGET_DIR", &target)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
    target.join("debug/kakoi-library-sync-example")
}

// @kotowari[REQ-library-104]
#[test]
fn explicit_context_preserves_os_bytes_and_configuration_uses_its_paths() {
    let dir = TempDir::new();
    dir.write(
        "home/.config/kakoi/profile/explicit.toml",
        "[network]\nmode='host'\n",
    );
    dir.write("policy.toml", "[network]\nmode='none'\n");
    let output = Command::new(consumer())
        .arg("--self-test-input")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home"))
        .env("XDG_CONFIG_HOME", dir.path().join("home/.config"))
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
}
