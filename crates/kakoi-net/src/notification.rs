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
    queue: VecDeque<Arc<str>>,
    // Shares the last queued line; when the queue is empty only this copy remains.
    current: Option<Arc<str>>,
    bytes: usize,
    omitted: u64,
    in_flight_bytes: usize,
}
impl Notifications {
    pub fn update(&mut self, state: NetworkState, reason: &str) {
        let line: Arc<str> =
            bounded_line(&format!("kakoi: network {}: ", state.label()), reason).into();
        if self.current.as_deref() == Some(&line) {
            return;
        }
        self.current = Some(Arc::clone(&line));
        self.push(line);
    }

    /// An event outside the network state, such as a cut grace. It does not
    /// replace the current state reported after omissions.
    pub fn notice(&mut self, text: &str) {
        self.push(bounded_line("kakoi: ", text).into());
    }

    fn push(&mut self, line: Arc<str>) {
        self.bytes += line.len();
        self.queue.push_back(line);
        while self.pending() > MAX_NOTICES || self.bytes + self.in_flight_bytes > MAX_BYTES {
            self.bytes -= self.queue.pop_front().expect("nonempty overflow").len();
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
            self.queue.clear();
            self.bytes = 0;
            self.omitted = 0;
            return Some(summary.into());
        }
        let line = self.queue.pop_front()?;
        self.bytes -= line.len();
        Some(line)
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
