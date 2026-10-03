//! Per-run result retention and non-owning control handles.

use crate::{Cleanup, DiagnosticRecord, ErrorKind, Phase};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::future::Future;
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, Waker};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MainOutcome {
    Exited(i32),
    Signaled(i32, bool),
    NotStarted,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExitReason {
    Completed,
    StopRequested,
    OwnerLost,
    InfrastructureFailure,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkCleanup {
    ConfirmedBlocked,
    Unconfirmed,
    NotApplicable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessCleanup {
    ConfirmedReaped,
    Unconfirmed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunOutcome {
    pub main: MainOutcome,
    pub reason: ExitReason,
    pub network: NetworkCleanup,
    pub processes: ProcessCleanup,
    pub diagnostics: Vec<DiagnosticRecord>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartError {
    pub phase: Phase,
    pub kind: ErrorKind,
    pub diagnostics: Vec<DiagnosticRecord>,
    pub cleanup: Cleanup,
    pub main: MainOutcome,
}
impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}/{:?}", self.phase, self.kind)
    }
}
impl std::error::Error for StartError {}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunStatus {
    Starting,
    Running,
    Stopping,
    Finished,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReceipt {
    Queued,
    AlreadyRequested,
    AlreadyFinished,
}
#[derive(Debug)]
pub struct ControlError(pub io::Error);
impl std::fmt::Display for ControlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for ControlError {}
#[derive(Serialize, Deserialize)]
pub(crate) enum Control {
    Start,
    Stop,
    MainRetained,
}
#[derive(Serialize, Deserialize)]
pub(crate) enum Started {
    Ready,
    Failed(StartError),
}
#[derive(Serialize, Deserialize)]
pub(crate) enum ResultUpdate {
    Main(MainOutcome),
    Finished(RunOutcome, u64),
}
pub(crate) struct State {
    pub status: RunStatus,
    pub outcome: Option<Arc<RunOutcome>>,
    pub worker_reaped: bool,
    stop_requested: bool,
    waiters: BTreeMap<u64, Waker>,
    next_waiter: u64,
}
pub(crate) struct Shared {
    pub events: Arc<crate::events::EventHistory>,
    pub state: Mutex<State>,
    pub changed: Condvar,
    pub control: Mutex<Option<UnixStream>>,
}
impl Shared {
    pub fn new(control: UnixStream) -> Self {
        Self {
            events: Arc::default(),
            state: Mutex::new(State {
                status: RunStatus::Starting,
                outcome: None,
                worker_reaped: false,
                stop_requested: false,
                waiters: BTreeMap::new(),
                next_waiter: 0,
            }),
            changed: Condvar::new(),
            control: Mutex::new(Some(control)),
        }
    }
    pub fn finish(&self, outcome: RunOutcome) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.status = RunStatus::Finished;
        state.outcome = Some(Arc::new(outcome));
        let waiters = std::mem::take(&mut state.waiters);
        drop(state);
        self.changed.notify_all();
        for (_, waker) in waiters {
            waker.wake();
        }
    }
    fn stop(&self) -> Result<StopReceipt, ControlError> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.status == RunStatus::Finished {
            return Ok(StopReceipt::AlreadyFinished);
        }
        if state.stop_requested {
            return Ok(StopReceipt::AlreadyRequested);
        }
        state.stop_requested = true;
        state.status = RunStatus::Stopping;
        let control = self.control.lock().unwrap_or_else(|e| e.into_inner());
        let Some(control) = control.as_ref() else {
            return Ok(StopReceipt::AlreadyFinished);
        };
        crate::ipc::send(control, &Control::Stop, &[]).map_err(|cause| {
            // Shutdown reaches the worker even if a framed request cannot be delivered.
            let _ = control.shutdown(std::net::Shutdown::Both);
            ControlError(cause)
        })?;
        Ok(StopReceipt::Queued)
    }
}
pub struct Running {
    pub(crate) shared: Arc<Shared>,
    pub(crate) owner: UnixStream,
    pub(crate) stdin: Option<PipeWriter>,
    pub(crate) stdout: Option<PipeReader>,
    pub(crate) stderr: Option<PipeReader>,
}
impl std::fmt::Debug for Running {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Running")
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}
impl Running {
    pub fn status(&self) -> RunStatus {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .status
    }
    pub fn outcome(&self) -> Option<Arc<RunOutcome>> {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .outcome
            .clone()
    }
    pub fn request_stop(&self) -> Result<StopReceipt, ControlError> {
        self.shared.stop()
    }
    pub fn stop_handle(&self) -> StopHandle {
        StopHandle {
            shared: self.shared.clone(),
        }
    }
    pub fn events(&self) -> crate::Events {
        self.shared.events.receiver()
    }
    pub fn wait(&self) -> Arc<RunOutcome> {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(outcome) = &state.outcome {
                return outcome.clone();
            }
            state = self
                .shared
                .changed
                .wait(state)
                .unwrap_or_else(|e| e.into_inner());
        }
    }
    pub fn wait_async(&self) -> impl Future<Output = Arc<RunOutcome>> + Send + '_ {
        Wait {
            running: self,
            registration: None,
        }
    }
    pub fn take_stdin(&mut self) -> Option<PipeWriter> {
        self.stdin.take()
    }
    pub fn take_stdout(&mut self) -> Option<PipeReader> {
        self.stdout.take()
    }
    pub fn take_stderr(&mut self) -> Option<PipeReader> {
        self.stderr.take()
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.owner.shutdown(std::net::Shutdown::Both);
    }
}

struct Wait<'a> {
    running: &'a Running,
    registration: Option<u64>,
}
impl Future for Wait<'_> {
    type Output = Arc<RunOutcome>;
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        // Executor callbacks may pause; never run them while holding result retention.
        let waker = context.waker().clone();
        let mut state = this
            .running
            .shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(outcome) = state.outcome.clone() {
            let previous = this
                .registration
                .take()
                .and_then(|id| state.waiters.remove(&id));
            drop(state);
            drop(previous);
            return Poll::Ready(outcome);
        }
        let id = *this.registration.get_or_insert_with(|| {
            let id = state.next_waiter;
            state.next_waiter = state
                .next_waiter
                .checked_add(1)
                .expect("wait registration space exhausted");
            id
        });
        let previous = state.waiters.insert(id, waker);
        drop(state);
        drop(previous);
        Poll::Pending
    }
}
impl Drop for Wait<'_> {
    fn drop(&mut self) {
        if let Some(id) = self.registration.take() {
            let previous = self
                .running
                .shared
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .waiters
                .remove(&id);
            drop(previous);
        }
    }
}
#[derive(Clone)]
pub struct StopHandle {
    shared: Arc<Shared>,
}
impl StopHandle {
    pub fn request_stop(&self) -> Result<StopReceipt, ControlError> {
        self.shared.stop()
    }
}
pub struct PipeReader(pub(crate) std::fs::File);
pub struct PipeWriter(pub(crate) std::fs::File);
impl Read for PipeReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.0.read(buffer)
    }
}
impl Write for PipeWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0.write(buffer)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
impl AsFd for PipeReader {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}
impl AsFd for PipeWriter {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}
impl From<PipeReader> for OwnedFd {
    fn from(value: PipeReader) -> Self {
        value.0.into()
    }
}
impl From<PipeWriter> for OwnedFd {
    fn from(value: PipeWriter) -> Self {
        value.0.into()
    }
}
