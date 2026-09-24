//! Validated network policy values, independent of host state and runtime tools.

/// The managed resolver inside each filtered application network namespace.
pub const DNS_RESOLVER_ADDRESS: std::net::Ipv4Addr = std::net::Ipv4Addr::new(127, 0, 0, 53);

use std::num::NonZeroU16;

use serde::{Deserialize, Serialize};

mod ports;
pub use ports::Ports;
mod dns_name;
pub use dns_name::{reserved_host, DnsPattern};
mod address;
pub use address::{parse_ip, IpNetwork};
mod allow;
pub use allow::{Allow, Destination};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IpFamily {
    #[default]
    Ipv4,
    Ipv6,
}

/// A fixed loopback publication. Both endpoints always have the same IP family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "FixedPublicationInput")]
pub struct FixedPublication {
    pub protocol: Protocol,
    pub port: NonZeroU16,
    pub host_port: NonZeroU16,
    pub family: IpFamily,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum PublicationMode {
    Fixed,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct FixedPublicationInput {
    mode: PublicationMode,
    protocol: Protocol,
    port: NonZeroU16,
    host_port: NonZeroU16,
    #[serde(default)]
    target_family: IpFamily,
    #[serde(default)]
    host_family: IpFamily,
}

impl TryFrom<FixedPublicationInput> for FixedPublication {
    type Error = String;

    fn try_from(input: FixedPublicationInput) -> Result<Self, Self::Error> {
        let PublicationMode::Fixed = input.mode;
        if input.target_family != input.host_family {
            return Err("fixed publication requires matching target-family and host-family".into());
        }
        Ok(Self {
            protocol: input.protocol,
            port: input.port,
            host_port: input.host_port,
            family: input.target_family,
        })
    }
}

/// Concatenates fixed publications, rejecting conflicting endpoint assignments.
pub fn merge_publications(
    entries: impl IntoIterator<Item = FixedPublication>,
) -> Result<Vec<FixedPublication>, String> {
    let mut merged: Vec<FixedPublication> = Vec::new();
    for entry in entries {
        if merged.contains(&entry) {
            continue;
        }
        if merged.iter().any(|existing| {
            existing.protocol == entry.protocol
                && existing.family == entry.family
                && (existing.port == entry.port || existing.host_port == entry.host_port)
        }) {
            return Err(format!(
                "conflicting `network.publish` assignment: {:?} {:?} port {} host-port {}",
                entry.protocol, entry.family, entry.port, entry.host_port
            ));
        }
        merged.push(entry);
    }
    Ok(merged)
}

mod limits;
pub use limits::{
    LimitOverrides, NetworkLimits, MAX_DNS_CONCURRENT_RESOLUTIONS,
    MAX_DNS_RESOLUTION_TIMEOUT_SECONDS, MAX_DNS_WAITERS_PER_RESOLUTION,
    MAX_RECOVERY_ATTEMPT_TIMEOUT_SECONDS, MAX_UDP_IDLE_TIMEOUT_SECONDS,
};
mod upstream;
pub use upstream::{validate_upstreams, DnsUpstream};

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Tcp => "tcp",
            Self::Udp => "udp",
        })
    }
}

impl std::fmt::Display for IpFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Ipv4 => "ipv4",
            Self::Ipv6 => "ipv6",
        })
    }
}
