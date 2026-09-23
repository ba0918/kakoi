//! Two owned pasta processes, prepared behind a closed transit gate.
//! This layer does not launch applications or grant policy permissions.

use crate::{
    health::HealthGate,
    namespace::NetworkNamespace,
    pasta::{Pasta, PastaStage},
};
use kakoi_core::network::FixedPublication;
use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

pub struct Transport {
    // Field order keeps namespace keepers alive until their pasta processes stop.
    inner: Option<Pasta>,
    outer: Option<Pasta>,
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
            inner: Some(inner),
            outer: Some(outer),
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

    pub(crate) fn gate_mut(&mut self) -> &mut HealthGate {
        self.closed_confirmed = false;
        &mut self.gate
    }

    pub(crate) fn close_until(&mut self, deadline: Instant) -> io::Result<()> {
        if !self.closed_confirmed {
            self.gate.close_until(deadline)?;
            self.closed_confirmed = true;
        }
        Ok(())
    }

    pub fn is_running(&mut self) -> io::Result<bool> {
        let (Some(inner), Some(outer)) = (&mut self.inner, &mut self.outer) else {
            return Ok(false);
        };
        Ok(inner.is_running()? && outer.is_running()?)
    }

    /// Replace forwarders behind the latched guard, retaining both namespace
    /// identities and the original publication mapping. Never opens transit.
    pub fn restart_closed_until(&mut self, deadline: Instant) -> io::Result<()> {
        self.restart_controlled(deadline, None)
    }

    pub(crate) fn restart_controlled(
        &mut self,
        deadline: Instant,
        cancellation: Option<&crate::dns_workers::Cancellation>,
    ) -> io::Result<()> {
        self.close_until(deadline)?;
        self.inner.take();
        self.outer.take();
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
        self.outer = Some(outer);
        self.inner = Some(inner);
        Ok(())
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        // Process teardown follows even if the explicit close cannot be confirmed.
        let _ = self.close_until(Instant::now() + Duration::from_secs(2));
    }
}
