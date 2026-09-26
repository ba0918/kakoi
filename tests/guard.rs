//! The command guard: the rules of `[[commands.guard]]`, how they match, and the guard
//! placed inside the isolation.

mod common;

use std::process::Output;

use common::{assert_diagnostic, home_with_workspace, output_report, run, TempDir};

/// Writes `policy` as a `--policy-file` beside the `RW_WORKSPACE` profile and runs
/// `--print-plan` on the workspace.
fn print_plan(policy: &str) -> Output {
    let (home, workspace) = home_with_workspace();
    print_plan_in(&home, &workspace, policy)
}

fn print_plan_in(home: &TempDir, workspace: &std::path::Path, policy: &str) -> Output {
    let policy_file = home.write("policy.toml", policy);
    run(
        home.path(),
        [
            "--workspace".as_ref(),
            workspace.as_os_str(),
            "--policy-file".as_ref(),
            policy_file.as_os_str(),
            "--print-plan".as_ref(),
        ],
    )
}

/// Exit 0 and nothing on standard error.
fn assert_loads(output: &Output) {
    let report = output_report(output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert!(output.stderr.is_empty(), "{report}");
}

// @kotowari[EX-846]
#[test]
fn ex_846_a_rule_without_any_deny_form_is_a_policy_diagnostic() {
    let output = print_plan(
        r#"
[[commands.guard]]
program = "git"
reason = "push は人が行う"
"#,
    );

    assert_diagnostic(&output, 125, "policy");
}

// @kotowari[EX-847]
#[test]
fn ex_847_a_program_with_a_slash_is_a_policy_diagnostic() {
    let output = print_plan(
        r#"
[[commands.guard]]
program = "/usr/bin/git"
deny = [["push"]]
reason = "push は人が行う"
"#,
    );

    assert_diagnostic(&output, 125, "policy");
}

// @kotowari[EX-847]
#[test]
fn ex_847_the_program_kakoi_is_a_policy_diagnostic() {
    let output = print_plan(
        r#"
[[commands.guard]]
program = "kakoi"
deny = [["push"]]
reason = "push は人が行う"
"#,
    );

    assert_diagnostic(&output, 125, "policy");
}

// @kotowari[EX-847]
#[test]
fn ex_847_an_unknown_key_in_a_rule_is_a_policy_diagnostic() {
    let output = print_plan(
        r#"
[[commands.guard]]
program = "git"
deny = [["push"]]
allow = [["status"]]
reason = "push は人が行う"
"#,
    );

    assert_diagnostic(&output, 125, "policy");
}

// @kotowari[EX-871]
#[test]
fn ex_871_a_well_formed_rule_loads() {
    let output = print_plan(
        r#"
[[commands.guard]]
program = "git"
deny = [["push"]]
reason = "push は人が行う"
"#,
    );

    assert_loads(&output);
}

// @kotowari[REQ-438]
#[test]
fn empty_lists_sequences_program_and_reason_are_policy_diagnostics() {
    for rule in [
        "program = \"git\"\ndeny = []\nreason = \"r\"",
        "program = \"git\"\ndeny = [[]]\nreason = \"r\"",
        "program = \"git\"\ndeny = [[\"remote\", []]]\nreason = \"r\"",
        "program = \"git\"\ndeny = [[\"push\"]]\ndeny-flags = []\nreason = \"r\"",
        "program = \"git\"\ndeny = [[\"push\"]]\noptions-with-value = []\nreason = \"r\"",
        "program = \"git\"\ndeny = [[\"push\"]]\nfor = []\nreason = \"r\"",
        "program = \"git\"\ndeny-option-values = {}\nreason = \"r\"",
        "program = \"git\"\ndeny-option-values = { \"-c\" = [] }\nreason = \"r\"",
        "program = \"git\"\ndeny-env = []\nreason = \"r\"",
        "program = \"git\"\ndeny = [[\"push\"]]\nexamples.deny = []\nreason = \"r\"",
        "program = \"\"\ndeny = [[\"push\"]]\nreason = \"r\"",
        "program = \"git\"\ndeny = [[\"push\"]]\nreason = \"\"",
        "program = \"git\"\ndeny = [[\"push\"]]",
    ] {
        let output = print_plan(&format!("[[commands.guard]]\n{rule}\n"));

        assert_diagnostic(&output, 125, "policy");
    }
}
