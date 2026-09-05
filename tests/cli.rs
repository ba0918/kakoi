use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

mod common;

use common::{
    assert_diagnostic, binary, home_with_workspace, output_report, run,
    run_command_with_soft_fd_limit, run_from_deleted_dir, TempDir, RW_WORKSPACE,
};
use process_wrap::cli::{interpret, Invocation, Parsed};

fn interpret_ok(arguments: &[&str]) -> Parsed {
    interpret(arguments.iter().map(OsString::from)).unwrap()
}

fn invocation_anchored_at(arguments: &[&str], current_dir: &Path) -> Invocation {
    let Parsed::Invocation(invocation) = interpret_ok(arguments) else {
        panic!("not an invocation");
    };
    invocation.anchored(current_dir)
}

#[test]
fn unknown_option_is_a_usage_diagnostic_with_exit_125() {
    let home = TempDir::new();

    let output = run(home.path(), ["--bogus", "--", "true"]);

    assert_diagnostic(&output, 125, "usage");
}

#[test]
fn missing_command_without_print_plan_is_a_usage_diagnostic() {
    let home = TempDir::new();

    let output = run(home.path(), ["--profile", "x"]);

    assert_diagnostic(&output, 125, "usage");
}

#[test]
fn empty_command_after_dashes_is_a_usage_diagnostic() {
    let home = TempDir::new();

    for arguments in [&["--"][..], &["--print-plan", "--"][..]] {
        let output = run(home.path(), arguments);

        assert_diagnostic(&output, 125, "usage");
    }
}

#[test]
fn help_or_version_beside_an_invalid_command_line_is_a_usage_diagnostic() {
    let home = TempDir::new();

    for arguments in [
        &["--help", "--bogus"][..],
        &["--version", "--bogus"][..],
        &["--help", "--"][..],
    ] {
        let output = run(home.path(), arguments);

        assert_diagnostic(&output, 125, "usage");
    }
}

#[test]
fn help_prints_to_stdout_and_exits_zero() {
    let home = TempDir::new();

    let output = run(home.path(), ["--help"]);

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert!(output.stderr.is_empty(), "{report}");
    let help = String::from_utf8(output.stdout).unwrap();
    for option in [
        "--profile",
        "--policy-file",
        "--workspace",
        "--rw",
        "--hide",
        "--print-plan",
        "--version",
        "--help",
    ] {
        assert!(help.contains(option), "{option} is missing from {help:?}");
    }
}

#[test]
fn version_prints_the_cargo_version_and_exits_zero() {
    let home = TempDir::new();

    let output = run(home.path(), ["--version"]);

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert!(output.stderr.is_empty(), "{report}");
    let version = String::from_utf8(output.stdout).unwrap();
    assert!(
        version.contains(env!("CARGO_PKG_VERSION")),
        "{version:?} does not contain the Cargo.toml version"
    );
}

#[test]
fn print_plan_form_parses_without_a_command() {
    let Parsed::Invocation(invocation) = interpret_ok(&["--print-plan"]) else {
        panic!("not an invocation");
    };

    assert!(invocation.print_plan);
    assert!(invocation.command.is_empty());
}

#[test]
fn relative_option_paths_are_resolved_against_the_current_directory() {
    let invocation = invocation_anchored_at(
        &[
            "--policy-file",
            "policy.toml",
            "--workspace",
            "../ws",
            "--rw",
            "a",
            "--rw",
            "/abs",
            "--hide",
            "~/b",
            "--",
            "true",
        ],
        Path::new("/cwd"),
    );

    assert_eq!(
        invocation.policy_file,
        Some(PathBuf::from("/cwd/policy.toml"))
    );
    assert_eq!(invocation.workspace, Some(PathBuf::from("/cwd/../ws")));
    assert_eq!(
        invocation.rw,
        [PathBuf::from("/cwd/a"), PathBuf::from("/abs")]
    );
    assert_eq!(invocation.hide, [PathBuf::from("/cwd/~/b")]);
}

#[test]
fn command_is_passed_through_unresolved() {
    let invocation = invocation_anchored_at(
        &["--", "rel/cmd", "--rw", "x", "~/y", "--help"],
        Path::new("/cwd"),
    );

    assert_eq!(
        invocation.command,
        ["rel/cmd", "--rw", "x", "~/y", "--help"].map(OsString::from)
    );
    assert!(invocation.rw.is_empty());
}

#[test]
fn a_profile_name_that_is_not_a_single_path_component_is_a_usage_diagnostic() {
    let home = TempDir::new();

    for name in ["", "a/b", ".", ".."] {
        let output = run(home.path(), ["--profile", name, "--", "true"]);

        assert_diagnostic(&output, 125, "usage");
    }
}

#[test]
fn help_beside_a_valid_option_is_a_usage_diagnostic() {
    let home = TempDir::new();

    for arguments in [
        &["--help", "--profile", "x"][..],
        &["--version", "--", "true"][..],
        &["--print-plan", "--help"][..],
    ] {
        let output = run(home.path(), arguments);

        assert_diagnostic(&output, 125, "usage");
    }
}

#[test]
fn a_dash_led_option_value_is_a_usage_diagnostic() {
    let home = TempDir::new();

    for arguments in [
        &["--profile", "--rw", "/x", "--", "true"][..],
        &["--workspace", "-", "--", "true"][..],
    ] {
        let output = run(home.path(), arguments);

        assert_diagnostic(&output, 125, "usage");
    }
}

#[test]
fn an_empty_option_value_is_a_usage_diagnostic() {
    let home = TempDir::new();

    for arguments in [
        &["--workspace", "", "--", "true"][..],
        &["--profile=", "--", "true"][..],
    ] {
        let output = run(home.path(), arguments);

        assert_diagnostic(&output, 125, "usage");
    }
}

#[test]
fn a_repeated_single_use_option_is_a_usage_diagnostic() {
    let home = TempDir::new();

    for arguments in [
        &["--profile", "a", "--profile", "b", "--", "true"][..],
        &["--print-plan", "--print-plan"][..],
    ] {
        let output = run(home.path(), arguments);

        assert_diagnostic(&output, 125, "usage");
    }

    let Parsed::Invocation(invocation) = interpret_ok(&["--rw", "/a", "--rw", "/b", "--", "true"])
    else {
        panic!("not an invocation");
    };
    assert_eq!(invocation.rw, [PathBuf::from("/a"), PathBuf::from("/b")]);
}

#[test]
fn the_equals_form_means_the_same_as_the_separated_form() {
    let separated = interpret_ok(&[
        "--profile",
        "p",
        "--policy-file",
        "f.toml",
        "--workspace",
        "w",
        "--rw",
        "a",
        "--hide",
        "b",
        "--",
        "true",
    ]);
    let equals = interpret_ok(&[
        "--profile=p",
        "--policy-file=f.toml",
        "--workspace=w",
        "--rw=a",
        "--hide=b",
        "--",
        "true",
    ]);

    assert_eq!(separated, equals);
    let Parsed::Invocation(invocation) = equals else {
        panic!("not an invocation");
    };
    assert_eq!(invocation.profile, "p");
    assert_eq!(invocation.workspace, Some(PathBuf::from("w")));
}

#[test]
fn help_from_a_deleted_current_directory_exits_zero() {
    let home = TempDir::new();

    let output = run_from_deleted_dir(home.path(), ["--help"]);

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert!(output.stderr.is_empty(), "{report}");
    assert!(!output.stdout.is_empty(), "{report}");
}

#[test]
fn an_unknown_option_from_a_deleted_current_directory_is_a_usage_diagnostic() {
    let home = TempDir::new();

    let output = run_from_deleted_dir(home.path(), ["--bogus", "--", "true"]);

    assert_diagnostic(&output, 125, "usage");
}

#[test]
fn a_deleted_current_directory_is_a_path_diagnostic() {
    let home = TempDir::new();
    home.write(".config/process-wrap/profile/default.toml", "");
    let workspace = home
        .write("workspace/.keep", "")
        .parent()
        .unwrap()
        .to_path_buf();

    let output = run_from_deleted_dir(
        home.path(),
        ["--workspace", workspace.to_str().unwrap(), "--", "true"],
    );

    assert_diagnostic(&output, 125, "path");
}

#[test]
fn a_bad_home_beside_a_broken_profile_is_an_env_diagnostic() {
    let home = TempDir::new();
    home.write(
        ".config/process-wrap/profile/default.toml",
        "[mounts\nbroken",
    );
    let file = home.write("home-file", "");

    let output = binary(home.path())
        .env("HOME", &file)
        .args(["--", "true"])
        .output()
        .unwrap();

    assert_diagnostic(&output, 125, "env");
}

#[test]
fn a_broken_profile_beside_a_missing_workspace_is_a_policy_diagnostic() {
    let home = TempDir::new();
    home.write(
        ".config/process-wrap/profile/default.toml",
        "[mounts\nbroken",
    );
    let missing = home.path().join("missing");

    let output = run(
        home.path(),
        ["--workspace", missing.to_str().unwrap(), "--", "true"],
    );

    assert_diagnostic(&output, 125, "policy");
}

#[test]
fn a_policy_diagnostic_exits_125_with_one_stderr_line() {
    let (home, workspace) = home_with_workspace();
    home.write(
        ".config/process-wrap/profile/default.toml",
        "[mounts]\nrw = [\"relative/path\"]\n",
    );

    let output = run(
        home.path(),
        [
            "--workspace",
            workspace.to_str().unwrap(),
            "--",
            "/bin/true",
        ],
    );

    assert_diagnostic(&output, 125, "policy");
}

#[test]
fn print_plan_with_a_diagnostic_prints_no_plan() {
    let (home, workspace) = home_with_workspace();
    home.write(
        ".config/process-wrap/profile/default.toml",
        "[mounts]\nrw = [\"relative/path\"]\n",
    );

    let output = run(
        home.path(),
        ["--workspace", workspace.to_str().unwrap(), "--print-plan"],
    );

    assert_diagnostic(&output, 125, "policy");
}

#[test]
fn print_plan_exits_zero_and_prints_the_resolved_command() {
    let (home, workspace) = home_with_workspace();

    let output = run(
        home.path(),
        [
            "--workspace",
            workspace.to_str().unwrap(),
            "--print-plan",
            "--",
            "/bin/true",
        ],
    );

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert!(output.stderr.is_empty(), "{report}");
    let plan = String::from_utf8(output.stdout).unwrap();
    assert!(plan.contains("/bin/true"), "{report}");
    assert!(plan.contains(workspace.to_str().unwrap()), "{report}");
    assert!(plan.contains("--ro-bind"), "{report}");
}

#[test]
fn a_control_character_in_a_warning_is_escaped() {
    let (home, workspace) = home_with_workspace();
    // A secret whose file does not exist raises the warning of specification section 9,
    // which embeds the key name; the name carries an ESC.
    home.write(
        ".config/process-wrap/profile/default.toml",
        format!("{RW_WORKSPACE}[secrets]\n\"A\\u001bB\" = \"~/no-such-file\"\n"),
    );

    let output = run(
        home.path(),
        ["--workspace", workspace.to_str().unwrap(), "--print-plan"],
    );

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    assert_eq!(stderr.matches('\n').count(), 1, "{report}");
    assert!(stderr.starts_with("process-wrap: warning: "), "{report}");
    assert!(!output.stderr.contains(&0x1b), "{report}");
    assert!(!output.stdout.contains(&0x1b), "{report}");
}

/// The built binary started as a nested run: `PROCESS_WRAP=1` in its environment. No
/// profile exists under `home` and `PATH` is `path`, so anything but the nested branch
/// ends in a diagnostic.
fn nested(home: &TempDir, path: &Path) -> std::process::Command {
    let mut command = binary(home.path());
    command.env("PROCESS_WRAP", "1").env("PATH", path);
    command
}

#[test]
fn a_nested_launch_warns_and_runs_the_command_without_bwrap() {
    let home = TempDir::new();
    let empty_path = TempDir::new();

    let output = nested(&home, empty_path.path())
        .args(["--", "/bin/sh", "-c", "echo out; echo err >&2; exit 3"])
        .output()
        .unwrap();

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(3), "{report}");
    assert_eq!(output.stdout, b"out\n", "{report}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    let mut lines = stderr.lines();
    assert!(
        lines.next().unwrap().starts_with("process-wrap: warning: "),
        "{report}"
    );
    assert_eq!(lines.collect::<Vec<_>>(), ["err"], "{report}");
}

#[test]
fn a_nested_launch_leaves_the_environment_unchanged() {
    let home = TempDir::new();
    let mut command = nested(&home, Path::new("/nonexistent"));
    command
        .env_clear()
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("PROCESS_WRAP", "1")
        .env("PATH", "/nonexistent")
        .env("MARKER", "kept as is");
    let expected: std::collections::BTreeSet<String> = command
        .get_envs()
        .map(|(name, value)| {
            format!(
                "{}={}",
                name.to_str().unwrap(),
                value.unwrap().to_str().unwrap()
            )
        })
        .collect();

    let output = command.args(["--", "/usr/bin/env"]).output().unwrap();

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let inside: std::collections::BTreeSet<String> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(inside, expected, "{report}");
}

#[test]
fn a_nested_launch_resolves_the_command_on_the_host_path_and_exits_127_when_missing() {
    let home = TempDir::new();
    let bin = TempDir::new();
    let tool = bin.write("tool", "#!/bin/sh\nexit 7\n");
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
    let empty_path = TempDir::new();

    let found = nested(&home, bin.path())
        .args(["--", "tool"])
        .output()
        .unwrap();
    let report = output_report(&found);
    assert_eq!(found.status.code(), Some(7), "{report}");

    let missing = nested(&home, empty_path.path())
        .args(["--", "tool"])
        .output()
        .unwrap();
    let report = output_report(&missing);
    assert_eq!(missing.status.code(), Some(127), "{report}");
    assert!(missing.stdout.is_empty(), "{report}");
    let stderr = String::from_utf8(missing.stderr).unwrap();
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(lines.len(), 2, "{report}");
    assert!(lines[0].starts_with("process-wrap: warning: "), "{report}");
    assert!(
        lines[1].starts_with("process-wrap: command not found: "),
        "{report}"
    );
}

#[test]
fn a_nested_launch_passes_the_given_name_as_argv0() {
    // Specification sections 1 and 12.1: the nested run execs the command found on the
    // host's `PATH` with `COMMAND` as argv[0]. `sh` is a copy in a temporary directory, so
    // argv[0] `sh` and the resolved path are told apart.
    let home = TempDir::new();
    let bin = TempDir::new();
    std::fs::copy("/bin/sh", bin.path().join("sh")).unwrap();

    let output = nested(&home, bin.path())
        .args(["--", "sh", "-c", "echo \"$0\""])
        .output()
        .unwrap();

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert_eq!(output.stdout, b"sh\n", "{report}");
}

#[test]
fn a_nested_launch_of_a_script_with_a_missing_interpreter_exits_126() {
    // The command is found, but its exec fails (specification sections 4.2 and 12.1): one
    // warning line, then `command not executable`, exit code 126.
    let home = TempDir::new();
    let script = home.write("tool", "#!/nonexistent/interpreter\n");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

    let output = nested(&home, Path::new("/nonexistent"))
        .arg("--")
        .arg(&script)
        .output()
        .unwrap();

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(126), "{report}");
    assert!(output.stdout.is_empty(), "{report}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(lines.len(), 2, "{report}");
    assert!(lines[0].starts_with("process-wrap: warning: "), "{report}");
    assert!(
        lines[1].starts_with("process-wrap: command not executable: "),
        "{report}"
    );
}

#[test]
fn a_nested_launch_leaves_the_soft_limit_unchanged() {
    // A nested run makes no descriptors, so it does not raise the limit (specification
    // section 14): the command sees the 1024 the shell set.
    let home = TempDir::new();

    let output = run_command_with_soft_fd_limit(
        nested(&home, Path::new("/nonexistent")),
        1024,
        ["--", "/bin/sh", "-c", "ulimit -Sn"],
    );

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert_eq!(output.stdout, b"1024\n", "{report}");
}

#[test]
fn a_nested_print_plan_reads_the_policy_and_marks_the_plan_as_nested() {
    let home = TempDir::new();
    let workspace = home.path().join("ws");
    std::fs::create_dir(&workspace).unwrap();
    let arguments = ["--workspace", workspace.to_str().unwrap(), "--print-plan"];

    // Without a profile the policy is read and found missing: a diagnostic, not a plan.
    let without_a_profile = binary(home.path())
        .env("PROCESS_WRAP", "1")
        .args(arguments)
        .output()
        .unwrap();
    assert_diagnostic(&without_a_profile, 125, "policy");

    home.write(".config/process-wrap/profile/default.toml", RW_WORKSPACE);
    let plain = run(home.path(), arguments);
    let nested = binary(home.path())
        .env("PROCESS_WRAP", "1")
        .args(arguments)
        .output()
        .unwrap();

    let report = format!("{}\n{}", output_report(&plain), output_report(&nested));
    assert_eq!(plain.status.code(), Some(0), "{report}");
    assert_eq!(nested.status.code(), Some(0), "{report}");
    // The nested plan is the plain plan with one line in front that marks it as nested.
    let plain = String::from_utf8(plain.stdout).unwrap();
    let nested = String::from_utf8(nested.stdout).unwrap();
    let (first_line, rest) = nested.split_once('\n').unwrap();
    assert_eq!(rest, plain, "{report}");
    assert!(!plain.contains(first_line), "{report}");
}

#[test]
fn secret_values_never_reach_stdout_or_stderr() {
    let (home, workspace) = home_with_workspace();
    let value = "FAKE-SECRET-VALUE-not-a-real-credential";
    home.write(".config/process-wrap/secrets/token", format!("{value}\n"));
    // `GIT_CONFIG_COUNT` as a secret with a non-numeric value: the `env` diagnostic of
    // specification section 10 names the variable and must not show its value.
    home.write(".config/process-wrap/secrets/count", format!("{value}\n"));
    home.write(
        ".config/process-wrap/profile/default.toml",
        format!("{RW_WORKSPACE}[secrets]\nTOKEN = \"${{config_dir}}/secrets/token\"\n"),
    );
    let with_diagnostic = home.write(
        "count.toml",
        "[secrets]\nGIT_CONFIG_COUNT = \"${config_dir}/secrets/count\"\n\
         [git.instead-of]\n\"git@example.com:\" = \"https://example.com/\"\n",
    );
    let workspace = workspace.to_str().unwrap();

    for (arguments, code) in [
        (
            vec!["--workspace", workspace, "--print-plan", "--", "/bin/true"],
            0,
        ),
        (vec!["--workspace", workspace, "--", "/bin/true"], 0),
        (
            vec![
                "--workspace",
                workspace,
                "--policy-file",
                with_diagnostic.to_str().unwrap(),
                "--",
                "/bin/true",
            ],
            125,
        ),
    ] {
        let output = run(home.path(), &arguments);

        let report = output_report(&output);
        assert_eq!(output.status.code(), Some(code), "{report}");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!stdout.contains(value), "{report}");
        assert!(!stderr.contains(value), "{report}");
    }
}

#[test]
fn print_plan_is_identical_across_two_runs() {
    let (home, workspace) = home_with_workspace();
    home.write("ws/.env", "SECRET=x\n");
    home.write("ws/sub/.env.local", "SECRET=y\n");
    home.write("ws/sub/keep.txt", "");
    home.write("cache/.keep", "");
    home.write("notes/a.md", "");
    home.write(".config/process-wrap/secrets/token", "FAKE-TOKEN\n");
    home.write(
        ".config/process-wrap/profile/default.toml",
        "[mounts]\n\
         rw = [\"${workspace}\", \"~/cache\", \"${git_common_dir}\"]\n\
         ro = [\"~/notes\"]\n\
         hide = [\"~/missing\", \"~/notes/a.md\"]\n\
         [[mounts.scan]]\nroot = \"${worktree}\"\nnames = [\".env\", \".env.*\"]\n\
         [env]\nunset = [\"*_TOKEN\"]\nset = { B = \"2\", A = \"1\" }\n\
         [secrets]\nTOKEN = \"${config_dir}/secrets/token\"\n\
         [git.instead-of]\n\"git@example.com:\" = \"https://example.com/\"\n",
    );
    let arguments = [
        "--workspace",
        workspace.to_str().unwrap(),
        "--print-plan",
        "--",
        "/bin/true",
    ];

    let first = run(home.path(), arguments);
    let second = run(home.path(), arguments);

    let report = format!("{}\n{}", output_report(&first), output_report(&second));
    assert_eq!(first.status.code(), Some(0), "{report}");
    assert_eq!(second.status.code(), Some(0), "{report}");
    assert!(!first.stdout.is_empty(), "{report}");
    assert_eq!(first.stdout, second.stdout, "{report}");
}
