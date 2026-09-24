use kakoi_net::leases::{record_deadline, LeaseBook};
use std::time::{Duration, Instant};

// @kotowari[REQ-014, REQ-022, REQ-389]
#[test]
fn dns_chain_deadline_uses_each_reception_time_and_zero_ttl_grace_only_for_zero() {
    let received = Instant::now();
    let grace = Duration::from_millis(1000);
    let cname = record_deadline(received, 2, grace).unwrap();
    let address = record_deadline(received + Duration::from_millis(1500), 0, grace).unwrap();
    assert_eq!(cname.min(address), received + Duration::from_secs(2));
    assert_eq!(record_deadline(received, 0, grace), Some(received + grace));
    assert_eq!(
        record_deadline(received, 1, Duration::from_secs(10)),
        Some(received + Duration::from_secs(1))
    );
}

// @kotowari[REQ-014, REQ-133, REQ-389, EX-300]
#[test]
fn old_dns_grants_keep_their_deadline_and_rules_do_not_revoke_each_other() {
    let start = Instant::now();
    let ip = "192.0.2.1".parse().unwrap();
    let other = "192.0.2.2".parse().unwrap();
    let original = LeaseBook::default().record(0, ip, start + Duration::from_secs(30), start);
    let updated = original
        .record(0, other, start + Duration::from_secs(10), start)
        .record(1, ip, start + Duration::from_secs(60), start)
        .record(0, ip, start + Duration::from_secs(1), start);
    assert!(updated.permits(0, ip, start + Duration::from_secs(29)));
    assert!(!updated.permits(0, ip, start + Duration::from_secs(30)));
    assert!(updated.permits(1, ip, start + Duration::from_secs(30)));
    assert!(!updated.permits(0, other, start + Duration::from_secs(10)));
    assert!(!original.permits(1, ip, start));
}

// @kotowari[REQ-389, REQ-014, EX-721]
#[test]
fn delayed_activation_and_recovery_cannot_resurrect_expired_dns_grants() {
    let start = Instant::now();
    let ip = "192.0.2.1".parse().unwrap();
    let deadline = record_deadline(start, 0, Duration::from_secs(1)).unwrap();
    let delayed = LeaseBook::default().record(0, ip, deadline, deadline);
    assert!(!delayed.permits(0, ip, deadline));
    assert!(delayed.active(deadline).is_empty());
    let active = LeaseBook::default().record(0, ip, deadline, start);
    assert_eq!(active.active(start).len(), 1);
    assert!(active.active(deadline).is_empty());
    let refreshed = active.record(0, ip, deadline + Duration::from_secs(1), deadline);
    assert!(refreshed.permits(0, ip, deadline));
}
