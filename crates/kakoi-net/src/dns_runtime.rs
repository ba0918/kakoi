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
    resolution::QueryAllowance,
    scope::AddressContext,
};
use kakoi_core::network::{Allow, DnsUpstream, NetworkLimits};
use std::{
    collections::{HashMap, HashSet},
    io,
    net::IpAddr,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

/// How often the host's DNS configuration is read again while it is followed.
const FOLLOW_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone)]
pub struct DnsRuntimeConfig {
    pub policy: Vec<Allow>,
    pub upstreams: Vec<DnsUpstream>,
    pub limits: NetworkLimits,
    pub trust: Option<TlsClient>,
    pub nft: PathBuf,
    pub scope: AddressContext,
    pub generation: u64,
    /// Upstreams that follow the host's DNS configuration rather than a policy.
    pub host_dns: Option<HostDns>,
}

/// The host's DNS configuration and how it names upstreams. A change of its
/// contents replaces `upstreams`; contents that cannot be read or give no
/// upstream fail every question until they can, and never fall back.
#[derive(Clone)]
pub struct HostDns {
    pub path: PathBuf,
    pub parse: fn(&str) -> Result<Vec<DnsUpstream>, String>,
}

struct Following {
    source: HostDns,
    text: Option<String>,
    next: Instant,
}

pub struct DnsRuntime {
    service: DnsService,
    resolver: Arc<ExplicitResolver>,
    policy: Vec<Allow>,
    limits: NetworkLimits,
    trust: Option<TlsClient>,
    following: Option<Following>,
    // Questions handed to a worker, kept to ask again under new settings with
    // the same deadline and what is left of the same query count.
    inflight: HashMap<ResolutionId, (Vec<u8>, Instant, QueryAllowance)>,
    // Workers whose settings were replaced; their results are never used.
    retired: HashSet<ResolutionId>,
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
        .map_err(io::Error::other)?
        .with_failure_hold(Duration::from_secs(u64::from(
            config.limits.dns_failure_cache_seconds,
        )));
        let resolver = Arc::new(ExplicitResolver::new(
            config.policy.clone(),
            config.upstreams,
            config.limits.clone(),
            config.trust.clone(),
        )?);
        let following = config.host_dns.map(|source| Following {
            text: std::fs::read_to_string(&source.path).ok(),
            source,
            next: Instant::now() + FOLLOW_INTERVAL,
        });
        let policy = config.policy.clone();
        let workers = DnsWorkers::new(limit).map_err(io::Error::other)?;
        let front = DnsFront::new(DnsSockets::bind(Arc::clone(&namespace))?, limit, timeout)?;
        let adoption = DnsAdoption::new(namespace, config.nft, config.policy, limit)?;
        Ok(Self {
            service: DnsService::new(front, requests),
            resolver,
            policy,
            limits: config.limits,
            trust: config.trust,
            following,
            inflight: HashMap::new(),
            retired: HashSet::new(),
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

    /// Reads the host's DNS configuration when it is due. A changed content takes
    /// effect for every question from now on, including those already asked.
    fn follow(&mut self, now: Instant) -> io::Result<()> {
        let Some(following) = &mut self.following else {
            return Ok(());
        };
        if now < following.next {
            return Ok(());
        }
        following.next = now + FOLLOW_INTERVAL;
        let text = std::fs::read_to_string(&following.source.path).ok();
        if text == following.text {
            return Ok(());
        }
        let upstreams = text
            .as_deref()
            .and_then(|text| (following.source.parse)(text).ok())
            .unwrap_or_default();
        following.text = text;
        self.resolver = Arc::new(ExplicitResolver::new(
            self.policy.clone(),
            upstreams,
            self.limits.clone(),
            self.trust.clone(),
        )?);
        self.service.advance_generation();
        self.retired.extend(self.inflight.keys().copied());
        self.workers.retire();
        Ok(())
    }

    fn dispatch(
        &mut self,
        id: ResolutionId,
        wire: Vec<u8>,
        deadline: Instant,
        queries: QueryAllowance,
        now: Instant,
    ) -> io::Result<()> {
        if now >= deadline {
            return self.fail_query(id, now);
        }
        let resolver = Arc::clone(&self.resolver);
        let scope = Arc::clone(&self.scope);
        let route = Arc::clone(&self.route);
        let shared = queries.clone();
        let task = crate::dns::ResolutionTask {
            id,
            wire: wire.clone(),
            deadline,
        };
        match self.workers.start(task, move |task, cancel| {
            resolver.prepare_sharing(
                &task.wire,
                task.deadline,
                &shared,
                Some(&cancel),
                &scope,
                |address| route(address),
            )
        }) {
            Ok(()) => {
                self.inflight.insert(id, (wire, deadline, queries));
                Ok(())
            }
            Err((_, error)) if error.kind() == io::ErrorKind::BrokenPipe => Err(error),
            Err(_) => self.fail_busy(id, now),
        }
    }

    fn poll_inner(&mut self, now: Instant) -> io::Result<()> {
        self.follow(now)?;
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
            let task = self.inflight.remove(&completed.id);
            if self.retired.remove(&completed.id) && !self.stopping {
                // Asked under replaced settings: ask again, within the same deadline.
                if let (Some((wire, deadline, queries)), false) =
                    (task, matches!(completed.result, WorkResult::Panicked))
                {
                    self.service.rekey(completed.id);
                    self.dispatch(completed.id, wire, deadline, queries, now)?;
                    continue;
                }
            }
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
                        self.fail_busy(completed.id, now)?;
                    }
                }
                Err(error) => self.service.complete(completed.id, Err(error), now)?,
            }
        }
        if !self.stopping {
            for task in self.service.poll(now)? {
                let queries = QueryAllowance::new(&self.limits);
                self.dispatch(task.id, task.wire, task.deadline, queries, now)?;
            }
        }
        Ok(())
    }

    fn fail_query(&mut self, id: ResolutionId, now: Instant) -> io::Result<()> {
        self.service
            .complete(id, Err(DnsError::IncompleteResponse), now)
    }

    /// Fails a question for want of room, which is not held as a failure.
    fn fail_busy(&mut self, id: ResolutionId, now: Instant) -> io::Result<()> {
        self.service.complete(id, Err(DnsError::Overloaded), now)
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
