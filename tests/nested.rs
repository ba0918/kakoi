//! Nesting: the mark an isolation carries, how kakoi tells a nested run by it, and what a
//! kakoi started inside an isolation of the built kakoi does.

use std::collections::BTreeSet;
use std::process::Output;

mod common;

use common::{
    assert_diagnostic, binary, home_with_workspace, output_report, run_inside, TempDir, KAKOI,
    RW_WORKSPACE,
};

/// The name of the nesting mark inside an isolation.
const MARK: &str = "/dev/kakoi-isolated";

/// Overwrites the `default` profile of `home` with `text`.
fn profile(home: &TempDir, text: &str) {
    home.write(".config/kakoi/profile/default.toml", text);
}

/// Standard error as lines.
fn stderr_lines(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .map(str::to_string)
        .collect()
}

/// Exit 0, and on standard error the warning of a nested run and nothing else from kakoi.
fn assert_nested_warning_only(output: &Output) {
    let report = output_report(output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let lines = stderr_lines(output);
    assert_eq!(lines.len(), 1, "{report}");
    assert!(lines[0].starts_with("kakoi: warning: "), "{report}");
}

// @kotowari[REQ-455]
#[test]
fn req_455_the_mark_is_a_regular_file_the_isolation_cannot_write_or_remove() {
    for mode in ["host", "none"] {
        let (home, workspace) = home_with_workspace();
        profile(
            &home,
            &format!("{RW_WORKSPACE}[network]\nmode = \"{mode}\"\n"),
        );

        let output = run_inside(
            home.path(),
            &workspace,
            &format!(
                "test -f {MARK} && test ! -L {MARK} && echo regular; \
                 (echo x > {MARK}) 2>/dev/null || echo not-writable; \
                 rm -f {MARK} 2>/dev/null || echo not-removable; \
                 mv {MARK} /dev/moved 2>/dev/null || echo not-movable; \
                 test -f {MARK} && test ! -s {MARK} && echo still-there"
            ),
        );

        let report = output_report(&output);
        assert_eq!(output.status.code(), Some(0), "{mode}: {report}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "regular\nnot-writable\nnot-removable\nnot-movable\nstill-there\n",
            "{mode}: {report}"
        );
    }
}

// @kotowari[REQ-315]
#[test]
fn req_315_the_mark_follows_the_dev_argument_in_the_plan() {
    let (home, workspace) = home_with_workspace();

    let output = binary(home.path())
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--print-plan=json",
            "--",
            "/bin/true",
        ])
        .output()
        .unwrap();

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let arguments = plan["bwrap_arguments"].as_array().unwrap();
    let dev = arguments
        .iter()
        .position(|argument| argument["value"] == "--dev")
        .unwrap_or_else(|| panic!("no --dev: {report}"));
    assert_eq!(arguments[dev + 1]["value"], "/dev", "{report}");
    assert_eq!(arguments[dev + 2]["value"], "--ro-bind-data", "{report}");
    assert_eq!(arguments[dev + 3]["kind"], "empty-file", "{report}");
    assert_eq!(arguments[dev + 4]["value"], MARK, "{report}");
}

// A nested run without `--nested=isolate`: the command runs under the outer boundary,
// started by kakoi itself.

// @kotowari[REQ-284]
#[test]
fn req_284_a_nested_launch_warns_and_runs_the_command_without_bwrap() {
    let (home, workspace) = home_with_workspace();

    // Without `bwrap` on its `PATH`, anything but the nested branch ends in a diagnostic.
    let output = run_inside(
        home.path(),
        &workspace,
        &format!(
            "exec /usr/bin/env PATH=/nonexistent {KAKOI} -- \
             /bin/sh -c 'echo out; echo err >&2; exit 3'"
        ),
    );

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(3), "{report}");
    assert_eq!(output.stdout, b"out\n", "{report}");
    let lines = stderr_lines(&output);
    assert_eq!(lines.len(), 2, "{report}");
    assert!(lines[0].starts_with("kakoi: warning: "), "{report}");
    assert_eq!(lines[1], "err", "{report}");
}

// @kotowari[REQ-284]
#[test]
fn req_284_a_nested_launch_leaves_the_environment_unchanged() {
    let (home, workspace) = home_with_workspace();

    // The same environment is printed once by `env` itself and once through kakoi.
    let output = run_inside(
        home.path(),
        &workspace,
        &format!(
            "/usr/bin/env MARKER='kept as is' PATH=/nonexistent /usr/bin/env; echo ---; \
             exec /usr/bin/env MARKER='kept as is' PATH=/nonexistent {KAKOI} -- /usr/bin/env"
        ),
    );

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    let (expected, inside) = stdout.split_once("---\n").unwrap();
    let set = |text: &str| text.lines().map(str::to_string).collect::<BTreeSet<_>>();
    assert!(expected.contains("MARKER=kept as is\n"), "{report}");
    assert_eq!(set(inside), set(expected), "{report}");
}

// @kotowari[REQ-261, REQ-290]
#[test]
fn req_261_a_nested_launch_resolves_the_command_on_the_host_path_and_exits_127_when_missing() {
    let (home, workspace) = home_with_workspace();
    let bin = home.write_executable("ws/bin/tool", "#!/bin/sh\nexit 7\n");
    let bin = bin.parent().unwrap().display();
    std::fs::create_dir(workspace.join("no-programs")).unwrap();

    let found = run_inside(
        home.path(),
        &workspace,
        &format!("exec /usr/bin/env PATH={bin} {KAKOI} -- tool"),
    );
    let report = output_report(&found);
    assert_eq!(found.status.code(), Some(7), "{report}");

    let missing = run_inside(
        home.path(),
        &workspace,
        &format!(
            "exec /usr/bin/env PATH={} {KAKOI} -- tool",
            workspace.join("no-programs").display()
        ),
    );
    let report = output_report(&missing);
    assert_eq!(missing.status.code(), Some(127), "{report}");
    assert!(missing.stdout.is_empty(), "{report}");
    let lines = stderr_lines(&missing);
    assert_eq!(lines.len(), 2, "{report}");
    assert!(lines[0].starts_with("kakoi: warning: "), "{report}");
    assert!(
        lines[1].starts_with("kakoi: command not found: "),
        "{report}"
    );
}

// @kotowari[REQ-263]
#[test]
fn req_263_a_nested_launch_passes_the_given_name_as_argv0() {
    // `sh` is a copy in a directory of its own, so argv[0] `sh` and the resolved path are
    // told apart.
    let (home, workspace) = home_with_workspace();
    std::fs::create_dir(workspace.join("bin")).unwrap();
    common::copy_executable(std::path::Path::new("/bin/sh"), &workspace.join("bin/sh"));

    let output = run_inside(
        home.path(),
        &workspace,
        &format!(
            "exec /usr/bin/env PATH={} {KAKOI} -- sh -c 'echo \"$0\"'",
            workspace.join("bin").display()
        ),
    );

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert_eq!(output.stdout, b"sh\n", "{report}");
}

// @kotowari[REQ-262, REQ-290, REQ-401, EX-502]
#[test]
fn ex_502_a_nested_launch_of_a_script_with_a_missing_interpreter_exits_126() {
    // The command is found, but its exec fails: one warning line, then `command not
    // executable`, exit code 126.
    let (home, workspace) = home_with_workspace();
    let script = home.write_executable("ws/tool", "#!/nonexistent/interpreter\n");

    let output = run_inside(
        home.path(),
        &workspace,
        &format!(
            "exec /usr/bin/env PATH=/nonexistent {KAKOI} -- {}",
            script.display()
        ),
    );

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(126), "{report}");
    assert!(output.stdout.is_empty(), "{report}");
    let lines = stderr_lines(&output);
    assert_eq!(lines.len(), 2, "{report}");
    assert!(lines[0].starts_with("kakoi: warning: "), "{report}");
    assert!(
        lines[1].starts_with("kakoi: command not executable: "),
        "{report}"
    );
}

// @kotowari[REQ-309]
#[test]
fn req_309_a_nested_launch_leaves_the_soft_limit_unchanged() {
    // The outer run raised the soft limit to the hard one; the shell lowers it again, and a
    // nested run, which makes no descriptors, leaves it there.
    let (home, workspace) = home_with_workspace();

    let output = run_inside(
        home.path(),
        &workspace,
        &format!("ulimit -Sn 1024 && exec {KAKOI} -- /bin/sh -c 'ulimit -Sn'"),
    );

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert_eq!(output.stdout, b"1024\n", "{report}");
}

// @kotowari[REQ-293]
#[test]
fn req_293_a_nested_launch_goes_from_the_grammar_to_the_command() {
    let (home, workspace) = home_with_workspace();

    // The grammar is still checked first.
    let usage = run_inside(
        home.path(),
        &workspace,
        &format!("exec {KAKOI} --bogus -- /bin/true"),
    );
    assert_diagnostic(&usage, 125, "usage");

    // From a directory that is gone and with a `HOME` that is a file, the checks after the
    // grammar are skipped and the command runs.
    let home_file = home.write("ws/home-file", "");
    let output = run_inside(
        home.path(),
        &workspace,
        &format!(
            "mkdir gone && cd gone && rmdir ../gone && \
             exec /usr/bin/env HOME={} {KAKOI} -- /bin/echo ran",
            home_file.display()
        ),
    );

    assert_nested_warning_only(&output);
    assert_eq!(output.stdout, b"ran\n", "{}", output_report(&output));
}

// @kotowari[EX-520]
#[test]
fn ex_520_a_nested_launch_execs_the_command_itself_with_the_environment_as_received() {
    let (home, workspace) = home_with_workspace();

    // The shell prints its own process ID and the environment `env` hands on; kakoi then
    // replaces the same process, and the command prints its process ID and the
    // environment it was started with (not its own, to which it adds `PWD`).
    let output = run_inside(
        home.path(),
        &workspace,
        &format!(
            "echo $$; /usr/bin/env MARKER='kept as is' /usr/bin/env; echo ---; \
             exec /usr/bin/env MARKER='kept as is' {KAKOI} -- \
             /bin/sh -c 'echo $$; /usr/bin/tr \"\\0\" \"\\n\" < /proc/$$/environ'"
        ),
    );

    let report = output_report(&output);
    assert_nested_warning_only(&output);
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    let (outer, inner) = stdout.split_once("---\n").unwrap();
    let (outer_pid, expected) = outer.split_once('\n').unwrap();
    let (inner_pid, inside) = inner.split_once('\n').unwrap();
    assert_eq!(inner_pid, outer_pid, "{report}");
    let set = |text: &str| text.lines().map(str::to_string).collect::<BTreeSet<_>>();
    assert!(expected.contains("MARKER=kept as is\n"), "{report}");
    assert_eq!(set(inside), set(expected), "{report}");
}

// A nested `--print-plan`: the policy is read and the plan is marked as nested.

// @kotowari[REQ-285]
#[test]
fn req_285_a_nested_print_plan_reads_the_policy_and_marks_the_plan_as_nested() {
    let (home, workspace) = home_with_workspace();
    let ws = workspace.display();

    // A nested plan reads the policy too: a named profile that is not there is a
    // diagnostic, not a plan.
    let without_a_profile = run_inside(
        home.path(),
        &workspace,
        &format!("exec {KAKOI} --workspace {ws} --profile strict --print-plan"),
    );
    assert_diagnostic(&without_a_profile, 125, "policy");

    // The full form: the nested plan carries what marks it as nested, which the plain plan
    // does not. The nested run is handed the environment the plain one gets, so that the
    // final environment the full form shows is no difference between them.
    let plain = binary(home.path())
        .current_dir(&workspace)
        .args(["--workspace", &ws.to_string(), "--print-plan=full"])
        .output()
        .unwrap();
    let nested = run_inside(
        home.path(),
        &workspace,
        &format!(
            "exec /usr/bin/env -i KAKOI=1 HOME=\"$HOME\" XDG_CONFIG_HOME=\"$XDG_CONFIG_HOME\" \
             PATH=\"$PATH\" {KAKOI} --workspace {ws} --print-plan=full"
        ),
    );

    let report = format!("{}\n{}", output_report(&plain), output_report(&nested));
    assert_eq!(plain.status.code(), Some(0), "{report}");
    assert_eq!(nested.status.code(), Some(0), "{report}");
    let plain = String::from_utf8(plain.stdout).unwrap();
    let nested = String::from_utf8(nested.stdout).unwrap();
    let marking: Vec<&str> = nested
        .lines()
        .filter(|line| !plain.lines().any(|plain| plain == *line))
        .collect();
    assert!(!marking.is_empty(), "{report}");

    // The summary carries the same marking, and the JSON form the `nested` key.
    let summary = run_inside(
        home.path(),
        &workspace,
        &format!("exec {KAKOI} --workspace {ws} --print-plan"),
    );
    let report = output_report(&summary);
    assert_eq!(summary.status.code(), Some(0), "{report}");
    let summary = String::from_utf8(summary.stdout).unwrap();
    for line in &marking {
        assert!(summary.lines().any(|summary| summary == *line), "{report}");
    }
    assert!(summary.contains("\n  rw      ~/ws\n"), "{report}");

    let json = run_inside(
        home.path(),
        &workspace,
        &format!("exec {KAKOI} --workspace {ws} --print-plan=json"),
    );
    let report = output_report(&json);
    assert_eq!(json.status.code(), Some(0), "{report}");
    let plan: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(plan["nested"], true, "{report}");
}

// @kotowari[EX-501]
#[test]
fn ex_501_a_nested_plan_shows_the_command_found_on_the_host_path() {
    // The profile, read by the outer run and the nested one alike, puts one `tool` first
    // on the isolation's `PATH`; the nested run's own `PATH` starts with another.
    let (home, workspace) = home_with_workspace();
    let host_tool = home.write_executable("host-bin/tool", "#!/bin/sh\n");
    home.write_executable("isolated-bin/tool", "#!/bin/sh\n");
    profile(
        &home,
        &format!(
            "{RW_WORKSPACE}[env]\npath-prepend = [\"{}\"]\n",
            home.path().join("isolated-bin").display()
        ),
    );

    let output = run_inside(
        home.path(),
        &workspace,
        &format!(
            "exec /usr/bin/env PATH={}:\"$PATH\" {KAKOI} --workspace {} \
             --print-plan=json -- tool",
            home.path().join("host-bin").display(),
            workspace.display()
        ),
    );

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        plan["environment"]["PATH"]
            .as_str()
            .unwrap()
            .starts_with(home.path().join("isolated-bin").to_str().unwrap()),
        "the isolated PATH should start elsewhere than the host's: {report}"
    );
    assert_eq!(
        plan["command"]["path"],
        host_tool.canonicalize().unwrap().to_str().unwrap(),
        "{report}"
    );
}

// @kotowari[EX-521]
#[test]
fn ex_521_a_nested_plan_of_a_missing_named_profile_is_a_policy_diagnostic() {
    let (home, workspace) = home_with_workspace();

    let output = run_inside(
        home.path(),
        &workspace,
        &format!(
            "exec {KAKOI} --workspace {} --profile missing --print-plan",
            workspace.display()
        ),
    );

    assert_diagnostic(&output, 125, "policy");
}

// `init` inside an isolation.

// @kotowari[EX-499]
#[test]
fn ex_499_init_inside_an_isolation_without_bwrap_succeeds_without_the_nesting_warning() {
    // The home is read-only inside, so the configuration directory is put in the workspace.
    let (home, workspace) = home_with_workspace();
    std::fs::create_dir(workspace.join("no-programs")).unwrap();
    let config = workspace.join("config");

    let output = run_inside(
        home.path(),
        &workspace,
        &format!(
            "exec /usr/bin/env PATH={} XDG_CONFIG_HOME={} {KAKOI} init",
            workspace.join("no-programs").display(),
            config.display()
        ),
    );

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert!(output.stderr.is_empty(), "{report}");
    assert!(
        config.join("kakoi/profile/default.toml").is_file(),
        "{report}"
    );
}

// Telling a nested run: the mark, not the environment.

// @kotowari[EX-884, REQ-284]
#[test]
fn ex_884_a_kakoi_started_inside_without_kakoi_is_still_nested() {
    let (home, workspace) = home_with_workspace();
    home.write("box/file", "hidden outside\n");
    std::fs::create_dir(home.path().join("data")).unwrap();
    profile(
        &home,
        &format!(
            "{RW_WORKSPACE}hide = [\"{}\"]\n",
            home.path().join("box").display()
        ),
    );

    // The nested run asks for `~/data` as `rw`; the outer isolation has it read-only.
    let output = run_inside(
        home.path(),
        &workspace,
        &format!(
            "exec /usr/bin/env -u KAKOI {KAKOI} --rw {} -- /bin/sh -c \
             'test ! -e {} && echo still-hidden; (echo x > {}) 2>/dev/null || echo not-writable'",
            home.path().join("data").display(),
            home.path().join("box/file").display(),
            home.path().join("data/file").display(),
        ),
    );

    assert_nested_warning_only(&output);
    assert_eq!(
        output.stdout,
        b"still-hidden\nnot-writable\n",
        "{}",
        output_report(&output)
    );
    assert!(!home.path().join("data/file").exists());
}

// @kotowari[EX-884, REQ-284]
#[test]
fn ex_884_a_nested_run_without_kakoi_does_not_read_the_secret_the_outer_run_emptied() {
    // A nested run reads no policy, so the secret file the outer isolation shows empty
    // does not stop it.
    let (home, workspace) = home_with_workspace();
    home.write("token", "FAKE-TOKEN\n");
    profile(
        &home,
        &format!("{RW_WORKSPACE}[secrets]\nTOKEN = \"~/token\"\n"),
    );

    let output = run_inside(
        home.path(),
        &workspace,
        &format!("exec /usr/bin/env -u KAKOI {KAKOI} -- /bin/echo ran"),
    );

    assert_nested_warning_only(&output);
    assert_eq!(output.stdout, b"ran\n", "{}", output_report(&output));
}

// @kotowari[EX-885]
#[test]
fn ex_885_kakoi_1_on_the_host_is_not_nested() {
    let (home, workspace) = home_with_workspace();
    home.write("box/file", "hidden\n");
    profile(
        &home,
        &format!(
            "{RW_WORKSPACE}hide = [\"{}\"]\n",
            home.path().join("box").display()
        ),
    );

    let output = binary(home.path())
        .env("KAKOI", "1")
        .current_dir(&workspace)
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--",
            "/bin/sh",
            "-c",
            &format!(
                "test -f {MARK} && test ! -e {} && echo isolated",
                home.path().join("box/file").display()
            ),
        ])
        .output()
        .unwrap();

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert_eq!(output.stdout, b"isolated\n", "{report}");
    assert!(output.stderr.is_empty(), "{report}");
}

// @kotowari[REQ-314, EX-547]
#[test]
fn req_314_the_host_environment_chooses_the_profile_and_the_work_place_but_not_the_nesting() {
    let home = TempDir::new();
    let first = home.write("first/kakoi/profile/default.toml", RW_WORKSPACE);
    let second = home.write("second/kakoi/profile/default.toml", RW_WORKSPACE);
    let place = home.path().join("place");
    std::fs::create_dir(&place).unwrap();
    let tool = home.write_executable("bin/tool", "#!/bin/sh\n");
    let mut path = vec![home.path().join("bin")];
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let plan = |config_home: &str| {
        let output = binary(home.path())
            .env("KAKOI", "1")
            .env("XDG_CONFIG_HOME", home.path().join(config_home))
            .env("PATH", std::env::join_paths(&path).unwrap())
            .current_dir(&place)
            .args(["--print-plan=json", "--", "tool"])
            .output()
            .unwrap();
        let report = output_report(&output);
        assert_eq!(output.status.code(), Some(0), "{report}");
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };

    let from_first = plan("first");
    let from_second = plan("second");

    assert_eq!(
        from_first["policy_sources"][0]["path"],
        first.to_str().unwrap()
    );
    assert_eq!(
        from_second["policy_sources"][0]["path"],
        second.to_str().unwrap()
    );
    assert_eq!(from_first["nested"], false);
    assert_eq!(from_second["nested"], false);
    let place = place.canonicalize().unwrap();
    assert_eq!(
        from_first["variables"]["workspace"],
        place.to_str().unwrap()
    );
    assert_eq!(
        from_first["home"],
        home.path().canonicalize().unwrap().to_str().unwrap()
    );
    assert_eq!(
        from_first["command"]["path"],
        tool.canonicalize().unwrap().to_str().unwrap()
    );
}

// A nested run with `--nested=isolate`: an isolation of its own inside the outer one.

// @kotowari[EX-886]
#[test]
fn ex_886_an_isolation_asked_for_inside_is_no_wider_than_the_outer_one() {
    let (home, workspace) = home_with_workspace();
    let data = home.path().join("data");
    std::fs::create_dir(&data).unwrap();

    // The nested run asks for `~/data` as `rw`; the outer isolation has it read-only.
    let output = run_inside(
        home.path(),
        &workspace,
        &format!(
            "exec {KAKOI} --nested=isolate --workspace {} --rw {} -- /bin/sh -c \
             'test -f {MARK} && echo isolated; (echo x > {}) 2>/dev/null || echo not-writable'",
            workspace.display(),
            data.display(),
            data.join("file").display(),
        ),
    );

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert_eq!(output.stdout, b"isolated\nnot-writable\n", "{report}");
    assert!(output.stderr.is_empty(), "{report}");
    assert!(!data.join("file").exists(), "{report}");
}

// @kotowari[REQ-456]
#[test]
fn req_456_a_nested_isolation_finds_the_command_on_the_isolated_path() {
    let (home, workspace) = home_with_workspace();
    home.write_executable("host-bin/tool", "#!/bin/sh\necho host\n");
    home.write_executable("isolated-bin/tool", "#!/bin/sh\necho isolated\n");
    profile(
        &home,
        &format!(
            "{RW_WORKSPACE}[env]\npath-prepend = [\"{}\"]\n",
            home.path().join("isolated-bin").display()
        ),
    );

    let output = run_inside(
        home.path(),
        &workspace,
        &format!(
            "exec /usr/bin/env PATH={}:\"$PATH\" {KAKOI} --nested=isolate --workspace {} -- tool",
            home.path().join("host-bin").display(),
            workspace.display()
        ),
    );

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert_eq!(output.stdout, b"isolated\n", "{report}");
    assert!(output.stderr.is_empty(), "{report}");
}

// @kotowari[REQ-456]
#[test]
fn req_456_a_nested_isolation_checks_the_home_directory() {
    let (home, workspace) = home_with_workspace();
    let home_file = home.write("ws/home-file", "");

    let output = run_inside(
        home.path(),
        &workspace,
        &format!(
            "exec /usr/bin/env HOME={} {KAKOI} --nested=isolate -- /bin/echo ran",
            home_file.display()
        ),
    );

    assert_diagnostic(&output, 125, "env");
}

// @kotowari[REQ-456]
#[test]
fn req_456_a_nested_isolation_stops_on_the_secret_the_outer_run_emptied() {
    let (home, workspace) = home_with_workspace();
    home.write("token", "FAKE-TOKEN\n");
    profile(
        &home,
        &format!("{RW_WORKSPACE}[secrets]\nTOKEN = \"~/token\"\n"),
    );

    let output = run_inside(
        home.path(),
        &workspace,
        &format!(
            "exec {KAKOI} --nested=isolate --workspace {} -- /bin/echo ran",
            workspace.display()
        ),
    );

    assert_diagnostic(&output, 125, "secret");
}

/// The JSON plan a kakoi started inside with `arguments` prints, after checking it
/// exited 0.
fn nested_json_plan(
    home: &TempDir,
    workspace: &std::path::Path,
    arguments: &str,
) -> serde_json::Value {
    let output = run_inside(
        home.path(),
        workspace,
        &format!(
            "exec {KAKOI} {arguments} --workspace {} --print-plan=json",
            workspace.display()
        ),
    );
    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| panic!("{error}: {report}"))
}

// @kotowari[EX-888]
#[test]
fn ex_888_nested_and_applied_in_the_json_plan() {
    let (home, workspace) = home_with_workspace();

    let isolate = nested_json_plan(&home, &workspace, "--nested=isolate");
    let exec = nested_json_plan(&home, &workspace, "--nested=exec");
    let host = binary(home.path())
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--print-plan=json",
        ])
        .output()
        .unwrap();
    let report = output_report(&host);
    assert_eq!(host.status.code(), Some(0), "{report}");
    let host: serde_json::Value = serde_json::from_slice(&host.stdout).unwrap();

    assert_eq!(
        [&isolate["nested"], &exec["nested"], &host["nested"]],
        [true, true, false]
    );
    assert_eq!(
        [&isolate["applied"], &exec["applied"], &host["applied"]],
        [true, false, true]
    );
    for plan in [&isolate, &exec, &host] {
        assert_eq!(plan["format_version"], 1);
    }
}

// @kotowari[REQ-457, REQ-297]
#[test]
fn req_457_a_nested_isolation_plan_is_marked_apart_from_a_nested_exec_plan() {
    let (home, workspace) = home_with_workspace();
    let first_line = |arguments: &str, form: &str| {
        let output = run_inside(
            home.path(),
            &workspace,
            &format!(
                "exec {KAKOI} {arguments} --workspace {} {form}",
                workspace.display()
            ),
        );
        let report = output_report(&output);
        assert_eq!(output.status.code(), Some(0), "{report}");
        let text = String::from_utf8(output.stdout).unwrap();
        text.lines().next().unwrap().to_string()
    };
    let plain = binary(home.path())
        .args(["--workspace", workspace.to_str().unwrap(), "--print-plan"])
        .output()
        .unwrap();
    let plain = String::from_utf8(plain.stdout).unwrap();

    let isolate_summary = first_line("--nested=isolate", "--print-plan");
    let isolate_full = first_line("--nested=isolate", "--print-plan=full");
    let exec_summary = first_line("--nested=exec", "--print-plan");

    assert_eq!(isolate_summary, isolate_full);
    assert_ne!(isolate_summary, exec_summary);
    assert!(!plain.contains(&isolate_summary), "{plain}");
    assert!(!plain.contains(&exec_summary), "{plain}");
}

// `network.allow-nested-filtered`: the host's `/dev/net/tun` inside.

/// Writes `policy` as a policy file under `home` and returns its path.
fn policy_file(home: &TempDir, name: &str, policy: &str) -> std::path::PathBuf {
    home.write(name, policy)
}

/// Runs `script` inside the isolation of `home`'s profile and `policy`.
fn run_with_policy(
    home: &TempDir,
    workspace: &std::path::Path,
    policy: &std::path::Path,
    script: &str,
) -> Output {
    binary(home.path())
        .current_dir(workspace)
        .args([
            "--workspace".as_ref(),
            workspace.as_os_str(),
            "--policy-file".as_ref(),
            policy.as_os_str(),
            "--".as_ref(),
            "/bin/sh".as_ref(),
            "-c".as_ref(),
            std::ffi::OsStr::new(script),
        ])
        .output()
        .unwrap()
}

const LOOK_FOR_TUN: &str = "test -c /dev/net/tun && echo tun || echo no-tun";

// @kotowari[EX-890]
#[test]
fn ex_890_the_tun_device_is_not_shown_by_default() {
    let (home, workspace) = home_with_workspace();

    let output = run_inside(home.path(), &workspace, LOOK_FOR_TUN);

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert_eq!(output.stdout, b"no-tun\n", "{report}");
}

// @kotowari[REQ-458]
#[test]
fn req_458_the_key_shows_the_tun_device_without_a_warning_in_host_mode() {
    let (home, workspace) = home_with_workspace();
    let policy = policy_file(
        &home,
        "tun.toml",
        "[network]\nallow-nested-filtered = true\n",
    );

    let output = run_with_policy(&home, &workspace, &policy, LOOK_FOR_TUN);

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert_eq!(output.stdout, b"tun\n", "{report}");
    assert!(output.stderr.is_empty(), "{report}");
}

// @kotowari[REQ-458]
#[test]
fn req_458_the_upper_layer_decides() {
    let (home, workspace) = home_with_workspace();
    let with = |profile: bool, file: bool| {
        profile_text(&home, profile);
        let policy = policy_file(
            &home,
            "tun.toml",
            &format!("[network]\nallow-nested-filtered = {file}\n"),
        );
        let output = run_with_policy(&home, &workspace, &policy, LOOK_FOR_TUN);
        let report = output_report(&output);
        assert_eq!(output.status.code(), Some(0), "{report}");
        String::from_utf8(output.stdout).unwrap()
    };

    assert_eq!(with(true, false), "no-tun\n");
    assert_eq!(with(false, true), "tun\n");
}

/// Writes the profile of `home`: the workspace `rw` and `network.allow-nested-filtered`.
fn profile_text(home: &TempDir, allow: bool) {
    profile(
        home,
        &format!("{RW_WORKSPACE}[network]\nallow-nested-filtered = {allow}\n"),
    );
}

// @kotowari[REQ-458]
#[test]
fn req_458_the_summary_says_the_tun_device_is_shown() {
    let (home, workspace) = home_with_workspace();
    let summary = |allow: bool| {
        profile_text(&home, allow);
        let output = binary(home.path())
            .args(["--workspace", workspace.to_str().unwrap(), "--print-plan"])
            .output()
            .unwrap();
        let report = output_report(&output);
        assert_eq!(output.status.code(), Some(0), "{report}");
        assert!(output.stderr.is_empty(), "{report}");
        String::from_utf8(output.stdout).unwrap()
    };

    let shown = summary(true);
    let not_shown = summary(false);

    let extra: Vec<&str> = shown
        .lines()
        .filter(|line| !not_shown.lines().any(|other| other == *line))
        .collect();
    assert_eq!(extra.len(), 1, "{shown}\n{not_shown}");
    assert_eq!(not_shown.lines().count() + 1, shown.lines().count());
}

// @kotowari[REQ-458]
#[test]
fn req_458_without_a_tun_device_on_the_host_the_run_and_the_plan_stop() {
    // The outer isolation shows no `/dev/net/tun`, so for the nested run the host has
    // none.
    let (home, workspace) = home_with_workspace();
    let inner = policy_file(
        &home,
        "inner.toml",
        "[network]\nallow-nested-filtered = true\n",
    );
    let nested = |arguments: &str| {
        run_inside(
            home.path(),
            &workspace,
            &format!(
                "exec {KAKOI} --nested=isolate --workspace {} --policy-file {} {arguments}",
                workspace.display(),
                inner.display()
            ),
        )
    };

    assert_diagnostic(&nested("-- /bin/echo ran"), 125, "bwrap");
    assert_diagnostic(&nested("--print-plan"), 125, "bwrap");
}

// @kotowari[REQ-315]
#[test]
fn req_315_the_tun_device_follows_the_mark() {
    let (home, workspace) = home_with_workspace();
    profile_text(&home, true);

    let output = binary(home.path())
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--print-plan=json",
        ])
        .output()
        .unwrap();

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let arguments = plan["bwrap_arguments"].as_array().unwrap();
    let mark = arguments
        .iter()
        .position(|argument| argument["value"] == MARK)
        .unwrap_or_else(|| panic!("no mark: {report}"));
    let values: Vec<&serde_json::Value> = arguments[mark + 1..mark + 4]
        .iter()
        .map(|argument| &argument["value"])
        .collect();
    assert_eq!(
        values,
        ["--dev-bind", "/dev/net/tun", "/dev/net/tun"],
        "{report}"
    );
    assert_eq!(arguments[mark + 4]["value"], "--proc", "{report}");
}

// The outer run's command guards inside a nested isolation.

const GIT_PUSH: &str =
    "[[commands.guard]]\nprogram = \"git\"\ndeny = [[\"push\"]]\nreason = \"push は人が行う\"\n";

/// Runs `script` inside the isolation of `home`'s profile with `outer` added, where it
/// starts a nested isolation of the profile with `inner` added, and in that the shell
/// command `inner_script`.
fn nested_under_policies(
    home: &TempDir,
    workspace: &std::path::Path,
    outer: &str,
    inner: &str,
    arguments: &str,
) -> Output {
    let outer = policy_file(home, "outer.toml", outer);
    let inner = policy_file(home, "inner.toml", inner);
    run_with_policy(
        home,
        workspace,
        &outer,
        &format!(
            "exec {KAKOI} --nested=isolate --workspace {} --policy-file {} {arguments}",
            workspace.display(),
            inner.display()
        ),
    )
}

// @kotowari[EX-899]
#[test]
fn ex_899_the_outer_guard_still_denies_inside_a_nested_isolation() {
    let (home, workspace) = home_with_workspace();

    let output = nested_under_policies(
        &home,
        &workspace,
        GIT_PUSH,
        "",
        "-- /bin/sh -c 'git push origin main'",
    );

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(126), "{report}");
    assert!(output.stdout.is_empty(), "{report}");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "kakoi: guard: git push: push は人が行う\n",
        "{report}"
    );
}

/// Runs `git push` by name in a nested isolation whose own policy is `inner`, under an
/// outer policy that denies it.
fn outer_guard_denies_push_under(inner: &str) {
    let (home, workspace) = home_with_workspace();
    let bin = home.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    std::os::unix::fs::symlink("/usr/bin/git", bin.join("git")).unwrap();
    let inner = inner.replace("BIN", bin.to_str().unwrap());

    let output = nested_under_policies(
        &home,
        &workspace,
        GIT_PUSH,
        &inner,
        "-- /bin/sh -c 'git push origin main'",
    );

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(126), "{report}");
    assert!(output.stdout.is_empty(), "{report}");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "kakoi: guard: git push: push は人が行う\n",
        "{report}"
    );
}

// @kotowari[REQ-465]
#[test]
fn req_465_the_outer_guard_goes_first_on_a_path_the_inner_policy_starts_afresh() {
    outer_guard_denies_push_under("[env]\nmode = \"clear\"\nset = { PATH = \"/usr/bin:/bin\" }\n");
}

// @kotowari[REQ-465]
#[test]
fn req_465_the_outer_guard_goes_first_on_a_path_the_inner_policy_sets() {
    outer_guard_denies_push_under("[env]\nset = { PATH = \"/usr/bin:/bin\" }\n");
}

// @kotowari[REQ-465]
#[test]
fn req_465_the_outer_guard_goes_before_what_the_inner_policy_prepends() {
    outer_guard_denies_push_under("[env]\npath-prepend = [\"BIN\"]\n");
}

// @kotowari[EX-900]
#[test]
fn ex_900_a_guard_over_the_real_program_still_starts_it_inside_a_nested_isolation() {
    let (home, workspace) = home_with_workspace();
    let version = std::process::Command::new("/usr/bin/git")
        .arg("--version")
        .output()
        .unwrap();

    let output = nested_under_policies(
        &home,
        &workspace,
        &format!("{GIT_PUSH}guard-absolute-path = true\n"),
        "",
        "-- /bin/sh -c '/usr/bin/git --version'",
    );

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert_eq!(output.stdout, version.stdout, "{report}");
    assert!(output.stderr.is_empty(), "{report}");
}

// @kotowari[EX-901, REQ-466]
#[test]
fn ex_901_guards_outside_and_inside_stop_the_nested_isolation() {
    let (home, workspace) = home_with_workspace();

    let output = nested_under_policies(&home, &workspace, GIT_PUSH, GIT_PUSH, "-- /bin/echo ran");

    assert_diagnostic(&output, 125, "policy");
}

/// The `value`s of the literal arguments of the JSON plan in `output`.
fn literal_arguments(output: &Output) -> Vec<String> {
    let report = output_report(output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    plan["bwrap_arguments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|argument| argument["value"].as_str().unwrap_or("").to_string())
        .collect()
}

// @kotowari[REQ-465]
#[test]
fn req_465_nothing_is_handed_on_without_an_outer_guard() {
    let (home, workspace) = home_with_workspace();

    let output = nested_under_policies(&home, &workspace, "", "", "--print-plan=json");

    let arguments = literal_arguments(&output);
    assert!(
        !arguments
            .iter()
            .any(|value| value.starts_with("/dev/kakoi-guard")),
        "{arguments:?}"
    );
}

// @kotowari[REQ-315, REQ-465]
#[test]
fn req_315_the_outer_guard_follows_the_mark_and_the_tun_device() {
    let (home, workspace) = home_with_workspace();
    let tun = "[network]\nallow-nested-filtered = true\n";

    let output = nested_under_policies(
        &home,
        &workspace,
        &format!("{GIT_PUSH}{tun}"),
        tun,
        "--print-plan=json",
    );

    let arguments = literal_arguments(&output);
    let mark = arguments
        .iter()
        .position(|value| value == MARK)
        .unwrap_or_else(|| panic!("{arguments:?}"));
    assert_eq!(
        arguments[mark + 1..mark + 8],
        [
            "--dev-bind",
            "/dev/net/tun",
            "/dev/net/tun",
            "--ro-bind",
            "/dev/kakoi-guard",
            "/dev/kakoi-guard",
            "--proc",
        ],
        "{arguments:?}"
    );
}
