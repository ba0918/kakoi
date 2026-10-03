use kakoi_runtime::{
    prepare, CommandSpec, HostContext, Io, MainOutcome, Policy, RunRequest, StdioSpec,
};
use std::io::Read;

fn request(policy: &str, program: &str, args: &[&str]) -> RunRequest {
    let mut command = CommandSpec::new(program.into());
    for argument in args {
        command = command.arg((*argument).into());
    }
    RunRequest::new(
        Policy::from_toml(policy).unwrap(),
        command,
        HostContext::capture().unwrap(),
        StdioSpec {
            stdin: Io::Null,
            stdout: Io::Pipe,
            stderr: Io::Inherit,
        },
    )
}

pub fn own_guard() {
    let cwd = std::env::current_dir().unwrap();
    std::os::unix::fs::symlink(std::env::current_exe().unwrap(), cwd.join("app")).unwrap();
    let policy = format!("[network]\nmode='none'\n[env.set]\nPATH={:?}\n[[commands.guard]]\nprogram='app'\ndeny=[['blocked']]\nreason='blocked application'\n", cwd.to_str().unwrap());
    let prepared = prepare(request(&policy, "app", &["blocked"])).unwrap();
    assert_eq!(prepared.description().guard_count, 1);
    assert_eq!(
        prepared.spawn().unwrap().wait().main,
        MainOutcome::Exited(126)
    );
}

pub fn listed_copies() {
    let cwd = std::env::current_dir().unwrap();
    let policy = format!("[mounts]\nhide=[{:?}]\n[network]\nmode='none'\n[env.set]\nPATH={:?}\n[commands]\nmode='listed'\nallow=['/usr/bin/python3','/bin/sh',{:?}]\n[[commands.guard]]\nprogram='tool'\nguard-absolute-path=true\ndeny=[['blocked']]\nreason='blocked'\n[[commands.guard]]\nprogram='tool2'\ndeny=[['blocked']]\nreason='blocked'\n",cwd.join("secret").to_str().unwrap(), cwd.join("tools").to_str().unwrap(), cwd.join("tools").to_str().unwrap());
    let code = "import os,subprocess; a=os.stat('/dev/kakoi-guard/bin/tool'); b=os.stat('/dev/kakoi-guard/bin/tool2'); c=os.stat(os.getcwd()+'/tools/tool'); assert len({(a.st_dev,a.st_ino),(b.st_dev,b.st_ino),(c.st_dev,c.st_ino)})==3; assert subprocess.call(['tool'])==7; assert subprocess.call(['tool2'])==8; assert subprocess.call(['tool','blocked'])==126; assert subprocess.call([os.getcwd()+'/tools/tool'])==7; assert subprocess.call([os.getcwd()+'/tools/tool','blocked'])==126; print('copies passed')";
    let mut run = prepare(request(&policy, "/usr/bin/python3", &["-c", code]))
        .unwrap()
        .spawn()
        .unwrap();
    let mut output = String::new();
    run.take_stdout()
        .unwrap()
        .read_to_string(&mut output)
        .unwrap();
    assert_eq!(run.wait().main, MainOutcome::Exited(0));
    assert_eq!(output.trim(), "copies passed");
    let executable = std::env::current_exe().unwrap();
    let error = prepare(request(
        &policy,
        executable.to_str().unwrap(),
        &["--self-test-input"],
    ))
    .unwrap()
    .spawn()
    .unwrap_err();
    assert_eq!(error.main, MainOutcome::NotStarted);
    assert!(error.diagnostics.iter().any(|d| d.os_error == Some(13)));
}

pub fn changed_generated_table() {
    let cwd = std::env::current_dir().unwrap();
    let policy=format!("[network]\nmode='none'\n[env.set]\nPATH={:?}\n[[commands.guard]]\nprogram='tool'\ndeny=[['blocked']]\nreason='blocked'\n",cwd.join("tools").to_str().unwrap());
    let prepared = prepare(request(&policy, "tool", &["blocked"])).unwrap();
    let error = prepared.spawn().unwrap_err();
    assert_eq!(error.kind, kakoi_runtime::ErrorKind::PlanChanged);
    assert_eq!(error.main, MainOutcome::NotStarted);
    assert!(!cwd.join("target-ran").exists());
}

pub fn dispatch_failures() {
    let cwd = std::env::current_dir().unwrap();
    let mut policy = format!(
        "[network]\nmode='none'\n[env.set]\nPATH={:?}\n",
        cwd.join("tools").to_str().unwrap()
    );
    for program in [
        "tool",
        "tool2",
        "tool (deleted)",
        "tool (deleted) (deleted)",
    ] {
        policy.push_str(&format!(
            "[[commands.guard]]\nprogram={program:?}\ndeny=[['blocked']]\nreason='blocked'\n"
        ));
    }
    let mut run = prepare(request(&policy, "/usr/bin/python3", &["dispatch.py"]))
        .unwrap()
        .spawn()
        .unwrap();
    let mut output = String::new();
    run.take_stdout()
        .unwrap()
        .read_to_string(&mut output)
        .unwrap();
    assert_eq!(run.wait().main, MainOutcome::Exited(0));
    assert_eq!(output.trim(), "dispatch passed");
}

pub fn probe_descendants() {
    let cwd = std::env::current_dir().unwrap();
    let policy=format!("[network]\nmode='none'\n[env.set]\nPATH={:?}\nKAKOI_TEST_PROBE_CHILD='1'\n[commands]\nmode='listed'\nallow=['/usr/bin/python3']\n[[commands.guard]]\nprogram='tool'\ndeny=[['blocked']]\nreason='blocked'\n[[commands.guard]]\nprogram='tool2'\ndeny=[['blocked']]\nreason='blocked'\n",cwd.join("tools").to_str().unwrap());
    let code="import os; p=[n for n in os.listdir('/proc') if n.isdigit()]; assert all(open('/proc/'+n+'/comm').read().strip()!='probe-child' for n in p); print('target-clean')";
    let mut run = prepare(request(&policy, "/usr/bin/python3", &["-c", code]))
        .unwrap()
        .spawn()
        .unwrap();
    let mut output = String::new();
    run.take_stdout()
        .unwrap()
        .read_to_string(&mut output)
        .unwrap();
    assert_eq!(run.wait().main, MainOutcome::Exited(0));
    assert_eq!(output, "probe-created\nprobe-created\ntarget-clean\n");
}

pub fn missing_dynamic_dependencies() {
    let cwd = std::env::current_dir().unwrap();
    let python = std::fs::canonicalize("/usr/bin/python3").unwrap();
    let python = python.to_str().unwrap();
    let code = "from pathlib import Path; Path('target-ran').write_text('ran')";
    let visible = format!(
        "[mounts]\nmode='listed'\nrw=[{:?}]\n[network]\nmode='none'\n",
        cwd.to_str().unwrap()
    );
    let run = prepare(request(&visible, python, &["-c", code]))
        .unwrap()
        .spawn()
        .unwrap();
    assert_eq!(run.wait().main, MainOutcome::Exited(0));
    std::fs::remove_file(cwd.join("target-ran")).unwrap();
    let missing=format!("[mounts]\nmode='listed'\nsystem=false\nrw=[{:?}]\nro=[{python:?}]\n[network]\nmode='none'\n",cwd.to_str().unwrap());
    let prepared = prepare(request(&missing, python, &["-c", code])).unwrap();
    assert!(prepared.spawn().is_err());
    assert!(!cwd.join("target-ran").exists());
}

pub fn denied_guard() {
    let cwd = std::env::current_dir().unwrap();
    let policy=format!("[network]\nmode='none'\n[env.set]\nPATH={:?}\n[[commands.guard]]\nprogram='tool'\ndeny=[['blocked']]\nreason='blocked'\n",cwd.join("tools").to_str().unwrap());
    let error = prepare(request(&policy, "tool", &[]))
        .unwrap()
        .spawn()
        .unwrap_err();
    assert_eq!(error.kind, kakoi_runtime::ErrorKind::HelperFailure);
    assert_eq!(error.main, MainOutcome::NotStarted);
    assert_eq!(error.cleanup, kakoi_runtime::Cleanup::Confirmed);
    assert!(!cwd.join("target-ran").exists());
}
