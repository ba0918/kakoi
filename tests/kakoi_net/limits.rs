use kakoi_core::layers::{merge, Layer, LayerOrigin};
use kakoi_core::policy::parse_policy;
use std::path::Path;

fn layer(text: &str) -> Layer {
    Layer {
        origin: LayerOrigin::PolicyFile("limits.toml".into()),
        policy: parse_policy(text, Path::new("limits.toml")).unwrap(),
    }
}

// @kotowari[REQ-095, REQ-097, EX-204, EX-205, EX-206, EX-210, EX-211, EX-213, EX-119, EX-212]
#[test]
fn shutdown_grace_has_a_bounded_default_and_requires_explicit_network_mode() {
    assert_eq!(merge(&[]).unwrap().shutdown_grace_seconds, 5);
    for value in [1, 300] {
        let text = format!("[process]\nshutdown-grace-seconds={value}");
        let setting = layer(&text);
        assert!(merge(std::slice::from_ref(&setting)).is_err());
        let merged = merge(&[layer("[network]\nmode='host'"), setting]).unwrap();
        assert_eq!(merged.shutdown_grace_seconds, value);
    }
    for value in ["0", "301", "-1", "1.5", "'5'"] {
        for mode in ["", "[network]\nmode='host'\n"] {
            assert!(parse_policy(
                &format!("{mode}[process]\nshutdown-grace-seconds={value}"),
                Path::new("limits.toml")
            )
            .is_err());
        }
    }
    let merged = merge(&[
        layer("[network]\nmode='none'\n[process]\nshutdown-grace-seconds=10"),
        layer("[process]\nshutdown-grace-seconds=20"),
        layer(""),
    ])
    .unwrap();
    assert_eq!(merged.shutdown_grace_seconds, 20);
}

// @kotowari[REQ-093, REQ-117, REQ-119, REQ-121, REQ-125, REQ-128, REQ-134, REQ-143, REQ-389, EX-281, EX-282, EX-257, EX-261, EX-262, EX-267, EX-268, EX-720, EX-198, EX-199]
#[test]
fn every_network_limit_rejects_out_of_range_and_noninteger_values() {
    for (key, min, max) in [
        ("udp-idle-timeout-seconds", 1, 86400),
        ("dns-zero-ttl-grace-milliseconds", 100, 10000),
        ("dns-server-timeout-seconds", 1, 300),
        ("dns-resolution-timeout-seconds", 1, 3600),
        ("dns-max-cname-hops", 1, 128),
        ("dns-max-upstream-queries", 1, 4096),
        ("dns-max-concurrent-resolutions", 1, 4096),
        ("dns-max-waiters-per-resolution", 1, 1024),
        ("dns-failure-cache-seconds", 1, 300),
        ("recovery-attempt-timeout-seconds", 1, 300),
    ] {
        for value in [min, max] {
            let text = format!("[network]\nmode='host'\n{key}={value}");
            assert!(
                parse_policy(&text, Path::new("limits.toml")).is_ok(),
                "{text}"
            );
        }
        for value in [
            (min - 1).to_string(),
            (max + 1).to_string(),
            "-1".into(),
            "1.5".into(),
            "'10'".into(),
            "true".into(),
        ] {
            let text = format!("[network]\nmode='none'\n{key}={value}");
            assert!(
                parse_policy(&text, Path::new("limits.toml")).is_err(),
                "{text}"
            );
        }
    }
}

// @kotowari[REQ-094, REQ-117, REQ-118, REQ-087, EX-258, EX-259, EX-260, EX-180, EX-201, EX-203]
#[test]
fn limits_inherit_then_override_and_validate_the_merged_dns_deadline() {
    let defaults = merge(&[]).unwrap().network_limits;
    assert_eq!(defaults.udp_idle_timeout_seconds, 120);
    assert_eq!(defaults.dns_zero_ttl_grace_milliseconds, 1000);
    assert_eq!(defaults.dns_server_timeout_seconds, 2);
    assert_eq!(defaults.dns_resolution_timeout_seconds, 10);
    assert_eq!(defaults.dns_max_cname_hops, 16);
    assert_eq!(defaults.dns_max_upstream_queries, 64);
    assert_eq!(defaults.dns_max_concurrent_resolutions, 256);
    assert_eq!(defaults.dns_max_waiters_per_resolution, 64);
    assert_eq!(defaults.dns_failure_cache_seconds, 5);
    assert_eq!(defaults.recovery_attempt_timeout_seconds, 10);
    let lower =
        layer("[network]\nmode='none'\nudp-idle-timeout-seconds=10\ndns-server-timeout-seconds=20");
    assert!(merge(std::slice::from_ref(&lower)).is_err());
    let upper = layer("[network]\ndns-resolution-timeout-seconds=30");
    let merged = merge(&[
        lower,
        upper,
        layer("[network]\nudp-idle-timeout-seconds=15"),
    ])
    .unwrap();
    assert_eq!(merged.network_limits.udp_idle_timeout_seconds, 15);
    assert_eq!(merged.network_limits.dns_server_timeout_seconds, 20);
    assert_eq!(merged.network_limits.dns_resolution_timeout_seconds, 30);
    assert!(merge(&[layer("[network]\ndns-max-cname-hops=16")]).is_err());
}
