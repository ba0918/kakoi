//! The command mode "listed": only the programs the policy allows can be started inside
//! the isolation.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Output;

mod common;

use common::{assert_diagnostic, binary, output_report, TempDir};
use kakoi_core::diagnostic::{Diagnostic, Kind};
use kakoi_core::layers::LayerSelection;
use kakoi_core::planning::{plan_for, Request};

/// A home with a workspace `ws` and a `default` profile that makes the workspace `rw`
/// and adds `extra`.
struct Scene {
    home: TempDir,
    workspace: PathBuf,
}

impl Scene {
    fn new(extra: &str) -> Self {
        let home = TempDir::new();
        let workspace = home.path().join("ws");
        std::fs::create_dir(&workspace).unwrap();
        home.write(
            ".config/kakoi/profile/default.toml",
            format!("[mounts]\nrw = [\"${{workspace}}\"]\n{extra}"),
        );
        Self { home, workspace }
    }

    /// A scene whose command mode is "listed", allowing `allow`, with `extra`.
    fn allowing(allow: &[&str], extra: &str) -> Self {
        let allow: Vec<String> = allow.iter().map(|path| format!("\"{path}\"")).collect();
        Self::new(&format!(
            "{extra}\n[commands]\nmode = \"listed\"\nallow = [{}]\n",
            allow.join(", ")
        ))
    }

    /// Runs the built kakoi from the workspace with `arguments`.
    fn kakoi(&self, arguments: &[&str]) -> Output {
        self.kakoi_at(Path::new(env!("CARGO_BIN_EXE_kakoi")), arguments)
    }

    /// Runs the kakoi at `executable` from the workspace with `arguments`.
    fn kakoi_at(&self, executable: &Path, arguments: &[&str]) -> Output {
        let template = binary(self.home.path());
        let mut command = std::process::Command::new(executable);
        command.env_clear();
        for (key, value) in template.get_envs() {
            if let Some(value) = value {
                command.env(key, value);
            }
        }
        command
            .current_dir(&self.workspace)
            .args(arguments)
            .output()
            .unwrap()
    }

    /// Runs `script` with `/bin/sh -c` inside the isolation.
    fn run(&self, script: &str) -> Output {
        self.kakoi(&["--", "/bin/sh", "-c", script])
    }

    /// The plan `plan_for` makes of the scene for `command`, with the Landlock ABI
    /// version `abi` standing for the host's.
    fn plan_with_landlock(
        &self,
        command: &[&str],
        abi: Option<u32>,
    ) -> Result<kakoi_core::plan::Plan, Diagnostic> {
        let mut host = BTreeMap::new();
        host.insert(
            OsString::from("HOME"),
            self.home.path().as_os_str().to_owned(),
        );
        host.insert(
            OsString::from("XDG_CONFIG_HOME"),
            self.home.path().join(".config").into_os_string(),
        );
        host.insert(
            OsString::from("PATH"),
            std::env::var_os("PATH").unwrap_or_default(),
        );
        plan_for(&Request {
            layers: LayerSelection {
                profile: "default".into(),
                policy_file: None,
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

fn assert_success(output: &Output) {
    assert_eq!(output.status.code(), Some(0), "{}", output_report(output));
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

/// Starts `/usr/bin/id` from the shell and says whether it ran.
const START_ID: &str = "/usr/bin/id -u >/dev/null 2>&1 && echo ran || echo failed";

// @kotowari[EX-922]
#[test]
fn ex_922_an_allowed_program_starts_from_an_allowed_shell() {
    let scene = Scene::allowing(&["/bin/sh", "/usr/bin/ls"], "");

    let output = scene.run("/usr/bin/ls -d / && echo listed");

    assert_success(&output);
    assert_eq!(stdout(&output), "/\nlisted\n");
}

// @kotowari[EX-923, REQ-475]
#[test]
fn ex_923_a_program_not_allowed_does_not_start_under_the_host_network() {
    let scene = Scene::allowing(&["/bin/sh"], "");

    let output = scene.run(START_ID);

    assert_success(&output);
    assert_eq!(stdout(&output), "failed\n");
}

// @kotowari[REQ-475]
#[test]
fn a_program_not_allowed_does_not_start_under_the_none_network() {
    let scene = Scene::allowing(&["/bin/sh"], "[network]\nmode = \"none\"\n");

    let output = scene.run(START_ID);

    assert_success(&output);
    assert_eq!(stdout(&output), "failed\n");
}

// @kotowari[REQ-475]
#[test]
fn a_program_not_allowed_does_not_start_under_listed_mounts() {
    let scene = Scene::allowing(&["/bin/sh"], "mode = \"listed\"\n");

    let output = scene.run(START_ID);

    assert_success(&output);
    assert_eq!(stdout(&output), "failed\n");
}

// @kotowari[EX-924]
#[test]
fn ex_924_an_allowed_link_allows_the_program_it_points_at() {
    let links = TempDir::new();
    let link = links.path().join("true-link");
    std::os::unix::fs::symlink("/usr/bin/true", &link).unwrap();
    let scene = Scene::allowing(&["/bin/sh", link.to_str().unwrap()], "");

    let output = scene.run("/usr/bin/true && echo ran");

    assert_success(&output);
    assert_eq!(stdout(&output), "ran\n");
}

// @kotowari[EX-925]
#[test]
fn ex_925_an_allowed_program_the_host_does_not_have_is_skipped_with_a_reason() {
    let scene = Scene::allowing(&["/bin/sh", "/opt/kakoi-no-such-program"], "");

    let plan = scene.kakoi(&["--print-plan=json"]);
    let run = scene.run("echo ran");

    assert_success(&plan);
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    let skipped = plan["skipped_command_allow"].as_array().unwrap();
    assert_eq!(skipped.len(), 1, "{plan}");
    assert_eq!(skipped[0]["written"], "/opt/kakoi-no-such-program");
    assert!(!skipped[0]["reason"].as_str().unwrap().is_empty());
    assert_success(&run);
    assert_eq!(stdout(&run), "ran\n");
}

// @kotowari[EX-926]
#[test]
fn ex_926_a_skipped_allowed_program_widens_nothing() {
    let scene = Scene::allowing(&["/opt/kakoi-no-such-program", "/bin/sh"], "");

    let output = scene.run(START_ID);

    assert_success(&output);
    assert_eq!(stdout(&output), "failed\n");
}

// @kotowari[EX-927]
#[test]
fn ex_927_a_host_without_landlock_does_not_start_a_listed_command_mode() {
    let scene = Scene::allowing(&["/bin/sh"], "");

    let error = scene.plan_with_landlock(&["/bin/true"], None).unwrap_err();

    assert_eq!(error.kind(), Kind::Bwrap, "{error:?}");
    assert_eq!(error.exit_code(), 125);
}

// @kotowari[EX-928]
#[test]
fn ex_928_the_host_command_mode_needs_no_landlock() {
    let scene = Scene::new("");
    let marker = scene.workspace.join("ran");

    let plan = scene
        .plan_with_landlock(&["/usr/bin/touch", marker.to_str().unwrap()], None)
        .unwrap();
    let output = kakoi_core::launch::assemble(&plan)
        .unwrap()
        .command
        .output()
        .unwrap();

    assert_success(&output);
    assert!(marker.exists());
}

// @kotowari[EX-947]
#[test]
fn ex_947_a_command_not_allowed_started_directly_ends_with_126_naming_it() {
    let scene = Scene::allowing(&["/bin/sh"], "");

    let output = scene.kakoi(&["--", "/usr/bin/id"]);

    let diagnostic = assert_diagnostic(&output, 126, "command not executable");
    assert!(diagnostic.contains("/usr/bin/id"), "{diagnostic}");
}

// @kotowari[EX-948]
#[test]
fn ex_948_a_kakoi_outside_the_shown_places_still_starts_the_first_process() {
    let scene = Scene::allowing(&["/bin/sh"], "mode = \"listed\"\n");
    let kakoi = scene.home.path().join("bin/kakoi");
    std::fs::create_dir_all(kakoi.parent().unwrap()).unwrap();
    common::copy_executable(Path::new(env!("CARGO_BIN_EXE_kakoi")), &kakoi);

    let output = scene.kakoi_at(&kakoi, &["--", "/bin/sh", "-c", "echo ran"]);

    assert_success(&output);
    assert_eq!(stdout(&output), "ran\n");
}

// @kotowari[REQ-475, REQ-263]
#[test]
fn the_command_keeps_the_name_it_was_given() {
    let scene = Scene::allowing(&["/bin/sh"], "");

    let output = scene.kakoi(&["--", "sh", "-c", "echo \"$0\""]);

    assert_success(&output);
    assert_eq!(stdout(&output), "sh\n");
}
