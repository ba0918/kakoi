use super::*;
use std::thread::{self, JoinHandle};

pub(super) struct Rebuild {
    pub attempt: Attempt,
    handle: JoinHandle<Rebuilt>,
    cancellation: crate::dns_workers::Cancellation,
}
pub(super) struct Rebuilt {
    pub transport: Transport,
    pub dns: io::Result<DnsRuntime>,
    pub retired_guards: Vec<String>,
}
impl Rebuild {
    pub fn start(
        transport: Transport,
        config: DnsRuntimeConfig,
        script: String,
        route: Arc<Route>,
        attempt: Attempt,
        mut retired_guards: Vec<String>,
    ) -> io::Result<Self> {
        let cancellation = crate::dns_workers::Cancellation::new();
        let cancel = cancellation.clone();
        let handle = thread::Builder::new()
            .name("kakoi-rebuild".into())
            .spawn(move || {
                let mut transport = transport;
                let result = (|| {
                    if !retired_guards.is_empty() {
                        let cleanup: String = retired_guards
                            .iter()
                            .map(|name| {
                                format!("add table inet {name}\ndelete table inet {name}\n")
                            })
                            .collect();
                        nft::apply_cancellable(
                            transport.gate().namespace(),
                            &config.nft,
                            &cleanup,
                            attempt
                                .deadline()
                                .min(Instant::now() + Duration::from_secs(2)),
                            &cancel,
                        )?;
                        retired_guards.clear();
                    }
                    transport.restart_controlled(attempt.deadline(), Some(&cancel))?;
                    nft::apply_cancellable(
                        transport.controller_namespace(),
                        &config.nft,
                        &format!("delete table inet kakoi_policy\n{script}"),
                        attempt
                            .deadline()
                            .min(Instant::now() + Duration::from_secs(2)),
                        &cancel,
                    )?;
                    if cancel.is_cancelled() || Instant::now() >= attempt.deadline() {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "recovery deadline expired",
                        ));
                    }
                    DnsRuntime::new(transport.controller_namespace_handle(), config, move |ip| {
                        route(ip)
                    })
                })();
                Rebuilt {
                    transport,
                    dns: result,
                    retired_guards,
                }
            })?;
        Ok(Self {
            attempt,
            handle,
            cancellation,
        })
    }
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }
    pub fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }
    pub fn join(self) -> io::Result<Rebuilt> {
        self.handle
            .join()
            .map_err(|_| io::Error::other("network recovery worker panicked"))
    }
}
