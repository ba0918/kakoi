//! Finite kernel permission for the intermediate namespace in the two-pasta topology.
//! Application loopback lives in a different namespace and never crosses this gate.

use crate::namespace::NetworkNamespace;
use std::io;
use std::num::NonZeroU16;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const TRANSIT_INTERFACE: &str = "middle0";

// Independent of the renewable lease: an in-flight renewal cannot undo closure.
pub(crate) const CLOSE_RULES: &str = "table inet kakoi_guard {\n\
    chain input { type filter hook input priority -200; policy drop; }\n\
    chain output { type filter hook output priority -200; policy drop; }\n\
    chain forward { type filter hook forward priority -200; policy drop; }\n\
    }\n";

pub struct HealthGate {
    namespace: Arc<NetworkNamespace>,
    nft: PathBuf,
}

impl HealthGate {
    pub fn create(nft: impl AsRef<Path>) -> io::Result<Self> {
        let gate = Self {
            namespace: Arc::new(NetworkNamespace::create()?),
            nft: nft.as_ref().to_owned(),
        };
        gate.batch("table inet kakoi_health {\n\
            set live { type ifname; flags timeout; }\n\
            chain input { type filter hook input priority -100; policy drop; iifname @live accept; }\n\
            chain output { type filter hook output priority -100; policy drop; oifname @live accept; }\n\
            chain forward { type filter hook forward priority -100; policy drop; }\n\
            }\n")?;
        Ok(gate)
    }

    /// Start an independent monitor; send heartbeats only after verifying the
    /// controller and inspect its acknowledgements before renewing health.
    pub fn watchdog(&self, timeout: Duration) -> io::Result<crate::namespace::TransitWatchdog> {
        crate::namespace::TransitWatchdog::start(Arc::clone(&self.namespace), &self.nft, timeout)
    }

    /// Trusted transit controllers only; application processes must not enter here.
    pub fn namespace(&self) -> &NetworkNamespace {
        &self.namespace
    }

    pub(crate) fn namespace_handle(&self) -> Arc<NetworkNamespace> {
        Arc::clone(&self.namespace)
    }

    /// Renew only while the supervisor has verified all enforcement components.
    /// This is a relative health lease, not a DNS permission deadline.
    /// The supported health window is 1000..=65535 milliseconds. DNS grants use
    /// separately staged/read-back subsecond elements; an active health set must
    /// never receive a timeout that could round to zero kernel ticks.
    pub fn renew_for(&mut self, milliseconds: NonZeroU16) -> io::Result<()> {
        self.batch(&renewal_script(milliseconds)?)
    }

    /// The caller must first replace/verify failed controllers and policy behind
    /// the guard. Routine heartbeats never invoke this operation.
    pub fn reopen_after_validation(&mut self, milliseconds: NonZeroU16) -> io::Result<()> {
        self.reopen_until(milliseconds, Instant::now() + Duration::from_secs(2))
    }

    pub(crate) fn reopen_until(
        &mut self,
        milliseconds: NonZeroU16,
        deadline: Instant,
    ) -> io::Result<()> {
        crate::nft::apply(
            &self.namespace,
            &self.nft,
            &format!(
                "add table inet kakoi_guard\n{}delete table inet kakoi_guard\n",
                renewal_script(milliseconds)?,
            ),
            deadline,
        )
    }

    pub fn close(&mut self) -> io::Result<()> {
        self.close_until(Instant::now() + Duration::from_secs(2))
    }

    /// Use the controller's absolute closure deadline, without starting another
    /// timeout on a retry. An error leaves closure unconfirmed; the supervisor
    /// must not preserve application execution on that result alone.
    pub fn close_until(&mut self, deadline: Instant) -> io::Result<()> {
        crate::nft::apply(
            &self.namespace,
            &self.nft,
            &format!("{CLOSE_RULES}flush set inet kakoi_health live\n"),
            deadline,
        )
    }

    fn batch(&self, script: &str) -> io::Result<()> {
        crate::nft::apply(
            &self.namespace,
            &self.nft,
            script,
            Instant::now() + Duration::from_secs(2),
        )
    }
}

fn renewal_script(milliseconds: NonZeroU16) -> io::Result<String> {
    if milliseconds.get() < 1000 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "health lease must be at least 1000 milliseconds",
        ));
    }
    Ok(format!(
            "flush set inet kakoi_health live\n\
             add element inet kakoi_health live {{ \"lo\" timeout {milliseconds}ms, \"{TRANSIT_INTERFACE}\" timeout {milliseconds}ms }}\n"
        ))
}
