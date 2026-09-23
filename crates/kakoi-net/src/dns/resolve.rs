use super::{AddressCandidate, AddressProgress, DnsError, Question, ValidatedResponse};
use crate::resolution::ResolutionBudget;
use hickory_proto::rr::{Name, Record};
use kakoi_core::network::NetworkLimits;
use std::time::{Duration, Instant};

pub struct ResolvedAddresses {
    pub response: ValidatedResponse,
    pub candidates: Vec<AddressCandidate>,
}

/// Resolve an already-authorized IN A/AAAA question, following validated CNAMEs.
/// Every network send inside `lookup` must reserve against the supplied budget;
/// this function never replaces it when advancing to another name. Candidate
/// deadlines remain absolute and still require scope checks and kernel adoption.
pub fn resolve_addresses(
    wire: &[u8],
    limits: &NetworkLimits,
    budget: &mut ResolutionBudget,
    mut lookup: impl FnMut(&[u8], &mut ResolutionBudget) -> Result<(Vec<u8>, Instant), DnsError>,
) -> Result<ResolvedAddresses, DnsError> {
    let original = Question::parse(wire)?;
    let mut chain = original.address_chain(limits.dns_max_cname_hops)?;
    let mut request = wire.to_vec();
    let mut previous: Vec<(ValidatedResponse, Instant)> = Vec::new();
    loop {
        budget
            .ensure_live(Instant::now())
            .map_err(|_| DnsError::IncompleteResponse)?;
        let (reply, received) = lookup(&request, budget)?;
        budget
            .ensure_live(received)
            .map_err(|_| DnsError::IncompleteResponse)?;
        if previous.last().is_some_and(|(_, last)| received < *last) {
            return Err(DnsError::Malformed);
        }
        let response = Question::parse(&request)?.validate_response(&reply)?;
        let progress = chain.consume(
            &response,
            received,
            Duration::from_millis(u64::from(limits.dns_zero_ttl_grace_milliseconds)),
        )?;
        match progress {
            AddressProgress::Follow(target) => {
                // A TSIG binds the whole message. It cannot authenticate a newly
                // synthesized follow-up or a response assembled from two messages.
                if original.message.signature.is_some() || response.message.signature.is_some() {
                    return Err(DnsError::IncompleteResponse);
                }
                let mut follow = original.message.clone();
                follow.queries[0]
                    .set_name(Name::from_ascii(target).map_err(|_| DnsError::Malformed)?);
                request = follow.to_vec().map_err(|_| DnsError::Malformed)?;
                previous.push((response, received));
            }
            terminal => {
                let candidates = match terminal {
                    AddressProgress::Complete(candidates) => candidates,
                    AddressProgress::NoData => Vec::new(),
                    AddressProgress::Follow(_) => unreachable!(),
                };
                let response = if previous.is_empty() {
                    response
                } else {
                    assemble(&original, previous, response, received)?
                };
                return Ok(ResolvedAddresses {
                    response,
                    candidates,
                });
            }
        }
    }
}

fn assemble(
    original: &Question,
    previous: Vec<(ValidatedResponse, Instant)>,
    last: ValidatedResponse,
    now: Instant,
) -> Result<ValidatedResponse, DnsError> {
    if last.message.signature.is_some() {
        return Err(DnsError::IncompleteResponse);
    }
    let mut combined = last.message.clone();
    combined.metadata.id = original.message.metadata.id;
    combined.metadata.authoritative = false;
    combined.metadata.recursion_desired = original.message.metadata.recursion_desired;
    combined.metadata.checking_disabled = original.message.metadata.checking_disabled;
    combined.queries = original.message.queries.clone();
    combined.answers.clear();
    for (response, received) in previous {
        combined.metadata.authentic_data &= response.message.metadata.authentic_data;
        for mut record in response.message.answers {
            age(&mut record, received, now)?;
            combined.answers.push(record);
        }
    }
    // RRSIG data and complete RRsets are retained. Only their normal cache TTLs
    // age; cryptographic validation remains the upstream's responsibility.
    combined.answers.extend(last.message.answers);
    original.validate_response(&combined.to_vec().map_err(|_| DnsError::Malformed)?)
}

fn age(record: &mut Record, received: Instant, now: Instant) -> Result<(), DnsError> {
    let expiry = received
        .checked_add(Duration::from_secs(u64::from(record.ttl)))
        .ok_or(DnsError::LifetimeOverflow)?;
    record.ttl = expiry
        .saturating_duration_since(now)
        .as_secs()
        .min(u64::from(u32::MAX)) as u32;
    Ok(())
}
