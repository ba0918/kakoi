use super::{resolve_addresses, DnsError, DnsGate, PreparedAnswer, Question};
use crate::{
    dns_transport::{exchange_upstreams_cancellable, TlsClient},
    dns_workers::Cancellation,
    dynamic::DynamicPermissions,
    leases::ActiveGrant,
    resolution::{QueryAllowance, ResolutionBudget, UpstreamWait},
    scope::{AddressContext, DnsAdmission},
};
use hickory_proto::rr::{DNSClass, RecordType};
use kakoi_core::network::{validate_upstreams, Allow, DnsUpstream, NetworkLimits};
use std::{collections::BTreeMap, io, net::IpAddr, time::Instant};

/// Synchronous resolution worker for one immutable explicit-upstream snapshot.
/// The environment controller owns concurrency, settings generations and caches.
pub struct ExplicitResolver {
    gate: DnsGate,
    upstreams: Vec<DnsUpstream>,
    limits: NetworkLimits,
    trust: Option<TlsClient>,
    wait: UpstreamWait,
}

#[derive(Debug)]
pub enum EnforcedDnsError {
    Query(DnsError),
    /// Kernel state is uncertain or the controller connected inconsistent policy
    /// owners. The supervisor must close traffic and reconcile before continuing.
    Enforcement(io::Error),
}

impl ExplicitResolver {
    pub fn resolve_enforced(
        &self,
        wire: &[u8],
        scope: &AddressContext,
        route: impl Fn(IpAddr) -> Option<String>,
        permissions: &mut DynamicPermissions<'_>,
    ) -> Result<Vec<u8>, EnforcedDnsError> {
        let deadline = self.default_deadline();
        self.resolve_enforced_until(wire, deadline, scope, route, permissions)
    }

    pub fn resolve_enforced_until(
        &self,
        wire: &[u8],
        deadline: Instant,
        scope: &AddressContext,
        route: impl Fn(IpAddr) -> Option<String>,
        permissions: &mut DynamicPermissions<'_>,
    ) -> Result<Vec<u8>, EnforcedDnsError> {
        if permissions.requires_reconciliation() {
            return Err(EnforcedDnsError::Enforcement(io::Error::other(
                "kernel DNS permissions need reconciliation before resolution",
            )));
        }
        if !permissions.matches_policy(&self.gate.policy) {
            return Err(EnforcedDnsError::Enforcement(io::Error::other(
                "DNS resolver and kernel permissions must share the same policy",
            )));
        }
        let mut fault = None;
        let result = self.resolve_until(wire, deadline, scope, route, |grants| {
            permissions.install(grants).map_err(|error| {
                if permissions.requires_reconciliation() {
                    fault = Some(error);
                }
                DnsError::IncompleteResponse
            })
        });
        match fault {
            Some(error) => Err(EnforcedDnsError::Enforcement(error)),
            None => result.map_err(EnforcedDnsError::Query),
        }
    }

    pub fn new(
        policy: Vec<Allow>,
        upstreams: Vec<DnsUpstream>,
        limits: NetworkLimits,
        trust: Option<TlsClient>,
    ) -> io::Result<Self> {
        validate_upstreams(&upstreams).map_err(io::Error::other)?;
        // Without an upstream every question fails; a policy without DNS names
        // never asks one.
        if upstreams
            .first()
            .is_some_and(|first| first.tls_name().is_some())
            && trust.is_none()
        {
            return Err(io::Error::other("TLS DNS requires a startup CA snapshot"));
        }
        Ok(Self {
            gate: DnsGate::new(policy),
            upstreams,
            limits,
            trust,
            wait: UpstreamWait::Explicit,
        })
    }

    /// How long each upstream is waited for: one candidate's wait by default, or
    /// the whole resolution for a host resolver that chooses among its servers.
    pub fn waiting(mut self, wait: UpstreamWait) -> Self {
        self.wait = wait;
        self
    }

    /// Compute and screen an answer without changing kernel permissions. Workers
    /// return this single-use candidate to the session's permission owner.
    pub fn prepare_until(
        &self,
        wire: &[u8],
        deadline: Instant,
        cancellation: Option<&Cancellation>,
        scope: &AddressContext,
        route: impl Fn(IpAddr) -> Option<String>,
    ) -> Result<PreparedAnswer, DnsError> {
        let queries = QueryAllowance::new(&self.limits);
        self.prepare_sharing(wire, deadline, &queries, cancellation, scope, route)
    }

    /// As [`Self::prepare_until`], taking upstream queries from `queries`: work
    /// asked again under new settings continues the count of the first.
    pub fn prepare_sharing(
        &self,
        wire: &[u8],
        deadline: Instant,
        queries: &QueryAllowance,
        cancellation: Option<&Cancellation>,
        scope: &AddressContext,
        route: impl Fn(IpAddr) -> Option<String>,
    ) -> Result<PreparedAnswer, DnsError> {
        let deadline = deadline.min(self.default_deadline());
        let question = Question::parse(wire)?;
        let mut grants = Vec::new();
        let answer = self.resolve_controlled(
            wire,
            deadline,
            Some(queries),
            cancellation,
            scope,
            route,
            |candidate| {
                grants.extend_from_slice(candidate);
                Ok(())
            },
        )?;
        let validated = question.validate_response(&answer)?;
        // The final lifetime/cancellation check may have turned a candidate into
        // SERVFAIL after collecting grants. Such an answer must adopt nothing.
        if validated.message.metadata.response_code != super::ResponseCode::NoError {
            grants.clear();
        }
        Ok(PreparedAnswer::new(
            answer,
            grants,
            deadline,
            cancellation.cloned(),
            self.gate.policy.clone(),
        ))
    }

    fn default_deadline(&self) -> Instant {
        Instant::now()
            + std::time::Duration::from_secs(u64::from(self.limits.dns_resolution_timeout_seconds))
    }

    /// `route` supplies controller-observed host routing, never application data.
    /// `install` must atomically adopt the absolute-deadline grants or report a
    /// failure; it must not revive expired grants during a delayed kernel commit.
    /// No successful DNS reply is returned before that adoption succeeds.
    pub fn resolve(
        &self,
        wire: &[u8],
        scope: &AddressContext,
        route: impl Fn(IpAddr) -> Option<String>,
        install: impl FnOnce(&[ActiveGrant]) -> Result<(), DnsError>,
    ) -> Result<Vec<u8>, DnsError> {
        let deadline = self.default_deadline();
        self.resolve_until(wire, deadline, scope, route, install)
    }

    /// Use the admission deadline when a controller schedules this worker later.
    /// A caller-supplied deadline can shorten, but never extend, configured limits.
    pub fn resolve_until(
        &self,
        wire: &[u8],
        deadline: Instant,
        scope: &AddressContext,
        route: impl Fn(IpAddr) -> Option<String>,
        install: impl FnOnce(&[ActiveGrant]) -> Result<(), DnsError>,
    ) -> Result<Vec<u8>, DnsError> {
        self.resolve_controlled(wire, deadline, None, None, scope, route, install)
    }

    pub fn resolve_cancellable_until(
        &self,
        wire: &[u8],
        deadline: Instant,
        cancellation: &Cancellation,
        scope: &AddressContext,
        route: impl Fn(IpAddr) -> Option<String>,
        install: impl FnOnce(&[ActiveGrant]) -> Result<(), DnsError>,
    ) -> Result<Vec<u8>, DnsError> {
        self.resolve_controlled(
            wire,
            deadline,
            None,
            Some(cancellation),
            scope,
            route,
            install,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_controlled(
        &self,
        wire: &[u8],
        deadline: Instant,
        queries: Option<&QueryAllowance>,
        cancellation: Option<&Cancellation>,
        scope: &AddressContext,
        route: impl Fn(IpAddr) -> Option<String>,
        install: impl FnOnce(&[ActiveGrant]) -> Result<(), DnsError>,
    ) -> Result<Vec<u8>, DnsError> {
        self.gate.dispatch(wire, |request, rules| {
            let check_cancel = || {
                if cancellation.is_some_and(Cancellation::is_cancelled) {
                    Err(DnsError::IncompleteResponse)
                } else {
                    Ok(())
                }
            };
            check_cancel()?;
            let queries = queries
                .cloned()
                .unwrap_or_else(|| QueryAllowance::new(&self.limits));
            let mut budget = ResolutionBudget::sharing(Instant::now(), &self.limits, queries)
                .map_err(|_| DnsError::IncompleteResponse)?;
            budget.constrain_deadline(deadline);
            ensure_live(&budget, &[])?;
            let question = Question::parse(request)?;
            let query = &question.message.queries[0];
            if query.query_class() != DNSClass::IN
                || !matches!(query.query_type(), RecordType::A | RecordType::AAAA)
            {
                // Ordinary non-address data never creates IP permissions.
                return exchange_upstreams_cancellable(
                    request,
                    &self.upstreams,
                    self.trust.as_ref(),
                    &mut budget,
                    cancellation,
                    self.wait,
                )
                .map(|answer| answer.wire().to_vec())
                .map_err(|_| DnsError::IncompleteResponse);
            }
            let resolved =
                resolve_addresses(request, &self.limits, &mut budget, |query, budget| {
                    let answer = exchange_upstreams_cancellable(
                        query,
                        &self.upstreams,
                        self.trust.as_ref(),
                        budget,
                        cancellation,
                        self.wait,
                    )
                    .map_err(|_| DnsError::IncompleteResponse)?;
                    Ok((answer.wire().to_vec(), answer.received_at))
                })?;
            if resolved.candidates.is_empty() {
                return Ok(resolved.response.wire().to_vec());
            }
            let mut routes = BTreeMap::new();
            for candidate in &resolved.candidates {
                routes
                    .entry(candidate.address)
                    .or_insert_with(|| route(candidate.address));
            }
            let admit = |rule: usize, address: IpAddr| {
                scope.admit_dns(
                    &self.gate.policy[rule],
                    address,
                    routes.get(&address).and_then(|name| name.as_deref()),
                    &self.gate.policy,
                )
            };
            let screened = resolved
                .response
                .screen_addresses(&resolved.candidates, |address| {
                    let mut result = DnsAdmission::Denied;
                    for rule in rules {
                        match admit(*rule, address) {
                            DnsAdmission::Dynamic => return DnsAdmission::Dynamic,
                            DnsAdmission::ExistingOnly => result = DnsAdmission::ExistingOnly,
                            DnsAdmission::Denied => {}
                        }
                    }
                    result
                })?;
            let mut grants = Vec::new();
            for candidate in screened.dynamic_grants {
                for rule in rules {
                    if admit(*rule, candidate.address) == DnsAdmission::Dynamic {
                        grants.push(ActiveGrant {
                            rule: *rule,
                            address: candidate.address,
                            deadline: candidate.deadline,
                        });
                    }
                }
            }
            ensure_live(&budget, &grants)?;
            check_cancel()?;
            if !grants.is_empty() {
                install(&grants)?;
            }
            ensure_live(&budget, &grants)?;
            check_cancel()?;
            Ok(screened.wire)
        })
    }
}

fn ensure_live(budget: &ResolutionBudget, grants: &[ActiveGrant]) -> Result<(), DnsError> {
    let now = Instant::now();
    budget
        .ensure_live(now)
        .map_err(|_| DnsError::IncompleteResponse)?;
    if grants.iter().any(|grant| grant.deadline <= now) {
        return Err(DnsError::IncompleteResponse);
    }
    Ok(())
}
