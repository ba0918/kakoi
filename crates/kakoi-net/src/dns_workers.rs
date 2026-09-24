//! Physical worker accounting, independent of expired client/request slots.
use crate::dns::{ResolutionId, ResolutionTask};
use kakoi_core::network::MAX_DNS_CONCURRENT_RESOLUTIONS;
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
};

#[derive(Clone)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub(crate) fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }
    pub(crate) fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

pub enum WorkResult<R> {
    Finished(R),
    Cancelled,
    Panicked,
}

pub struct Completion<R> {
    pub id: ResolutionId,
    pub result: WorkResult<R>,
}

struct Worker<R> {
    id: ResolutionId,
    handle: JoinHandle<R>,
    cancellation: Cancellation,
}

/// Each worker computes a candidate result; kernel adoption belongs to the
/// supervised owner after completion. `stop` invalidates all outstanding results.
/// Call `collect` until idle before releasing the session's runtime resources.
pub struct DnsWorkers<R> {
    workers: Vec<Worker<R>>,
    limit: usize,
    // Shared by the workers started since the last `retire`.
    cancellation: Cancellation,
    stopped: bool,
}

impl<R: Send + 'static> DnsWorkers<R> {
    pub fn new(limit: usize) -> Result<Self, &'static str> {
        if !(1..=MAX_DNS_CONCURRENT_RESOLUTIONS as usize).contains(&limit) {
            return Err("DNS worker limit outside supported range");
        }
        Ok(Self {
            workers: Vec::new(),
            limit,
            cancellation: Cancellation::new(),
            stopped: false,
        })
    }

    /// No waiting queue. A slot is released only after joining its finished
    /// thread, not when a request expires or a worker produces an answer.
    pub fn start(
        &mut self,
        task: ResolutionTask,
        run: impl FnOnce(ResolutionTask, Cancellation) -> R + Send + 'static,
    ) -> Result<(), (ResolutionId, io::Error)> {
        let id = task.id;
        if self.stopped {
            return Err((
                id,
                io::Error::new(io::ErrorKind::BrokenPipe, "DNS workers stopped"),
            ));
        }
        if self.workers.len() == self.limit {
            return Err((
                id,
                io::Error::new(io::ErrorKind::WouldBlock, "DNS workers at capacity"),
            ));
        }
        let cancellation = self.cancellation.clone();
        let handle = thread::Builder::new()
            .name("kakoi-dns".into())
            .spawn(move || run(task, cancellation))
            .map_err(|error| (id, error))?;
        self.workers.push(Worker {
            id,
            handle,
            cancellation: self.cancellation.clone(),
        });
        Ok(())
    }

    pub fn collect(&mut self) -> Vec<Completion<R>> {
        let mut completed = Vec::new();
        let mut index = 0;
        while index < self.workers.len() {
            if !self.workers[index].handle.is_finished() {
                index += 1;
                continue;
            }
            let worker = self.workers.swap_remove(index);
            let result = match worker.handle.join() {
                Err(_) => WorkResult::Panicked,
                Ok(_) if worker.cancellation.is_cancelled() => WorkResult::Cancelled,
                Ok(result) => WorkResult::Finished(result),
            };
            completed.push(Completion {
                id: worker.id,
                result,
            });
        }
        completed
    }
}

impl<R> DnsWorkers<R> {
    pub fn stop(&mut self) {
        self.stopped = true;
        self.cancellation.cancel();
    }

    /// Invalidates the results of every running worker; new workers still start.
    pub fn retire(&mut self) {
        self.cancellation.cancel();
        self.cancellation = Cancellation::new();
    }

    pub fn is_idle(&self) -> bool {
        self.workers.is_empty()
    }
}

impl<R> Drop for DnsWorkers<R> {
    fn drop(&mut self) {
        // Never block the health controller in a destructor. Workers retain only
        // their query/snapshot, and must observe cancellation or their deadline.
        self.stop();
    }
}
