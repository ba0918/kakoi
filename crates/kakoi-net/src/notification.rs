//! Bounded notification history, independent of any potentially blocked writer.
use std::{collections::VecDeque, sync::Arc};
mod output;
pub use output::NotificationWriter;

const MAX_NOTICES: usize = 256;
const MAX_BYTES: usize = 1024 * 1024;
const MAX_LINE: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkState {
    Running,
    Isolated,
    Unsafe,
}
impl NetworkState {
    fn label(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Isolated => "isolated",
            Self::Unsafe => "unsafe",
        }
    }
}

#[derive(Default)]
pub struct Notifications {
    queue: VecDeque<Notification>,
    // Shares the last queued line; when the queue is empty only this copy remains.
    current: Option<Arc<str>>,
    current_state: Option<NetworkState>,
    bytes: usize,
    omitted: u64,
    in_flight_bytes: usize,
}
pub struct Notification {
    pub detail: Arc<str>,
    pub state: Option<NetworkState>,
}
impl Notifications {
    pub fn update(&mut self, state: NetworkState, reason: &str) {
        let line: Arc<str> =
            bounded_line(&format!("kakoi: network {}: ", state.label()), reason).into();
        if self.current.as_deref() == Some(&line) {
            return;
        }
        self.current = Some(Arc::clone(&line));
        self.current_state = Some(state);
        self.push(Notification {
            detail: line,
            state: Some(state),
        });
    }

    /// An event outside the network state, such as a cut grace. It does not
    /// replace the current state reported after omissions.
    pub fn notice(&mut self, text: &str) {
        self.push(Notification {
            detail: bounded_line("kakoi: ", text).into(),
            state: None,
        });
    }

    fn push(&mut self, line: Notification) {
        self.bytes += line.detail.len();
        self.queue.push_back(line);
        while self.pending() > MAX_NOTICES || self.bytes + self.in_flight_bytes > MAX_BYTES {
            self.bytes -= self
                .queue
                .pop_front()
                .expect("nonempty overflow")
                .detail
                .len();
            self.omitted = self.omitted.saturating_add(1);
        }
    }
    pub fn pending(&self) -> usize {
        self.queue.len() + usize::from(self.in_flight_bytes != 0)
    }
    pub fn retained_bytes(&self) -> usize {
        let buffered = if self.queue.is_empty() {
            self.current.as_ref().map_or(0, |line| line.len())
        } else {
            self.bytes
        };
        buffered + self.in_flight_bytes
    }

    /// Use one writer for this queue's lifetime. Only one datagram is in flight,
    /// and its bytes/count remain charged until the writer acknowledges it.
    pub fn flush_to(&mut self, writer: &mut NotificationWriter) {
        match writer.poll() {
            output::Progress::Busy => return,
            output::Progress::Closed => return,
            output::Progress::Lost => {
                self.in_flight_bytes = 0;
                if self.current.is_some() {
                    self.omitted = self.omitted.saturating_add(1);
                }
            }
            output::Progress::Ready => self.in_flight_bytes = 0,
        }
        if let Some(line) = self.pop() {
            if writer.send(&line).is_ok() {
                self.in_flight_bytes = line.len();
            } else {
                self.omitted = self.omitted.saturating_add(1);
            }
        }
    }
    pub fn pop(&mut self) -> Option<Arc<str>> {
        self.pop_with_loss().map(|(line, _)| line)
    }
    pub fn pop_with_loss(&mut self) -> Option<(Arc<str>, u64)> {
        self.pop_record()
            .map(|(record, lost)| (record.detail, lost))
    }
    pub fn pop_record(&mut self) -> Option<(Notification, u64)> {
        if self.omitted != 0 {
            // Summarize the backlog as well: showing old state changes after the
            // latest state would misleadingly appear to reverse the recovery.
            let omitted = self
                .omitted
                .saturating_add(self.queue.len().saturating_sub(1) as u64);
            let summary = match self.current.as_deref() {
                Some(current) => bounded_line(
                    &format!("kakoi: {omitted} network notifications omitted; current "),
                    current
                        .trim_end()
                        .strip_prefix("kakoi: network ")
                        .unwrap_or(current),
                ),
                None => bounded_line(&format!("kakoi: {omitted} notifications omitted"), ""),
            };
            let lost = self.omitted.saturating_add(self.queue.len() as u64);
            self.queue.clear();
            self.bytes = 0;
            self.omitted = 0;
            return Some((
                Notification {
                    detail: summary.into(),
                    state: self.current_state,
                },
                lost,
            ));
        }
        let line = self.queue.pop_front()?;
        self.bytes -= line.detail.len();
        Some((line, 0))
    }
}

fn bounded_line(prefix: &str, text: &str) -> String {
    let mut line = String::with_capacity(MAX_LINE);
    for character in prefix.chars().chain(text.chars()) {
        let piece = if character.is_control() {
            character.escape_default().to_string()
        } else {
            character.to_string()
        };
        if line.len() + piece.len() > MAX_LINE - 4 {
            line.push_str("...\n");
            return line;
        }
        line.push_str(&piece);
    }
    line.push('\n');
    line
}
