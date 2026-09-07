use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

mod common;

use common::{
    assert_diagnostic, binary, home_with_workspace, output_report, run,
    run_command_with_soft_fd_limit, run_from_deleted_dir, TempDir, RW_WORKSPACE,
};
use process_wrap::cli::{interpret, Invocation, Parsed};
use process_wrap::diagnostic::Kind;

/// A name for the failure message and the arrangement it makes under a temporary home.
type Arrangement = (&'static str, fn(&Path));

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

    // A nested run reads the policy too: a named profile that is not there is a
    // diagnostic, not a plan.
    let without_a_profile = binary(home.path())
        .env("PROCESS_WRAP", "1")
        .args(["--profile", "strict"])
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

/// A workspace under a fresh home, with nothing written to the configuration directory.
fn home_without_a_configuration_directory() -> (TempDir, PathBuf) {
    let home = TempDir::new();
    let workspace = home.path().join("ws");
    std::fs::create_dir(&workspace).unwrap();
    (home, workspace)
}

#[test]
fn the_built_in_default_is_used_when_default_toml_is_absent() {
    // The state of a new machine: nothing has been written to the configuration directory,
    // whether it is the one under `~/.config` or the one `XDG_CONFIG_HOME` names, whose own
    // ancestors are missing too (specification section 5.3).
    let (home, workspace) = home_without_a_configuration_directory();

    for (name, config_home) in [
        ("no configuration directory", home.path().join(".config")),
        (
            "no XDG_CONFIG_HOME directory",
            home.path().join("missing/xdg"),
        ),
    ] {
        let output = binary(home.path())
            .env("XDG_CONFIG_HOME", &config_home)
            .current_dir(&workspace)
            .args(["--print-plan", "--", "/bin/true"])
            .output()
            .unwrap();

        let report = output_report(&output);
        assert_eq!(output.status.code(), Some(0), "{name}: {report}");
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("process-wrap init"),
            "{name}: {report}"
        );
    }
}

#[test]
fn a_named_profile_never_falls_back_to_the_built_in_default() {
    // A user who names a profile means that file; falling back would run a wider policy
    // than the one asked for (specification section 5.3).
    let (home, workspace) = home_without_a_configuration_directory();

    let output = binary(home.path())
        .current_dir(&workspace)
        .args(["--profile", "strict", "--print-plan", "--", "/bin/true"])
        .output()
        .unwrap();

    assert_diagnostic(&output, 125, "policy");
}

#[test]
fn a_broken_default_toml_or_configuration_directory_is_a_policy_diagnostic() {
    // Only a name that is not there falls back: a `default.toml` or a configuration
    // directory that exists but cannot be followed stops the run, so a policy that broke is
    // never replaced by the wider built-in default (specification section 5.3).
    let arrangements: [Arrangement; 4] = [
        ("a broken link at default.toml", |home| {
            std::fs::create_dir_all(home.join(".config/process-wrap/profile")).unwrap();
            std::os::unix::fs::symlink(
                home.join("nowhere"),
                home.join(".config/process-wrap/profile/default.toml"),
            )
            .unwrap();
        }),
        ("a broken link at the configuration directory", |home| {
            std::fs::create_dir_all(home.join(".config")).unwrap();
            std::os::unix::fs::symlink(home.join("nowhere"), home.join(".config/process-wrap"))
                .unwrap();
        }),
        ("a regular file at the configuration directory", |home| {
            std::fs::create_dir_all(home.join(".config")).unwrap();
            std::fs::write(home.join(".config/process-wrap"), "").unwrap();
        }),
        ("a regular file at profile/", |home| {
            std::fs::create_dir_all(home.join(".config/process-wrap")).unwrap();
            std::fs::write(home.join(".config/process-wrap/profile"), "").unwrap();
        }),
    ];

    for (name, arrange) in arrangements {
        let (home, workspace) = home_without_a_configuration_directory();
        arrange(home.path());

        let output = binary(home.path())
            .current_dir(&workspace)
            .args(["--print-plan", "--", "/bin/true"])
            .output()
            .unwrap();

        let report = output_report(&output);
        assert_eq!(output.status.code(), Some(125), "{name}: {report}");
        assert!(
            String::from_utf8_lossy(&output.stderr).starts_with("process-wrap: policy: "),
            "{name}: {report}"
        );
    }
}

#[test]
fn a_policy_file_overlays_the_built_in_default() {
    // The built-in default adds no layer: `--policy-file` stacks on it as it would on a
    // profile file. Its `${config_dir}` item has no value, because a run on the built-in
    // default has no configuration directory, so it is reported as skipped (specification
    // sections 5.2 and 5.3).
    let (home, workspace) = home_without_a_configuration_directory();
    let policy_file = home.write("p.toml", "[mounts]\nro = [\"${config_dir}/x\"]\n");

    let output = binary(home.path())
        .current_dir(&workspace)
        .args([
            "--policy-file",
            policy_file.to_str().unwrap(),
            "--print-plan",
            "--",
            "/bin/true",
        ])
        .output()
        .unwrap();

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let plan = String::from_utf8_lossy(&output.stdout);
    assert!(plan.contains("process-wrap init"), "{report}");
    assert!(
        plan.lines()
            .any(|line| line.contains("skipped") && line.contains("${config_dir}/x")),
        "{report}"
    );
}

#[test]
fn a_present_default_toml_replaces_the_built_in_default() {
    // Once `default.toml` is there it is the whole global scope; the built-in default is
    // not read beside it (specification section 5.3).
    let (home, workspace) = home_without_a_configuration_directory();
    let profile = home.write(".config/process-wrap/profile/default.toml", RW_WORKSPACE);

    let output = binary(home.path())
        .current_dir(&workspace)
        .args(["--print-plan", "--", "/bin/true"])
        .output()
        .unwrap();

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let plan = String::from_utf8_lossy(&output.stdout);
    assert!(!plan.contains("process-wrap init"), "{report}");
    assert!(plan.contains(profile.to_str().unwrap()), "{report}");
}

/// Every path under `root`, relative and sorted. A symbolic link is listed but not walked.
fn tree(root: &Path) -> Vec<PathBuf> {
    fn walk(root: &Path, dir: &Path, into: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            into.push(path.strip_prefix(root).unwrap().to_path_buf());
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                walk(root, &path, into);
            }
        }
    }
    let mut paths = Vec::new();
    walk(root, root, &mut paths);
    paths.sort();
    paths
}

/// The bundled profile as bytes: what `init` writes and what the built-in default is.
fn bundled_profile() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/profile/default.toml"
    ))
    .unwrap()
}

#[test]
fn init_writes_the_built_in_default_and_prints_its_path() {
    let home = TempDir::new();
    let before = tree(home.path());
    // `/tmp/process-wrap` is the user's or the shim's to make; `init` does not touch it
    // (specification section 14).
    let shared = Path::new("/tmp/process-wrap");
    let shared_before = shared.symlink_metadata().is_ok();

    let output = run(home.path(), ["init"]);

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert!(output.stderr.is_empty(), "{report}");
    let written = home
        .path()
        .join(".config/process-wrap/profile/default.toml");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("{}\n", written.display()),
        "{report}"
    );
    assert_eq!(
        std::fs::read(&written).unwrap(),
        bundled_profile(),
        "{report}"
    );
    let secrets = home.path().join(".config/process-wrap/secrets");
    assert_eq!(
        std::fs::metadata(&secrets).unwrap().permissions().mode() & 0o7777,
        0o700,
        "{report}"
    );
    let added: Vec<PathBuf> = tree(home.path())
        .into_iter()
        .filter(|path| !before.contains(path))
        .collect();
    assert_eq!(
        added,
        [
            ".config",
            ".config/process-wrap",
            ".config/process-wrap/profile",
            ".config/process-wrap/profile/default.toml",
            ".config/process-wrap/secrets",
        ]
        .map(PathBuf::from),
        "{report}"
    );
    assert_eq!(shared.symlink_metadata().is_ok(), shared_before, "{report}");
}

#[test]
fn init_takes_a_profile_name() {
    let home = TempDir::new();

    let output = run(home.path(), ["init", "strict"]);

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let written = home.path().join(".config/process-wrap/profile/strict.toml");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("{}\n", written.display()),
        "{report}"
    );
    assert_eq!(
        std::fs::read(&written).unwrap(),
        bundled_profile(),
        "{report}"
    );
}

#[test]
fn init_rejects_a_bad_name_or_extra_arguments() {
    // `init` takes at most a NAME, and the NAME is one path component: anything else,
    // including an option or a command, is a usage diagnostic (specification section 4.1).
    for arguments in [
        ["init", "../x"].as_slice(),
        &["init", "--", "sh"],
        &["init", "--print-plan"],
    ] {
        let diagnostic = interpret(arguments.iter().map(OsString::from)).unwrap_err();

        assert_eq!(diagnostic.kind(), Kind::Usage, "{arguments:?}");
    }
}

#[test]
fn init_refuses_to_overwrite_an_existing_profile() {
    // There is no `--force`: the boundary the user wrote is never replaced by the product,
    // so the way to rewrite it is to remove it first (specification section 4.1).
    let home = TempDir::new();
    let written = home.write(".config/process-wrap/profile/default.toml", "# mine\n");

    let output = run(home.path(), ["init"]);

    let diagnostic = assert_diagnostic(&output, 125, "path");
    assert!(
        diagnostic.contains(written.to_str().unwrap()),
        "{diagnostic}"
    );
    assert_eq!(std::fs::read_to_string(&written).unwrap(), "# mine\n");
}

#[test]
fn init_refuses_a_broken_link_or_a_regular_file_in_the_way() {
    // The name written to is not followed, so a broken link there is something that already
    // exists; the components above it are followed and must end at directories
    // (specification section 4.1).
    let arrangements: [(Arrangement, &str); 2] = [
        (
            ("a broken link at the file", |home| {
                std::fs::create_dir_all(home.join(".config/process-wrap/profile")).unwrap();
                std::os::unix::fs::symlink(
                    home.join("nowhere"),
                    home.join(".config/process-wrap/profile/default.toml"),
                )
                .unwrap();
            }),
            ".config/process-wrap/profile/default.toml",
        ),
        (
            ("a regular file at profile/", |home| {
                std::fs::create_dir_all(home.join(".config/process-wrap")).unwrap();
                std::fs::write(home.join(".config/process-wrap/profile"), "").unwrap();
            }),
            ".config/process-wrap/profile",
        ),
    ];

    for ((name, arrange), target) in arrangements {
        let home = TempDir::new();
        arrange(home.path());

        let output = run(home.path(), ["init"]);

        let diagnostic = assert_diagnostic(&output, 125, "path");
        assert!(
            diagnostic.contains(home.path().join(target).to_str().unwrap()),
            "{name}: {diagnostic}"
        );
    }
}

#[test]
fn init_follows_a_linked_configuration_directory_and_prints_the_written_path() {
    // A user keeps the configuration directory in dotfiles behind a link: the file lands at
    // the target, and the line printed is the path as assembled, link and all
    // (specification section 4.1).
    let home = TempDir::new();
    let target = home.path().join("dotfiles/process-wrap");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::create_dir_all(home.path().join(".config")).unwrap();
    std::os::unix::fs::symlink(&target, home.path().join(".config/process-wrap")).unwrap();

    let output = run(home.path(), ["init"]);

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let real_home = home.path().canonicalize().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!(
            "{}\n",
            real_home
                .join(".config/process-wrap/profile/default.toml")
                .display()
        ),
        "{report}"
    );
    assert_eq!(
        std::fs::read(target.join("profile/default.toml")).unwrap(),
        bundled_profile(),
        "{report}"
    );
}

#[test]
fn init_creates_missing_ancestors_of_the_configuration_directory() {
    // A new machine has no `~/.config` either; the ancestors of the place the user's own
    // environment names are made too (specification section 4.1).
    let home = TempDir::new();

    let output = binary(home.path())
        .env_remove("XDG_CONFIG_HOME")
        .args(["init"])
        .output()
        .unwrap();

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let real_home = home.path().canonicalize().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!(
            "{}\n",
            real_home
                .join(".config/process-wrap/profile/default.toml")
                .display()
        ),
        "{report}"
    );
}

#[test]
fn init_ignores_nesting_the_current_directory_and_bwrap() {
    // Inside an isolation, from a directory that is gone, and on a machine without `bwrap`,
    // the configuration can still be put in place (specification sections 4.1 and 12.1).
    let nested_home = TempDir::new();
    let nested = binary(nested_home.path())
        .env("PROCESS_WRAP", "1")
        .args(["init"])
        .output()
        .unwrap();
    let report = output_report(&nested);
    assert_eq!(nested.status.code(), Some(0), "{report}");
    assert!(nested.stderr.is_empty(), "{report}");

    let deleted_home = TempDir::new();
    let deleted = run_from_deleted_dir(deleted_home.path(), ["init"]);
    assert_eq!(
        deleted.status.code(),
        Some(0),
        "{}",
        output_report(&deleted)
    );

    let bare_home = TempDir::new();
    let without_bwrap = binary(bare_home.path())
        .env("PATH", "")
        .args(["init"])
        .output()
        .unwrap();
    assert_eq!(
        without_bwrap.status.code(),
        Some(0),
        "{}",
        output_report(&without_bwrap)
    );
}

#[test]
fn init_without_a_usable_home_is_an_env_diagnostic() {
    // The home directory is the one check `init` passes (specification section 13,
    // stage 2).
    let home = TempDir::new();

    let output = binary(home.path())
        .env("HOME", "")
        .args(["init"])
        .output()
        .unwrap();

    assert_diagnostic(&output, 125, "env");
}
