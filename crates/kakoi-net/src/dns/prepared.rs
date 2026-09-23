use super::{DnsError, EnforcedDnsError};
use crate::{dns_workers::Cancellation, dynamic::DynamicPermissions, leases::ActiveGrant};
use kakoi_core::network::Allow;
use std::{io, sync::Arc, time::Instant};

/// Screened answer whose wire bytes become available only after permission
/// adoption. Dropping a candidate has no effect on the kernel. It cannot be
/// cloned or reused to refresh permissions with a new lifetime.
pub struct PreparedAnswer {
    wire: Vec<u8>,
    grants: Vec<ActiveGrant>,
    deadline: Instant,
    cancellation: Option<Cancellation>,
    policy: Arc<[Allow]>,
}

impl PreparedAnswer {
    pub(super) fn new(
        wire: Vec<u8>,
        grants: Vec<ActiveGrant>,
        deadline: Instant,
        cancellation: Option<Cancellation>,
        policy: Arc<[Allow]>,
    ) -> Self {
        Self {
            wire,
            grants,
            deadline,
            cancellation,
            policy,
        }
    }

    fn ensure_live(&self, additional: Option<&Cancellation>) -> Result<(), EnforcedDnsError> {
        let now = Instant::now();
        if additional.is_some_and(Cancellation::is_cancelled)
            || now >= self.deadline
            || self.grants.iter().any(|grant| now >= grant.deadline)
            || self
                .cancellation
                .as_ref()
                .is_some_and(Cancellation::is_cancelled)
        {
            return Err(EnforcedDnsError::Query(DnsError::IncompleteResponse));
        }
        Ok(())
    }

    /// The session serializes adoption and closes its transit gate on shutdown
    /// or uncertain kernel state. Staging remains unreachable until the second
    /// validity check; successful activation never reinserts element timeouts.
    pub fn adopt(
        self,
        permissions: &mut DynamicPermissions<'_>,
    ) -> Result<Vec<u8>, EnforcedDnsError> {
        self.adopt_controlled(permissions, None)
    }

    pub(crate) fn adopt_controlled(
        self,
        permissions: &mut DynamicPermissions<'_>,
        cancellation: Option<&Cancellation>,
    ) -> Result<Vec<u8>, EnforcedDnsError> {
        if permissions.requires_reconciliation() || !permissions.matches_policy(&self.policy) {
            return Err(EnforcedDnsError::Enforcement(io::Error::other(
                "DNS candidate and kernel owner are inconsistent or need reconciliation",
            )));
        }
        self.ensure_live(cancellation)?;
        let result = (|| {
            if !self.grants.is_empty() {
                let staged = permissions
                    .stage(&self.grants)
                    .map_err(EnforcedDnsError::Enforcement)?;
                self.ensure_live(cancellation)?;
                staged.activate().map_err(EnforcedDnsError::Enforcement)?;
            }
            self.ensure_live(cancellation)?;
            Ok(())
        })();
        // Dropping an unadopted staging token can itself discover a kernel fault.
        // Do not hide that failure behind an otherwise ordinary cancellation.
        if permissions.requires_reconciliation() {
            return match result {
                Err(EnforcedDnsError::Enforcement(error)) => {
                    Err(EnforcedDnsError::Enforcement(error))
                }
                _ => Err(EnforcedDnsError::Enforcement(io::Error::other(
                    "DNS staging cleanup needs reconciliation",
                ))),
            };
        }
        match result {
            Ok(()) => Ok(self.wire),
            Err(EnforcedDnsError::Enforcement(_)) => {
                Err(EnforcedDnsError::Query(DnsError::IncompleteResponse))
            }
            Err(error) => Err(error),
        }
    }
}
