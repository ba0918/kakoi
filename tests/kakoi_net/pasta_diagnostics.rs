//! What kakoi says when pasta is missing, too old, or fails its start for another reason.

use crate::cli::{Filtered, HANG};
use crate::common::{assert_diagnostic, TempDir};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Where the diagnostics send a user whose pasta is missing or too old.
const PASTA_GUIDE: &str = "https://github.com/ba0918/kakoi/blob/main/docs/pasta.md";

/// A directory holding only `bwrap` and the given tools, whatever else this
/// machine has installed.
fn only_bwrap_and(tools: &[(&str, &str)]) -> TempDir {
    let directory = TempDir::new();
    let bwrap = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join("bwrap"))
        .find(|candidate| candidate.is_file())
        .unwrap();
    std::os::unix::fs::symlink(bwrap, directory.path().join("bwrap")).unwrap();
    for (name, target) in tools {
        std::os::unix::fs::symlink(target, directory.path().join(name)).unwrap();
    }
    directory
}

// @kotowari[REQ-057, EX-106, REQ-429, EX-824, EX-825]
#[test]
fn a_missing_pasta_ends_the_start_before_the_application_runs() {
    let filtered = Filtered::new("");
    let marker = filtered.workspace.join("ran");
    let tools = only_bwrap_and(&[("nft", "/usr/sbin/nft")]);
    let output = filtered
        .command(
            tools.path().to_str().unwrap(),
            &[],
            &["/bin/sh", "-c", &format!("touch '{}'", marker.display())],
        )
        .output()
        .unwrap();
    let diagnostic = assert_diagnostic(&output, 125, "bwrap");
    assert!(diagnostic.contains("pasta"), "{diagnostic}");
    assert!(diagnostic.contains(PASTA_GUIDE), "{diagnostic}");
    assert!(!marker.exists());
}

// @kotowari[REQ-429, EX-839]
#[test]
fn a_missing_pasta_is_reported_before_a_missing_nft() {
    let filtered = Filtered::new("");
    let tools = only_bwrap_and(&[]);
    let output = filtered
        .command(tools.path().to_str().unwrap(), &[], &["/bin/true"])
        .output()
        .unwrap();
    let diagnostic = assert_diagnostic(&output, 125, "bwrap");
    assert!(
        diagnostic.contains("pasta") && !diagnostic.contains("nft"),
        "{diagnostic}"
    );
    assert!(diagnostic.contains(PASTA_GUIDE), "{diagnostic}");
}

/// Usage text in the shape of the real pasta's `--help`: one line per option,
/// the long name followed by a tab, listing every long name kakoi passes but
/// those in `absent`, and then the lines in `extra`.
fn usage(absent: &[&str], extra: &[&str]) -> String {
    let mut text = String::from("Usage: pasta [OPTION]... [COMMAND] [ARGS]...\n\n");
    for name in kakoi_net::pasta::LONG_OPTIONS {
        if !absent.contains(&name) {
            text.push_str(&format!("  {name}\tAn option kakoi passes\n"));
        }
    }
    for line in extra {
        text.push_str(&format!("  {line}\tAnother option\n"));
    }
    text
}

/// Writes a pasta that fails its start with `startup`, a shell fragment, and
/// answers `--help` with `help`, another.
fn failing_pasta(filtered: &Filtered, startup: &str, help: &str) {
    filtered.bin.write_executable(
        "pasta",
        format!("#!/bin/sh\nif [ \"$1\" = --help ]; then\n{help}\nfi\n{startup}\n"),
    );
}

/// A `--help` fragment printing `text` on standard output and ending with `code`.
fn prints(text: &str, code: i32) -> String {
    format!("cat <<'USAGE'\n{text}USAGE\nexit {code}")
}

/// The first lines of an old pasta's complaint about an option it does not know.
const COMPLAINT: [&str; 3] = [
    "pasta: unrecognized option by test",
    "Usage: pasta [OPTION]... [COMMAND] [ARGS]...",
    "Without PID or --netns, run the given command or a",
];

/// A startup fragment that complains as an old pasta does and ends.
fn complains() -> String {
    format!(
        "cat >&2 <<'COMPLAINT'\n{}\nCOMPLAINT\nexit 1",
        COMPLAINT.join("\n")
    )
}

fn filtered_start(filtered: &Filtered) -> std::process::Output {
    filtered.with_pasta(&["/bin/true"]).output().unwrap()
}

// @kotowari[REQ-430, EX-826, EX-827]
#[test]
fn an_old_pasta_is_named_by_its_missing_options_without_its_own_output() {
    let filtered = Filtered::new("");
    failing_pasta(
        &filtered,
        &complains(),
        &prints(
            &usage(&["--host-lo-to-ns-lo", "--map-host-loopback"], &[]),
            0,
        ),
    );
    let diagnostic = assert_diagnostic(&filtered_start(&filtered), 125, "bwrap");
    for expected in ["--host-lo-to-ns-lo", "--map-host-loopback", PASTA_GUIDE] {
        assert!(diagnostic.contains(expected), "{expected}: {diagnostic}");
    }
    for line in COMPLAINT {
        assert!(!diagnostic.contains(line), "{line}: {diagnostic}");
    }
}

// @kotowari[REQ-430, EX-828]
#[test]
fn every_long_option_is_checked_not_only_the_two_known_to_be_missing() {
    let filtered = Filtered::new("");
    failing_pasta(
        &filtered,
        &complains(),
        &prints(&usage(&["--no-map-gw"], &[]), 0),
    );
    let diagnostic = assert_diagnostic(&filtered_start(&filtered), 125, "bwrap");
    assert!(diagnostic.contains("--no-map-gw"), "{diagnostic}");
    assert!(diagnostic.contains(PASTA_GUIDE), "{diagnostic}");
}

// @kotowari[REQ-430, EX-840]
#[test]
fn a_longer_name_that_starts_with_an_option_does_not_list_it() {
    let filtered = Filtered::new("");
    failing_pasta(
        &filtered,
        &complains(),
        &prints(
            &usage(&["--host-lo-to-ns-lo"], &["--host-lo-to-ns-lo-extra"]),
            0,
        ),
    );
    let diagnostic = assert_diagnostic(&filtered_start(&filtered), 125, "bwrap");
    assert!(diagnostic.contains("--host-lo-to-ns-lo"), "{diagnostic}");
    assert!(diagnostic.contains(PASTA_GUIDE), "{diagnostic}");
}

// The pipe closes half a second before the exit, so that the closed pipe is
// seen first.
// @kotowari[REQ-430, EX-841]
#[test]
fn an_old_pasta_that_closes_its_startup_pipe_before_ending_is_told_apart() {
    let filtered = Filtered::new("");
    failing_pasta(
        &filtered,
        "exec >&-\nsleep 0.5\nexit 1",
        &prints(&usage(&["--map-host-loopback"], &[]), 0),
    );
    let diagnostic = assert_diagnostic(&filtered_start(&filtered), 125, "bwrap");
    assert!(diagnostic.contains("--map-host-loopback"), "{diagnostic}");
    assert!(diagnostic.contains(PASTA_GUIDE), "{diagnostic}");
}

/// Asserts the diagnostic of a pasta that failed for another reason than its
/// age: its own reason, and no pointer to the installation guide.
fn assert_start_failure(output: &std::process::Output, reason: &str) {
    let diagnostic = assert_diagnostic(output, 125, "bwrap");
    assert!(diagnostic.contains(reason), "{diagnostic}");
    assert!(!diagnostic.contains(PASTA_GUIDE), "{diagnostic}");
}

// @kotowari[REQ-432, EX-831]
#[test]
fn a_pasta_with_every_option_keeps_its_own_startup_failure() {
    let filtered = Filtered::new("");
    failing_pasta(
        &filtered,
        "echo 'permission denied by test' >&2\nexit 1",
        &prints(&usage(&[], &[]), 0),
    );
    assert_start_failure(&filtered_start(&filtered), "permission denied by test");
}

// @kotowari[REQ-432, EX-832]
#[test]
fn an_empty_help_does_not_make_pasta_old() {
    let filtered = Filtered::new("");
    failing_pasta(
        &filtered,
        "echo 'startup failed by test' >&2\nexit 1",
        "exit 0",
    );
    assert_start_failure(&filtered_start(&filtered), "startup failed by test");
}

// @kotowari[REQ-432, EX-844]
#[test]
fn the_exit_code_of_help_is_not_what_tells_pasta_old() {
    let filtered = Filtered::new("");
    failing_pasta(
        &filtered,
        "echo 'startup failed by test' >&2\nexit 1",
        &prints(&usage(&[], &[]), 1),
    );
    assert_start_failure(&filtered_start(&filtered), "startup failed by test");
}

// The help is given up at the 10 second startup deadline.
// @kotowari[REQ-432, EX-845]
#[test]
fn a_help_that_never_ends_is_given_up_at_the_startup_deadline() {
    let filtered = Filtered::new("");
    failing_pasta(
        &filtered,
        "echo 'startup failed by test' >&2\nexit 1",
        "exec sleep 600",
    );
    let started = Instant::now();
    let output = filtered_start(&filtered);
    let elapsed = started.elapsed();
    assert_start_failure(&output, "startup failed by test");
    assert!(
        (Duration::from_secs(9)..HANG).contains(&elapsed),
        "{elapsed:?}"
    );
}

/// Writes a pasta that records each call's arguments in `calls` beside it and
/// then runs `behaviour`, a Python fragment.
fn recording_pasta(filtered: &Filtered, behaviour: &str) -> PathBuf {
    let calls = filtered.bin.path().join("calls");
    filtered.bin.write_executable(
        "pasta",
        format!(
            "#!/usr/bin/python3\nimport os, sys, time\nwith open({:?}, 'a') as out:\n    out.write(' '.join(sys.argv[1:]) + '\\n')\n{behaviour}\n",
            calls.to_str().unwrap()
        ),
    );
    calls
}

/// Asserts that pasta was called, and never with `--help`.
fn assert_no_help(calls: &Path) {
    let calls = std::fs::read_to_string(calls).unwrap();
    assert!(!calls.is_empty());
    assert!(
        calls
            .lines()
            .all(|call| !call.split(' ').any(|word| word == "--help")),
        "{calls}"
    );
}

// @kotowari[REQ-431, EX-829, EX-830]
#[test]
fn a_pasta_that_starts_is_not_asked_for_help() {
    let filtered = Filtered::new("");
    let calls = recording_pasta(&filtered, "print(os.getpid(), flush=True)\ntime.sleep(600)");
    let output = filtered_start(&filtered);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_no_help(&calls);
}

// kakoi waits for the 10 second startup deadline.
// @kotowari[REQ-431, EX-842]
#[test]
fn a_pasta_start_that_times_out_is_not_asked_for_help() {
    let filtered = Filtered::new("");
    let calls = recording_pasta(&filtered, "time.sleep(600)");
    assert_diagnostic(&filtered_start(&filtered), 125, "bwrap");
    assert_no_help(&calls);
}

// @kotowari[REQ-431]
#[test]
fn a_pasta_that_signals_another_pid_is_not_asked_for_help() {
    let filtered = Filtered::new("");
    let calls = recording_pasta(&filtered, "print(1, flush=True)\ntime.sleep(600)");
    assert_diagnostic(&filtered_start(&filtered), 125, "bwrap");
    assert_no_help(&calls);
}

// kakoi waits for the 10 second startup deadline.
// @kotowari[REQ-431]
#[test]
fn a_pasta_that_closes_its_startup_pipe_and_keeps_running_is_not_asked_for_help() {
    let filtered = Filtered::new("");
    let calls = recording_pasta(&filtered, "os.close(1)\ntime.sleep(600)");
    assert_diagnostic(&filtered_start(&filtered), 125, "bwrap");
    assert_no_help(&calls);
}
