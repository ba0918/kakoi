//! What the built binary does with the real bwrap (specification section 15.2): the rows
//! of the tables of section 13, and what is seen from inside the isolation.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

mod common;

use common::{
    assert_diagnostic, binary, home_with_workspace, output_report, run,
    run_command_from_deleted_dir, run_command_with_soft_fd_limit, run_from_deleted_dir, TempDir,
    RW_WORKSPACE,
};

// @kotowari[REQ-291]
#[test]
fn a_command_exit_code_passes_through() {
    let (home, workspace) = home_with_workspace();

    for code in [0, 3, 42] {
        let output = binary(home.path())
            .args([
                "--workspace",
                workspace.to_str().unwrap(),
                "--",
                "/bin/sh",
                "-c",
                &format!("echo out; echo err >&2; exit {code}"),
            ])
            .output()
            .unwrap();

        let report = output_report(&output);
        assert_eq!(output.status.code(), Some(code), "{report}");
        assert_eq!(output.stdout, b"out\n", "{report}");
        assert_eq!(output.stderr, b"err\n", "{report}");
    }
}

/// Overwrites the `default` profile of `home` with `text`.
fn profile(home: &TempDir, text: &str) {
    home.write(".config/kakoi/profile/default.toml", text);
}

/// Runs `script` with `/bin/sh -c` inside the isolation of `home`'s profile, with the
/// workspace at `workspace`, which is also the current directory.
fn run_script(home: &TempDir, workspace: &Path, script: &str) -> Output {
    binary(home.path())
        .current_dir(workspace)
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--",
            "/bin/sh",
            "-c",
            script,
        ])
        .output()
        .unwrap()
}

/// Exit 0 and no diagnostic or warning from `kakoi` itself; returns standard
/// output as text.
fn assert_ran_clean(output: &Output) -> String {
    let report = output_report(output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("kakoi:"),
        "{report}"
    );
    String::from_utf8(output.stdout.clone()).unwrap()
}

// @kotowari[REQ-293]
#[test]
fn adjacent_stages_yield_the_earlier_diagnostic() {
    // Stage 1 beside stage 2 has no input: `--help` beside any other argument is itself
    // the grammar error of stage 2. Stage 1 beside stage 3: `--help` needs no current
    // directory.
    let home = TempDir::new();
    let output = run_from_deleted_dir(home.path(), ["--help"]);
    assert_eq!(output.status.code(), Some(0), "{}", output_report(&output));

    // Stage 2 beside stage 3: an unknown option from a deleted current directory.
    let output = run_from_deleted_dir(home.path(), ["--bogus", "--", "/bin/true"]);
    assert_diagnostic(&output, 125, "usage");

    // Stage 3 beside stage 4: a deleted current directory and a `HOME` that is a file.
    let home_file = home.write("home-file", "");
    let mut command = binary(home.path());
    command.env("HOME", &home_file);
    let output = run_command_from_deleted_dir(command, home.path(), ["--", "/bin/true"]);
    assert_diagnostic(&output, 125, "path");

    // Stage 4 beside stage 5: a `HOME` that is a file and a broken profile.
    profile(&home, "[mounts\nbroken");
    let output = binary(home.path())
        .env("HOME", &home_file)
        .args(["--", "/bin/true"])
        .output()
        .unwrap();
    assert_diagnostic(&output, 125, "env");

    // Stage 5 beside stage 6: a broken profile and a missing workspace.
    let missing = home.path().join("missing");
    let output = run(
        home.path(),
        ["--workspace", missing.to_str().unwrap(), "--", "/bin/true"],
    );
    assert_diagnostic(&output, 125, "policy");

    // Stage 6 beside stage 7: a missing workspace and two directives on one path in one
    // layer (`policy`, decided in stage 7).
    home.write("x/.keep", "");
    profile(&home, "[mounts]\nrw = [\"~/x\"]\nro = [\"~/x\"]\n");
    let output = run(
        home.path(),
        ["--workspace", missing.to_str().unwrap(), "--", "/bin/true"],
    );
    assert_diagnostic(&output, 125, "path");

    // Stage 7 beside stage 8: an empty secret file and no `bwrap` on `PATH`.
    let (home, workspace) = home_with_workspace();
    let empty_path = TempDir::new();
    home.write(".config/kakoi/secrets/empty", "");
    profile(
        &home,
        &format!("{RW_WORKSPACE}[secrets]\nEMPTY = \"${{config_dir}}/secrets/empty\"\n"),
    );
    let output = binary(home.path())
        .env("PATH", empty_path.path())
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--",
            "/bin/true",
        ])
        .output()
        .unwrap();
    assert_diagnostic(&output, 125, "secret");

    // Stage 8 beside stage 9: no `bwrap` on `PATH` and a command that is nowhere.
    profile(&home, RW_WORKSPACE);
    let output = binary(home.path())
        .env("PATH", empty_path.path())
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--",
            "no-such-tool",
        ])
        .output()
        .unwrap();
    assert_diagnostic(&output, 125, "bwrap");
}

// @kotowari[REQ-310]
#[test]
fn a_fifo_policy_file_ends_in_a_diagnostic_from_the_binary() {
    let (home, workspace) = home_with_workspace();
    let fifo = home.path().join("policy.fifo");
    let c_path = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    // SAFETY: `mkfifo` reads the NUL-terminated path and touches nothing else.
    assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);

    // Nothing ever writes to the FIFO: a blocking open would never return.
    let output = run(
        home.path(),
        [
            "--workspace",
            workspace.to_str().unwrap(),
            "--policy-file",
            fifo.to_str().unwrap(),
            "--",
            "/bin/true",
        ],
    );

    assert_diagnostic(&output, 125, "policy");
}

// @kotowari[REQ-311, EX-544]
#[test]
fn a_policy_file_over_the_reading_limit_ends_in_a_diagnostic_from_the_binary() {
    let (home, workspace) = home_with_workspace();
    let line = "# 0123456789012345678901234567890123456789012345678901234567890\n";
    let mut text = line.repeat((1 << 20) / line.len());
    text.push_str(&line[..(1 << 20) - text.len()]);
    assert_eq!(text.len(), 1 << 20);
    let one_over = home.write("over.toml", format!("{text}\n"));

    let output = run(
        home.path(),
        [
            "--workspace",
            workspace.to_str().unwrap(),
            "--policy-file",
            one_over.to_str().unwrap(),
            "--",
            "/bin/true",
        ],
    );

    assert_diagnostic(&output, 125, "policy");
}

// @kotowari[REQ-291, EX-527]
#[test]
fn a_command_signal_passes_through_as_128_plus_s() {
    let (home, workspace) = home_with_workspace();

    let output = run_script(&home, &workspace, "kill -TERM $$");

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(128 + libc::SIGTERM), "{report}");
}

// @kotowari[REQ-250, EX-490]
#[test]
fn command_arguments_arrive_unchanged() {
    let (home, workspace) = home_with_workspace();
    let arguments = [
        "--rw",
        "--help",
        "--print-plan",
        "-x",
        "",
        "two words",
        "--",
        "~/tilde",
        "${workspace}",
        "日本語",
    ];

    let output = binary(home.path())
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--",
            "/bin/sh",
            "-c",
        ])
        .arg("for a in \"$@\"; do printf '[%s]\\n' \"$a\"; done")
        .arg("sh")
        .args(arguments)
        .output()
        .unwrap();

    let stdout = assert_ran_clean(&output);
    let expected: String = arguments.iter().map(|a| format!("[{a}]\n")).collect();
    assert_eq!(stdout, expected, "{}", output_report(&output));
}

// @kotowari[REQ-315, EX-548]
#[test]
fn a_command_path_starting_with_a_dash_is_executed_as_a_path() {
    // Specification section 4.2: a COMMAND containing `/` is used as that path. A relative
    // path can start with `-`; it must reach the command, not be read as an option of bwrap.
    // The tool is a copy of `echo` rather than a script: the kernel hands a script's path
    // to its interpreter as an argument, and `sh` would read `-x/tool` as options too.
    let (home, workspace) = home_with_workspace();
    let tool = workspace.join("-x/tool");
    std::fs::create_dir(tool.parent().unwrap()).unwrap();
    common::copy_executable(Path::new("/bin/echo"), &tool);

    let output = binary(home.path())
        .current_dir(&workspace)
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--",
            "-x/tool",
            "ran",
        ])
        .output()
        .unwrap();

    let stdout = assert_ran_clean(&output);
    assert_eq!(stdout, "ran\n", "{}", output_report(&output));
}

// @kotowari[REQ-263, EX-503]
#[test]
fn the_command_sees_the_given_name_as_argv0() {
    // Specification sections 1 and 4.2: argv[0] is the `COMMAND` string as given, not the
    // resolved path. `sh` is found on `PATH` and prints `$0`; `-x/tool` is a relative path
    // starting with `-`, a copy of `cat` that shows its own command line (a script would
    // show the path the kernel hands its interpreter, not argv[0]).
    let (home, workspace) = home_with_workspace();
    let tool = workspace.join("-x/tool");
    std::fs::create_dir(tool.parent().unwrap()).unwrap();
    common::copy_executable(Path::new("/bin/cat"), &tool);

    let by_name = binary(home.path())
        .current_dir(&workspace)
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--",
            "sh",
            "-c",
            "echo \"$0\"",
        ])
        .output()
        .unwrap();
    assert_eq!(
        assert_ran_clean(&by_name),
        "sh\n",
        "{}",
        output_report(&by_name)
    );

    let by_dash_path = binary(home.path())
        .current_dir(&workspace)
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--",
            "-x/tool",
            "/proc/self/cmdline",
        ])
        .output()
        .unwrap();
    let cmdline = assert_ran_clean(&by_dash_path);
    assert_eq!(
        cmdline.split('\0').next(),
        Some("-x/tool"),
        "{}",
        output_report(&by_dash_path)
    );
}

/// A home whose profile scans the workspace for `.env*`, and a workspace holding `count`
/// such files.
fn home_with_many_env_files(count: usize) -> (TempDir, PathBuf) {
    let (home, workspace) = home_with_workspace();
    profile(
        &home,
        &format!("{RW_WORKSPACE}[[mounts.scan]]\nroot = \"${{workspace}}\"\nnames = [\".env*\"]\n"),
    );
    for index in 0..count {
        home.write(format!("ws/.env.{index}"), "");
    }
    (home, workspace)
}

/// The built binary under a soft limit of 1024 open files, running `script` with
/// `/bin/sh -c` inside the isolation of `home`'s profile.
fn run_script_under_soft_limit_1024(home: &TempDir, workspace: &Path, script: &str) -> Output {
    let mut command = binary(home.path());
    command.current_dir(workspace);
    run_command_with_soft_fd_limit(
        command,
        1024,
        [
            "--workspace",
            workspace.to_str().unwrap(),
            "--",
            "/bin/sh",
            "-c",
            script,
        ],
    )
}

// @kotowari[REQ-309]
#[test]
fn a_scan_of_more_hidden_files_than_the_soft_limit_still_launches() {
    // Specification section 14: every hidden file needs a descriptor; with 1100 of them
    // and a soft limit of 1024, the start raises the soft limit to the hard limit first.
    let (home, workspace) = home_with_many_env_files(1100);

    let output = run_script_under_soft_limit_1024(&home, &workspace, "test ! -s .env.0");

    assert_eq!(output.status.code(), Some(0), "{}", output_report(&output));
}

// @kotowari[REQ-309]
#[test]
fn the_isolated_process_inherits_the_raised_soft_limit() {
    // The limit is raised whether or not it is needed, so the same input gives the same
    // result: inside, the soft limit equals the hard limit.
    let (home, workspace) = home_with_workspace();

    let output = run_script_under_soft_limit_1024(&home, &workspace, "ulimit -Sn; ulimit -Hn");

    let stdout = assert_ran_clean(&output);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2, "{}", output_report(&output));
    assert_eq!(lines[0], lines[1], "{}", output_report(&output));
    assert_ne!(lines[0], "1024", "{}", output_report(&output));
}

// A `bwrap` that passes the lookup on PATH but cannot be executed fails while kakoi is
// still running, so kakoi reports it.
// @kotowari[REQ-401, EX-755]
#[test]
fn a_bwrap_that_cannot_be_executed_is_a_bwrap_diagnostic() {
    let (home, workspace) = home_with_workspace();
    let tools = home.path().join("tools");
    home.write_executable("tools/bwrap", "#!/nonexistent/interpreter\n");
    let mut paths = vec![tools];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));

    let output = binary(home.path())
        .env("PATH", std::env::join_paths(paths).unwrap())
        .current_dir(&workspace)
        .args(["--", "/bin/true"])
        .output()
        .unwrap();

    assert_diagnostic(&output, 125, "bwrap");
}

// @kotowari[REQ-264, REQ-401, EX-756, EX-504]
#[test]
fn a_command_inside_a_hidden_directory_fails_at_exec_with_bwrap_status() {
    let (home, workspace) = home_with_workspace();
    let tool = home.write_executable("hidden/tool", "#!/bin/sh\necho ran\n");
    let hidden = home.path().join("hidden");
    profile(
        &home,
        &format!("{RW_WORKSPACE}hide = [\"{}\"]\n", hidden.display()),
    );

    let through_kakoi = binary(home.path())
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--",
            tool.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    // The same environment as `binary()` gives the product, so that the two exec-failure
    // lines are compared under the same locale.
    let mut bwrap_alone = Command::new("bwrap");
    bwrap_alone.env_clear();
    if let Some(path) = std::env::var_os("PATH") {
        bwrap_alone.env("PATH", path);
    }
    let bwrap_alone = bwrap_alone
        .args(["--ro-bind", "/", "/", "--tmpfs"])
        .arg(&hidden)
        .arg("--")
        .arg(&tool)
        .output()
        .unwrap();

    let report = format!(
        "{}\nbwrap alone: {}",
        output_report(&through_kakoi),
        output_report(&bwrap_alone)
    );
    assert_ne!(bwrap_alone.status.code(), Some(0), "{report}");
    assert_eq!(
        through_kakoi.status.code(),
        bwrap_alone.status.code(),
        "{report}"
    );
    assert!(through_kakoi.stdout.is_empty(), "{report}");
    assert_eq!(through_kakoi.stderr, bwrap_alone.stderr, "{report}");
}

// @kotowari[REQ-287, EX-523]
#[test]
fn two_concurrent_launches_both_pass_their_status_through() {
    let (home, workspace) = home_with_workspace();
    let start = |code: i32| {
        binary(home.path())
            .args([
                "--workspace",
                workspace.to_str().unwrap(),
                "--",
                "/bin/sh",
                "-c",
                &format!("sleep 0.3; exit {code}"),
            ])
            .spawn()
            .unwrap()
    };

    let mut first = start(5);
    let mut second = start(6);

    assert_eq!(first.wait().unwrap().code(), Some(5));
    assert_eq!(second.wait().unwrap().code(), Some(6));
}

// @kotowari[REQ-173]
#[test]
fn a_worktree_at_home_exits_125() {
    let (home, workspace) = home_with_workspace();
    let status = Command::new("git")
        .args(["init", "-q"])
        .arg(home.path())
        .status()
        .unwrap();
    assert!(status.success());

    let output = run_script(&home, &workspace, "exit 0");

    assert_diagnostic(&output, 125, "path");
}

// @kotowari[REQ-173, EX-349]
#[test]
fn a_cwd_inside_a_hide_exits_125() {
    let (home, workspace) = home_with_workspace();
    let boxed = home.path().join("box");
    std::fs::create_dir(&boxed).unwrap();
    profile(
        &home,
        &format!("{RW_WORKSPACE}hide = [\"{}\"]\n", boxed.display()),
    );

    let output = binary(home.path())
        .current_dir(&boxed)
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--",
            "/bin/true",
        ])
        .output()
        .unwrap();

    assert_diagnostic(&output, 125, "path");
}

/// Every entry under `root` except `ws` and the profile, as (relative path, kind, size).
fn tree_snapshot(root: &Path) -> Vec<(PathBuf, String, u64)> {
    fn walk(root: &Path, dir: &Path, into: &mut Vec<(PathBuf, String, u64)>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(Result::unwrap)
            .collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            if relative == Path::new("ws") || relative == Path::new(".config/kakoi/profile") {
                continue;
            }
            let metadata = std::fs::symlink_metadata(&path).unwrap();
            let kind = format!("{:?}", metadata.file_type());
            into.push((relative, kind, metadata.len()));
            if metadata.is_dir() {
                walk(root, &path, into);
            }
        }
    }
    let mut snapshot = Vec::new();
    walk(root, root, &mut snapshot);
    snapshot
}

// @kotowari[REQ-169]
#[test]
fn a_launch_leaves_the_host_tree_unchanged() {
    let (home, workspace) = home_with_workspace();
    home.write(".config/kakoi/secrets/token", "FAKE\n");
    home.write("cache/.keep", "");
    home.write("notes/a.md", "");
    home.write("ws/.env", "X=1\n");
    home.write("ws/sub/.env.local", "Y=2\n");
    profile(
        &home,
        "[mounts]\nrw = [\"${workspace}\", \"~/cache\"]\nro = [\"~/notes\"]\n\
         hide = [\"~/notes/a.md\", \"~/missing\"]\n\
         [[mounts.scan]]\nroot = \"${worktree}\"\nnames = [\".env\", \".env.*\"]\n\
         [secrets]\nTOKEN = \"${config_dir}/secrets/token\"\n",
    );
    let before = tree_snapshot(home.path());

    let output = run_script(&home, &workspace, "exit 0");

    assert_ran_clean(&output);
    assert_eq!(tree_snapshot(home.path()), before);
}

// @kotowari[REQ-172]
#[test]
fn a_hidden_ancestor_does_not_hide_the_rw_worktree() {
    // Scene 1 of specification section 6.4: `/mnt/c` hidden, the worktree in it `rw`.
    let home = TempDir::new();
    home.write("box/other/secret.txt", "s\n");
    let workspace = home.path().join("box/proj");
    std::fs::create_dir(&workspace).unwrap();
    profile(
        &home,
        &format!(
            "[mounts]\nrw = [\"${{worktree}}\"]\nhide = [\"{}\"]\n",
            home.path().join("box").display()
        ),
    );

    let output = run_script(
        &home,
        &workspace,
        "test ! -e ../other && echo written > from-inside && cat from-inside",
    );

    assert_eq!(assert_ran_clean(&output), "written\n");
    assert_eq!(
        std::fs::read_to_string(workspace.join("from-inside")).unwrap(),
        "written\n"
    );
}

// @kotowari[REQ-172]
#[test]
fn an_ro_file_inside_an_rw_directory_is_read_only() {
    // Scene 2: `~/.codex` rw, `~/.codex/AGENTS.md` ro.
    let (home, workspace) = home_with_workspace();
    home.write(".codex/AGENTS.md", "keep\n");
    profile(
        &home,
        "[mounts]\nrw = [\"${workspace}\", \"~/.codex\"]\nro = [\"~/.codex/AGENTS.md\"]\n",
    );

    let output = run_script(
        &home,
        &workspace,
        "if echo changed > ~/.codex/AGENTS.md; then echo agents-writable; fi; \
         echo other > ~/.codex/other && echo other-written",
    );

    assert_eq!(assert_ran_clean(&output), "other-written\n");
    assert_eq!(
        std::fs::read_to_string(home.path().join(".codex/AGENTS.md")).unwrap(),
        "keep\n"
    );
    assert_eq!(
        std::fs::read_to_string(home.path().join(".codex/other")).unwrap(),
        "other\n"
    );
}

// @kotowari[REQ-172]
#[test]
fn a_scanned_env_file_reads_empty() {
    // Scene 3: the worktree rw, `.env` files found by the scan hidden. Two of them, so
    // the launch carries more than one data descriptor.
    let (home, workspace) = home_with_workspace();
    home.write("ws/.env", "SECRET=x\n");
    home.write("ws/sub/.env.local", "SECRET=y\n");
    home.write("ws/sub/keep.txt", "kept\n");
    profile(
        &home,
        &format!(
            "{RW_WORKSPACE}[[mounts.scan]]\nroot = \"${{worktree}}\"\nnames = [\".env\", \".env.*\"]\n"
        ),
    );

    let output = run_script(
        &home,
        &workspace,
        "wc -c < .env; wc -c < sub/.env.local; cat sub/keep.txt",
    );

    assert_eq!(assert_ran_clean(&output), "0\n0\nkept\n");
    assert_eq!(
        std::fs::read_to_string(workspace.join(".env")).unwrap(),
        "SECRET=x\n"
    );
}

// @kotowari[REQ-172]
#[test]
fn the_shared_tmp_subdirectory_is_visible_inside_an_empty_tmp() {
    // Scene 4: `/tmp` hidden, one directory under it rw. The scene names `/tmp`, and
    // `TempDir::new` follows `TMPDIR`, which may point elsewhere: so the workspace that
    // stands in for `/tmp/kakoi` is placed under `/tmp` explicitly, beside a
    // marker that only the hiding of `/tmp` can make invisible (the root is `ro` bound).
    // Nothing on the host outside the temporary directories is touched.
    let home = TempDir::new();
    let shared = TempDir::under(Path::new("/tmp"));
    let marker = shared.write("marker", "");
    let workspace = shared.path().join("ws");
    std::fs::create_dir(&workspace).unwrap();
    profile(&home, &format!("{RW_WORKSPACE}hide = [\"/tmp\"]\n"));

    let output = run_script(
        &home,
        &workspace,
        &format!(
            "test ! -e {} && ls /tmp && echo shared > shared.txt",
            marker.display()
        ),
    );

    assert_eq!(
        assert_ran_clean(&output),
        format!("{}\n", shared.path().file_name().unwrap().to_str().unwrap())
    );
    assert_eq!(
        std::fs::read_to_string(workspace.join("shared.txt")).unwrap(),
        "shared\n"
    );
}

// @kotowari[REQ-172]
#[test]
fn a_scanned_file_under_an_rw_cache_reads_empty() {
    // Scene 5: `~/.cache` rw, `~/.cache/x/.env` hidden by the scan.
    let (home, workspace) = home_with_workspace();
    home.write(".cache/x/.env", "SECRET=z\n");
    profile(
        &home,
        "[mounts]\nrw = [\"${workspace}\", \"~/.cache\"]\n\
         [[mounts.scan]]\nroot = \"~/.cache\"\nnames = [\".env\"]\n",
    );

    let output = run_script(
        &home,
        &workspace,
        "wc -c < ~/.cache/x/.env; echo new > ~/.cache/x/other && cat ~/.cache/x/other",
    );

    assert_eq!(assert_ran_clean(&output), "0\nnew\n");
}

// @kotowari[REQ-167]
#[test]
fn rename_onto_an_rw_file_fails_but_in_place_writes_work() {
    let (home, workspace) = home_with_workspace();
    home.write("d/state.json", "{}\n");
    profile(
        &home,
        "[mounts]\nrw = [\"${workspace}\", \"~/d\"]\nrw-file = [\"~/d/state.json\"]\n",
    );

    let output = run_script(
        &home,
        &workspace,
        "echo replaced > ~/d/state.json.tmp; \
         if mv ~/d/state.json.tmp ~/d/state.json 2>/dev/null; then echo renamed; fi; \
         printf 'appended\\n' >> ~/d/state.json && echo in-place",
    );

    assert_eq!(assert_ran_clean(&output), "in-place\n");
    assert_eq!(
        std::fs::read_to_string(home.path().join("d/state.json")).unwrap(),
        "{}\nappended\n"
    );
}

// @kotowari[REQ-167]
#[test]
fn a_host_socket_visible_read_only_is_connectable() {
    let (home, workspace) = home_with_workspace();
    let socket_path = home.path().join("sockets/echo.sock");
    std::fs::create_dir(socket_path.parent().unwrap()).unwrap();
    let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0; 5];
        std::io::Read::read_exact(&mut stream, &mut buffer).unwrap();
        std::io::Write::write_all(&mut stream, &buffer).unwrap();
    });

    let output = run_script(
        &home,
        &workspace,
        &format!(
            "/usr/bin/python3 -c \"\
             import socket; s = socket.socket(socket.AF_UNIX); \
             s.connect('{}'); s.sendall(b'hello'); print(s.recv(5).decode())\"",
            socket_path.display()
        ),
    );

    assert_eq!(assert_ran_clean(&output), "hello\n");
    server.join().unwrap();
}

// @kotowari[REQ-388, EX-167, EX-716]
#[test]
fn network_none_has_no_route() {
    let (home, workspace) = home_with_workspace();
    profile(
        &home,
        &format!("{RW_WORKSPACE}[network]\nmode = \"none\"\n"),
    );

    // 192.0.2.1 is TEST-NET-1: never routed, so nothing is contacted even by mistake.
    let output = run_script(
        &home,
        &workspace,
        "/usr/bin/python3 -c \"\
         import socket, errno; s = socket.socket(); s.settimeout(5)\n\
         try:\n    s.connect(('192.0.2.1', 80)); print('connected')\n\
         except OSError as e:\n    print(e.errno == errno.ENETUNREACH)\"",
    );

    assert_eq!(assert_ran_clean(&output), "True\n");
}

// @kotowari[REQ-271]
#[test]
fn a_secret_is_readable_as_a_variable_and_the_file_is_empty() {
    let (home, workspace) = home_with_workspace();
    let value = "FAKE-SECRET-VALUE-not-a-real-credential";
    let file = home.write(".config/kakoi/secrets/token", format!("{value}\n"));
    home.write(".config/kakoi/secrets/unnamed", "also-not-real\n");
    profile(
        &home,
        &format!("{RW_WORKSPACE}[secrets]\nTOKEN = \"${{config_dir}}/secrets/token\"\n"),
    );

    let output = binary(home.path())
        .env("TOKEN", "from-the-host")
        .env("OTHER", "unset-by-file-absence")
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--",
            "/bin/sh",
            "-c",
            &format!(
                "printf '%s\\n' \"$TOKEN\"; wc -c < {}; ls {}",
                file.display(),
                file.parent().unwrap().display()
            ),
        ])
        .output()
        .unwrap();

    // The named secret file is there, empty; the unnamed one under `secrets/` is not
    // visible at all.
    assert_eq!(assert_ran_clean(&output), format!("{value}\n0\ntoken\n"));
}

// @kotowari[REQ-271, EX-508]
#[test]
fn a_secret_whose_file_is_missing_is_not_set_from_the_host() {
    let (home, workspace) = home_with_workspace();
    profile(
        &home,
        &format!("{RW_WORKSPACE}[secrets]\nGH_TOKEN = \"${{config_dir}}/secrets/none\"\n"),
    );

    let output = binary(home.path())
        .env("GH_TOKEN", "from-the-host")
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--",
            "/bin/sh",
            "-c",
            "if [ -z \"${GH_TOKEN+set}\" ]; then echo absent; fi",
        ])
        .output()
        .unwrap();

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert_eq!(output.stdout, b"absent\n", "{report}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(stderr.lines().count(), 1, "{report}");
    assert!(stderr.starts_with("kakoi: warning: "), "{report}");
}

// @kotowari[REQ-276]
#[test]
fn instead_of_appears_in_git_config_inside_with_host_entries_kept() {
    let (home, workspace) = home_with_workspace();
    profile(
        &home,
        &format!(
            "{RW_WORKSPACE}[git.instead-of]\n\"git@example.com:\" = \"https://example.com/\"\n"
        ),
    );

    let output = binary(home.path())
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "host.key")
        .env("GIT_CONFIG_VALUE_0", "from-the-host")
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--",
            "/usr/bin/git",
            "config",
            "--list",
        ])
        .output()
        .unwrap();

    let listing = assert_ran_clean(&output);
    let lines: Vec<&str> = listing.lines().collect();
    assert!(
        lines.contains(&"url.https://example.com/.insteadof=git@example.com:"),
        "{listing}"
    );
    assert!(lines.contains(&"host.key=from-the-host"), "{listing}");
}

/// A Python program run inside the isolation that makes one raw system call through
/// `ctypes` and prints the return value and `errno`.
fn raw_syscall_script(number: u64, arguments: &str) -> String {
    format!(
        "/usr/bin/python3 -c \"\
         import ctypes; libc = ctypes.CDLL(None, use_errno=True); \
         libc.syscall.restype = ctypes.c_long; \
         libc.syscall.argtypes = [ctypes.c_long, ctypes.c_int, ctypes.c_ulong, ctypes.c_void_p]; \
         r = libc.syscall({number}, {arguments}); print(r, ctypes.get_errno())\""
    )
}

// @kotowari[REQ-282]
#[test]
fn tiocsti_is_denied_with_eperm() {
    let (home, workspace) = home_with_workspace();

    let output = run_script(
        &home,
        &workspace,
        &raw_syscall_script(
            libc::SYS_ioctl as u64,
            &format!(
                "0, ctypes.c_ulong({}), ctypes.c_char_p(b'x')",
                libc::TIOCSTI
            ),
        ),
    );

    assert_eq!(assert_ran_clean(&output), format!("-1 {}\n", libc::EPERM));
}

// @kotowari[REQ-282, EX-518]
#[test]
fn tiocsti_with_high_bits_is_denied_with_eperm() {
    let (home, workspace) = home_with_workspace();
    // `TIOCSTI` is a `c_ulong` against glibc and a `c_int` against musl, so the shift is
    // written in 64 bits rather than in the constant's own type. The cast is what makes
    // the musl build compile; against glibc the same cast is the one clippy calls
    // unnecessary, so the lint is turned off here rather than the cast removed.
    #[allow(clippy::unnecessary_cast)]
    let request = libc::TIOCSTI as u64 | (1u64 << 32);

    let output = run_script(
        &home,
        &workspace,
        &raw_syscall_script(
            libc::SYS_ioctl as u64,
            &format!("0, ctypes.c_ulong({request}), ctypes.c_char_p(b'x')"),
        ),
    );

    assert_eq!(assert_ran_clean(&output), format!("-1 {}\n", libc::EPERM));
}

// @kotowari[REQ-281, EX-517]
#[test]
fn an_x32_syscall_kills_the_process() {
    let (home, workspace) = home_with_workspace();
    let x32_getpid = 0x4000_0000_u64 | libc::SYS_getpid as u64;

    let output = run_script(
        &home,
        &workspace,
        &raw_syscall_script(x32_getpid, "0, ctypes.c_ulong(0), None"),
    );

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(128 + libc::SIGSYS), "{report}");
    assert!(output.stdout.is_empty(), "{report}");
}

// @kotowari[REQ-284]
#[test]
fn a_nested_launch_runs_under_the_outer_boundary() {
    let (home, workspace) = home_with_workspace();
    home.write("box/file", "hidden outside\n");
    profile(
        &home,
        &format!(
            "{RW_WORKSPACE}hide = [\"{}\"]\n",
            home.path().join("box").display()
        ),
    );

    // The inner `kakoi` is the same binary, started from inside the isolation
    // with the environment the outer one assembled (`KAKOI=1` included).
    let output = run_script(
        &home,
        &workspace,
        &format!(
            "{} -- /bin/sh -c 'test ! -e {} && echo inner-ok'",
            env!("CARGO_BIN_EXE_kakoi"),
            home.path().join("box/file").display()
        ),
    );

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert_eq!(output.stdout, b"inner-ok\n", "{report}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(stderr.lines().count(), 1, "{report}");
    assert!(stderr.starts_with("kakoi: warning: "), "{report}");
}

// `rw-copy`: the isolation starts from the host's content, writes it freely, and the host
// keeps what it had (specification section 6.1).

/// A directory under `home` with `mode`.
fn directory(home: &TempDir, relative: &str, mode: u32) -> PathBuf {
    let path = home.path().join(relative);
    std::fs::create_dir_all(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
    path
}

/// A file under `home` with `body` and `mode`.
fn file(home: &TempDir, relative: &str, body: &str, mode: u32) -> PathBuf {
    let path = home.write(relative, body);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
    path
}

// @kotowari[REQ-167]
#[test]
fn an_rw_copy_file_is_written_inside_and_the_host_file_is_untouched() {
    let (home, workspace) = home_with_workspace();
    home.write("conf/settings.json", "{\"from\":\"host\"}\n");
    profile(
        &home,
        "[mounts]\nrw = [\"${workspace}\"]\nrw-copy = [\"~/conf/settings.json\"]\n",
    );
    let before = tree_snapshot(home.path());

    let output = run_script(
        &home,
        &workspace,
        "cat ~/conf/settings.json; \
         printf '{\"from\":\"inside\"}\\n' > ~/conf/settings.json && echo written; \
         cat ~/conf/settings.json",
    );

    // The host's content is what the isolation starts from, the write succeeds, and
    // reading it back shows what was written.
    assert_eq!(
        assert_ran_clean(&output),
        "{\"from\":\"host\"}\nwritten\n{\"from\":\"inside\"}\n"
    );
    // The host's file still holds what it held, and nothing of the run is left anywhere
    // under the home.
    assert_eq!(
        std::fs::read_to_string(home.path().join("conf/settings.json")).unwrap(),
        "{\"from\":\"host\"}\n"
    );
    assert_eq!(tree_snapshot(home.path()), before);
}

// @kotowari[REQ-167]
#[test]
fn an_rw_copy_directory_carries_the_host_tree_and_keeps_every_change_inside() {
    let (home, workspace) = home_with_workspace();
    file(&home, "conf/keep.txt", "host\n", 0o644);
    file(&home, "conf/sub/deep.txt", "deep\n", 0o600);
    file(&home, "conf/run-me", "#!/bin/sh\necho from-host\n", 0o755);
    directory(&home, "conf/empty", 0o755);
    profile(
        &home,
        "[mounts]\nrw = [\"${workspace}\"]\nrw-copy = [\"~/conf\"]\n",
    );
    let before = tree_snapshot(home.path());

    let output = run_script(
        &home,
        &workspace,
        "cat ~/conf/keep.txt ~/conf/sub/deep.txt; \
         ~/conf/run-me; \
         test -d ~/conf/empty && echo empty-is-there; \
         echo new > ~/conf/made.txt && cat ~/conf/made.txt; \
         echo changed > ~/conf/keep.txt && cat ~/conf/keep.txt; \
         printf '#!/bin/sh\\necho made-inside\\n' > ~/conf/mine && chmod +x ~/conf/mine && ~/conf/mine",
    );

    assert_eq!(
        assert_ran_clean(&output),
        // The host's files read as the host has them, the execute bit came with `run-me`,
        // the empty directory is there, a new file can be made, an existing one can be
        // overwritten, and a file made inside can be marked executable and run.
        "host\ndeep\nfrom-host\nempty-is-there\nnew\nchanged\nmade-inside\n"
    );
    assert_eq!(
        std::fs::read_to_string(home.path().join("conf/keep.txt")).unwrap(),
        "host\n"
    );
    assert!(!home.path().join("conf/made.txt").exists());
    assert!(!home.path().join("conf/mine").exists());
    assert_eq!(tree_snapshot(home.path()), before);
}

// The copy's own directory takes the host directory's mode, as every directory under it
// does.
// @kotowari[REQ-167]
#[test]
fn an_rw_copy_directory_keeps_the_mode_of_the_directory_itself() {
    let (home, workspace) = home_with_workspace();
    directory(&home, "conf", 0o700);
    directory(&home, "conf/sub", 0o750);
    profile(
        &home,
        "[mounts]\nrw = [\"${workspace}\"]\nrw-copy = [\"~/conf\"]\n",
    );
    let output = run_script(&home, &workspace, "stat -c %a ~/conf ~/conf/sub");
    assert_eq!(assert_ran_clean(&output), "700\n750\n");
}

// @kotowari[REQ-167]
#[test]
fn an_rw_copy_directory_reproduces_a_symbolic_link_as_a_link() {
    // A link is copied as a link with the same target text, not followed: the isolation
    // sees the tree the host has.
    let (home, workspace) = home_with_workspace();
    home.write("conf/real.txt", "behind the link\n");
    std::os::unix::fs::symlink("real.txt", home.path().join("conf/alias.txt")).unwrap();
    profile(
        &home,
        "[mounts]\nrw = [\"${workspace}\"]\nrw-copy = [\"~/conf\"]\n",
    );
    let before = tree_snapshot(home.path());

    let output = run_script(
        &home,
        &workspace,
        "test -L ~/conf/alias.txt && echo still-a-link; \
         readlink ~/conf/alias.txt; cat ~/conf/alias.txt",
    );

    assert_eq!(
        assert_ran_clean(&output),
        "still-a-link\nreal.txt\nbehind the link\n"
    );
    assert_eq!(tree_snapshot(home.path()), before);
}

// @kotowari[REQ-168]
#[test]
fn an_entry_an_rw_copy_cannot_reproduce_is_reported_in_the_plan_and_left_out() {
    // A socket is the host's own; a copy of one is not it, and no bwrap argument makes one
    // in a tmpfs. It is left out of the copy and the plan says so, rather than passing over
    // it without a word.
    let (home, workspace) = home_with_workspace();
    home.write("conf/keep.txt", "host\n");
    let socket_path = home.path().join("conf/ipc.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
    profile(
        &home,
        "[mounts]\nrw = [\"${workspace}\"]\nrw-copy = [\"~/conf\"]\n",
    );

    let plan = run(
        home.path(),
        [
            "--workspace",
            workspace.to_str().unwrap(),
            "--print-plan",
            "--",
            "/bin/true",
        ],
    );
    let output = run_script(&home, &workspace, "cat ~/conf/keep.txt; ls ~/conf");

    let report = output_report(&plan);
    let text = String::from_utf8(plan.stdout).unwrap();
    assert!(
        text.contains(&format!("not copied {}", socket_path.display())),
        "{report}"
    );
    assert!(text.contains("is not a regular file"), "{report}");
    // The rest of the directory is there, and the socket is not.
    assert_eq!(assert_ran_clean(&output), "host\nkeep.txt\n");
}

// @kotowari[REQ-169]
#[test]
fn an_rw_copy_of_a_path_that_does_not_exist_is_skipped_like_any_other_item() {
    // Nothing is mounted on a path that does not exist (specification section 6.2): the
    // item is skipped with its reason and no name is created on the host.
    let (home, workspace) = home_with_workspace();
    profile(
        &home,
        "[mounts]\nrw = [\"${workspace}\"]\nrw-copy = [\"~/missing\"]\n",
    );
    let before = tree_snapshot(home.path());

    let plan = run(
        home.path(),
        [
            "--workspace",
            workspace.to_str().unwrap(),
            "--print-plan",
            "--",
            "/bin/true",
        ],
    );
    let output = run_script(&home, &workspace, "test ! -e ~/missing && echo not-there");

    let report = output_report(&plan);
    assert!(
        String::from_utf8_lossy(&plan.stdout)
            .contains("skipped rw-copy `~/missing`: does not exist"),
        "{report}"
    );
    assert_eq!(assert_ran_clean(&output), "not-there\n");
    assert_eq!(tree_snapshot(home.path()), before);
}

// @kotowari[REQ-167, REQ-404, EX-762]
#[test]
fn an_rw_copy_of_something_that_is_neither_a_directory_nor_a_regular_file_is_a_path_diagnostic() {
    // A FIFO has no content to copy. The run stops rather than standing an empty regular
    // file in for it.
    let (home, workspace) = home_with_workspace();
    let fifo = home.path().join("pipe");
    let name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: `mkfifo` reads the NUL-terminated name and makes the FIFO.
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o644) }, 0);
    profile(
        &home,
        "[mounts]\nrw = [\"${workspace}\"]\nrw-copy = [\"~/pipe\"]\n",
    );

    let output = run_script(&home, &workspace, "exit 0");

    let diagnostic = assert_diagnostic(&output, 125, "path");
    assert!(diagnostic.contains("~/pipe"), "{diagnostic}");
    assert!(diagnostic.contains("not a regular file"), "{diagnostic}");
}

// @kotowari[REQ-168, REQ-404, EX-761]
#[test]
fn an_rw_copy_source_over_the_entry_limit_is_a_path_diagnostic() {
    // The content is held in memory twice over, so a source pointed at something large is
    // refused before the start rather than paged in.
    let (home, workspace) = home_with_workspace();
    for index in 0..=kakoi_core::copies::ENTRY_LIMIT {
        home.write(format!("conf/f{index}"), "x\n");
    }
    profile(
        &home,
        "[mounts]\nrw = [\"${workspace}\"]\nrw-copy = [\"~/conf\"]\n",
    );

    let output = run_script(&home, &workspace, "exit 0");

    let diagnostic = assert_diagnostic(&output, 125, "path");
    assert!(diagnostic.contains("~/conf"), "{diagnostic}");
    assert!(
        diagnostic.contains(&format!(
            "more than {} entries",
            kakoi_core::copies::ENTRY_LIMIT
        )),
        "{diagnostic}"
    );
}

// @kotowari[REQ-172]
#[test]
fn a_narrower_rw_inside_an_rw_copy_directory_still_reaches_the_host() {
    // The order of section 6.4 holds: the tmpfs is mounted at the `rw-copy` item and the
    // narrower `rw` after it, so that one directory keeps writing through to the host.
    let (home, workspace) = home_with_workspace();
    home.write("conf/settings.json", "host\n");
    home.write("conf/state/db", "host\n");
    profile(
        &home,
        "[mounts]\nrw = [\"${workspace}\", \"~/conf/state\"]\nrw-copy = [\"~/conf\"]\n",
    );

    let output = run_script(
        &home,
        &workspace,
        "echo inside > ~/conf/settings.json; echo inside > ~/conf/state/db; echo done",
    );

    assert_eq!(assert_ran_clean(&output), "done\n");
    assert_eq!(
        std::fs::read_to_string(home.path().join("conf/settings.json")).unwrap(),
        "host\n"
    );
    assert_eq!(
        std::fs::read_to_string(home.path().join("conf/state/db")).unwrap(),
        "inside\n"
    );
}

// @kotowari[REQ-158]
#[test]
fn a_policy_file_inside_an_rw_copy_area_launches_and_the_host_profile_is_untouched() {
    // An `rw-copy` area is not a writable place for the placement rules: what is written
    // there dies with the isolation, so the profile the next start reads cannot be changed
    // from inside. The launch is allowed, and the rewrite from inside stays inside.
    let (home, workspace) = home_with_workspace();
    let written = format!(
        "[mounts]\nrw = [\"${{workspace}}\"]\nrw-copy = [\"{}\"]\n",
        home.path().join(".config").display()
    );
    profile(&home, &written);

    let output = run_script(
        &home,
        &workspace,
        "echo '[mounts]' > ~/.config/kakoi/profile/default.toml && echo rewritten; \
         cat ~/.config/kakoi/profile/default.toml",
    );

    assert_eq!(assert_ran_clean(&output), "rewritten\n[mounts]\n");
    assert_eq!(
        std::fs::read_to_string(home.path().join(".config/kakoi/profile/default.toml")).unwrap(),
        written
    );
}

// @kotowari[REQ-297]
#[test]
fn the_plan_shows_an_rw_copy_item_in_every_form() {
    let (home, workspace) = home_with_workspace();
    home.write("conf/settings.json", "host\n");
    profile(
        &home,
        "[mounts]\nrw = [\"${workspace}\"]\nrw-copy = [\"~/conf\"]\n",
    );
    let workspace = workspace.to_str().unwrap();
    let real = home.path().join("conf");

    let summary = run(
        home.path(),
        ["--workspace", workspace, "--print-plan", "--", "/bin/true"],
    );
    let json = run(
        home.path(),
        [
            "--workspace",
            workspace,
            "--print-plan=json",
            "--",
            "/bin/true",
        ],
    );

    // The summary names the item with its directive and says what the directive does, so
    // that `rw-copy` is not read as an `rw`.
    let report = output_report(&summary);
    let text = String::from_utf8(summary.stdout).unwrap();
    assert!(text.contains("rw-copy ~/conf\n"), "{report}");
    assert!(
        text.contains("rw-copy starts from a copy of the host's content"),
        "{report}"
    );
    assert!(
        text.contains("nothing written there reaches the host"),
        "{report}"
    );

    // The JSON form carries the same item with the directive as its own string.
    let report = output_report(&json);
    let document: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    let items = document["mounts"].as_array().unwrap();
    let copied = items
        .iter()
        .find(|item| item["directive"] == "rw-copy")
        .unwrap_or_else(|| panic!("no rw-copy item: {report}"));
    assert_eq!(copied["path"], real.to_str().unwrap());
    assert_eq!(copied["kind"], "directory");
    assert_eq!(copied["written"], "~/conf");
    assert!(document["not_copied"].is_array(), "{report}");
    assert_eq!(
        document["policy"]["mounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["directive"] == "rw-copy")
            .unwrap()["path"],
        "~/conf"
    );
}

// @kotowari[REQ-168]
#[test]
fn a_copied_file_is_a_mount_point_but_an_entry_of_a_copied_directory_is_not() {
    // The two forms differ where it shows: a copied regular file is one mount point laid
    // over the host's, so `rename` over it and `unlink` fail as they do for `rw-file`; an
    // entry inside a copied directory is an ordinary file of the tmpfs and takes both.
    let (home, workspace) = home_with_workspace();
    home.write("cfg.toml", "host\n");
    home.write("d/inner.toml", "host\n");
    profile(
        &home,
        "[mounts]\nrw = [\"${workspace}\"]\nrw-copy = [\"~/cfg.toml\", \"~/d\"]\n",
    );
    let before = tree_snapshot(home.path());

    let output = run_script(
        &home,
        &workspace,
        "printf 'x\\n' > ~/d/t1; \
         if mv ~/d/t1 ~/cfg.toml 2>/dev/null; then echo file-renamed; fi; \
         if rm ~/cfg.toml 2>/dev/null; then echo file-removed; fi; \
         printf 'appended\\n' >> ~/cfg.toml && echo file-appended; \
         printf 'y\\n' > ~/d/t2 && mv ~/d/t2 ~/d/inner.toml && echo entry-renamed; \
         rm ~/d/inner.toml && echo entry-removed",
    );

    assert_eq!(
        assert_ran_clean(&output),
        "file-appended\nentry-renamed\nentry-removed\n"
    );
    assert_eq!(tree_snapshot(home.path()), before);
    assert_eq!(
        std::fs::read_to_string(home.path().join("cfg.toml")).unwrap(),
        "host\n"
    );
    assert_eq!(
        std::fs::read_to_string(home.path().join("d/inner.toml")).unwrap(),
        "host\n"
    );
}

// The environment handed to the command (specification: `docs/ir/core/core-environment.md`).

/// Runs `script` with `/bin/sh -c` in the isolation of `home`'s profile, with `variables`
/// added to the environment the binary starts with.
fn run_script_with_env(
    home: &TempDir,
    workspace: &Path,
    variables: &[(&str, &str)],
    script: &str,
) -> Output {
    let mut command = binary(home.path());
    for (name, value) in variables {
        command.env(name, value);
    }
    command
        .current_dir(workspace)
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--",
            "/bin/sh",
            "-c",
            script,
        ])
        .output()
        .unwrap()
}

// @kotowari[EX-505]
#[test]
fn a_name_both_unset_and_set_carries_the_set_value() {
    let (home, workspace) = home_with_workspace();
    profile(
        &home,
        &format!("{RW_WORKSPACE}[env]\nunset = [\"FOO\"]\nset = {{ FOO = \"from-set\" }}\n"),
    );

    let output = run_script_with_env(
        &home,
        &workspace,
        &[("FOO", "from-the-host")],
        "printf '[%s]' \"$FOO\"",
    );

    assert_eq!(assert_ran_clean(&output), "[from-set]");
}

// @kotowari[EX-506]
#[test]
fn the_bwrap_arguments_carry_neither_environment_flags_nor_values() {
    let (home, workspace) = home_with_workspace();
    let secret = "FAKE-SECRET-VALUE-not-a-real-credential";
    let set = "a-value-set-by-the-policy";
    home.write(".config/kakoi/secrets/token", format!("{secret}\n"));
    profile(
        &home,
        &format!(
            "{RW_WORKSPACE}[env]\nset = {{ SET_BY_POLICY = \"{set}\" }}\n\
             [secrets]\nTOKEN = \"${{config_dir}}/secrets/token\"\n"
        ),
    );

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

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let arguments = plan["bwrap_arguments"].as_array().unwrap();
    for argument in arguments {
        let text = argument.to_string();
        for flag in ["--setenv", "--unsetenv", "--clearenv"] {
            assert_ne!(argument["value"], flag, "{report}");
        }
        assert!(!text.contains(secret), "{report}");
        assert!(!text.contains(set), "{report}");
    }
}

// @kotowari[EX-507]
#[test]
fn clear_without_path_and_without_path_prepend_leaves_path_unset_without_a_warning() {
    let (home, workspace) = home_with_workspace();
    profile(&home, &format!("{RW_WORKSPACE}[env]\nmode = \"clear\"\n"));

    let output = binary(home.path())
        .current_dir(&workspace)
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--",
            "/usr/bin/env",
        ])
        .output()
        .unwrap();

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert!(output.stderr.is_empty(), "{report}");
    let environment = String::from_utf8(output.stdout).unwrap();
    assert!(
        !environment.lines().any(|line| line.starts_with("PATH=")),
        "{report}"
    );
}

// @kotowari[EX-509]
#[test]
fn a_secret_loses_only_one_trailing_line_feed() {
    let (home, workspace) = home_with_workspace();
    home.write(".config/kakoi/secrets/token", "v\n\n");
    profile(
        &home,
        &format!("{RW_WORKSPACE}[secrets]\nTOKEN = \"${{config_dir}}/secrets/token\"\n"),
    );

    let output = run_script(&home, &workspace, "printf '[%s]' \"$TOKEN\"");

    assert_eq!(assert_ran_clean(&output), "[v\n]");
}

// @kotowari[EX-510]
#[test]
fn a_secret_of_65537_bytes_is_a_secret_diagnostic() {
    let (home, workspace) = home_with_workspace();
    home.write(".config/kakoi/secrets/token", "v".repeat(65537));
    profile(
        &home,
        &format!("{RW_WORKSPACE}[secrets]\nTOKEN = \"${{config_dir}}/secrets/token\"\n"),
    );

    let output = run_script(&home, &workspace, "exit 0");

    assert_diagnostic(&output, 125, "secret");
}

// @kotowari[EX-511]
#[test]
fn a_non_numeric_git_config_count_from_a_secret_is_an_env_diagnostic_without_its_value() {
    let (home, workspace) = home_with_workspace();
    let value = "FAKE-SECRET-VALUE-not-a-number";
    home.write(".config/kakoi/secrets/count", format!("{value}\n"));
    profile(
        &home,
        &format!(
            "{RW_WORKSPACE}[secrets]\nGIT_CONFIG_COUNT = \"${{config_dir}}/secrets/count\"\n\
             [git.instead-of]\n\"git@example.com:\" = \"https://example.com/\"\n"
        ),
    );

    let output = run_script(&home, &workspace, "exit 0");

    let diagnostic = assert_diagnostic(&output, 125, "env");
    assert!(!diagnostic.contains(value), "{diagnostic}");
}

// @kotowari[EX-512]
#[test]
fn an_inherited_variable_no_unset_pattern_matches_stays() {
    let (home, workspace) = home_with_workspace();
    profile(
        &home,
        &format!("{RW_WORKSPACE}[env]\nmode = \"inherit\"\nunset = [\"*_TOKEN\"]\n"),
    );

    let output = run_script_with_env(
        &home,
        &workspace,
        &[("TOKEN", "from-the-host")],
        "printf '[%s]' \"$TOKEN\"",
    );

    assert_eq!(assert_ran_clean(&output), "[from-the-host]");
}

// @kotowari[EX-513]
#[test]
fn instead_of_is_numbered_after_the_host_entries_it_keeps() {
    let (home, workspace) = home_with_workspace();
    profile(
        &home,
        &format!(
            "{RW_WORKSPACE}[git.instead-of]\n\"git@example.com:\" = \"https://example.com/\"\n"
        ),
    );

    let output = run_script_with_env(
        &home,
        &workspace,
        &[
            ("GIT_CONFIG_COUNT", "1"),
            ("GIT_CONFIG_KEY_0", "host.key"),
            ("GIT_CONFIG_VALUE_0", "from-the-host"),
        ],
        "printf '%s\\n' \"$GIT_CONFIG_KEY_0\" \"$GIT_CONFIG_VALUE_0\" \
         \"$GIT_CONFIG_KEY_1\" \"$GIT_CONFIG_VALUE_1\"",
    );

    assert_eq!(
        assert_ran_clean(&output),
        "host.key\nfrom-the-host\nurl.https://example.com/.insteadof\ngit@example.com:\n"
    );
}

// @kotowari[EX-514]
#[test]
fn an_empty_git_config_count_without_instead_of_is_kept_as_is() {
    let (home, workspace) = home_with_workspace();

    let output = run_script_with_env(
        &home,
        &workspace,
        &[("GIT_CONFIG_COUNT", "")],
        "if [ \"${GIT_CONFIG_COUNT+set}\" = set ]; then printf '[%s]' \"$GIT_CONFIG_COUNT\"; fi",
    );

    assert_eq!(assert_ran_clean(&output), "[]");
}

// Mount items (specification: `docs/ir/core/core-mounts.md`).

/// The JSON plan of `home`'s profile with the workspace at `workspace`, after checking the
/// binary exited 0.
fn json_plan(home: &TempDir, workspace: &Path) -> serde_json::Value {
    let output = binary(home.path())
        .current_dir(workspace)
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
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| panic!("{error}: {report}"))
}

/// The items of `plan` mounted at `path`, as their directives.
fn directives_at<'a>(plan: &'a serde_json::Value, path: &Path) -> Vec<&'a str> {
    plan["mounts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["path"] == path.to_str().unwrap())
        .map(|item| item["directive"].as_str().unwrap())
        .collect()
}

/// The position of the item mounted at `path` in the mount order of `plan`.
fn mount_position(plan: &serde_json::Value, path: &Path) -> usize {
    plan["mounts"]
        .as_array()
        .unwrap()
        .iter()
        .position(|item| item["path"] == path.to_str().unwrap())
        .unwrap_or_else(|| panic!("nothing is mounted at {}: {plan}", path.display()))
}

// @kotowari[EX-336]
#[test]
fn an_rw_directory_is_written_through_to_the_host() {
    let (home, workspace) = home_with_workspace();
    std::fs::create_dir(home.path().join("data")).unwrap();
    profile(&home, "[mounts]\nrw = [\"${workspace}\", \"~/data\"]\n");

    let output = run_script(&home, &workspace, "echo inside > ~/data/made && echo done");

    assert_eq!(assert_ran_clean(&output), "done\n");
    assert_eq!(
        std::fs::read_to_string(home.path().join("data/made")).unwrap(),
        "inside\n"
    );
}

// @kotowari[EX-337]
#[test]
fn rw_on_a_regular_file_is_a_path_diagnostic_pointing_to_rw_file() {
    let (home, workspace) = home_with_workspace();
    home.write("state.json", "{}\n");
    profile(
        &home,
        "[mounts]\nrw = [\"${workspace}\", \"~/state.json\"]\n",
    );

    let output = run_script(&home, &workspace, "exit 0");

    let diagnostic = assert_diagnostic(&output, 125, "path");
    assert!(diagnostic.contains("rw-file"), "{diagnostic}");
}

// @kotowari[EX-338]
#[test]
fn a_file_in_an_rw_copy_directory_is_renamed_inside_only() {
    let (home, workspace) = home_with_workspace();
    home.write("conf/before", "host\n");
    profile(
        &home,
        "[mounts]\nrw = [\"${workspace}\"]\nrw-copy = [\"~/conf\"]\n",
    );

    let output = run_script(
        &home,
        &workspace,
        "mv ~/conf/before ~/conf/after && test ! -e ~/conf/before && cat ~/conf/after",
    );

    assert_eq!(assert_ran_clean(&output), "host\n");
    assert_eq!(
        std::fs::read_to_string(home.path().join("conf/before")).unwrap(),
        "host\n"
    );
    assert!(home.path().join("conf/after").symlink_metadata().is_err());
}

// @kotowari[EX-339]
#[test]
fn an_rw_copy_of_more_than_4096_entries_is_a_path_diagnostic() {
    let (home, workspace) = home_with_workspace();
    for index in 0..4097 {
        home.write(format!("conf/f{index}"), "x\n");
    }
    profile(
        &home,
        "[mounts]\nrw = [\"${workspace}\"]\nrw-copy = [\"~/conf\"]\n",
    );

    let output = run_script(&home, &workspace, "exit 0");

    assert_diagnostic(&output, 125, "path");
}

// @kotowari[EX-340]
#[test]
fn an_item_is_mounted_at_the_real_path_behind_its_link() {
    let (home, workspace) = home_with_workspace();
    let real = home.path().join("real");
    std::fs::create_dir(&real).unwrap();
    std::os::unix::fs::symlink(&real, home.path().join("link")).unwrap();
    profile(&home, &format!("{RW_WORKSPACE}ro = [\"~/link\"]\n"));

    let plan = json_plan(&home, &workspace);

    let real = real.canonicalize().unwrap();
    assert_eq!(directives_at(&plan, &real), ["ro"], "{plan}");
    assert!(
        directives_at(&plan, &home.path().join("link")).is_empty(),
        "{plan}"
    );
}

// @kotowari[EX-341]
#[test]
fn a_hide_whose_target_does_not_exist_is_skipped_with_a_reason_and_not_created() {
    let (home, workspace) = home_with_workspace();
    profile(&home, &format!("{RW_WORKSPACE}hide = [\"~/missing\"]\n"));

    let plan = json_plan(&home, &workspace);
    let output = run_script(&home, &workspace, "exit 0");

    let skipped = plan["skipped_mounts"].as_array().unwrap();
    let entry = skipped
        .iter()
        .find(|entry| entry["directive"] == "hide" && entry["written"] == "~/missing")
        .unwrap_or_else(|| panic!("the hide is not skipped: {plan}"));
    assert!(
        !entry["reason"].as_str().unwrap_or_default().is_empty(),
        "{plan}"
    );
    assert_ran_clean(&output);
    assert!(home.path().join("missing").symlink_metadata().is_err());
}

// @kotowari[EX-342]
#[test]
fn the_secrets_directory_of_the_configuration_directory_is_hidden() {
    let (home, workspace) = home_with_workspace();
    home.write(".config/kakoi/secrets/unreferenced", "FAKE\n");

    let plan = json_plan(&home, &workspace);

    let secrets = home
        .path()
        .join(".config/kakoi/secrets")
        .canonicalize()
        .unwrap();
    assert_eq!(directives_at(&plan, &secrets), ["hide"], "{plan}");
}

// @kotowari[EX-343]
#[test]
fn a_scanned_link_to_a_directory_is_not_hidden() {
    let (home, workspace) = home_with_workspace();
    let target = home.path().join("target");
    std::fs::create_dir(&target).unwrap();
    std::os::unix::fs::symlink(&target, workspace.join(".env")).unwrap();
    profile(
        &home,
        &format!("{RW_WORKSPACE}[[mounts.scan]]\nroot = \"${{workspace}}\"\nnames = [\".env\"]\n"),
    );

    let plan = json_plan(&home, &workspace);

    let hidden: Vec<&serde_json::Value> = plan["mounts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["directive"] == "hide")
        .collect();
    assert!(hidden.is_empty(), "{plan}");
}

// @kotowari[EX-344]
#[test]
fn a_generated_hide_replaces_a_written_ro_on_the_same_real_path() {
    let (home, workspace) = home_with_workspace();
    let env_file = home.write("ws/.env", "SECRET=x\n");
    profile(
        &home,
        "[mounts]\nrw = [\"${workspace}\"]\nro = [\"${workspace}/.env\"]\n\
         [[mounts.scan]]\nroot = \"${workspace}\"\nnames = [\".env\"]\n",
    );

    let plan = json_plan(&home, &workspace);

    assert_eq!(
        directives_at(&plan, &env_file.canonicalize().unwrap()),
        ["hide"],
        "{plan}"
    );
}

// @kotowari[EX-345]
#[test]
fn a_scan_root_under_an_rw_the_generation_will_hide_is_checked_against_the_rw() {
    let (home, workspace) = home_with_workspace();
    let root = home.path().join(".config/kakoi/secrets/sub");
    std::fs::create_dir_all(&root).unwrap();
    profile(
        &home,
        &format!(
            "[mounts]\nrw = [\"${{workspace}}\", \"${{config_dir}}/secrets\"]\n\
             [[mounts.scan]]\nroot = \"{}\"\nnames = [\".env\"]\n",
            root.display()
        ),
    );

    let output = run_script(&home, &workspace, "exit 0");

    let diagnostic = assert_diagnostic(&output, 125, "path");
    assert!(diagnostic.contains(root.to_str().unwrap()), "{diagnostic}");
}

// @kotowari[EX-346]
#[test]
fn a_narrower_rw_is_mounted_after_the_hide_around_it() {
    let (home, workspace) = home_with_workspace();
    let wide = home.path().join("t");
    let narrow = wide.join("kakoi");
    std::fs::create_dir_all(&narrow).unwrap();
    profile(
        &home,
        &format!(
            "[mounts]\nrw = [\"${{workspace}}\", \"{}\"]\nhide = [\"{}\"]\n",
            narrow.display(),
            wide.display()
        ),
    );

    let plan = json_plan(&home, &workspace);

    let (wide, narrow) = (wide.canonicalize().unwrap(), narrow.canonicalize().unwrap());
    assert_eq!(directives_at(&plan, &wide), ["hide"], "{plan}");
    assert_eq!(directives_at(&plan, &narrow), ["rw"], "{plan}");
    assert!(
        mount_position(&plan, &wide) < mount_position(&plan, &narrow),
        "{plan}"
    );
}

// @kotowari[EX-347]
#[test]
fn siblings_are_mounted_in_the_byte_order_of_their_real_paths() {
    let (home, workspace) = home_with_workspace();
    let first = home.path().join("a");
    let second = home.path().join("b");
    std::fs::create_dir(&first).unwrap();
    std::fs::create_dir(&second).unwrap();
    profile(&home, &format!("{RW_WORKSPACE}ro = [\"~/b\", \"~/a\"]\n"));

    let plan = json_plan(&home, &workspace);

    assert!(
        mount_position(&plan, &first.canonicalize().unwrap())
            < mount_position(&plan, &second.canonicalize().unwrap()),
        "{plan}"
    );
}

// @kotowari[EX-348]
#[test]
fn a_work_place_under_rw_copy_only_warns_and_starts() {
    let (home, workspace) = home_with_workspace();
    profile(&home, "[mounts]\nrw-copy = [\"${workspace}\"]\n");

    let output = run_script(&home, &workspace, "echo ran");

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert_eq!(output.stdout, b"ran\n", "{report}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(stderr.lines().count(), 1, "{report}");
    assert!(stderr.starts_with("kakoi: warning: "), "{report}");
}

// The seccomp filter, the process, and the runtime (specification:
// `docs/ir/core/core-terminal.md`, `core-process.md`, `core-runtime.md`).

/// A Python program that makes the i386 `getpid` system call with `int 0x80` from a second
/// thread and prints `survived` once that thread is joined.
const I386_SYSCALL_FROM_A_THREAD: &str = "\
import ctypes, mmap, threading
code = bytes([0xb8, 20, 0, 0, 0, 0xcd, 0x80, 0xc3])
page = mmap.mmap(-1, mmap.PAGESIZE, prot=mmap.PROT_READ | mmap.PROT_WRITE | mmap.PROT_EXEC)
page.write(code)
call = ctypes.CFUNCTYPE(ctypes.c_long)(ctypes.addressof(ctypes.c_char.from_buffer(page)))
thread = threading.Thread(target=call)
thread.start()
thread.join()
print('survived')
";

// @kotowari[EX-516]
#[test]
fn an_i386_system_call_ends_the_whole_process() {
    let (home, workspace) = home_with_workspace();
    let program = home.write("ws/i386.py", I386_SYSCALL_FROM_A_THREAD);
    // The host kernel runs i386 system calls, so what ends the process inside is the filter.
    let on_the_host = Command::new("/usr/bin/python3")
        .arg(&program)
        .output()
        .unwrap();
    assert_eq!(
        (on_the_host.status.code(), on_the_host.stdout.as_slice()),
        (Some(0), b"survived\n".as_slice()),
        "{}",
        output_report(&on_the_host)
    );

    let output = run_script(&home, &workspace, "exec /usr/bin/python3 i386.py");

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(128 + libc::SIGSYS), "{report}");
    assert!(output.stdout.is_empty(), "{report}");
}

// @kotowari[EX-519, REQ-283]
#[test]
fn a_terminal_size_ioctl_is_allowed() {
    let (home, workspace) = home_with_workspace();

    let output = run_script(
        &home,
        &workspace,
        "/usr/bin/python3 -c \"\
         import os, fcntl, termios; controller, terminal = os.openpty(); \
         fcntl.ioctl(terminal, termios.TIOCGWINSZ, bytes(8)); print('allowed')\"",
    );

    assert_eq!(assert_ran_clean(&output), "allowed\n");
}

/// A Python program that runs its arguments on a new pseudo-terminal, as a user's shell
/// would under a terminal, and passes on what they printed and their exit code.
const UNDER_A_TERMINAL: &str = "\
import os, pty, sys
pid, controller = pty.fork()
if pid == 0:
    os.execvp(sys.argv[1], sys.argv[1:])
out = b''
while True:
    try:
        chunk = os.read(controller, 4096)
    except OSError:
        break
    if not chunk:
        break
    out += chunk
_, status = os.waitpid(pid, 0)
sys.stdout.write(out.decode())
sys.exit(os.waitstatus_to_exitcode(status))
";

// @kotowari[REQ-283]
#[test]
fn the_command_keeps_the_controlling_terminal() {
    let (home, workspace) = home_with_workspace();
    let runner = home.write("under-a-terminal.py", UNDER_A_TERMINAL);
    let opens_the_terminal = "exec 3</dev/tty && echo has-a-terminal";
    // A new session loses the controlling terminal, and `/dev/tty` cannot be opened.
    let new_session = Command::new("/usr/bin/python3")
        .arg(&runner)
        .args(["bwrap", "--ro-bind", "/", "/", "--dev", "/dev"])
        .args(["--new-session", "/bin/sh", "-c", opens_the_terminal])
        .output()
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&new_session.stdout).contains("has-a-terminal"),
        "{}",
        output_report(&new_session)
    );

    let output = Command::new("/usr/bin/python3")
        .arg(&runner)
        .arg(env!("CARGO_BIN_EXE_kakoi"))
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--",
            "/bin/sh",
            "-c",
            opens_the_terminal,
        ])
        .env_clear()
        .envs(
            binary(home.path())
                .get_envs()
                .filter_map(|(name, value)| value.map(|value| (name, value))),
        )
        .current_dir(&workspace)
        .output()
        .unwrap();

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("has-a-terminal"),
        "{report}"
    );
}

// @kotowari[EX-520, EX-522, EX-547, REQ-286]
#[test]
fn kakoi_1_on_the_host_runs_the_command_itself_with_the_environment_as_received() {
    let home = TempDir::new();
    let empty_path = TempDir::new();
    let mut command = binary(home.path());
    command
        .env("KAKOI", "1")
        .env("PATH", empty_path.path())
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

    // The shell prints its own process ID, the process kakoi became if it executed the
    // command itself rather than starting an isolation around it, and the environment it
    // was started with (not its own, to which it adds `PWD`).
    let child = command
        .args([
            "--",
            "/bin/sh",
            "-c",
            "echo $$; /usr/bin/tr '\\0' '\\n' < /proc/$$/environ",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    let output = child.wait_with_output().unwrap();

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    let mut lines = stdout.lines();
    assert_eq!(lines.next(), Some(pid.to_string().as_str()), "{report}");
    let inside: std::collections::BTreeSet<String> = lines.map(str::to_string).collect();
    assert_eq!(inside, expected, "{report}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(stderr.lines().count(), 1, "{report}");
    assert!(stderr.starts_with("kakoi: warning: "), "{report}");
}

// @kotowari[EX-521]
#[test]
fn a_nested_plan_of_a_missing_named_profile_is_a_policy_diagnostic() {
    let (home, workspace) = home_with_workspace();

    let output = binary(home.path())
        .env("KAKOI", "1")
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--profile",
            "missing",
            "--print-plan",
        ])
        .output()
        .unwrap();

    assert_diagnostic(&output, 125, "policy");
}

// @kotowari[REQ-286]
#[test]
fn a_kakoi_started_inside_without_kakoi_1_isolates_again_within_the_outer_boundary() {
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

    // The inner run asks for `~/data` as `rw`; the outer isolation has it read-only.
    let output = run_script(
        &home,
        &workspace,
        &format!(
            "env -u KAKOI {} --rw {} -- /bin/sh -c \
             'test ! -e {} && echo still-hidden; echo x > {} || echo not-writable'",
            env!("CARGO_BIN_EXE_kakoi"),
            home.path().join("data").display(),
            home.path().join("box/file").display(),
            home.path().join("data/file").display(),
        ),
    );

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert_eq!(output.stdout, b"still-hidden\nnot-writable\n", "{report}");
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("kakoi: warning:"),
        "{report}"
    );
    assert!(!home.path().join("data/file").exists());
}

// @kotowari[REQ-286]
#[test]
fn a_kakoi_started_inside_without_kakoi_1_finds_the_secret_emptied() {
    let (home, workspace) = home_with_workspace();
    home.write("token", "FAKE-TOKEN\n");
    profile(
        &home,
        &format!("{RW_WORKSPACE}[secrets]\nTOKEN = \"~/token\"\n"),
    );

    let output = run_script(
        &home,
        &workspace,
        &format!("env -u KAKOI {} -- /bin/true", env!("CARGO_BIN_EXE_kakoi")),
    );

    assert_diagnostic(&output, 125, "secret");
}

// @kotowari[REQ-314]
#[test]
fn the_host_environment_chooses_the_profile_the_nesting_and_the_work_place() {
    let home = TempDir::new();
    let first = home.write("first/kakoi/profile/default.toml", RW_WORKSPACE);
    let second = home.write("second/kakoi/profile/default.toml", RW_WORKSPACE);
    let place = home.path().join("place");
    std::fs::create_dir(&place).unwrap();
    let bin = TempDir::new();
    let tool = bin.write_executable("tool", "#!/bin/sh\n");
    let mut path = vec![bin.path().to_path_buf()];
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let plan = |config_home: &str, nested: bool| {
        let mut command = binary(home.path());
        if nested {
            command.env("KAKOI", "1");
        }
        let output = command
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

    let from_first = plan("first", false);
    let from_second = plan("second", true);

    assert_eq!(
        from_first["policy_sources"][0]["path"],
        first.to_str().unwrap()
    );
    assert_eq!(
        from_second["policy_sources"][0]["path"],
        second.to_str().unwrap()
    );
    assert_eq!(from_first["nested"], false);
    assert_eq!(from_second["nested"], true);
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

// @kotowari[EX-541]
#[test]
fn the_full_plan_shows_descriptors_as_symbols_not_numbers() {
    let (home, workspace) = home_with_workspace();
    home.write("conf/copied", "content\n");
    home.write("ws/.env", "SECRET=x\n");
    profile(
        &home,
        "[mounts]\nrw = [\"${workspace}\"]\nrw-copy = [\"~/conf\"]\n\
         [[mounts.scan]]\nroot = \"${workspace}\"\nnames = [\".env\"]\n",
    );

    let output = run(
        home.path(),
        [
            "--workspace",
            workspace.to_str().unwrap(),
            "--print-plan=full",
            "--",
            "/bin/true",
        ],
    );

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let plan = String::from_utf8(output.stdout).unwrap();
    let arguments: Vec<&str> = plan
        .split_once("bwrap arguments:\n")
        .unwrap_or_else(|| panic!("no bwrap arguments: {report}"))
        .1
        .lines()
        .map(str::trim)
        .collect();
    // The bwrap options whose first value is a file descriptor.
    let taking_a_descriptor = ["--seccomp", "--file", "--ro-bind-data", "--bind-data"];
    let descriptors: Vec<&str> = arguments
        .windows(2)
        .filter(|pair| taking_a_descriptor.contains(&pair[0]))
        .map(|pair| pair[1])
        .collect();
    assert!(descriptors.len() >= 3, "{report}");
    for descriptor in descriptors {
        assert!(
            descriptor.parse::<u64>().is_err(),
            "{descriptor} is a number: {report}"
        );
    }
}

// @kotowari[EX-542]
#[test]
fn more_hidden_files_than_the_soft_limit_start_and_the_command_gets_the_raised_limit() {
    let (home, workspace) = home_with_many_env_files(1100);

    let output = run_script_under_soft_limit_1024(&home, &workspace, "ulimit -Sn; ulimit -Hn");

    let stdout = assert_ran_clean(&output);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2, "{}", output_report(&output));
    assert_eq!(lines[0], lines[1], "{}", output_report(&output));
}

// @kotowari[EX-543]
#[test]
fn a_fifo_policy_file_is_a_policy_diagnostic_without_waiting() {
    let (home, workspace) = home_with_workspace();
    let fifo = home.path().join("policy.fifo");
    let name = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    // SAFETY: `mkfifo` reads the NUL-terminated path and makes the FIFO.
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);

    // Nothing ever opens the FIFO for writing.
    let mut child = binary(home.path())
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--policy-file",
            fifo.to_str().unwrap(),
            "--",
            "/bin/true",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while child.try_wait().unwrap().is_none() {
        if std::time::Instant::now() > deadline {
            child.kill().unwrap();
            panic!("kakoi waited on the FIFO");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();

    assert_diagnostic(&output, 125, "policy");
}

// @kotowari[EX-545]
#[test]
fn a_secret_file_behind_a_link_gives_the_value_of_its_target() {
    let (home, workspace) = home_with_workspace();
    let target = home.write("secret-target", "from-the-target\n");
    std::os::unix::fs::symlink(&target, home.path().join("secret-link")).unwrap();
    profile(
        &home,
        &format!("{RW_WORKSPACE}[secrets]\nTOKEN = \"~/secret-link\"\n"),
    );

    let output = run_script(&home, &workspace, "printf '[%s]' \"$TOKEN\"");

    assert_eq!(assert_ran_clean(&output), "[from-the-target]");
}

/// Makes a chain of `count` symbolic links in `dir`, `l0` to `l1` and so on, the last one
/// pointing at `target`, and returns `l0`: a path that takes `count` links to resolve.
fn chain_of_links(dir: &Path, count: usize, target: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    for index in 0..count {
        let next = if index + 1 == count {
            target.to_path_buf()
        } else {
            dir.join(format!("l{}", index + 1))
        };
        std::os::unix::fs::symlink(next, dir.join(format!("l{index}"))).unwrap();
    }
    dir.join("l0")
}

// @kotowari[EX-546, REQ-313]
#[test]
fn a_written_item_behind_more_than_40_links_is_skipped_as_having_no_real_path() {
    let (home, workspace) = home_with_workspace();
    let real = home.path().join("real");
    std::fs::create_dir(&real).unwrap();
    let forty = chain_of_links(&home.path().join("forty"), 40, &real);
    let forty_one = chain_of_links(&home.path().join("forty-one"), 41, &real);

    for (start, reached) in [(&forty, true), (&forty_one, false)] {
        profile(
            &home,
            &format!("{RW_WORKSPACE}ro = [\"{}\"]\n", start.display()),
        );

        let plan = json_plan(&home, &workspace);

        let mounted = directives_at(&plan, &real.canonicalize().unwrap()) == ["ro"];
        let skipped = plan["skipped_mounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| {
                entry["written"] == start.to_str().unwrap()
                    && !entry["reason"].as_str().unwrap_or_default().is_empty()
            });
        assert_eq!((mounted, skipped), (reached, !reached), "{plan}");
    }
}

// @kotowari[REQ-313]
#[test]
fn a_workspace_or_path_prepend_entry_behind_more_than_40_links_counts_as_missing() {
    let (home, workspace) = home_with_workspace();
    let bin = workspace.join("bin");
    std::fs::create_dir(&bin).unwrap();

    // The workspace: 40 links resolve, 41 are a workspace that does not exist.
    for (count, resolves) in [(40, true), (41, false)] {
        let start = chain_of_links(&home.path().join(format!("ws{count}")), count, &workspace);
        let output = binary(home.path())
            .current_dir(&workspace)
            .args(["--workspace", start.to_str().unwrap(), "--print-plan=json"])
            .output()
            .unwrap();
        if resolves {
            assert_eq!(output.status.code(), Some(0), "{}", output_report(&output));
        } else {
            assert_diagnostic(&output, 125, "path");
        }
    }

    // A protected path: behind 40 links the resolution reaches the `rw` workspace and is
    // refused; behind 41 it stops before it, and the entry is skipped as missing.
    for (count, reaches_the_workspace) in [(40, true), (41, false)] {
        let start = chain_of_links(&home.path().join(format!("bin{count}")), count, &bin);
        profile(
            &home,
            &format!(
                "{RW_WORKSPACE}[env]\npath-prepend = [\"{}\"]\n",
                start.display()
            ),
        );
        let output = binary(home.path())
            .current_dir(&workspace)
            .args([
                "--workspace",
                workspace.to_str().unwrap(),
                "--print-plan=json",
            ])
            .output()
            .unwrap();
        if reaches_the_workspace {
            assert_diagnostic(&output, 125, "path");
        } else {
            let report = output_report(&output);
            assert_eq!(output.status.code(), Some(0), "{report}");
            let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert!(
                plan["skipped_paths"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|entry| entry["written"] == start.to_str().unwrap()),
                "{report}"
            );
        }
    }
}

// @kotowari[REQ-313]
#[test]
fn a_protected_path_behind_more_than_40_links_is_checked_up_to_where_it_stopped() {
    // `l1` of the 41 links sits in the `rw` workspace and is followed second; the rest sit
    // outside every writable item. The resolution stops before the 41st link, but what it
    // referenced until then is still checked.
    let (home, workspace) = home_with_workspace();
    let outside = home.path().join("outside");
    let inside = workspace.join("mid");
    std::fs::create_dir(&outside).unwrap();
    std::fs::create_dir(&inside).unwrap();
    let target = home.path().join("elsewhere");
    std::fs::create_dir(&target).unwrap();
    let place = |index: usize| {
        if index == 1 {
            inside.join(format!("l{index}"))
        } else {
            outside.join(format!("l{index}"))
        }
    };
    for index in 0..41 {
        let next = if index == 40 {
            target.clone()
        } else {
            place(index + 1)
        };
        std::os::unix::fs::symlink(next, place(index)).unwrap();
    }
    profile(
        &home,
        &format!(
            "{RW_WORKSPACE}[env]\npath-prepend = [\"{}\"]\n",
            place(0).display()
        ),
    );

    let output = binary(home.path())
        .current_dir(&workspace)
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--print-plan=json",
        ])
        .output()
        .unwrap();

    let diagnostic = assert_diagnostic(&output, 125, "path");
    assert!(
        diagnostic.contains(place(1).to_str().unwrap()),
        "{diagnostic}"
    );
}

// @kotowari[EX-549]
#[test]
fn a_plan_without_a_command_has_no_argv0_and_no_trailing_separator() {
    let (home, workspace) = home_with_workspace();

    let output = run(
        home.path(),
        [
            "--workspace",
            workspace.to_str().unwrap(),
            "--print-plan=json",
        ],
    );

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    for argument in plan["bwrap_arguments"].as_array().unwrap() {
        assert_ne!(argument["value"], "--argv0", "{report}");
        assert_ne!(argument["value"], "--", "{report}");
    }
}

// @kotowari[EX-717]
#[test]
fn network_host_reaches_a_listener_on_the_host_loopback() {
    let (home, workspace) = home_with_workspace();
    profile(
        &home,
        &format!("{RW_WORKSPACE}[network]\nmode = \"host\"\n"),
    );
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0; 5];
        std::io::Read::read_exact(&mut stream, &mut buffer).unwrap();
        std::io::Write::write_all(&mut stream, &buffer).unwrap();
    });

    let output = run_script(
        &home,
        &workspace,
        &format!(
            "/usr/bin/python3 -c \"\
             import socket; s = socket.create_connection(('127.0.0.1', {port}), timeout=5); \
             s.sendall(b'hello'); print(s.recv(5).decode())\""
        ),
    );

    assert_eq!(assert_ran_clean(&output), "hello\n");
    server.join().unwrap();
}
