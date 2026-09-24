use std::path::Path;

use kakoi_core::policy::parse_policy;

const FIXED: &str = "mode = 'fixed'\nprotocol = 'tcp'\nport = 8000\nhost-port = 18000\n";

fn layer(text: &str) -> kakoi_core::layers::Layer {
    kakoi_core::layers::Layer {
        origin: kakoi_core::layers::LayerOrigin::PolicyFile("fixed.toml".into()),
        policy: parse_policy(text, Path::new("fixed.toml")).unwrap(),
    }
}

// @kotowari[REQ-395]
#[test]
fn publication_merge_deduplicates_and_an_empty_upper_layer_preserves_lower_entries() {
    let lower = layer(&format!("[[network.publish]]\n{FIXED}"));
    let explicit = layer(&format!(
        "[[network.publish]]\n{FIXED}target-family = 'ipv4'\nhost-family = 'ipv4'\n"
    ));
    let upper = layer("[network]\nmode = 'none'\npublish = []\n");
    let merged = kakoi_core::layers::merge(&[lower.clone(), explicit, upper]).unwrap();
    assert_eq!(merged.network_publish, lower.policy.network.publish);
    assert_eq!(merged.network_mode, kakoi_core::policy::NetworkMode::None);
}

// @kotowari[REQ-395, EX-729]
#[test]
fn publication_conflicts_are_rejected_within_and_across_layers() {
    for conflicting in [
        FIXED.replace("\nport = 8000\n", "\nport = 8001\n"),
        FIXED.replace("18000", "18001"),
    ] {
        let entry = format!("[[network.publish]]\n{FIXED}");
        let other = format!("[[network.publish]]\n{conflicting}");
        assert!(parse_policy(&format!("{entry}{other}"), Path::new("fixed.toml")).is_err());
        let error = kakoi_core::layers::merge(&[
            layer("[network]\nmode = 'host'"),
            layer(&entry),
            layer(&other),
        ])
        .unwrap_err();
        assert_eq!(error.kind(), kakoi_core::diagnostic::Kind::Policy);
    }
}

// @kotowari[REQ-395]
#[test]
fn distinct_publication_protocols_and_families_can_share_numbers() {
    let tcp = format!("[[network.publish]]\n{FIXED}");
    let udp = tcp.replace("'tcp'", "'udp'");
    let ipv6 = format!("{tcp}target-family = 'ipv6'\nhost-family = 'ipv6'\n");
    let merged = kakoi_core::layers::merge(&[
        layer("[network]\nmode = 'host'"),
        layer(&tcp),
        layer(&udp),
        layer(&ipv6),
    ])
    .unwrap();
    assert_eq!(merged.network_publish.len(), 3);
}

// @kotowari[REQ-087, EX-179, EX-181]
#[test]
fn empty_network_lists_still_require_an_explicit_mode_in_some_layer() {
    for field in ["allow", "publish"] {
        let empty = layer(&format!("[network]\n{field} = []"));
        assert!(
            kakoi_core::layers::merge(std::slice::from_ref(&empty)).is_err(),
            "{field}"
        );
        let mode = layer("[network]\nmode = 'host'");
        assert!(kakoi_core::layers::merge(&[mode.clone(), empty.clone()]).is_ok());
        assert!(kakoi_core::layers::merge(&[empty, mode]).is_ok());
    }
    assert!(kakoi_core::layers::merge(&[layer("")]).is_ok());
}

// @kotowari[REQ-092, EX-195, EX-196, EX-197]
#[test]
fn allow_merge_keeps_lower_rules_and_deduplicates_normalized_entries() {
    let first = layer("[network]\nmode='host'\n[[network.allow]]\ndestination={ip='192.0.2.1'}\nprotocol='tcp'\nports=['443']");
    let second = layer(
        "[[network.allow]]\ndestination={ip='::ffff:192.0.2.1'}\nprotocol='tcp'\nports=['443-443']",
    );
    let third =
        layer("[[network.allow]]\ndestination={ip='192.0.2.1'}\nprotocol='udp'\nports=['443']");
    let empty = layer("[network]\nallow=[]");
    let merged = kakoi_core::layers::merge(&[first.clone(), second, third.clone(), empty]).unwrap();
    assert_eq!(
        merged.network_allow,
        [
            first.policy.network.allow[0].clone(),
            third.policy.network.allow[0].clone()
        ]
    );
}

// @kotowari[REQ-394]
#[test]
fn fixed_publication_accepts_both_protocols_and_same_family_endpoints() {
    for protocol in ["tcp", "udp"] {
        for family in ["ipv4", "ipv6"] {
            for port in [1, 65535] {
                let text = format!(
                    "[network]\nmode = 'host'\n[[network.publish]]\nmode = 'fixed'\nprotocol = '{protocol}'\nport = {port}\nhost-port = {port}\ntarget-family = '{family}'\nhost-family = '{family}'\n"
                );
                let parsed = parse_policy(&text, Path::new("fixed.toml")).unwrap();
                assert_eq!(parsed.network.publish.len(), 1);
                assert_eq!(parsed.network.publish[0].port.get(), port);
                assert_eq!(parsed.network.publish[0].host_port.get(), port);
            }
        }
    }
}

// @kotowari[REQ-394]
#[test]
fn omitted_families_mean_ipv4() {
    let parse = |entry: &str| {
        parse_policy(
            &format!("[[network.publish]]\n{entry}"),
            Path::new("fixed.toml"),
        )
        .unwrap()
        .network
        .publish
    };
    assert_eq!(
        parse(FIXED),
        parse(&format!(
            "{FIXED}target-family = 'ipv4'\nhost-family = 'ipv4'\n"
        ))
    );
}

// @kotowari[REQ-394, EX-727, EX-728, EX-171]
#[test]
fn fixed_publication_rejects_unsupported_forms_even_when_inactive() {
    let mut invalid = vec![
        FIXED.replace("mode = 'fixed'\n", ""),
        FIXED.replace("'fixed'", "'dynamic'"),
        FIXED.replace("'tcp'", "'sctp'"),
        format!("{FIXED}host-family = 'both'\n"),
        format!("{FIXED}host-family = 'ipv6'\n"),
        format!("{FIXED}target-family = 'ipv6'\n"),
        format!("{FIXED}unknown = true\n"),
    ];
    for field in ["port", "host-port"] {
        let old = if field == "port" {
            "port = 8000\n"
        } else {
            "host-port = 18000\n"
        };
        invalid.push(FIXED.replace(old, ""));
        for value in ["0", "65536", "-1", "1.5", "'8000'", "'8000-8001'", "true"] {
            invalid.push(FIXED.replace(old, &format!("{field} = {value}\n")));
        }
    }
    for mode in ["host", "none"] {
        for entry in &invalid {
            let text = format!("[network]\nmode = '{mode}'\n[[network.publish]]\n{entry}");
            assert!(
                parse_policy(&text, Path::new("fixed.toml")).is_err(),
                "{text}"
            );
        }
    }
}
