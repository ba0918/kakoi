use std::ffi::OsString;
use std::path::{Path, PathBuf};

use kakoi_linux::retained_mounts::{recheck, retain, Identity};
use kakoi_runtime::cli::plan::Argument;

use crate::common::TempDir;

fn bind(source: &Path, destination: &Path) -> Vec<Argument> {
    [
        OsString::from("--ro-bind"),
        source.as_os_str().into(),
        destination.as_os_str().into(),
    ]
    .map(Argument::Literal)
    .into()
}

// @kotowari[REQ-library-106]
#[test]
fn retained_mount_detects_symlink_retargeting_without_fixing_live_contents() {
    use std::os::unix::fs::symlink;
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    let original = dir.write("original", "first");
    let replacement = dir.write("replacement", "other");
    let link = dir.path().join("link");
    symlink(&original, &link).unwrap();
    let mounts = retain(&bind(&link, Path::new("/mounted"))).unwrap();
    let before = Identity::of_fd(&mounts[0].descriptor).unwrap();
    std::fs::write(&original, "updated live contents").unwrap();
    recheck(&mounts).unwrap();
    assert_eq!(Identity::of_fd(&mounts[0].descriptor).unwrap(), before);
    std::fs::remove_file(&link).unwrap();
    symlink(&replacement, &link).unwrap();
    assert_eq!(
        recheck(&mounts).unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
    assert_eq!(Identity::of_fd(&mounts[0].descriptor).unwrap(), before);
}

// @kotowari[REQ-library-106]
#[test]
fn retained_mount_detects_an_inode_replaced_at_the_same_name() {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    let original = dir.write("source", "first");
    let replacement = dir.write("replacement", "second");
    let mounts = retain(&bind(&original, Path::new("/mounted"))).unwrap();
    std::fs::rename(replacement, original).unwrap();
    assert_eq!(
        recheck(&mounts).unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
}

// @kotowari[REQ-library-106]
#[test]
fn descriptor_mount_reaches_bwrap_with_the_retained_inode_and_live_data() {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    let source = dir.write("source", "first");
    let plan = plan(
        &dir,
        &source,
        vec!["/bin/cat".into(), source.as_os_str().into()],
    );
    let mounts = retain(&plan.arguments[..plan.launch_layout.command_separator.unwrap()]).unwrap();
    std::fs::write(&source, "updated live contents").unwrap();
    let mut launch = kakoi_linux::launch::assemble_retained(&plan, &mounts).unwrap();
    let args = launch.command.get_args().collect::<Vec<_>>();
    assert!(args.iter().any(|arg| *arg == "--ro-bind-fd"));
    // A rename after the assembly check cannot redirect the descriptor mount.
    // The final init handshake is a separate, still-required library gate.
    let replacement = dir.write("replacement", "different inode");
    std::fs::rename(&source, dir.path().join("moved-original")).unwrap();
    std::fs::rename(replacement, source).unwrap();
    let output = launch.command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        crate::common::output_report(&output)
    );
    assert_eq!(output.stdout, b"updated live contents");
}

// @kotowari[REQ-library-106]
#[test]
fn an_unlinked_descriptor_source_fails_before_the_command_can_write() {
    let dir = TempDir::under(Path::new(env!("CARGO_TARGET_TMPDIR")));
    let source = dir.write("source", "first");
    let marker = dir.path().join("workspace/command-started");
    let plan = plan(
        &dir,
        &source,
        vec![
            "/bin/sh".into(),
            "-c".into(),
            "echo started > command-started".into(),
        ],
    );
    let mounts = retain(&plan.arguments[..plan.launch_layout.command_separator.unwrap()]).unwrap();
    let output = kakoi_linux::launch::assemble_retained(&plan, &mounts)
        .unwrap()
        .command
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        crate::common::output_report(&output)
    );
    assert!(marker.exists(), "the positive command did not write");
    std::fs::remove_file(&marker).unwrap();
    let mut launch = kakoi_linux::launch::assemble_retained(&plan, &mounts).unwrap();
    std::fs::remove_file(source).unwrap();
    let output = launch.command.output().unwrap();
    assert!(
        !output.status.success(),
        "{}",
        crate::common::output_report(&output)
    );
    assert!(
        !marker.exists(),
        "the command ran after losing the mount source"
    );
}

fn plan(dir: &TempDir, source: &Path, command: Vec<OsString>) -> kakoi_runtime::cli::plan::Plan {
    use kakoi_runtime::cli::layers::LayerSelection;
    use kakoi_runtime::cli::planning::{plan_for, Request};
    let home = dir.path().join("home");
    dir.write(
        "home/.config/kakoi/profile/default.toml",
        "[network]\nmode='none'\n",
    );
    dir.write("workspace/.git/HEAD", "ref: refs/heads/test\n");
    let policy = dir.write(
        "policy.toml",
        format!(
            "[mounts]\nro=[{:?}]\nrw=[{:?}]\n",
            source,
            dir.path().join("workspace")
        ),
    );
    let host = [
        (OsString::from("HOME"), home.into_os_string()),
        (OsString::from("PATH"), std::env::var_os("PATH").unwrap()),
    ]
    .into();
    plan_for(&Request {
        layers: LayerSelection {
            profile: "default".into(),
            policy_file: Some(policy),
            rw: vec![],
            hide: vec![],
        },
        workspace: None,
        command,
        current_dir: dir.path().join("workspace"),
        host,
        executable: None,
        nested: false,
        applied: true,
        outer_guard: false,
        outer_table: None,
        landlock_abi: kakoi_linux::landlock::abi_version(),
    })
    .unwrap()
}

// @kotowari[REQ-library-106]
#[test]
fn device_identity_includes_type_and_device_number() {
    let mounts = retain(&[
        Argument::Literal("--dev-bind".into()),
        Argument::Literal("/dev/null".into()),
        Argument::Literal("/dev/null".into()),
    ])
    .unwrap();
    let identity = Identity::of_fd(&mounts[0].descriptor).unwrap();
    assert_eq!(identity.kind, libc::S_IFCHR);
    assert_ne!(identity.rdev, 0);
    assert_eq!(
        Identity::of_path(&PathBuf::from("/dev/null")).unwrap(),
        identity
    );
}
