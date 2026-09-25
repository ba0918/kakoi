//! Two-phase DNS permissions. Relative kernel timeouts are staged in unreachable
//! sets, then exposed only after their application latency has been bounded.

use crate::{
    filter,
    leases::{ActiveGrant, LeaseBook},
    namespace::NetworkNamespace,
    nft,
};
use kakoi_core::network::{Allow, Destination};
use std::{
    collections::BTreeMap,
    fmt::Write,
    io,
    net::IpAddr,
    path::Path,
    time::{Duration, Instant},
};

pub struct DynamicPermissions<'ns> {
    namespace: &'ns NetworkNamespace,
    nft: &'ns Path,
    rules: Vec<Allow>,
    leases: LeaseBook,
    sets: Vec<String>,
    generation: u64,
    failed: bool,
}

impl<'ns> DynamicPermissions<'ns> {
    pub fn requires_reconciliation(&self) -> bool {
        self.failed
    }

    pub(crate) fn matches_policy(&self, policy: &[Allow]) -> bool {
        self.rules == policy
    }

    /// One owner per installed kakoi_policy table. Any error that poisons this
    /// owner requires supervisor isolation/reconciliation before another update.
    pub fn new(namespace: &'ns NetworkNamespace, nft: &'ns Path, rules: Vec<Allow>) -> Self {
        Self {
            namespace,
            nft,
            rules,
            leases: LeaseBook::default(),
            sets: Vec::new(),
            generation: 0,
            failed: false,
        }
    }

    pub fn install(&mut self, grants: &[ActiveGrant]) -> io::Result<()> {
        self.stage(grants)?.activate()
    }

    /// Stages `grants` in sets no rule refers to yet. A staging that cannot finish
    /// within its reserve is discarded and retried with a doubled reserve while
    /// the grants leave room; the owner stays usable when the kernel state is known.
    pub fn stage(&mut self, grants: &[ActiveGrant]) -> io::Result<PreparedGrants<'_, 'ns>> {
        if self.failed {
            return Err(io::Error::other(
                "DNS permission owner needs reconciliation",
            ));
        }
        let mut cap = INITIAL_RESERVE;
        loop {
            let started = Instant::now();
            let room = self.room(grants, started)?;
            let reserve = room.min(cap);
            let attempt = self.plan(grants, started, reserve)?;
            match self.apply_staging(&attempt, started + reserve) {
                Ok(()) => {
                    return Ok(PreparedGrants {
                        owner: self,
                        leases: Some(attempt.leases),
                        sets: attempt.sets,
                        activation: attempt.activation,
                        activated: false,
                    })
                }
                Err(Staging::Fault(error)) => {
                    self.failed = true;
                    return Err(error);
                }
                Err(Staging::Late) => {
                    if let Err(error) = discard_sets(self.namespace, self.nft, &attempt.discard) {
                        self.failed = true;
                        return Err(error);
                    }
                    // Another attempt only helps when it may use a longer reserve.
                    let room = self.room(grants, Instant::now()).unwrap_or_default();
                    if room <= reserve {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "DNS staging reserve exceeded",
                        ));
                    }
                    cap = reserve * 2;
                }
            }
        }
    }

    /// Half the time left to the earliest grant: the longest reserve that still
    /// leaves the permission a lifetime of its own.
    fn room(&self, grants: &[ActiveGrant], now: Instant) -> io::Result<Duration> {
        for grant in grants {
            if !self
                .rules
                .get(grant.rule)
                .is_some_and(|rule| matches!(rule.destination, Destination::Dns(_)))
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid DNS rule",
                ));
            }
            if grant.deadline <= now {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "expired grant"));
            }
        }
        Ok(grants
            .iter()
            .map(|grant| grant.deadline.duration_since(now) / 2)
            .min()
            .unwrap_or(INITIAL_RESERVE))
    }

    fn plan(
        &mut self,
        grants: &[ActiveGrant],
        started: Instant,
        reserve: Duration,
    ) -> io::Result<Attempt> {
        let mut leases = self.leases.clone();
        for grant in grants {
            leases = leases.record(grant.rule, grant.address, grant.deadline, started);
        }
        let active = leases.active(started);
        // A nearly expired old lease must not turn an unrelated fresh answer
        // into a controller fault. Old entries that cannot survive this staging
        // window are omitted below, never reinserted with a fresh lifetime.
        let deadline = started + reserve;
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| io::Error::other("DNS generation exhausted"))?;
        let mut grouped = BTreeMap::new();
        for grant in active {
            let remaining = grant
                .deadline
                .saturating_duration_since(deadline)
                .as_millis();
            if remaining == 0 {
                continue;
            }
            grouped
                .entry((grant.rule, grant.address.is_ipv4()))
                .or_insert_with(Vec::new)
                .push((grant.address, remaining));
        }
        let mut stage = String::new();
        let mut discard = String::new();
        let mut activation = String::from("flush chain inet kakoi_policy dns_permitted\n");
        let mut sets = Vec::new();
        let mut expected_elements = Vec::new();
        for ((index, ipv4), elements) in grouped {
            let datatype = if ipv4 { "ipv4_addr" } else { "ipv6_addr" };
            let set = format!(
                "dns_{}_{}_{}",
                self.generation,
                index,
                if ipv4 { 4 } else { 6 }
            );
            let declaration =
                format!("add set inet kakoi_policy {set} {{ type {datatype}; flags timeout; }}\n");
            expected_elements.push((
                set.clone(),
                elements.iter().map(|(address, _)| *address).collect(),
            ));
            stage.push_str(&declaration);
            // Whether or not a late staging committed, this removes the set.
            discard.push_str(&declaration);
            writeln!(discard, "delete set inet kakoi_policy {set}").unwrap();
            for (address, milliseconds) in elements {
                writeln!(
                    stage,
                    "add element inet kakoi_policy {set} {{ {address} timeout {milliseconds}ms }}"
                )
                .unwrap();
            }
            let rule = &self.rules[index];
            write!(
                activation,
                "add rule inet kakoi_policy dns_permitted {} daddr @{set} ",
                filter::family(ipv4)
            )
            .unwrap();
            filter::write_permit(&mut activation, rule.protocol, &rule.ports, ipv4);
            sets.push(set);
        }
        Ok(Attempt {
            leases,
            sets,
            expected_elements,
            stage,
            discard,
            activation,
        })
    }

    /// The relative kernel timeouts were computed from `deadline`; staging that
    /// ends later could outlive an answer, so it is late, not a fault.
    fn apply_staging(&self, attempt: &Attempt, deadline: Instant) -> Result<(), Staging> {
        if attempt.stage.is_empty() {
            return Ok(());
        }
        let late = |error: io::Error| {
            if error.kind() == io::ErrorKind::TimedOut {
                Staging::Late
            } else {
                Staging::Fault(error)
            }
        };
        nft::apply(self.namespace, self.nft, &attempt.stage, deadline).map_err(late)?;
        let json =
            nft::inspect(self.namespace, self.nft, "kakoi_policy", deadline).map_err(late)?;
        verify_expirations(&json, &attempt.expected_elements).map_err(Staging::Fault)?;
        if Instant::now() > deadline {
            return Err(Staging::Late);
        }
        Ok(())
    }
}

/// The first reserve for staging; doubled on each late attempt.
const INITIAL_RESERVE: Duration = Duration::from_millis(50);

struct Attempt {
    leases: LeaseBook,
    sets: Vec<String>,
    expected_elements: Vec<(String, Vec<IpAddr>)>,
    stage: String,
    discard: String,
    activation: String,
}

enum Staging {
    Late,
    Fault(io::Error),
}

fn discard_sets(namespace: &NetworkNamespace, executable: &Path, script: &str) -> io::Result<()> {
    if script.is_empty() {
        return Ok(());
    }
    nft::apply(
        namespace,
        executable,
        script,
        Instant::now() + Duration::from_secs(2),
    )
}

fn verify_expirations(json: &[u8], sets: &[(String, Vec<IpAddr>)]) -> io::Result<()> {
    let invalid = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "DNS candidate lacks a verified kernel permission or expiration",
        )
    };
    let value: serde_json::Value = serde_json::from_slice(json).map_err(io::Error::other)?;
    let entries = value["nftables"].as_array().ok_or_else(invalid)?;
    for (name, expected) in sets {
        let set = entries
            .iter()
            .filter_map(|entry| entry.get("set"))
            .find(|set| set["name"].as_str() == Some(name.as_str()))
            .ok_or_else(invalid)?;
        let elements = set
            .get("elem")
            .and_then(|value| value.as_array())
            .ok_or_else(invalid)?;
        if elements.len() != expected.len() {
            return Err(invalid());
        }
        let mut actual = Vec::with_capacity(elements.len());
        for element in elements {
            // nft JSON reports whole seconds, so zero is a valid subsecond
            // remainder. Absence means no expiration extension exists at all.
            if element["elem"]["expires"].as_u64().is_none() {
                return Err(invalid());
            }
            let address = element["elem"]["val"]
                .as_str()
                .and_then(|value| value.parse::<IpAddr>().ok())
                .ok_or_else(invalid)?;
            actual.push(address);
        }
        actual.sort_unstable();
        let mut expected = expected.clone();
        expected.sort_unstable();
        if actual != expected {
            return Err(invalid());
        }
    }
    Ok(())
}

/// Holding this token exclusively borrows the permission owner: another update
/// cannot race staging/activation or cause a set name to be reused.
pub struct PreparedGrants<'owner, 'ns> {
    owner: &'owner mut DynamicPermissions<'ns>,
    leases: Option<LeaseBook>,
    sets: Vec<String>,
    activation: String,
    activated: bool,
}

impl PreparedGrants<'_, '_> {
    pub fn activate(mut self) -> io::Result<()> {
        if let Err(error) = nft::apply(
            self.owner.namespace,
            self.owner.nft,
            &self.activation,
            Instant::now() + Duration::from_secs(2),
        ) {
            // The kernel may have committed despite losing the acknowledgement.
            self.owner.failed = true;
            return Err(error);
        }
        self.activated = true;
        self.owner.leases = self.leases.take().expect("single activation");
        let previous = std::mem::replace(&mut self.owner.sets, std::mem::take(&mut self.sets));
        if let Err(error) = remove_sets(self.owner.namespace, self.owner.nft, &previous) {
            self.owner.failed = true;
            return Err(error);
        }
        Ok(())
    }
}

impl Drop for PreparedGrants<'_, '_> {
    fn drop(&mut self) {
        if !self.activated
            && !self.owner.failed
            && remove_sets(self.owner.namespace, self.owner.nft, &self.sets).is_err()
        {
            self.owner.failed = true;
        }
    }
}

fn remove_sets(namespace: &NetworkNamespace, executable: &Path, sets: &[String]) -> io::Result<()> {
    if sets.is_empty() {
        return Ok(());
    }
    let script: String = sets
        .iter()
        .map(|set| format!("delete set inet kakoi_policy {set}\n"))
        .collect();
    nft::apply(
        namespace,
        executable,
        &script,
        Instant::now() + Duration::from_secs(2),
    )
}

#[cfg(test)]
mod tests {
    use super::verify_expirations;
    use std::net::IpAddr;

    #[test]
    fn an_empty_kernel_set_cannot_confirm_a_staged_dns_permission() {
        let readback = br#"{"nftables":[{"set":{"name":"dns_1_0_4"}}]}"#;
        let error = verify_expirations(readback, &[("dns_1_0_4".into(), addresses(&["1.1.1.1"]))])
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn a_partially_populated_kernel_set_cannot_confirm_all_staged_permissions() {
        let readback = br#"{"nftables":[{"set":{"name":"dns_1_0_4","elem":[{"elem":{"val":"1.1.1.1","expires":10}}]}}]}"#;
        let error = verify_expirations(
            readback,
            &[("dns_1_0_4".into(), addresses(&["1.1.1.1", "2.2.2.2"]))],
        )
        .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn all_staged_permissions_with_finite_expiration_are_confirmed() {
        let readback = br#"{"nftables":[{"set":{"name":"dns_1_0_4","elem":[{"elem":{"val":"1.1.1.1","expires":0}},{"elem":{"val":"2.2.2.2","expires":10}}]}}]}"#;
        verify_expirations(
            readback,
            &[("dns_1_0_4".into(), addresses(&["1.1.1.1", "2.2.2.2"]))],
        )
        .unwrap();
    }

    #[test]
    fn an_unexpected_kernel_address_does_not_confirm_the_staged_permission() {
        let readback = br#"{"nftables":[{"set":{"name":"dns_1_0_4","elem":[{"elem":{"val":"2.2.2.2","expires":10}}]}}]}"#;
        let error = verify_expirations(readback, &[("dns_1_0_4".into(), addresses(&["1.1.1.1"]))])
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("permission"));
    }

    fn addresses(values: &[&str]) -> Vec<IpAddr> {
        values.iter().map(|value| value.parse().unwrap()).collect()
    }
}
