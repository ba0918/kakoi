//! Kernel enforcement for resolved, static IP permissions. Host alias routing and
//! DNS-derived permissions are separate from this compilation layer.

use kakoi_core::network::{IpNetwork, Ports, Protocol};
use std::fmt::Write;

pub struct FilterRule {
    pub network: IpNetwork,
    pub protocol: Protocol,
    pub ports: Ports,
}

/// Install before application execution. The caller must resolve policy address
/// scopes and host aliases before producing these static permissions.
pub fn compile_static(rules: &[FilterRule], udp_idle_seconds: u32) -> Result<String, String> {
    if !(1..=86400).contains(&udp_idle_seconds) {
        return Err("UDP idle timeout must be in 1..=86400 seconds".into());
    }
    let mut script = format!(
        r#"table inet kakoi_policy {{
 ct timeout udp4 {{ protocol udp; l3proto ip; policy = {{ unreplied: {udp_idle_seconds}, replied: {udp_idle_seconds} }}; }}
 ct timeout udp6 {{ protocol udp; l3proto ip6; policy = {{ unreplied: {udp_idle_seconds}, replied: {udp_idle_seconds} }}; }}
 chain input {{ type filter hook input priority 0; policy drop;
  ct state invalid drop
  iifname "lo" meta l4proto {{ tcp, udp }} accept
  ip6 hoplimit 255 icmpv6 type {{ nd-router-advert, nd-neighbor-solicit, nd-neighbor-advert }} accept
  ct state related meta l4proto {{ icmp, ipv6-icmp }} accept
  ct mark 1 meta l4proto {{ tcp, udp }} accept
 }}
 chain forward {{ type filter hook forward priority 0; policy drop; }}
 chain output {{ type filter hook output priority 0; policy drop;
  ct state invalid drop
  oifname "lo" meta l4proto {{ tcp, udp }} accept
  ip6 hoplimit 255 icmpv6 type {{ nd-router-solicit, nd-neighbor-solicit, nd-neighbor-advert }} accept
  ct state related meta l4proto {{ icmp, ipv6-icmp }} accept
  ip daddr {{ 224.0.0.0/4, 255.255.255.255 }} drop
  ip6 daddr ff00::/8 drop
  ct mark 1 meta l4proto tcp ct state established accept
  ct mark 1 meta l4proto udp accept
  jump permitted
 }}
 chain permitted {{
"#
    );
    for rule in rules {
        let (family, timeout) = if rule.network.address().is_ipv4() {
            ("ip", "udp4")
        } else {
            ("ip6", "udp6")
        };
        let _ = write!(
            script,
            "  {family} daddr {} {} dport {{ {} }} ",
            rule.network, rule.protocol, rule.ports
        );
        if rule.protocol == Protocol::Udp {
            let _ = write!(script, "ct timeout set \"{timeout}\" ");
        }
        script.push_str("ct mark set 1 accept\n");
    }
    script.push_str("  jump dns_permitted\n }\n chain dns_permitted { }\n}\n");
    Ok(script)
}
