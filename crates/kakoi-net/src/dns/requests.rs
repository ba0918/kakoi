use super::{
    Admission, DnsError, DnsGate, Question, ResolutionId, ResolutionKey, ResolutionPool,
    ResponseCode,
};
use hickory_proto::op::Query;
use kakoi_core::network::Allow;
use std::time::{Duration, Instant};

pub struct DnsReply<W> {
    pub recipient: W,
    pub wire: Vec<u8>,
}

pub enum AcceptedRequest<W> {
    Start(ResolutionTask),
    Waiting,
    Answer(DnsReply<W>),
}

pub struct ResolutionTask {
    pub id: ResolutionId,
    pub wire: Vec<u8>,
    pub deadline: Instant,
}

struct Waiter<W> {
    recipient: W,
    id: u16,
    query: Query,
}

struct Running {
    id: ResolutionId,
    question: Question,
    deadline: Instant,
}

/// One immutable policy/settings generation per environment. The caller executes
/// only `Start` tasks and enforces their original absolute deadline. Completion
/// must follow address screening and kernel permission installation; this owner
/// only validates and distributes the answer, and never installs permissions.
pub struct DnsRequests<W> {
    gate: DnsGate,
    generation: u64,
    pool: ResolutionPool<ResolutionKey, Waiter<W>>,
    running: Vec<Running>,
    timeout: Duration,
}

impl<W> DnsRequests<W> {
    pub fn new(
        policy: Vec<Allow>,
        generation: u64,
        max_resolutions: usize,
        max_waiters: usize,
        timeout: Duration,
    ) -> Result<Self, &'static str> {
        if timeout.is_zero() || timeout > Duration::from_secs(3600) {
            return Err("DNS resolution timeout outside supported range");
        }
        Ok(Self {
            gate: DnsGate::new(policy),
            generation,
            pool: ResolutionPool::new(max_resolutions, max_waiters)?,
            running: Vec::new(),
            timeout,
        })
    }

    pub fn accept(
        &mut self,
        wire: &[u8],
        recipient: W,
        now: Instant,
    ) -> Result<AcceptedRequest<W>, (DnsError, W)> {
        let prepare = || {
            let question = Question::parse(wire)?;
            let key = question.resolution_key(self.generation)?;
            Ok((question, key))
        };
        let (question, key) = match prepare() {
            Ok(prepared) => prepared,
            Err(error) => return Err((error, recipient)),
        };
        // Check every requester before sharing a slot, even with an identical key.
        if self.gate.authorized_rules(&question).is_empty() {
            return reply(&question, recipient, ResponseCode::Refused);
        }
        let waiter = Waiter {
            recipient,
            id: question.message.metadata.id,
            query: question.message.queries[0].clone(),
        };
        match self.pool.admit(key, waiter) {
            Ok(Admission::Start(id)) => {
                let deadline = now + self.timeout;
                self.running.push(Running {
                    id,
                    question,
                    deadline,
                });
                Ok(AcceptedRequest::Start(ResolutionTask {
                    id,
                    wire: wire.to_vec(),
                    deadline,
                }))
            }
            Ok(Admission::Join(_)) => Ok(AcceptedRequest::Waiting),
            Err((_, waiter)) => reply(&question, waiter.recipient, ResponseCode::ServFail),
        }
    }

    /// Unknown or already expired task IDs have no recipients. They cannot consume
    /// a newer slot, nor cause a second answer to an earlier requester.
    pub fn complete(
        &mut self,
        id: ResolutionId,
        answer: Result<Vec<u8>, DnsError>,
        now: Instant,
    ) -> Vec<DnsReply<W>> {
        let Some(index) = self.running.iter().position(|entry| entry.id == id) else {
            return Vec::new();
        };
        let running = self.running.swap_remove(index);
        let result = if now >= running.deadline {
            Err(DnsError::IncompleteResponse)
        } else {
            answer.and_then(|wire| running.question.validate_response(&wire))
        };
        let code = if matches!(result, Err(DnsError::PolicyDenied)) {
            ResponseCode::Refused
        } else {
            ResponseCode::ServFail
        };
        self.pool
            .complete(id)
            .into_iter()
            .map(|waiter| {
                let wire = result
                    .as_ref()
                    .ok()
                    .and_then(|answer| {
                        let original = &answer.message;
                        let same = original.metadata.id == waiter.id
                            && original.queries[0].name().to_ascii()
                                == waiter.query.name().to_ascii();
                        if same {
                            return Some(answer.wire.clone());
                        }
                        // Never rewrite a message authenticator. Signed requests have
                        // distinct keys; an unexpected signed response cannot be shared.
                        if original.signature.is_some() {
                            return None;
                        }
                        let mut message = original.clone();
                        message.metadata.id = waiter.id;
                        message.queries[0] = waiter.query.clone();
                        message.to_vec().ok()
                    })
                    .unwrap_or_else(|| {
                        let mut question = Question {
                            message: running.question.message.clone(),
                        };
                        question.message.metadata.id = waiter.id;
                        question.message.queries[0] = waiter.query;
                        // Only the fixed header and one previously decoded question are
                        // serialized; there are no response records or unbounded options.
                        question
                            .error_response(code)
                            .expect("valid question fits DNS error response")
                    });
                DnsReply {
                    recipient: waiter.recipient,
                    wire,
                }
            })
            .collect()
    }

    pub fn expire(&mut self, now: Instant) -> Vec<DnsReply<W>> {
        let expired: Vec<_> = self
            .running
            .iter()
            .filter(|entry| now >= entry.deadline)
            .map(|entry| entry.id)
            .collect();
        expired
            .into_iter()
            .flat_map(|id| self.complete(id, Err(DnsError::IncompleteResponse), now))
            .collect()
    }
}

fn reply<W>(
    question: &Question,
    recipient: W,
    code: ResponseCode,
) -> Result<AcceptedRequest<W>, (DnsError, W)> {
    match question.error_response(code) {
        Ok(wire) => Ok(AcceptedRequest::Answer(DnsReply { recipient, wire })),
        Err(error) => Err((error, recipient)),
    }
}
