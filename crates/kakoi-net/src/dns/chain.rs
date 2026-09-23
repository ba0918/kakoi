use super::{DnsError, ResponseDisposition, ValidatedResponse};
use crate::{
    leases::record_deadline,
    resolution::{CnameChain, CnameError},
};
use hickory_proto::{
    op::{Query, ResponseCode},
    rr::{DNSClass, Name, RData, RecordType},
};
use std::{
    net::IpAddr,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddressCandidate {
    pub address: IpAddr,
    pub deadline: Instant,
    pub cache_deadline: Instant,
}

#[derive(Debug, PartialEq, Eq)]
pub enum AddressProgress {
    Complete(Vec<AddressCandidate>),
    Follow(String),
    NoData,
}

#[derive(Debug, Clone)]
pub struct AddressChain {
    current: Name,
    kind: RecordType,
    aliases: CnameChain,
    cache_deadline: Option<Instant>,
}

impl AddressChain {
    pub(super) fn new(query: &Query, max_hops: u32) -> Result<Self, DnsError> {
        if query.query_class() != DNSClass::IN
            || !matches!(query.query_type(), RecordType::A | RecordType::AAAA)
        {
            return Err(DnsError::UnsupportedQuery);
        }
        Ok(Self {
            current: query.name().clone(),
            kind: query.query_type(),
            aliases: CnameChain::new(&query.name().to_ascii(), max_hops).map_err(alias_error)?,
            cache_deadline: None,
        })
    }

    /// Candidates still require address-scope and immutable-policy checks before
    /// granting anything. Invalid replies leave the previously accepted chain intact.
    pub fn consume(
        &mut self,
        response: &ValidatedResponse,
        received: Instant,
        zero_grace: Duration,
    ) -> Result<AddressProgress, DnsError> {
        let mut next = self.clone();
        let result = next.consume_inner(response, received, zero_grace)?;
        *self = next;
        Ok(result)
    }

    fn consume_inner(
        &mut self,
        response: &ValidatedResponse,
        received: Instant,
        zero_grace: Duration,
    ) -> Result<AddressProgress, DnsError> {
        let question = &response.message.queries[0];
        if !question.name().eq_ignore_root(&self.current)
            || question.query_type() != self.kind
            || question.query_class() != DNSClass::IN
        {
            return Err(DnsError::MismatchedResponse);
        }
        if response.disposition() != ResponseDisposition::Answer {
            return Err(DnsError::IncompleteResponse);
        }
        if response.message.metadata.response_code != ResponseCode::NoError {
            return Ok(AddressProgress::NoData);
        }
        let mut followed = false;
        loop {
            let records: Vec<_> = response
                .message
                .answers
                .iter()
                .filter(|record| {
                    record.dns_class == DNSClass::IN && record.name.eq_ignore_root(&self.current)
                })
                .collect();
            let aliases: Vec<_> = records
                .iter()
                .filter_map(|record| match &record.data {
                    RData::CNAME(name) => Some((&name.0, record.ttl)),
                    _ => None,
                })
                .collect();
            if let Some((target, _)) = aliases.first() {
                if aliases
                    .iter()
                    .any(|(other, _)| !other.eq_ignore_root(target))
                    || records.iter().any(|record| {
                        !matches!(
                            record.record_type(),
                            RecordType::CNAME | RecordType::RRSIG | RecordType::NSEC
                        )
                    })
                {
                    return Err(DnsError::ConflictingAlias);
                }
                let ttl = aliases
                    .iter()
                    .map(|(_, ttl)| *ttl)
                    .min()
                    .expect("at least one alias");
                let deadline =
                    earliest_deadline(aliases.iter().map(|(_, ttl)| *ttl), received, zero_grace)?
                        .expect("at least one alias");
                let cache = record_deadline(received, ttl, Duration::ZERO)
                    .ok_or(DnsError::LifetimeOverflow)?;
                self.aliases
                    .follow(&target.to_ascii(), deadline)
                    .map_err(alias_error)?;
                self.cache_deadline = Some(self.cache_deadline.map_or(cache, |old| old.min(cache)));
                self.current = (*target).clone();
                followed = true;
                continue;
            }
            let addresses: Vec<_> = records
                .iter()
                .filter_map(|record| match (&record.data, self.kind) {
                    (RData::A(address), RecordType::A) => Some((IpAddr::V4(address.0), record.ttl)),
                    (RData::AAAA(address), RecordType::AAAA) => {
                        Some((IpAddr::V6(address.0).to_canonical(), record.ttl))
                    }
                    _ => None,
                })
                .collect();
            if let Some(ttl) = addresses.iter().map(|(_, ttl)| *ttl).min() {
                let deadline = self.aliases.permission_deadline(
                    earliest_deadline(addresses.iter().map(|(_, ttl)| *ttl), received, zero_grace)?
                        .expect("at least one address"),
                );
                let cache = record_deadline(received, ttl, Duration::ZERO)
                    .ok_or(DnsError::LifetimeOverflow)?;
                let cache_deadline = self.cache_deadline.map_or(cache, |old| old.min(cache));
                return Ok(AddressProgress::Complete(
                    addresses
                        .into_iter()
                        .map(|(address, _)| AddressCandidate {
                            address,
                            deadline,
                            cache_deadline,
                        })
                        .collect(),
                ));
            }
            return Ok(if followed {
                AddressProgress::Follow(self.current.to_ascii())
            } else {
                AddressProgress::NoData
            });
        }
    }
}

fn earliest_deadline(
    ttls: impl Iterator<Item = u32>,
    received: Instant,
    grace: Duration,
) -> Result<Option<Instant>, DnsError> {
    ttls.map(|ttl| record_deadline(received, ttl, grace).ok_or(DnsError::LifetimeOverflow))
        .try_fold(None, |earliest: Option<Instant>, next| {
            let next = next?;
            Ok(Some(earliest.map_or(next, |old| old.min(next))))
        })
}

fn alias_error(error: CnameError) -> DnsError {
    match error {
        CnameError::Cycle => DnsError::AliasCycle,
        CnameError::Hops => DnsError::AliasLimit,
        CnameError::InvalidName => DnsError::Malformed,
    }
}
