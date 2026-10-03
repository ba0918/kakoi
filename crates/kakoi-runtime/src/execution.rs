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
pub(crate) struct SourceCheck {
    path: PathBuf,
    identity: Identity,
}
pub(crate) fn place_helper_images(plan: &mut Plan) -> io::Result<()> {
    if plan.commands.is_some() {
        let root = kakoi_plan::command_limits::FIRST_ROOT;
        let start = plan
            .arguments
            .windows(2)
            .position(|args| {
                matches!(args, [Argument::Literal(option), Argument::Literal(path)]
                if option == "--tmpfs" && path == root)
            })
            .and_then(|index| index.checked_sub(2))
            .ok_or_else(|| io::Error::other("missing first-process placement"))?;
        let end = plan
            .arguments
            .windows(2)
            .position(|args| {
                matches!(args, [Argument::Literal(option), Argument::Literal(path)]
                if option == "--remount-ro" && path == root)
            })
            .ok_or_else(|| io::Error::other("missing first-process boundary"))?
            + 2;
        plan.arguments.drain(start..end);
        shift_layout(&mut plan.launch_layout, end, -((end - start) as isize));
    }
    if plan.guards.table.entries.is_empty() {
        return Ok(());
    }
    let guard_image =
        FileContent::new(crate::helper_image::copy(crate::helper_image::Role::Guard)?);
    for entry in &plan.guards.table.entries {
        let index = plan
            .arguments
            .windows(3)
            .position(|args| {
                matches!(args, [Argument::Literal(option), _, Argument::Literal(path)]
                if option == "--ro-bind" && path.as_bytes() == entry.location)
            })
            .ok_or_else(|| io::Error::other("missing guard placement"))?;
        plan.arguments[index] = Argument::Literal("--ro-bind-data".into());
        plan.arguments[index + 1] = Argument::CopiedFile(guard_image.clone());
        plan.arguments.splice(
            index..index,
            [
                Argument::Literal("--perms".into()),
                Argument::Literal("0555".into()),
            ],
        );
        shift_layout(&mut plan.launch_layout, index, 2);
    }
    Ok(())
}
fn shift_layout(layout: &mut kakoi_plan::plan::LaunchLayout, from: usize, delta: isize) {
    for position in [
        &mut layout.argv0,
        &mut layout.command_separator,
        &mut layout.resolver_destination,
    ]
    .into_iter()
    .flatten()
    {
        if *position >= from {
            *position = position
                .checked_add_signed(delta)
                .expect("layout shift stays in bounds");
        }
    }
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
        );
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

fn data_covered(arguments: &[Argument], index: usize, path: &Path) -> bool {
    arguments.windows(3).enumerate().any(|(later,args)| {
        later > index && matches!(args, [Argument::Literal(option), _, Argument::Literal(dest)]
            if matches!(option.to_str(),Some("--bind" | "--ro-bind" | "--dev-bind" | "--ro-bind-data" | "--bind-data")) && path.starts_with(Path::new(dest)))
    }) || arguments.windows(2).enumerate().any(|(later,args)| {
        later > index && matches!(args,[Argument::Literal(option), Argument::Literal(dest)]
            if matches!(option.to_str(),Some("--tmpfs" | "--proc" | "--dev")) && path.starts_with(Path::new(dest)))
    })
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
}

pub(crate) fn run_worker(
    mut plan: Plan,
    mut mounts: Vec<RetainedMount>,
    sources: Vec<SourceCheck>,
    stdio: [IoInput; 3],
    descriptors: Vec<OwnedFd>,
    channels: Channels<'_>,
) -> Result<(), PrepareError> {
    let Channels {
        control,
        owner,
        result,
    } = channels;
    if plan.policy.network_mode == crate::NetworkMode::Filtered {
        let mut error = failure(
            ErrorKind::UnsupportedEnvironment,
            io::Error::new(
                io::ErrorKind::Unsupported,
                "filtered supervision not yet connected",
            ),
            MainOutcome::NotStarted,
        );
        error.cleanup = Cleanup::Confirmed;
        ipc::send(control, &Started::Failed(error), &[]).map_err(prepare_failure)?;
        return Ok(());
    }
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
        let checks = checks(&plan, &mounts)?;
        let command = plan.command.as_ref().unwrap();
        let mut request = InitRequest {
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
        let separator = plan.launch_layout.command_separator.unwrap();
        plan.arguments.truncate(separator);
        if let Some(index) = plan.launch_layout.argv0 {
            plan.arguments[index] = Argument::Literal(INIT.into());
        }
        plan.arguments.extend([
            Argument::Literal("--dir".into()),
            Argument::Literal("/dev/kakoi-runtime".into()),
            Argument::Literal("--perms".into()),
            Argument::Literal("0700".into()),
            Argument::Literal("--ro-bind-data".into()),
            Argument::CopiedFile(FileContent::new(image)),
            Argument::Literal(INIT.into()),
            Argument::Literal("--as-pid-1".into()),
            Argument::Literal("--".into()),
            Argument::Literal(INIT.into()),
        ]);
        let mut expected_data = Vec::new();
        for (index, args) in plan.arguments.windows(3).enumerate() {
            let [Argument::Literal(option), content, Argument::Literal(path)] = args else {
                continue;
            };
            if !matches!(option.to_str(), Some("--ro-bind-data" | "--bind-data")) {
                continue;
            }
            if data_covered(&plan.arguments, index, Path::new(path)) {
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
        let mut bwrap = kakoi_linux::launch::assemble_retained(&plan, &mounts)?;
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
            bwrap
                .command
                .pre_exec(move || kakoi_linux::launch::inherit_only(&inherited));
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
            error.cleanup = Cleanup::Confirmed;
            ipc::send(control, &Started::Failed(error), &[]).map_err(prepare_failure)?;
            return Ok(());
        }
    };
    let mut child = ChildGuard(child);
    let startup = (|| -> io::Result<InitReply> {
        ipc::send(&init_control, &request, &[])?;
        for batch in expected_data.chunks(16) {
            ipc::send(&init_control, &(), batch)?;
        }
        while !poll(&init_control, 20)? {
            if poll(owner, 0)? || poll(control, 0)? {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "owner or control lost during startup",
                ));
            }
        }
        let (reply, fds) = ipc::recv(&init_control)?;
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
        let _ = init_control.shutdown(std::net::Shutdown::Both);
        if !reported {
            let _ = child.0.kill();
        }
        // A reported pre-release failure is followed by init's orderly descendant wait.
        // On an unreported failure, closing control starts its independent death path.
        let waited = child.0.wait();
        error.cleanup = if matches!(waited, Ok(status) if status.success()) {
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
        let _ = init_control.shutdown(std::net::Shutdown::Both);
        let _ = child.0.wait();
        return Err(prepare_failure(cause));
    }
    drop(pipes);
    let mut reason = ExitReason::Completed;
    let mut stopping = false;
    let mut owner_lost = false;
    let mut control_lost = false;
    let mut main = MainOutcome::Unknown;
    let outcome = loop {
        let init_ready = poll(&init_control, 20).map_err(prepare_failure)?;
        if !owner_lost && poll(owner, 0).map_err(prepare_failure)? {
            owner_lost = true;
            reason = strongest_reason(reason, ExitReason::OwnerLost);
            if !stopping {
                stopping = true;
                let _ = ipc::send(&init_control, &Control::Stop, &[]);
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
                let _ = ipc::send(&init_control, &Control::Stop, &[]);
            }
        }
        if init_ready {
            match ipc::recv::<InitReply>(&init_control) {
                Ok((InitReply::Main(observed), fds)) if fds.is_empty() => {
                    main = observed;
                    ipc::send(result, &crate::running::ResultUpdate::Main(main), &[])
                        .map_err(prepare_failure)?;
                    let (_, fds): ((), _) = ipc::recv(result).map_err(prepare_failure)?;
                    if !fds.is_empty() {
                        return Err(prepare_failure(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "unexpected result acknowledgement descriptors",
                        )));
                    }
                    ipc::send(&init_control, &Control::MainRetained, &[])
                        .map_err(prepare_failure)?;
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
    if !waited.success() {
        outcome.processes = ProcessCleanup::Unconfirmed;
        outcome.reason = ExitReason::InfrastructureFailure;
    }
    ipc::send(
        result,
        &crate::running::ResultUpdate::Finished(outcome),
        &[],
    )
    .map_err(prepare_failure)
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
    // ELF initializers can create descendants before dispatch. None survive release.
    reap_before_release()?;
    let validation = (|| -> io::Result<()> {
        let mut data = Vec::new();
        while data.len() < request.data.len() {
            let (_, batch): ((), _) = ipc::recv(control)?;
            if batch.is_empty() || batch.len() > request.data.len() - data.len() {
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
