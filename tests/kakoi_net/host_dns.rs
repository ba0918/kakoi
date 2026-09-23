use kakoi_net::host_dns::upstreams_from_resolv_conf;

fn addresses(text: &str) -> Vec<String> {
    upstreams_from_resolv_conf(text)
        .unwrap()
        .iter()
        .map(|upstream| {
            assert_eq!(upstream.port().get(), 53);
            assert!(upstream.tls_name().is_none());
            upstream.address.to_string()
        })
        .collect()
}

// @kotowari[REQ-396, EX-731]
#[test]
fn the_host_nameservers_are_the_upstreams_in_their_order() {
    assert_eq!(
        addresses("nameserver 10.255.255.254\nsearch example.test\nnameserver 192.0.2.53\n"),
        ["10.255.255.254", "192.0.2.53"]
    );
    assert_eq!(
        addresses("# comment\nnameserver ::1\noptions ndots:2\nnameserver 127.0.0.53\n"),
        ["::1", "127.0.0.53"]
    );
}

// @kotowari[EX-732]
#[test]
fn the_systemd_resolved_stub_alone_is_reached_through_its_proxy() {
    assert_eq!(
        addresses("nameserver 127.0.0.53\noptions edns0 trust-ad\nsearch .\n"),
        ["127.0.0.54"]
    );
}

// @kotowari[REQ-145, EX-322]
#[test]
fn a_host_configuration_without_a_nameserver_asks_for_an_explicit_upstream() {
    for text in ["search example.test\n", "", "nameserver not-an-address\n"] {
        let error = upstreams_from_resolv_conf(text).unwrap_err();
        assert!(error.contains("network.dns-upstream"), "{error}");
    }
}
