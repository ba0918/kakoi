//! Owned network controllers. Unsafe requires immediate application termination.
//! Explicit shutdown closes transit, then drains workers before releasing this owner.
mod rebuild;
use crate::{
    dns_runtime::{DnsRuntime, DnsRuntimeConfig},
    filter::{self, FilterRule},
    namespace::{NetworkNamespace, TransitWatchdog, WatchdogState},
    nft,
    notification::{NetworkState, NotificationWriter, Notifications},
    recovery::{Attempt, Recovery, RecoveryAction},
    transport::Transport,
};
use rebuild::Rebuild;
use std::{
    io,
    net::IpAddr,
    num::NonZeroU16,
    sync::Arc,
    time::{Duration, Instant},
};

const WATCHDOG_TIMEOUT: Duration = Duration::from_secs(3);
const HEALTH_INTERVAL: Duration = Duration::from_secs(1);
const HEALTH_WINDOW_MS: NonZeroU16 = NonZeroU16::new(5000).unwrap();
type Route = dyn Fn(IpAddr) -> Option<String> + Send + Sync;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Prepared,
    Running,
    Isolated,
    Unsafe,
}

pub struct Session {
    dns: Option<DnsRuntime>,
    watchdog: Option<TransitWatchdog>,
    transport: Option<Transport>,
    namespace: Arc<NetworkNamespace>,
    config: DnsRuntimeConfig,
    script: String,
    route: Arc<Route>,
    recovery: Recovery,
    rebuilding: Option<Rebuild>,
    retired_guards: Vec<String>,
    automatic: bool,
    state: SessionState,
    next_health: Instant,
    notifications: Notifications,
}
impl Session {
    /// The trusted caller resolves static policy addresses and interface scopes
    /// into `rules`. Recovery retains this exact policy; no application input is read.
    pub fn prepare(
        transport: Transport,
        config: DnsRuntimeConfig,
        rules: &[FilterRule],
        route: impl Fn(IpAddr) -> Option<String> + Send + Sync + 'static,
    ) -> io::Result<Self> {
        let recovery = Recovery::new(config.limits.recovery_attempt_timeout_seconds)
            .map_err(io::Error::other)?;
        let script = filter::compile_static(rules, config.limits.udp_idle_timeout_seconds)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        nft::apply(
            transport.controller_namespace(),
            &config.nft,
            &script,
            Instant::now() + Duration::from_secs(2),
        )?;
        let namespace = transport.controller_namespace_handle();
        let route: Arc<Route> = Arc::new(route);
        let resolver_route = Arc::clone(&route);
        let dns = DnsRuntime::new(Arc::clone(&namespace), config.clone(), move |ip| {
            resolver_route(ip)
        })?;
        let watchdog = transport.gate().watchdog(WATCHDOG_TIMEOUT)?;
        Ok(Self {
            dns: Some(dns),
            watchdog: Some(watchdog),
            transport: Some(transport),
            namespace,
            config,
            script,
            route,
            recovery,
            rebuilding: None,
            retired_guards: Vec::new(),
            automatic: false,
            state: SessionState::Prepared,
            next_health: Instant::now(),
            notifications: Notifications::default(),
        })
    }
    pub fn namespace(&self) -> &NetworkNamespace {
        &self.namespace
    }
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Pull one bounded line without performing output I/O in the control loop.
    pub fn take_notification(&mut self) -> Option<Arc<str>> {
        self.notifications.pop()
    }

    /// Queues an event of the environment that is not a network state change.
    pub fn notice(&mut self, text: &str) {
        self.notifications.notice(text);
    }

    pub fn pending_notifications(&self) -> usize {
        self.notifications.pending()
    }

    /// Notification loss never becomes a network fault. The caller retains one
    /// writer for this session, normally targeting the application's stderr.
    pub fn flush_notifications(&mut self, writer: &mut NotificationWriter) {
        self.notifications.flush_to(writer);
    }

    /// Initial activation only; runtime failures use the original recovery policy.
    pub fn activate(&mut self) -> io::Result<()> {
        if self.state != SessionState::Prepared {
            return Err(io::Error::other(
                "session is not prepared for initial activation",
            ));
        }
        let result = self.verify().and_then(|()| self.refresh());
        match result {
            Ok(()) => {
                self.state = SessionState::Running;
                self.notifications.update(NetworkState::Running, "ready");
                Ok(())
            }
            Err(error) => Err(self.fail(error)),
        }
    }
    pub fn poll(&mut self) -> io::Result<()> {
        if matches!(self.state, SessionState::Isolated | SessionState::Unsafe) {
            return self.poll_isolated();
        }
        let result = (|| {
            self.verify()?;
            self.dns.as_mut().expect("live DNS").poll(Instant::now())?;
            if self.state == SessionState::Running && Instant::now() >= self.next_health {
                self.refresh()?;
            } else {
                self.watchdog.as_mut().expect("live watchdog").heartbeat()?;
            }
            Ok(())
        })();
        result.map_err(|error| self.fail(error))
    }
    fn verify(&mut self) -> io::Result<()> {
        if !self
            .transport
            .as_mut()
            .ok_or_else(|| io::Error::other("transport unavailable"))?
            .is_running()?
            || self.dns.as_ref().is_none_or(|dns| dns.is_faulted())
        {
            return Err(io::Error::other("network controller stopped"));
        }
        match self
            .watchdog
            .as_mut()
            .ok_or_else(|| io::Error::other("watchdog unavailable"))?
            .poll()?
        {
            WatchdogState::Healthy => Ok(()),
            state => Err(io::Error::other(format!("transit watchdog is {state:?}"))),
        }
    }
    fn refresh(&mut self) -> io::Result<()> {
        self.watchdog.as_mut().expect("live watchdog").heartbeat()?;
        self.transport
            .as_mut()
            .expect("live transport")
            .gate_mut()
            .renew_for(HEALTH_WINDOW_MS)?;
        self.verify()?;
        self.next_health = Instant::now() + HEALTH_INTERVAL;
        Ok(())
    }
    fn fail(&mut self, cause: io::Error) -> io::Error {
        let recover = self.state == SessionState::Running;
        let error = match self.close_until(Instant::now() + Duration::from_secs(2)) {
            Ok(()) => {
                self.automatic = recover;
                if recover {
                    self.recovery.isolate(Instant::now());
                }
                cause
            }
            Err(closure) => {
                io::Error::other(format!("{cause}; transit closure unconfirmed: {closure}"))
            }
        };
        self.report_failure(&error);
        error
    }

    fn report_failure(&mut self, error: &io::Error) {
        let state = if self.state == SessionState::Unsafe {
            NetworkState::Unsafe
        } else {
            NetworkState::Isolated
        };
        self.notifications.update(state, &error.to_string());
    }
    fn poll_isolated(&mut self) -> io::Result<()> {
        if let Some(dns) = &mut self.dns {
            // A stopped owner's failure must not prevent collecting its remaining work.
            let result = dns.poll(Instant::now());
            if !dns.is_finished() {
                return result;
            }
        }
        if self.rebuilding.as_ref().is_some_and(Rebuild::is_finished) {
            return self.finish_rebuild();
        }
        if !self.automatic || self.state == SessionState::Unsafe {
            return Ok(());
        }
        match self.recovery.poll(Instant::now()) {
            RecoveryAction::Start(attempt) => {
                self.dns.take();
                let transport = self.transport.take().expect("no overlapping recovery");
                match Rebuild::start(
                    transport,
                    self.config.clone(),
                    self.script.clone(),
                    Arc::clone(&self.route),
                    attempt,
                    std::mem::take(&mut self.retired_guards),
                ) {
                    Ok(work) => self.rebuilding = Some(work),
                    Err(error) => {
                        self.state = SessionState::Unsafe;
                        self.automatic = false;
                        self.report_failure(&error);
                        return Err(error);
                    }
                }
            }
            // All rebuild operations share this absolute deadline; keep the slot
            // occupied until their owner returns, even after cancellation is due.
            RecoveryAction::Cancel(_) => {
                if let Some(work) = &self.rebuilding {
                    work.cancel();
                }
            }
            RecoveryAction::Wait => {}
        }
        Ok(())
    }
    fn finish_rebuild(&mut self) -> io::Result<()> {
        let work = self.rebuilding.take().expect("finished recovery");
        let attempt = work.attempt;
        let rebuilt = match work.join() {
            Ok(result) => result,
            Err(error) => {
                self.state = SessionState::Unsafe;
                self.automatic = false;
                self.report_failure(&error);
                return Err(error);
            }
        };
        self.transport = Some(rebuilt.transport);
        self.retired_guards = rebuilt.retired_guards;
        let mut outcome = rebuilt.dns.map(|dns| self.dns = Some(dns));
        if outcome.is_ok() && self.automatic && Instant::now() < attempt.deadline() {
            outcome = (|| {
                self.watchdog = Some(
                    self.transport
                        .as_ref()
                        .expect("returned transport")
                        .gate()
                        .watchdog(WATCHDOG_TIMEOUT)?,
                );
                self.verify()?;
                self.transport
                    .as_mut()
                    .expect("returned transport")
                    .gate_mut()
                    .reopen_until(
                        HEALTH_WINDOW_MS,
                        attempt
                            .deadline()
                            .min(Instant::now() + Duration::from_secs(2)),
                    )?;
                self.verify()
            })();
        } else if outcome.is_ok() {
            outcome = Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "recovery result is no longer admissible",
            ));
        }
        let recovered = self
            .recovery
            .complete(attempt, outcome.is_ok(), Instant::now())
            .map_err(io::Error::other)?;
        if recovered {
            self.state = SessionState::Running;
            self.notifications.update(NetworkState::Running, "restored");
            self.next_health = Instant::now() + HEALTH_INTERVAL;
            Ok(())
        } else {
            let retry = self.automatic;
            if let Err(error) = self.close_until(Instant::now() + Duration::from_secs(2)) {
                self.report_failure(&error);
                return Err(error);
            }
            self.automatic = retry;
            let error = outcome.err().unwrap_or_else(|| {
                io::Error::new(io::ErrorKind::TimedOut, "recovery deadline expired")
            });
            if retry {
                self.report_failure(&error);
            }
            Err(error)
        }
    }
    /// Explicit shutdown disables automatic recovery. Transit is already latched
    /// closed while a rebuild owns it; that worker never opens the gate.
    pub fn close_until(&mut self, deadline: Instant) -> io::Result<()> {
        self.automatic = false;
        if let Some(work) = &self.rebuilding {
            work.cancel();
        }
        let closed = match &mut self.transport {
            Some(transport) => transport.close_until(deadline),
            None if self.rebuilding.is_some() && self.state == SessionState::Isolated => Ok(()),
            None => Err(io::Error::other("transit owner unavailable")),
        };
        self.state = if closed.is_ok() {
            SessionState::Isolated
        } else {
            SessionState::Unsafe
        };
        if let Some(dns) = &mut self.dns {
            dns.stop();
        }
        if closed.is_ok() {
            if let Some(watchdog) = self.watchdog.take() {
                self.retired_guards.push(watchdog.retire());
            }
        }
        closed
    }
    pub fn is_drained(&self) -> bool {
        !self.automatic
            && self.rebuilding.is_none()
            && self.dns.as_ref().is_none_or(DnsRuntime::is_finished)
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.close_until(Instant::now() + Duration::from_secs(2));
    }
}
