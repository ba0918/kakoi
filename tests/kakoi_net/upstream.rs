use kakoi_core::layers::{merge, Layer, LayerOrigin};
use kakoi_core::policy::parse_policy;
use std::path::Path;

const TLS: &str = "[[network.dns-upstream]]\ntransport='tls'\nip='192.0.2.53'\nport=853\ntls-name='ＲＥＳＯＬＶＥＲ.example.'\n";
const PLAIN: &str = "[[network.dns-upstream]]\ntransport='plain'\nip='2001:db8::53'\nport=53\n";

fn layer(text: &str) -> Layer {
    Layer {
        origin: LayerOrigin::PolicyFile("upstream.toml".into()),
        policy: parse_policy(text, Path::new("upstream.toml")).unwrap(),
    }
}

// @kotowari[REQ-146]
#[test]
fn upstream_configuration_separates_connection_address_from_tls_identity() {
    let tls = layer(TLS);
    assert_eq!(
        tls.policy.network.dns_upstream[0].tls_name(),
        Some("resolver.example")
    );
    assert_eq!(tls.policy.network.dns_upstream[0].port().get(), 853);
    for line in TLS.lines().skip(1) {
        assert!(
            parse_policy(
                &TLS.replace(&format!("{line}\n"), ""),
                Path::new("upstream.toml")
            )
            .is_err(),
            "{line}"
        );
    }
    for bad in [
        TLS.replace("853", "0"),
        TLS.replace("'192.0.2.53'", "'resolver.example'"),
        TLS.replace("'tls'", "'https'"),
        TLS.replace("'ＲＥＳＯＬＶＥＲ.example.'", "'*.example'"),
        format!("{PLAIN}tls-name='resolver.example'\n"),
    ] {
        assert!(
            parse_policy(&bad, Path::new("upstream.toml")).is_err(),
            "{bad}"
        );
    }
}

// @kotowari[REQ-146, REQ-087, EX-324, EX-325]
#[test]
fn upstream_layers_append_preserving_order_and_reject_mixed_transport() {
    let mode = layer("[network]\nmode='host'");
    let empty = layer("[network]\ndns-upstream=[]");
    assert!(merge(std::slice::from_ref(&empty)).is_err());
    let first = layer(TLS);
    let second = layer(&TLS.replace("192.0.2.53", "192.0.2.54"));
    let merged = merge(&[
        mode.clone(),
        first.clone(),
        second.clone(),
        empty,
        first.clone(),
    ])
    .unwrap();
    assert_eq!(
        merged.dns_upstream,
        [
            first.policy.network.dns_upstream[0].clone(),
            second.policy.network.dns_upstream[0].clone(),
            first.policy.network.dns_upstream[0].clone()
        ]
    );
    assert!(merge(&[mode, first, layer(PLAIN)]).is_err());
    assert!(parse_policy(&format!("{TLS}{PLAIN}"), Path::new("upstream.toml")).is_err());
}
