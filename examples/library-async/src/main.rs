use kakoi_runtime::{
    prepare, CommandSpec, HostContext, Io, MainOutcome, Policy, RunRequest, StdioSpec,
};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use tokio::io::unix::AsyncFd;

fn main() -> std::process::ExitCode {
    match kakoi_runtime::dispatch_helper().unwrap() {
        kakoi_runtime::Dispatch::Completed(code) => return code,
        kakoi_runtime::Dispatch::Application => {}
    }
    // Dispatch runs before constructing any asynchronous runtime.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
        .unwrap();
    runtime.block_on(self_test());
    std::process::ExitCode::SUCCESS
}

fn async_pipe(fd: OwnedFd) -> io::Result<AsyncFd<File>> {
    let file = File::from(fd);
    // SAFETY: fcntl only reads/updates flags of the descriptor owned by this file.
    let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        || unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
    {
        return Err(io::Error::last_os_error());
    }
    AsyncFd::new(file)
}

async fn write_input(pipe: AsyncFd<File>, bytes: &[u8]) -> io::Result<()> {
    let mut offset = 0;
    while offset < bytes.len() {
        let mut ready = pipe.writable().await?;
        match ready.try_io(|fd| fd.get_ref().write(&bytes[offset..])) {
            Ok(Ok(0)) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(Ok(count)) => offset += count,
            Ok(Err(cause)) if cause.kind() == io::ErrorKind::Interrupted => {}
            Ok(Err(cause)) => return Err(cause),
            Err(_) => {}
        }
    }
    // Dropping the sole input owner sends EOF; no duplicated owner remains.
    Ok(())
}

async fn read_output(pipe: AsyncFd<File>) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut bytes = [0; 16384];
    loop {
        let mut ready = pipe.readable().await?;
        match ready.try_io(|fd| fd.get_ref().read(&mut bytes)) {
            Ok(Ok(0)) => return Ok(output),
            Ok(Ok(count)) => output.extend_from_slice(&bytes[..count]),
            Ok(Err(cause)) if cause.kind() == io::ErrorKind::Interrupted => {}
            Ok(Err(cause)) => return Err(cause),
            Err(_) => {}
        }
    }
}

async fn self_test() {
    let root = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join(format!("library-async-self-test-{}", std::process::id()));
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
        CommandSpec::new("/bin/cat".into()),
        context,
        StdioSpec {
            stdin: Io::Pipe,
            stdout: Io::Pipe,
            stderr: Io::Null,
        },
    ))
    .unwrap()
    .spawn()
    .unwrap();
    let input = async_pipe(running.take_stdin().unwrap().into()).unwrap();
    let output = async_pipe(running.take_stdout().unwrap().into()).unwrap();
    assert!(running.take_stdin().is_none());
    assert!(running.take_stdout().is_none());
    let payload = vec![b'x'; 1024 * 1024];
    let (written, received, outcome) = tokio::join!(
        write_input(input, &payload),
        read_output(output),
        running.wait_async()
    );
    written.unwrap();
    assert_eq!(received.unwrap(), payload);
    assert_eq!(outcome.main, MainOutcome::Exited(0));
    assert!(std::sync::Arc::ptr_eq(&outcome, &running.wait()));
    drop(running);
    std::fs::remove_dir_all(root).unwrap();
    println!("async pipe self-test passed");
}
