use kakoi_core::{network::Allow, policy::parse_policy};
use kakoi_net::scope::{AddressContext, AddressScope, DnsAdmission};
use std::path::Path;

fn rule(destination: &str, protocol: &str, ports: &str) -> Allow {
    parse_policy(&format!("[[network.allow]]\ndestination = {{{destination}}}\nprotocol = '{protocol}'\nports = [{ports}]"), Path::new("scope.toml")).unwrap().network.allow.remove(0)
}

// @kotowari[REQ-043, REQ-140, REQ-031]
#[test]
fn iana_ranges_and_mapped_addresses_keep_their_scope_without_public_anycast_exceptions() {
    let context = AddressContext::default();
    for ip in [
        "10.1.2.3",
        "100.64.0.1",
        "172.16.0.1",
        "192.168.1.1",
        "169.254.0.1",
        "192.0.0.9",
        "192.31.196.1",
        "198.18.0.1",
        "fc00::1",
        "fe80::1",
        "2001:db8::1",
        "64:ff9b::808:808",
        "3fff::1",
        "5f00::1",
        "::ffff:192.168.1.1",
    ] {
        assert_eq!(
            context.classify(ip.parse().unwrap()),
            AddressScope::Explicit,
            "{ip}"
        );
    }
    for ip in [
        "1.1.1.1",
        "8.8.8.8",
        "2606:4700:4700::1111",
        "::ffff:8.8.8.8",
    ] {
        assert_eq!(
            context.classify(ip.parse().unwrap()),
            AddressScope::Global,
            "{ip}"
        );
    }
    for ip in ["127.1.2.3", "::1", "::ffff:127.0.0.1"] {
        assert_eq!(
            context.classify(ip.parse().unwrap()),
            AddressScope::Loopback
        );
    }
    for ip in ["224.0.0.1", "ff02::1", "255.255.255.255"] {
        assert_eq!(
            context.classify(ip.parse().unwrap()),
            AddressScope::Unsupported
        );
    }
}

// @kotowari[REQ-043, REQ-139, REQ-140]
#[test]
fn special_dns_results_use_existing_ip_permissions_without_expanding_their_ports() {
    let context = AddressContext {
        host_addresses: vec!["8.8.8.8".parse().unwrap()],
        ..Default::default()
    };
    let origin = rule("dns='api.example.com'", "tcp", "'443','8443'");
    let private = "192.168.1.5".parse().unwrap();
    assert_eq!(
        context.admit_dns(&origin, private, None, &[]),
        DnsAdmission::Denied
    );
    let explicit = rule("cidr='192.168.1.0/24'", "tcp", "'8443'");
    assert_eq!(
        context.admit_dns(&origin, private, None, &[explicit]),
        DnsAdmission::ExistingOnly
    );
    for wrong in [
        rule("cidr='192.168.1.0/24'", "udp", "'443'"),
        rule("cidr='192.168.1.0/24'", "tcp", "'80'"),
    ] {
        assert_eq!(
            context.admit_dns(&origin, private, None, &[wrong]),
            DnsAdmission::Denied
        );
    }
    assert_eq!(
        context.admit_dns(&origin, "8.8.8.8".parse().unwrap(), None, &[]),
        DnsAdmission::Denied
    );
    assert_eq!(
        context.admit_dns(&origin, "1.1.1.1".parse().unwrap(), None, &[]),
        DnsAdmission::Dynamic
    );
}

// @kotowari[REQ-032, REQ-043]
#[test]
fn broad_cidrs_do_not_authorize_host_aliases_or_unscoped_link_local_routes() {
    let context = AddressContext {
        host_loopback_v4: Some("169.254.10.1".parse().unwrap()),
        ..Default::default()
    };
    let origin = rule("dns='api.example.com'", "tcp", "'443'");
    let broad = vec![
        rule("cidr='0.0.0.0/0'", "tcp", "'*'"),
        rule("cidr='::/0'", "tcp", "'*'"),
    ];
    let alias = "169.254.10.1".parse().unwrap();
    assert_eq!(
        context.admit_dns(&origin, alias, None, &broad),
        DnsAdmission::Denied
    );
    assert_eq!(
        context.admit_dns(
            &origin,
            alias,
            None,
            &[rule("host-loopback='ipv4'", "tcp", "'443'")]
        ),
        DnsAdmission::ExistingOnly
    );
    let link = "fe80::1".parse().unwrap();
    assert_eq!(
        context.admit_dns(&origin, link, Some("eth0"), &broad),
        DnsAdmission::Denied
    );
    let scoped = vec![rule("ip='fe80::1', host-interface='eth0'", "tcp", "'443'")];
    assert_eq!(
        context.admit_dns(&origin, link, Some("eth0"), &scoped),
        DnsAdmission::ExistingOnly
    );
    assert_eq!(
        context.admit_dns(&origin, link, Some("eth1"), &scoped),
        DnsAdmission::Denied
    );
    assert_eq!(
        context.admit_dns(&origin, link, None, &scoped),
        DnsAdmission::Denied
    );
}

// A DNS answer alone never opens an internal, shared, or special-purpose
// address, in whatever notation; an explicit rule for all of IPv4 does.
// @kotowari[EX-077, EX-078, EX-080, EX-081, EX-084, EX-312]
#[test]
fn internal_and_special_answers_need_an_explicit_rule() {
    let context = AddressContext::default();
    let origin = rule("dns='api.example.com'", "tcp", "'443'");
    for ip in [
        "::ffff:192.168.1.10",
        "100.64.0.1",
        "fd00::1",
        "198.18.0.1",
        "192.0.0.9",
    ] {
        assert_eq!(
            context.admit_dns(&origin, ip.parse().unwrap(), None, &[]),
            DnsAdmission::Denied,
            "{ip}"
        );
    }
    let explicit = rule("ip='192.168.1.10'", "tcp", "'443'");
    assert_eq!(
        context.admit_dns(
            &origin,
            "::ffff:192.168.1.10".parse().unwrap(),
            None,
            &[explicit]
        ),
        DnsAdmission::ExistingOnly
    );
    let everything = rule("cidr='0.0.0.0/0'", "tcp", "'443'");
    assert_eq!(
        context.admit_dns(
            &origin,
            "192.168.1.10".parse().unwrap(),
            None,
            &[everything]
        ),
        DnsAdmission::ExistingOnly
    );
}
