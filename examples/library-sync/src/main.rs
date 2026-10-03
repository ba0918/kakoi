use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;

use kakoi_runtime::{
    config, dispatch_helper, prepare, CommandSpec, Dispatch, HostContext, Io, ListMode,
    NetworkMode, Policy, RunRequest, StdioSpec,
};

#[path = "../../../tests/fixtures/library-api/signal_state.rs"]
mod signal_state;

// A supported ELF initializer can run before main. The fixture deliberately
// interrupts only the product-owned worker, never an unrelated process.
#[used]
#[unsafe(link_section = ".init_array")]
static PREPARE_FAULT: extern "C" fn() = prepare_fault;

extern "C" fn prepare_fault() {
    unsafe extern "C" {
        fn getenv(name: *const std::ffi::c_char) -> *const std::ffi::c_char;
        fn atoi(value: *const std::ffi::c_char) -> i32;
        fn close(fd: i32) -> i32;
        fn raise(signal: i32) -> i32;
    }
    unsafe {
        let worker = getenv(c"KAKOI_RUNTIME_WORKER_FD".as_ptr());
        let fault = getenv(c"KAKOI_TEST_PREPARE_FAULT".as_ptr());
        if !worker.is_null() && !fault.is_null() {
            if *fault == b's' as std::ffi::c_char {
                raise(19);
            } else {
                close(atoi(worker));
            }
        }
    }
}

fn main() -> std::process::ExitCode {
    match dispatch_helper().unwrap() {
        Dispatch::Application => {}
        Dispatch::Completed(code) => return code,
    }
    match std::env::args_os().nth(1).as_deref() {
        Some(value) if value == "--self-test-input" => self_test_input(),
        Some(value) if value == "--self-test-prepare" => self_test_prepare(),
        Some(value) if value == "--self-test-prepare-errors" => self_test_prepare_errors(),
        Some(value) if value == "--self-test-fds" => self_test_fds(),
        Some(value) if value == "--self-test-prepare-fault" => self_test_prepare_fault(),
        Some(value) if value == "--self-test-repeat" => self_test_repeat(),
        _ => panic!("this example currently verifies input and preparation only"),
    }
    std::process::ExitCode::SUCCESS
}

fn self_test_prepare() {
    let context = HostContext::capture().unwrap();
    let before = context.clone();
    let program = OsString::from_vec(b"./tool-\xff".to_vec());
    let policy = Policy::from_toml(
        "[network]\nmode='none'\n[env]\nmode='clear'\n[env.set]\nSECRET='private-value'\n",
    )
    .unwrap();
    let request = RunRequest::new(
        policy,
        CommandSpec::new(program)
            .arg("argument".into())
            .arg("--bind".into())
            .arg("/missing-mount-source".into())
            .arg("/".into()),
        context,
        StdioSpec {
            stdin: Io::Null,
            stdout: Io::Null,
            stderr: Io::Null,
        },
    );
    let prepared = prepare(request).unwrap();
    let description = prepared.description();
    assert_eq!(description.cwd, before.cwd());
    assert_eq!(
        description.program,
        before.cwd().join(OsString::from_vec(b"tool-\xff".to_vec()))
    );
    assert_eq!(description.network_mode, NetworkMode::None);
    assert!(!format!("{prepared:?}").contains("private-value"));
    assert!(!before.cwd().join("command-ran").exists());
    assert_eq!(HostContext::capture().unwrap(), before);
    drop(prepared);
    assert!(!before.cwd().join("command-ran").exists());
    println!("worker preparation succeeded");
}

fn self_test_prepare_errors() {
    let context = HostContext::capture().unwrap();
    let stdio = || StdioSpec {
        stdin: Io::Null,
        stdout: Io::Null,
        stderr: Io::Null,
    };
    let request = RunRequest::new(
        Policy::from_toml("").unwrap(),
        CommandSpec::new(OsString::from_vec(b"/bin/true\0".to_vec())),
        context.clone(),
        stdio(),
    );
    let error = prepare(request).unwrap_err();
    assert_eq!(error.kind, kakoi_runtime::ErrorKind::InvalidInput);
    assert_eq!(error.cleanup, kakoi_runtime::Cleanup::NotNeeded);
    let mut env = context.environment().clone();
    env.insert("PATH".into(), context.cwd().join("missing-tools").into());
    let context = HostContext::new(context.cwd().into(), env).unwrap();
    let request = RunRequest::new(
        Policy::from_toml("").unwrap(),
        CommandSpec::new("/bin/true".into()),
        context,
        stdio(),
    );
    let error = prepare(request).unwrap_err();
    assert_eq!(error.kind, kakoi_runtime::ErrorKind::UnsupportedEnvironment);
    assert_eq!(error.cleanup, kakoi_runtime::Cleanup::Confirmed);
    assert!(children().is_empty());
}

fn children() -> String {
    std::fs::read_to_string(format!("/proc/self/task/{}/children", std::process::id()))
        .unwrap()
        .trim()
        .into()
}

fn self_test_fds() {
    use std::os::fd::AsRawFd;
    use std::time::{Duration, Instant};
    unsafe extern "C" {
        fn fcntl(fd: i32, operation: i32, ...) -> i32;
    }
    let context = HostContext::capture().unwrap();
    let before_limits = std::fs::read_to_string("/proc/self/limits").unwrap();
    signal_state::install_application_state();
    let before_signals = signal_state::Signals::capture();
    let before_raw = signal_state::raw_state();
    let source = std::fs::File::open("stdio-source").unwrap();
    let extra = std::fs::File::open("unrequested").unwrap();
    // A caller may supply descriptors from a foreign API without CLOEXEC.
    // The helper must still inherit only capabilities assigned to this request.
    unsafe {
        assert_eq!(fcntl(source.as_raw_fd(), 2, 0), 0);
        assert_eq!(fcntl(extra.as_raw_fd(), 2, 0), 0);
    }
    let prepared = prepare(RunRequest::new(
        Policy::from_toml("[network]\nmode='none'\n").unwrap(),
        CommandSpec::new("/bin/true".into()),
        context.clone(),
        StdioSpec {
            stdin: Io::Fd(source.into()),
            stdout: Io::Null,
            stderr: Io::Null,
        },
    ))
    .unwrap();
    let child: u32 = children().parse().unwrap();
    let descriptors = std::fs::read_dir(format!("/proc/{child}/fd"))
        .unwrap()
        .map(|entry| std::fs::read_link(entry.unwrap().path()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        descriptors
            .iter()
            .filter(|path| path.ends_with("stdio-source"))
            .count(),
        1
    );
    assert_eq!(
        descriptors
            .iter()
            .filter(|path| path.ends_with("unrequested"))
            .count(),
        0
    );
    assert_eq!(
        std::fs::read_to_string("/proc/self/limits").unwrap(),
        before_limits
    );
    before_signals.assert_preserved();
    assert_eq!(HostContext::capture().unwrap(), context);
    drop(prepared);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !children().is_empty() {
        assert!(
            Instant::now() < deadline,
            "preparation worker was not reaped"
        );
        std::thread::yield_now();
    }
    assert!(!std::path::Path::new(&format!("/proc/{child}")).exists());
    before_signals.assert_preserved();
    signal_state::report(before_raw, before_signals.usable);
    drop(extra);
}

fn null_request() -> RunRequest {
    RunRequest::new(
        Policy::from_toml("[network]\nmode='none'\n").unwrap(),
        CommandSpec::new("/bin/true".into()),
        HostContext::capture().unwrap(),
        StdioSpec {
            stdin: Io::Null,
            stdout: Io::Null,
            stderr: Io::Null,
        },
    )
}

fn self_test_prepare_fault() {
    let error = prepare(null_request()).unwrap_err();
    assert_eq!(error.cleanup, kakoi_runtime::Cleanup::Confirmed);
    assert!(matches!(
        error.kind,
        kakoi_runtime::ErrorKind::Io | kakoi_runtime::ErrorKind::Protocol
    ));
    assert!(children().is_empty());
}

fn self_test_repeat() {
    use std::time::{Duration, Instant};
    let count = || std::fs::read_dir("/proc/self/fd").unwrap().count();
    let before = count();
    for _ in 0..16 {
        drop(prepare(null_request()).unwrap());
        let deadline = Instant::now() + Duration::from_secs(5);
        while !children().is_empty() {
            assert!(Instant::now() < deadline, "worker not reaped");
            std::thread::yield_now();
        }
        assert_eq!(count(), before, "request descriptors leaked");
    }
}

fn self_test_input() {
    let cwd = std::env::current_dir().unwrap();
    let env: BTreeMap<_, _> = std::env::vars_os().collect();
    let context = HostContext::new(cwd.clone(), env.clone()).unwrap();
    assert_eq!(context.cwd(), cwd);
    assert_eq!(context.environment(), &env);
    assert!(HostContext::new(PathBuf::from("relative"), env.clone()).is_err());
    assert!(HostContext::new(
        PathBuf::from(OsString::from_vec(b"/with\0nul".to_vec())),
        env.clone()
    )
    .is_err());
    for name in ["", "A=B", "A\0B"] {
        let mut invalid = env.clone();
        invalid.insert(name.into(), "value".into());
        assert!(HostContext::new(cwd.clone(), invalid).is_err());
    }
    let mut invalid = env.clone();
    invalid.insert("NUL_VALUE".into(), OsString::from_vec(b"value\0".to_vec()));
    assert!(HostContext::new(cwd.clone(), invalid).is_err());
    let mut non_utf8 = env.clone();
    let name = OsString::from_vec(b"RAW_\xff".to_vec());
    let value = OsString::from_vec(b"VALUE_\xfe".to_vec());
    non_utf8.insert(name.clone(), value.clone());
    let raw = HostContext::new(cwd.clone(), non_utf8).unwrap();
    assert_eq!(raw.environment().get(&name), Some(&value));
    assert!(!format!("{raw:?}").contains("VALUE_"));
    let selection = config::Selection {
        profile: "explicit".into(),
        policy_file: Some("policy.toml".into()),
        rw: Vec::new(),
        hide: Vec::new(),
    };
    let loaded = config::load(&selection, &context).unwrap();
    assert_eq!(loaded.policy.mounts_mode(), ListMode::Host);
    assert_eq!(loaded.policy.network_mode(), NetworkMode::None);
    assert_eq!(loaded.sources.len(), 2);
    let prepared = prepare(RunRequest::new(
        loaded.policy,
        CommandSpec::new("/bin/true".into()),
        context,
        StdioSpec {
            stdin: Io::Null,
            stdout: Io::Null,
            stderr: Io::Null,
        },
    ))
    .unwrap();
    assert_eq!(prepared.description().network_mode, NetworkMode::None);
    drop(prepared);
    assert_eq!(std::env::current_dir().unwrap(), cwd);
    assert_eq!(std::env::vars_os().collect::<BTreeMap<_, _>>(), env);
    println!("explicit input validation succeeded");
}
