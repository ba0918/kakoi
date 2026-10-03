use std::path::{Path, PathBuf};
use std::process::Command;

use kakoi_policy::{
    EnvMode, EnvironmentPolicy, ListMode, MountPolicy, NetworkMode, NetworkPolicy, Policy,
    PolicyInput, Scan,
};

fn input() -> PolicyInput {
    PolicyInput::new(
        MountPolicy::new(ListMode::Host),
        NetworkPolicy::new(NetworkMode::None),
        EnvironmentPolicy::new(EnvMode::Clear),
    )
}

// @kotowari[REQ-library-102, EX-library-103]
#[test]
fn ex_library_103_constructs_policy_without_toml_or_files() {
    let policy = Policy::validate(input()).unwrap();
    assert_eq!(policy.mounts_mode(), ListMode::Host);
    assert_eq!(policy.network_mode(), NetworkMode::None);
    assert_eq!(policy.environment_mode(), EnvMode::Clear);
    assert_eq!(policy.commands_mode(), ListMode::Host);
}

// @kotowari[REQ-library-102, EX-library-104]
#[test]
fn ex_library_104_rejects_the_same_invalid_conditions_at_both_entrances() {
    let mut rust = input();
    rust.process.shutdown_grace_seconds = Some(0);
    assert!(Policy::validate(rust).is_err());
    assert!(Policy::from_toml("[process]\nshutdown-grace-seconds=0").is_err());

    let mut rust = input();
    rust.mounts.scan.push(Scan {
        root: PathBuf::from("/work").into(),
        names: vec![],
        exclude: vec![],
        prune: vec![],
    });
    assert!(Policy::validate(rust).is_err());
    assert!(Policy::from_toml("[[mounts.scan]]\nroot='/work'\nnames=[]").is_err());

    let mut rust = input();
    rust.environment.set.insert("KEY".into(), "value".into());
    rust.secrets
        .insert("KEY".into(), PathBuf::from("/secret").into());
    assert!(Policy::validate(rust).is_err());
    assert!(Policy::from_toml("[env.set]\nKEY='value'\n[secrets]\nKEY='/secret'").is_err());
}

// @kotowari[REQ-library-102, EX-library-104]
#[test]
fn ex_library_104_rejects_invalid_paths_and_link_local_destinations_from_rust() {
    let mut rust = input();
    rust.mounts.ro.push(PathBuf::from("relative").into());
    assert!(Policy::validate(rust).is_err());
    assert!(Policy::from_toml("[mounts]\nro=['relative']").is_err());

    let mut rust = input();
    rust.network.allow.push(kakoi_policy::Allow {
        destination: kakoi_policy::Destination::Address {
            network: "fe80::/64".parse().unwrap(),
            host_interface: None,
        },
        protocol: kakoi_policy::Protocol::Tcp,
        ports: vec!["443".into()].try_into().unwrap(),
    });
    assert!(Policy::validate(rust).is_err());
    assert!(Policy::from_toml(
        "[[network.allow]]\ndestination={cidr='fe80::/64'}\nprotocol='tcp'\nports=['443']"
    )
    .is_err());
}

// @kotowari[REQ-library-103, EX-library-106]
#[test]
fn ex_library_106_preserves_toml_mode_defaults() {
    let policy = Policy::from_toml("").unwrap();
    assert_eq!(policy.mounts_mode(), ListMode::Host);
    assert_eq!(policy.network_mode(), NetworkMode::Host);
    assert_eq!(policy.environment_mode(), EnvMode::Inherit);
    assert_eq!(policy.commands_mode(), ListMode::Host);
}

fn compile_fixture(name: &str, source: &str) -> std::process::Output {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("target/library-policy-compile").join(name);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let dependency = root.join("crates/kakoi-policy");
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[workspace]\n[package]\nname='consumer-{name}'\nversion='0.0.0'\nedition='2021'\n[dependencies]\nkakoi-policy={{path={:?}}}\n",
            dependency
        ),
    ).unwrap();
    std::fs::write(dir.join("src/main.rs"), source).unwrap();
    Command::new(env!("CARGO"))
        .args(["check", "--offline", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .env(
            "CARGO_TARGET_DIR",
            root.join("target/library-policy-consumer"),
        )
        .output()
        .unwrap()
}

// @kotowari[REQ-library-103, EX-library-105]
#[test]
fn ex_library_105_requires_all_three_modes_at_compile_time() {
    let positive = compile_fixture(
        "valid",
        include_str!("fixtures/library-api/policy-valid.rs"),
    );
    assert!(
        positive.status.success(),
        "{}",
        String::from_utf8_lossy(&positive.stderr)
    );
    for (name, source) in [
        (
            "mount",
            include_str!("fixtures/library-api/policy-missing-mount-mode.rs"),
        ),
        (
            "network",
            include_str!("fixtures/library-api/policy-missing-network-mode.rs"),
        ),
        (
            "env",
            include_str!("fixtures/library-api/policy-missing-env-mode.rs"),
        ),
    ] {
        let output = compile_fixture(name, source);
        assert!(!output.status.success(), "{name} unexpectedly compiled");
        assert!(String::from_utf8_lossy(&output.stderr).contains("E0061"));
    }
}

// @kotowari[REQ-library-401, EX-library-402]
#[test]
fn ex_library_402_external_consumer_depends_only_on_policy() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new(env!("CARGO"))
        .args([
            "run",
            "--locked",
            "--manifest-path",
            "examples/library-policy/Cargo.toml",
            "--",
            "--self-test",
        ])
        .current_dir(root)
        .env(
            "CARGO_TARGET_DIR",
            root.join("target/library-policy-example"),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
