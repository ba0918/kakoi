use serde::Serialize;
use std::net::IpAddr;
use std::str::FromStr;

/// Parses the strict standard IP syntax and canonicalizes IPv4-mapped addresses.
pub fn parse_ip(input: &str) -> Result<IpAddr, String> {
    input
        .parse::<IpAddr>()
        .map(|address| address.to_canonical())
        .map_err(|_| format!("invalid IP address {input:?}"))
}

/// A canonical network prefix, with mapped IPv6 ranges represented as IPv4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct IpNetwork {
    pub(super) address: IpAddr,
    pub(super) prefix: u8,
}

fn masked(address: IpAddr, prefix: u8) -> IpAddr {
    match address {
        IpAddr::V4(address) => {
            let mask = u32::MAX.checked_shl(u32::from(32 - prefix)).unwrap_or(0);
            IpAddr::V4((u32::from(address) & mask).into())
        }
        IpAddr::V6(address) => {
            let mask = u128::MAX.checked_shl(u32::from(128 - prefix)).unwrap_or(0);
            IpAddr::V6((u128::from(address) & mask).into())
        }
    }
}

impl IpNetwork {
    pub fn address(&self) -> IpAddr {
        self.address
    }

    pub fn contains(&self, address: IpAddr) -> bool {
        let address = address.to_canonical();
        if address.is_ipv4() != self.address.is_ipv4() {
            return false;
        }
        masked(address, self.prefix) == self.address
    }
}

impl FromStr for IpNetwork {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let (address, prefix) = input
            .split_once('/')
            .ok_or_else(|| format!("CIDR {input:?} requires a prefix length"))?;
        let address = address
            .parse::<IpAddr>()
            .map_err(|_| format!("invalid CIDR address {address:?}"))?;
        if prefix.is_empty()
            || !prefix.bytes().all(|b| b.is_ascii_digit())
            || (prefix.len() > 1 && prefix.starts_with('0'))
        {
            return Err(format!("invalid CIDR prefix {prefix:?}"));
        }
        let prefix: u8 = prefix.parse().map_err(|_| "CIDR prefix too large")?;
        if prefix > if address.is_ipv4() { 32 } else { 128 } {
            return Err("CIDR prefix too large for its address family".into());
        }
        let network = masked(address, prefix);
        if network != address {
            return Err(format!(
                "CIDR has nonzero host bits; use {network}/{prefix}"
            ));
        }
        if let IpAddr::V6(ipv6) = address {
            if let Some(ipv4) = ipv6.to_ipv4_mapped() {
                if prefix >= 96 {
                    return Ok(Self {
                        address: ipv4.into(),
                        prefix: prefix - 96,
                    });
                }
            }
        }
        Ok(Self { address, prefix })
    }
}

impl std::fmt::Display for IpNetwork {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.address, self.prefix)
    }
}
