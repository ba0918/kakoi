//! DNS permission lifetimes. Times come from the caller's monotonic clock;
//! this module neither resolves names nor operates on kernel rules.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

pub fn record_deadline(
    received: Instant,
    ttl_seconds: u32,
    zero_ttl_grace: Duration,
) -> Option<Instant> {
    received.checked_add(if ttl_seconds == 0 {
        zero_ttl_grace
    } else {
        Duration::from_secs(u64::from(ttl_seconds))
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LeaseBook {
    deadlines: BTreeMap<(usize, IpAddr), Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveGrant {
    pub rule: usize,
    pub address: IpAddr,
    pub deadline: Instant,
}

impl LeaseBook {
    /// `rule` refers to the immutable policy's allow list. The caller must validate
    /// the DNS response and address scope before adding a grant.
    pub fn record(&self, rule: usize, address: IpAddr, deadline: Instant, now: Instant) -> Self {
        let mut next = Self {
            deadlines: self
                .deadlines
                .iter()
                .filter(|(_, expiry)| **expiry > now)
                .map(|(key, expiry)| (*key, *expiry))
                .collect(),
        };
        if deadline > now {
            next.deadlines
                .entry((rule, address.to_canonical()))
                .and_modify(|old| *old = (*old).max(deadline))
                .or_insert(deadline);
        }
        next
    }

    pub fn permits(&self, rule: usize, address: IpAddr, now: Instant) -> bool {
        self.deadlines
            .get(&(rule, address.to_canonical()))
            .is_some_and(|expiry| *expiry > now)
    }

    /// The grants still alive at `now`; each staging installs these again beside
    /// the new ones.
    pub fn active(&self, now: Instant) -> Vec<ActiveGrant> {
        self.deadlines
            .iter()
            .filter(|(_, expiry)| **expiry > now)
            .map(|((rule, address), deadline)| ActiveGrant {
                rule: *rule,
                address: *address,
                deadline: *deadline,
            })
            .collect()
    }
}
