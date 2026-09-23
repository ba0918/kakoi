use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::num::NonZeroU16;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "UpstreamInput")]
pub struct DnsUpstream {
    pub address: IpAddr,
    port: NonZeroU16,
    tls_name: Option<String>,
}

impl DnsUpstream {
    /// A plain DNS upstream chosen by the executor rather than written in a policy.
    pub fn plain(address: IpAddr, port: NonZeroU16) -> Self {
        Self {
            address,
            port,
            tls_name: None,
        }
    }

    pub fn port(&self) -> NonZeroU16 {
        self.port
    }
    pub fn tls_name(&self) -> Option<&str> {
        self.tls_name.as_deref()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Transport {
    Plain,
    Tls,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct UpstreamInput {
    transport: Transport,
    ip: String,
    port: NonZeroU16,
    tls_name: Option<String>,
}

impl TryFrom<UpstreamInput> for DnsUpstream {
    type Error = String;
    fn try_from(input: UpstreamInput) -> Result<Self, Self::Error> {
        let tls_name = match (input.transport, input.tls_name) {
            (Transport::Plain, None) => None,
            (Transport::Tls, Some(name)) => Some(super::dns_name::normalize(&name)?),
            (Transport::Plain, Some(_)) => return Err("plain DNS must not specify tls-name".into()),
            (Transport::Tls, None) => return Err("TLS DNS requires tls-name".into()),
        };
        Ok(Self {
            address: super::parse_ip(&input.ip)?,
            port: input.port,
            tls_name,
        })
    }
}

pub fn validate_upstreams(upstreams: &[DnsUpstream]) -> Result<(), String> {
    if let Some(first) = upstreams.first() {
        if upstreams
            .iter()
            .any(|item| item.tls_name.is_some() != first.tls_name.is_some())
        {
            return Err("dns-upstream must not mix plain DNS and TLS".into());
        }
    }
    Ok(())
}
