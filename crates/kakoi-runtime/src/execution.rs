//! Descriptor-mounted init and the worker's per-isolation supervision.

use std::ffi::OsString;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use crate::preparation::IoInput;
use crate::running::{Control, Started};
use crate::{
    ipc, Cleanup, DiagnosticRecord, Dispatch, ErrorKind, ExitReason, MainOutcome, NetworkCleanup,
    Phase, PrepareError, ProcessCleanup, RunOutcome, StartError,
};
use kakoi_linux::retained_mounts::{Identity, RetainedMount};
use kakoi_plan::{
    copies::FileContent,
    plan::{Argument, Plan},
};
use serde::{Deserialize, Serialize};

const INIT: &str = "/dev/kakoi-runtime/init";
const INIT_FD: &str = "KAKOI_RUNTIME_INIT_FD";

#[derive(Serialize, Deserialize)]
struct Check {
    path: Vec<u8>,
    device: u64,
    inode: u64,
    kind: u32,
    rdev: u64,
}
#[derive(Serialize, Deserialize)]
struct DataCheck {
    path: Vec<u8>,
    readonly: bool,
}
#[derive(Serialize, Deserialize)]
struct DataBatch {
    offset: usize,
}
pub(crate) struct SourceCheck {
    path: PathBuf,
    identity: Identity,
}
pub(crate) fn source_checks(
    plan: &Plan,
    mounts: &[RetainedMount],
    cwd: &Path,
) -> io::Result<Vec<SourceCheck>> {
    let mut checks = Vec::new();
    for (source, real) in &plan.source_paths {
        let source = if source.is_absolute() {
            source.to_path_buf()
        } else {
            cwd.join(source)
        };
        let Some(retained) = mounts.iter().find(|mount| &mount.source == real) else {
            continue;
        };
        let identity = Identity::of_fd(&retained.descriptor)?;
        if Identity::of_path(&source)? != identity {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "mount resolution changed during prepare",
            ));
        }
        checks.push(SourceCheck {
            path: source,
            identity,
        });
    }
    Ok(checks)
}
#[derive(Serialize, Deserialize)]
struct InitRequest {
    terminal_blocked: [bool; 4],
    program: Vec<u8>,
    argv0: Vec<u8>,
    arguments: Vec<Vec<u8>>,
    checks: Vec<Check>,
    grace_ms: u64,
    guards: Vec<Vec<u8>>,
    allowed: Option<Vec<Vec<u8>>>,
    data: Vec<DataCheck>,
}
#[derive(Serialize, Deserialize)]
enum InitReply {
    Ready,
    Failed(StartError),
    Main(MainOutcome),
    Finished(RunOutcome),
}

fn record(cause: &io::Error) -> DiagnosticRecord {
    DiagnosticRecord {
        detail: cause.to_string(),
        os_error: cause.raw_os_error(),
    }
}
fn failure(kind: ErrorKind, cause: io::Error, main: MainOutcome) -> StartError {
    StartError {
        phase: Phase::Launch,
        kind,
        diagnostics: vec![record(&cause)],
        cleanup: Cleanup::Unconfirmed,
        main,
    }
}
fn prepare_failure(cause: io::Error) -> PrepareError {
    PrepareError {
        phase: Phase::Execution,
        kind: ErrorKind::Io,
        diagnostics: vec![record(&cause)],
        cleanup: Cleanup::Unconfirmed,
    }
}

pub(crate) fn dispatch_image() -> Result<Option<Dispatch>, PrepareError> {
    let role = match crate::helper_image::role() {
        Ok(role) => role,
        Err(_) => return Ok(Some(Dispatch::Completed(ExitCode::from(126)))),
    };
    if role == Some(crate::helper_image::Role::Guard) {
        return Ok(Some(Dispatch::Completed(
            crate::helper_image::dispatch_guard(),
        )));
    }
    if role.is_none() {
        if std::env::var_os(INIT_FD).is_some() {
            return Err(prepare_failure(io::Error::new(
                io::ErrorKind::InvalidData,
                "init role mismatch",
            )));
        }
        return Ok(None);
    }
    let fd = std::env::var(INIT_FD)
        .ok()
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|fd| *fd >= 3)
        .ok_or_else(|| {
            prepare_failure(io::Error::new(
                io::ErrorKind::InvalidData,
                "missing init control",
            ))
        })?;
    let control = crate::preparation::inherited_socket(fd)?;
    std::env::remove_var(INIT_FD);
    if unsafe { libc::getpid() } != 1 {
        return Err(prepare_failure(io::Error::new(
            io::ErrorKind::InvalidData,
            "init must own a PID namespace",
        )));
    }
    init(&control).map_err(prepare_failure)?;
    Ok(Some(Dispatch::Completed(ExitCode::SUCCESS)))
}

fn poll(socket: &UnixStream, milliseconds: i32) -> io::Result<bool> {
    let mut descriptor = libc::pollfd {
        fd: socket.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    loop {
        let result = unsafe { libc::poll(&mut descriptor, 1, milliseconds) };
        if result >= 0 {
            return Ok(result != 0);
        }
        let cause = io::Error::last_os_error();
        if cause.kind() != io::ErrorKind::Interrupted {
            return Err(cause);
        }
    }
}

fn pipe() -> io::Result<[OwnedFd; 2]> {
    let mut fds = [-1; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { [OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])] })
}

fn checks(plan: &Plan, mounts: &[RetainedMount]) -> io::Result<Vec<Check>> {
    let separator = plan.launch_layout.command_separator.unwrap();
    let mut result = Vec::new();
    for mount in mounts {
        // A later mount of an ancestor intentionally covers an earlier publication.
        let covered = data_covered(
            &plan.arguments[..separator],
            mount.argument_index(),
            &mount.destination,
        )?;
        if covered {
            continue;
        }
        let id = Identity::of_fd(&mount.descriptor)?;
        result.push(Check {
            path: mount.destination.as_os_str().as_bytes().to_vec(),
            device: id.device,
            inode: id.inode,
            kind: id.kind,
            rdev: id.rdev,
        });
    }
    Ok(result)
}

fn data_covered(arguments: &[Argument], index: usize, path: &Path) -> io::Result<bool> {
    Ok(kakoi_linux::bwrap_arguments::operations(arguments)?.into_iter().any(|(later,args)| {
        later > index && (matches!(args, [Argument::Literal(option), _, Argument::Literal(dest)]
            if matches!(option.to_str(),Some("--bind" | "--ro-bind" | "--dev-bind" | "--ro-bind-data" | "--bind-data")) && path.starts_with(Path::new(dest)))
        || matches!(args,[Argument::Literal(option), Argument::Literal(dest)]
            if matches!(option.to_str(),Some("--tmpfs" | "--proc" | "--dev")) && path.starts_with(Path::new(dest))))
    }))
}

fn verify_data(check: &DataCheck, expected: OwnedFd) -> io::Result<()> {
    use std::io::Read;
    let path = PathBuf::from(OsString::from_vec(check.path.clone()));
    let mut actual = std::fs::File::open(&path)?;
    let identity = Identity::of_fd(&actual.try_clone()?.into())?;
    if identity.kind != libc::S_IFREG || Identity::of_path(&path)? != identity {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "generated file identity mismatch",
        ));
    }
    if check.readonly {
        let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        if unsafe { libc::fstatvfs(actual.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { stat.assume_init() }.f_flag & libc::ST_RDONLY == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "generated file is not read-only",
            ));
        }
    }
    let mut expected = std::fs::File::from(expected);
    if actual.metadata()?.len() != expected.metadata()?.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "generated file content changed",
        ));
    }
    let mut left = [0; 8192];
    let mut right = [0; 8192];
    loop {
        let size = expected.read(&mut right)?;
        if size == 0 {
            break;
        }
        actual.read_exact(&mut left[..size])?;
        if left[..size] != right[..size] {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "generated file content changed",
            ));
        }
    }
    Ok(())
}

struct ChildGuard(std::process::Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub(crate) struct Channels<'a> {
    pub control: &'a UnixStream,
    pub owner: &'a UnixStream,
    pub result: &'a UnixStream,
    pub events: std::os::unix::net::UnixDatagram,
}

pub(crate) fn run_worker(
    mut plan: Plan,
    mut mounts: Vec<RetainedMount>,
    sources: Vec<SourceCheck>,
    stdio: [IoInput; 3],
    descriptors: Vec<OwnedFd>,
    host: &std::collections::BTreeMap<OsString, OsString>,
    channels: Channels<'_>,
) -> Result<(), PrepareError> {
    let Channels {
        control,
        owner,
        result,
        events,
    } = channels;
    let mut events = crate::events::Sender::new(events).map_err(prepare_failure)?;
    // This dedicated worker owns only this run's helpers, not caller children.
    if unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) } != 0 {
        return Err(prepare_failure(io::Error::last_os_error()));
    }
    let mut network = if plan.policy.network_mode == crate::NetworkMode::Filtered {
        let prepared = kakoi_net::filtered::Tools::locate(host)
            .map_err(kakoi_net::filtered::PreparationError::from)
            .and_then(|tools| {
                kakoi_net::filtered::check_publications(&plan.policy)?;
                kakoi_net::filtered::prepare_session(&plan.policy, &tools)
            });
        match prepared {
            Ok(session) => Some(session),
            Err(cause) => {
                let mut error = failure(
                    if cause.resource_conflict {
                        ErrorKind::ResourceConflict
                    } else if cause.unsupported_environment {
                        ErrorKind::UnsupportedEnvironment
                    } else {
                        ErrorKind::HelperFailure
                    },
                    io::Error::other(cause.diagnostic.to_string()),
                    MainOutcome::NotStarted,
                );
                error.diagnostics[0].os_error = cause.os_error;
                let mut diagnostics = Vec::new();
                error.cleanup = if drain_network(&mut None, &mut diagnostics) {
                    Cleanup::Confirmed
                } else {
                    Cleanup::Unconfirmed
                };
                error.diagnostics.extend(diagnostics);
                error.phase = Phase::Execution;
                ipc::send(control, &Started::Failed(error), &[]).map_err(prepare_failure)?;
                return Ok(());
            }
        }
    } else {
        None
    };
    let prepared = (|| -> io::Result<_> {
        for source in sources {
            if Identity::of_path(&source.path)
                .map_err(|cause| io::Error::new(io::ErrorKind::InvalidData, cause))?
                != source.identity
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "original mount resolution changed",
                ));
            }
        }
        plan.arguments = kakoi_linux::retained_mounts::settle_shared(&plan.arguments, &mut mounts)?;
        let mut inherited_resolver = None;
        if network.is_some() {
            if let Some(target) = kakoi_linux::launch::network_resolver_target(&plan)
                .map_err(|cause| io::Error::other(cause.to_string()))?
            {
                let index = plan
                    .launch_layout
                    .resolver_destination
                    .ok_or_else(|| io::Error::other("missing managed resolver"))?;
                plan.arguments[index] = Argument::Literal(target.into());
            }
            if plan.nested {
                let index = plan
                    .launch_layout
                    .resolver_destination
                    .ok_or_else(|| io::Error::other("missing managed resolver"))?;
                if let (Argument::CopiedFile(content), Argument::Literal(path)) =
                    (&plan.arguments[index - 1], &plan.arguments[index])
                {
                    let check = DataCheck {
                        path: path.as_bytes().to_vec(),
                        readonly: true,
                    };
                    let expected =
                        kakoi_linux::launch::memory_file("kakoi-resolver", content.bytes())?;
                    if verify_data(&check, expected).is_ok() {
                        // An inherited read-only managed resolver already has the
                        // required bytes. Overmounting bwrap's unlinked data file
                        // cannot work on all supported versions; verify the same
                        // final data below instead of changing its contents.
                        inherited_resolver = Some((check, content.bytes().to_vec()));
                        kakoi_linux::retained_mounts::remove_generated_arguments(
                            &mut plan.arguments,
                            &mut mounts,
                            index - 2..index + 1,
                        )?;
                        plan.launch_layout.resolver_destination = None;
                        kakoi_linux::helper_placement::shift_layout(
                            &mut plan.launch_layout,
                            index + 1,
                            -3,
                        );
                    }
                }
            }
        }
        let checks = checks(&plan, &mounts)?;
        let command = plan.command.as_ref().unwrap();
        let mut request = InitRequest {
            terminal_blocked: kakoi_linux::supervisor_signals::blocked()?,
            program: command.path.as_os_str().as_bytes().to_vec(),
            argv0: command.command.as_bytes().to_vec(),
            arguments: command
                .arguments
                .iter()
                .map(|arg| arg.as_bytes().to_vec())
                .collect(),
            checks,
            grace_ms: u64::from(plan.policy.shutdown_grace_seconds) * 1000,
            guards: plan
                .guards
                .table
                .entries
                .iter()
                .map(|entry| entry.location.clone())
                .collect(),
            allowed: plan.commands.as_ref().map(|limits| {
                limits
                    .allowed
                    .iter()
                    .chain(&limits.relocated)
                    .chain(&limits.outer_guards)
                    .map(|path| path.as_os_str().as_bytes().to_vec())
                    .collect()
            }),
            data: Vec::new(),
        };
        let image = crate::helper_image::copy(crate::helper_image::Role::Init)?;
        kakoi_linux::helper_placement::place_init(&mut plan, FileContent::new(image), INIT);
        let mut expected_data = Vec::new();
        for (index, args) in kakoi_linux::bwrap_arguments::operations(&plan.arguments)? {
            let [Argument::Literal(option), content, Argument::Literal(path)] = args else {
                continue;
            };
            if !matches!(option.to_str(), Some("--ro-bind-data" | "--bind-data")) {
                continue;
            }
            if data_covered(&plan.arguments, index, Path::new(path))? {
                continue;
            }
            let bytes = match content {
                Argument::CopiedFile(content) => content.bytes(),
                Argument::EmptyFile => &[],
                _ => continue,
            };
            request.data.push(DataCheck {
                path: path.as_bytes().to_vec(),
                readonly: option == "--ro-bind-data",
            });
            expected_data.push(kakoi_linux::launch::memory_file("kakoi-check", bytes)?);
        }
        if let Some((check, bytes)) = inherited_resolver {
            request.data.push(check);
            expected_data.push(kakoi_linux::launch::memory_file("kakoi-check", &bytes)?);
        }
        let mut bwrap = match &network {
            Some(session) => {
                kakoi_net::application::prepare_retained(&plan, &mounts, session.namespace())?
            }
            None => kakoi_linux::launch::assemble_retained(&plan, &mounts)?,
        };
        crate::preparation::remove_helper_environment(&mut bwrap.command);
        let (init_control, child_control) = UnixStream::pair()?;
        let fd = child_control.as_raw_fd();
        bwrap.command.env(INIT_FD, fd.to_string());
        let mut pipes = Vec::new();
        let mut slots = [None; 3];
        for (index, input) in stdio.into_iter().enumerate() {
            let endpoint = match input {
                IoInput::Null => Stdio::null(),
                IoInput::Descriptor(number) => Stdio::from(descriptors[number].try_clone()?),
                IoInput::Pipe => {
                    let [reader, writer] = pipe()?;
                    slots[index] = Some(pipes.len());
                    if index == 0 {
                        pipes.push(writer);
                        Stdio::from(reader)
                    } else {
                        pipes.push(reader);
                        Stdio::from(writer)
                    }
                }
            };
            match index {
                0 => {
                    bwrap.command.stdin(endpoint);
                }
                1 => {
                    bwrap.command.stdout(endpoint);
                }
                _ => {
                    bwrap.command.stderr(endpoint);
                }
            }
        }
        let mut inherited = bwrap
            .descriptors
            .iter()
            .map(AsRawFd::as_raw_fd)
            .collect::<Vec<_>>();
        inherited.push(fd);
        unsafe {
            bwrap.command.pre_exec(move || {
                // bwrap is also a supervisor: it must outlive terminal interrupts.
                kakoi_linux::supervisor_signals::block()?;
                kakoi_linux::launch::inherit_only(&inherited)
            });
        }
        let child = bwrap.command.spawn()?;
        drop(child_control);
        Ok((child, init_control, request, expected_data, pipes, slots))
    })();
    let (child, init_control, request, expected_data, pipes, slots) = match prepared {
        Ok(prepared) => prepared,
        Err(cause) => {
            let kind = if cause.kind() == io::ErrorKind::InvalidData {
                ErrorKind::PlanChanged
            } else if cause.kind() == io::ErrorKind::Unsupported {
                ErrorKind::UnsupportedEnvironment
            } else {
                ErrorKind::Io
            };
            let mut error = failure(kind, cause, MainOutcome::NotStarted);
            let mut diagnostics = Vec::new();
            let closed = close_network(&mut network, &mut diagnostics);
            let drained = drain_network(&mut network, &mut diagnostics);
            error.cleanup = if closed != NetworkCleanup::Unconfirmed && drained {
                Cleanup::Confirmed
            } else {
                Cleanup::Unconfirmed
            };
            error.diagnostics.extend(diagnostics);
            ipc::send(control, &Started::Failed(error), &[]).map_err(prepare_failure)?;
            return Ok(());
        }
    };
    let mut child = ChildGuard(child);
    let startup = (|| -> io::Result<InitReply> {
        let watches = [owner.as_raw_fd(), control.as_raw_fd()];
        ipc::send_watched(&init_control, &request, &[], &watches)?;
        for (index, batch) in expected_data.chunks(16).enumerate() {
            ipc::send_watched(
                &init_control,
                &DataBatch { offset: index * 16 },
                batch,
                &watches,
            )?;
        }
        let (reply, fds) = ipc::recv_watched(&init_control, &watches)?;
        if !fds.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected init descriptors",
            ));
        }
        Ok(reply)
    })();
    if !matches!(startup, Ok(InitReply::Ready)) {
        let reported = matches!(startup, Ok(InitReply::Failed(_)));
        let mut error = match startup {
            Ok(InitReply::Failed(error)) => error,
            _ => failure(
                ErrorKind::HelperFailure,
                io::Error::other("init startup not confirmed"),
                MainOutcome::Unknown,
            ),
        };
        let mut diagnostics = Vec::new();
        let closed = close_network(&mut network, &mut diagnostics);
        let _ = init_control.shutdown(std::net::Shutdown::Both);
        if !reported {
            let _ = child.0.kill();
        }
        // A reported pre-release failure is followed by init's orderly descendant wait.
        // On an unreported failure, closing control starts its independent death path.
        let waited = child.0.wait();
        let drained = drain_network(&mut network, &mut diagnostics);
        error.diagnostics.extend(diagnostics);
        error.cleanup = if matches!(waited, Ok(status) if status.success())
            && drained
            && closed != NetworkCleanup::Unconfirmed
        {
            Cleanup::Confirmed
        } else {
            Cleanup::Unconfirmed
        };
        ipc::send(control, &Started::Failed(error), &[]).map_err(prepare_failure)?;
        return Ok(());
    }
    if let Err(cause) =
        ipc::send(control, &Started::Ready, &pipes).and_then(|()| ipc::send(control, &slots, &[]))
    {
        let mut diagnostics = Vec::new();
        close_network(&mut network, &mut diagnostics);
        let _ = init_control.shutdown(std::net::Shutdown::Both);
        let _ = child.0.wait();
        drain_network(&mut network, &mut diagnostics);
        return Err(prepare_failure(cause));
    }
    drop(pipes);
    let mut reason = ExitReason::Completed;
    let mut stopping = false;
    let mut owner_lost = false;
    let mut control_lost = false;
    let mut main = MainOutcome::Unknown;
    let mut network_cleanup = NetworkCleanup::NotApplicable;
    let mut network_diagnostics = Vec::new();
    let outcome = loop {
        events.flush();
        if kakoi_linux::supervisor_signals::take_pending() && !stopping {
            reason = strongest_reason(reason, ExitReason::StopRequested);
            stopping = true;
            events.emit(crate::RunEventKind::Status(crate::RunStatus::Stopping));
            network_cleanup = close_network(&mut network, &mut network_diagnostics);
            if network_cleanup == NetworkCleanup::Unconfirmed {
                let _ = child.0.kill();
            } else {
                let _ = ipc::send(&init_control, &Control::Stop, &[]);
            }
        }
        if !stopping {
            if let Some(session) = &mut network {
                if let Err(cause) = session.poll() {
                    if session.state() == kakoi_net::session::SessionState::Unsafe {
                        network_diagnostics.push(record(&cause));
                        reason = ExitReason::InfrastructureFailure;
                        stopping = true;
                        events.emit(crate::RunEventKind::Status(crate::RunStatus::Stopping));
                        network_cleanup = close_network(&mut network, &mut network_diagnostics);
                        let _ = child.0.kill();
                    }
                }
            }
        }
        if let Some(session) = &mut network {
            while let Some((notification, missed)) = session.take_state_notification() {
                events.next = events.next.saturating_add(missed);
                events.emit(crate::RunEventKind::Network {
                    state: notification.state.map(|state| match state {
                        kakoi_net::notification::NetworkState::Running => {
                            crate::NetworkEventState::Running
                        }
                        kakoi_net::notification::NetworkState::Isolated => {
                            crate::NetworkEventState::Isolated
                        }
                        kakoi_net::notification::NetworkState::Unsafe => {
                            crate::NetworkEventState::Unsafe
                        }
                    }),
                    detail: notification.detail.to_string(),
                    truncated: false,
                });
            }
        }
        let init_ready = poll(&init_control, 20).map_err(prepare_failure)?;
        if !owner_lost && poll(owner, 0).map_err(prepare_failure)? {
            owner_lost = true;
            reason = strongest_reason(reason, ExitReason::OwnerLost);
            if !stopping {
                stopping = true;
                events.emit(crate::RunEventKind::Status(crate::RunStatus::Stopping));
                network_cleanup = close_network(&mut network, &mut network_diagnostics);
                if network_cleanup == NetworkCleanup::Unconfirmed {
                    let _ = child.0.kill();
                } else {
                    let _ = ipc::send(&init_control, &Control::Stop, &[]);
                }
            }
        }
        if !control_lost && poll(control, 0).map_err(prepare_failure)? {
            let observed = match ipc::recv::<Control>(control) {
                Ok((Control::Stop, fds)) if fds.is_empty() => ExitReason::StopRequested,
                _ => {
                    control_lost = true;
                    ExitReason::InfrastructureFailure
                }
            };
            reason = strongest_reason(reason, observed);
            if !stopping {
                stopping = true;
                events.emit(crate::RunEventKind::Status(crate::RunStatus::Stopping));
                network_cleanup = close_network(&mut network, &mut network_diagnostics);
                if network_cleanup == NetworkCleanup::Unconfirmed {
                    let _ = child.0.kill();
                } else {
                    let _ = ipc::send(&init_control, &Control::Stop, &[]);
                }
            }
        }
        if init_ready {
            match ipc::recv::<InitReply>(&init_control) {
                Ok((InitReply::Main(observed), fds)) if fds.is_empty() => {
                    main = observed;
                    stopping = true;
                    events.emit(crate::RunEventKind::Status(crate::RunStatus::Stopping));
                    let retained =
                        ipc::send(result, &crate::running::ResultUpdate::Main(main), &[])
                            .and_then(|()| ipc::recv::<()>(result));
                    // Retain the observed main independently before a network
                    // close can block or fail; init still waits for release.
                    network_cleanup = close_network(&mut network, &mut network_diagnostics);
                    let (_, fds) = retained.map_err(prepare_failure)?;
                    if !fds.is_empty() {
                        return Err(prepare_failure(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "unexpected result acknowledgement descriptors",
                        )));
                    }
                    if network_cleanup == NetworkCleanup::Unconfirmed {
                        let _ = child.0.kill();
                    } else {
                        ipc::send(&init_control, &Control::MainRetained, &[])
                            .map_err(prepare_failure)?;
                    }
                }
                Ok((InitReply::Finished(mut outcome), fds)) if fds.is_empty() => {
                    outcome.reason = strongest_reason(reason, outcome.reason);
                    break outcome;
                }
                _ => {
                    break RunOutcome {
                        main,
                        reason: ExitReason::InfrastructureFailure,
                        network: NetworkCleanup::NotApplicable,
                        processes: ProcessCleanup::Unconfirmed,
                        diagnostics: vec![DiagnosticRecord {
                            detail: "init result connection lost".into(),
                            os_error: None,
                        }],
                    }
                }
            }
        }
    };
    let waited = child.0.wait().map_err(prepare_failure)?;
    let mut outcome = outcome;
    if network.is_some() {
        if network_cleanup == NetworkCleanup::NotApplicable {
            network_cleanup = close_network(&mut network, &mut network_diagnostics);
        }
        if !drain_network(&mut network, &mut network_diagnostics) {
            outcome.processes = ProcessCleanup::Unconfirmed;
        }
    }
    outcome.network = network_cleanup;
    if !network_diagnostics.is_empty() {
        outcome.reason = ExitReason::InfrastructureFailure;
        outcome.diagnostics.extend(network_diagnostics);
    }
    if !waited.success() {
        outcome.processes = ProcessCleanup::Unconfirmed;
        outcome.reason = ExitReason::InfrastructureFailure;
    }
    events.emit(crate::RunEventKind::Status(crate::RunStatus::Finished));
    ipc::send(
        result,
        &crate::running::ResultUpdate::Finished(outcome, events.next),
        &[],
    )
    .map_err(prepare_failure)
}

fn close_network(
    network: &mut Option<kakoi_net::session::Session>,
    diagnostics: &mut Vec<DiagnosticRecord>,
) -> NetworkCleanup {
    match network {
        None => NetworkCleanup::NotApplicable,
        Some(session) => match session.close_until(Instant::now() + Duration::from_secs(2)) {
            Ok(()) => NetworkCleanup::ConfirmedBlocked,
            Err(cause) => {
                diagnostics.push(record(&cause));
                NetworkCleanup::Unconfirmed
            }
        },
    }
}

fn drain_network(
    network: &mut Option<kakoi_net::session::Session>,
    diagnostics: &mut Vec<DiagnosticRecord>,
) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut drained = true;
    if let Some(session) = network.as_mut() {
        while !session.is_drained() && Instant::now() < deadline {
            let _ = session.poll();
            std::thread::sleep(Duration::from_millis(10));
        }
        drained = session.is_drained();
    }
    drop(network.take());
    // Drop ends the existing helper owners; independently confirm that no child
    // or adopted helper descendant remains before claiming process cleanup.
    loop {
        let mut status = 0;
        let pid = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
        if pid > 0 {
            continue;
        }
        let cause = io::Error::last_os_error();
        if pid < 0 && cause.raw_os_error() == Some(libc::ECHILD) {
            return drained;
        }
        if pid < 0 && cause.kind() != io::ErrorKind::Interrupted {
            diagnostics.push(record(&cause));
            return false;
        }
        if Instant::now() >= deadline {
            diagnostics.push(record(&io::Error::new(
                io::ErrorKind::TimedOut,
                "network helper cleanup not confirmed",
            )));
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn strongest_reason(current: ExitReason, observed: ExitReason) -> ExitReason {
    let priority = |reason| match reason {
        ExitReason::Completed => 0,
        ExitReason::StopRequested => 1,
        ExitReason::OwnerLost => 2,
        ExitReason::InfrastructureFailure => 3,
    };
    if priority(observed) > priority(current) {
        observed
    } else {
        current
    }
}

fn init(control: &UnixStream) -> io::Result<()> {
    let (request, fds): (InitRequest, _) = ipc::recv(control)?;
    if !fds.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unexpected init request descriptors",
        ));
    }
    // PID 1's default dispositions survive terminal signals. Probes and the target
    // inherit the original mask, not bwrap's supervisor-only blocking.
    kakoi_linux::supervisor_signals::restore(request.terminal_blocked)?;
    // ELF initializers can create descendants before dispatch. None survive release.
    reap_before_release()?;
    let validation = (|| -> io::Result<()> {
        let mut data = Vec::new();
        while data.len() < request.data.len() {
            let (header, batch): (DataBatch, _) = ipc::recv(control)?;
            if header.offset != data.len()
                || batch.is_empty()
                || batch.len() > request.data.len() - data.len()
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid generated data descriptors",
                ));
            }
            data.extend(batch);
        }
        for (check, expected) in request.data.iter().zip(data) {
            verify_data(check, expected)?;
        }
        for check in &request.checks {
            let path = PathBuf::from(OsString::from_vec(check.path.clone()));
            let expected = Identity {
                device: check.device,
                inode: check.inode,
                kind: check.kind,
                rdev: check.rdev,
            };
            if Identity::of_path(&path)? != expected {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "visible mount identity changed",
                ));
            }
        }
        Ok(())
    })();
    if let Err(cause) = validation {
        return ipc::send(
            control,
            &InitReply::Failed(failure(
                ErrorKind::PlanChanged,
                cause,
                MainOutcome::NotStarted,
            )),
            &[],
        );
    }
    let readiness = (|| -> io::Result<()> {
        if let Some(paths) = &request.allowed {
            let mut allowed = Vec::new();
            for bytes in paths.iter().chain(&request.guards) {
                let path = PathBuf::from(OsString::from_vec(bytes.clone()));
                if let Ok(fd) = kakoi_linux::landlock::open_path(&path) {
                    allowed.push(fd);
                }
            }
            allowed.push(kakoi_linux::landlock::open_path(Path::new(INIT))?);
            allowed.extend(
                kakoi_linux::landlock::open_path(Path::new(
                    kakoi_plan::command_limits::DYNAMIC_LINKER,
                ))
                .ok(),
            );
            kakoi_linux::landlock::restrict_execution(&allowed)?;
        }
        for bytes in &request.guards {
            let path = PathBuf::from(OsString::from_vec(bytes.clone()));
            let confirmed = crate::helper_image::probe(&path);
            reap_before_release()?;
            confirmed?;
        }
        Ok(())
    })();
    if let Err(cause) = readiness {
        return ipc::send(
            control,
            &InitReply::Failed(failure(
                ErrorKind::HelperFailure,
                cause,
                MainOutcome::NotStarted,
            )),
            &[],
        );
    }
    let program = OsString::from_vec(request.program);
    let mut command = Command::new(program);
    crate::preparation::remove_helper_environment(&mut command);
    command.arg0(OsString::from_vec(request.argv0));
    command.args(request.arguments.into_iter().map(OsString::from_vec));
    unsafe {
        command.pre_exec(|| kakoi_linux::launch::inherit_only(&[]));
    }
    let child = match command.spawn() {
        Ok(child) => child,
        Err(cause) => {
            ipc::send(
                control,
                &InitReply::Failed(failure(ErrorKind::Io, cause, MainOutcome::NotStarted)),
                &[],
            )?;
            return Ok(());
        }
    };
    let main_pid = child.id() as i32;
    // waitpid below owns this child and every orphan adopted by namespace PID 1.
    drop(child);
    let early_status = match confirm_exec(control, main_pid) {
        Ok(status) => status,
        Err(error) => {
            reap_before_release()?;
            return ipc::send(control, &InitReply::Failed(error), &[]);
        }
    };
    ipc::send(control, &InitReply::Ready, &[])?;
    let mut main = early_status
        .map(main_status)
        .unwrap_or(MainOutcome::Unknown);
    let mut reason = ExitReason::Completed;
    if early_status.is_some() && retain_main(control, main)? {
        reason = ExitReason::StopRequested;
    }
    let mut stopping: Option<Instant> = early_status.map(|_| Instant::now());
    if stopping.is_some() {
        unsafe {
            libc::kill(-1, libc::SIGTERM);
        }
    }
    let mut killed = false;
    let mut control_lost = false;
    loop {
        loop {
            let mut status = 0;
            let pid = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
            if pid == main_pid {
                main = main_status(status);
                if retain_main(control, main)? {
                    reason = ExitReason::StopRequested;
                }
                if stopping.is_none() {
                    stopping = Some(Instant::now());
                    unsafe {
                        libc::kill(-1, libc::SIGTERM);
                    }
                }
            }
            if pid > 0 {
                continue;
            }
            if pid < 0 {
                let cause = io::Error::last_os_error();
                if cause.raw_os_error() == Some(libc::ECHILD) {
                    return ipc::send(
                        control,
                        &InitReply::Finished(RunOutcome {
                            main,
                            reason,
                            network: NetworkCleanup::NotApplicable,
                            processes: ProcessCleanup::ConfirmedReaped,
                            diagnostics: vec![],
                        }),
                        &[],
                    );
                }
                if cause.kind() != io::ErrorKind::Interrupted {
                    return Err(cause);
                }
            }
            break;
        }
        if !control_lost && poll(control, 20)? {
            let observed = match ipc::recv::<Control>(control) {
                Ok((Control::Stop, fds)) if fds.is_empty() => ExitReason::StopRequested,
                _ => {
                    control_lost = true;
                    ExitReason::OwnerLost
                }
            };
            reason = strongest_reason(reason, observed);
            if stopping.is_none() {
                stopping = Some(Instant::now());
                unsafe {
                    libc::kill(-1, libc::SIGTERM);
                }
            }
        }
        if let Some(start) = stopping {
            if !killed && start.elapsed() >= Duration::from_millis(request.grace_ms) {
                unsafe {
                    libc::kill(-1, libc::SIGKILL);
                }
                killed = true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn retain_main(control: &UnixStream, main: MainOutcome) -> io::Result<bool> {
    // Acknowledgement comes only after the caller's independent reaper retained the fact.
    // Descendant shutdown may fail after this point without erasing a known main result.
    ipc::send(control, &InitReply::Main(main), &[])?;
    let mut stopped = false;
    loop {
        let (reply, fds): (Control, _) = ipc::recv(control)?;
        if !fds.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected main acknowledgement descriptors",
            ));
        }
        match reply {
            Control::MainRetained => return Ok(stopped),
            Control::Stop => stopped = true,
            Control::Start => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unexpected start during execution",
                ))
            }
        }
    }
}

fn main_status(status: i32) -> MainOutcome {
    if libc::WIFEXITED(status) {
        MainOutcome::Exited(libc::WEXITSTATUS(status))
    } else {
        MainOutcome::Signaled(libc::WTERMSIG(status), libc::WCOREDUMP(status))
    }
}

fn confirm_exec(control: &UnixStream, pid: i32) -> Result<Option<i32>, StartError> {
    let init_identity = Identity::of_path(Path::new("/proc/self/exe"))
        .map_err(|cause| failure(ErrorKind::Io, cause, MainOutcome::Unknown))?;
    let executable = PathBuf::from(format!("/proc/{pid}/exe"));
    loop {
        // The inherited init inode proves only fork. A different executable inode
        // proves exec, including a shebang interpreter; error-pipe EOF does not.
        if Identity::of_path(&executable).is_ok_and(|id| id != init_identity) {
            return Ok(None);
        }
        let mut status = 0;
        let waited = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
        if waited == pid {
            if libc::WIFEXITED(status) {
                return Ok(Some(status));
            }
            let mut error = failure(
                ErrorKind::HelperFailure,
                io::Error::other(format!(
                    "command died before exec confirmation: signal {}, core={}",
                    libc::WTERMSIG(status),
                    libc::WCOREDUMP(status)
                )),
                MainOutcome::Unknown,
            );
            error.cleanup = Cleanup::Confirmed;
            return Err(error);
        }
        if waited < 0 {
            let cause = io::Error::last_os_error();
            if cause.kind() != io::ErrorKind::Interrupted {
                return Err(failure(ErrorKind::Io, cause, MainOutcome::Unknown));
            }
        }
        if poll(control, 0).map_err(|cause| failure(ErrorKind::Io, cause, MainOutcome::Unknown))? {
            return Err(failure(
                ErrorKind::Protocol,
                io::Error::new(io::ErrorKind::ConnectionAborted, "startup control lost"),
                MainOutcome::Unknown,
            ));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn reap_before_release() -> io::Result<()> {
    // This function is called only after dispatch proved that we are namespace PID 1.
    unsafe {
        libc::kill(-1, libc::SIGKILL);
    }
    loop {
        let mut status = 0;
        let pid = unsafe { libc::waitpid(-1, &mut status, 0) };
        if pid > 0 {
            continue;
        }
        let cause = io::Error::last_os_error();
        if cause.raw_os_error() == Some(libc::ECHILD) {
            return Ok(());
        }
        if cause.kind() != io::ErrorKind::Interrupted {
            return Err(cause);
        }
    }
}
