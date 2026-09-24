use kakoi_core::network::DnsPattern;

// @kotowari[REQ-003, REQ-010, EX-004, EX-005, EX-006, EX-017, EX-018, EX-019]
#[test]
fn dns_patterns_match_complete_names_at_label_boundaries() {
    let exact: DnsPattern = "EXAMPLE.com.".parse().unwrap();
    let children: DnsPattern = "*.example.com".parse().unwrap();
    assert_eq!(exact.ascii_name(), "example.com");
    for (name, exact_match, child_match) in [
        ("example.com", true, false),
        ("API.Example.com.", false, true),
        ("v1.api.example.com", false, true),
        ("badexample.com", false, false),
        ("example.com.attacker.test", false, false),
        ("example", false, false),
    ] {
        assert_eq!(exact.matches(name), exact_match, "{name}");
        assert_eq!(children.matches(name), child_match, "{name}");
    }
    // No search domain completes a short name.
    let short: DnsPattern = "api".parse().unwrap();
    assert!(!short.matches("api.example.com"));
}

// @kotowari[REQ-011, EX-021]
#[test]
fn international_names_use_nontransitional_idna() {
    for (name, ascii) in [
        ("例え.テスト", "xn--r8jz45g.xn--zckzah"),
        ("ＥＸＡＭＰＬＥ.com", "example.com"),
        ("faß.de", "xn--fa-hia.de"),
    ] {
        let pattern: DnsPattern = name.parse().unwrap();
        assert_eq!(pattern.ascii_name(), ascii);
        assert!(pattern.matches(ascii));
        assert!(pattern.matches(name));
    }
}

// @kotowari[REQ-004, REQ-011, EX-007, EX-020, EX-022]
#[test]
fn invalid_names_and_nonleading_wildcards_are_rejected() {
    for name in [
        "",
        ".",
        "example.com..",
        "a..com",
        "api*.example.com",
        "api.*.com",
        "＊.example.com",
        "*",
        "*.＊.com",
        " example.com",
        "example.com ",
        "https://example.com",
        "-a.example",
        "a_.example",
        "xn--.example",
    ] {
        assert!(name.parse::<DnsPattern>().is_err(), "{name}");
    }
    assert!(format!("{}.com", "a".repeat(64))
        .parse::<DnsPattern>()
        .is_err());
}

// @kotowari[REQ-091, EX-193]
#[test]
fn reserved_host_names_require_the_host_loopback_destination() {
    for name in ["host-v4.kakoi.internal", "HOST-V6.kakoi.internal."] {
        let error = name.parse::<DnsPattern>().unwrap_err();
        assert!(error.contains("host-loopback"), "{error}");
    }
    let wildcard: DnsPattern = "*.kakoi.internal".parse().unwrap();
    assert!(!wildcard.matches("host-v4.kakoi.internal"));
    assert!(!wildcard.matches("host-v6.kakoi.internal"));
}
