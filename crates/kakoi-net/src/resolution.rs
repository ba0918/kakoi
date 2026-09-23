//! Work limits owned by one DNS resolution, across upstream and settings changes.

use hickory_proto::rr::Name;
use kakoi_core::network::NetworkLimits;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CnameError {
    InvalidName,
    Cycle,
    Hops,
}

#[derive(Debug, Clone)]
pub struct CnameChain {
    names: Vec<Name>,
    max_hops: u32,
    deadline: Option<Instant>,
}

impl CnameChain {
    /// Names use DNS presentation syntax, including escaped label octets.
    pub fn new(original: &str, max_hops: u32) -> Result<Self, CnameError> {
        Ok(Self {
            names: vec![Name::from_ascii(original).map_err(|_| CnameError::InvalidName)?],
            max_hops,
            deadline: None,
        })
    }

    /// The caller validates the CNAME owner and response before following it.
    /// `deadline` is absolute, calculated from this record's reception time.
    pub fn follow(&mut self, target: &str, deadline: Instant) -> Result<(), CnameError> {
        let name = Name::from_ascii(target).map_err(|_| CnameError::InvalidName)?;
        if self.names.iter().any(|seen| seen.eq_ignore_root(&name)) {
            return Err(CnameError::Cycle);
        }
        if self.names.len() > self.max_hops as usize {
            return Err(CnameError::Hops);
        }
        self.names.push(name);
        self.deadline = Some(self.deadline.map_or(deadline, |old| old.min(deadline)));
        Ok(())
    }

    pub fn permission_deadline(&self, address_deadline: Instant) -> Instant {
        self.deadline
            .map_or(address_deadline, |cname| cname.min(address_deadline))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionLimit {
    Deadline,
    Queries,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpstreamWait {
    Explicit,
    HostResolver,
}

#[derive(Debug)]
pub struct ResolutionBudget {
    deadline: Instant,
    server_timeout: Duration,
    remaining_queries: u32,
}

impl ResolutionBudget {
    pub fn new(start: Instant, limits: &NetworkLimits) -> Result<Self, ResolutionLimit> {
        Ok(Self {
            deadline: start
                .checked_add(Duration::from_secs(u64::from(
                    limits.dns_resolution_timeout_seconds,
                )))
                .ok_or(ResolutionLimit::Deadline)?,
            server_timeout: Duration::from_secs(u64::from(limits.dns_server_timeout_seconds)),
            remaining_queries: limits.dns_max_upstream_queries,
        })
    }

    pub(crate) fn constrain_deadline(&mut self, deadline: Instant) {
        self.deadline = self.deadline.min(deadline);
    }

    pub fn ensure_live(&self, now: Instant) -> Result<(), ResolutionLimit> {
        if now >= self.deadline {
            Err(ResolutionLimit::Deadline)
        } else {
            Ok(())
        }
    }

    /// Reserve immediately before sending a DNS query. A failed send consumes its
    /// reservation too; transport-level packet retries do not reserve again.
    pub fn reserve_query(
        &mut self,
        now: Instant,
        upstream: UpstreamWait,
    ) -> Result<Instant, ResolutionLimit> {
        self.ensure_live(now)?;
        self.remaining_queries = self
            .remaining_queries
            .checked_sub(1)
            .ok_or(ResolutionLimit::Queries)?;
        Ok(match upstream {
            UpstreamWait::HostResolver => self.deadline,
            UpstreamWait::Explicit => now
                .checked_add(self.server_timeout)
                .map_or(self.deadline, |deadline| deadline.min(self.deadline)),
        })
    }
}
