use std::path::Path;
use std::process::Command;

mod common;
#[allow(dead_code)]
#[path = "kakoi_net/fake_host.rs"]
mod fake_host;
#[path = "library_api/filtered.rs"]
mod filtered;

// @kotowari[REQ-library-101, REQ-library-105]
#[test]
fn secret_permission_errors_retain_the_os_cause_without_string_parsing() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    dir.write("home/.keep", "");
    dir.write("workspace/unreadable", "private value");
    std::fs::set_permissions(
        dir.path().join("workspace/unreadable"),
        std::fs::Permissions::from_mode(0o000),
    )
    .unwrap();
    let output = Command::new(consumer())
        .arg("--self-test-secret-permission")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home"))
        .current_dir(dir.path().join("workspace"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
}

// @kotowari[REQ-library-104]
#[test]
fn command_names_matching_mount_options_are_values_not_operations() {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    dir.write("home/.keep", "");
    for name in ["ordinary", "--bind", "--ro-bind", "--dev-bind", "--tmpfs"] {
        let destination = dir.path().join("tools").join(name);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::copy("/bin/true", destination).unwrap();
    }
    let output = Command::new(consumer())
        .arg("--self-test-option-command")
        .env_clear()
        .env(
            "PATH",
            format!(
                "{}:{}",
                dir.path().join("tools").display(),
                std::env::var("PATH").unwrap()
            ),
        )
        .env("HOME", dir.path().join("home"))
        .current_dir(dir.path().join("workspace"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
}

// @kotowari[REQ-library-205, EX-library-209]
#[test]
fn completion_during_wait_poll_registration_is_not_blocked_by_executor_waker_cloning() {
    run_fixture("--self-test-wait-registration");
}

// @kotowari[REQ-library-205, REQ-library-302, EX-library-213]
#[test]
fn completion_during_event_poll_registration_retains_the_event_and_independent_result() {
    run_fixture("--self-test-event-registration");
}

// @kotowari[REQ-library-304, REQ-library-401, REQ-library-404, EX-library-308, EX-library-401, EX-library-406]
#[test]
fn standalone_async_consumer_moves_pipe_ownership_and_reads_and_writes_nonblocking() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target = root.join("target/library-async-consumer");
    let mut build = Command::new(env!("CARGO"));
    if cfg!(target_env = "musl") {
        build.args(["--config", "build.target='x86_64-unknown-linux-musl'"]);
    }
    let output = build
        .args(["build", "--locked", "--manifest-path"])
        .arg(root.join("examples/library-async/Cargo.toml"))
        .env("CARGO_TARGET_DIR", &target)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
    let target = if cfg!(target_env = "musl") {
        target.join("x86_64-unknown-linux-musl")
    } else {
        target
    };
    let output = Command::new(target.join("debug/kakoi-library-async-example"))
        .arg("--self-test")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
}

// @kotowari[REQ-library-303, REQ-library-304, EX-library-306, EX-library-309]
#[test]
fn caller_created_terminal_descriptors_and_inheritance_remain_terminals() {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("home/.keep", "");
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    let script = r#"
import os, subprocess, sys, termios, tty
master, slave = os.openpty()
tty.setraw(slave)
p = subprocess.Popen([sys.argv[1], '--self-test-pty'], stdin=slave, stdout=slave, stderr=subprocess.PIPE, close_fds=True)
os.close(slave)
data = b''
while data.count(b'pty-connected') < 2:
    chunk = os.read(master, 4096)
    if not chunk: break
    data += chunk
_, errors = p.communicate(timeout=10)
assert p.returncode == 0, errors
assert data == b'pty-connectedpty-connected', data
os.close(master)
"#;
    let output = Command::new("python3")
        .args(["-c", script])
        .arg(consumer())
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home"))
        .current_dir(dir.path().join("workspace"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
}

// @kotowari[REQ-library-205, REQ-library-302, EX-library-210, EX-library-213, EX-library-304, EX-library-306]
#[test]
fn event_wait_cancellation_preserves_unread_events_and_receivers_are_independent() {
    run_fixture("--self-test-events");
}

// @kotowari[REQ-library-205, EX-library-209, EX-library-210]
#[test]
fn wait_futures_cancel_without_stopping_and_share_the_retained_sync_result() {
    run_fixture("--self-test-wait-future");
}

// @kotowari[REQ-library-301, EX-library-303]
#[test]
fn natural_main_completion_reports_stopping_while_descendant_cleanup_is_pending() {
    run_fixture("--self-test-natural-stopping");
}

// @kotowari[REQ-library-206, REQ-library-402, EX-library-211, EX-library-212]
#[test]
fn nested_api_creates_a_new_isolation_preserves_outer_guards_and_never_falls_back_to_exec() {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("home/.config/kakoi/profile/default.toml", "invalid profile");
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    dir.write("workspace/secret", "outer secret");
    dir.write_executable("workspace/tools/tool", "#!/bin/sh\nprintf unguarded\n");
    let real = Command::new("/bin/sh")
        .args(["-c", "command -v bwrap"])
        .output()
        .unwrap();
    let real = String::from_utf8(real.stdout).unwrap();
    dir.write_executable(
        "workspace/failing/bwrap",
        format!(
            "#!/bin/sh\nif [ \"$1\" = --help ]; then exec {} --help; fi\nexit 1\n",
            real.trim()
        ),
    );
    let output = Command::new(consumer())
        .arg("--self-test-api-nested")
        .env_clear()
        .env(
            "PATH",
            format!(
                "{}:{}",
                dir.path().join("workspace/tools").display(),
                std::env::var("PATH").unwrap()
            ),
        )
        .env("HOME", dir.path().join("home"))
        .current_dir(dir.path().join("workspace"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
}
#[path = "library_api/retained_mounts.rs"]
mod retained_mounts;
use common::{output_report, TempDir};

// @kotowari[REQ-library-410, REQ-288, EX-library-417]
#[test]
fn consumer_itself_is_subject_to_its_requested_guard() {
    run_fixture("--self-test-own-guard");
}

// @kotowari[REQ-library-408, REQ-library-409, REQ-449, REQ-475, EX-library-412, EX-library-416]
#[test]
fn listed_allows_each_copied_guard_but_not_the_original_consumer() {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("home/.config/kakoi/profile/default.toml", "invalid profile");
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    dir.write("workspace/secret", "private");
    dir.write("runtime/sentinel", "keep");
    dir.write_executable("workspace/tools/tool", "#!/bin/sh\nexit 7\n");
    dir.write_executable("workspace/tools/tool2", "#!/bin/sh\nexit 8\n");
    let output = Command::new(consumer())
        .arg("--self-test-listed-copies")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home"))
        .env("XDG_RUNTIME_DIR", dir.path().join("runtime"))
        .current_dir(dir.path().join("workspace"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
    let files: Vec<_> = std::fs::read_dir(dir.path().join("runtime/kakoi"))
        .unwrap()
        .map(|f| f.unwrap().file_name())
        .collect();
    assert_eq!(files, [std::ffi::OsString::from("empty")]);
    assert_eq!(
        std::fs::read(dir.path().join("runtime/kakoi/empty")).unwrap(),
        b""
    );
}

// @kotowari[REQ-library-106, REQ-library-408]
#[test]
fn changed_generated_guard_table_fails_before_releasing_the_target() {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("home/.config/kakoi/profile/default.toml", "invalid profile");
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    dir.write_executable(
        "workspace/tools/tool",
        "#!/bin/sh\nprintf ran > target-ran\n",
    );
    let real = Command::new("/bin/sh")
        .args(["-c", "command -v bwrap"])
        .output()
        .unwrap();
    let real = String::from_utf8(real.stdout).unwrap();
    dir.write_executable("wrapper/bwrap", format!("#!/usr/bin/python3\nimport os,sys,json\na=sys.argv[1:]\nfor i,v in enumerate(a):\n if v=='/dev/kakoi-guard/table' and a[i-2]=='--ro-bind-data':\n  t=json.loads(os.read(int(a[i-1]),1000000)); [e.update(rules=[]) for e in t['entries']]; f=os.memfd_create('replacement',0); os.write(f,json.dumps(t).encode()); os.lseek(f,0,0); a[i-1]=str(f)\nos.execv({:?},[{:?}]+a)\n",real.trim(),real.trim()));
    let output = Command::new(consumer())
        .arg("--self-test-changed-table")
        .env_clear()
        .env(
            "PATH",
            format!(
                "{}:{}",
                dir.path().join("wrapper").display(),
                std::env::var("PATH").unwrap()
            ),
        )
        .env("HOME", dir.path().join("home"))
        .current_dir(dir.path().join("workspace"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
}

// @kotowari[REQ-library-408, REQ-453, EX-library-413]
#[test]
fn role_and_per_copy_identity_reject_missing_tables_and_match_deleted_suffixes() {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("home/.config/kakoi/profile/default.toml", "invalid profile");
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    for (name, exit) in [
        ("tool", 7),
        ("tool2", 8),
        ("tool (deleted)", 9),
        ("tool (deleted) (deleted)", 10),
    ] {
        dir.write_executable(
            format!("workspace/tools/{name}"),
            format!("#!/bin/sh\nexit {exit}\n"),
        );
    }
    dir.write(
        "workspace/dispatch.py",
        include_str!("fixtures/library-api/guard_dispatch.py"),
    );
    let output = Command::new(consumer())
        .arg("--self-test-dispatch-failures")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home"))
        .current_dir(dir.path().join("workspace"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
}

// @kotowari[REQ-library-408, REQ-475]
#[test]
fn probe_initializers_descendants_are_reaped_before_target_release() {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("home/.config/kakoi/profile/default.toml", "invalid profile");
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    for name in ["tool", "tool2"] {
        dir.write_executable(format!("workspace/tools/{name}"), "#!/bin/sh\nexit 0\n");
    }
    let output = Command::new(consumer())
        .arg("--self-test-probe-descendants")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home"))
        .current_dir(dir.path().join("workspace"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
}

// @kotowari[REQ-library-406, EX-library-408, EX-library-409]
#[test]
fn dynamically_linked_helper_dependencies_are_not_implicitly_published() {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("home/.config/kakoi/profile/default.toml", "invalid profile");
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/library-sync-dynamic");
    let build = Command::new(env!("CARGO"))
        .args([
            "build",
            "--manifest-path",
            "examples/library-sync/Cargo.toml",
            "--locked",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--target-dir",
        ])
        .arg(&target)
        .output()
        .unwrap();
    assert!(build.status.success(), "{}", output_report(&build));
    let executable = target.join("x86_64-unknown-linux-gnu/debug/kakoi-library-sync-example");
    let dependencies = Command::new("ldd").arg(&executable).output().unwrap();
    assert!(
        dependencies.status.success(),
        "{}",
        output_report(&dependencies)
    );
    assert!(String::from_utf8_lossy(&dependencies.stdout).contains("libc.so"));
    let output = Command::new(executable)
        .arg("--self-test-missing-dynamic")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home"))
        .current_dir(dir.path().join("workspace"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
}

// @kotowari[REQ-library-408, EX-library-414]
#[test]
fn guard_execution_denial_fails_before_target_release_without_disk_fallback() {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("home/.config/kakoi/profile/default.toml", "invalid profile");
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    dir.write_executable(
        "workspace/tools/tool",
        "#!/bin/sh\nprintf ran > target-ran\n",
    );
    let real = Command::new("/bin/sh")
        .args(["-c", "command -v bwrap"])
        .output()
        .unwrap();
    let real = String::from_utf8(real.stdout).unwrap();
    dir.write_executable("wrapper/bwrap",format!("#!/usr/bin/python3\nimport os,sys\na=sys.argv[1:]\nfor i,v in enumerate(a):\n if v=='/dev/kakoi-guard/bin/tool' and a[i-2]=='--ro-bind-data':\n  assert a[i-4]=='--perms'; a[i-3]='0444'\nos.execv({:?},[{:?}]+a)\n",real.trim(),real.trim()));
    let output = Command::new(consumer())
        .arg("--self-test-denied-guard")
        .env_clear()
        .env(
            "PATH",
            format!(
                "{}:{}",
                dir.path().join("wrapper").display(),
                std::env::var("PATH").unwrap()
            ),
        )
        .env("HOME", dir.path().join("home"))
        .current_dir(dir.path().join("workspace"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
}

// @kotowari[REQ-library-106]
#[test]
fn generated_data_checks_transfer_more_than_one_descriptor_batch() {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("home/.config/kakoi/profile/default.toml", "invalid profile");
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    for n in 0..17 {
        dir.write(format!("workspace/hide-{n}"), "hidden");
    }
    let output = Command::new(consumer())
        .arg("--self-test-data-batches")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home"))
        .current_dir(dir.path().join("workspace"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
}

// @kotowari[REQ-library-301, REQ-library-303, EX-library-303]
#[test]
fn host_and_none_launch_report_exit_and_pipe_output_and_reap_descendants() {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("home/.config/kakoi/profile/default.toml", "invalid profile");
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    let output = Command::new(consumer())
        .arg("--self-test-spawn")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home"))
        .current_dir(dir.path().join("workspace"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
}

// @kotowari[REQ-library-202, REQ-library-203, REQ-library-303, EX-library-203, EX-library-205, EX-library-307]
#[test]
fn stop_and_owner_drop_work_with_unread_pipes_and_nonowning_handles() {
    run_fixture("--self-test-lifetime");
}

// @kotowari[REQ-library-301]
#[test]
fn synchronous_exec_failure_is_not_a_normal_nonzero_exit() {
    run_fixture("--self-test-exec-error");
}

// @kotowari[REQ-library-106, EX-library-110]
#[test]
fn changed_explicit_mount_symlink_fails_before_releasing_the_command() {
    run_fixture("--self-test-mount-identity");
}

// @kotowari[REQ-library-204, EX-library-207]
#[test]
fn stopping_one_parallel_run_leaves_the_other_pipe_usable() {
    run_fixture("--self-test-parallel");
}

// @kotowari[REQ-library-303]
#[test]
fn inherited_stdio_keeps_the_preparation_snapshot_after_descriptor_replacement() {
    run_fixture("--self-test-inherit");
}

// @kotowari[REQ-library-402]
#[test]
fn helper_control_environment_is_not_published_to_the_target() {
    run_fixture("--self-test-helper-environment");
}

// @kotowari[REQ-library-104, EX-library-107, REQ-library-105, EX-library-108]
#[test]
fn requested_cwd_and_os_bytes_are_used_without_mutating_the_caller() {
    run_fixture("--self-test-context-run");
}

// @kotowari[REQ-library-104]
#[test]
fn distinct_non_utf8_mount_sources_are_not_matched_by_lossy_display_strings() {
    run_fixture("--self-test-raw-mounts");
}

// @kotowari[REQ-library-407, EX-library-410, EX-library-411]
#[test]
fn missing_descriptor_mount_features_fail_before_any_target_side_effect() {
    run_fixture("--self-test-missing-features");
}

// @kotowari[REQ-library-409]
#[test]
fn requested_shared_files_are_prepared_at_spawn_but_not_at_prepare() {
    run_fixture("--self-test-shared-files");
}

// @kotowari[REQ-library-106, EX-library-111]
#[test]
fn public_descriptor_mounts_keep_live_content_and_validate_device_identity() {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("home/.config/kakoi/profile/default.toml", "invalid profile");
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    dir.write("workspace/device", "");
    let output = Command::new("unshare")
        .args(["--user", "--map-root-user", "--mount", "--pid", "--fork", "--mount-proc", "/bin/sh", "-c",
            "KAKOI_TEST_REAL_BWRAP=$(command -v bwrap); export KAKOI_TEST_REAL_BWRAP; mount --bind /dev/null device && exec \"$1\" --self-test-live-device", "fixture"])
        .arg(consumer()).env_clear().env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home")).current_dir(dir.path().join("workspace"))
        .output().unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
}

// @kotowari[REQ-library-301]
#[test]
fn worker_death_does_not_claim_confirmed_cleanup() {
    run_fixture("--self-test-worker-death");
}

// @kotowari[REQ-library-301, EX-library-302]
#[test]
fn worker_death_after_main_exit_preserves_success_without_claiming_cleanup() {
    run_fixture("--self-test-main-retention");
}

// @kotowari[REQ-library-301, REQ-library-202, EX-library-204]
#[test]
fn later_control_failure_takes_priority_over_an_already_started_stop() {
    run_fixture("--self-test-stopping-failure");
}

// @kotowari[REQ-library-201, EX-library-201]
#[test]
fn supervision_and_reaping_continue_while_the_caller_runs_unrelated_children() {
    run_fixture("--self-test-independent-supervision");
}

// @kotowari[REQ-library-301]
#[test]
fn command_death_before_exec_does_not_turn_error_pipe_eof_into_start_success() {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("home/.config/kakoi/profile/default.toml", "invalid profile");
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    let output = Command::new("python3")
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/library-api/kill_before_exec.py"),
        )
        .arg(consumer())
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home"))
        .current_dir(dir.path().join("workspace"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
}

// @kotowari[REQ-library-201, REQ-library-301]
#[test]
fn caller_waitpid_reaping_the_worker_is_reported_as_unconfirmed() {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("home/.config/kakoi/profile/default.toml", "invalid profile");
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    let output = Command::new("python3")
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/library-api/external_reap.py"),
        )
        .arg(consumer())
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home"))
        .current_dir(dir.path().join("workspace"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
}

// @kotowari[REQ-library-203, EX-library-206]
#[test]
fn owner_process_death_terminates_the_private_isolation_without_drop() {
    use std::io::{BufRead, BufReader};
    use std::time::{Duration, Instant};
    fn descendants(pid: u32, found: &mut Vec<u32>) {
        let children =
            std::fs::read_to_string(format!("/proc/{pid}/task/{pid}/children")).unwrap_or_default();
        for child in children
            .split_whitespace()
            .map(|s| s.parse::<u32>().unwrap())
        {
            found.push(child);
            descendants(child, found);
        }
    }
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("home/.config/kakoi/profile/default.toml", "invalid profile");
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    let mut child = Command::new(consumer())
        .arg("--self-test-owner")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home"))
        .current_dir(dir.path().join("workspace"))
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert_eq!(line, "owner-ready\n");
    let mut owned = Vec::new();
    descendants(child.id(), &mut owned);
    assert!(owned.len() >= 3, "worker, bwrap and init were not observed");
    child.kill().unwrap();
    child.wait().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let live = owned
            .iter()
            .filter(|pid| {
                std::fs::read_to_string(format!("/proc/{pid}/status"))
                    .is_ok_and(|status| !status.lines().any(|line| line.starts_with("State:\tZ")))
            })
            .copied()
            .collect::<Vec<_>>();
        if live.is_empty() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "owned processes survived: {live:?}"
        );
        std::thread::yield_now();
    }
}

// @kotowari[REQ-library-203]
#[test]
fn owner_death_during_init_startup_does_not_leave_a_blocked_isolation() {
    use std::io::{BufRead, BufReader};
    use std::time::{Duration, Instant};
    fn descendants(pid: u32, found: &mut Vec<u32>) {
        let children =
            std::fs::read_to_string(format!("/proc/{pid}/task/{pid}/children")).unwrap_or_default();
        for child in children
            .split_whitespace()
            .map(|s| s.parse::<u32>().unwrap())
        {
            found.push(child);
            descendants(child, found);
        }
    }
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    dir.write("home/.config/kakoi/profile/default.toml", "invalid profile");
    let mut child = Command::new(consumer())
        .arg("--self-test-startup-owner")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home"))
        .env("KAKOI_TEST_STARTUP_FAULT", "stop")
        .current_dir(dir.path().join("workspace"))
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert_eq!(line, "init-startup\n");
    let mut owned = Vec::new();
    descendants(child.id(), &mut owned);
    assert!(owned.len() >= 3, "worker, bwrap and init were not observed");
    child.kill().unwrap();
    child.wait().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let live = owned
            .iter()
            .filter(|pid| {
                std::fs::read_to_string(format!("/proc/{pid}/status"))
                    .is_ok_and(|status| !status.lines().any(|line| line.starts_with("State:\tZ")))
            })
            .copied()
            .collect::<Vec<_>>();
        if live.is_empty() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "startup processes survived: {live:?}"
        );
        std::thread::yield_now();
    }
}

// @kotowari[REQ-library-106]
#[test]
fn final_mount_checks_accept_intentional_hides_child_overlays_and_copies() {
    run_fixture("--self-test-final-mounts");
}

fn run_fixture(argument: &str) {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("home/.config/kakoi/profile/default.toml", "invalid profile");
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    dir.write_executable("workspace/bad-exec", "#!/missing-loader\n");
    let output = Command::new(consumer())
        .arg(argument)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home"))
        .current_dir(dir.path().join("workspace"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
}

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

// @kotowari[REQ-446, REQ-library-104, EX-library-112, EX-library-404]
#[test]
fn named_command_follows_the_guard_placed_on_the_isolated_path() {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    dir.write("home/.config/kakoi/profile/default.toml", "invalid profile");
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    dir.write_executable("workspace/requested-tools/tool", "#!/bin/sh\nexit 0\n");
    dir.write_executable("workspace/policy-tools/tool", "#!/bin/sh\nexit 1\n");
    dir.write_executable("workspace/prepended-tools/tool", "#!/bin/sh\nexit 2\n");
    dir.write_executable("workspace/policy-tools/bwrap", "#!/bin/sh\nexit 99\n");
    let real = Command::new("/bin/sh")
        .args(["-c", "command -v bwrap"])
        .output()
        .unwrap();
    assert!(real.status.success());
    dir.write_executable(
        "workspace/requested-tools/bwrap",
        format!(
            "#!/bin/sh\nprintf used > {:?}\nexec {} \"$@\"\n",
            dir.path().join("workspace/bwrap-used"),
            String::from_utf8(real.stdout).unwrap().trim()
        ),
    );
    let output = Command::new(consumer())
        .arg("--self-test-command-guard-path")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home"))
        .current_dir(dir.path().join("workspace"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_report(&output));
    assert_eq!(
        std::fs::read(dir.path().join("workspace/bwrap-used")).unwrap(),
        b"used"
    );
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
    let directory = root
        .join(if cfg!(target_env = "musl") {
            "target/library-runtime-compile-musl"
        } else {
            "target/library-runtime-compile"
        })
        .join(name);
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
            root.join(if cfg!(target_env = "musl") {
                "target/library-runtime-compile-target-musl"
            } else {
                "target/library-runtime-compile-target"
            }),
        )
        .output()
        .unwrap()
}

// @kotowari[REQ-library-105, EX-library-109]
#[test]
fn run_owners_are_send_but_cannot_be_cloned_modified_or_reused_from_a_consumer() {
    let valid = compile_consumer("valid", "use kakoi_runtime::{PreparedRun, Running}; fn send<T: Send>() {} fn sync<T: Sync>() {} fn main() { send::<PreparedRun>(); send::<Running>(); sync::<Running>(); }\n");
    assert!(valid.status.success(), "{}", output_report(&valid));
    for (name, source, code) in [
        ("clone", "use kakoi_runtime::PreparedRun; fn clone(p: PreparedRun) { let _ = p.clone(); } fn main() {}", "E0599"),
        ("modify", "use kakoi_runtime::PreparedRun; fn modify(p: &mut PreparedRun) { p.description.cwd = \"/\".into(); } fn main() {}", "E0616"),
        ("reuse", "use kakoi_runtime::PreparedRun; fn reuse(p: PreparedRun) { let _ = p.spawn(); let _ = p.spawn(); } fn main() {}", "E0382"),
        ("running-clone", "use kakoi_runtime::Running; fn clone(r: Running) { let _ = r.clone(); } fn main() {}", "E0599"),
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

// @kotowari[REQ-library-104, REQ-library-201, EX-library-202]
#[test]
fn launch_transfers_only_requested_fds_and_reaps_without_changing_caller_signals_or_limits() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = root.join(if cfg!(target_env = "musl") {
        "target/library-thread-signals-musl"
    } else {
        "target/library-thread-signals-gnu"
    });
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
