//! Following the host's DNS when a policy names no upstream: the `nameserver`
//! entries of the host's resolver configuration, in their order, as plain DNS
//! upstreams. A host that names only systemd-resolved's stub is reached through
//! resolved's proxy at 127.0.0.54, which forwards without rewriting answers.

use crate::resolution::UpstreamWait;
use kakoi_core::network::DnsUpstream;
use std::{
    net::{IpAddr, Ipv4Addr},
    num::NonZeroU16,
};

/// Read again when it changes; the executor compares contents, not timestamps.
pub const RESOLV_CONF: &str = "/etc/resolv.conf";

const STUB: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 53));
const PROXY: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 54));
const PORT: NonZeroU16 = NonZeroU16::new(53).unwrap();

/// `text` is the host's resolver configuration. The upstream queries are sent
/// from kakoi's own network namespace, so loopback nameservers are the host's.
/// Entries that are not an IP address are skipped, as the C library does.
pub fn upstreams_from_resolv_conf(text: &str) -> Result<Vec<DnsUpstream>, String> {
    let nameservers: Vec<IpAddr> = text
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            (words.next() == Some("nameserver")).then(|| words.next()?.parse().ok())?
        })
        .collect();
    if nameservers.is_empty() {
        return Err(format!(
            "the host DNS configuration ({RESOLV_CONF}) names no nameserver; set network.dns-upstream"
        ));
    }
    let nameservers = if nameservers == [STUB] {
        vec![PROXY]
    } else {
        nameservers
    };
    Ok(nameservers
        .into_iter()
        .map(|address| DnsUpstream::plain(address, PORT))
        .collect())
}

/// systemd-resolved's proxy chooses among the host's servers by itself, so it is
/// given the whole resolution; nameservers kakoi tries in turn each get one
/// candidate's wait.
pub fn wait_for(upstreams: &[DnsUpstream]) -> UpstreamWait {
    match upstreams {
        [only] if only.address == PROXY && only.port() == PORT => UpstreamWait::HostResolver,
        _ => UpstreamWait::Explicit,
    }
}
