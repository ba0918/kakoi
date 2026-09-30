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

    /// Runs `script` with `/bin/sh -c` inside the isolation, from the workspace.
    fn run(&self, script: &str) -> Output {
        binary(&self.home)
            .current_dir(&self.workspace)
            .args([
                OsStr::new("--workspace"),
                self.workspace.as_os_str(),
                OsStr::new("--"),
                OsStr::new("/bin/sh"),
                OsStr::new("-c"),
                OsStr::new(script),
            ])
            .output()
            .unwrap()
    }

    /// The JSON plan of the scene, without a command.
    fn json_plan(&self) -> serde_json::Value {
        let output = binary(&self.home)
            .current_dir(&self.workspace)
            .args([
                OsStr::new("--workspace"),
                self.workspace.as_os_str(),
                OsStr::new("--print-plan=json"),
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0), "{}", output_report(&output));
        serde_json::from_slice(&output.stdout).unwrap()
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

// @kotowari[EX-912]
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
