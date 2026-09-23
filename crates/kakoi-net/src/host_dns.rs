//! Following the host's DNS when a policy names no upstream. Only the verified
//! configuration is followed: a host whose resolver is systemd-resolved's stub is
//! reached through resolved's proxy at 127.0.0.54, which forwards with the host's
//! own per-link settings and follows their changes. Any other host configuration
//! asks for an explicit upstream instead of guessing one.

use kakoi_core::network::DnsUpstream;
use std::{
    net::{IpAddr, Ipv4Addr},
    num::NonZeroU16,
};

const STUB: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 53));
const PROXY: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 54));

/// `text` is the host's resolver configuration (`/etc/resolv.conf`). The upstream
/// queries are sent from kakoi's own network namespace, where 127.0.0.54 is the
/// host's.
pub fn upstreams_from_resolv_conf(text: &str) -> Result<Vec<DnsUpstream>, String> {
    let nameservers: Vec<_> = text
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            (words.next() == Some("nameserver")).then(|| words.next())?
        })
        .collect();
    if nameservers.len() == 1 && nameservers[0].parse::<IpAddr>().ok() == Some(STUB) {
        Ok(vec![DnsUpstream::plain(
            PROXY,
            NonZeroU16::new(53).unwrap(),
        )])
    } else {
        Err(format!(
            "the host DNS ({}) is not systemd-resolved's stub, which is the only host \
             configuration followed; set network.dns-upstream",
            if nameservers.is_empty() {
                "no nameserver".to_owned()
            } else {
                nameservers.join(", ")
            }
        ))
    }
}
