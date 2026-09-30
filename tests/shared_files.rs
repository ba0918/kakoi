//! The shared file place `$XDG_RUNTIME_DIR/kakoi/`: the real files a run binds read-only
//! for the empty file of a `hide` and for the resolver configuration of filtered, and the
//! runs that make the same files from data instead.

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Output;

mod common;

use common::{
    assert_diagnostic, binary, home_with_workspace, output_report, run_inside, TempDir, KAKOI,
    RW_WORKSPACE,
};

/// A home with a workspace, a file `secret.txt` beside it that the profile hides, and a
/// runtime directory `run` of its own for `XDG_RUNTIME_DIR`.
struct Scene {
    home: TempDir,
    workspace: PathBuf,
    hidden: PathBuf,
    runtime: PathBuf,
}

impl Scene {
    fn new() -> Self {
        let (home, workspace) = home_with_workspace();
        let hidden = home.write("secret.txt", "SECRET-CONTENT\n");
        let runtime = home.path().join("run");
        std::fs::create_dir(&runtime).unwrap();
        home.write(
            ".config/kakoi/profile/default.toml",
            format!("{RW_WORKSPACE}hide = [\"{}\"]\n", hidden.display()),
        );
        Self {
            home,
            workspace,
            hidden,
            runtime,
        }
    }

    /// The shared file place under the runtime directory.
    fn place(&self) -> PathBuf {
        self.runtime.join("kakoi")
    }

    /// The built binary with `XDG_RUNTIME_DIR` at `runtime`, or without it for `None`,
    /// from the workspace.
    fn kakoi(&self, runtime: Option<&std::ffi::OsStr>) -> std::process::Command {
        let mut command = binary(self.home.path());
        if let Some(runtime) = runtime {
            command.env("XDG_RUNTIME_DIR", runtime);
        }
        command
            .current_dir(&self.workspace)
            .args(["--workspace".as_ref(), self.workspace.as_os_str()]);
        command
    }

    /// Runs `script` inside the isolation with `XDG_RUNTIME_DIR` as given.
    fn run(&self, runtime: Option<&std::ffi::OsStr>, script: &str) -> Output {
        self.kakoi(runtime)
            .args(["--", "/bin/sh", "-c", script])
            .output()
            .unwrap()
    }

    /// Runs `script` inside the isolation with `XDG_RUNTIME_DIR` at `runtime`.
    fn run_with_place(&self, script: &str) -> Output {
        self.run(Some(self.runtime.as_os_str()), script)
    }

    /// The JSON plan with `XDG_RUNTIME_DIR` as given.
    fn json_plan(&self, runtime: Option<&std::ffi::OsStr>) -> serde_json::Value {
        let output = self
            .kakoi(runtime)
            .args(["--print-plan=json", "--", "/bin/true"])
            .output()
            .unwrap();
        let report = output_report(&output);
        assert_eq!(output.status.code(), Some(0), "{report}");
        serde_json::from_slice(&output.stdout).unwrap()
    }

    /// What the hidden file looks like from inside: its mode, its type, and its size.
    fn look_at_hidden(&self) -> String {
        format!("stat -c '%a %F %s' {}", self.hidden.display())
    }
}

/// Exit 0 and nothing at all on standard error; returns standard output as text.
fn assert_quiet(output: &Output) -> String {
    let report = output_report(output);
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert!(output.stderr.is_empty(), "{report}");
    String::from_utf8(output.stdout.clone()).unwrap()
}

const EMPTY_REGULAR_0600: &str = "600 regular empty file 0\n";

fn mode(path: &Path) -> u32 {
    std::fs::symlink_metadata(path)
        .unwrap()
        .permissions()
        .mode()
        & 0o7777
}

/// The three arguments of `plan` that put something over `path`.
fn mount_over<'a>(plan: &'a serde_json::Value, path: &Path) -> &'a [serde_json::Value] {
    let arguments = plan["bwrap_arguments"].as_array().unwrap();
    let at = arguments
        .iter()
        .rposition(|argument| argument["value"] == path.to_str().unwrap())
        .unwrap_or_else(|| panic!("nothing over {}: {plan}", path.display()));
    &arguments[at - 2..=at]
}

// @kotowari[EX-893]
#[test]
fn ex_893_without_xdg_runtime_dir_a_hidden_file_is_still_an_empty_0600_file() {
    let scene = Scene::new();

    let output = scene.run(None, &scene.look_at_hidden());

    assert_eq!(assert_quiet(&output), EMPTY_REGULAR_0600);
}

// @kotowari[EX-898]
#[test]
fn ex_898_a_hidden_file_bound_from_the_place_cannot_be_written() {
    let scene = Scene::new();

    let output = scene.run_with_place(&format!(
        "{}; (echo x > {}) 2>/dev/null || echo not-writable",
        scene.look_at_hidden(),
        scene.hidden.display()
    ));

    assert_eq!(
        assert_quiet(&output),
        format!("{EMPTY_REGULAR_0600}not-writable\n")
    );
    let place = scene.place();
    let files: Vec<_> = std::fs::read_dir(&place)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert!(!files.is_empty(), "the place was not used");
    for file in files {
        assert_eq!(std::fs::read(&file).unwrap(), b"", "{}", file.display());
    }
}

// @kotowari[REQ-461]
#[test]
fn req_461_the_place_is_made_0700_and_its_file_0600() {
    let scene = Scene::new();

    let output = scene.run_with_place(&scene.look_at_hidden());

    assert_eq!(assert_quiet(&output), EMPTY_REGULAR_0600);
    let place = scene.place();
    assert_eq!(mode(&place), 0o700);
    let files: Vec<_> = std::fs::read_dir(&place)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(files.len(), 1, "{files:?}");
    let metadata = std::fs::symlink_metadata(&files[0]).unwrap();
    assert!(metadata.file_type().is_file());
    assert_eq!(metadata.permissions().mode() & 0o7777, 0o600);
    assert_eq!(metadata.len(), 0);
    // SAFETY: `getuid` has no preconditions.
    assert_eq!(metadata.uid(), unsafe { libc::getuid() });
}

/// The file in the place a run with the hidden file of `scene` binds, after one run made
/// it.
fn made_file(scene: &Scene) -> PathBuf {
    assert_quiet(&scene.run_with_place("true"));
    let files: Vec<_> = std::fs::read_dir(scene.place())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(files.len(), 1, "{files:?}");
    files.into_iter().next().unwrap()
}

// @kotowari[EX-894]
#[test]
fn ex_894_a_file_of_the_place_written_to_is_made_again() {
    let scene = Scene::new();
    let file = made_file(&scene);
    std::fs::write(&file, "written by someone\n").unwrap();

    let output = scene.run_with_place(&format!("cat {}", scene.hidden.display()));

    assert_eq!(assert_quiet(&output), "");
    assert_eq!(std::fs::read(&file).unwrap(), b"");
}

// @kotowari[REQ-461, REQ-306]
#[test]
fn req_461_a_file_of_the_place_with_another_mode_is_made_again() {
    let scene = Scene::new();
    let file = made_file(&scene);
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();

    let output = scene.run_with_place(&scene.look_at_hidden());

    assert_eq!(assert_quiet(&output), EMPTY_REGULAR_0600);
    assert_eq!(mode(&file), 0o600);
    // No file of another name is left behind.
    assert_eq!(std::fs::read_dir(scene.place()).unwrap().count(), 1);
}

// @kotowari[REQ-461]
#[test]
fn req_461_a_file_of_the_place_that_is_a_link_is_made_again() {
    let scene = Scene::new();
    let file = made_file(&scene);
    let elsewhere = scene.home.write("elsewhere", "");
    std::fs::remove_file(&file).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &file).unwrap();

    let output = scene.run_with_place(&scene.look_at_hidden());

    assert_eq!(assert_quiet(&output), EMPTY_REGULAR_0600);
    assert!(std::fs::symlink_metadata(&file).unwrap().is_file());
}

// @kotowari[REQ-461, REQ-460]
#[test]
fn req_461_a_place_that_is_a_link_or_has_another_mode_is_not_used() {
    // A place that is a link to a directory of the user's.
    let scene = Scene::new();
    let target = scene.home.path().join("target");
    std::fs::create_dir(&target).unwrap();
    std::os::unix::fs::symlink(&target, scene.place()).unwrap();

    let output = scene.run_with_place(&scene.look_at_hidden());

    assert_eq!(assert_quiet(&output), EMPTY_REGULAR_0600);
    assert_eq!(std::fs::read_dir(&target).unwrap().count(), 0);

    // A place that others may read.
    let scene = Scene::new();
    std::fs::create_dir(scene.place()).unwrap();
    std::fs::set_permissions(scene.place(), std::fs::Permissions::from_mode(0o755)).unwrap();

    let output = scene.run_with_place(&scene.look_at_hidden());

    assert_eq!(assert_quiet(&output), EMPTY_REGULAR_0600);
    assert_eq!(std::fs::read_dir(scene.place()).unwrap().count(), 0);
    assert_eq!(mode(&scene.place()), 0o755);
}

// @kotowari[REQ-461, REQ-460]
#[test]
fn req_461_an_empty_or_relative_xdg_runtime_dir_is_not_used() {
    let scene = Scene::new();

    for runtime in ["", "run"] {
        let output = scene.run(Some(runtime.as_ref()), &scene.look_at_hidden());

        assert_eq!(assert_quiet(&output), EMPTY_REGULAR_0600, "{runtime:?}");
        assert!(!scene.place().exists(), "{runtime:?}");
        assert!(!scene.workspace.join("run").exists(), "{runtime:?}");
    }
}

// @kotowari[REQ-461]
#[test]
fn req_461_a_command_not_found_makes_no_place() {
    let scene = Scene::new();

    let output = scene
        .kakoi(Some(scene.runtime.as_os_str()))
        .args(["--", "no-such-tool-anywhere"])
        .output()
        .unwrap();

    assert_diagnostic(&output, 127, "command not found");
    assert!(!scene.place().exists());
}

// @kotowari[REQ-461, REQ-460, REQ-308, REQ-314]
#[test]
fn req_461_the_plan_shows_the_place_it_would_use_and_writes_nothing() {
    let scene = Scene::new();

    let with_place = scene.json_plan(Some(scene.runtime.as_os_str()));
    let without = scene.json_plan(None);
    let relative = scene.json_plan(Some("run".as_ref()));

    let over = mount_over(&with_place, &scene.hidden);
    assert_eq!(over[0]["value"], "--ro-bind", "{with_place}");
    let bound = PathBuf::from(over[1]["value"].as_str().unwrap());
    assert_eq!(
        bound.parent(),
        Some(scene.place().as_path()),
        "{with_place}"
    );
    for plan in [&without, &relative] {
        let over = mount_over(plan, &scene.hidden);
        assert_eq!(over[0]["value"], "--ro-bind-data", "{plan}");
        assert_eq!(over[1]["kind"], "empty-file", "{plan}");
    }
    assert!(!scene.place().exists());
}

// @kotowari[REQ-460]
#[test]
fn req_460_a_nested_isolation_plans_and_runs_without_the_place() {
    // The runtime directory is inside the workspace, which the outer isolation leaves
    // writable, so the nested run could make the place if it tried.
    let (home, workspace) = home_with_workspace();
    let hidden = home.write("secret.txt", "SECRET-CONTENT\n");
    let runtime = workspace.join("run");
    std::fs::create_dir(&runtime).unwrap();
    let inner = home.write(
        "inner.toml",
        format!("[mounts]\nhide = [\"{}\"]\n", hidden.display()),
    );
    let nested = |arguments: &str| {
        run_inside(
            home.path(),
            &workspace,
            &format!(
                "exec /usr/bin/env XDG_RUNTIME_DIR={} {KAKOI} --nested=isolate \
                 --workspace {} --policy-file {} {arguments}",
                runtime.display(),
                workspace.display(),
                inner.display()
            ),
        )
    };

    let plan = nested("--print-plan=json");
    let run = nested(&format!("-- /bin/cat {}", hidden.display()));

    let report = output_report(&plan);
    assert_eq!(plan.status.code(), Some(0), "{report}");
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    let over = mount_over(&plan, &hidden);
    assert_eq!(over[0]["value"], "--ro-bind-data", "{plan}");
    assert_eq!(assert_quiet(&run), "");
    assert!(!runtime.join("kakoi").exists());
}

// @kotowari[EX-892]
#[test]
fn ex_892_outer_and_inner_can_hide_the_same_file() {
    let scene = Scene::new();

    let output = scene.run_with_place(&format!(
        "exec {KAKOI} --nested=isolate --workspace {} -- /bin/sh -c 'test -f {} && cat {} && echo ran'",
        scene.workspace.display(),
        "/dev/kakoi-isolated",
        scene.hidden.display()
    ));

    assert_eq!(assert_quiet(&output), "ran\n");
}

/// Every entry under `root`, as (relative path, kind, size), leaving out `skip`.
fn tree_snapshot(root: &Path, skip: &[&Path]) -> Vec<(PathBuf, String, u64)> {
    fn walk(root: &Path, dir: &Path, skip: &[&Path], into: &mut Vec<(PathBuf, String, u64)>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(Result::unwrap)
            .collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            if skip.contains(&path.as_path()) {
                continue;
            }
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            let metadata = std::fs::symlink_metadata(&path).unwrap();
            into.push((
                relative,
                format!("{:?}", metadata.file_type()),
                metadata.len(),
            ));
            if metadata.is_dir() {
                walk(root, &path, skip, into);
            }
        }
    }
    let mut snapshot = Vec::new();
    walk(root, root, skip, &mut snapshot);
    snapshot
}

// @kotowari[REQ-305]
#[test]
fn req_305_a_run_writes_nothing_of_the_host_but_the_place() {
    let scene = Scene::new();
    let place = scene.place();
    let before = tree_snapshot(scene.home.path(), &[&place]);

    let plan = scene
        .kakoi(Some(scene.runtime.as_os_str()))
        .arg("--print-plan")
        .output()
        .unwrap();
    assert_quiet(&plan);
    assert!(!place.exists());
    let run = scene.run_with_place("true");
    assert_quiet(&run);

    assert_eq!(tree_snapshot(scene.home.path(), &[&place]), before);
    assert!(place.is_dir());
}

// @kotowari[REQ-287]
#[test]
fn req_287_concurrent_runs_share_the_place_without_affecting_each_other() {
    let scene = Scene::new();
    let start = |code: i32| {
        scene
            .kakoi(Some(scene.runtime.as_os_str()))
            .args([
                "--",
                "/bin/sh",
                "-c",
                &format!("test ! -s {} && exit {code}", scene.hidden.display()),
            ])
            .spawn()
            .unwrap()
    };

    let children: Vec<_> = (0..8).map(|index| (index, start(10 + index))).collect();

    for (index, mut child) in children {
        assert_eq!(child.wait().unwrap().code(), Some(10 + index));
    }
}

// Keeping the place from writable items.

impl Scene {
    /// Replaces the profile with the workspace and `rw` as `rw`, the hidden file, and
    /// `more` in the mounts table.
    fn profile(&self, rw: Option<&Path>, more: &str) {
        let rw = rw.map_or(String::new(), |path| format!(", \"{}\"", path.display()));
        self.home.write(
            ".config/kakoi/profile/default.toml",
            format!(
                "[mounts]\nrw = [\"${{workspace}}\"{rw}]\nhide = [\"{}\"]\n{more}",
                self.hidden.display()
            ),
        );
    }
}

// @kotowari[EX-895]
#[test]
fn ex_895_the_place_under_an_rw_item_is_a_path_diagnostic() {
    let scene = Scene::new();
    scene.profile(Some(&scene.runtime), "");

    let output = scene.run_with_place("echo ran");

    assert_diagnostic(&output, 125, "path");
    assert!(!scene.place().exists());
}

// @kotowari[REQ-462]
#[test]
fn req_462_an_rw_file_item_inside_the_place_is_a_path_diagnostic_in_a_run_and_a_plan() {
    let scene = Scene::new();
    let file = made_file(&scene);
    scene.profile(None, &format!("rw-file = [\"{}\"]\n", file.display()));

    let run = scene.run_with_place("echo ran");
    let plan = scene
        .kakoi(Some(scene.runtime.as_os_str()))
        .arg("--print-plan")
        .output()
        .unwrap();

    assert_diagnostic(&run, 125, "path");
    assert_diagnostic(&plan, 125, "path");
}

// @kotowari[REQ-462]
#[test]
fn req_462_the_place_is_checked_only_when_it_would_be_used() {
    let scene = Scene::new();
    scene.profile(Some(&scene.runtime), "");

    let plan = scene
        .kakoi(Some(scene.runtime.as_os_str()))
        .arg("--print-plan")
        .output()
        .unwrap();
    let without = scene.run(None, "echo ran");

    assert_diagnostic(&plan, 125, "path");
    assert_eq!(assert_quiet(&without), "ran\n");
}

// @kotowari[REQ-462]
#[test]
fn req_462_a_run_that_places_nothing_from_the_place_may_cover_it_with_rw() {
    let scene = Scene::new();
    scene.home.write(
        ".config/kakoi/profile/default.toml",
        format!(
            "[mounts]\nrw = [\"${{workspace}}\", \"{}\"]\n",
            scene.runtime.display()
        ),
    );

    let plan = scene
        .kakoi(Some(scene.runtime.as_os_str()))
        .arg("--print-plan")
        .output()
        .unwrap();
    let run = scene.run_with_place("echo ran");

    let report = output_report(&plan);
    assert_eq!(plan.status.code(), Some(0), "{report}");
    assert_eq!(assert_quiet(&run), "ran\n");
}

// @kotowari[REQ-295, REQ-462]
#[test]
fn req_295_the_place_is_named_after_the_other_protected_paths() {
    let scene = Scene::new();
    let token = scene.home.write("run/token", "FAKE-TOKEN\n");
    scene.profile(
        Some(&scene.runtime),
        &format!("[secrets]\nTOKEN = \"{}\"\n", token.display()),
    );

    let output = scene.run_with_place("echo ran");

    let diagnostic = assert_diagnostic(&output, 125, "path");
    assert!(diagnostic.contains("TOKEN"), "{diagnostic}");
}
