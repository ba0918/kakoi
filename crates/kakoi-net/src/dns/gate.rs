use super::{DnsError, Message, Question, ResponseCode};
use crate::host::{HOST_LOOPBACK_V4, HOST_LOOPBACK_V6};
use hickory_proto::rr::{
    rdata::{A, AAAA},
    DNSClass, RData, Record, RecordType,
};
use kakoi_core::network::{reserved_host, Allow, Destination, IpFamily};

/// Per-environment immutable authorization boundary. Host-reserved names are
/// served separately by the controller and never sent to an upstream here.
pub struct DnsGate {
    pub(super) policy: std::sync::Arc<[Allow]>,
}

impl DnsGate {
    pub fn new(policy: Vec<Allow>) -> Self {
        Self {
            policy: policy.into(),
        }
    }

    pub(super) fn authorized_rules(&self, question: &Question) -> Vec<usize> {
        let name = question.message.queries[0].name().to_ascii();
        self.policy
            .iter()
            .enumerate()
            .filter_map(|(index, rule)| match &rule.destination {
                Destination::Dns(pattern) if pattern.matches(&name) => Some(index),
                _ => None,
            })
            .collect()
    }

    /// The resolver receives only authorized initial questions and original allow
    /// indices. It must finish CNAME validation, address screening and kernel
    /// permission installation before returning a successful response. An index
    /// authorizes only its own protocol and ports, even if several rules match.
    pub fn dispatch(
        &self,
        wire: &[u8],
        resolve: impl FnOnce(&[u8], &[usize]) -> Result<Vec<u8>, DnsError>,
    ) -> Result<Vec<u8>, DnsError> {
        let question = Question::parse(wire)?;
        let matched = self.authorized_rules(&question);
        let result = if matched.is_empty() {
            Err(DnsError::PolicyDenied)
        } else {
            resolve(wire, &matched).and_then(|answer| {
                question.validate_response(&answer)?;
                Ok(answer)
            })
        };
        match result {
            Ok(answer) => Ok(answer),
            Err(error) => question.error_response(if error == DnsError::PolicyDenied {
                ResponseCode::Refused
            } else {
                ResponseCode::ServFail
            }),
        }
    }
}

/// Reserved host names always resolve to the same address in the environment.
const RESERVED_HOST_TTL: u32 = 300;

impl Question {
    /// The environment's own answer for a reserved host name, if this is one.
    pub(super) fn reserved_host_response(&self) -> Option<Result<Vec<u8>, DnsError>> {
        let query = &self.message.queries[0];
        let family = reserved_host(&query.name().to_ascii())?;
        let mut response = Message::error_msg(
            self.message.metadata.id,
            self.message.metadata.op_code,
            ResponseCode::NoError,
        );
        response.metadata.authoritative = true;
        response.metadata.recursion_desired = self.message.metadata.recursion_desired;
        response.metadata.checking_disabled = self.message.metadata.checking_disabled;
        response.metadata.recursion_available = true;
        response.queries = self.message.queries.clone();
        let data = match (query.query_class(), family, query.query_type()) {
            (DNSClass::IN, IpFamily::Ipv4, RecordType::A) => Some(RData::A(A(HOST_LOOPBACK_V4))),
            (DNSClass::IN, IpFamily::Ipv6, RecordType::AAAA) => {
                Some(RData::AAAA(AAAA(HOST_LOOPBACK_V6)))
            }
            // Other types of a reserved name have no data.
            _ => None,
        };
        if let Some(data) = data {
            response.answers.push(Record::from_rdata(
                query.name().clone(),
                RESERVED_HOST_TTL,
                data,
            ));
        }
        Some(response.to_vec().map_err(|_| DnsError::Malformed))
    }

    pub(super) fn error_response(&self, code: ResponseCode) -> Result<Vec<u8>, DnsError> {
        let mut response = Message::error_msg(
            self.message.metadata.id,
            self.message.metadata.op_code,
            code,
        );
        response.metadata.recursion_desired = self.message.metadata.recursion_desired;
        response.metadata.checking_disabled = self.message.metadata.checking_disabled;
        response.metadata.recursion_available = true;
        response.queries = self.message.queries.clone();
        response.to_vec().map_err(|_| DnsError::Malformed)
    }
}
