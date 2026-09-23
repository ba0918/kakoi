//! Explicit-upstream DNS runtime. The session owns transit health and must close
//! its gate on any error here, and before stopping or replacing this runtime.
use crate::{
    dns::{
        DnsError, DnsRequests, EnforcedDnsError, ExplicitResolver, PreparedAnswer, ResolutionId,
    },
    dns_adoption::DnsAdoption,
    dns_front::DnsFront,
    dns_service::DnsService,
    dns_transport::TlsClient,
    dns_workers::{DnsWorkers, WorkResult},
    namespace::{DnsSockets, NetworkNamespace},
    scope::AddressContext,
};
use kakoi_core::network::{Allow, DnsUpstream, NetworkLimits};
use std::{
    io,
    net::IpAddr,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct DnsRuntimeConfig {
    pub policy: Vec<Allow>,
    pub upstreams: Vec<DnsUpstream>,
    pub limits: NetworkLimits,
    pub trust: Option<TlsClient>,
    pub nft: PathBuf,
    pub scope: AddressContext,
    pub generation: u64,
}

pub struct DnsRuntime {
    service: DnsService,
    resolver: Arc<ExplicitResolver>,
    scope: Arc<AddressContext>,
    route: Arc<dyn Fn(IpAddr) -> Option<String> + Send + Sync>,
    workers: DnsWorkers<Result<PreparedAnswer, DnsError>>,
    adoption: DnsAdoption,
    stopping: bool,
    faulted: bool,
}

impl DnsRuntime {
    /// Start after installing the policy table, while the transit gate is closed.
    /// Routing observations and all configuration come from the trusted session.
    pub fn new(
        namespace: Arc<NetworkNamespace>,
        config: DnsRuntimeConfig,
        route: impl Fn(IpAddr) -> Option<String> + Send + Sync + 'static,
    ) -> io::Result<Self> {
        config
            .limits
            .validate_deadlines()
            .map_err(io::Error::other)?;
        let limit = config.limits.dns_max_concurrent_resolutions as usize;
        let timeout = Duration::from_secs(u64::from(config.limits.dns_resolution_timeout_seconds));
        let requests = DnsRequests::new(
            config.policy.clone(),
            config.generation,
            limit,
            config.limits.dns_max_waiters_per_resolution as usize,
            timeout,
        )
        .map_err(io::Error::other)?;
        let resolver = Arc::new(ExplicitResolver::new(
            config.policy.clone(),
            config.upstreams,
            config.limits,
            config.trust,
        )?);
        let workers = DnsWorkers::new(limit).map_err(io::Error::other)?;
        let front = DnsFront::new(DnsSockets::bind(Arc::clone(&namespace))?, limit, timeout)?;
        let adoption = DnsAdoption::new(namespace, config.nft, config.policy, limit)?;
        Ok(Self {
            service: DnsService::new(front, requests),
            resolver,
            scope: Arc::new(config.scope),
            route: Arc::new(route),
            workers,
            adoption,
            stopping: false,
            faulted: false,
        })
    }

    pub fn poll(&mut self, now: Instant) -> io::Result<()> {
        let result = self.poll_inner(now);
        if result.is_err() {
            self.faulted = true;
            self.stop();
        }
        result
    }

    fn poll_inner(&mut self, now: Instant) -> io::Result<()> {
        // Collect before admitting more work so physical slots are reclaimed
        // without confusing an expired request with a finished worker.
        for completed in self.adoption.poll()? {
            let answer = match completed.answer {
                Ok(wire) => Ok(wire),
                Err(EnforcedDnsError::Query(error)) => Err(error),
                Err(EnforcedDnsError::Enforcement(error)) => return Err(error),
            };
            if !self.stopping {
                self.service.complete(completed.id, answer, now)?;
            }
        }
        if self.adoption.is_faulted() && !self.stopping {
            return Err(io::Error::other("DNS permission executor faulted"));
        }
        for completed in self.workers.collect() {
            let answer = match completed.result {
                WorkResult::Finished(answer) => answer,
                WorkResult::Cancelled => Err(DnsError::IncompleteResponse),
                WorkResult::Panicked => {
                    return Err(io::Error::other("DNS resolution worker panicked"))
                }
            };
            if self.stopping {
                continue;
            }
            match answer {
                Ok(candidate) => {
                    if let Err(error) = self.adoption.submit(completed.id, candidate) {
                        if error.kind() != io::ErrorKind::WouldBlock {
                            return Err(error);
                        }
                        self.fail_query(completed.id, now)?;
                    }
                }
                Err(error) => self.service.complete(completed.id, Err(error), now)?,
            }
        }
        if !self.stopping {
            for task in self.service.poll(now)? {
                let resolver = Arc::clone(&self.resolver);
                let scope = Arc::clone(&self.scope);
                let route = Arc::clone(&self.route);
                if let Err((id, error)) = self.workers.start(task, move |task, cancel| {
                    resolver.prepare_until(
                        &task.wire,
                        task.deadline,
                        Some(&cancel),
                        &scope,
                        |address| route(address),
                    )
                }) {
                    if error.kind() == io::ErrorKind::BrokenPipe {
                        return Err(error);
                    }
                    self.fail_query(id, now)?;
                }
            }
        }
        Ok(())
    }

    fn fail_query(&mut self, id: ResolutionId, now: Instant) -> io::Result<()> {
        self.service
            .complete(id, Err(DnsError::IncompleteResponse), now)
    }

    /// The caller closes transit first; keep polling until `is_finished` before
    /// releasing namespace resources or constructing a replacement owner.
    pub fn stop(&mut self) {
        self.stopping = true;
        self.workers.stop();
        self.adoption.stop();
    }
    pub fn is_finished(&self) -> bool {
        self.stopping && self.workers.is_idle() && self.adoption.is_finished()
    }
    pub fn is_faulted(&self) -> bool {
        self.faulted
    }
}

impl Drop for DnsRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}
