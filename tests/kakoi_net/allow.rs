use std::path::Path;

use kakoi_core::policy::parse_policy;

fn entry(destination: &str) -> String {
    format!("[[network.allow]]\ndestination = {{ {destination} }}\nprotocol = 'tcp'\nports = ['443', '8000-8010']\n")
}

// @kotowari[REQ-088, REQ-089, REQ-005]
#[test]
fn allow_entries_accept_each_destination_kind_and_keep_protocol_ports() {
    for destination in [
        "dns = 'api.example.com'",
        "ip = '192.0.2.1'",
        "cidr = '2001:db8::/32'",
        "host-loopback = 'ipv4'",
        "host-loopback = 'ipv6'",
    ] {
        let parsed = parse_policy(&entry(destination), Path::new("allow.toml")).unwrap();
        let rule = &parsed.network.allow[0];
        assert_eq!(rule.protocol, kakoi_core::network::Protocol::Tcp);
        assert!(rule.ports.contains(443));
        assert!(rule.ports.contains(8010));
        assert!(!rule.ports.contains(8011));
    }
}

// @kotowari[REQ-088, REQ-005, REQ-009, EX-184, EX-186, EX-009, EX-189]
#[test]
fn allow_entries_require_all_fields_and_one_destination() {
    let good = entry("ip = '192.0.2.1'");
    for line in good.lines().skip(1) {
        let bad = good.replace(&format!("{line}\n"), "");
        assert!(
            parse_policy(&bad, Path::new("allow.toml")).is_err(),
            "{bad}"
        );
    }
    for destination in [
        "",
        "ip = '192.0.2.1', dns = 'example.com'",
        "host-loopback = 'ipv4', cidr = '0.0.0.0/0'",
        "host-loopback = 'both'",
        "dns = 'example.com', host-interface = 'eth0'",
        "ip = '192.0.2.1', extra = 1",
    ] {
        let bad = entry(destination);
        assert!(
            parse_policy(&bad, Path::new("allow.toml")).is_err(),
            "{bad}"
        );
    }
    for ports in ["[443]", "[]", "['https']", "['*', '443']"] {
        let bad = good.replace("['443', '8000-8010']", ports);
        assert!(
            parse_policy(&bad, Path::new("allow.toml")).is_err(),
            "{bad}"
        );
    }
}

// @kotowari[REQ-141, EX-185]
#[test]
fn link_local_requires_an_explicit_interface_but_wide_cidr_does_not() {
    for destination in ["ip = 'fe80::1'", "cidr = 'fe80::/64'"] {
        assert!(parse_policy(&entry(destination), Path::new("allow.toml")).is_err());
        let explicit = format!("{destination}, host-interface = 'unobserved0'");
        assert!(parse_policy(&entry(&explicit), Path::new("allow.toml")).is_ok());
    }
    assert!(parse_policy(&entry("cidr = '::/0'"), Path::new("allow.toml")).is_ok());
    assert!(parse_policy(&entry("ip = 'fe80::1%eth0'"), Path::new("allow.toml")).is_err());
}
