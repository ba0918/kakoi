//! Preparation ownership and dispatch before application initialization.

use std::ffi::OsString;
use std::fmt;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::net::{UnixDatagram, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::{ipc, wire_policy, Io, NetworkMode, RunRequest};

mod description;
mod dispatch;
mod error;

use description::DescriptionInput;
pub use description::{MountDescription, PlanDescription, SkippedDescription};
pub(crate) use dispatch::inherited_socket;
pub use dispatch::{dispatch_helper, Dispatch, DispatchError};
use error::{error, io_error};
pub use error::{Cleanup, DiagnosticRecord, ErrorKind, Phase, PrepareError};

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
        crate::execution::INIT_FD,
        crate::helper_image::PROBE,
    ] {
        command.env_remove(key);
    }
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
    terminal_blocked: [bool; 4],
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
/// The reaper of one run: it keeps the events the worker sends, waits for the worker's
/// result, then lets go of the result and control channels, stops the event reader, and
/// reaps the worker, in that order, before it settles the outcome.
fn reap(
    result_state: Arc<crate::running::Shared>,
    reaper: Arc<Mutex<ChildOwner>>,
    result_channel: UnixStream,
    event_channel: UnixDatagram,
    network: crate::NetworkCleanup,
) {
    use std::sync::atomic::{AtomicBool, Ordering};
    let done = Arc::new(AtomicBool::new(false));
    let event_done = done.clone();
    let history = result_state.events.clone();
    let event_reader = std::thread::Builder::new()
        .name("kakoi-events".into())
        .spawn(move || read_events(&event_channel, &history, &event_done));
    let mut main = crate::MainOutcome::Unknown;
    let mut event_count = None;
    let received = loop {
        match ipc::recv::<crate::running::ResultUpdate>(&result_channel) {
            Ok((crate::running::ResultUpdate::Main(observed), fds)) if fds.is_empty() => {
                main = observed;
                let mut state = result_state.state.lock().unwrap_or_else(|e| e.into_inner());
                state.status = crate::RunStatus::Stopping;
                drop(state);
                if let Err(cause) = ipc::send(&result_channel, &(), &[]) {
                    break Err(cause);
                }
            }
            Ok((crate::running::ResultUpdate::Finished(outcome, next), fds)) if fds.is_empty() => {
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
}

/// Keeps every event the worker sends until `done`, then reads what is left without
/// waiting.
fn read_events(
    event_channel: &UnixDatagram,
    history: &crate::events::EventHistory,
    event_done: &std::sync::atomic::AtomicBool,
) {
    use std::sync::atomic::Ordering;
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
}

fn bytes(value: &std::ffi::OsStr) -> Vec<u8> {
    value.as_bytes().to_vec()
}
fn path(bytes: Vec<u8>) -> PathBuf {
    OsString::from_vec(bytes).into()
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
            terminal_blocked: kakoi_linux::supervisor_signals::blocked()
                .map_err(|cause| io_error(Phase::Input, cause))?,
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
        > ipc::MAX_MESSAGE
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
    // SAFETY: descriptor inheritance and sigprocmask are async-signal-safe.
    unsafe {
        command.pre_exec(move || {
            kakoi_linux::supervisor_signals::block()?;
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
        .spawn(move || reap(result_state, reaper, result_channel, event_channel, network))
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
