//! DNS address admission from immutable policy and controller-observed addresses.

use kakoi_core::network::{Allow, Destination, IpFamily, IpNetwork};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    sync::OnceLock,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressScope {
    Global,
    Explicit,
    Loopback,
    HostLoopback(IpFamily),
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsAdmission {
    Dynamic,
    /// Return the IP using existing static/loopback permissions, without adding a
    /// DNS grant that could enlarge their protocol, port or interface scope.
    ExistingOnly,
    Denied,
}

/// Supplied by the trusted controller, never by the application. Host aliases are
/// virtual non-loopback addresses routed to the corresponding host loopback.
#[derive(Debug, Default, Clone)]
pub struct AddressContext {
    pub host_addresses: Vec<IpAddr>,
    pub host_loopback_v4: Option<Ipv4Addr>,
    pub host_loopback_v6: Option<Ipv6Addr>,
    pub directed_broadcasts: Vec<Ipv4Addr>,
}

impl AddressContext {
    pub fn classify(&self, address: IpAddr) -> AddressScope {
        let address = address.to_canonical();
        if address.is_multicast()
            || matches!(address, IpAddr::V4(ip) if ip.is_broadcast() || self.directed_broadcasts.contains(&ip))
        {
            return AddressScope::Unsupported;
        }
        if self.host_loopback_v4.map(IpAddr::V4) == Some(address) {
            return AddressScope::HostLoopback(IpFamily::Ipv4);
        }
        if self.host_loopback_v6.map(IpAddr::V6) == Some(address) {
            return AddressScope::HostLoopback(IpFamily::Ipv6);
        }
        if address.is_loopback() {
            return AddressScope::Loopback;
        }
        if self
            .host_addresses
            .iter()
            .any(|host| host.to_canonical() == address)
            || special_networks()
                .iter()
                .any(|network| network.contains(address))
        {
            AddressScope::Explicit
        } else {
            AddressScope::Global
        }
    }

    /// `origin` is the DNS rule whose name already matched the authorized query.
    /// `host_interface` identifies a verified, currently available host route.
    pub fn admit_dns(
        &self,
        origin: &Allow,
        address: IpAddr,
        host_interface: Option<&str>,
        policy: &[Allow],
    ) -> DnsAdmission {
        if !matches!(origin.destination, Destination::Dns(_)) {
            return DnsAdmission::Denied;
        }
        let address = address.to_canonical();
        let scope = self.classify(address);
        match scope {
            AddressScope::Global => return DnsAdmission::Dynamic,
            AddressScope::Loopback => return DnsAdmission::ExistingOnly,
            AddressScope::Unsupported => return DnsAdmission::Denied,
            _ => {}
        }
        let explicit = policy.iter().any(|rule| {
            if rule.protocol != origin.protocol || !rule.ports.overlaps(&origin.ports) {
                return false;
            }
            match (&rule.destination, scope) {
                (Destination::HostLoopback(family), AddressScope::HostLoopback(expected)) => {
                    *family == expected
                }
                (
                    Destination::Address {
                        network,
                        host_interface: required,
                    },
                    AddressScope::Explicit,
                ) => {
                    if matches!(address, IpAddr::V6(ip) if ip.is_unicast_link_local())
                        && required.is_none()
                    {
                        return false;
                    }
                    network.contains(address)
                        && required
                            .as_deref()
                            .is_none_or(|required| Some(required) == host_interface)
                }
                _ => false,
            }
        });
        if explicit {
            DnsAdmission::ExistingOnly
        } else {
            DnsAdmission::Denied
        }
    }
}

fn special_networks() -> &'static [IpNetwork] {
    static NETWORKS: OnceLock<Vec<IpNetwork>> = OnceLock::new();
    NETWORKS.get_or_init(|| {
        include_str!("../data/iana-ipv4-special.txt")
            .lines()
            .map(|line| (line, false))
            .chain(
                include_str!("../data/iana-ipv6-special.txt")
                    .lines()
                    .map(|line| (line, true)),
            )
            .filter_map(|(line, ipv6)| {
                let network: IpNetwork = line.parse().expect("bundled IANA prefix is valid");
                // Mapped IPv6 is classified as IPv4 before lookup. Converting its
                // registry /96 to IPv4 /0 here would misclassify every IPv4 address.
                if ipv6 && network.address().is_ipv4() {
                    None
                } else {
                    Some(network)
                }
            })
            .collect()
    })
}
