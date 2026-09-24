use kakoi_core::network::{parse_ip, IpNetwork};

// @kotowari[REQ-044, REQ-048, REQ-050, REQ-051, EX-088, EX-089, EX-090, EX-096, EX-097]
#[test]
fn ip_input_is_strict_and_mapped_addresses_normalize_to_ipv4() {
    assert_eq!(
        parse_ip("::ffff:192.0.2.1").unwrap(),
        parse_ip("192.0.2.1").unwrap()
    );
    assert_eq!(
        parse_ip("2001:DB8:0000::1").unwrap(),
        parse_ip("2001:db8::1").unwrap()
    );
    for input in [
        "127.1",
        "2130706433",
        "0x7f000001",
        "0127.0.0.1",
        "192.168.01.1",
        "::ffff:192.168.01.1",
        "[::1]",
        "[::1]:80",
        "127.0.0.1:80",
        " 127.0.0.1",
        "::1 ",
        "fe80::1%eth0",
        "https://example.com",
        "192.0.2.0/24",
    ] {
        assert!(parse_ip(input).is_err(), "{input}");
    }
}

// @kotowari[REQ-045, REQ-046, REQ-047, REQ-049, EX-082, EX-083, EX-086, EX-087, EX-092, EX-093]
#[test]
fn cidr_requires_canonical_network_bits_and_normalizes_mapped_ranges() {
    let mapped: IpNetwork = "::ffff:192.0.2.0/120".parse().unwrap();
    assert_eq!(mapped, "192.0.2.0/24".parse().unwrap());
    assert!(mapped.contains(parse_ip("192.0.2.255").unwrap()));
    assert!(!mapped.contains(parse_ip("192.0.3.0").unwrap()));
    for input in [
        "0.0.0.0/0",
        "::/0",
        "192.0.2.1/32",
        "2001:db8::1/128",
        "::ffff:0:0/96",
    ] {
        assert!(input.parse::<IpNetwork>().is_ok(), "{input}");
    }
    for input in [
        "192.0.2.0/33",
        "::/129",
        "::/",
        "::/-1",
        "::/+1",
        "::/01",
        "::/ 1",
        "::/1 ",
        "192.0.2.0",
        "::ffff:192.0.2.1/120",
    ] {
        assert!(input.parse::<IpNetwork>().is_err(), "{input}");
    }
    let error = "192.0.2.1/24".parse::<IpNetwork>().unwrap_err();
    assert!(error.contains("192.0.2.0/24"), "{error}");
    let error = "2001:db8::1/64".parse::<IpNetwork>().unwrap_err();
    assert!(error.contains("2001:db8::/64"), "{error}");
}

// One destination, however it is written: a mapped IPv4 address meets the
// IPv4 rule, an expanded IPv6 rule meets the compressed destination, and a
// /128 holds one address only.
// @kotowari[EX-080, EX-091, EX-094, EX-095]
#[test]
fn a_destination_meets_the_same_rule_whatever_its_notation() {
    let ipv4: IpNetwork = "192.168.1.10/32".parse().unwrap();
    assert!(ipv4.contains(parse_ip("::ffff:192.168.1.10").unwrap()));
    assert!(ipv4.contains("::ffff:192.168.1.10".parse().unwrap()));
    let expanded: IpNetwork = format!(
        "{}/128",
        parse_ip("FD00:0000:0000:0000:0000:0000:0000:0001").unwrap()
    )
    .parse()
    .unwrap();
    assert!(expanded.contains(parse_ip("fd00::1").unwrap()));
    let single: IpNetwork = "fd00::1/128".parse().unwrap();
    assert!(single.contains(parse_ip("fd00::1").unwrap()));
    assert!(!single.contains(parse_ip("fd00::2").unwrap()));
    assert!(!single.contains(parse_ip("fd00::").unwrap()));
    assert!(parse_ip("fd00::1::2").is_err());
    assert!("fd00::1::2/128".parse::<IpNetwork>().is_err());
}
