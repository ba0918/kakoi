use serde::{Deserialize, Serialize};

use super::{parse_ip, DnsPattern, IpFamily, IpNetwork, Ports, Protocol};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Allow {
    pub destination: Destination,
    pub protocol: Protocol,
    pub ports: Ports,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "DestinationInput")]
pub enum Destination {
    Dns(DnsPattern),
    Address {
        network: IpNetwork,
        host_interface: Option<String>,
    },
    HostLoopback(IpFamily),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct DestinationInput {
    dns: Option<String>,
    ip: Option<String>,
    cidr: Option<String>,
    host_loopback: Option<IpFamily>,
    host_interface: Option<String>,
}

impl TryFrom<DestinationInput> for Destination {
    type Error = String;

    fn try_from(input: DestinationInput) -> Result<Self, Self::Error> {
        let count = [
            input.dns.is_some(),
            input.ip.is_some(),
            input.cidr.is_some(),
            input.host_loopback.is_some(),
        ]
        .into_iter()
        .filter(|present| *present)
        .count();
        if count != 1 {
            return Err("destination requires exactly one of dns, ip, cidr, host-loopback".into());
        }
        if let Some(dns) = input.dns {
            if input.host_interface.is_some() {
                return Err("host-interface requires an IP or CIDR destination".into());
            }
            return Ok(Self::Dns(dns.parse()?));
        }
        if let Some(family) = input.host_loopback {
            if input.host_interface.is_some() {
                return Err("host-interface requires an IP or CIDR destination".into());
            }
            return Ok(Self::HostLoopback(family));
        }
        let network: IpNetwork = if let Some(ip) = input.ip {
            let address = parse_ip(&ip)?;
            IpNetwork {
                address,
                prefix: if address.is_ipv4() { 32 } else { 128 },
            }
        } else {
            input
                .cidr
                .expect("exactly one destination was checked")
                .parse()?
        };
        if let std::net::IpAddr::V6(address) = network.address {
            if address.is_unicast_link_local()
                && network.prefix >= 10
                && input.host_interface.is_none()
            {
                return Err("IPv6 link-local destination requires host-interface".into());
            }
        }
        Ok(Self::Address {
            network,
            host_interface: input.host_interface,
        })
    }
}

impl std::fmt::Display for Destination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dns(name) => write!(f, "{name}"),
            Self::HostLoopback(family) => write!(f, "host-loopback {family}"),
            Self::Address {
                network,
                host_interface,
            } => {
                write!(f, "{network}")?;
                if let Some(interface) = host_interface {
                    write!(f, " host-interface={interface}")?;
                }
                Ok(())
            }
        }
    }
}
