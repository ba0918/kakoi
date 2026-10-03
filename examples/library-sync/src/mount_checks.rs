use kakoi_runtime::{prepare, CommandSpec, HostContext, Io, Policy, RunRequest, StdioSpec};

pub(super) fn final_mounts() {
    let context = HostContext::capture().unwrap();
    let directory = context.cwd().join("overlay-directory");
    std::fs::create_dir_all(directory.join("child")).unwrap();
    let file = directory.join("child/file");
    std::fs::write(&file, b"original").unwrap();
    std::fs::write(directory.join("sibling"), b"hidden sibling").unwrap();
    let dir = format!("{:?}", directory.to_str().unwrap());
    let source = format!("{:?}", file.to_str().unwrap());
    for (directives, check) in [
        (format!("rw=[{dir}]\nhide=[{source}]"), "assert open(p).read()==''"),
        (format!("rw-file=[{source}]\nhide=[{dir}]"), "assert open(p).read()=='original'; assert not os.path.exists(os.path.join(os.path.dirname(os.path.dirname(p)),'sibling'))"),
        (format!("ro=[{dir}]\nrw-file=[{source}]"), "assert open(p).read()=='original'"),
        (format!("rw=[{dir}]\nrw-copy=[{source}]"), "assert open(p).read()=='original'; open(p,'w').write('copy-only')"),
    ] {
        let running = prepare(RunRequest::new(
            Policy::from_toml(&format!("[mounts]\n{directives}\n[network]\nmode='none'\n")).unwrap(),
            CommandSpec::new("/usr/bin/python3".into()).arg("-c".into())
                .arg(format!("import os,sys\np=sys.argv[1]\n{check}").into()).arg(file.clone().into()),
            context.clone(), StdioSpec { stdin: Io::Null, stdout: Io::Null, stderr: Io::Inherit },
        )).unwrap().spawn().unwrap();
        assert_eq!(running.wait().main, kakoi_runtime::MainOutcome::Exited(0), "{directives}");
        drop(running);
        assert_eq!(std::fs::read(&file).unwrap(), b"original");
    }
}
