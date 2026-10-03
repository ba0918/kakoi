use std::path::Path;
use std::process::Command;

mod common;
#[path = "library_api/retained_mounts.rs"]
mod retained_mounts;
use common::{output_report, TempDir};

// @kotowari[REQ-library-104]
#[test]
fn command_names_use_the_effective_path_while_bwrap_uses_the_host_path() {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("home/.config/kakoi/profile/default.toml", "invalid profile");
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    dir.write_executable("workspace/requested-tools/tool", "#!/bin/sh\nexit 0\n");
    dir.write_executable("workspace/policy-tools/tool", "#!/bin/sh\nexit 1\n");
    dir.write_executable("workspace/prepended-tools/tool", "#!/bin/sh\nexit 2\n");
    let output = Command::new(consumer())
        .arg("--self-test-command-path")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home"))
        .current_dir(dir.path().join("workspace"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
}

// @kotowari[REQ-446]
#[test]
fn named_command_follows_the_guard_placed_on_the_isolated_path() {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("home/.config/kakoi/profile/default.toml", "invalid profile");
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    dir.write_executable("workspace/requested-tools/tool", "#!/bin/sh\nexit 0\n");
    dir.write_executable("workspace/policy-tools/tool", "#!/bin/sh\nexit 1\n");
    let output = Command::new(consumer())
        .arg("--self-test-command-guard-path")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home"))
        .current_dir(dir.path().join("workspace"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
}

fn consumer() -> std::path::PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target = root.join("target/library-sync-consumer");
    let mut command = Command::new(env!("CARGO"));
    if cfg!(target_env = "musl") {
        command.args(["--config", "build.target='x86_64-unknown-linux-musl'"]);
    }
    let output = command
        .args(["build", "--locked", "--manifest-path"])
        .arg(root.join("examples/library-sync/Cargo.toml"))
        .env("CARGO_TARGET_DIR", &target)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
    let target = if cfg!(target_env = "musl") {
        target.join("x86_64-unknown-linux-musl")
    } else {
        target
    };
    target.join("debug/kakoi-library-sync-example")
}

// @kotowari[REQ-library-403]
#[test]
fn preparation_reaps_a_worker_killed_or_disconnected_before_the_handshake() {
    use std::time::{Duration, Instant};
    let executable = consumer();
    for fault in ["stop", "close"] {
        let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
        dir.write("home/.config/kakoi/profile/default.toml", "invalid profile");
        dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
        let mut child = Command::new(&executable)
            .arg("--self-test-prepare-fault")
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", dir.path().join("home"))
            .env("KAKOI_TEST_PREPARE_FAULT", fault)
            .current_dir(dir.path().join("workspace"))
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        if fault == "stop" {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                assert!(
                    child.try_wait().unwrap().is_none(),
                    "consumer exited before fault"
                );
                let children = std::fs::read_to_string(format!(
                    "/proc/{}/task/{}/children",
                    child.id(),
                    child.id()
                ))
                .unwrap();
                if let Some(worker) = children.split_whitespace().next() {
                    let status = std::fs::read_to_string(format!("/proc/{worker}/status")).unwrap();
                    if status.lines().any(|line| line.starts_with("State:\tT")) {
                        assert_eq!(
                            unsafe { libc::kill(worker.parse().unwrap(), libc::SIGKILL) },
                            0
                        );
                        break;
                    }
                }
                if Instant::now() >= deadline {
                    child.kill().unwrap();
                    panic!("worker did not reach the ELF initialization barrier");
                }
                std::thread::yield_now();
            }
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{fault}: {}",
            output_report(&output)
        );
    }
}

// @kotowari[REQ-library-403]
#[test]
fn repeated_preparation_releases_descriptors_and_reaps_each_worker() {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("home/.config/kakoi/profile/default.toml", "invalid profile");
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    let output = Command::new(consumer())
        .arg("--self-test-repeat")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home"))
        .current_dir(dir.path().join("workspace"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
}

fn compile_consumer(name: &str, source: &str) -> std::process::Output {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let directory = root.join("target/library-runtime-compile").join(name);
    std::fs::create_dir_all(directory.join("src")).unwrap();
    std::fs::write(directory.join("Cargo.toml"), format!(
        "[workspace]\n[package]\nname='runtime-{name}'\nversion='0.0.0'\nedition='2021'\n[dependencies]\nkakoi-runtime={{path={:?}}}\n", root.join("crates/kakoi-runtime")
    )).unwrap();
    std::fs::write(directory.join("src/main.rs"), source).unwrap();
    Command::new(env!("CARGO"))
        .args(["check", "--offline", "--manifest-path"])
        .arg(directory.join("Cargo.toml"))
        .env(
            "CARGO_TARGET_DIR",
            root.join("target/library-runtime-compile-target"),
        )
        .output()
        .unwrap()
}

// @kotowari[REQ-library-105]
#[test]
fn prepared_run_is_send_but_cannot_be_cloned_or_modified_from_a_consumer() {
    let valid = compile_consumer("valid", "use kakoi_runtime::PreparedRun; fn send<T: Send>() {} fn main() { send::<PreparedRun>(); }\n");
    assert!(valid.status.success(), "{}", output_report(&valid));
    for (name, source, code) in [
        ("clone", "use kakoi_runtime::PreparedRun; fn clone(p: PreparedRun) { let _ = p.clone(); } fn main() {}", "E0599"),
        ("modify", "use kakoi_runtime::PreparedRun; fn modify(p: &mut PreparedRun) { p.description.cwd = \"/\".into(); } fn main() {}", "E0616"),
    ] {
        let output = compile_consumer(name, source);
        assert!(!output.status.success(), "{name} unexpectedly compiled");
        assert!(String::from_utf8_lossy(&output.stderr).contains(code), "{}", output_report(&output));
    }
}

// @kotowari[REQ-library-104, REQ-library-101, EX-library-102]
#[test]
fn explicit_context_preserves_os_bytes_and_configuration_uses_its_paths() {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write(
        "home/.config/kakoi/profile/explicit.toml",
        "[network]\nmode='host'\n",
    );
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    dir.write("workspace/policy.toml", "[network]\nmode='none'\n");
    let output = Command::new(consumer())
        .arg("--self-test-input")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home"))
        .env("XDG_CONFIG_HOME", dir.path().join("home/.config"))
        .current_dir(dir.path().join("workspace"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
}

// @kotowari[REQ-library-101, EX-library-101, REQ-library-403, EX-library-405]
#[test]
fn memory_preparation_reexecutes_the_consumer_without_loading_profiles_or_running_the_command() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write(
        "home/.config/kakoi/profile/default.toml",
        "invalid profile that must not be read",
    );
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    let path = dir
        .path()
        .join("workspace")
        .join(OsString::from_vec(b"tool-\xff".to_vec()));
    std::fs::write(&path, "#!/bin/sh\ntouch command-ran\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let output = Command::new(consumer())
        .arg("--self-test-prepare")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home"))
        .env("XDG_CONFIG_HOME", dir.path().join("home/.config"))
        .current_dir(dir.path().join("workspace"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
    assert!(!dir.path().join("workspace/command-ran").exists());
}

// @kotowari[REQ-library-301]
#[test]
fn preparation_distinguishes_invalid_input_from_missing_host_tools_and_reaps_failures() {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("home/.config/kakoi/profile/default.toml", "invalid profile");
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    let output = Command::new(consumer())
        .arg("--self-test-prepare-errors")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home"))
        .env("XDG_CONFIG_HOME", dir.path().join("home/.config"))
        .current_dir(dir.path().join("workspace"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
}

// @kotowari[REQ-library-104]
#[test]
fn preparation_transfers_only_requested_fds_and_drop_reaps_the_worker_without_global_changes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = root.join("target/library-thread-signals");
    let mut command = Command::new("rustc");
    if cfg!(target_env = "musl") {
        command.args(["--target", "x86_64-unknown-linux-musl"]);
    }
    let compile = command
        .args(["--edition=2021"])
        .arg(root.join("tests/fixtures/library-api/thread_signals.rs"))
        .arg("-o")
        .arg(&fixture)
        .output()
        .unwrap();
    assert!(compile.status.success(), "{}", output_report(&compile));
    let ordinary = Command::new(fixture).output().unwrap();
    assert!(ordinary.status.success(), "{}", output_report(&ordinary));
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("home/.config/kakoi/profile/default.toml", "invalid profile");
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    dir.write("workspace/stdio-source", "input");
    dir.write("workspace/unrequested", "private input");
    let output = Command::new(consumer())
        .arg("--self-test-fds")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home"))
        .env("XDG_CONFIG_HOME", dir.path().join("home/.config"))
        .current_dir(dir.path().join("workspace"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
    fn delta(output: &std::process::Output) -> Vec<u64> {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .find(|line| line.starts_with("signals "))
            .unwrap()
            .split_whitespace()
            .skip(1)
            .map(|word| u64::from_str_radix(word, 16).unwrap())
            .collect()
    }
    let product = delta(&output);
    let runtime = delta(&ordinary);
    eprintln!(
        "libc={}: product delta {product:016x?}, ordinary thread delta {runtime:016x?}",
        if cfg!(target_env = "musl") {
            "musl"
        } else {
            "gnu"
        }
    );
    assert_eq!(
        product, runtime,
        "changes differ from ordinary libc thread initialization"
    );
    assert_eq!(product[1], 0, "caller mask changed");
    for change in &product[2..] {
        assert_eq!(change & product[0], 0, "application-usable signal changed");
    }
}
