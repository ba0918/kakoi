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

#[test]
fn a_command_signal_passes_through_as_128_plus_s() {
    let (home, workspace) = home_with_workspace();

    let output = run_script(&home, &workspace, "kill -TERM $$");

    let report = output_report(&output);
    assert_eq!(output.status.code(), Some(128 + libc::SIGTERM), "{report}");
}

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

#[test]
fn a_command_path_starting_with_a_dash_is_executed_as_a_path() {
    // Specification section 4.2: a COMMAND containing `/` is used as that path. A relative
    // path can start with `-`; it must reach the command, not be read as an option of bwrap.
    // The tool is a copy of `echo` rather than a script: the kernel hands a script's path
    // to its interpreter as an argument, and `sh` would read `-x/tool` as options too.
    let (home, workspace) = home_with_workspace();
    let tool = workspace.join("-x/tool");
    std::fs::create_dir(tool.parent().unwrap()).unwrap();
    std::fs::copy("/bin/echo", &tool).unwrap();

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

#[test]
fn the_command_sees_the_given_name_as_argv0() {
    // Specification sections 1 and 4.2: argv[0] is the `COMMAND` string as given, not the
    // resolved path. `sh` is found on `PATH` and prints `$0`; `-x/tool` is a relative path
    // starting with `-`, a copy of `cat` that shows its own command line (a script would
    // show the path the kernel hands its interpreter, not argv[0]).
    let (home, workspace) = home_with_workspace();
    let tool = workspace.join("-x/tool");
    std::fs::create_dir(tool.parent().unwrap()).unwrap();
    std::fs::copy("/bin/cat", &tool).unwrap();

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

#[test]
fn a_scan_of_more_hidden_files_than_the_soft_limit_still_launches() {
    // Specification section 14: every hidden file needs a descriptor; with 1100 of them
    // and a soft limit of 1024, the start raises the soft limit to the hard limit first.
    let (home, workspace) = home_with_many_env_files(1100);

    let output = run_script_under_soft_limit_1024(&home, &workspace, "test ! -s .env.0");

    assert_eq!(output.status.code(), Some(0), "{}", output_report(&output));
}

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

#[test]
fn a_command_inside_a_hidden_directory_fails_at_exec_with_bwrap_status() {
    let (home, workspace) = home_with_workspace();
    let tool = home.write("hidden/tool", "#!/bin/sh\necho ran\n");
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
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

#[test]
fn an_rw_copy_source_over_the_entry_limit_is_a_path_diagnostic() {
    // The content is held in memory twice over, so a source pointed at something large is
    // refused before the start rather than paged in.
    let (home, workspace) = home_with_workspace();
    for index in 0..=kakoi::copies::ENTRY_LIMIT {
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
        diagnostic.contains(&format!("more than {} entries", kakoi::copies::ENTRY_LIMIT)),
        "{diagnostic}"
    );
}

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
