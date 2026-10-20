//! Two owned pasta processes, prepared behind a closed transit gate.
//! This layer does not launch applications or grant policy permissions.

use crate::{
    health::HealthGate,
    namespace::NetworkNamespace,
    pasta::{Pasta, PastaStage},
};
use kakoi_policy::network::FixedPublication;
use std::{
    io,
    num::NonZeroU16,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

pub struct Transport {
    // Field order keeps namespace keepers alive until their pasta processes stop.
    forwarders: Option<Forwarders>,
    app: Arc<NetworkNamespace>,
    gate: HealthGate,
    closed_confirmed: bool,
    executable: PathBuf,
    publications: Vec<FixedPublication>,
}

impl Transport {
    pub fn start_closed(
        pasta: &Path,
        nft: &Path,
        publications: &[FixedPublication],
        startup_timeout: Duration,
    ) -> io::Result<Self> {
        let gate = HealthGate::create(nft)?;
        let outer = Pasta::start(
            pasta,
            None,
            gate.namespace_handle(),
            PastaStage::Outer,
            publications,
            startup_timeout,
        )?;
        let app = Arc::new(NetworkNamespace::create_within(gate.namespace())?);
        let inner = Pasta::start(
            pasta,
            Some(gate.namespace()),
            Arc::clone(&app),
            PastaStage::Inner,
            publications,
            startup_timeout,
        )?;
        Ok(Self {
            forwarders: Some(Forwarders { inner, outer }),
            app,
            gate,
            closed_confirmed: false,
            executable: pasta.to_owned(),
            publications: publications.to_vec(),
        })
    }

    /// For trusted rule installation. This is not an application execution API.
    pub fn controller_namespace(&self) -> &NetworkNamespace {
        &self.app
    }

    pub(crate) fn controller_namespace_handle(&self) -> Arc<NetworkNamespace> {
        Arc::clone(&self.app)
    }

    pub(crate) fn gate(&self) -> &HealthGate {
        &self.gate
    }

    /// Extends the health lease. A closure flushes the lease that this refills, so a
    /// closure confirmed earlier no longer stands in full and is made again next time.
    pub(crate) fn renew_for(&mut self, milliseconds: NonZeroU16) -> io::Result<()> {
        self.closed_confirmed = false;
        self.gate.renew_for(milliseconds)
    }

    /// Opens transit again after the controllers and the policy were validated; the
    /// next closure is made again.
    pub(crate) fn reopen_until(
        &mut self,
        milliseconds: NonZeroU16,
        deadline: Instant,
    ) -> io::Result<()> {
        self.closed_confirmed = false;
        self.gate.reopen_until(milliseconds, deadline)
    }

    pub(crate) fn close_until(&mut self, deadline: Instant) -> io::Result<()> {
        if !self.closed_confirmed {
            self.gate.close_until(deadline)?;
            self.closed_confirmed = true;
        }
        Ok(())
    }

    pub fn is_running(&mut self) -> io::Result<bool> {
        let Some(forwarders) = &mut self.forwarders else {
            return Ok(false);
        };
        Ok(forwarders.inner.is_running()? && forwarders.outer.is_running()?)
    }

    /// Replace forwarders behind the latched guard, retaining both namespace
    /// identities and the original publication mapping. Never opens transit.
    pub fn restart_closed_until(&mut self, deadline: Instant) -> io::Result<()> {
        self.restart_controlled(deadline, None)
    }

    pub(crate) fn restart_controlled(
        &mut self,
        deadline: Instant,
        cancellation: Option<&crate::cancellation::Cancellation>,
    ) -> io::Result<()> {
        self.close_until(deadline)?;
        self.forwarders.take();
        let remaining = || {
            if cancellation.is_some_and(|cancel| cancel.is_cancelled()) {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "transport recovery cancelled",
                ));
            }
            deadline
                .checked_duration_since(Instant::now())
                .filter(|time| !time.is_zero())
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::TimedOut,
                        "transport recovery deadline expired",
                    )
                })
        };
        let outer = Pasta::start_controlled(
            &self.executable,
            None,
            self.gate.namespace_handle(),
            PastaStage::Outer,
            &self.publications,
            remaining()?,
            cancellation,
        )?;
        let inner = Pasta::start_controlled(
            &self.executable,
            Some(self.gate.namespace()),
            Arc::clone(&self.app),
            PastaStage::Inner,
            &self.publications,
            remaining()?,
            cancellation,
        )?;
        remaining()?;
        self.forwarders = Some(Forwarders { inner, outer });
        Ok(())
    }
}

/// The two pasta processes, present together or not at all. The inner one, whose
/// namespace is inside the outer one's, stops first.
struct Forwarders {
    inner: Pasta,
    outer: Pasta,
}

impl Drop for Transport {
    fn drop(&mut self) {
        // Process teardown follows even if the explicit close cannot be confirmed.
        let _ = self.close_until(Instant::now() + crate::nft::COMMAND_BUDGET);
    }
}
