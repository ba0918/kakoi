use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

mod common;

use common::{
    assert_diagnostic, binary, home_with_workspace, output_report, run, run_from_deleted_dir,
    TempDir, RW_WORKSPACE,
};
use kakoi::cli::{interpret, Invocation, Nesting, Parsed, PlanForm};
use kakoi_runtime::cli::diagnostic::Kind;

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

// @kotowari[REQ-250]
#[test]
fn unknown_option_is_a_usage_diagnostic_with_exit_125() {
    let home = TempDir::new();

    let output = run(home.path(), ["--bogus", "--", "true"]);

    assert_diagnostic(&output, 125, "usage");
}

// @kotowari[REQ-250]
#[test]
fn missing_command_without_print_plan_is_a_usage_diagnostic() {
    let home = TempDir::new();

    let output = run(home.path(), ["--profile", "x"]);

    assert_diagnostic(&output, 125, "usage");
}

// @kotowari[REQ-250]
#[test]
fn empty_command_after_dashes_is_a_usage_diagnostic() {
    let home = TempDir::new();

    for arguments in [&["--"][..], &["--print-plan", "--"][..]] {
        let output = run(home.path(), arguments);

        assert_diagnostic(&output, 125, "usage");
    }
}

// @kotowari[REQ-255]
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

// @kotowari[REQ-255]
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

// @kotowari[REQ-255]
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

// @kotowari[REQ-253]
#[test]
fn print_plan_form_parses_without_a_command() {
    let Parsed::Invocation(invocation) = interpret_ok(&["--print-plan"]) else {
        panic!("not an invocation");
    };

    assert_eq!(invocation.print_plan, Some(PlanForm::Summary));
    assert!(invocation.command.is_empty());
}

// @kotowari[REQ-253]
#[test]
fn print_plan_takes_its_form_after_an_equals_sign() {
    // The value is written with `=` only: separated, `full` would be taken for a command
    // written without `--` (specification section 4.1).
    for (arguments, form) in [
        (&["--print-plan=full", "--", "true"][..], PlanForm::Full),
        (&["--print-plan=summary"][..], PlanForm::Summary),
        (&["--print-plan=json"][..], PlanForm::Json),
    ] {
        let Parsed::Invocation(invocation) = interpret_ok(arguments) else {
            panic!("not an invocation");
        };
        assert_eq!(invocation.print_plan, Some(form), "{arguments:?}");
    }

    let home = TempDir::new();
    for arguments in [
        &["--print-plan=bogus"][..],
        &["--print-plan="][..],
        &["--print-plan", "full"][..],
        &["--print-plan=summary", "--print-plan=full"][..],
    ] {
        let output = run(home.path(), arguments);

        assert_diagnostic(&output, 125, "usage");
    }
}

// @kotowari[REQ-254]
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

// @kotowari[REQ-250]
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

// @kotowari[REQ-251]
#[test]
fn a_profile_name_that_is_not_a_single_path_component_is_a_usage_diagnostic() {
    let home = TempDir::new();

    for name in ["", "a/b", ".", "..", "a\nb"] {
        let output = run(home.path(), ["--profile", name, "--", "true"]);

        assert_diagnostic(&output, 125, "usage");
    }
}

// @kotowari[REQ-255]
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

// @kotowari[REQ-252]
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

// @kotowari[REQ-252]
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

// @kotowari[REQ-252]
#[test]
fn a_repeated_single_use_option_is_a_usage_diagnostic() {
    let home = TempDir::new();

    for arguments in [
        &["--profile", "a", "--profile", "b", "--", "true"][..],
        &["--print-plan", "--print-plan"][..],
        &["--print-plan=full", "--print-plan"][..],
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

// @kotowari[REQ-252]
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

/// How a nested run is to be handled, as the command line gives it.
fn nesting_of(arguments: &[&str]) -> Nesting {
    let Parsed::Invocation(invocation) = interpret_ok(arguments) else {
        panic!("not an invocation");
    };
    invocation.nested
}

// @kotowari[EX-896]
#[test]
fn ex_896_only_the_equals_form_of_a_known_nesting_value_is_accepted() {
    let home = TempDir::new();

    assert_eq!(
        nesting_of(&["--nested=isolate", "--", "true"]),
        Nesting::Isolate
    );
    for arguments in [
        &["--nested", "isolate", "--", "true"][..],
        &["--nested=other", "--", "true"][..],
        &["--nested", "--", "true"][..],
    ] {
        let output = run(home.path(), arguments);

        assert_diagnostic(&output, 125, "usage");
    }
}

// @kotowari[REQ-464]
#[test]
fn req_464_nesting_is_exec_unless_given_and_is_accepted_with_a_plan() {
    assert_eq!(nesting_of(&["--", "true"]), Nesting::Exec);
    assert_eq!(nesting_of(&["--nested=exec", "--", "true"]), Nesting::Exec);
    assert_eq!(
        nesting_of(&["--nested=isolate", "--print-plan=json"]),
        Nesting::Isolate
    );
}

// @kotowari[EX-897]
#[test]
fn ex_897_init_takes_no_nesting() {
    let home = TempDir::new();

    for arguments in [
        &["init", "--nested=isolate"][..],
        &["--nested=isolate", "init"][..],
    ] {
        let output = run(home.path(), arguments);

        assert_diagnostic(&output, 125, "usage");
    }
    assert!(!home.path().join(".config").exists());
}

// @kotowari[REQ-464, REQ-252]
#[test]
fn req_464_nesting_given_twice_or_beside_help_or_version_is_a_usage_diagnostic() {
    let home = TempDir::new();

    for arguments in [
        &["--nested=exec", "--nested=isolate", "--", "true"][..],
        &["--nested=isolate", "--nested=isolate", "--", "true"][..],
        &["--version", "--nested=isolate"][..],
        &["--help", "--nested=exec"][..],
    ] {
        let output = run(home.path(), arguments);

        assert_diagnostic(&output, 125, "usage");
    }
}

// @kotowari[REQ-255, EX-495]
#[test]
fn help_from_a_deleted_current_directory_exits_zero() {
    let home = TempDir::new();

    let output = run_from_deleted_dir(home.path(), ["--help"]);

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert!(output.stderr.is_empty(), "{report}");
    assert!(!output.stdout.is_empty(), "{report}");
}

// @kotowari[REQ-293]
#[test]
fn an_unknown_option_from_a_deleted_current_directory_is_a_usage_diagnostic() {
    let home = TempDir::new();

    let output = run_from_deleted_dir(home.path(), ["--bogus", "--", "true"]);

    assert_diagnostic(&output, 125, "usage");
}

// @kotowari[REQ-293, REQ-400, EX-753]
#[test]
fn a_deleted_current_directory_is_a_path_diagnostic() {
    let home = TempDir::new();
    home.write(".config/kakoi/profile/default.toml", "");
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

// With the current directory at hand, a relative `HOME` stops the run at the next stage.
// @kotowari[REQ-400, EX-754]
#[test]
fn a_relative_home_under_a_usable_current_directory_is_an_env_diagnostic() {
    let (home, workspace) = home_with_workspace();

    let output = binary(home.path())
        .env("HOME", "relative/home")
        .current_dir(&workspace)
        .args(["--", "true"])
        .output()
        .unwrap();

    assert_diagnostic(&output, 125, "env");
}

// @kotowari[REQ-293, EX-529]
#[test]
fn a_bad_home_beside_a_broken_profile_is_an_env_diagnostic() {
    let home = TempDir::new();
    home.write(".config/kakoi/profile/default.toml", "[mounts\nbroken");
    let file = home.write("home-file", "");

    let output = binary(home.path())
        .env("HOME", &file)
        .args(["--", "true"])
        .output()
        .unwrap();

    assert_diagnostic(&output, 125, "env");
}

// @kotowari[REQ-293]
#[test]
fn a_broken_profile_beside_a_missing_workspace_is_a_policy_diagnostic() {
    let home = TempDir::new();
    home.write(".config/kakoi/profile/default.toml", "[mounts\nbroken");
    let missing = home.path().join("missing");

    let output = run(
        home.path(),
        ["--workspace", missing.to_str().unwrap(), "--", "true"],
    );

    assert_diagnostic(&output, 125, "policy");
}

// @kotowari[REQ-288, REQ-290, EX-524]
#[test]
fn a_policy_diagnostic_exits_125_with_one_stderr_line() {
    let (home, workspace) = home_with_workspace();
    home.write(
        ".config/kakoi/profile/default.toml",
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

// @kotowari[REQ-292, EX-528]
#[test]
fn print_plan_with_a_diagnostic_prints_no_plan() {
    let (home, workspace) = home_with_workspace();
    home.write(
        ".config/kakoi/profile/default.toml",
        "[mounts]\nrw = [\"relative/path\"]\n",
    );

    let output = run(
        home.path(),
        ["--workspace", workspace.to_str().unwrap(), "--print-plan"],
    );

    assert_diagnostic(&output, 125, "policy");
}

// @kotowari[REQ-292]
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
    assert!(plan.contains("command: /bin/true\n"), "{report}");
    assert!(plan.contains(workspace.to_str().unwrap()), "{report}");
    // The summary leaves the bwrap argument list to the full form.
    assert!(!plan.contains("bwrap arguments:"), "{report}");
    assert!(!plan.contains("--ro-bind"), "{report}");
}

// @kotowari[REQ-297, REQ-308]
#[test]
fn print_plan_full_adds_the_merged_policy_the_environment_and_the_bwrap_arguments() {
    let (home, workspace) = home_with_workspace();
    home.write(".config/kakoi/profile/default.toml", RW_WORKSPACE);

    let output = binary(home.path())
        .env("KEPT", "as-is")
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--print-plan=full",
            "--",
            "/bin/true",
        ])
        .output()
        .unwrap();

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let plan = String::from_utf8(output.stdout).unwrap();
    assert!(plan.contains("policy (merged):\n"), "{report}");
    assert!(
        plan.contains("mounts.rw `${workspace}` (from the profile "),
        "{report}"
    );
    // The whole environment, a variable the policy did not touch included.
    assert!(plan.contains("\n  KEPT=as-is\n"), "{report}");
    assert!(plan.contains("command: /bin/true\n"), "{report}");
    assert!(
        plan.contains("bwrap arguments:\n  --ro-bind\n  /\n  /\n"),
        "{report}"
    );
    assert!(
        plan.contains("\n  --seccomp\n  <fd: seccomp filter>\n"),
        "{report}"
    );
}

// @kotowari[REQ-299, REQ-298, REQ-300, REQ-301]
#[test]
fn print_plan_json_is_one_document_with_the_keys_of_the_contract() {
    // The JSON form is for tools: the keys of specification section 13, secret values as
    // `null`, the descriptors as tagged objects, and `command` as `null` when `COMMAND` is
    // left out.
    let (home, workspace) = home_with_workspace();
    home.write(".config/kakoi/secrets/token", "FAKE-TOKEN-VALUE\n");
    home.write(
        ".config/kakoi/profile/default.toml",
        format!("{RW_WORKSPACE}[secrets]\nTOKEN = \"${{config_dir}}/secrets/token\"\n"),
    );
    let workspace = workspace.to_str().unwrap();

    let output = run(
        home.path(),
        [
            "--workspace",
            workspace,
            "--print-plan=json",
            "--",
            "/bin/true",
            "one",
        ],
    );

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("FAKE-TOKEN-VALUE"),
        "{report}"
    );
    // One line: the form is for tools, which read it whole.
    assert_eq!(
        output.stdout.iter().filter(|byte| **byte == b'\n').count(),
        1,
        "{report}"
    );
    assert!(output.stdout.ends_with(b"\n"), "{report}");
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    for key in [
        "format_version",
        "nested",
        "applied",
        "policy_sources",
        "variables",
        "home",
        "policy",
        "mounts",
        "skipped_mounts",
        "left_visible",
        "skipped_paths",
        "guards",
        "skipped_guards",
        "environment",
        "environment_changes",
        "command",
        "bwrap",
        "bwrap_arguments",
    ] {
        assert!(plan.get(key).is_some(), "{key} is missing: {report}");
    }
    assert_eq!(plan["format_version"], 1, "{report}");
    assert_eq!(plan["nested"], false, "{report}");
    assert_eq!(plan["applied"], true, "{report}");
    assert_eq!(plan["policy_sources"][0]["kind"], "file", "{report}");
    assert_eq!(plan["variables"]["workspace"], workspace, "{report}");
    assert_eq!(
        plan["variables"]["git_common_dir"],
        serde_json::Value::Null,
        "{report}"
    );
    assert_eq!(
        plan["policy"]["mounts"][0]["path"], "${workspace}",
        "{report}"
    );
    assert_eq!(
        plan["policy"]["mounts"][0]["origin"]["kind"], "profile",
        "{report}"
    );
    let mounts = plan["mounts"].as_array().unwrap();
    let workspace_item = mounts
        .iter()
        .find(|item| item["path"] == workspace)
        .unwrap_or_else(|| panic!("no workspace item: {report}"));
    assert_eq!(workspace_item["directive"], "rw", "{report}");
    assert_eq!(workspace_item["kind"], "directory", "{report}");
    assert!(
        mounts.iter().any(|item| item["origin"]["kind"] == "secret"
            && item["origin"]["name"] == "TOKEN"
            && item["kind"] == "not-directory"),
        "{report}"
    );
    assert_eq!(
        plan["environment"]["TOKEN"],
        serde_json::Value::Null,
        "{report}"
    );
    assert_eq!(plan["environment"]["KAKOI"], "1", "{report}");
    assert_eq!(plan["environment_changes"]["mode"], "inherit", "{report}");
    assert_eq!(
        plan["environment_changes"]["secrets"][0], "TOKEN",
        "{report}"
    );
    assert_eq!(plan["environment_changes"]["set"]["KAKOI"], "1", "{report}");
    assert_eq!(plan["command"]["given"], "/bin/true", "{report}");
    assert_eq!(plan["command"]["arguments"][0], "one", "{report}");
    assert_eq!(plan["command"]["path"], "/bin/true", "{report}");
    let arguments = plan["bwrap_arguments"].as_array().unwrap();
    assert_eq!(arguments[0]["kind"], "literal", "{report}");
    assert_eq!(arguments[0]["value"], "--ro-bind", "{report}");
    assert!(
        arguments
            .iter()
            .any(|argument| argument["kind"] == "seccomp-filter"),
        "{report}"
    );
    assert!(
        arguments
            .iter()
            .any(|argument| argument["kind"] == "empty-file"),
        "{report}"
    );
    assert_eq!(arguments.last().unwrap()["value"], "one", "{report}");

    let without_a_command = run(home.path(), ["--workspace", workspace, "--print-plan=json"]);

    let report = output_report(&without_a_command);
    assert_eq!(without_a_command.status.code(), Some(0), "{report}");
    let plan: serde_json::Value = serde_json::from_slice(&without_a_command.stdout).unwrap();
    assert_eq!(plan["command"], serde_json::Value::Null, "{report}");
}

// @kotowari[REQ-296]
#[test]
fn the_summary_shows_the_changes_to_the_environment_and_shortens_the_home() {
    // The summary shows how the environment differs from the host's rather than the whole
    // of it, masks the secrets, shows the home directory as `~` in the mount items, and
    // notes the origin only of items that did not come from the global scope
    // (specification section 13).
    let (home, workspace) = home_with_workspace();
    home.write("bin/.keep", "");
    home.write("cache/.keep", "");
    home.write(".config/kakoi/secrets/token", "FAKE-TOKEN-VALUE\n");
    home.write(
        ".config/kakoi/profile/default.toml",
        format!(
            "{RW_WORKSPACE}[env]\nunset = [\"*_SECRET\"]\nset = {{ ADDED = \"1\" }}\n\
             path-prepend = [\"~/bin\"]\n\
             [secrets]\nTOKEN = \"${{config_dir}}/secrets/token\"\n"
        ),
    );
    let cache = home.path().join("cache");

    let output = binary(home.path())
        .env("PATH", "/usr/bin:/bin")
        .env("KEPT", "as-is")
        .env("MY_SECRET", "gone")
        .env("TOKEN", "from-the-host")
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--rw",
            cache.to_str().unwrap(),
            "--print-plan",
            "--",
            "/bin/true",
        ])
        .output()
        .unwrap();

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let plan = String::from_utf8(output.stdout).unwrap();
    assert!(plan.contains("\nnetwork: host\n"), "{report}");
    assert!(plan.contains("\n  rw      ~/ws\n"), "{report}");
    assert!(
        plan.contains("\n  rw      ~/cache (command line)\n"),
        "{report}"
    );
    // Three variables are as on the host: HOME, XDG_CONFIG_HOME, and KEPT.
    assert!(
        plan.contains("\nenvironment (inherit): 3 variables as on the host, and:\n"),
        "{report}"
    );
    assert!(plan.contains("\n  unset  MY_SECRET\n"), "{report}");
    assert!(plan.contains("\n  set    ADDED=1\n"), "{report}");
    assert!(
        plan.contains(&format!(
            "\n  set    PATH={}:<the host's PATH>\n",
            home.path().join("bin").display()
        )),
        "{report}"
    );
    assert!(plan.contains("\n  set    KAKOI=1\n"), "{report}");
    assert!(
        plan.contains("\n  secret TOKEN (value not shown)\n"),
        "{report}"
    );
    assert!(!plan.contains("FAKE-TOKEN-VALUE"), "{report}");
    assert!(!plan.contains("from-the-host"), "{report}");
    assert!(!plan.contains("KEPT"), "{report}");
    assert!(
        plan.ends_with(
            "--print-plan=json is the same as one line of JSON, for LLM agents and tools)\n"
        ),
        "{report}"
    );
}

// @kotowari[REQ-289, EX-525]
#[test]
fn a_control_character_in_a_warning_is_escaped() {
    let (home, workspace) = home_with_workspace();
    // A secret whose file does not exist raises the warning of specification section 9,
    // which embeds the key name; the name carries an ESC.
    home.write(
        ".config/kakoi/profile/default.toml",
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
    assert!(stderr.starts_with("kakoi: warning: "), "{report}");
    assert!(!output.stderr.contains(&0x1b), "{report}");
    assert!(!output.stdout.contains(&0x1b), "{report}");
}

// @kotowari[REQ-274]
#[test]
fn secret_values_never_reach_stdout_or_stderr() {
    let (home, workspace) = home_with_workspace();
    let value = "FAKE-SECRET-VALUE-not-a-real-credential";
    home.write(".config/kakoi/secrets/token", format!("{value}\n"));
    // `GIT_CONFIG_COUNT` as a secret with a non-numeric value: the `env` diagnostic of
    // specification section 10 names the variable and must not show its value.
    home.write(".config/kakoi/secrets/count", format!("{value}\n"));
    home.write(
        ".config/kakoi/profile/default.toml",
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

// @kotowari[REQ-305]
#[test]
fn print_plan_is_identical_across_two_runs() {
    let (home, workspace) = home_with_workspace();
    home.write("ws/.env", "SECRET=x\n");
    home.write("ws/sub/.env.local", "SECRET=y\n");
    home.write("ws/sub/keep.txt", "");
    home.write("cache/.keep", "");
    home.write("notes/a.md", "");
    home.write(".config/kakoi/secrets/token", "FAKE-TOKEN\n");
    home.write(
        ".config/kakoi/profile/default.toml",
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

// @kotowari[REQ-154, EX-356]
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
            String::from_utf8_lossy(&output.stdout).contains("kakoi init"),
            "{name}: {report}"
        );
    }
}

// @kotowari[REQ-158]
#[test]
fn a_missing_configuration_directory_inside_a_writable_item_is_refused() {
    // The built-in default makes the workspace writable, so a configuration directory named
    // under it could be made from inside the isolation and a profile planted where the next
    // start reads one. Nothing is at the name yet, but its deepest existing ancestor is, and
    // that is what the check looks at (specification section 5.6).
    let (home, workspace) = home_without_a_configuration_directory();

    let output = binary(home.path())
        .env("XDG_CONFIG_HOME", workspace.join("cfg"))
        .current_dir(&workspace)
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--print-plan",
            "--",
            "/bin/true",
        ])
        .output()
        .unwrap();

    assert_diagnostic(&output, 125, "path");
}

// @kotowari[REQ-154]
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

// @kotowari[REQ-154]
#[test]
fn a_broken_default_toml_or_configuration_directory_is_a_policy_diagnostic() {
    // Only a name that is not there falls back: a `default.toml` or a configuration
    // directory that exists but cannot be followed stops the run, so a policy that broke is
    // never replaced by the wider built-in default (specification section 5.3).
    let arrangements: [Arrangement; 4] = [
        ("a broken link at default.toml", |home| {
            std::fs::create_dir_all(home.join(".config/kakoi/profile")).unwrap();
            std::os::unix::fs::symlink(
                home.join("nowhere"),
                home.join(".config/kakoi/profile/default.toml"),
            )
            .unwrap();
        }),
        ("a broken link at the configuration directory", |home| {
            std::fs::create_dir_all(home.join(".config")).unwrap();
            std::os::unix::fs::symlink(home.join("nowhere"), home.join(".config/kakoi")).unwrap();
        }),
        ("a regular file at the configuration directory", |home| {
            std::fs::create_dir_all(home.join(".config")).unwrap();
            std::fs::write(home.join(".config/kakoi"), "").unwrap();
        }),
        ("a regular file at profile/", |home| {
            std::fs::create_dir_all(home.join(".config/kakoi")).unwrap();
            std::fs::write(home.join(".config/kakoi/profile"), "").unwrap();
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
            String::from_utf8_lossy(&output.stderr).starts_with("kakoi: policy: "),
            "{name}: {report}"
        );
    }
}

/// Puts `mode` back on `path` when the test leaves, whether it passed or panicked, so that
/// a directory a test made unsearchable never outlives it.
struct RestoredMode(PathBuf, u32);

impl Drop for RestoredMode {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(self.1));
    }
}

// @kotowari[REQ-154]
#[test]
fn an_unsearchable_configuration_directory_is_not_taken_for_an_absent_one() {
    // A configuration directory whose search bit an installer or an archive left off is
    // there: its `default.toml` cannot be looked at, which is not the same as never having
    // been written out. The run stops instead of standing the wider built-in default in for
    // the profile that is on disk (specification section 5.3).
    let (home, workspace) = home_without_a_configuration_directory();
    home.write(".config/kakoi/profile/default.toml", RW_WORKSPACE);
    let config_dir = home.path().join(".config/kakoi");
    std::fs::set_permissions(&config_dir, std::fs::Permissions::from_mode(0o000)).unwrap();
    let _restore = RestoredMode(config_dir, 0o700);

    let output = binary(home.path())
        .current_dir(&workspace)
        .args(["--print-plan", "--", "/bin/true"])
        .output()
        .unwrap();

    assert_diagnostic(&output, 125, "policy");
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("kakoi init"),
        "{}",
        output_report(&output)
    );
}

// @kotowari[REQ-154]
#[test]
fn a_policy_file_overlays_the_built_in_default() {
    // The built-in default adds no layer: `--policy-file` stacks on it as it would on a
    // profile file, and the run still reports the built-in default (specification
    // sections 5.2 and 5.3). What becomes of its valueless `${config_dir}` item is fixed on
    // the structured value by `plan.rs::a_missing_configuration_directory_leaves_config_dir_valueless`;
    // the plan's display form is not a contract of 0.1 (specification section 13).
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
    assert!(plan.contains("kakoi init"), "{report}");
}

// @kotowari[REQ-154]
#[test]
fn a_present_default_toml_replaces_the_built_in_default() {
    // Once `default.toml` is there it is the whole global scope; the built-in default is
    // not read beside it (specification section 5.3).
    let (home, workspace) = home_without_a_configuration_directory();
    let profile = home.write(".config/kakoi/profile/default.toml", RW_WORKSPACE);

    let output = binary(home.path())
        .current_dir(&workspace)
        .args(["--print-plan", "--", "/bin/true"])
        .output()
        .unwrap();

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let plan = String::from_utf8_lossy(&output.stdout);
    assert!(!plan.contains("kakoi init"), "{report}");
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

// @kotowari[REQ-256, EX-496]
#[test]
fn init_writes_the_built_in_default_and_prints_its_path() {
    let home = TempDir::new();
    let before = tree(home.path());
    // `/tmp/kakoi` is the user's or the shim's to make; `init` does not touch it
    // (specification section 14).
    let shared = Path::new("/tmp/kakoi");
    let shared_before = shared.symlink_metadata().is_ok();

    let output = run(home.path(), ["init"]);

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert!(output.stderr.is_empty(), "{report}");
    let written = home.path().join(".config/kakoi/profile/default.toml");
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
    let secrets = home.path().join(".config/kakoi/secrets");
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
            ".config/kakoi",
            ".config/kakoi/profile",
            ".config/kakoi/profile/default.toml",
            ".config/kakoi/secrets",
        ]
        .map(PathBuf::from),
        "{report}"
    );
    assert_eq!(shared.symlink_metadata().is_ok(), shared_before, "{report}");
}

// @kotowari[REQ-256]
#[test]
fn init_takes_a_profile_name() {
    let home = TempDir::new();

    let output = run(home.path(), ["init", "strict"]);

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let written = home.path().join(".config/kakoi/profile/strict.toml");
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

// @kotowari[REQ-257, EX-497]
#[test]
fn init_narrows_an_existing_secrets_directory_to_0700() {
    // The install instructions before `init` had the user make the directory by hand, where
    // the usual umask leaves it 0755. `init` sets the mode on a `secrets/` that is already
    // there, so following the current instructions does not leave a wide one behind
    // (specification section 4.1).
    let home = TempDir::new();
    let secrets = home.path().join(".config/kakoi/secrets");
    std::fs::create_dir_all(&secrets).unwrap();
    std::fs::set_permissions(&secrets, std::fs::Permissions::from_mode(0o755)).unwrap();

    let output = run(home.path(), ["init"]);

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert_eq!(
        std::fs::metadata(&secrets).unwrap().permissions().mode() & 0o7777,
        0o700,
        "{report}"
    );
}

// @kotowari[REQ-259]
#[test]
fn init_rejects_a_bad_name_or_extra_arguments() {
    // `init` takes at most a NAME, and the NAME is one path component without a control
    // character: anything else, including an option or a command, is a usage diagnostic
    // (specification section 4.1).
    for arguments in [
        ["init", "../x"].as_slice(),
        &["init", "a\nb"],
        &["init", "--", "sh"],
        &["init", "--print-plan"],
    ] {
        let diagnostic = interpret(arguments.iter().map(OsString::from)).unwrap_err();

        assert_eq!(diagnostic.kind(), Kind::Usage, "{arguments:?}");
    }
}

// @kotowari[REQ-258]
#[test]
fn init_refuses_to_overwrite_an_existing_profile() {
    // There is no `--force`: the boundary the user wrote is never replaced by the product,
    // so the way to rewrite it is to remove it first (specification section 4.1).
    let home = TempDir::new();
    let written = home.write(".config/kakoi/profile/default.toml", "# mine\n");

    let output = run(home.path(), ["init"]);

    let diagnostic = assert_diagnostic(&output, 125, "path");
    assert!(
        diagnostic.contains(written.to_str().unwrap()),
        "{diagnostic}"
    );
    assert_eq!(std::fs::read_to_string(&written).unwrap(), "# mine\n");
}

// @kotowari[REQ-258]
#[test]
fn init_refuses_a_broken_link_or_a_regular_file_in_the_way() {
    // The name written to is not followed, so a broken link there is something that already
    // exists; the components above it are followed and must end at directories
    // (specification section 4.1).
    let arrangements: [(Arrangement, &str); 2] = [
        (
            ("a broken link at the file", |home| {
                std::fs::create_dir_all(home.join(".config/kakoi/profile")).unwrap();
                std::os::unix::fs::symlink(
                    home.join("nowhere"),
                    home.join(".config/kakoi/profile/default.toml"),
                )
                .unwrap();
            }),
            ".config/kakoi/profile/default.toml",
        ),
        (
            ("a regular file at profile/", |home| {
                std::fs::create_dir_all(home.join(".config/kakoi")).unwrap();
                std::fs::write(home.join(".config/kakoi/profile"), "").unwrap();
            }),
            ".config/kakoi/profile",
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

// @kotowari[REQ-258]
#[test]
fn init_follows_a_linked_configuration_directory_and_prints_the_written_path() {
    // A user keeps the configuration directory in dotfiles behind a link: the file lands at
    // the target, and the line printed is the path as assembled, link and all
    // (specification section 4.1).
    let home = TempDir::new();
    let target = home.path().join("dotfiles/kakoi");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::create_dir_all(home.path().join(".config")).unwrap();
    std::os::unix::fs::symlink(&target, home.path().join(".config/kakoi")).unwrap();

    let output = run(home.path(), ["init"]);

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let real_home = home.path().canonicalize().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!(
            "{}\n",
            real_home
                .join(".config/kakoi/profile/default.toml")
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

// @kotowari[REQ-256]
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
                .join(".config/kakoi/profile/default.toml")
                .display()
        ),
        "{report}"
    );
}

// @kotowari[REQ-259]
#[test]
fn init_ignores_the_current_directory_and_bwrap() {
    // From a directory that is gone and on a machine without `bwrap`, the configuration
    // can still be put in place (specification section 4.1). Inside an isolation is the
    // nesting tests' part.
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

// @kotowari[REQ-259]
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

// @kotowari[REQ-154]
#[test]
fn the_built_in_default_starts_without_a_warning() {
    // Someone running on the built-in default has placed no secret file yet, and must not
    // be warned about one on every start (specification section 16).
    let (home, workspace) = home_without_a_configuration_directory();

    let output = binary(home.path())
        .current_dir(&workspace)
        .args(["--print-plan", "--", "/bin/true"])
        .output()
        .unwrap();

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert!(output.stderr.is_empty(), "{report}");
}

/// The plan the binary prints with `--print-plan=json`, after checking it exited 0.
fn json_plan(output: &std::process::Output) -> serde_json::Value {
    let report = output_report(output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| panic!("{error}: {report}"))
}

// @kotowari[EX-491]
#[test]
fn init_with_a_name_that_climbs_out_of_profile_is_a_usage_diagnostic() {
    let home = TempDir::new();

    let output = run(home.path(), ["init", "../x"]);

    assert_diagnostic(&output, 125, "usage");
}

// @kotowari[EX-492]
#[test]
fn repeated_rw_options_are_all_kept_in_the_order_given() {
    let (home, workspace) = home_with_workspace();
    let first = home.path().join("a");
    let second = home.path().join("b");

    for order in [[&first, &second], [&second, &first]] {
        let output = run(
            home.path(),
            [
                "--workspace",
                workspace.to_str().unwrap(),
                "--rw",
                order[0].to_str().unwrap(),
                "--rw",
                order[1].to_str().unwrap(),
                "--print-plan=json",
            ],
        );

        let plan = json_plan(&output);
        let from_the_command_line: Vec<&str> = plan["policy"]["mounts"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["origin"]["kind"] == "command-line")
            .map(|entry| {
                assert_eq!(entry["directive"], "rw");
                entry["path"].as_str().unwrap()
            })
            .collect();
        assert_eq!(
            from_the_command_line,
            [order[0].to_str().unwrap(), order[1].to_str().unwrap()],
            "{}",
            output_report(&output)
        );
    }
}

// @kotowari[EX-493]
#[test]
fn a_print_plan_form_given_as_a_separate_word_is_a_usage_diagnostic() {
    let (home, workspace) = home_with_workspace();

    let output = run(
        home.path(),
        [
            "--workspace",
            workspace.to_str().unwrap(),
            "--print-plan",
            "full",
        ],
    );

    assert_diagnostic(&output, 125, "usage");
}

// @kotowari[EX-494]
#[test]
fn a_tilde_in_a_command_line_path_is_a_name_under_the_current_directory() {
    let (home, workspace) = home_with_workspace();
    let current = home.path().join("cwd");
    std::fs::create_dir_all(current.join("~/x")).unwrap();
    std::fs::create_dir(home.path().join("x")).unwrap();

    let output = binary(home.path())
        .current_dir(&current)
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--hide",
            "~/x",
            "--print-plan=json",
        ])
        .output()
        .unwrap();

    let plan = json_plan(&output);
    let report = output_report(&output);
    let hidden: Vec<&str> = plan["mounts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["directive"] == "hide")
        .map(|item| item["path"].as_str().unwrap())
        .collect();
    let under_current = current.canonicalize().unwrap().join("~/x");
    assert_eq!(hidden, [under_current.to_str().unwrap()], "{report}");
}

// @kotowari[EX-498]
#[test]
fn init_leaves_the_target_of_a_broken_link_at_the_profile_uncreated() {
    let home = TempDir::new();
    let profile = home.path().join(".config/kakoi/profile/default.toml");
    std::fs::create_dir_all(profile.parent().unwrap()).unwrap();
    let target = home.path().join("target.toml");
    std::os::unix::fs::symlink(&target, &profile).unwrap();

    let output = run(home.path(), ["init"]);

    assert_diagnostic(&output, 125, "path");
    assert!(
        target.symlink_metadata().is_err(),
        "{}",
        output_report(&output)
    );
}

// @kotowari[EX-500, EX-526]
#[test]
fn a_command_that_is_nowhere_is_command_not_found_with_127() {
    let (home, workspace) = home_with_workspace();
    let absent = home.path().join("opt/tool");

    for command in [absent.to_str().unwrap(), "no-such-tool-anywhere"] {
        let output = run(
            home.path(),
            ["--workspace", workspace.to_str().unwrap(), "--", command],
        );

        assert_diagnostic(&output, 127, "command not found");
    }
}

// @kotowari[EX-530]
#[test]
fn a_collision_in_identity_comes_before_placement_and_secret_diagnostics() {
    // The two directives name one real path through different spellings, so only the
    // identity of stage 7 finds the collision; the policy file inside the `rw` workspace
    // and the empty secret would be diagnosed later in the same stage.
    let (home, workspace) = home_with_workspace();
    std::fs::create_dir(home.path().join("d")).unwrap();
    std::os::unix::fs::symlink(home.path().join("d"), home.path().join("d-link")).unwrap();
    home.write("empty-secret", "");
    home.write(
        ".config/kakoi/profile/default.toml",
        "[mounts]\nrw = [\"${workspace}\", \"~/d\"]\nhide = [\"~/d-link\"]\n\
         [secrets]\nEMPTY = \"~/empty-secret\"\n",
    );
    let policy_file = home.write("ws/p.toml", "");

    let output = run(
        home.path(),
        [
            "--workspace",
            workspace.to_str().unwrap(),
            "--policy-file",
            policy_file.to_str().unwrap(),
            "--print-plan",
        ],
    );

    assert_diagnostic(&output, 125, "policy");
}

// @kotowari[EX-531]
#[test]
fn of_two_bad_scan_links_the_first_in_byte_order_is_named() {
    // The scan roots are walked in the order written, `s2` first, so the link under `s2`
    // is found before the one under `s1`; both land in an `ro` item written through a link
    // inside the `rw` workspace, which could be re-pointed from inside.
    let (home, workspace) = home_with_workspace();
    home.write("t/x", "kept\n");
    std::os::unix::fs::symlink(home.path().join("t"), workspace.join("ro-link")).unwrap();
    let mut links = Vec::new();
    for root in ["s2", "s1"] {
        std::fs::create_dir(home.path().join(root)).unwrap();
        let link = home.path().join(root).join(".env");
        std::os::unix::fs::symlink(home.path().join("t/x"), &link).unwrap();
        links.push(link);
    }
    home.write(
        ".config/kakoi/profile/default.toml",
        format!(
            "[mounts]\nrw = [\"${{workspace}}\"]\nro = [\"{}\"]\n\
             [[mounts.scan]]\nroot = \"~/s2\"\nnames = [\".env\"]\n\
             [[mounts.scan]]\nroot = \"~/s1\"\nnames = [\".env\"]\n",
            workspace.join("ro-link").display()
        ),
    );

    let output = run(
        home.path(),
        ["--workspace", workspace.to_str().unwrap(), "--print-plan"],
    );

    let diagnostic = assert_diagnostic(&output, 125, "path");
    let (found_first, first_in_byte_order) = (&links[0], &links[1]);
    assert!(
        diagnostic.contains(first_in_byte_order.to_str().unwrap()),
        "{diagnostic}"
    );
    assert!(
        !diagnostic.contains(found_first.to_str().unwrap()),
        "{diagnostic}"
    );
}

// @kotowari[EX-532]
#[test]
fn the_summary_counts_a_variable_as_on_the_host_and_shows_an_added_one() {
    let (home, workspace) = home_with_workspace();
    home.write(
        ".config/kakoi/profile/default.toml",
        format!("{RW_WORKSPACE}[env]\nset = {{ NEW = \"new-value\" }}\n"),
    );
    let plan = |form: &str, kept: bool| {
        let mut command = binary(home.path());
        if kept {
            command.env("KEPT", "as-on-the-host");
        }
        command
            .args(["--workspace", workspace.to_str().unwrap(), form])
            .output()
            .unwrap()
    };

    let summary = |kept: bool| {
        let output = plan("--print-plan", kept);
        assert_eq!(output.status.code(), Some(0), "{}", output_report(&output));
        String::from_utf8(output.stdout).unwrap()
    };

    let with_kept = summary(true);
    assert!(with_kept.contains("NEW"), "{with_kept}");
    assert!(with_kept.contains("new-value"), "{with_kept}");
    assert!(!with_kept.contains("KEPT"), "{with_kept}");
    assert!(!with_kept.contains("as-on-the-host"), "{with_kept}");

    // Counted: the only difference KEPT makes is one number that grows by one.
    let without_kept = summary(false);
    let (with_parts, without_parts) = (digit_runs(&with_kept), digit_runs(&without_kept));
    assert_eq!(
        with_parts.len(),
        without_parts.len(),
        "{with_kept}\n{without_kept}"
    );
    let differences: Vec<(&str, &str)> = with_parts
        .iter()
        .zip(&without_parts)
        .filter(|(with, without)| with != without)
        .map(|(with, without)| (*with, *without))
        .collect();
    let [(with, without)] = differences[..] else {
        panic!("not one difference: {differences:?}\n{with_kept}\n{without_kept}");
    };
    let count = |part: &str| -> u64 {
        part.parse()
            .unwrap_or_else(|_| panic!("{part:?} is not a number\n{with_kept}\n{without_kept}"))
    };
    assert_eq!(
        count(with),
        count(without) + 1,
        "{with_kept}\n{without_kept}"
    );
}

/// `text` split into its maximal runs of ASCII digits and the runs between them, in order.
fn digit_runs(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    for (index, character) in text.char_indices().skip(1) {
        let previous = text[..index].chars().next_back().unwrap();
        if previous.is_ascii_digit() != character.is_ascii_digit() {
            parts.push(&text[start..index]);
            start = index;
        }
    }
    if start < text.len() {
        parts.push(&text[start..]);
    }
    parts
}

/// A home whose profile copies `~/conf` (a regular file and a FIFO in it) and injects the
/// secret `TOKEN` from `~/token`.
fn home_with_a_copy_and_a_secret(copied: &str, secret: &str) -> (TempDir, PathBuf) {
    let (home, workspace) = home_with_workspace();
    home.write("conf/copied", copied);
    let fifo = std::ffi::CString::new(home.path().join("conf/pipe").to_str().unwrap()).unwrap();
    // SAFETY: `mkfifo` reads the NUL-terminated path and makes the FIFO.
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    home.write("token", format!("{secret}\n"));
    home.write(
        ".config/kakoi/profile/default.toml",
        "[mounts]\nrw = [\"${workspace}\"]\nrw-copy = [\"~/conf\"]\n\
         [secrets]\nTOKEN = \"~/token\"\n",
    );
    (home, workspace)
}

// @kotowari[EX-534]
#[test]
fn a_json_plan_with_not_copied_and_copied_files_stays_at_format_version_1() {
    let (home, workspace) = home_with_a_copy_and_a_secret("content\n", "FAKE-TOKEN");

    let plan = json_plan(&run(
        home.path(),
        [
            "--workspace",
            workspace.to_str().unwrap(),
            "--print-plan=json",
        ],
    ));

    assert!(!plan["not_copied"].as_array().unwrap().is_empty(), "{plan}");
    assert!(
        plan["bwrap_arguments"]
            .as_array()
            .unwrap()
            .iter()
            .any(|argument| argument["kind"] == "copied-file"),
        "{plan}"
    );
    assert_eq!(plan["format_version"], 1, "{plan}");
}

// @kotowari[EX-535]
#[test]
fn a_json_plan_without_a_command_has_a_null_command_and_the_four_variables() {
    let (home, workspace) = home_with_workspace();

    let plan = json_plan(&run(
        home.path(),
        [
            "--workspace",
            workspace.to_str().unwrap(),
            "--print-plan=json",
        ],
    ));

    assert_eq!(plan["command"], serde_json::Value::Null, "{plan}");
    let names: std::collections::BTreeSet<&str> = plan["variables"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        names,
        ["config_dir", "git_common_dir", "workspace", "worktree"].into(),
        "{plan}"
    );
}

// @kotowari[EX-536]
#[test]
fn a_json_plan_shows_the_kind_and_length_of_descriptors_but_no_secret_or_copied_content() {
    let copied = "COPIED-CONTENT-not-to-be-shown\n";
    let secret = "FAKE-SECRET-VALUE-not-a-real-credential";
    let (home, workspace) = home_with_a_copy_and_a_secret(copied, secret);

    let output = run(
        home.path(),
        [
            "--workspace",
            workspace.to_str().unwrap(),
            "--print-plan=json",
            "--",
            "/bin/true",
        ],
    );

    let plan = json_plan(&output);
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(!text.contains(secret), "{text}");
    assert!(!text.contains(copied.trim_end()), "{text}");
    assert_eq!(plan["environment"]["TOKEN"], serde_json::Value::Null);
    let arguments = plan["bwrap_arguments"].as_array().unwrap();
    assert!(
        arguments
            .iter()
            .any(|argument| argument["kind"] == "copied-file" && argument["bytes"] == copied.len()),
        "{plan}"
    );
    for kind in ["seccomp-filter", "empty-file"] {
        assert!(
            arguments.iter().any(|argument| argument["kind"] == kind),
            "{kind}: {plan}"
        );
    }
}

// @kotowari[EX-537]
#[test]
fn a_mount_from_a_secret_carries_its_origin_kind_and_name_in_json() {
    let (home, workspace) = home_with_workspace();
    let token = home.write("token", "FAKE-TOKEN\n");
    home.write(
        ".config/kakoi/profile/default.toml",
        format!("{RW_WORKSPACE}[secrets]\nTOKEN = \"~/token\"\n"),
    );

    let plan = json_plan(&run(
        home.path(),
        [
            "--workspace",
            workspace.to_str().unwrap(),
            "--print-plan=json",
        ],
    ));

    let item = plan["mounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["path"] == token.canonicalize().unwrap().to_str().unwrap())
        .unwrap_or_else(|| panic!("the secret file is not mounted: {plan}"));
    assert_eq!(item["origin"]["kind"], "secret", "{plan}");
    assert_eq!(item["origin"]["name"], "TOKEN", "{plan}");
}

// @kotowari[EX-538]
#[test]
fn two_print_plans_keep_nothing_and_give_the_same_plan() {
    let (home, workspace) = home_with_workspace();
    home.write("ws/.env", "SECRET=x\n");
    home.write(".config/kakoi/secrets/token", "FAKE-TOKEN\n");
    home.write(
        ".config/kakoi/profile/default.toml",
        format!(
            "{RW_WORKSPACE}hide = [\"~/missing\"]\n\
             [[mounts.scan]]\nroot = \"${{workspace}}\"\nnames = [\".env\"]\n\
             [secrets]\nTOKEN = \"${{config_dir}}/secrets/token\"\n"
        ),
    );
    let arguments = [
        "--workspace",
        workspace.to_str().unwrap(),
        "--print-plan=full",
        "--",
        "/bin/true",
    ];
    let before = tree(home.path());

    let first = run(home.path(), arguments);
    let after_first = tree(home.path());
    let second = run(home.path(), arguments);

    let report = format!("{}\n{}", output_report(&first), output_report(&second));
    assert_eq!(first.status.code(), Some(0), "{report}");
    assert_eq!(second.status.code(), Some(0), "{report}");
    assert_eq!(after_first, before, "{report}");
    assert_eq!(tree(home.path()), before, "{report}");
    assert_eq!(first.stdout, second.stdout, "{report}");
}

fn listed_example() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/profile/listed.toml"
    ))
    .unwrap()
}

// @kotowari[EX-937, REQ-482, REQ-256]
#[test]
fn ex_937_init_writes_the_example_chosen() {
    for (arguments, expected) in [
        (
            ["init", "work", "--example", "listed"].as_slice(),
            listed_example(),
        ),
        (&["init", "work", "--example=listed"], listed_example()),
        (&["init", "--example", "listed", "work"], listed_example()),
        (&["init", "work", "--example=default"], bundled_profile()),
        (&["init", "work"], bundled_profile()),
    ] {
        let home = TempDir::new();

        let output = run(home.path(), arguments);

        let report = output_report(&output);
        assert_eq!(output.status.code(), Some(0), "{arguments:?}: {report}");
        let written = home.path().join(".config/kakoi/profile/work.toml");
        assert_eq!(std::fs::read(&written).unwrap(), expected, "{arguments:?}");
    }
    let home = TempDir::new();
    let output = run(home.path(), ["init", "--example", "listed"]);
    assert_eq!(output.status.code(), Some(0), "{}", output_report(&output));
    assert_eq!(
        std::fs::read(home.path().join(".config/kakoi/profile/default.toml")).unwrap(),
        listed_example()
    );
}

// @kotowari[EX-938, REQ-482, REQ-259]
#[test]
fn ex_938_an_unknown_missing_or_repeated_example_is_a_usage_diagnostic_writing_nothing() {
    for arguments in [
        ["init", "work", "--example", "strict"].as_slice(),
        &["init", "work", "--example=strict"],
        &["init", "work", "--example"],
        &["init", "work", "--example="],
        &["init", "work", "--example", "listed", "--example", "listed"],
        &["init", "work", "--example=default", "--example=listed"],
        &["init", "work", "--example", "listed", "extra"],
    ] {
        let home = TempDir::new();

        let output = run(home.path(), arguments);

        assert_diagnostic(&output, 125, "usage");
        assert!(
            !home.path().join(".config").exists(),
            "{arguments:?} wrote something"
        );
    }
}

// @kotowari[EX-939, REQ-482]
#[test]
fn ex_939_example_outside_init_is_a_usage_diagnostic() {
    for arguments in [
        ["--example", "listed", "--", "/bin/true"].as_slice(),
        &["--example=listed", "--print-plan"],
        &["--version", "--example=listed"],
        &["--help", "--example", "listed"],
    ] {
        let diagnostic = interpret(arguments.iter().map(OsString::from)).unwrap_err();

        assert_eq!(diagnostic.kind(), Kind::Usage, "{arguments:?}");
    }
}
