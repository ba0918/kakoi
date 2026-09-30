//! The mount mode "listed": only the base and the places the policy writes are there
//! inside the isolation.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::process::Output;

mod common;

use common::fixture::{home, layers, merged, variables, Facts, CONFIG_DIR, WORKTREE};
use common::{binary, output_report, TempDir};
use kakoi_core::copies::CopySources;
use kakoi_core::diagnostic::Diagnostic;
use kakoi_core::guard_placement::GuardPlan;
use kakoi_core::mounts::{expand_policy, SkippedRole};
use kakoi_core::plan::{
    bwrap_arguments, resolve_isolation, Argument, Inputs, Isolation, IsolationFacts, Provisions,
};
use kakoi_core::policy::NetworkMode;

/// A scene on the real host: a directory under the build's own temporary directory, so
/// that nothing of it is under "/tmp", which "listed" replaces; a home and a workspace
/// side by side in it, the workspace a worktree of its own rather than a part of the
/// repository the build is in; and a `default` profile in the home.
struct Scene {
    /// Removed with the scene.
    _root: TempDir,
    home: PathBuf,
    workspace: PathBuf,
}

impl Scene {
    /// A scene whose profile is the "listed" mount mode, the workspace and `rw` written
    /// `rw`, and `extra`.
    fn new(rw: &[&str], extra: &str) -> Self {
        let root = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
        let home = root.path().join("home");
        let workspace = root.path().join("ws");
        fs::create_dir(&home).unwrap();
        fs::create_dir_all(workspace.join(".git")).unwrap();
        let rw: String = rw.iter().map(|path| format!(", \"{path}\"")).collect();
        let profile = home.join(".config/kakoi/profile/default.toml");
        fs::create_dir_all(profile.parent().unwrap()).unwrap();
        fs::write(
            &profile,
            format!("[mounts]\nmode = \"listed\"\nrw = [\"${{workspace}}\"{rw}]\n{extra}"),
        )
        .unwrap();
        Self {
            _root: root,
            home,
            workspace,
        }
    }

    /// Writes `body` at `relative` under the home.
    fn write(&self, relative: &str, body: &str) -> PathBuf {
        let path = self.home.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, body).unwrap();
        path
    }

    /// Runs the built kakoi with `--workspace` and `arguments` from `current_dir`, with
    /// the test's `PATH` or `path`.
    fn kakoi(&self, current_dir: &Path, path: Option<&str>, arguments: &[&OsStr]) -> Output {
        let mut command = binary(&self.home);
        if let Some(path) = path {
            command.env("PATH", path);
        }
        command
            .current_dir(current_dir)
            .arg("--workspace")
            .arg(&self.workspace)
            .args(arguments)
            .output()
            .unwrap()
    }

    /// Runs `script` with `/bin/sh -c` inside the isolation, from the workspace.
    fn run(&self, script: &str) -> Output {
        self.kakoi(
            &self.workspace,
            None,
            &[
                OsStr::new("--"),
                OsStr::new("/bin/sh"),
                OsStr::new("-c"),
                OsStr::new(script),
            ],
        )
    }

    /// The JSON plan of the scene, without a command, with the test's `PATH` or `path`.
    fn json_plan_on(&self, path: Option<&str>) -> serde_json::Value {
        let output = self.kakoi(&self.workspace, path, &[OsStr::new("--print-plan=json")]);
        assert_eq!(output.status.code(), Some(0), "{}", output_report(&output));
        serde_json::from_slice(&output.stdout).unwrap()
    }

    /// The JSON plan of the scene, without a command.
    fn json_plan(&self) -> serde_json::Value {
        self.json_plan_on(None)
    }

    /// A directory beside the home and the workspace holding an executable script `name`
    /// that prints `name` and where it is; not shown unless the policy writes it.
    fn shim(&self, name: &str) -> PathBuf {
        let shim = self.home.parent().unwrap().join("shim");
        fs::create_dir_all(&shim).unwrap();
        common::write_executable(&shim.join(name), format!("#!/bin/sh\necho shim {name}\n"));
        shim
    }
}

fn assert_success(output: &Output) {
    assert_eq!(output.status.code(), Some(0), "{}", output_report(output));
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

/// The sockets directly in "/run" and one level below, as the host has them.
fn host_sockets_under_run() -> Vec<PathBuf> {
    let mut sockets = Vec::new();
    let mut directories = vec![PathBuf::from("/run")];
    for depth in 0..2 {
        let mut next = Vec::new();
        for directory in directories {
            let Ok(entries) = fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_socket() {
                    sockets.push(entry.path());
                } else if kind.is_dir() && depth == 0 {
                    next.push(entry.path());
                }
            }
        }
        directories = next;
    }
    sockets
}

// @kotowari[EX-905]
#[test]
fn ex_905_a_socket_under_run_that_is_not_written_does_not_exist() {
    let scene = Scene::new(&[], "");
    let mut paths = host_sockets_under_run();
    paths.push(PathBuf::from("/run"));
    let script = paths
        .iter()
        .map(|path| format!("! test -e '{0}' && ! test -L '{0}'", path.display()))
        .collect::<Vec<_>>()
        .join(" && ");

    let output = scene.run(&format!("{script} && echo absent"));

    assert_success(&output);
    assert_eq!(stdout(&output), "absent\n");
}

// @kotowari[EX-906]
#[test]
fn ex_906_the_base_and_a_written_directory_are_there_and_the_rest_of_the_home_is_not() {
    let scene = Scene::new(&[], "ro = [\"~/data\"]\n");
    let file = scene.write("data/file", "written\n");
    let other = scene.write("other/file", "other\n");

    let output = scene.run(&format!(
        "/usr/bin/cat '{}' && ! test -e '{}' && ! test -e '{}'",
        file.display(),
        other.display(),
        other.parent().unwrap().display()
    ));

    assert_success(&output);
    assert_eq!(stdout(&output), "written\n");
}

// @kotowari[EX-907]
#[test]
fn ex_907_a_hide_inside_a_shown_directory_hides() {
    let scene = Scene::new(&[], "ro = [\"~/data\"]\nhide = [\"~/data/secret\"]\n");
    let secret = scene.write("data/secret", "token\n");

    let output = scene.run(&format!("/usr/bin/cat '{}'; echo end", secret.display()));

    assert_success(&output);
    assert_eq!(stdout(&output), "end\n");
}

// @kotowari[EX-908]
#[test]
fn ex_908_without_the_system_the_base_is_not_shown() {
    let scene = Scene::new(&[], "system = false\n");

    let plan = scene.json_plan();

    let arguments = plan["bwrap_arguments"].as_array().unwrap();
    for argument in arguments {
        let value = argument["value"].as_str().unwrap_or_default();
        assert!(
            value != "/usr" && !value.starts_with("/usr/"),
            "{value}: {plan}"
        );
    }
}

// @kotowari[EX-910, REQ-469]
#[test]
fn ex_910_a_made_parent_holds_only_what_is_shown_and_cannot_be_written() {
    let scene = Scene::new(&[], "ro = [\"~/data\"]\n");
    scene.write("data/file", "");
    scene.write("other/file", "");
    let home = scene.home.display();

    let output = scene.run(&format!(
        "/usr/bin/ls -A '{home}'; /usr/bin/touch '{home}/made' 2>/dev/null && echo home-written; \
         /usr/bin/touch /made 2>/dev/null && echo root-written; echo end"
    ));

    assert_success(&output);
    assert_eq!(stdout(&output), "data\nend\n");
    assert!(!scene.home.join("made").exists());
}

// @kotowari[EX-911]
#[test]
fn ex_911_a_written_rw_place_can_be_written_and_the_host_keeps_it() {
    let scene = Scene::new(&[], "");

    let output = scene.run(&format!("echo made > '{}/made'", scene.workspace.display()));

    assert_success(&output);
    assert_eq!(
        fs::read_to_string(scene.workspace.join("made")).unwrap(),
        "made\n"
    );
}

// @kotowari[EX-912, REQ-470]
#[test]
fn ex_912_the_isolation_has_an_empty_tmp_of_its_own() {
    let scene = Scene::new(&[], "");
    let host_file = TempDir::new();
    let made = format!("kakoi-listed-made-{}", std::process::id());

    let output = scene.run(&format!(
        "/usr/bin/ls -A /tmp; echo x > '/tmp/{made}' && echo written"
    ));

    assert_success(&output);
    assert_eq!(stdout(&output), "written\n");
    assert!(host_file.path().exists());
    assert!(!Path::new("/tmp").join(&made).exists());
}

// @kotowari[EX-913]
#[test]
fn ex_913_a_place_under_tmp_written_rw_is_shared_with_the_host() {
    let shared = TempDir::new();
    let shared_path = shared.path().to_str().unwrap();
    let scene = Scene::new(&[shared_path], "");

    let output = scene.run(&format!("echo x > '{}/made'", shared.path().display()));

    assert_success(&output);
    assert_eq!(
        fs::read_to_string(shared.path().join("made")).unwrap(),
        "x\n"
    );
}

// @kotowari[EX-949, REQ-468]
#[test]
fn ex_949_a_link_on_a_written_path_is_there_pointing_where_it_did_and_cannot_be_changed() {
    let scene = Scene::new(&[], "ro = [\"~/a\"]\n");
    scene.write("b/file", "through the link\n");
    std::os::unix::fs::symlink("b", scene.home.join("a")).unwrap();
    let link = scene.home.join("a");

    let output = scene.run(&format!(
        "/usr/bin/cat '{0}/file' && /usr/bin/readlink '{0}' && \
         {{ /usr/bin/ln -sfn elsewhere '{0}' 2>/dev/null; /usr/bin/readlink '{0}'; }}",
        link.display()
    ));

    assert_success(&output);
    assert_eq!(stdout(&output), "through the link\nb\nb\n");
}

/// The stage-seven resolution on the fixture host for a profile and a network mode.
fn isolation(profile: &str, network_mode: &str, facts: Facts) -> Result<Isolation, Diagnostic> {
    let layers = layers(
        &format!("[network]\nmode = \"{network_mode}\"\n{profile}"),
        Some(""),
        &[],
        &[],
    );
    let policy = merged(&layers);
    let variables = variables();
    let home = home();
    let expanded = expand_policy(&policy, &variables, &home);
    let inputs = Inputs {
        layers: &layers,
        policy: &policy,
        expanded: &expanded,
        variables: &variables,
        home: &home,
        config_dir: Path::new(CONFIG_DIR),
        workspace: None,
        current_dir: Path::new(WORKTREE),
        host: &BTreeMap::new(),
        nested: false,
        applied: true,
        outer_guard: false,
        shared_files: None,
    };
    resolve_isolation(
        &inputs,
        &IsolationFacts {
            mounts: facts.mount_facts(),
            secrets: BTreeMap::new(),
        },
    )
}

/// The bwrap arguments of an isolation, without a command.
fn arguments(isolation: &Isolation, network_mode: NetworkMode) -> Vec<OsString> {
    bwrap_arguments(
        network_mode,
        Path::new(WORKTREE),
        &isolation.mounts.items,
        &CopySources::default(),
        &GuardPlan::default(),
        &Provisions {
            listed: isolation.listed.clone(),
            ..Provisions::default()
        },
        None,
    )
    .into_iter()
    .map(|argument| match argument {
        Argument::Literal(text) => text,
        other => OsString::from(format!("{other:?}")),
    })
    .collect()
}

/// What each bind in `arguments` binds from.
fn bound_sources(arguments: &[OsString]) -> Vec<&OsStr> {
    arguments
        .windows(2)
        .filter(|pair| pair[0] == "--bind" || pair[0] == "--ro-bind")
        .map(|pair| pair[1].as_os_str())
        .collect()
}

/// The fixture host's base: every directory of it there, the workspace too.
fn base_facts() -> Facts {
    Facts::new()
        .dir("/usr")
        .dir("/usr/bin")
        .dir("/usr/lib")
        .dir("/usr/lib64")
        .dir("/usr/sbin")
        .link_to_dir("/bin", "/usr/bin")
        .link_to_dir("/lib", "/usr/lib")
        .link_to_dir("/lib64", "/usr/lib64")
        .link_to_dir("/sbin", "/usr/sbin")
        .dir("/etc")
        .dir_with_ancestors(WORKTREE)
}

// @kotowari[EX-909]
#[test]
fn ex_909_a_base_directory_the_host_does_not_have_is_skipped_with_a_reason() {
    let facts = Facts {
        paths: base_facts()
            .paths
            .into_iter()
            .filter(|(path, _)| path != Path::new("/lib64"))
            .collect(),
        ..base_facts()
    };

    let isolation = isolation(
        "[mounts]\nmode = \"listed\"\nrw = [\"${workspace}\"]\n",
        "host",
        facts,
    )
    .unwrap();

    let skipped: Vec<_> = isolation
        .skipped_paths
        .iter()
        .filter(|skipped| skipped.role == SkippedRole::Base)
        .collect();
    assert_eq!(skipped.len(), 1, "{skipped:?}");
    assert_eq!(skipped[0].written, "/lib64");
    assert!(!skipped[0].reason.is_empty());
    assert!(!arguments(&isolation, NetworkMode::Host).contains(&OsString::from("/lib64")));
}

/// The fixture host's base with "/etc/resolv.conf" a link to "/mnt/wsl/resolv.conf" and
/// another file beside that.
fn resolver_outside_the_base() -> Facts {
    base_facts()
        .link_to_file("/etc/resolv.conf", "/mnt/wsl/resolv.conf")
        .links_traversed("/etc/resolv.conf", &["/etc/resolv.conf"])
        .link_target("/etc/resolv.conf", "/mnt/wsl/resolv.conf")
        .file_with_ancestors("/mnt/wsl/other")
}

// @kotowari[EX-914, REQ-471]
#[test]
fn ex_914_the_file_a_resolver_link_outside_the_base_points_at_is_shown_read_only() {
    let listed = "[mounts]\nmode = \"listed\"\nrw = [\"${workspace}\"]\n";
    let isolation = isolation(listed, "host", resolver_outside_the_base()).unwrap();

    let arguments = arguments(&isolation, NetworkMode::Host);

    let shown = ["--ro-bind", "/mnt/wsl/resolv.conf", "/mnt/wsl/resolv.conf"].map(OsString::from);
    assert!(
        arguments.windows(3).any(|window| window == shown),
        "{arguments:?}"
    );
}

// @kotowari[REQ-471]
#[test]
fn the_resolver_target_is_not_shown_under_filtered_or_without_the_system() {
    let filtered = isolation(
        "[mounts]\nmode = \"listed\"\nrw = [\"${workspace}\"]\n",
        "filtered",
        resolver_outside_the_base(),
    )
    .unwrap();
    let no_system = isolation(
        "[mounts]\nmode = \"listed\"\nsystem = false\nrw = [\"${workspace}\"]\n",
        "host",
        resolver_outside_the_base(),
    )
    .unwrap();

    for (isolation, mode) in [
        (filtered, NetworkMode::Filtered),
        (no_system, NetworkMode::Host),
    ] {
        let arguments = arguments(&isolation, mode);
        assert!(
            !bound_sources(&arguments).contains(&OsStr::new("/mnt/wsl/resolv.conf")),
            "{arguments:?}"
        );
    }
}

// @kotowari[EX-915]
#[test]
fn ex_915_nothing_else_beside_the_resolver_target_is_shown() {
    let isolation = isolation(
        "[mounts]\nmode = \"listed\"\nrw = [\"${workspace}\"]\n",
        "host",
        resolver_outside_the_base(),
    )
    .unwrap();

    let arguments = arguments(&isolation, NetworkMode::Host);

    let sources = bound_sources(&arguments);
    for hidden in ["/mnt", "/mnt/wsl", "/mnt/wsl/other"] {
        assert!(!sources.contains(&OsStr::new(hidden)), "{arguments:?}");
    }
    assert!(!arguments.contains(&OsString::from("/mnt/wsl/other")));
}

/// The kinds of the items the plan says were not shown, as `origin` names them.
fn not_shown(plan: &serde_json::Value) -> Vec<(String, String)> {
    plan["not_shown"]
        .as_array()
        .unwrap_or_else(|| panic!("no not_shown: {plan}"))
        .iter()
        .map(|entry| {
            assert!(!entry["reason"].as_str().unwrap().is_empty(), "{entry}");
            (
                entry["origin"]["kind"].as_str().unwrap().to_string(),
                entry["path"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

// @kotowari[EX-942]
#[test]
fn ex_942_the_secret_place_of_an_unshown_configuration_directory_is_not_there() {
    let scene = Scene::new(&[], "");
    let secrets = scene.write(".config/kakoi/secrets/token", "secret\n");
    let config_dir = secrets.parent().unwrap().parent().unwrap();

    let output = scene.run(&format!(
        "! test -e '{}' && echo absent",
        config_dir.display()
    ));
    let plan = scene.json_plan();

    assert_success(&output);
    assert_eq!(stdout(&output), "absent\n");
    assert!(
        not_shown(&plan).contains(&(
            "config-secrets".to_string(),
            secrets.parent().unwrap().display().to_string()
        )),
        "{plan}"
    );
}

// @kotowari[REQ-468]
#[test]
fn a_written_hide_outside_what_is_shown_is_skipped_with_a_reason() {
    let scene = Scene::new(&[], "hide = [\"~/.ssh\"]\n");
    let ssh = scene.write(".ssh/id", "key\n");

    let output = scene.run(&format!(
        "! test -e '{}' && echo absent",
        ssh.parent().unwrap().display()
    ));
    let plan = scene.json_plan();

    assert_success(&output);
    assert_eq!(stdout(&output), "absent\n");
    assert!(
        not_shown(&plan).contains(&(
            "profile".to_string(),
            ssh.parent().unwrap().display().to_string()
        )),
        "{plan}"
    );
}

// @kotowari[EX-943]
#[test]
fn ex_943_a_guard_wraps_the_real_program_that_is_shown() {
    let scene = Scene::new(
        &[],
        "[[commands.guard]]\nprogram = \"git\"\nreason = \"no push\"\ndeny = [[\"push\"]]\n",
    );
    let shim = scene.shim("git");
    let path = format!("{}:/usr/bin:/bin", shim.display());

    let plan = scene.json_plan_on(Some(&path));
    let output = scene.kakoi(
        &scene.workspace,
        Some(&path),
        &[OsStr::new("--"), OsStr::new("git"), OsStr::new("--version")],
    );

    let guards = plan["guards"].as_array().unwrap();
    assert_eq!(guards.len(), 1, "{plan}");
    assert_eq!(guards[0]["found"], "/usr/bin/git", "{plan}");
    assert_success(&output);
    assert!(
        stdout(&output).starts_with("git version"),
        "{}",
        output_report(&output)
    );
}

// @kotowari[EX-944, REQ-483]
#[test]
fn ex_944_a_current_directory_that_is_not_shown_is_a_path_diagnostic() {
    let scene = Scene::new(&[], "");
    let marker = scene.home.join("ran");

    let run = scene.kakoi(
        &scene.home,
        None,
        &[
            OsStr::new("--"),
            OsStr::new("/usr/bin/touch"),
            marker.as_os_str(),
        ],
    );
    let print = scene.kakoi(&scene.home, None, &[OsStr::new("--print-plan")]);

    common::assert_diagnostic(&run, 125, "path");
    common::assert_diagnostic(&print, 125, "path");
    assert!(!marker.exists());
}

// @kotowari[REQ-483]
#[test]
fn a_current_directory_in_the_hosts_tmp_is_not_shown_by_the_isolations_own_tmp() {
    let scene = Scene::new(&[], "");
    let host_tmp = TempDir::under(Path::new("/tmp"));
    let marker = host_tmp.path().join("ran");

    let run = scene.kakoi(
        host_tmp.path(),
        None,
        &[
            OsStr::new("--"),
            OsStr::new("/usr/bin/touch"),
            marker.as_os_str(),
        ],
    );

    common::assert_diagnostic(&run, 125, "path");
    assert!(!marker.exists());
}

// @kotowari[EX-945]
#[test]
fn ex_945_a_shown_workspace_as_the_current_directory_runs_the_command() {
    let scene = Scene::new(&[], "");

    let output = scene.run("echo ran");

    assert_success(&output);
    assert_eq!(stdout(&output), "ran\n");
}

// @kotowari[EX-950, REQ-484]
#[test]
fn ex_950_an_unshown_program_first_on_path_is_passed_over_for_a_shown_one() {
    let scene = Scene::new(&[], "");
    let shim = scene.shim("id");
    let path = format!("{}:/usr/bin:/bin", shim.display());

    let output = scene.kakoi(
        &scene.workspace,
        Some(&path),
        &[OsStr::new("--"), OsStr::new("id"), OsStr::new("-u")],
    );

    assert_success(&output);
    assert_eq!(stdout(&output), format!("{}\n", unsafe { libc::getuid() }));
}

// @kotowari[EX-951]
#[test]
fn ex_951_a_program_only_in_an_unshown_place_is_not_found() {
    let scene = Scene::new(&[], "");
    let shim = scene.shim("kakoi-only-in-the-shim");
    let path = format!("{}:/usr/bin:/bin", shim.display());

    let output = scene.kakoi(
        &scene.workspace,
        Some(&path),
        &[OsStr::new("--"), OsStr::new("kakoi-only-in-the-shim")],
    );

    common::assert_diagnostic(&output, 127, "command not found");
}

// @kotowari[REQ-468]
#[test]
fn a_scan_root_or_a_hide_mounts_under_outside_what_is_shown_is_skipped_with_a_reason() {
    let profile = "[mounts]\nmode = \"listed\"\nrw = [\"${workspace}\"]\n\
                   [[mounts.scan]]\nroot = \"/home/u/elsewhere\"\nnames = [\".env\"]\n\
                   [[mounts.hide-mounts]]\nunder = \"/mnt\"\nfstype = [\"9p\"]\n";
    let facts = base_facts()
        .dir("/home/u/elsewhere")
        .dir("/mnt")
        .mount("/mnt/c", "9p");

    let isolation = isolation(profile, "host", facts).unwrap();

    let skipped: Vec<_> = isolation
        .skipped_paths
        .iter()
        .map(|skipped| (skipped.role, skipped.written.as_str()))
        .collect();
    assert!(
        skipped.contains(&(SkippedRole::ScanRoot, "/home/u/elsewhere")),
        "{skipped:?}"
    );
    assert!(
        skipped.contains(&(SkippedRole::HideMountsUnder, "/mnt")),
        "{skipped:?}"
    );
    let arguments = arguments(&isolation, NetworkMode::Host);
    assert!(
        !arguments.contains(&OsString::from("/mnt/c")),
        "{arguments:?}"
    );
}

// @kotowari[REQ-468]
#[test]
fn a_hide_mounts_under_outside_what_is_shown_needs_no_mount_list() {
    let profile = "[mounts]\nmode = \"listed\"\nrw = [\"${workspace}\"]\n\
                   [[mounts.hide-mounts]]\nunder = \"/mnt\"\nfstype = [\"9p\"]\n";
    let facts = base_facts().dir("/mnt").mount_list_unreadable();

    let isolation = isolation(profile, "host", facts).unwrap();

    let skipped: Vec<_> = isolation
        .skipped_paths
        .iter()
        .map(|skipped| (skipped.role, skipped.written.as_str()))
        .collect();
    assert!(
        skipped.contains(&(SkippedRole::HideMountsUnder, "/mnt")),
        "{skipped:?}"
    );
}

/// A name for an abstract UNIX socket no other test uses.
fn abstract_name() -> String {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    format!(
        "kakoi-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

/// A Python script in the workspace that connects to the abstract UNIX socket named by
/// its argument and prints how it went.
fn connect_script(scene: &Scene) -> PathBuf {
    let script = scene.workspace.join("connect.py");
    fs::write(
        &script,
        "import socket, sys\n\
         s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)\n\
         try:\n    s.connect('\\0' + sys.argv[1])\n    print('connected')\n\
         except PermissionError:\n    print('refused')\n",
    )
    .unwrap();
    script
}

// @kotowari[EX-916]
#[test]
fn ex_916_an_abstract_socket_made_outside_cannot_be_reached_under_listed_and_host() {
    use std::os::linux::net::SocketAddrExt;
    let name = abstract_name();
    let address = std::os::unix::net::SocketAddr::from_abstract_name(name.as_bytes()).unwrap();
    let _listener = std::os::unix::net::UnixListener::bind_addr(&address).unwrap();
    let scene = Scene::new(&[], "");
    let connect = connect_script(&scene);

    let output = scene.run(&format!(
        "/usr/bin/python3 '{}' '{name}'",
        connect.display()
    ));

    assert_success(&output);
    assert_eq!(stdout(&output), "refused\n");
}

// @kotowari[REQ-472]
#[test]
fn an_abstract_socket_made_inside_can_still_be_reached_from_inside() {
    let scene = Scene::new(&[], "");
    let name = abstract_name();
    let connect = connect_script(&scene);
    let serve = scene.workspace.join("serve.py");
    fs::write(
        &serve,
        "import socket, subprocess, sys\n\
         server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)\n\
         server.bind('\\0' + sys.argv[2])\n\
         server.listen(1)\n\
         subprocess.run(['/usr/bin/python3', sys.argv[1], sys.argv[2]], check=True)\n",
    )
    .unwrap();

    let output = scene.run(&format!(
        "/usr/bin/python3 '{}' '{}' '{name}'",
        serve.display(),
        connect.display()
    ));

    assert_success(&output);
    assert_eq!(stdout(&output), "connected\n");
}

impl Scene {
    /// The plan `plan_for` makes of the scene with the network mode written in the
    /// policy file `policy`, the command `command`, and the Landlock ABI version `abi`
    /// standing for the host's.
    fn plan_with_landlock(
        &self,
        policy: &str,
        command: &[&str],
        abi: Option<u32>,
    ) -> Result<kakoi_core::plan::Plan, Diagnostic> {
        let policy_file = self.home.join("policy.toml");
        fs::write(&policy_file, policy).unwrap();
        let mut host = BTreeMap::new();
        host.insert(OsString::from("HOME"), self.home.clone().into_os_string());
        host.insert(
            OsString::from("XDG_CONFIG_HOME"),
            self.home.join(".config").into_os_string(),
        );
        host.insert(
            OsString::from("PATH"),
            std::env::var_os("PATH").unwrap_or_default(),
        );
        kakoi_core::planning::plan_for(&kakoi_core::planning::Request {
            layers: kakoi_core::layers::LayerSelection {
                profile: "default".into(),
                policy_file: Some(policy_file),
                rw: vec![],
                hide: vec![],
            },
            workspace: Some(self.workspace.clone()),
            command: command.iter().map(OsString::from).collect(),
            current_dir: self.workspace.clone(),
            host,
            executable: Some(PathBuf::from(env!("CARGO_BIN_EXE_kakoi"))),
            nested: false,
            applied: true,
            outer_guard: false,
            landlock_abi: abi,
        })
    }
}

// @kotowari[EX-917]
#[test]
fn ex_917_a_host_without_the_abstract_socket_scope_does_not_start_listed_and_host() {
    let scene = Scene::new(&[], "");

    for abi in [None, Some(5)] {
        let error = scene
            .plan_with_landlock("[network]\nmode = \"host\"\n", &["/bin/true"], abi)
            .unwrap_err();
        assert_eq!(
            error.kind(),
            kakoi_core::diagnostic::Kind::Bwrap,
            "{error:?}"
        );
        assert_eq!(error.exit_code(), 125);
    }
}

// @kotowari[REQ-472]
#[test]
fn listed_with_the_none_network_needs_no_abstract_socket_scope() {
    let scene = Scene::new(&[], "");
    let marker = scene.workspace.join("ran");

    let plan = scene
        .plan_with_landlock(
            "[network]\nmode = \"none\"\n",
            &["/usr/bin/touch", marker.to_str().unwrap()],
            None,
        )
        .unwrap();
    let output = kakoi_core::launch::assemble(&plan)
        .unwrap()
        .command
        .output()
        .unwrap();

    assert_success(&output);
    assert!(marker.exists());
}
