use super::{AddressCandidate, DnsError, ResponseDisposition, ValidatedResponse};
use crate::scope::DnsAdmission;
use hickory_proto::{
    op::ResponseCode,
    rr::{DNSClass, RData, Record, RecordType},
};
use std::{collections::BTreeSet, net::IpAddr};

pub struct ScreenedAnswer {
    pub wire: Vec<u8>,
    pub dynamic_grants: Vec<AddressCandidate>,
}

impl ValidatedResponse {
    /// `candidates` must be the completed address walk for this response. Admission
    /// is a pure decision over the original policy and controller-observed scope.
    pub fn screen_addresses(
        &self,
        candidates: &[AddressCandidate],
        admit: impl Fn(IpAddr) -> DnsAdmission,
    ) -> Result<ScreenedAnswer, DnsError> {
        if self.disposition() != ResponseDisposition::Answer
            || self.message.metadata.response_code != ResponseCode::NoError
        {
            return Err(DnsError::IncompleteResponse);
        }
        let mut retained = BTreeSet::new();
        let mut dynamic_grants = Vec::new();
        for candidate in candidates {
            match admit(candidate.address) {
                DnsAdmission::Denied => {}
                DnsAdmission::ExistingOnly => {
                    retained.insert(candidate.address);
                }
                DnsAdmission::Dynamic => {
                    retained.insert(candidate.address);
                    dynamic_grants.push(*candidate);
                }
            }
        }
        if !candidates.is_empty() && retained.is_empty() {
            return Err(DnsError::PolicyDenied);
        }
        let signed = self.message.signature.is_some()
            || self
                .message
                .answers
                .iter()
                .chain(&self.message.authorities)
                .chain(&self.message.additionals)
                .any(|record| record.record_type() == RecordType::RRSIG);
        // Permissions end with the shortest time to live in the answer; a longer
        // one would let the application keep an address past its permission.
        let shortest = self.message.answers.iter().map(|record| record.ttl).min();
        let align = self.message.signature.is_none()
            && self
                .message
                .answers
                .iter()
                .any(|record| Some(record.ttl) > shortest);
        // Only a screened address of the question's own name and type is kept:
        // an address record of another name or family was never screened.
        let kind = self.message.queries[0].query_type();
        let screened = |record: &Record| {
            if record.dns_class != DNSClass::IN {
                return true;
            }
            let ip = match &record.data {
                RData::A(address) => IpAddr::V4(address.0),
                RData::AAAA(address) => IpAddr::V6(address.0).to_canonical(),
                _ => return true,
            };
            record.record_type() == kind && retained.contains(&ip)
        };
        let filter = !signed && !self.message.answers.iter().all(screened);
        let wire = if !align && !filter {
            self.wire.clone()
        } else {
            let mut message = self.message.clone();
            if filter {
                message.answers.retain(screened);
                // After rewriting unsigned data, do not claim the original upstream's
                // authenticated-data status for the modified answer.
                message.metadata.authentic_data = false;
            }
            if let (true, Some(shortest)) = (align, shortest) {
                for record in &mut message.answers {
                    record.ttl = shortest;
                }
            }
            message.to_vec().map_err(|_| DnsError::Malformed)?
        };
        Ok(ScreenedAnswer {
            wire,
            dynamic_grants,
        })
    }
}
