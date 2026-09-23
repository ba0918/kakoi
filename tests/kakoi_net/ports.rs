use kakoi_core::network::Ports;

fn parse(items: &[&str]) -> Result<Ports, String> {
    Ports::try_from(
        items
            .iter()
            .map(|item| (*item).to_owned())
            .collect::<Vec<_>>(),
    )
}

// @kotowari[REQ-005, REQ-007, REQ-008]
#[test]
fn port_union_is_normalized_and_includes_both_endpoints() {
    let ports = parse(&["8005-8020", "443", "8000-8010", "443-443", "1", "65535"]).unwrap();
    for port in 0..=u16::MAX {
        assert_eq!(
            ports.contains(port),
            matches!(port, 1 | 443 | 8000..=8020 | 65535),
            "{port}"
        );
    }
    assert_eq!(ports, parse(&["1", "443", "8000-8020", "65535"]).unwrap());
    assert_eq!(parse(&["1-2", "3"]).unwrap(), parse(&["1-3"]).unwrap());
    let all = parse(&["*"]).unwrap();
    assert!(!all.contains(0));
    assert!((1..=u16::MAX).all(|port| all.contains(port)));
}

// @kotowari[REQ-006, REQ-007, REQ-009]
#[test]
fn port_input_rejects_ambiguous_empty_or_invalid_ranges() {
    for items in [vec![], vec!["*", "443"], vec!["*", "*"]] {
        assert!(parse(&items).is_err(), "{items:?}");
    }
    for item in [
        "0",
        "65536",
        "-1",
        "+443",
        "0443",
        " 443",
        "443 ",
        "https",
        "",
        "8010-8000",
        "0-1",
        "1-65536",
        "1-02",
        "1 -2",
        "1--2",
        "１",
        "1-2-3",
    ] {
        assert!(parse(&[item]).is_err(), "{item}");
    }
}
