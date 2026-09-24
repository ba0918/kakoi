//! Work limits owned by one DNS resolution, across upstream and settings changes.

use hickory_proto::rr::Name;
use kakoi_core::network::NetworkLimits;
use std::{
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

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

/// The upstream queries one resolution may still send. Clones share the
/// count, so that asking again under new settings continues it.
#[derive(Debug, Clone)]
pub struct QueryAllowance(Arc<AtomicU32>);

impl QueryAllowance {
    pub fn new(limits: &NetworkLimits) -> Self {
        Self(Arc::new(AtomicU32::new(limits.dns_max_upstream_queries)))
    }

    pub fn remaining(&self) -> u32 {
        self.0.load(Ordering::SeqCst)
    }

    fn take(&self) -> bool {
        self.0
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                left.checked_sub(1)
            })
            .is_ok()
    }
}

#[derive(Debug)]
pub struct ResolutionBudget {
    deadline: Instant,
    server_timeout: Duration,
    queries: QueryAllowance,
}

impl ResolutionBudget {
    pub fn new(start: Instant, limits: &NetworkLimits) -> Result<Self, ResolutionLimit> {
        Self::sharing(start, limits, QueryAllowance::new(limits))
    }

    /// A budget whose queries come out of `queries`, shared with earlier work
    /// on the same resolution.
    pub fn sharing(
        start: Instant,
        limits: &NetworkLimits,
        queries: QueryAllowance,
    ) -> Result<Self, ResolutionLimit> {
        Ok(Self {
            deadline: start
                .checked_add(Duration::from_secs(u64::from(
                    limits.dns_resolution_timeout_seconds,
                )))
                .ok_or(ResolutionLimit::Deadline)?,
            server_timeout: Duration::from_secs(u64::from(limits.dns_server_timeout_seconds)),
            queries,
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
        if !self.queries.take() {
            return Err(ResolutionLimit::Queries);
        }
        Ok(match upstream {
            UpstreamWait::HostResolver => self.deadline,
            UpstreamWait::Explicit => now
                .checked_add(self.server_timeout)
                .map_or(self.deadline, |deadline| deadline.min(self.deadline)),
        })
    }
}
