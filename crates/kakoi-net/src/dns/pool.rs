use super::{DnsError, Question};
use std::{collections::HashMap, hash::Hash};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResolutionKey {
    generation: u64,
    wire: Vec<u8>,
}

impl Question {
    pub fn resolution_key(&self, generation: u64) -> Result<ResolutionKey, DnsError> {
        let mut message = self.message.clone();
        // A message authenticator can bind the exact question bytes. Keep those
        // distinctions for signed queries instead of sharing a rewritten request.
        if message.signature.is_none() {
            message.metadata.id = 0;
            for query in &mut message.queries {
                query.set_name(query.name().to_lowercase());
            }
        }
        Ok(ResolutionKey {
            generation,
            wire: message.to_vec().map_err(|_| DnsError::Malformed)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResolutionId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Start(ResolutionId),
    Join(ResolutionId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapacityError {
    Resolutions,
    Waiters,
    IdsExhausted,
}

struct Pending<W> {
    id: ResolutionId,
    waiters: Vec<W>,
}

/// Owned by one environment's controller. Keys must include name, type, class,
/// DNS options and resolver settings generation; authorize each waiter before
/// admission. Only `Start` creates a worker and its time/query budget. Joining
/// retains the existing worker, so it cannot reset that budget.
pub struct ResolutionPool<K, W> {
    pending: HashMap<K, Pending<W>>,
    max_resolutions: usize,
    max_waiters: usize,
    next_id: u64,
}

impl<K: Eq + Hash, W> ResolutionPool<K, W> {
    pub fn new(max_resolutions: usize, max_waiters: usize) -> Result<Self, &'static str> {
        if !(1..=4096).contains(&max_resolutions) || !(1..=1024).contains(&max_waiters) {
            return Err("DNS capacity limits outside supported range");
        }
        Ok(Self {
            pending: HashMap::new(),
            max_resolutions,
            max_waiters,
            next_id: 0,
        })
    }

    pub fn admit(&mut self, key: K, waiter: W) -> Result<Admission, (CapacityError, W)> {
        if let Some(pending) = self.pending.get_mut(&key) {
            if pending.waiters.len() == self.max_waiters {
                return Err((CapacityError::Waiters, waiter));
            }
            pending.waiters.push(waiter);
            return Ok(Admission::Join(pending.id));
        }
        if self.pending.len() == self.max_resolutions {
            return Err((CapacityError::Resolutions, waiter));
        }
        let Some(next) = self.next_id.checked_add(1) else {
            return Err((CapacityError::IdsExhausted, waiter));
        };
        let id = ResolutionId(self.next_id);
        self.next_id = next;
        self.pending.insert(
            key,
            Pending {
                id,
                waiters: vec![waiter],
            },
        );
        Ok(Admission::Start(id))
    }

    /// A stale completion cannot remove a newer resolution for the same key.
    pub fn complete(&mut self, id: ResolutionId) -> Vec<W> {
        let mut waiters = Vec::new();
        self.pending.retain(|_, pending| {
            if pending.id == id {
                waiters = std::mem::take(&mut pending.waiters);
                false
            } else {
                true
            }
        });
        waiters
    }
}
