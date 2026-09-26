//! The command guard: the rules of `[[commands.guard]]`, how they match, and the guard
//! placed inside the isolation.

mod common;

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::Path;
use std::process::Output;

use common::{assert_diagnostic, home_with_workspace, output_report, run, TempDir};
use kakoi_core::guard::{evaluate, GuardRule};
use kakoi_core::policy::parse_policy;

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

/// The one rule of `text`, the body of a `[[commands.guard]]` table.
fn rule(text: &str) -> GuardRule {
    let policy = parse_policy(
        &format!("[[commands.guard]]\n{text}\nreason = \"r\"\n"),
        Path::new("/policy.toml"),
    )
    .unwrap();
    policy.commands.guard.into_iter().next().unwrap()
}

/// The words of `command` after the program name.
fn arguments(command: &str) -> Vec<OsString> {
    command.split(' ').skip(1).map(OsString::from).collect()
}

/// Whether `rules` deny `command` with the environment variables `environment` present.
fn denies_with(rules: &[GuardRule], command: &str, environment: &[&str]) -> bool {
    let environment: Vec<OsString> = environment.iter().map(OsString::from).collect();
    evaluate(rules, &arguments(command), &environment).is_some()
}

fn denies(rule: &GuardRule, command: &str) -> bool {
    denies_with(std::slice::from_ref(rule), command, &[])
}

// @kotowari[EX-848]
#[test]
fn ex_848_a_plain_word_does_not_match_part_of_a_word() {
    let rule = rule("program = \"git\"\ndeny = [[\"push\"]]");

    assert!(!denies(&rule, "git pushx"));
}

// @kotowari[EX-849]
#[test]
fn ex_849_a_regular_expression_matches_the_whole_word() {
    let rule = rule("program = \"git\"\ndeny = [[\"/pu.h/\"]]");

    assert!(denies(&rule, "git push"));
    assert!(!denies(&rule, "git pushx"));
}

// @kotowari[REQ-439]
#[test]
fn a_word_that_is_not_utf8_matches_nothing() {
    let rule = rule("program = \"git\"\ndeny = [[\"/.*/\"]]");
    let word = OsString::from_vec(vec![b'p', 0xff]);

    assert!(evaluate(std::slice::from_ref(&rule), &[word], &[]).is_none());
}

// @kotowari[EX-850]
#[test]
fn ex_850_global_options_with_a_value_are_skipped_with_the_value() {
    let rule = rule("program = \"git\"\noptions-with-value = [\"-C\"]\ndeny = [[\"push\"]]");

    assert!(denies(&rule, "git --no-pager -C push push origin"));
}

// @kotowari[EX-851]
#[test]
fn ex_851_an_undeclared_option_takes_no_value() {
    let rule = rule("program = \"git\"\ndeny = [[\"push\"]]");

    assert!(!denies(&rule, "git -C dir push"));
}

// @kotowari[EX-852]
#[test]
fn ex_852_a_word_inside_the_arguments_is_not_a_prefix() {
    let rule = rule("program = \"git\"\ndeny = [[\"push\"]]");

    assert!(!denies(&rule, "git commit -m push"));
}

// @kotowari[EX-853]
#[test]
fn ex_853_alternatives_per_position_and_no_skipping_in_the_middle() {
    let rule = rule("program = \"git\"\ndeny = [[\"remote\", [\"add\", \"set-url\"]]]");

    assert!(denies(&rule, "git remote set-url origin x"));
    assert!(!denies(&rule, "git remote -v"));
    assert!(!denies(&rule, "git remote -v add origin x"));
}

// @kotowari[EX-872]
#[test]
fn ex_872_a_double_dash_ends_the_skipping() {
    let rule = rule("program = \"git\"\ndeny = [[\"push\"]]");

    assert!(denies(&rule, "git -- push"));
    assert!(!denies(&rule, "git -- -x push"));
}

// @kotowari[EX-854]
#[test]
fn ex_854_flags_match_in_a_bundle_and_before_an_equals_sign() {
    let rule = rule("program = \"git\"\ndeny-flags = [\"-f\", \"--force\"]");

    assert!(denies(&rule, "git push -fq"));
    assert!(denies(&rule, "git push --force=yes"));
}

// @kotowari[EX-855]
#[test]
fn ex_855_flags_after_a_double_dash_do_not_match() {
    let rule = rule("program = \"git\"\ndeny-flags = [\"--force\"]");

    assert!(!denies(&rule, "git log -- --force"));
}

// @kotowari[EX-856]
#[test]
fn ex_856_option_values_that_make_an_alias_are_denied() {
    let rule = rule(
        "program = \"git\"\noptions-with-value = [\"-c\", \"--config-env\"]\n\
         deny-option-values = { \"-c\" = [\"/alias[.].*/\"], \"--config-env\" = [\"/alias[.].*/\"] }",
    );

    assert!(denies(&rule, "git -c alias.p=push p"));
    assert!(denies(&rule, "git --config-env=alias.p=ENV p"));
    assert!(!denies(&rule, "git -c color.ui=false log"));
}

// @kotowari[EX-873]
#[test]
fn ex_873_flags_apply_only_to_a_run_that_matches_for() {
    let rule = rule("program = \"git\"\nfor = [[\"push\"]]\ndeny-flags = [\"-f\"]");

    assert!(denies(&rule, "git push -f"));
    assert!(!denies(&rule, "git checkout -f"));
}

// @kotowari[EX-857]
#[test]
fn ex_857_a_variable_matching_the_pattern_denies() {
    let rule = rule("program = \"git\"\ndeny-env = [\"GIT_CONFIG_*\"]");

    assert!(denies_with(
        std::slice::from_ref(&rule),
        "git status",
        &["GIT_CONFIG_COUNT"]
    ));
}

// @kotowari[EX-858]
#[test]
fn ex_858_no_matching_variable_does_not_deny() {
    let rule = rule("program = \"git\"\ndeny-env = [\"GIT_CONFIG_*\"]");

    assert!(!denies_with(
        std::slice::from_ref(&rule),
        "git status",
        &["GIT_DIR"]
    ));
}

// @kotowari[EX-874]
#[test]
fn ex_874_a_run_no_rule_matches_is_not_denied() {
    let rule = rule("program = \"git\"\ndeny = [[\"push\"]]");

    assert!(!denies(&rule, "git status"));
}

// @kotowari[REQ-443]
#[test]
fn deny_env_then_deny_then_flags_then_option_values_and_the_first_match_is_reported() {
    let rule = rule(
        "program = \"git\"\noptions-with-value = [\"-c\"]\ndeny-env = [\"GIT_DIR\"]\ndeny = [[\"push\"]]\n\
         deny-flags = [\"--force\"]\ndeny-option-values = { \"-c\" = [\"x\"] }",
    );
    let rules = std::slice::from_ref(&rule);
    let matched = |command: &str, environment: &[&str]| {
        let environment: Vec<OsString> = environment.iter().map(OsString::from).collect();
        evaluate(rules, &arguments(command), &environment).map(|denial| denial.matched)
    };

    assert_eq!(
        matched("git -c x push --force", &["GIT_DIR"]).as_deref(),
        Some("GIT_DIR")
    );
    assert_eq!(
        matched("git -c x push --force", &[]).as_deref(),
        Some("push")
    );
    assert_eq!(
        matched("git -c x fetch --force", &[]).as_deref(),
        Some("--force")
    );
    assert_eq!(matched("git -c x fetch", &[]).as_deref(), Some("-c x"));
    assert_eq!(matched("git fetch", &[]), None);
}
