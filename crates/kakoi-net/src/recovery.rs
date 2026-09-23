//! Pure retry timing. The executor cancels/reaps a timed-out attempt before
//! reporting completion; expiry alone never makes room for an overlapping task.
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attempt {
    generation: u64,
    deadline: Instant,
}
impl Attempt {
    pub fn deadline(self) -> Instant {
        self.deadline
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction {
    Wait,
    Start(Attempt),
    Cancel(Attempt),
}

enum State {
    Healthy,
    Waiting(Instant),
    Working { attempt: Attempt, cancelled: bool },
}

pub struct Recovery {
    timeout: Duration,
    state: State,
    failures: u8,
    generation: u64,
}

impl Recovery {
    pub fn new(timeout_seconds: u32) -> Result<Self, &'static str> {
        if !(1..=300).contains(&timeout_seconds) {
            return Err("recovery timeout must be 1..=300 seconds");
        }
        Ok(Self {
            timeout: Duration::from_secs(u64::from(timeout_seconds)),
            state: State::Healthy,
            failures: 0,
            generation: 0,
        })
    }

    /// Call only after transit isolation has been confirmed. Repeated failure
    /// observations must not reset an in-flight attempt or its backoff.
    pub fn isolate(&mut self, now: Instant) {
        if matches!(self.state, State::Healthy) {
            self.state = State::Waiting(now);
        }
    }

    pub fn poll(&mut self, now: Instant) -> RecoveryAction {
        match &mut self.state {
            State::Waiting(at) if now >= *at => {
                self.generation = self
                    .generation
                    .checked_add(1)
                    .expect("recovery generation exhausted");
                let attempt = Attempt {
                    generation: self.generation,
                    deadline: now + self.timeout,
                };
                self.state = State::Working {
                    attempt,
                    cancelled: false,
                };
                RecoveryAction::Start(attempt)
            }
            State::Working { attempt, cancelled } if now >= attempt.deadline && !*cancelled => {
                *cancelled = true;
                RecoveryAction::Cancel(*attempt)
            }
            _ => RecoveryAction::Wait,
        }
    }

    /// Return true only for a current, timely success. The executor must have
    /// reaped all work for this attempt before calling, including cancelled work.
    pub fn complete(
        &mut self,
        completed: Attempt,
        succeeded: bool,
        now: Instant,
    ) -> Result<bool, &'static str> {
        let State::Working { attempt, cancelled } = self.state else {
            return Err("no recovery attempt is running");
        };
        if attempt != completed {
            return Err("stale recovery completion");
        }
        let success = succeeded && !cancelled && now < attempt.deadline;
        if success {
            self.state = State::Healthy;
            self.failures = 0;
        } else {
            let delay = [1, 2, 4, 8, 16, 30][usize::from(self.failures)];
            self.failures = (self.failures + 1).min(5);
            self.state = State::Waiting(now + Duration::from_secs(delay));
        }
        Ok(success)
    }
}
