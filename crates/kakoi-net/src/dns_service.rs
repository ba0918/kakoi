//! Nonblocking controller side of DNS service. Resolution tasks run separately;
//! the supervisor can keep checking health while upstreams or kernel updates wait.
use crate::{
    dns::{AcceptedRequest, DnsError, DnsReply, DnsRequests, ResolutionId, ResolutionTask},
    dns_front::{DnsFront, ReplyToken},
};
use std::{io, time::Instant};

pub struct DnsService {
    front: DnsFront,
    requests: DnsRequests<ReplyToken>,
}

impl DnsService {
    pub fn new(front: DnsFront, requests: DnsRequests<ReplyToken>) -> Self {
        Self { front, requests }
    }

    /// Each returned task owns one admitted resolution slot. Schedule it once,
    /// carrying its deadline, and report its completion after permission adoption.
    /// Workers must remain bounded independently of client expiration: an expired
    /// task may still be exiting and must not be replaced with unbounded threads.
    pub fn poll(&mut self, now: Instant) -> io::Result<Vec<ResolutionTask>> {
        let expired = self.requests.expire(now);
        self.send(expired, now)?;
        let mut tasks = Vec::new();
        for query in self.front.poll(now)? {
            match self.requests.accept(&query.wire, query.reply, now) {
                Ok(AcceptedRequest::Start(task)) => tasks.push(task),
                Ok(AcceptedRequest::Waiting) => {}
                Ok(AcceptedRequest::Answer(answer)) => self.send(vec![answer], now)?,
                Err((error, token)) => {
                    if let Some(wire) = rejection(&query.wire, error) {
                        self.front.respond(token, &wire, now)?;
                    }
                }
            }
        }
        Ok(tasks)
    }

    pub fn advance_generation(&mut self) {
        self.requests.advance_generation();
    }

    pub fn rekey(&mut self, id: ResolutionId) {
        self.requests.rekey(id);
    }

    /// Enforcement failures belong to the supervisor and must close traffic;
    /// callers must not turn uncertain kernel state into an ordinary DNS error.
    pub fn complete(
        &mut self,
        id: ResolutionId,
        answer: Result<Vec<u8>, DnsError>,
        now: Instant,
    ) -> io::Result<()> {
        let replies = self.requests.complete(id, answer, now);
        self.send(replies, now)
    }

    fn send(&mut self, replies: Vec<DnsReply<ReplyToken>>, now: Instant) -> io::Result<()> {
        for reply in replies {
            self.front.respond(reply.recipient, &reply.wire, now)?;
        }
        Ok(())
    }
}

fn rejection(query: &[u8], error: DnsError) -> Option<Vec<u8>> {
    // A missing header has no trustworthy identity. Never answer a response,
    // which could otherwise start a response loop between local endpoints.
    if query.len() < 12 || query[2] & 0x80 != 0 {
        return None;
    }
    let mut response = vec![0; 12];
    response[..2].copy_from_slice(&query[..2]);
    response[2] = 0x80 | (query[2] & 0x79); // QR, original opcode and RD.
    response[3] = 0x80 | (query[3] & 0x10); // RA and original CD.
    response[3] |= if error == DnsError::UnsupportedQuery {
        4
    } else {
        1
    };
    Some(response)
}
