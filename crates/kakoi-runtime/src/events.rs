//! Finite observation history; neither cursors nor waiters own the live run.
use crate::running::RunStatus;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::io;
use std::os::unix::net::UnixDatagram;
use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, Waker};

const MAX_EVENTS: usize = 256;
const MAX_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_EVENT: usize = 8192;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunEventKind {
    Status(RunStatus),
    Network {
        state: Option<NetworkEventState>,
        detail: String,
        truncated: bool,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkEventState {
    Running,
    Isolated,
    Unsafe,
}

#[cfg(test)]
mod tests {
    use super::*;
    use kakoi_net::notification::{NetworkState, Notifications};
    use std::task::Wake;

    struct Notify(std::sync::mpsc::Sender<()>);
    impl Wake for Notify {
        fn wake(self: Arc<Self>) {
            let _ = self.0.send(());
        }
    }
    fn notices(sender: &mut Sender, count: usize) {
        let mut notifications = Notifications::default();
        for index in 0..count {
            // Watchdog isolation/recovery is the operational producer of these updates.
            notifications.update(
                if index % 2 == 0 {
                    NetworkState::Running
                } else {
                    NetworkState::Isolated
                },
                "transport changed",
            );
            let (detail, missed) = notifications.pop_with_loss().unwrap();
            sender.next += missed;
            sender.emit(RunEventKind::Network {
                state: None,
                detail: detail.to_string(),
                truncated: false,
            });
        }
    }
    fn drain(socket: &UnixDatagram, history: &EventHistory) {
        socket.set_nonblocking(true).unwrap();
        let mut bytes = [0; MAX_EVENT + 1];
        loop {
            match socket.recv(&mut bytes) {
                Ok(size) => history.push(serde_json::from_slice(&bytes[..size]).unwrap(), size),
                Err(cause) if cause.kind() == io::ErrorKind::WouldBlock => break,
                Err(cause) => panic!("{cause}"),
            }
        }
    }

    // @kotowari[REQ-library-302, EX-library-305, EX-library-214]
    #[test]
    fn network_notification_burst_reports_worker_and_parent_loss_with_independent_positions() {
        let (parent, worker) = UnixDatagram::pair().unwrap();
        let mut sender = Sender::new(worker).unwrap();
        let history = Arc::new(EventHistory::default());
        let mut slow = history.receiver();
        let (notify, _) = std::sync::mpsc::channel();
        let waker = Waker::from(Arc::new(Notify(notify)));
        let mut cx = Context::from_waker(&waker);
        let mut canceled = Box::pin(slow.recv_async());
        assert!(canceled.as_mut().poll(&mut cx).is_pending());
        drop(canceled);
        // The real kernel queue fills when the parent event reader is descheduled.
        notices(&mut sender, 2000);
        assert!(sender.pending.len() <= MAX_EVENTS);
        assert!(sender.bytes <= MAX_BYTES);
        assert!(!sender.pending.is_empty(), "kernel queue did not saturate");
        drain(&parent, &history);
        while !sender.pending.is_empty() {
            sender.flush();
            drain(&parent, &history);
        }
        history.close(Some(sender.next));
        let mut total = 0;
        let mut missed = 0;
        loop {
            match slow.recv() {
                EventRead::Event(_) => total += 1,
                EventRead::Lagged { missed: n } => {
                    total += n;
                    missed += n;
                }
                EventRead::Closed => break,
            }
        }
        assert_eq!(total, 2000);
        assert!(missed > 0);
        assert!(matches!(history.receiver().recv(), EventRead::Closed));

        let history = Arc::new(EventHistory::default());
        let mut slow = history.receiver();
        let mut fast = history.receiver();
        let (parent, worker) = UnixDatagram::pair().unwrap();
        let mut sender = Sender::new(worker).unwrap();
        let mut notifications = Notifications::default();
        for _ in 0..600 {
            // A verbose network diagnostic remains bounded after JSON escaping too.
            notifications.notice(&"\"".repeat(3800));
            let (detail, _) = notifications.pop_with_loss().unwrap();
            sender.emit(RunEventKind::Network {
                state: None,
                detail: detail.to_string(),
                truncated: false,
            });
            drain(&parent, &history);
            assert!(matches!(fast.recv(), EventRead::Event(_)));
        }
        let state = history.state.lock().unwrap();
        assert!(state.entries.len() <= MAX_EVENTS);
        assert!(state.bytes <= MAX_BYTES);
        let retained_first = state.entries.front().unwrap().0.sequence;
        drop(state);
        assert!(
            retained_first > 344,
            "byte bound must evict before count bound"
        );
        assert_eq!(
            slow.recv(),
            EventRead::Lagged {
                missed: retained_first
            }
        );
        let EventRead::Event(event) = slow.recv() else {
            panic!("retained history missing")
        };
        assert_eq!(event.sequence, retained_first);
    }

    // @kotowari[REQ-library-205, EX-library-213]
    #[test]
    fn registration_and_delivery_share_a_lock_and_pending_drop_never_advances_cursor() {
        for delivery_before_poll in [false, true] {
            let history = Arc::new(EventHistory::default());
            let mut events = history.receiver();
            let (notify, notified) = std::sync::mpsc::channel();
            let waker = Waker::from(Arc::new(Notify(notify)));
            let mut cx = Context::from_waker(&waker);
            let event = RunEvent {
                sequence: 0,
                kind: RunEventKind::Status(RunStatus::Stopping),
            };
            if delivery_before_poll {
                history.push(event.clone(), 100);
            }
            let mut future = Box::pin(events.recv_async());
            if delivery_before_poll {
                assert_eq!(
                    future.as_mut().poll(&mut cx),
                    Poll::Ready(EventRead::Event(event))
                );
                drop(future);
            } else {
                assert!(future.as_mut().poll(&mut cx).is_pending());
                history.push(event.clone(), 100);
                notified.recv().unwrap();
                drop(future);
                assert_eq!(events.recv(), EventRead::Event(event));
            }
            assert!(history.state.lock().unwrap().waiters.is_empty());
            history.close(Some(1));
            assert_eq!(events.recv(), EventRead::Closed);
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunEvent {
    pub sequence: u64,
    pub kind: RunEventKind,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventRead {
    Event(RunEvent),
    Lagged { missed: u64 },
    Closed,
}
#[derive(Default)]
struct History {
    entries: VecDeque<(RunEvent, usize)>,
    bytes: usize,
    next: u64,
    closed: bool,
    waiters: BTreeMap<u64, Waker>,
    next_waiter: u64,
}
#[derive(Default)]
pub(crate) struct EventHistory {
    state: Mutex<History>,
    changed: Condvar,
}
impl EventHistory {
    pub fn receiver(self: &Arc<Self>) -> Events {
        let cursor = self.state.lock().unwrap_or_else(|e| e.into_inner()).next;
        Events {
            history: self.clone(),
            cursor,
        }
    }
    pub fn push(&self, event: RunEvent, bytes: usize) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if event.sequence < state.next || state.closed {
            return;
        }
        state.next = event.sequence + 1;
        state.bytes += bytes;
        state.entries.push_back((event, bytes));
        while state.entries.len() > MAX_EVENTS || state.bytes > MAX_BYTES {
            state.bytes -= state.entries.pop_front().unwrap().1;
        }
        self.notify(state);
    }
    pub fn close(&self, next: Option<u64>) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(next) = next {
            state.next = state.next.max(next);
        }
        state.closed = true;
        self.notify(state);
    }
    fn notify(&self, mut state: std::sync::MutexGuard<'_, History>) {
        let waiters = std::mem::take(&mut state.waiters);
        drop(state);
        self.changed.notify_all();
        for (_, waker) in waiters {
            waker.wake();
        }
    }
}
pub struct Events {
    history: Arc<EventHistory>,
    cursor: u64,
}
fn read(state: &History, cursor: &mut u64) -> Option<EventRead> {
    if let Some((event, _)) = state
        .entries
        .iter()
        .find(|(event, _)| event.sequence >= *cursor)
    {
        if event.sequence > *cursor {
            let missed = event.sequence - *cursor;
            *cursor = event.sequence;
            return Some(EventRead::Lagged { missed });
        }
        *cursor += 1;
        return Some(EventRead::Event(event.clone()));
    }
    if state.next > *cursor {
        let missed = state.next - *cursor;
        *cursor = state.next;
        return Some(EventRead::Lagged { missed });
    }
    state.closed.then_some(EventRead::Closed)
}
impl Events {
    pub fn recv(&mut self) -> EventRead {
        let mut state = self.history.state.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(event) = read(&state, &mut self.cursor) {
                return event;
            }
            state = self
                .history
                .changed
                .wait(state)
                .unwrap_or_else(|e| e.into_inner());
        }
    }
    pub fn recv_async(&mut self) -> impl Future<Output = EventRead> + Send + '_ {
        Receive {
            events: self,
            registration: None,
        }
    }
}
struct Receive<'a> {
    events: &'a mut Events,
    registration: Option<u64>,
}
impl Future for Receive<'_> {
    type Output = EventRead;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<EventRead> {
        let this = self.get_mut();
        let mut state = this
            .events
            .history
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(event) = read(&state, &mut this.events.cursor) {
            if let Some(id) = this.registration.take() {
                state.waiters.remove(&id);
            }
            return Poll::Ready(event);
        }
        let id = *this.registration.get_or_insert_with(|| {
            let id = state.next_waiter;
            state.next_waiter = state
                .next_waiter
                .checked_add(1)
                .expect("event registration space exhausted");
            id
        });
        state.waiters.insert(id, cx.waker().clone());
        Poll::Pending
    }
}
impl Drop for Receive<'_> {
    fn drop(&mut self) {
        if let Some(id) = self.registration.take() {
            self.events
                .history
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .waiters
                .remove(&id);
        }
    }
}

pub(crate) struct Sender {
    socket: UnixDatagram,
    pending: VecDeque<Vec<u8>>,
    bytes: usize,
    pub next: u64,
}
impl Sender {
    pub fn new(socket: UnixDatagram) -> io::Result<Self> {
        socket.set_nonblocking(true)?;
        Ok(Self {
            socket,
            pending: VecDeque::new(),
            bytes: 0,
            next: 0,
        })
    }
    pub fn emit(&mut self, mut kind: RunEventKind) {
        let sequence = self.next;
        self.next += 1;
        let mut encoded = serde_json::to_vec(&RunEvent {
            sequence,
            kind: kind.clone(),
        })
        .unwrap();
        while encoded.len() > MAX_EVENT {
            if let RunEventKind::Network {
                detail, truncated, ..
            } = &mut kind
            {
                *truncated = true;
                detail.truncate(
                    detail
                        .char_indices()
                        .nth(detail.chars().count() / 2)
                        .map_or(0, |(i, _)| i),
                );
            }
            encoded = serde_json::to_vec(&RunEvent {
                sequence,
                kind: kind.clone(),
            })
            .unwrap();
        }
        self.bytes += encoded.len();
        self.pending.push_back(encoded);
        while self.pending.len() > MAX_EVENTS || self.bytes > MAX_BYTES {
            self.bytes -= self.pending.pop_front().unwrap().len();
        }
        self.flush();
    }
    pub fn flush(&mut self) {
        while let Some(message) = self.pending.front() {
            match self.socket.send(message) {
                Ok(_) => {
                    self.bytes -= self.pending.pop_front().unwrap().len();
                }
                Err(cause) if cause.kind() == io::ErrorKind::Interrupted => continue,
                Err(cause) if cause.kind() == io::ErrorKind::WouldBlock => break,
                Err(_) => {
                    self.pending.clear();
                    self.bytes = 0;
                    break;
                }
            }
        }
    }
}
