use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;

use kakoi_runtime::{
    config, dispatch_helper, prepare, CommandSpec, Dispatch, HostContext, Io, ListMode,
    NetworkMode, Policy, RunRequest, StdioSpec,
};

mod helper_checks;
mod mount_checks;
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
        fn write(fd: i32, data: *const u8, length: usize) -> isize;
        fn pause() -> i32;
        fn open(path: *const std::ffi::c_char, flags: i32, mode: u32) -> i32;
        fn strlen(value: *const std::ffi::c_char) -> usize;
        fn pipe(fds: *mut i32) -> i32;
        fn fork() -> i32;
        fn read(fd: i32, data: *mut u8, length: usize) -> isize;
        fn prctl(option: i32, ...) -> i32;
    }
    unsafe {
        if !getenv(c"KAKOI_RUNTIME_PROBE_FD".as_ptr()).is_null()
            && !getenv(c"KAKOI_TEST_PROBE_CHILD".as_ptr()).is_null()
        {
            let mut fds = [-1; 2];
            if pipe(fds.as_mut_ptr()) == 0 {
                let pid = fork();
                if pid == 0 {
                    close(fds[0]);
                    prctl(15, c"probe-child".as_ptr(), 0usize, 0usize, 0usize);
                    write(fds[1], b"x".as_ptr(), 1);
                    close(fds[1]);
                    loop {
                        pause();
                    }
                }
                close(fds[1]);
                let mut ready = 0;
                if read(fds[0], &mut ready, 1) == 1 {
                    write(1, b"probe-created\n".as_ptr(), 14);
                }
                close(fds[0]);
            }
        }
        let worker = getenv(c"KAKOI_RUNTIME_WORKER_FD".as_ptr());
        let fault = getenv(c"KAKOI_TEST_PREPARE_FAULT".as_ptr());
        let record = getenv(c"KAKOI_TEST_CONTROL_RECORD".as_ptr());
        if !worker.is_null() && !record.is_null() {
            let output = open(record, 1 | 64 | 512, 0o600);
            if output >= 0 {
                write(output, worker.cast(), strlen(worker));
                let owner = getenv(c"KAKOI_RUNTIME_OWNER_FD".as_ptr());
                if !owner.is_null() {
                    write(output, b" ".as_ptr(), 1);
                    write(output, owner.cast(), strlen(owner));
                }
                close(output);
            }
        }
        if !getenv(c"KAKOI_RUNTIME_INIT_FD".as_ptr()).is_null()
            && !getenv(c"KAKOI_TEST_STARTUP_FAULT".as_ptr()).is_null()
        {
            write(1, b"init-startup\n".as_ptr(), 13);
            loop {
                pause();
            }
        }
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
        Some(value) if value == "--self-test-command-path" => self_test_command_path(),
        Some(value) if value == "--self-test-command-guard-path" => self_test_command_guard_path(),
        Some(value) if value == "--self-test-spawn" => self_test_spawn(),
        Some(value) if value == "--self-test-lifetime" => self_test_lifetime(),
        Some(value) if value == "--self-test-exec-error" => self_test_exec_error(),
        Some(value) if value == "--self-test-mount-identity" => self_test_mount_identity(),
        Some(value) if value == "--self-test-parallel" => self_test_parallel(),
        Some(value) if value == "--self-test-inherit" => self_test_inherit(),
        Some(value) if value == "--self-test-helper-environment" => self_test_helper_environment(),
        Some(value) if value == "--self-test-worker-death" => self_test_worker_death(),
        Some(value) if value == "--self-test-main-retention" => self_test_main_retention(),
        Some(value) if value == "--self-test-external-reap" => self_test_external_reap(),
        Some(value) if value == "--self-test-stopping-failure" => self_test_stopping_failure(),
        Some(value) if value == "--self-test-independent-supervision" => {
            self_test_independent_supervision()
        }
        Some(value) if value == "--self-test-owner" => self_test_owner(),
        Some(value) if value == "--self-test-startup-owner" => {
            let request = RunRequest::new(
                Policy::from_toml(
                    "[network]\nmode='none'\n[env.set]\nKAKOI_TEST_STARTUP_FAULT='stop'\n",
                )
                .unwrap(),
                CommandSpec::new("/bin/true".into()),
                HostContext::capture().unwrap(),
                StdioSpec {
                    stdin: Io::Null,
                    stdout: Io::Inherit,
                    stderr: Io::Null,
                },
            );
            prepare(request).unwrap().spawn().unwrap().wait();
        }
        Some(value) if value == "--self-test-preexec-death" => self_test_preexec_death(),
        Some(value) if value == "--self-test-context-run" => self_test_context_run(),
        Some(value) if value == "--self-test-missing-features" => self_test_missing_features(),
        Some(value) if value == "--self-test-shared-files" => self_test_shared_files(),
        Some(value) if value == "--self-test-live-device" => self_test_live_device(),
        Some(value) if value == "--self-test-raw-mounts" => self_test_raw_mounts(),
        Some(value) if value == "--self-test-final-mounts" => mount_checks::final_mounts(),
        Some(value) if value == "--self-test-own-guard" => helper_checks::own_guard(),
        Some(value) if value == "--self-test-listed-copies" => helper_checks::listed_copies(),
        Some(value) if value == "--self-test-changed-table" => {
            helper_checks::changed_generated_table()
        }
        Some(value) if value == "--self-test-dispatch-failures" => {
            helper_checks::dispatch_failures()
        }
        Some(value) if value == "--self-test-probe-descendants" => {
            helper_checks::probe_descendants()
        }
        Some(value) if value == "--self-test-missing-dynamic" => {
            helper_checks::missing_dynamic_dependencies()
        }
        Some(value) if value == "--self-test-denied-guard" => helper_checks::denied_guard(),
        Some(value) if value == "--self-test-data-batches" => helper_checks::data_batches(),
        Some(value) if value == "--self-test-filtered" => self_test_filtered(),
        Some(value) if value == "--self-test-filtered-owner" => self_test_filtered_owner(),
        Some(value) if value == "--self-test-api-nested" => self_test_api_nested(),
        Some(value) if value == "--self-test-api-inner" => self_test_api_inner(),
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

fn self_test_spawn() {
    use kakoi_runtime::{MainOutcome, ProcessCleanup};
    use std::io::Read;
    for mode in ["host", "none"] {
        let prepared = prepare(RunRequest::new(
            Policy::from_toml(&format!("[network]\nmode='{mode}'\n")).unwrap(),
            CommandSpec::new("/bin/sh".into())
                .arg("-c".into())
                .arg("printf 'pipe output'; sleep 1000 & exit 7".into()),
            HostContext::capture().unwrap(),
            StdioSpec {
                stdin: Io::Null,
                stdout: Io::Pipe,
                stderr: Io::Pipe,
            },
        ))
        .unwrap();
        let mut running = prepared.spawn().unwrap();
        let mut text = String::new();
        running
            .take_stdout()
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "pipe output");
        let result = running.wait();
        assert_eq!(result.main, MainOutcome::Exited(7));
        assert_eq!(result.network, kakoi_runtime::NetworkCleanup::NotApplicable);
        assert_eq!(result.processes, ProcessCleanup::ConfirmedReaped);
        assert!(std::sync::Arc::ptr_eq(&result, &running.wait()));
        assert!(children().is_empty());
    }
}

fn self_test_exec_error() {
    let prepared = prepare(RunRequest::new(
        Policy::from_toml("[network]\nmode='none'\n").unwrap(),
        CommandSpec::new("./bad-exec".into()),
        HostContext::capture().unwrap(),
        StdioSpec {
            stdin: Io::Null,
            stdout: Io::Null,
            stderr: Io::Null,
        },
    ))
    .unwrap();
    let error = prepared.spawn().unwrap_err();
    assert_eq!(error.main, kakoi_runtime::MainOutcome::NotStarted);
    assert_eq!(error.kind, kakoi_runtime::ErrorKind::Io);
    assert_eq!(error.cleanup, kakoi_runtime::Cleanup::Confirmed);
    assert_eq!(error.diagnostics[0].os_error, Some(2));
    assert!(children().is_empty());
}

fn self_test_mount_identity() {
    use std::os::unix::fs::symlink;
    std::fs::write("source", "original").unwrap();
    std::fs::write("replacement", "replacement").unwrap();
    symlink("source", "mount-link").unwrap();
    let context = HostContext::capture().unwrap();
    let policy = Policy::from_toml(&format!(
        "[mounts]\nrw-file=[{:?}]\n[network]\nmode='none'\n",
        context.cwd().join("mount-link").to_str().unwrap(),
    ))
    .unwrap();
    let prepared = prepare(RunRequest::new(
        policy,
        CommandSpec::new("/bin/sh".into())
            .arg("-c".into())
            .arg("touch must-not-run".into()),
        context,
        StdioSpec {
            stdin: Io::Null,
            stdout: Io::Null,
            stderr: Io::Null,
        },
    ))
    .unwrap();
    std::fs::remove_file("mount-link").unwrap();
    symlink("replacement", "mount-link").unwrap();
    let error = prepared.spawn().unwrap_err();
    assert_eq!(error.kind, kakoi_runtime::ErrorKind::PlanChanged);
    assert_eq!(error.main, kakoi_runtime::MainOutcome::NotStarted);
    assert_eq!(error.cleanup, kakoi_runtime::Cleanup::Confirmed);
    assert!(!std::path::Path::new("must-not-run").exists());
}

fn cat_request() -> RunRequest {
    RunRequest::new(
        Policy::from_toml("[network]\nmode='none'\n").unwrap(),
        CommandSpec::new("/bin/sh".into())
            .arg("-c".into())
            .arg("printf ready; exec /bin/cat".into()),
        HostContext::capture().unwrap(),
        StdioSpec {
            stdin: Io::Pipe,
            stdout: Io::Pipe,
            stderr: Io::Null,
        },
    )
}

fn self_test_context_run() {
    use std::io::Read;
    let before = HostContext::capture().unwrap();
    let cwd = before.cwd().join("requested");
    std::fs::create_dir(&cwd).unwrap();
    let program = OsString::from_vec(b"tool-\xff".to_vec());
    std::fs::copy("/bin/echo", cwd.join(&program)).unwrap();
    let mut environment = before.environment().clone();
    environment.insert("REQUEST_ONLY".into(), "specific-value".into());
    let context = HostContext::new(cwd.clone(), environment).unwrap();
    let mut relative = OsString::from("./");
    relative.push(program);
    let argument = OsString::from_vec(b"argument-\xfe".to_vec());
    let prepared = prepare(RunRequest::new(
        Policy::from_toml("[network]\nmode='none'\n").unwrap(),
        CommandSpec::new(relative).arg(argument),
        context.clone(),
        StdioSpec {
            stdin: Io::Null,
            stdout: Io::Pipe,
            stderr: Io::Null,
        },
    ))
    .unwrap();
    assert_eq!(prepared.description().cwd, cwd);
    let mut running = prepared.spawn().unwrap();
    let mut output = Vec::new();
    running
        .take_stdout()
        .unwrap()
        .read_to_end(&mut output)
        .unwrap();
    assert_eq!(output, b"argument-\xfe\n");
    assert_eq!(running.wait().main, kakoi_runtime::MainOutcome::Exited(0));
    let mut running = prepare(RunRequest::new(
        Policy::from_toml("[network]\nmode='none'\n").unwrap(),
        CommandSpec::new("/bin/sh".into())
            .arg("-c".into())
            .arg("printf '%s\\n' \"$PWD\" \"$REQUEST_ONLY\"".into()),
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
    assert_eq!(output, format!("{}\nspecific-value\n", cwd.display()));
    running.wait();
    assert_eq!(HostContext::capture().unwrap(), before);
}

fn self_test_helper_environment() {
    use std::io::Read;
    let mut running = prepare(RunRequest::new(
        Policy::from_toml("[network]\nmode='none'\n[env]\nmode='inherit'\n[env.set]\nKAKOI_RUNTIME_WORKER_FD='caller-value'\nKAKOI_RUNTIME_OWNER_FD='caller-value'\nKAKOI_RUNTIME_RESULT_FD='caller-value'\nKAKOI_RUNTIME_INIT_FD='caller-value'\n").unwrap(),
        CommandSpec::new("/usr/bin/env".into()),
        HostContext::capture().unwrap(),
        StdioSpec { stdin: Io::Null, stdout: Io::Pipe, stderr: Io::Null },
    )).unwrap().spawn().unwrap();
    let mut output = String::new();
    running
        .take_stdout()
        .unwrap()
        .read_to_string(&mut output)
        .unwrap();
    assert_eq!(running.wait().main, kakoi_runtime::MainOutcome::Exited(0));
    assert!(
        !output
            .lines()
            .any(|line| line.starts_with("KAKOI_RUNTIME_")),
        "{output}"
    );
}

fn self_test_missing_features() {
    use std::os::unix::fs::PermissionsExt;
    let before = HostContext::capture().unwrap();
    let tools = before.cwd().join("feature-tools");
    std::fs::create_dir(&tools).unwrap();
    let bwrap = tools.join("bwrap");
    std::fs::write(&bwrap, "#!/bin/sh\nif [ \"$1\" = --help ]; then printf 'bwrap 99.0'; exit 0; fi\ntouch helper-must-not-run\n").unwrap();
    std::fs::set_permissions(&bwrap, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut env = before.environment().clone();
    env.insert("PATH".into(), tools.into());
    let prepared = prepare(RunRequest::new(
        Policy::from_toml("[network]\nmode='none'\n").unwrap(),
        CommandSpec::new("/bin/sh".into())
            .arg("-c".into())
            .arg("touch must-not-run".into()),
        HostContext::new(before.cwd().into(), env).unwrap(),
        StdioSpec {
            stdin: Io::Null,
            stdout: Io::Null,
            stderr: Io::Null,
        },
    ))
    .unwrap();
    let error = prepared.spawn().unwrap_err();
    assert_eq!(error.kind, kakoi_runtime::ErrorKind::UnsupportedEnvironment);
    assert_eq!(error.main, kakoi_runtime::MainOutcome::NotStarted);
    assert_eq!(error.cleanup, kakoi_runtime::Cleanup::Confirmed);
    assert!(!std::path::Path::new("must-not-run").exists());
    assert!(!std::path::Path::new("helper-must-not-run").exists());
    let real = std::env::split_paths(
        before
            .environment()
            .get(std::ffi::OsStr::new("PATH"))
            .unwrap(),
    )
    .map(|path| path.join("bwrap"))
    .find(|path| path.is_file())
    .unwrap();
    let real = real.to_str().unwrap().replace('\'', "'\\''");
    std::fs::write(&bwrap, format!("#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'bubblewrap 0.1.0'; exit 0; fi\nexec '{real}' \"$@\"\n")).unwrap();
    let mut env = before.environment().clone();
    env.insert("PATH".into(), bwrap.parent().unwrap().into());
    let running = prepare(RunRequest::new(
        Policy::from_toml("[network]\nmode='none'\n").unwrap(),
        CommandSpec::new("/bin/true".into()),
        HostContext::new(before.cwd().into(), env).unwrap(),
        StdioSpec {
            stdin: Io::Null,
            stdout: Io::Null,
            stderr: Io::Null,
        },
    ))
    .unwrap()
    .spawn()
    .unwrap();
    assert_eq!(running.wait().main, kakoi_runtime::MainOutcome::Exited(0));
}

fn self_test_api_nested() {
    let cwd = std::env::current_dir().unwrap();
    let network = if std::env::var_os("KAKOI_TEST_FILTERED_NESTED").is_some() {
        "mode='filtered'\nallow-nested-filtered=true"
    } else {
        "mode='none'"
    };
    let policy = Policy::from_toml(&format!("[mounts]\nhide=[{:?}]\n[network]\n{network}\n[[commands.guard]]\nprogram='tool'\ndeny=[['forbidden']]\nreason='outer restriction'\n",cwd.join("secret").to_str().unwrap())).unwrap();
    let running = prepare(RunRequest::new(
        policy,
        CommandSpec::new(std::env::current_exe().unwrap().into())
            .arg("--self-test-api-inner".into()),
        HostContext::capture().unwrap(),
        StdioSpec {
            stdin: Io::Null,
            stdout: Io::Inherit,
            stderr: Io::Inherit,
        },
    ))
    .unwrap()
    .spawn()
    .unwrap();
    assert_eq!(running.wait().main, kakoi_runtime::MainOutcome::Exited(0));
}

fn self_test_api_inner() {
    use std::io::Read;
    let context = HostContext::capture().unwrap();
    let outer_ns = std::fs::read_link("/proc/self/ns/mnt").unwrap();
    let inner_network = if std::env::var_os("KAKOI_TEST_FILTERED_NESTED").is_some() {
        "host"
    } else {
        "none"
    };
    let policy = Policy::from_toml(&format!("[network]\nmode='{inner_network}'\n")).unwrap();
    let mut running = prepare(RunRequest::new(
        policy.clone(),
        CommandSpec::new("/bin/sh".into())
            .arg("-c".into())
            .arg("readlink /proc/self/ns/mnt; cat secret; tool forbidden".into()),
        context.clone(),
        StdioSpec {
            stdin: Io::Null,
            stdout: Io::Pipe,
            stderr: Io::Null,
        },
    ))
    .unwrap()
    .spawn()
    .unwrap();
    let mut text = String::new();
    running
        .take_stdout()
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    assert_eq!(running.wait().main, kakoi_runtime::MainOutcome::Exited(126));
    assert_ne!(text.trim(), outer_ns.to_str().unwrap());
    assert_eq!(
        text.lines().count(),
        1,
        "outer hidden secret became visible: {text}"
    );
    if inner_network == "host" {
        let outer_net = std::fs::read_link("/proc/self/ns/net").unwrap();
        let nested_filtered = Policy::from_toml("[network]\nmode='filtered'\n").unwrap();
        let mut inner = prepare(RunRequest::new(
            nested_filtered,
            CommandSpec::new("/usr/bin/readlink".into()).arg("/proc/self/ns/net".into()),
            context.clone(),
            StdioSpec {
                stdin: Io::Null,
                stdout: Io::Pipe,
                stderr: Io::Inherit,
            },
        ))
        .unwrap()
        .spawn()
        .unwrap();
        let mut net = String::new();
        inner
            .take_stdout()
            .unwrap()
            .read_to_string(&mut net)
            .unwrap();
        assert_ne!(net.trim(), outer_net.to_str().unwrap());
        assert_eq!(inner.wait().main, kakoi_runtime::MainOutcome::Exited(0));
        assert_eq!(
            inner.wait().network,
            kakoi_runtime::NetworkCleanup::ConfirmedBlocked
        );
    }
    let duplicate=Policy::from_toml("[network]\nmode='none'\n[[commands.guard]]\nprogram='tool'\ndeny=[['forbidden']]\nreason='duplicate'\n").unwrap();
    assert!(prepare(RunRequest::new(
        duplicate,
        CommandSpec::new("/bin/true".into()),
        context.clone(),
        StdioSpec {
            stdin: Io::Null,
            stdout: Io::Null,
            stderr: Io::Null
        }
    ))
    .is_err());
    let mut env = context.environment().clone();
    env.insert(
        "PATH".into(),
        format!(
            "{}:{}",
            context.cwd().join("failing").display(),
            env.get(std::ffi::OsStr::new("PATH"))
                .unwrap()
                .to_str()
                .unwrap()
        )
        .into(),
    );
    let prepared = prepare(RunRequest::new(
        policy,
        CommandSpec::new("/bin/sh".into())
            .arg("-c".into())
            .arg("printf ran > escaped".into()),
        HostContext::new(context.cwd().into(), env).unwrap(),
        StdioSpec {
            stdin: Io::Null,
            stdout: Io::Null,
            stderr: Io::Null,
        },
    ))
    .unwrap();
    assert!(prepared.spawn().is_err());
    assert!(!context.cwd().join("escaped").exists());
}

fn self_test_filtered_owner() {
    use std::io::{BufRead, BufReader};
    let policy = Policy::from_toml("[network]\nmode='filtered'\n[[network.publish]]\nmode='fixed'\nprotocol='tcp'\nhost-port=23451\nport=23452\n").unwrap();
    let script = "import socket\ns=socket.socket();s.bind(('0.0.0.0',23452));s.listen()\nprint('ready',flush=True)\nwhile True:\n c,_=s.accept();c.sendall(b'published');c.close()\n";
    let mut running = prepare(RunRequest::new(
        policy,
        CommandSpec::new("/usr/bin/python3".into())
            .arg("-c".into())
            .arg(script.into()),
        HostContext::capture().unwrap(),
        StdioSpec {
            stdin: Io::Null,
            stdout: Io::Pipe,
            stderr: Io::Inherit,
        },
    ))
    .unwrap()
    .spawn()
    .unwrap();
    let mut line = String::new();
    BufReader::new(running.take_stdout().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert_eq!(line.trim(), "ready");
    println!("ready");
    std::io::Write::flush(&mut std::io::stdout()).unwrap();
    line.clear();
    std::io::stdin().read_line(&mut line).unwrap();
    running.request_stop().unwrap();
    let outcome = running.wait();
    if std::env::var_os("KAKOI_TEST_CLOSURE_FAILURE").is_some() {
        assert_eq!(outcome.network, kakoi_runtime::NetworkCleanup::Unconfirmed);
        assert_eq!(
            outcome.reason,
            kakoi_runtime::ExitReason::InfrastructureFailure
        );
        assert_eq!(
            outcome.processes,
            kakoi_runtime::ProcessCleanup::Unconfirmed
        );
        assert!(!outcome.diagnostics.is_empty());
    } else {
        assert_eq!(
            outcome.network,
            kakoi_runtime::NetworkCleanup::ConfirmedBlocked
        );
        assert_eq!(
            outcome.processes,
            kakoi_runtime::ProcessCleanup::ConfirmedReaped
        );
    }
}

fn self_test_filtered() {
    use std::io::{BufRead, BufReader, Read};
    signal_state::install_application_state();
    let before_signals = signal_state::Signals::capture();
    let before_raw = signal_state::raw_state();
    let context = HostContext::capture().unwrap();
    let ambient_path = std::env::var_os("PATH").unwrap();
    std::env::set_var("PATH", "/missing-ambient-tools");
    let policy = Policy::from_toml("[network]\nmode='filtered'\n[[network.allow]]\ndestination={host-loopback='ipv4'}\nprotocol='tcp'\nports=['23450']\n[[network.publish]]\nmode='fixed'\nprotocol='tcp'\nhost-port=23451\nport=23452\n").unwrap();
    let script = r#"import socket,signal,time
s=socket.create_connection(('169.254.1.2',23450),20)
print(s.recv(64).decode(),flush=True)
server=socket.socket();server.bind(('0.0.0.0',23452));server.listen()
def term(*_):
 import sys
 sys.stdin.readline()
 try:
  probe=socket.create_connection(('169.254.1.2',23450),1)
  raise AssertionError('network remains open during termination')
 except OSError: pass
 print('blocked',flush=True)
 raise SystemExit(23)
signal.signal(signal.SIGTERM,term)
print('ready',flush=True)
while True: time.sleep(60)
"#;
    let mut running = prepare(RunRequest::new(
        policy,
        CommandSpec::new("/usr/bin/python3".into())
            .arg("-c".into())
            .arg(script.into()),
        context,
        StdioSpec {
            stdin: Io::Pipe,
            stdout: Io::Pipe,
            stderr: Io::Inherit,
        },
    ))
    .unwrap()
    .spawn()
    .unwrap();
    std::env::set_var("PATH", ambient_path);
    let mut output = BufReader::new(running.take_stdout().unwrap());
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "permitted");
    print!("{line}");
    line.clear();
    output.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "ready");
    assert_eq!(
        running.request_stop().unwrap(),
        kakoi_runtime::StopReceipt::Queued
    );
    assert!(running.outcome().is_none());
    std::io::Write::write_all(&mut running.take_stdin().unwrap(), b"finish\n").unwrap();
    let outcome = running.wait();
    assert_eq!(outcome.main, kakoi_runtime::MainOutcome::Exited(23));
    assert_eq!(
        outcome.network,
        kakoi_runtime::NetworkCleanup::ConfirmedBlocked
    );
    assert_eq!(
        outcome.processes,
        kakoi_runtime::ProcessCleanup::ConfirmedReaped
    );
    let mut rest = String::new();
    output.read_to_string(&mut rest).unwrap();
    assert_eq!(rest.trim(), "blocked");
    before_signals.assert_preserved();
    signal_state::report(before_raw, before_signals.usable);
    let launch = |published: bool, script: &str| {
        let policy = Policy::from_toml(if published {
            "[network]\nmode='filtered'\n[[network.publish]]\nmode='fixed'\nprotocol='tcp'\nhost-port=23451\nport=23452\n"
        } else {"[network]\nmode='filtered'\n"}).unwrap();
        prepare(RunRequest::new(
            policy,
            CommandSpec::new("/usr/bin/python3".into())
                .arg("-c".into())
                .arg(script.into()),
            HostContext::capture().unwrap(),
            StdioSpec {
                stdin: Io::Pipe,
                stdout: Io::Pipe,
                stderr: Io::Inherit,
            },
        ))
        .unwrap()
        .spawn()
    };
    let mut first = launch(
        true,
        "import sys\nprint('ready',flush=True)\nprint(sys.stdin.readline(),flush=True)",
    )
    .unwrap();
    let mut reader = BufReader::new(first.take_stdout().unwrap());
    line.clear();
    reader.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "ready");
    let conflict = launch(true, "raise AssertionError('conflicting command ran')").unwrap_err();
    assert_eq!(conflict.kind, kakoi_runtime::ErrorKind::ResourceConflict);
    assert_eq!(conflict.diagnostics[0].os_error, Some(98));
    assert_eq!(conflict.main, kakoi_runtime::MainOutcome::NotStarted);
    let mut second = launch(
        false,
        "import sys\nprint('ready',flush=True)\nprint(sys.stdin.readline(),flush=True)",
    )
    .unwrap();
    let mut second_reader = BufReader::new(second.take_stdout().unwrap());
    line.clear();
    second_reader.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "ready");
    first.request_stop().unwrap();
    assert_eq!(
        first.wait().network,
        kakoi_runtime::NetworkCleanup::ConfirmedBlocked
    );
    assert!(second.outcome().is_none());
    use std::io::Write;
    second
        .take_stdin()
        .unwrap()
        .write_all(b"still-running\n")
        .unwrap();
    line.clear();
    second_reader.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "still-running");
    let natural = second.wait();
    assert_eq!(natural.main, kakoi_runtime::MainOutcome::Exited(0));
    assert_eq!(
        natural.network,
        kakoi_runtime::NetworkCleanup::ConfirmedBlocked
    );
    assert_eq!(
        natural.processes,
        kakoi_runtime::ProcessCleanup::ConfirmedReaped
    );
    let mut dropped = launch(
        true,
        "import sys\nprint('ready',flush=True)\nsys.stdin.read()",
    )
    .unwrap();
    let mut dropped_reader = BufReader::new(dropped.take_stdout().unwrap());
    line.clear();
    dropped_reader.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "ready");
    let handle = dropped.stop_handle();
    drop(dropped);
    let mut rest = String::new();
    dropped_reader.read_to_string(&mut rest).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while handle.request_stop().unwrap() != kakoi_runtime::StopReceipt::AlreadyFinished {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
}

fn self_test_raw_mounts() {
    use kakoi_runtime::{
        EnvMode, EnvironmentPolicy, MountPolicy, NetworkPolicy, PolicyInput, PolicyPath,
    };
    use std::io::Read;
    let context = HostContext::capture().unwrap();
    let first = context
        .cwd()
        .join(OsString::from_vec(b"source-\xff".to_vec()));
    let second = context
        .cwd()
        .join(OsString::from_vec(b"source-\xfe".to_vec()));
    std::fs::write(&first, b"first").unwrap();
    std::fs::write(&second, b"second").unwrap();
    let mut mounts = MountPolicy::new(ListMode::Host);
    mounts.rw_file = vec![
        PolicyPath::from(first.clone()),
        PolicyPath::from(second.clone()),
    ];
    let policy = Policy::validate(PolicyInput::new(
        mounts,
        NetworkPolicy::new(NetworkMode::None),
        EnvironmentPolicy::new(EnvMode::Inherit),
    ))
    .unwrap();
    let prepared = prepare(RunRequest::new(
        policy,
        CommandSpec::new("/bin/cat".into())
            .arg(first.into())
            .arg(second.into()),
        context,
        StdioSpec {
            stdin: Io::Null,
            stdout: Io::Pipe,
            stderr: Io::Null,
        },
    ))
    .unwrap();
    let mut running = prepared.spawn().unwrap();
    let mut output = Vec::new();
    running
        .take_stdout()
        .unwrap()
        .read_to_end(&mut output)
        .unwrap();
    assert_eq!(output, b"firstsecond");
    assert_eq!(running.wait().main, kakoi_runtime::MainOutcome::Exited(0));
}

fn self_test_shared_files() {
    use std::io::Read;
    use std::os::unix::fs::PermissionsExt;
    let before = HostContext::capture().unwrap();
    std::fs::write("hidden-source", b"must be hidden").unwrap();
    let runtime = before.cwd().join("runtime");
    std::fs::create_dir(&runtime).unwrap();
    let mut env = before.environment().clone();
    env.insert("XDG_RUNTIME_DIR".into(), runtime.clone().into());
    let policy = Policy::from_toml(&format!(
        "[mounts]\nhide=[{:?}]\n[network]\nmode='none'\n",
        before.cwd().join("hidden-source").to_str().unwrap()
    ))
    .unwrap();
    let prepared = prepare(RunRequest::new(
        policy,
        CommandSpec::new("/bin/cat".into()).arg("hidden-source".into()),
        HostContext::new(before.cwd().into(), env).unwrap(),
        StdioSpec {
            stdin: Io::Null,
            stdout: Io::Pipe,
            stderr: Io::Null,
        },
    ))
    .unwrap();
    assert!(!runtime.join("kakoi").exists());
    let mut running = prepared.spawn().unwrap();
    let mut bytes = Vec::new();
    running
        .take_stdout()
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.is_empty());
    assert_eq!(running.wait().main, kakoi_runtime::MainOutcome::Exited(0));
    assert_eq!(std::fs::read_dir(runtime.join("kakoi")).unwrap().count(), 1);
    assert!(std::fs::read(runtime.join("kakoi/empty"))
        .unwrap()
        .is_empty());
    assert_eq!(HostContext::capture().unwrap(), before);
    let real_bwrap = std::env::split_paths(
        before
            .environment()
            .get(std::ffi::OsStr::new("PATH"))
            .unwrap(),
    )
    .map(|path| path.join("bwrap"))
    .find(|path| path.is_file())
    .unwrap();
    let tools = before.cwd().join("shared-race-tools");
    std::fs::create_dir(&tools).unwrap();
    let wrapper = tools.join("bwrap");
    let quote =
        |path: &std::path::Path| format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"));
    std::fs::write(&wrapper, format!("#!/bin/sh\nif [ \"$1\" != --help ]; then /bin/mv {} {}; printf replaced > {}; fi\nexec {} \"$@\"\n", quote(&runtime.join("kakoi/empty")), quote(&runtime.join("kakoi/old-empty")), quote(&runtime.join("kakoi/empty")), quote(&real_bwrap))).unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut env = before.environment().clone();
    env.insert("XDG_RUNTIME_DIR".into(), runtime.clone().into());
    let mut path = tools.into_os_string();
    path.push(":");
    path.push(env.get(std::ffi::OsStr::new("PATH")).unwrap());
    env.insert("PATH".into(), path);
    let mut running = prepare(RunRequest::new(
        Policy::from_toml(&format!(
            "[mounts]\nhide=[{:?}]\n[network]\nmode='none'\n[env]\nmode='inherit'\n",
            before.cwd().join("hidden-source").to_str().unwrap()
        ))
        .unwrap(),
        CommandSpec::new("/bin/cat".into()).arg("hidden-source".into()),
        HostContext::new(before.cwd().into(), env).unwrap(),
        StdioSpec {
            stdin: Io::Null,
            stdout: Io::Pipe,
            stderr: Io::Null,
        },
    ))
    .unwrap()
    .spawn()
    .unwrap();
    let mut bytes = Vec::new();
    running
        .take_stdout()
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(
        bytes.is_empty(),
        "substituted helper content was published: {bytes:?}"
    );
    assert_eq!(running.wait().main, kakoi_runtime::MainOutcome::Exited(0));
    assert_eq!(
        std::fs::read(runtime.join("kakoi/empty")).unwrap(),
        b"replaced"
    );
}

fn self_test_live_device() {
    use std::io::Read;
    use std::os::unix::fs::PermissionsExt;
    let context = HostContext::capture().unwrap();
    std::fs::write("live-source", b"old").unwrap();
    let policy = Policy::from_toml(&format!(
        "[mounts]\nrw-file=[{:?}, {:?}]\n[network]\nmode='none'\n",
        context.cwd().join("live-source").to_str().unwrap(),
        context.cwd().join("device").to_str().unwrap()
    ))
    .unwrap();
    let prepared = prepare(RunRequest::new(
        policy,
        CommandSpec::new("/bin/sh".into())
            .arg("-c".into())
            .arg("test -c device && cat live-source >device && cat live-source".into()),
        context.clone(),
        StdioSpec {
            stdin: Io::Null,
            stdout: Io::Pipe,
            stderr: Io::Null,
        },
    ))
    .unwrap();
    std::fs::write("live-source", b"updated after prepare").unwrap();
    let mut running = prepared.spawn().unwrap();
    let mut output = Vec::new();
    running
        .take_stdout()
        .unwrap()
        .read_to_end(&mut output)
        .unwrap();
    assert_eq!(output, b"updated after prepare");
    assert_eq!(running.wait().main, kakoi_runtime::MainOutcome::Exited(0));
    drop(running);
    let running = prepare(RunRequest::new(
        Policy::from_toml(&format!("[mounts]\nro=[{:?}]\n[network]\nmode='none'\n", context.cwd().join("device").to_str().unwrap())).unwrap(),
        CommandSpec::new("/usr/bin/python3".into()).arg("-c".into()).arg(
            "import os,stat,sys\np=sys.argv[1]\nassert stat.S_ISCHR(os.stat(p).st_mode)\nmounts=[s.split() for s in open('/proc/self/mountinfo')]\nassert any(m[4]==p and 'ro' in m[5].split(',') for m in mounts)".into(),
        ).arg(context.cwd().join("device").into()),
        context.clone(), StdioSpec { stdin: Io::Null, stdout: Io::Null, stderr: Io::Inherit },
    )).unwrap().spawn().unwrap();
    assert_eq!(running.wait().main, kakoi_runtime::MainOutcome::Exited(0));
    drop(running);
    // A private overmount after the worker's checks reproduces the dev-bind race.
    // The real bwrap still performs every mount; init must reject its changed result.
    let tools = context.cwd().join("racing-tools");
    std::fs::create_dir(&tools).unwrap();
    let wrapper = tools.join("bwrap");
    let real = std::env::var("KAKOI_TEST_REAL_BWRAP")
        .unwrap()
        .replace('\'', "'\\''");
    std::fs::write(&wrapper, format!("#!/bin/sh\nif [ \"$1\" != --help ]; then mount --bind /dev/zero device || exit 1; fi\nexec '{real}' \"$@\"\n")).unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut env = context.environment().clone();
    let mut path = tools.into_os_string();
    path.push(":");
    path.push(env.get(std::ffi::OsStr::new("PATH")).unwrap());
    env.insert("PATH".into(), path);
    let policy = Policy::from_toml(&format!(
        "[mounts]\nrw-file=[{:?}]\n[network]\nmode='none'\n",
        context.cwd().join("device").to_str().unwrap()
    ))
    .unwrap();
    let prepared = prepare(RunRequest::new(
        policy,
        CommandSpec::new("/bin/sh".into())
            .arg("-c".into())
            .arg("touch must-not-run".into()),
        HostContext::new(context.cwd().into(), env).unwrap(),
        StdioSpec {
            stdin: Io::Null,
            stdout: Io::Null,
            stderr: Io::Null,
        },
    ))
    .unwrap();
    let error = prepared.spawn().unwrap_err();
    assert_eq!(error.kind, kakoi_runtime::ErrorKind::PlanChanged);
    assert_eq!(error.main, kakoi_runtime::MainOutcome::NotStarted);
    assert_eq!(error.cleanup, kakoi_runtime::Cleanup::Confirmed);
    assert!(!std::path::Path::new("must-not-run").exists());
}

fn self_test_parallel() {
    use std::io::{Read, Write};
    let mut first = prepare(cat_request()).unwrap().spawn().unwrap();
    let mut second = prepare(cat_request()).unwrap().spawn().unwrap();
    let mut first_out = first.take_stdout().unwrap();
    let mut second_out = second.take_stdout().unwrap();
    let mut ready = [0; 5];
    first_out.read_exact(&mut ready).unwrap();
    second_out.read_exact(&mut ready).unwrap();
    first.request_stop().unwrap();
    first.wait();
    assert!(second.outcome().is_none());
    let mut input = second.take_stdin().unwrap();
    input.write_all(b"still running").unwrap();
    let mut echoed = [0; 13];
    second_out.read_exact(&mut echoed).unwrap();
    assert_eq!(&echoed, b"still running");
    drop(input);
    assert_eq!(second.wait().main, kakoi_runtime::MainOutcome::Exited(0));
}

fn self_test_inherit() {
    use std::os::fd::{AsFd, AsRawFd};
    unsafe extern "C" {
        fn dup2(old: i32, new: i32) -> i32;
    }
    std::fs::write("input-original", b"original input").unwrap();
    std::fs::write("input-replaced", b"wrong input").unwrap();
    let original_input = std::fs::File::open("input-original").unwrap();
    let replacement_input = std::fs::File::open("input-replaced").unwrap();
    let original_output = std::fs::File::create("output-original").unwrap();
    let replacement_output = std::fs::File::create("output-replaced").unwrap();
    let saved_input = std::io::stdin().as_fd().try_clone_to_owned().unwrap();
    let saved_output = std::io::stdout().as_fd().try_clone_to_owned().unwrap();
    unsafe {
        assert_eq!(dup2(original_input.as_raw_fd(), 0), 0);
        assert_eq!(dup2(original_output.as_raw_fd(), 1), 1);
    }
    let prepared = prepare(RunRequest::new(
        Policy::from_toml("[network]\nmode='none'\n").unwrap(),
        CommandSpec::new("/bin/cat".into()),
        HostContext::capture().unwrap(),
        StdioSpec {
            stdin: Io::Inherit,
            stdout: Io::Inherit,
            stderr: Io::Null,
        },
    ))
    .unwrap();
    unsafe {
        assert_eq!(dup2(replacement_input.as_raw_fd(), 0), 0);
        assert_eq!(dup2(replacement_output.as_raw_fd(), 1), 1);
    }
    let result = prepared.spawn().unwrap().wait();
    unsafe {
        assert_eq!(dup2(saved_input.as_raw_fd(), 0), 0);
        assert_eq!(dup2(saved_output.as_raw_fd(), 1), 1);
    }
    assert_eq!(result.main, kakoi_runtime::MainOutcome::Exited(0));
    assert_eq!(std::fs::read("output-original").unwrap(), b"original input");
    assert!(std::fs::read("output-replaced").unwrap().is_empty());
}

fn self_test_worker_death() {
    use std::io::Read;
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    let mut running = prepare(cat_request()).unwrap().spawn().unwrap();
    let mut output = running.take_stdout().unwrap();
    let mut ready = [0; 5];
    output.read_exact(&mut ready).unwrap();
    let worker: i32 = children().parse().unwrap();
    assert_eq!(unsafe { kill(worker, 9) }, 0);
    let result = running.wait();
    assert_eq!(result.processes, kakoi_runtime::ProcessCleanup::Unconfirmed);
    assert_eq!(result.network, kakoi_runtime::NetworkCleanup::NotApplicable);
    assert_eq!(
        result.reason,
        kakoi_runtime::ExitReason::InfrastructureFailure
    );
    assert_eq!(result.main, kakoi_runtime::MainOutcome::Unknown);
    assert_eq!(output.read(&mut ready).unwrap(), 0);
    assert!(children().is_empty());
}

fn self_test_main_retention() {
    use std::io::{BufRead, BufReader};
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    let mut running = prepare(RunRequest::new(
        Policy::from_toml("[network]\nmode='none'\n[process]\nshutdown-grace-seconds=30\n").unwrap(),
        CommandSpec::new("/usr/bin/python3".into()).arg("-c".into()).arg(
            "import os,signal\nr,w=os.pipe()\nif os.fork()==0:\n def stopped(s,f):\n  os.write(1,b'cleanup-ready\\n'); signal.signal(signal.SIGTERM,signal.SIG_IGN)\n signal.signal(signal.SIGTERM,stopped)\n os.write(w,b'R')\n while True: signal.pause()\nos.read(r,1)\nos._exit(0)".into(),
        ),
        HostContext::capture().unwrap(),
        StdioSpec { stdin: Io::Null, stdout: Io::Pipe, stderr: Io::Null },
    )).unwrap().spawn().unwrap();
    let mut output = BufReader::new(running.take_stdout().unwrap());
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    assert_eq!(line, "cleanup-ready\n");
    let worker: i32 = children().parse().unwrap();
    assert_eq!(unsafe { kill(worker, 9) }, 0);
    let outcome = running.wait();
    assert_eq!(outcome.main, kakoi_runtime::MainOutcome::Exited(0));
    assert_eq!(
        outcome.reason,
        kakoi_runtime::ExitReason::InfrastructureFailure
    );
    assert_eq!(
        outcome.processes,
        kakoi_runtime::ProcessCleanup::Unconfirmed
    );
    assert!(children().is_empty());
}

fn self_test_external_reap() {
    use std::io::Read;
    unsafe extern "C" {
        fn ptrace(request: usize, pid: usize, address: usize, data: usize) -> isize;
        fn raise(signal: i32) -> i32;
        fn kill(pid: i32, signal: i32) -> i32;
        fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
    }
    assert_eq!(unsafe { ptrace(0, 0, 0, 0) }, 0);
    assert_eq!(unsafe { raise(19) }, 0);
    let mut running = prepare(cat_request()).unwrap().spawn().unwrap();
    let mut ready = [0; 5];
    running
        .take_stdout()
        .unwrap()
        .read_exact(&mut ready)
        .unwrap();
    let worker: i32 = children().parse().unwrap();
    assert_eq!(unsafe { kill(worker, 9) }, 0);
    let mut status = 0;
    assert_eq!(unsafe { waitpid(worker, &mut status, 0) }, worker);
    let outcome = running.wait();
    assert_eq!(
        outcome.processes,
        kakoi_runtime::ProcessCleanup::Unconfirmed
    );
    assert_eq!(
        outcome.reason,
        kakoi_runtime::ExitReason::InfrastructureFailure
    );
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(|record| record.os_error == Some(10)),
        "{outcome:?}"
    );
    assert!(children().is_empty());
}

fn self_test_stopping_failure() {
    use std::io::{BufRead, BufReader};
    use std::os::fd::{FromRawFd, OwnedFd};
    unsafe extern "C" {
        fn syscall(number: i64, ...) -> i64;
        fn shutdown(fd: i32, how: i32) -> i32;
    }
    for fault in ["owner", "control", "both"] {
        let context = HostContext::capture().unwrap();
        let record = context.cwd().join("worker-control-record");
        std::env::set_var("KAKOI_TEST_CONTROL_RECORD", &record);
        let mut running = prepare(RunRequest::new(
        Policy::from_toml("[network]\nmode='none'\n[process]\nshutdown-grace-seconds=2\n").unwrap(),
        CommandSpec::new("/usr/bin/python3".into()).arg("-c".into()).arg(
            "import os,signal\ndef stopped(s,f):\n os.write(1,b'stopping\\n'); signal.signal(signal.SIGTERM,signal.SIG_IGN)\nsignal.signal(signal.SIGTERM,stopped)\nos.write(1,b'ready\\n')\nwhile True: signal.pause()".into(),
        ),
        context,
        StdioSpec { stdin: Io::Null, stdout: Io::Pipe, stderr: Io::Null },
    )).unwrap().spawn().unwrap();
        std::env::remove_var("KAKOI_TEST_CONTROL_RECORD");
        let mut output = BufReader::new(running.take_stdout().unwrap());
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        assert_eq!(line, "ready\n");
        running.request_stop().unwrap();
        line.clear();
        output.read_line(&mut line).unwrap();
        assert_eq!(line, "stopping\n");
        assert_eq!(running.status(), kakoi_runtime::RunStatus::Stopping);
        assert!(running.outcome().is_none());
        let worker: i32 = children().parse().unwrap();
        let channels = std::fs::read_to_string(record)
            .unwrap()
            .split_whitespace()
            .map(|number| number.parse::<i32>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(channels.len(), 2);
        let pidfd = unsafe { syscall(434, worker, 0i32) } as i32;
        assert!(pidfd >= 0, "{}", std::io::Error::last_os_error());
        let _pidfd = unsafe { OwnedFd::from_raw_fd(pidfd) };
        for channel in match fault {
            "owner" => &channels[1..],
            "control" => &channels[..1],
            _ => &channels[..],
        } {
            let duplicate = unsafe { syscall(438, pidfd, *channel, 0i32) } as i32;
            assert!(duplicate >= 0, "{}", std::io::Error::last_os_error());
            let _duplicate = unsafe { OwnedFd::from_raw_fd(duplicate) };
            assert_eq!(unsafe { shutdown(duplicate, 2) }, 0);
        }
        let outcome = running.wait();
        assert_eq!(
            outcome.reason,
            if fault == "owner" {
                kakoi_runtime::ExitReason::OwnerLost
            } else {
                kakoi_runtime::ExitReason::InfrastructureFailure
            },
            "{fault}"
        );
        assert_eq!(
            outcome.processes,
            kakoi_runtime::ProcessCleanup::ConfirmedReaped
        );
        drop(running);
    }
}

fn self_test_independent_supervision() {
    use std::time::{Duration, Instant};
    let running = prepare(null_request()).unwrap().spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while running.outcome().is_none() || !children().is_empty() {
        let other = std::process::Command::new("/bin/true").status().unwrap();
        assert!(other.success());
        assert!(
            Instant::now() < deadline,
            "supervision depended on a runtime wait"
        );
    }
    let outcome = running.outcome().unwrap();
    assert_eq!(outcome.main, kakoi_runtime::MainOutcome::Exited(0));
    assert_eq!(
        outcome.processes,
        kakoi_runtime::ProcessCleanup::ConfirmedReaped
    );
}

fn self_test_owner() {
    use std::io::{Read, Write};
    let mut running = prepare(cat_request()).unwrap().spawn().unwrap();
    let mut ready = [0; 5];
    running
        .take_stdout()
        .unwrap()
        .read_exact(&mut ready)
        .unwrap();
    println!("owner-ready");
    std::io::stdout().flush().unwrap();
    // The caller's process is deliberately killed without any Rust destructor.
    running.wait();
}

fn self_test_preexec_death() {
    unsafe extern "C" {
        fn ptrace(request: usize, pid: usize, address: usize, data: usize) -> isize;
        fn raise(signal: i32) -> i32;
    }
    assert_eq!(unsafe { ptrace(0, 0, 0, 0) }, 0);
    assert_eq!(unsafe { raise(19) }, 0);
    let prepared = prepare(RunRequest::new(
        Policy::from_toml("[network]\nmode='none'\n").unwrap(),
        CommandSpec::new("/bin/sh".into())
            .arg("-c".into())
            .arg("touch must-not-run".into()),
        HostContext::capture().unwrap(),
        StdioSpec {
            stdin: Io::Null,
            stdout: Io::Null,
            stderr: Io::Null,
        },
    ))
    .unwrap();
    let error = prepared.spawn().unwrap_err();
    assert_eq!(error.main, kakoi_runtime::MainOutcome::Unknown);
    assert_eq!(error.cleanup, kakoi_runtime::Cleanup::Confirmed);
    assert!(!std::path::Path::new("must-not-run").exists());
}

fn self_test_lifetime() {
    use kakoi_runtime::{ExitReason, ProcessCleanup, StopReceipt};
    use std::io::Read;
    use std::time::{Duration, Instant};
    let modes: &[&str] = if std::env::var_os("KAKOI_TEST_FILTERED_SCENARIOS").is_some() {
        &["filtered"]
    } else {
        &["host", "none"]
    };
    for mode in modes {
        let policy = Policy::from_toml(&format!(
            "[network]\nmode='{mode}'\n[process]\nshutdown-grace-seconds=1\n"
        ))
        .unwrap();
        let mut running = prepare(RunRequest::new(
            policy.clone(),
            CommandSpec::new("/bin/sh".into())
                .arg("-c".into())
                .arg("printf ready; sleep 1000 & exec /usr/bin/yes".into()),
            HostContext::capture().unwrap(),
            StdioSpec {
                stdin: Io::Null,
                stdout: Io::Pipe,
                stderr: Io::Null,
            },
        ))
        .unwrap()
        .spawn()
        .unwrap();
        let mut pipe = running.take_stdout().unwrap();
        let mut ready = [0; 5];
        pipe.read_exact(&mut ready).unwrap();
        assert_eq!(&ready, b"ready");
        let stop = running.stop_handle();
        assert_eq!(stop.request_stop().unwrap(), StopReceipt::Queued);
        let result = running.wait();
        assert_eq!(result.reason, ExitReason::StopRequested);
        assert_eq!(result.processes, ProcessCleanup::ConfirmedReaped);
        assert_eq!(stop.request_stop().unwrap(), StopReceipt::AlreadyFinished);
        drop(pipe);
        drop(running);

        let mut running = prepare(RunRequest::new(
            policy,
            CommandSpec::new("/bin/sh".into())
                .arg("-c".into())
                .arg("printf ready; exec /bin/cat".into()),
            HostContext::capture().unwrap(),
            StdioSpec {
                stdin: Io::Pipe,
                stdout: Io::Pipe,
                stderr: Io::Null,
            },
        ))
        .unwrap()
        .spawn()
        .unwrap();
        let input = running.take_stdin().unwrap();
        let mut output = running.take_stdout().unwrap();
        output.read_exact(&mut ready).unwrap();
        let stop = running.stop_handle();
        drop(running);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !children().is_empty() {
            assert!(
                Instant::now() < deadline,
                "Drop failed to stop and reap worker"
            );
            std::thread::yield_now();
        }
        assert_eq!(output.read(&mut ready).unwrap(), 0);
        assert_eq!(stop.request_stop().unwrap(), StopReceipt::AlreadyFinished);
        drop(input);
    }
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
    let outcome = prepared.spawn().unwrap().wait();
    assert_eq!(outcome.main, kakoi_runtime::MainOutcome::Exited(0));
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

fn self_test_command_path() {
    let before = HostContext::capture().unwrap();
    let mut env = before.environment().clone();
    let mut path = before.cwd().join("requested-tools").into_os_string();
    path.push(":");
    path.push(env.get(std::ffi::OsStr::new("PATH")).unwrap());
    env.insert("PATH".into(), path);
    let context = HostContext::new(before.cwd().into(), env).unwrap();
    for prepend in [false, true] {
        let extra = if prepend {
            format!(
                "path-prepend=[{:?}]\n",
                before.cwd().join("prepended-tools").to_str().unwrap()
            )
        } else {
            String::new()
        };
        let policy = Policy::from_toml(&format!(
            "[network]\nmode='none'\n[env]\nmode='clear'\n{extra}[env.set]\nPATH={:?}\n",
            before.cwd().join("policy-tools").to_str().unwrap(),
        ))
        .unwrap();
        let prepared = prepare(RunRequest::new(
            policy.clone(),
            CommandSpec::new("tool".into()),
            context.clone(),
            StdioSpec {
                stdin: Io::Null,
                stdout: Io::Null,
                stderr: Io::Null,
            },
        ))
        .unwrap();
        assert_eq!(
            prepared.description().program,
            before.cwd().join(if prepend {
                "prepended-tools/tool"
            } else {
                "policy-tools/tool"
            })
        );
        let outcome = prepared.spawn().unwrap().wait();
        assert_eq!(
            outcome.main,
            kakoi_runtime::MainOutcome::Exited(if prepend { 2 } else { 1 })
        );
        // Policy PATH cannot supply a host dependency missing from HostContext.
        let mut missing = context.environment().clone();
        missing.insert("PATH".into(), before.cwd().join("requested-tools").into());
        let missing = HostContext::new(before.cwd().into(), missing).unwrap();
        let failure = prepare(RunRequest::new(
            policy,
            CommandSpec::new("/bin/true".into()),
            missing,
            StdioSpec {
                stdin: Io::Null,
                stdout: Io::Null,
                stderr: Io::Null,
            },
        ))
        .unwrap_err();
        assert_eq!(
            failure.kind,
            kakoi_runtime::ErrorKind::UnsupportedEnvironment
        );
    }
    assert_eq!(HostContext::capture().unwrap(), before);
}

fn self_test_command_guard_path() {
    let before = HostContext::capture().unwrap();
    let mut env = before.environment().clone();
    let mut path = before.cwd().join("requested-tools").into_os_string();
    path.push(":");
    path.push(env.get(std::ffi::OsStr::new("PATH")).unwrap());
    env.insert("PATH".into(), path);
    let context = HostContext::new(before.cwd().into(), env).unwrap();
    let policy = Policy::from_toml(&format!(
        "[network]\nmode='none'\n[env]\nmode='clear'\npath-prepend=[{:?}]\n[env.set]\nPATH={:?}\n[[commands.guard]]\nprogram='tool'\ndeny=[['blocked']]\nreason='guarded'\n",
        before.cwd().join("prepended-tools").to_str().unwrap(),
        before.cwd().join("policy-tools").to_str().unwrap(),
    )).unwrap();
    let prepared = prepare(RunRequest::new(
        policy,
        CommandSpec::new("tool".into()),
        context,
        StdioSpec {
            stdin: Io::Null,
            stdout: Io::Null,
            stderr: Io::Null,
        },
    ))
    .unwrap();
    assert_eq!(prepared.description().guard_count, 1);
    assert_eq!(
        prepared.description().program,
        PathBuf::from("/dev/kakoi-guard/bin/tool")
    );
    assert_eq!(
        prepared.spawn().unwrap().wait().main,
        kakoi_runtime::MainOutcome::Exited(2)
    );
    let policy = Policy::from_toml(&format!(
        "[network]\nmode='none'\n[env.set]\nPATH={:?}\n[[commands.guard]]\nprogram='tool'\ndeny=[['blocked']]\nreason='guarded'\n",
        before.cwd().join("policy-tools").to_str().unwrap(),
    )).unwrap();
    let run = prepare(RunRequest::new(
        policy,
        CommandSpec::new("tool".into()).arg("blocked".into()),
        before,
        StdioSpec {
            stdin: Io::Null,
            stdout: Io::Null,
            stderr: Io::Null,
        },
    ))
    .unwrap()
    .spawn()
    .unwrap();
    assert_eq!(run.wait().main, kakoi_runtime::MainOutcome::Exited(126));
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
