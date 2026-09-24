//! DNS wire validation, independent of sockets, upstream selection and kernel rules.

use hickory_proto::{
    op::{Message, MessageType, OpCode, ResponseCode},
    serialize::binary::{BinDecodable, BinDecoder},
};

mod chain;
mod engine;
mod gate;
mod pool;
mod prepared;
mod requests;
mod resolve;
mod screen;
pub use chain::{AddressCandidate, AddressChain, AddressProgress};
pub use engine::{EnforcedDnsError, UpstreamResolver};
pub use gate::DnsGate;
pub use pool::{Admission, CapacityError, ResolutionId, ResolutionKey, ResolutionPool};
pub use prepared::PreparedAnswer;
pub use requests::{AcceptedRequest, DnsReply, DnsRequests, ResolutionTask};
pub use resolve::{resolve_addresses, ResolvedAddresses};
pub use screen::ScreenedAnswer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsError {
    Malformed,
    UnsupportedQuery,
    MismatchedResponse,
    AliasCycle,
    AliasLimit,
    ConflictingAlias,
    IncompleteResponse,
    LifetimeOverflow,
    PolicyDenied,
    /// The environment had no room for the work just now; not a failure of
    /// the name.
    Overloaded,
}

pub struct Question {
    message: Message,
}

pub struct ValidatedResponse {
    message: Message,
    wire: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseDisposition {
    Answer,
    NextUpstream,
    RetryTcp,
}

fn decode(wire: &[u8]) -> Result<Message, DnsError> {
    if wire.len() > u16::MAX as usize {
        return Err(DnsError::Malformed);
    }
    let mut decoder = BinDecoder::new(wire);
    let message = Message::read(&mut decoder).map_err(|_| DnsError::Malformed)?;
    if !decoder.is_empty() {
        return Err(DnsError::Malformed);
    }
    Ok(message)
}

impl Question {
    pub fn address_chain(&self, max_hops: u32) -> Result<AddressChain, DnsError> {
        AddressChain::new(&self.message.queries[0], max_hops)
    }

    /// This checks query syntax and operation type. The caller must independently
    /// authorize its name before contacting any upstream.
    pub fn parse(wire: &[u8]) -> Result<Self, DnsError> {
        let message = decode(wire)?;
        if message.metadata.message_type != MessageType::Query
            || message.metadata.op_code != OpCode::Query
            || message.queries.len() != 1
            || !message.answers.is_empty()
            || !message.authorities.is_empty()
        {
            return Err(DnsError::UnsupportedQuery);
        }
        // Meta-types and query-only transfer/mail operations are not ordinary RRs.
        if matches!(
            u16::from(message.queries[0].query_type()),
            0 | 41 | 249..=255 | 65535
        ) {
            return Err(DnsError::UnsupportedQuery);
        }
        Ok(Self { message })
    }

    /// Transport must also verify the peer and reject obsolete settings generations.
    pub fn validate_response(&self, wire: &[u8]) -> Result<ValidatedResponse, DnsError> {
        let message = decode(wire)?;
        if message.metadata.message_type != MessageType::Response
            || message.metadata.op_code != self.message.metadata.op_code
            || message.metadata.id != self.message.metadata.id
            || message.queries != self.message.queries
        {
            return Err(DnsError::MismatchedResponse);
        }
        Ok(ValidatedResponse {
            message,
            wire: wire.to_owned(),
        })
    }
}

impl ValidatedResponse {
    pub fn disposition(&self) -> ResponseDisposition {
        if self.message.metadata.truncation {
            ResponseDisposition::RetryTcp
        } else if matches!(
            self.message.metadata.response_code,
            ResponseCode::ServFail | ResponseCode::Refused
        ) {
            ResponseDisposition::NextUpstream
        } else {
            ResponseDisposition::Answer
        }
    }

    pub fn wire(&self) -> &[u8] {
        &self.wire
    }
}
