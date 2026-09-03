use std::ffi::OsString;
use std::path::{Path, PathBuf};

mod common;

use common::{assert_diagnostic, output_report, run, TempDir};
use process_wrap::cli::{interpret, Parsed};

fn interpret_ok(arguments: &[&str], current_dir: &Path) -> Parsed {
    interpret(arguments.iter().map(OsString::from), current_dir).unwrap()
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
    let Parsed::Invocation(invocation) = interpret_ok(&["--print-plan"], Path::new("/cwd")) else {
        panic!("not an invocation");
    };

    assert!(invocation.print_plan);
    assert!(invocation.command.is_empty());
}

#[test]
fn relative_option_paths_are_resolved_against_the_current_directory() {
    let Parsed::Invocation(invocation) = interpret_ok(
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
    ) else {
        panic!("not an invocation");
    };

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
    let Parsed::Invocation(invocation) = interpret_ok(
        &["--", "rel/cmd", "--rw", "x", "~/y", "--help"],
        Path::new("/cwd"),
    ) else {
        panic!("not an invocation");
    };

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
