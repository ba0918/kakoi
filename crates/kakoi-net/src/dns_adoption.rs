//! One kernel permission owner, separate from the controller's health loop.
use crate::{
    dns::{DnsError, EnforcedDnsError, PreparedAnswer, ResolutionId},
    dns_workers::Cancellation,
    dynamic::DynamicPermissions,
    namespace::NetworkNamespace,
};
use kakoi_core::network::Allow;
use std::{
    io,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
        Arc,
    },
    thread::{self, JoinHandle},
};

pub struct AdoptionCompletion {
    pub id: ResolutionId,
    pub answer: Result<Vec<u8>, EnforcedDnsError>,
}

type Candidate = (ResolutionId, PreparedAnswer);

pub struct DnsAdoption {
    sender: Option<SyncSender<Candidate>>,
    receiver: Receiver<AdoptionCompletion>,
    handle: Option<JoinHandle<()>>,
    cancellation: Cancellation,
    faulted: Arc<AtomicBool>,
    outstanding: usize,
    limit: usize,
}

impl DnsAdoption {
    /// The policy table must already exist. This owner exclusively updates its
    /// DNS chain until stopped and joined; do not construct a second owner for it.
    pub fn new(
        namespace: Arc<NetworkNamespace>,
        nft: PathBuf,
        policy: Vec<Allow>,
        limit: usize,
    ) -> io::Result<Self> {
        if !(1..=4096).contains(&limit) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid DNS adoption capacity",
            ));
        }
        let (sender, requests) = mpsc::sync_channel::<Candidate>(limit);
        let (results, receiver) = mpsc::sync_channel(limit);
        let cancellation = Cancellation::new();
        let cancel = cancellation.clone();
        let faulted = Arc::new(AtomicBool::new(false));
        let fault = Arc::clone(&faulted);
        let handle = thread::Builder::new()
            .name("kakoi-dns-adopt".into())
            .spawn(move || {
                let mut permissions = DynamicPermissions::new(&namespace, &nft, policy);
                while let Ok((id, candidate)) = requests.recv() {
                    let answer = if fault.load(Ordering::Acquire) {
                        Err(EnforcedDnsError::Enforcement(io::Error::other(
                            "DNS permission executor faulted",
                        )))
                    } else {
                        candidate.adopt_controlled(&mut permissions, Some(&cancel))
                    };
                    if matches!(answer, Err(EnforcedDnsError::Enforcement(_))) {
                        fault.store(true, Ordering::Release);
                    }
                    // Outstanding includes unread results, so this bounded channel
                    // always has a slot for each accepted request's one completion.
                    if results.send(AdoptionCompletion { id, answer }).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            receiver,
            handle: Some(handle),
            cancellation,
            faulted,
            outstanding: 0,
            limit,
        })
    }

    /// Capacity covers queued, active and completed-but-uncollected work together.
    /// A rejected candidate is dropped without refreshing any kernel permission.
    pub fn submit(&mut self, id: ResolutionId, candidate: PreparedAnswer) -> io::Result<()> {
        if self.cancellation.is_cancelled() || self.is_faulted() || self.sender.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "DNS adoption is stopped or faulted",
            ));
        }
        if self.outstanding == self.limit {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "DNS adoption is at capacity",
            ));
        }
        match self
            .sender
            .as_ref()
            .expect("checked sender")
            .try_send((id, candidate))
        {
            Ok(()) => {
                self.outstanding += 1;
                Ok(())
            }
            Err(TrySendError::Full(_)) => Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "DNS adoption queue is full",
            )),
            Err(TrySendError::Disconnected(_)) => {
                self.faulted.store(true, Ordering::Release);
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "DNS adoption worker disconnected",
                ))
            }
        }
    }

    pub fn poll(&mut self) -> io::Result<Vec<AdoptionCompletion>> {
        let mut completed = Vec::new();
        loop {
            match self.receiver.try_recv() {
                Ok(mut result) => {
                    self.outstanding -= 1;
                    if self.cancellation.is_cancelled() && result.answer.is_ok() {
                        result.answer = Err(EnforcedDnsError::Query(DnsError::IncompleteResponse));
                    }
                    completed.push(result);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if !self.cancellation.is_cancelled() {
                        self.faulted.store(true, Ordering::Release);
                    }
                    break;
                }
            }
        }
        if self.handle.as_ref().is_some_and(JoinHandle::is_finished) {
            let result = self.handle.take().expect("finished handle").join();
            if result.is_err() || !self.cancellation.is_cancelled() {
                self.outstanding = 0;
                self.faulted.store(true, Ordering::Release);
                return Err(io::Error::other("DNS adoption worker exited unexpectedly"));
            }
        }
        Ok(completed)
    }

    pub fn is_faulted(&self) -> bool {
        self.faulted.load(Ordering::Acquire)
    }
    pub fn is_finished(&self) -> bool {
        self.handle.is_none() && self.outstanding == 0
    }

    /// Close the session transit gate first. Active nft operations are bounded
    /// but not forcibly interrupted here; poll until finished before replacement.
    pub fn stop(&mut self) {
        self.cancellation.cancel();
        self.sender.take();
    }
}

impl Drop for DnsAdoption {
    fn drop(&mut self) {
        self.stop();
    }
}
