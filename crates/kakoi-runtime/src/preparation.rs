//! Preparation ownership and dispatch before application initialization.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::net::{UnixDatagram, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, ExitCode};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::{ipc, wire_policy, HostContext, Io, ListMode, NetworkMode, RunRequest};

const WORKER: &str = "KAKOI_RUNTIME_WORKER_FD";
const OWNER: &str = "KAKOI_RUNTIME_OWNER_FD";
const RESULT: &str = "KAKOI_RUNTIME_RESULT_FD";
const EVENTS: &str = "KAKOI_RUNTIME_EVENTS_FD";
const VERSION: u32 = 1;

pub(crate) fn remove_helper_environment(command: &mut Command) {
    for key in [
        WORKER,
        OWNER,
        RESULT,
        EVENTS,
        "KAKOI_RUNTIME_INIT_FD",
        crate::helper_image::PROBE,
    ] {
        command.env_remove(key);
    }
}

#[derive(Debug)]
pub enum Dispatch {
    Application,
    Completed(ExitCode),
}

pub type DispatchError = PrepareError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Input,
    Worker,
    Planning,
    Retention,
    Launch,
    Execution,
    Cleanup,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorKind {
    InvalidInput,
    UnsupportedEnvironment,
    PlanChanged,
    ResourceConflict,
    HelperFailure,
    Io,
    Protocol,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cleanup {
    NotNeeded,
    Confirmed,
    Unconfirmed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticRecord {
    pub detail: String,
    pub os_error: Option<i32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrepareError {
    pub phase: Phase,
    pub kind: ErrorKind,
    pub diagnostics: Vec<DiagnosticRecord>,
    pub cleanup: Cleanup,
}

impl fmt::Display for PrepareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}/{:?}", self.phase, self.kind)?;
        for diagnostic in &self.diagnostics {
            write!(f, ": {}", diagnostic.detail)?;
        }
        Ok(())
    }
}
impl std::error::Error for PrepareError {}

fn error(phase: Phase, kind: ErrorKind, detail: impl Into<String>) -> PrepareError {
    PrepareError {
        phase,
        kind,
        diagnostics: vec![DiagnosticRecord {
            detail: detail.into(),
            os_error: None,
        }],
        cleanup: Cleanup::NotNeeded,
    }
}
fn io_error(phase: Phase, cause: io::Error) -> PrepareError {
    let mut error = error(phase, ErrorKind::Io, cause.to_string());
    error.diagnostics[0].os_error = cause.raw_os_error();
    error
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountDescription {
    pub path: PathBuf,
    pub permission: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedDescription {
    pub written: String,
    pub reason: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanDescription {
    pub mounts_mode: ListMode,
    pub network_mode: NetworkMode,
    pub environment_mode: crate::EnvMode,
    pub commands_mode: ListMode,
    pub program: PathBuf,
    pub arguments: Vec<OsString>,
    pub cwd: PathBuf,
    pub mounts: Vec<MountDescription>,
    pub skipped: Vec<SkippedDescription>,
    pub guard_count: usize,
    pub required_features: Vec<String>,
    pub diagnostics: Vec<DiagnosticRecord>,
}

pub struct PreparedRun {
    description: PlanDescription,
    control: UnixStream,
    owner: Option<UnixStream>,
    shared: Arc<crate::running::Shared>,
}
impl PreparedRun {
    pub fn description(&self) -> &PlanDescription {
        &self.description
    }
    pub fn spawn(mut self) -> Result<crate::Running, crate::StartError> {
        use crate::running::{Control, Started};
        let failure = |cause: io::Error| crate::StartError {
            phase: Phase::Launch,
            kind: ErrorKind::Protocol,
            diagnostics: vec![DiagnosticRecord {
                detail: cause.to_string(),
                os_error: cause.raw_os_error(),
            }],
            cleanup: Cleanup::Unconfirmed,
            main: crate::MainOutcome::Unknown,
        };
        ipc::send(&self.control, &Control::Start, &[]).map_err(failure)?;
        let (reply, mut fds): (Started, _) = ipc::recv(&self.control).map_err(failure)?;
        match reply {
            Started::Failed(mut error) => {
                let _ = self
                    .owner
                    .as_ref()
                    .unwrap()
                    .shutdown(std::net::Shutdown::Both);
                let _ = self.control.shutdown(std::net::Shutdown::Both);
                let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
                while state.outcome.is_none() {
                    state = self
                        .shared
                        .changed
                        .wait(state)
                        .unwrap_or_else(|e| e.into_inner());
                }
                if !state.worker_reaped {
                    error.cleanup = Cleanup::Unconfirmed;
                }
                Err(error)
            }
            Started::Ready => {
                // Pipe slots carry explicit indices, so unused slots never shift ownership.
                let (slots, extra): ([Option<usize>; 3], _) =
                    ipc::recv(&self.control).map_err(failure)?;
                if !extra.is_empty() || slots.iter().flatten().copied().ne(0..fds.len()) {
                    return Err(failure(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid pipe slots",
                    )));
                }
                let mut take =
                    |slot: Option<usize>| slot.map(|_| std::fs::File::from(fds.remove(0)));
                let stdin = take(slots[0]).map(crate::PipeWriter);
                let stdout = take(slots[1]).map(crate::PipeReader);
                let stderr = take(slots[2]).map(crate::PipeReader);
                let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
                if state.status == crate::RunStatus::Starting {
                    state.status = crate::RunStatus::Running;
                }
                drop(state);
                Ok(crate::Running {
                    shared: self.shared.clone(),
                    owner: self.owner.take().unwrap(),
                    stdin,
                    stdout,
                    stderr,
                })
            }
        }
    }
}
impl fmt::Debug for PreparedRun {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedRun")
            .field("description", &self.description)
            .finish_non_exhaustive()
    }
}
impl Drop for PreparedRun {
    fn drop(&mut self) {
        // Only this owner holds the parent's endpoint; the reaper never clones it.
        if let Some(owner) = &self.owner {
            let _ = owner.shutdown(std::net::Shutdown::Both);
            let _ = self.control.shutdown(std::net::Shutdown::Both);
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) enum IoInput {
    Null,
    Pipe,
    Descriptor(usize),
}
#[derive(Serialize, Deserialize)]
struct RequestInput {
    version: u32,
    policy: Vec<wire_policy::LayerInput>,
    program: Vec<u8>,
    arguments: Vec<Vec<u8>>,
    cwd: Vec<u8>,
    env: Vec<(Vec<u8>, Vec<u8>)>,
    workspace: Option<Vec<u8>>,
    stdio: [IoInput; 3],
}
#[derive(Serialize, Deserialize)]
struct Hello {
    version: u32,
}
#[derive(Serialize, Deserialize)]
enum Response {
    Prepared(Box<DescriptionInput>),
    Failed(PrepareError),
}
#[derive(Serialize, Deserialize)]
struct DescriptionInput {
    mounts_mode: String,
    network_mode: String,
    environment_mode: String,
    commands_mode: String,
    program: Vec<u8>,
    arguments: Vec<Vec<u8>>,
    cwd: Vec<u8>,
    mounts: Vec<(Vec<u8>, String)>,
    skipped: Vec<(String, String)>,
    guard_count: usize,
    required_features: Vec<String>,
    diagnostics: Vec<DiagnosticRecord>,
}

fn bytes(value: &std::ffi::OsStr) -> Vec<u8> {
    value.as_bytes().to_vec()
}
fn path(bytes: Vec<u8>) -> PathBuf {
    OsString::from_vec(bytes).into()
}
fn parse_mode<T: serde::de::DeserializeOwned>(name: String) -> Result<T, PrepareError> {
    serde_json::from_value(serde_json::Value::String(name)).map_err(|_| {
        error(
            Phase::Worker,
            ErrorKind::Protocol,
            "invalid description mode",
        )
    })
}
impl DescriptionInput {
    fn from_plan(plan: &crate::plan::Plan, cwd: &std::path::Path) -> Self {
        let command = plan
            .command
            .as_ref()
            .expect("a library request contains a command");
        let program = if command.path.is_absolute() {
            command.path.clone()
        } else {
            cwd.join(&command.path)
        };
        Self {
            mounts_mode: plan.policy.mounts_mode.name().into(),
            network_mode: plan.policy.network_mode.name().into(),
            environment_mode: plan.policy.env_mode.name().into(),
            commands_mode: plan.policy.commands_mode.name().into(),
            program: bytes(program.as_os_str()),
            arguments: command.arguments.iter().map(|value| bytes(value)).collect(),
            cwd: bytes(cwd.as_os_str()),
            mounts: plan
                .mounts
                .items
                .iter()
                .map(|item| (bytes(item.real.as_os_str()), item.directive.name().into()))
                .collect(),
            skipped: plan
                .skipped_paths
                .iter()
                .map(|item| (item.written.clone(), format!("{:?}", item.reason)))
                .collect(),
            guard_count: plan.guards.placed.len(),
            required_features: vec!["bwrap bind-fd".into(), "bwrap ro-bind-fd".into()],
            diagnostics: plan
                .warnings
                .iter()
                .map(|warning| DiagnosticRecord {
                    detail: warning.to_string(),
                    os_error: None,
                })
                .collect(),
        }
    }
    fn into_public(self) -> Result<PlanDescription, PrepareError> {
        Ok(PlanDescription {
            mounts_mode: parse_mode(self.mounts_mode)?,
            network_mode: parse_mode(self.network_mode)?,
            environment_mode: parse_mode(self.environment_mode)?,
            commands_mode: parse_mode(self.commands_mode)?,
            program: path(self.program),
            arguments: self.arguments.into_iter().map(OsString::from_vec).collect(),
            cwd: path(self.cwd),
            mounts: self
                .mounts
                .into_iter()
                .map(|(bytes, permission)| MountDescription {
                    path: path(bytes),
                    permission,
                })
                .collect(),
            skipped: self
                .skipped
                .into_iter()
                .map(|(written, reason)| SkippedDescription { written, reason })
                .collect(),
            guard_count: self.guard_count,
            required_features: self.required_features,
            diagnostics: self.diagnostics,
        })
    }
}

fn snapshot(io: Io, standard: i32, fds: &mut Vec<OwnedFd>) -> Result<IoInput, PrepareError> {
    let fd = match io {
        Io::Null => return Ok(IoInput::Null),
        Io::Pipe => return Ok(IoInput::Pipe),
        Io::Fd(fd) => fd
            .try_clone()
            .map_err(|cause| io_error(Phase::Input, cause))?,
        Io::Inherit => {
            // SAFETY: fcntl creates a distinct CLOEXEC descriptor owned by the request.
            let fd = unsafe { libc::fcntl(standard, libc::F_DUPFD_CLOEXEC, 3) };
            if fd < 0 {
                return Err(io_error(Phase::Input, io::Error::last_os_error()));
            }
            unsafe { OwnedFd::from_raw_fd(fd) }
        }
    };
    let index = fds.len();
    fds.push(fd);
    Ok(IoInput::Descriptor(index))
}

fn encode_request(request: RunRequest) -> Result<(RequestInput, Vec<OwnedFd>), PrepareError> {
    for value in std::iter::once(&request.command.program).chain(request.command.arguments.iter()) {
        if value.as_bytes().contains(&0) {
            return Err(error(
                Phase::Input,
                ErrorKind::InvalidInput,
                "NUL in command",
            ));
        }
    }
    if request.command.program.is_empty() {
        return Err(error(
            Phase::Input,
            ErrorKind::InvalidInput,
            "empty command",
        ));
    }
    if request
        .workspace
        .as_ref()
        .is_some_and(|path| path.as_os_str().as_bytes().contains(&0))
    {
        return Err(error(
            Phase::Input,
            ErrorKind::InvalidInput,
            "NUL in workspace",
        ));
    }
    let mut fds = Vec::new();
    let stdio = [
        snapshot(request.stdio.stdin, 0, &mut fds)?,
        snapshot(request.stdio.stdout, 1, &mut fds)?,
        snapshot(request.stdio.stderr, 2, &mut fds)?,
    ];
    let cwd = request.context.cwd();
    let workspace = request.workspace.map(|value| {
        if value.is_absolute() {
            value
        } else {
            cwd.join(value)
        }
    });
    Ok((
        RequestInput {
            version: VERSION,
            policy: wire_policy::encode(&request.policy),
            program: bytes(&request.command.program),
            arguments: request
                .command
                .arguments
                .iter()
                .map(|value| bytes(value))
                .collect(),
            cwd: bytes(cwd.as_os_str()),
            env: request
                .context
                .environment()
                .iter()
                .map(|(key, value)| (bytes(key), bytes(value)))
                .collect(),
            workspace: workspace.map(|path| bytes(path.as_os_str())),
            stdio,
        },
        fds,
    ))
}

struct ChildOwner(Option<Child>);
impl ChildOwner {
    fn cleanup(&mut self) -> Cleanup {
        let Some(child) = &mut self.0 else {
            return Cleanup::NotNeeded;
        };
        let _ = child.kill();
        let cleanup = if child.wait().is_ok() {
            Cleanup::Confirmed
        } else {
            Cleanup::Unconfirmed
        };
        self.0 = None;
        cleanup
    }
}
impl Drop for ChildOwner {
    fn drop(&mut self) {
        self.cleanup();
    }
}

pub fn prepare(request: RunRequest) -> Result<PreparedRun, PrepareError> {
    let (input, descriptors) = encode_request(request)?;
    // Enforce the size limit before creating a helper.
    if serde_json::to_vec(&input)
        .map_err(|cause| error(Phase::Input, ErrorKind::Protocol, cause.to_string()))?
        .len()
        > 16 * 1024 * 1024
    {
        return Err(error(
            Phase::Input,
            ErrorKind::InvalidInput,
            "request exceeds 16 MiB",
        ));
    }
    let (control, child_control) =
        UnixStream::pair().map_err(|cause| io_error(Phase::Worker, cause))?;
    let (owner, child_owner) =
        UnixStream::pair().map_err(|cause| io_error(Phase::Worker, cause))?;
    let (result_channel, child_result) =
        UnixStream::pair().map_err(|cause| io_error(Phase::Worker, cause))?;
    let (event_channel, child_events) =
        UnixDatagram::pair().map_err(|cause| io_error(Phase::Worker, cause))?;
    event_channel
        .set_read_timeout(Some(std::time::Duration::from_millis(20)))
        .map_err(|cause| io_error(Phase::Worker, cause))?;
    let control_fd = child_control.as_raw_fd();
    let owner_fd = child_owner.as_raw_fd();
    let result_fd = child_result.as_raw_fd();
    let event_fd = child_events.as_raw_fd();
    let mut command = Command::new("/proc/self/exe");
    command
        .env(WORKER, control_fd.to_string())
        .env(OWNER, owner_fd.to_string())
        .env(RESULT, result_fd.to_string())
        .env(EVENTS, event_fd.to_string());
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // SAFETY: only async-signal-safe fcntl calls occur in the child before exec.
    unsafe {
        command.pre_exec(move || {
            kakoi_linux::launch::inherit_only(&[control_fd, owner_fd, result_fd, event_fd])
        });
    }
    let mut child = ChildOwner(Some(
        command
            .spawn()
            .map_err(|cause| io_error(Phase::Worker, cause))?,
    ));
    drop(child_control);
    drop(child_owner);
    drop(child_result);
    drop(child_events);
    let result = (|| {
        let (hello, fds): (Hello, _) =
            ipc::recv(&control).map_err(|cause| io_error(Phase::Worker, cause))?;
        if hello.version != VERSION || !fds.is_empty() {
            return Err(error(
                Phase::Worker,
                ErrorKind::Protocol,
                "helper version or descriptor count mismatch",
            ));
        }
        ipc::send(&control, &input, &descriptors)
            .map_err(|cause| io_error(Phase::Worker, cause))?;
        let (response, fds): (Response, _) =
            ipc::recv(&control).map_err(|cause| io_error(Phase::Worker, cause))?;
        if !fds.is_empty() {
            return Err(error(
                Phase::Worker,
                ErrorKind::Protocol,
                "unexpected preparation descriptors",
            ));
        }
        match response {
            Response::Prepared(description) => description.into_public(),
            Response::Failed(error) => Err(error),
        }
    })();
    let description = match result {
        Ok(description) => description,
        Err(mut error) => {
            error.cleanup = child.cleanup();
            return Err(error);
        }
    };
    let child = Arc::new(Mutex::new(child));
    let shared = Arc::new(crate::running::Shared::new(
        control
            .try_clone()
            .map_err(|cause| io_error(Phase::Worker, cause))?,
    ));
    let result_state = shared.clone();
    let network = if description.network_mode == NetworkMode::Filtered {
        crate::NetworkCleanup::Unconfirmed
    } else {
        crate::NetworkCleanup::NotApplicable
    };
    let reaper = child.clone();
    if let Err(cause) = std::thread::Builder::new()
        .name("kakoi-reaper".into())
        .spawn(move || {
            use std::sync::atomic::{AtomicBool, Ordering};
            let done = Arc::new(AtomicBool::new(false));
            let event_done = done.clone();
            let history = result_state.events.clone();
            let event_reader = std::thread::Builder::new()
                .name("kakoi-events".into())
                .spawn(move || {
                    let mut buffer = [0; crate::events::MAX_EVENT + 1];
                    loop {
                        if event_done.load(Ordering::Acquire) {
                            let _ = event_channel.set_nonblocking(true);
                        }
                        match event_channel.recv(&mut buffer) {
                            Ok(size) if size <= crate::events::MAX_EVENT => {
                                if let Ok(event) = serde_json::from_slice(&buffer[..size]) {
                                    history.push(event, size);
                                }
                            }
                            Ok(_) => {}
                            Err(cause) if cause.kind() == io::ErrorKind::Interrupted => continue,
                            Err(cause)
                                if matches!(
                                    cause.kind(),
                                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                                ) =>
                            {
                                if event_done.load(Ordering::Acquire) {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                });
            let mut main = crate::MainOutcome::Unknown;
            let mut event_count = None;
            let received = loop {
                match ipc::recv::<crate::running::ResultUpdate>(&result_channel) {
                    Ok((crate::running::ResultUpdate::Main(observed), fds)) if fds.is_empty() => {
                        main = observed;
                        let mut state =
                            result_state.state.lock().unwrap_or_else(|e| e.into_inner());
                        state.status = crate::RunStatus::Stopping;
                        drop(state);
                        if let Err(cause) = ipc::send(&result_channel, &(), &[]) {
                            break Err(cause);
                        }
                    }
                    Ok((crate::running::ResultUpdate::Finished(outcome, next), fds))
                        if fds.is_empty() =>
                    {
                        event_count = Some(next);
                        break Ok(outcome);
                    }
                    Ok(_) => {
                        break Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "unexpected result descriptors",
                        ))
                    }
                    Err(cause) => break Err(cause),
                }
            };
            drop(result_channel);
            result_state
                .control
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take();
            done.store(true, Ordering::Release);
            let events_ok = event_reader.is_ok_and(|reader| reader.join().is_ok());
            let mut owner = reaper.lock().unwrap_or_else(|error| error.into_inner());
            let waited = owner.0.as_mut().map(Child::wait);
            owner.0 = None;
            let mut outcome = match received {
                Ok(outcome) => outcome,
                _ => crate::RunOutcome {
                    main,
                    reason: crate::ExitReason::InfrastructureFailure,
                    network,
                    processes: crate::ProcessCleanup::Unconfirmed,
                    diagnostics: vec![DiagnosticRecord {
                        detail: "worker result connection lost".into(),
                        os_error: None,
                    }],
                },
            };
            if !matches!(&waited, Some(Ok(status)) if status.success()) {
                outcome.processes = crate::ProcessCleanup::Unconfirmed;
                outcome.reason = crate::ExitReason::InfrastructureFailure;
                outcome.diagnostics.push(DiagnosticRecord {
                    detail: match &waited {
                        Some(Err(cause)) if cause.raw_os_error() == Some(libc::ECHILD) => {
                            "worker was reaped externally".into()
                        }
                        _ => "worker wait failed or worker did not exit successfully".into(),
                    },
                    os_error: waited
                        .as_ref()
                        .and_then(|result| result.as_ref().err())
                        .and_then(io::Error::raw_os_error),
                });
            }
            if !events_ok {
                outcome.reason = crate::ExitReason::InfrastructureFailure;
                outcome.diagnostics.push(DiagnosticRecord {
                    detail: "event receiver unavailable".into(),
                    os_error: None,
                });
            }
            result_state
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .worker_reaped = matches!(waited, Some(Ok(_)));
            result_state.finish(outcome);
            result_state.events.close(event_count);
        })
    {
        let mut error = io_error(Phase::Worker, cause);
        error.cleanup = child
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .cleanup();
        return Err(error);
    }
    Ok(PreparedRun {
        description,
        control,
        owner: Some(owner),
        shared,
    })
}

fn descriptor_number(name: &str) -> Result<i32, PrepareError> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|fd| *fd >= 3)
        .ok_or_else(|| {
            error(
                Phase::Worker,
                ErrorKind::Protocol,
                "missing or invalid helper descriptor",
            )
        })
}

pub(crate) fn inherited_socket(fd: i32) -> Result<UnixStream, PrepareError> {
    // Validate the inherited capability before owning it.
    let mut ty: libc::c_int = 0;
    let mut size = std::mem::size_of_val(&ty) as libc::socklen_t;
    // SAFETY: pointers are valid, getsockopt only writes within their stated sizes.
    if unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_TYPE,
            (&mut ty as *mut libc::c_int).cast(),
            &mut size,
        )
    } < 0
        || ty != libc::SOCK_STREAM
    {
        return Err(error(
            Phase::Worker,
            ErrorKind::Protocol,
            "helper descriptor is not a stream socket",
        ));
    }
    // SAFETY: ownership is transferred from the re-executed parent's descriptor table.
    let socket = unsafe { UnixStream::from_raw_fd(fd) };
    if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
        return Err(io_error(Phase::Worker, io::Error::last_os_error()));
    }
    Ok(socket)
}

pub fn dispatch_helper() -> Result<Dispatch, DispatchError> {
    if let Some(dispatch) = crate::execution::dispatch_image()? {
        return Ok(dispatch);
    }
    if std::env::var_os(WORKER).is_none()
        && std::env::var_os(OWNER).is_none()
        && std::env::var_os(RESULT).is_none()
        && std::env::var_os(EVENTS).is_none()
    {
        return Ok(Dispatch::Application);
    }
    let control_fd = descriptor_number(WORKER)?;
    let owner_fd = descriptor_number(OWNER)?;
    let result_fd = descriptor_number(RESULT)?;
    let event_fd = descriptor_number(EVENTS)?;
    let channel_fds = [control_fd, owner_fd, result_fd, event_fd];
    if channel_fds
        .iter()
        .enumerate()
        .any(|(i, fd)| channel_fds[..i].contains(fd))
    {
        return Err(error(
            Phase::Worker,
            ErrorKind::Protocol,
            "helper channels must be distinct",
        ));
    }
    let control = inherited_socket(control_fd)?;
    let owner = inherited_socket(owner_fd)?;
    let result_channel = inherited_socket(result_fd)?;
    let mut ty: libc::c_int = 0;
    let mut size = std::mem::size_of_val(&ty) as libc::socklen_t;
    // SAFETY: getsockopt writes into the initialized type and size only.
    if unsafe {
        libc::getsockopt(
            event_fd,
            libc::SOL_SOCKET,
            libc::SO_TYPE,
            (&mut ty as *mut libc::c_int).cast(),
            &mut size,
        )
    } < 0
        || ty != libc::SOCK_DGRAM
    {
        return Err(error(
            Phase::Worker,
            ErrorKind::Protocol,
            "invalid event descriptor",
        ));
    }
    // SAFETY: the validated, distinct descriptor is inherited exclusively by this worker.
    let event_channel = unsafe { UnixDatagram::from_raw_fd(event_fd) };
    unsafe {
        libc::fcntl(event_fd, libc::F_SETFD, libc::FD_CLOEXEC);
    }
    std::env::remove_var(WORKER);
    std::env::remove_var(OWNER);
    std::env::remove_var(RESULT);
    std::env::remove_var(EVENTS);
    ipc::send(&control, &Hello { version: VERSION }, &[])
        .map_err(|cause| io_error(Phase::Worker, cause))?;
    let result = prepare_worker(&control, &owner, &result_channel, event_channel);
    match result {
        Ok(()) => Ok(Dispatch::Completed(ExitCode::SUCCESS)),
        Err(error) => {
            let _ = ipc::send(&control, &Response::Failed(error.clone()), &[]);
            Err(error)
        }
    }
}

fn prepare_worker(
    control: &UnixStream,
    owner: &UnixStream,
    result_channel: &UnixStream,
    event_channel: UnixDatagram,
) -> Result<(), PrepareError> {
    let (input, descriptors): (RequestInput, _) = ipc::recv_watched(control, &[owner.as_raw_fd()])
        .map_err(|cause| io_error(Phase::Worker, cause))?;
    if input.version != VERSION {
        return Err(error(
            Phase::Worker,
            ErrorKind::Protocol,
            "request version mismatch",
        ));
    }
    let expected = input
        .stdio
        .iter()
        .filter(|entry| matches!(entry, IoInput::Descriptor(_)))
        .count();
    if descriptors.len() != expected {
        return Err(error(
            Phase::Worker,
            ErrorKind::Protocol,
            "stdio descriptor count mismatch",
        ));
    }
    for (index, entry) in input
        .stdio
        .iter()
        .filter_map(|entry| {
            if let IoInput::Descriptor(index) = entry {
                Some(*index)
            } else {
                None
            }
        })
        .enumerate()
    {
        if index != entry {
            return Err(error(
                Phase::Worker,
                ErrorKind::Protocol,
                "stdio descriptor index mismatch",
            ));
        }
    }
    let env: BTreeMap<_, _> = input
        .env
        .into_iter()
        .map(|(key, value)| (OsString::from_vec(key), OsString::from_vec(value)))
        .collect();
    let context = HostContext::new(path(input.cwd), env)
        .map_err(|cause| error(Phase::Input, ErrorKind::InvalidInput, cause.to_string()))?;
    if input.program.contains(&0)
        || input.arguments.iter().any(|argument| argument.contains(&0))
        || input
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.contains(&0))
    {
        return Err(error(
            Phase::Input,
            ErrorKind::InvalidInput,
            "NUL in request",
        ));
    }
    let policy = wire_policy::decode(input.policy)
        .map_err(|cause| error(Phase::Input, ErrorKind::InvalidInput, cause))?;
    std::env::set_current_dir(context.cwd()).map_err(|cause| io_error(Phase::Planning, cause))?;
    kakoi_linux::launch::raise_open_file_limit();
    let mut command = vec![OsString::from_vec(input.program)];
    command.extend(input.arguments.into_iter().map(OsString::from_vec));
    let nested = std::path::Path::new(crate::plan::NESTING_MARK).exists();
    let outer_guard = std::path::Path::new(crate::guard_placement::GUARD_ROOT).exists();
    let request = crate::planning::Request {
        layers: crate::layers::LayerSelection {
            profile: String::new(),
            policy_file: None,
            rw: Vec::new(),
            hide: Vec::new(),
        },
        workspace: input.workspace.map(path),
        command,
        current_dir: context.cwd().into(),
        host: context.environment().clone(),
        executable: Some(PathBuf::from("/proc/self/exe")),
        nested,
        applied: true,
        outer_guard,
        outer_table: if outer_guard {
            std::fs::read(crate::guard_placement::GUARD_TABLE).ok()
        } else {
            None
        },
        landlock_abi: kakoi_linux::landlock::abi_version(),
    };
    let mut plan =
        crate::planning::plan_with_policy(&request, policy.source_layers(), policy.as_merged())
            .map_err(|cause| {
                let io_cause = cause.io_cause();
                let kind = if io_cause.is_some() {
                    ErrorKind::Io
                } else if cause.kind() == crate::diagnostic::Kind::Bwrap {
                    ErrorKind::UnsupportedEnvironment
                } else {
                    ErrorKind::InvalidInput
                };
                let mut failure = error(Phase::Planning, kind, cause.to_string());
                failure.diagnostics[0].os_error = io_cause.and_then(|cause| cause.os_error);
                failure
            })?;
    let guard_image = if plan.guards.table.entries.is_empty() {
        None
    } else {
        Some(kakoi_plan::copies::FileContent::new(
            crate::helper_image::copy(crate::helper_image::Role::Guard)
                .map_err(|cause| io_error(Phase::Retention, cause))?,
        ))
    };
    kakoi_linux::helper_placement::place(&mut plan, guard_image)
        .map_err(|cause| io_error(Phase::Retention, cause))?;
    let separator = plan.launch_layout.command_separator.ok_or_else(|| {
        error(
            Phase::Retention,
            ErrorKind::Protocol,
            "missing command boundary",
        )
    })?;
    let mounts = kakoi_linux::retained_mounts::retain(&plan.arguments[..separator])
        .map_err(|cause| io_error(Phase::Retention, cause))?;
    let sources = crate::execution::source_checks(&plan, &mounts, context.cwd())
        .map_err(|cause| io_error(Phase::Retention, cause))?;
    ipc::send_watched(
        control,
        &Response::Prepared(Box::new(DescriptionInput::from_plan(&plan, context.cwd()))),
        &[],
        &[owner.as_raw_fd()],
    )
    .map_err(|cause| io_error(Phase::Worker, cause))?;
    // The command and network are not started at preparation. Resources stay here
    // until the sole owner's endpoint closes; observer/reaper state cannot keep it alive.
    let mut poll = [
        libc::pollfd {
            fd: owner.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: control.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
    ];
    loop {
        // SAFETY: poll only writes these two initialized entries.
        let result = unsafe { libc::poll(poll.as_mut_ptr(), poll.len() as libc::nfds_t, -1) };
        if result < 0 {
            let cause = io::Error::last_os_error();
            if cause.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(io_error(Phase::Worker, cause));
        }
        if poll[0].revents != 0 {
            return Ok(());
        }
        if poll[1].revents != 0 {
            let (command, fds): (crate::running::Control, _) =
                ipc::recv_watched(control, &[owner.as_raw_fd()])
                    .map_err(|cause| io_error(Phase::Worker, cause))?;
            if !fds.is_empty() {
                return Err(error(
                    Phase::Worker,
                    ErrorKind::Protocol,
                    "unexpected control descriptors",
                ));
            }
            if matches!(command, crate::running::Control::Start) {
                return crate::execution::run_worker(
                    plan,
                    mounts,
                    sources,
                    input.stdio,
                    descriptors,
                    context.environment(),
                    crate::execution::Channels {
                        control,
                        owner,
                        result: result_channel,
                        events: event_channel,
                    },
                );
            }
        }
    }
}
