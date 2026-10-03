use std::collections::BTreeMap;

use kakoi_runtime::{prepare, CommandSpec, HostContext, Io, Policy, RunRequest, StdioSpec};

/// Runs `/bin/echo` in a `none` isolation over a synthetic home and workspace,
/// reads its output through a pipe, and waits for the retained result.
pub fn run() {
    use std::io::Read;
    let root = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join(format!("library-self-test-{}", std::process::id()));
    std::fs::create_dir_all(root.join("home")).unwrap();
    std::fs::create_dir_all(root.join("workspace/.git")).unwrap();
    let context = HostContext::new(
        root.join("workspace"),
        BTreeMap::from([
            ("HOME".into(), root.join("home").into_os_string()),
            ("PATH".into(), std::env::var_os("PATH").unwrap()),
        ]),
    )
    .unwrap();
    let mut running = prepare(RunRequest::new(
        Policy::from_toml("[network]\nmode='none'\n").unwrap(),
        CommandSpec::new("/bin/echo".into()).arg("library sync".into()),
        context,
        StdioSpec {
            stdin: Io::Null,
            stdout: Io::Pipe,
            stderr: Io::Null,
        },
    ))
    .unwrap()
    .spawn()
    .unwrap();
    let mut output = String::new();
    running
        .take_stdout()
        .unwrap()
        .read_to_string(&mut output)
        .unwrap();
    assert_eq!(output, "library sync\n");
    assert_eq!(running.wait().main, kakoi_runtime::MainOutcome::Exited(0));
    std::fs::remove_dir_all(root).unwrap();
    println!("sync self-test passed");
}
