//! The command guard: the rules of `[[commands.guard]]`, how they match, and the guard
//! placed inside the isolation.

mod common;

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::process::Output;

use common::{assert_diagnostic, home_with_workspace, output_report, run, TempDir};
use kakoi_core::guard::{evaluate, GuardRule};
use kakoi_core::layers::{merge, Layer, LayerOrigin};
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

// @kotowari[EX-859]
#[test]
fn ex_859_an_example_that_does_not_match_as_expected_stops_the_run() {
    let output = print_plan(
        r#"
[[commands.guard]]
program = "git"
deny = [["push"]]
reason = "push は人が行う"
examples.deny = ["git -C dir push"]
"#,
    );

    let diagnostic = assert_diagnostic(&output, 125, "policy");
    assert!(diagnostic.contains("git -C dir push"), "{diagnostic}");
}

// @kotowari[EX-860]
#[test]
fn ex_860_a_rule_that_matches_its_examples_loads() {
    let output = print_plan(
        r#"
[[commands.guard]]
program = "git"
deny = [["push"]]
reason = "push は人が行う"
examples.deny = ["git push"]
examples.allow = ["git commit -m 'push fix'"]
"#,
    );

    assert_loads(&output);
}

// @kotowari[EX-875]
#[test]
fn ex_875_an_example_is_checked_in_its_own_leading_environment_only() {
    let (home, workspace) = home_with_workspace();
    let policy_file = home.write(
        "policy.toml",
        r#"
[[commands.guard]]
program = "git"
deny-env = ["GIT_CONFIG_*"]
reason = "別名は作らない"
examples.deny = ["GIT_CONFIG_COUNT=1 git status"]
examples.allow = ["git status"]
"#,
    );

    let output = common::binary(home.path())
        .env("GIT_CONFIG_COUNT", "1")
        .arg("--workspace")
        .arg(&workspace)
        .arg("--policy-file")
        .arg(&policy_file)
        .arg("--print-plan")
        .output()
        .unwrap();

    assert_loads(&output);
}

// @kotowari[EX-876]
#[test]
fn ex_876_a_broken_regular_expression_stops_the_run() {
    let output = print_plan(
        r#"
[[commands.guard]]
program = "git"
deny = [["/pu(sh/"]]
reason = "push は人が行う"
"#,
    );

    assert_diagnostic(&output, 125, "policy");
}

// @kotowari[REQ-444]
#[test]
fn an_example_for_another_program_an_allowed_example_denied_or_unbalanced_quotes_stop_the_run() {
    for examples in [
        "examples.deny = [\"hg push\"]",
        "examples.allow = [\"git push origin\"]",
        "examples.allow = [\"git commit -m 'push\"]",
        "examples.deny = [\"GIT_DIR=x\"]",
    ] {
        let output = print_plan(&format!(
            "[[commands.guard]]\nprogram = \"git\"\ndeny = [[\"push\"]]\nreason = \"r\"\n{examples}\n"
        ));

        let diagnostic = assert_diagnostic(&output, 125, "policy");
        assert!(diagnostic.contains("git"), "{diagnostic}");
    }
}

// @kotowari[REQ-443, EX-861]
#[test]
fn ex_861_an_upper_layer_rule_does_not_remove_a_lower_layer_rule() {
    let layer = |origin: LayerOrigin, text: &str| Layer {
        policy: parse_policy(text, Path::new("/policy.toml")).unwrap(),
        origin,
    };
    let lower = layer(
        LayerOrigin::BuiltInDefault,
        "[[commands.guard]]\nprogram = \"git\"\ndeny = [[\"push\"]]\nreason = \"r\"\n",
    );
    let upper = layer(
        LayerOrigin::CommandLine,
        "[[commands.guard]]\nprogram = \"git\"\ndeny = [[\"fetch\"]]\nreason = \"r\"\n",
    );

    let policy = merge(&[lower, upper]).unwrap();

    let rules: Vec<GuardRule> = policy.guards.into_iter().map(|entry| entry.rule).collect();
    assert!(denies_with(&rules, "git push", &[]));
    assert!(denies_with(&rules, "git fetch", &[]));
}

/// A program that stands in for the real one: prints its name and arguments.
const FAKE_PROGRAM: &str = "#!/bin/sh\necho \"real $0 $*\"\n";

/// A home with the `RW_WORKSPACE` profile, a workspace, and a directory `bin` of fake
/// programs that the policy puts in front of `PATH`.
struct Scene {
    home: TempDir,
    workspace: PathBuf,
    bin: PathBuf,
}

impl Scene {
    fn new(programs: &[&str]) -> Self {
        let (home, workspace) = home_with_workspace();
        let bin = home.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        for program in programs {
            common::write_executable(&bin.join(program), FAKE_PROGRAM);
        }
        Self {
            home,
            workspace,
            bin,
        }
    }

    /// Writes the policy file: `bin` in front of `PATH`, then `rest`.
    fn policy(&self, rest: &str) -> PathBuf {
        self.home.write(
            "policy.toml",
            format!("[env]\npath-prepend = [\"{}\"]\n{rest}", self.bin.display()),
        )
    }

    /// The binary on the workspace with the policy file and `arguments`.
    fn command(&self, policy: &Path, arguments: &[&str]) -> std::process::Command {
        let mut command = common::binary(self.home.path());
        command
            .arg("--workspace")
            .arg(&self.workspace)
            .arg("--policy-file")
            .arg(policy)
            .args(arguments);
        command
    }

    fn run(&self, policy: &Path, arguments: &[&str]) -> Output {
        self.command(policy, arguments).output().unwrap()
    }

    /// The JSON plan, with `command` after `--` when not empty.
    fn json_plan(&self, policy: &Path, command: &[&str]) -> serde_json::Value {
        let mut arguments = vec!["--print-plan=json"];
        if !command.is_empty() {
            arguments.push("--");
            arguments.extend(command);
        }
        let output = self.run(policy, &arguments);
        assert_loads(&output);
        serde_json::from_slice(&output.stdout).unwrap()
    }
}

const GIT_PUSH: &str =
    "[[commands.guard]]\nprogram = \"git\"\ndeny = [[\"push\"]]\nreason = \"push は人が行う\"\n";

/// The entries of `key` (`guards` or `skipped_guards`) for `program`.
fn entries<'a>(
    plan: &'a serde_json::Value,
    key: &str,
    program: &str,
) -> Vec<&'a serde_json::Value> {
    plan[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} is not a list: {plan}"))
        .iter()
        .filter(|entry| entry["program"] == program)
        .collect()
}

/// The one guard placed for `program`.
fn guard<'a>(plan: &'a serde_json::Value, program: &str) -> &'a serde_json::Value {
    let guards = entries(plan, "guards", program);
    assert_eq!(guards.len(), 1, "{plan}");
    guards[0]
}

/// The one skipped guard of `program`, with a reason.
fn assert_skipped(plan: &serde_json::Value, program: &str) {
    let skipped = entries(plan, "skipped_guards", program);
    assert_eq!(skipped.len(), 1, "{plan}");
    assert!(
        skipped[0]["reason"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty()),
        "{plan}"
    );
    assert!(entries(plan, "guards", program).is_empty(), "{plan}");
}

fn host_path() -> String {
    std::env::var("PATH").unwrap()
}

// @kotowari[EX-870]
#[test]
fn ex_870_the_json_plan_shows_the_guard_location_and_the_rule_origin() {
    let scene = Scene::new(&["git"]);
    let policy = scene.policy(GIT_PUSH);

    let plan = scene.json_plan(&policy, &[]);

    let git = guard(&plan, "git");
    assert!(
        git["location"]
            .as_str()
            .is_some_and(|location| location.starts_with('/')),
        "{plan}"
    );
    let sources = git["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 1, "{plan}");
    assert_eq!(sources[0]["kind"], "policy-file", "{plan}");
    assert_eq!(sources[0]["path"], policy.to_str().unwrap(), "{plan}");
    assert_eq!(plan["format_version"], 1, "{plan}");
}

// @kotowari[REQ-446, REQ-268]
#[test]
fn the_guard_location_goes_first_on_path_in_front_of_path_prepend() {
    let scene = Scene::new(&["git"]);
    let policy = scene.policy(GIT_PUSH);

    let plan = scene.json_plan(&policy, &[]);

    let location = guard(&plan, "git")["location"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        plan["environment"]["PATH"],
        format!("{location}:{}:{}", scene.bin.display(), host_path()),
        "{plan}"
    );
}

// @kotowari[REQ-446]
#[test]
fn a_command_found_on_path_is_started_through_its_guard() {
    let scene = Scene::new(&["git"]);
    let policy = scene.policy(GIT_PUSH);

    let plan = scene.json_plan(&policy, &["git", "push"]);

    let location = guard(&plan, "git")["location"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(plan["command"]["path"], format!("{location}/git"), "{plan}");
    assert_eq!(plan["command"]["given"], "git", "{plan}");
}

// @kotowari[REQ-446, REQ-450]
#[test]
fn guard_absolute_path_relocates_the_real_program_and_the_plan_shows_where() {
    let scene = Scene::new(&["git", "hg"]);
    let policy = scene.policy(&format!(
        "{GIT_PUSH}guard-absolute-path = true\n\
         [[commands.guard]]\nprogram = \"hg\"\ndeny = [[\"push\"]]\nreason = \"r\"\n"
    ));

    let plan = scene.json_plan(&policy, &[]);

    let relocated = guard(&plan, "git")["relocated"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(relocated.starts_with('/'), "{plan}");
    assert_ne!(relocated, scene.bin.join("git").to_str().unwrap(), "{plan}");
    assert_eq!(
        guard(&plan, "hg")["relocated"],
        serde_json::Value::Null,
        "{plan}"
    );
}

// @kotowari[EX-868]
#[test]
fn ex_868_a_program_not_on_path_is_skipped_with_a_reason_and_the_run_goes_on() {
    let scene = Scene::new(&[]);
    let policy = scene
        .policy("[[commands.guard]]\nprogram = \"nosuchtool\"\ndeny = [[\"x\"]]\nreason = \"r\"\n");

    let plan = scene.json_plan(&policy, &[]);
    let summary = scene.run(&policy, &["--print-plan"]);
    let run = scene.run(&policy, &["--", "/bin/true"]);

    assert_skipped(&plan, "nosuchtool");
    assert_loads(&summary);
    assert!(
        String::from_utf8_lossy(&summary.stdout).contains("nosuchtool"),
        "{}",
        output_report(&summary)
    );
    assert_eq!(run.status.code(), Some(0), "{}", output_report(&run));
}

// @kotowari[REQ-268]
#[test]
fn path_is_left_as_it_is_when_every_guard_is_skipped() {
    let scene = Scene::new(&[]);
    let policy = scene
        .policy("[[commands.guard]]\nprogram = \"nosuchtool\"\ndeny = [[\"x\"]]\nreason = \"r\"\n");

    let plan = scene.json_plan(&policy, &[]);

    assert_eq!(
        plan["environment"]["PATH"],
        format!("{}:{}", scene.bin.display(), host_path()),
        "{plan}"
    );
}

// @kotowari[EX-869]
#[test]
fn ex_869_a_hidden_program_gets_no_guard() {
    let scene = Scene::new(&["git"]);
    let policy = scene.policy(&format!(
        "[mounts]\nhide = [\"{}\"]\n{GIT_PUSH}",
        scene.bin.join("git").display()
    ));

    let plan = scene.json_plan(&policy, &[]);

    assert_skipped(&plan, "git");
}

// @kotowari[EX-883]
#[test]
fn ex_883_without_path_no_guard_is_placed() {
    let scene = Scene::new(&["git"]);
    let policy = scene.home.write(
        "policy.toml",
        format!("[env]\nmode = \"clear\"\n{GIT_PUSH}"),
    );

    let plan = scene.json_plan(&policy, &[]);

    assert_skipped(&plan, "git");
    assert!(plan["environment"].get("PATH").is_none(), "{plan}");
}

// @kotowari[REQ-449]
#[test]
fn a_program_that_is_not_a_regular_file_gets_no_guard() {
    let scene = Scene::new(&[]);
    std::fs::create_dir(scene.bin.join("git")).unwrap();
    let policy = scene.policy(GIT_PUSH);

    let plan = scene.json_plan(&policy, &[]);

    assert_skipped(&plan, "git");
}

// @kotowari[REQ-449]
#[test]
fn a_program_that_is_kakoi_itself_gets_no_guard() {
    let scene = Scene::new(&[]);
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_kakoi"), scene.bin.join("git")).unwrap();
    let policy = scene.policy(GIT_PUSH);

    let plan = scene.json_plan(&policy, &[]);

    assert_skipped(&plan, "git");
}

// @kotowari[REQ-450]
#[test]
fn the_summary_and_the_full_plan_show_guards_and_skipped_guards_and_the_full_plan_the_rules() {
    let scene = Scene::new(&["git"]);
    let policy = scene.policy(&format!(
        "{GIT_PUSH}[[commands.guard]]\nprogram = \"nosuchtool\"\ndeny = [[\"x\"]]\nreason = \"never\"\n"
    ));
    let plan = scene.json_plan(&policy, &[]);
    let location = guard(&plan, "git")["location"]
        .as_str()
        .unwrap()
        .to_string();

    for form in ["--print-plan", "--print-plan=full"] {
        let output = scene.run(&policy, &[form]);
        assert_loads(&output);
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains(&location), "{text}");
        assert!(text.contains("nosuchtool"), "{text}");
    }
    let full = String::from_utf8(scene.run(&policy, &["--print-plan=full"]).stdout).unwrap();
    assert!(full.contains("push は人が行う"), "{full}");
    assert!(full.contains("never"), "{full}");
    assert!(full.contains(policy.to_str().unwrap()), "{full}");
}
