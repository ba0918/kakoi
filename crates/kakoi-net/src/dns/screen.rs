use super::{AddressCandidate, DnsError, ResponseDisposition, ValidatedResponse};
use crate::scope::DnsAdmission;
use hickory_proto::{
    op::ResponseCode,
    rr::{DNSClass, RData, RecordType},
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
        let mut denied = BTreeSet::new();
        let mut retained = 0;
        let mut dynamic_grants = Vec::new();
        for candidate in candidates {
            match admit(candidate.address) {
                DnsAdmission::Denied => {
                    denied.insert(candidate.address);
                }
                DnsAdmission::ExistingOnly => retained += 1,
                DnsAdmission::Dynamic => {
                    retained += 1;
                    dynamic_grants.push(*candidate);
                }
            }
        }
        if !candidates.is_empty() && retained == 0 {
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
        let wire = if signed || denied.is_empty() {
            self.wire.clone()
        } else {
            let mut message = self.message.clone();
            message.answers.retain(|record| {
                if record.dns_class != DNSClass::IN {
                    return true;
                }
                let ip = match &record.data {
                    RData::A(address) => IpAddr::V4(address.0),
                    RData::AAAA(address) => IpAddr::V6(address.0).to_canonical(),
                    _ => return true,
                };
                !denied.contains(&ip)
            });
            // After rewriting unsigned data, do not claim the original upstream's
            // authenticated-data status for the modified answer.
            message.metadata.authentic_data = false;
            message.to_vec().map_err(|_| DnsError::Malformed)?
        };
        Ok(ScreenedAnswer {
            wire,
            dynamic_grants,
        })
    }
}
