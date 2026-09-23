use kakoi_core::network::NetworkLimits;
use kakoi_net::resolution::{CnameChain, CnameError};
use kakoi_net::resolution::{ResolutionBudget, ResolutionLimit, UpstreamWait};
use std::time::{Duration, Instant};

// @kotowari[REQ-120, REQ-014, REQ-389]
#[test]
fn cname_chain_limits_hops_detects_case_insensitive_cycles_and_keeps_earliest_expiry() {
    let start = Instant::now();
    let mut chain = CnameChain::new("a.example.", 2).unwrap();
    chain
        .follow("b.example.", start + Duration::from_secs(5))
        .unwrap();
    assert_eq!(
        chain.follow("A.Example.", start + Duration::from_secs(10)),
        Err(CnameError::Cycle)
    );
    chain
        .follow("c.example.", start + Duration::from_secs(2))
        .unwrap();
    assert_eq!(
        chain.permission_deadline(start + Duration::from_secs(30)),
        start + Duration::from_secs(2)
    );
    assert_eq!(
        chain.follow("d.example.", start + Duration::from_secs(1)),
        Err(CnameError::Hops)
    );
    // Rejection must not alter the accepted chain or its expiration.
    assert_eq!(
        chain.permission_deadline(start + Duration::from_secs(30)),
        start + Duration::from_secs(2)
    );
}

// @kotowari[REQ-116, REQ-122]
#[test]
fn retries_share_the_original_deadline_and_send_budget() {
    let start = Instant::now();
    let limits = NetworkLimits {
        dns_max_upstream_queries: 3,
        ..NetworkLimits::default()
    };
    let mut budget = ResolutionBudget::new(start, &limits).unwrap();
    assert_eq!(
        budget.reserve_query(start, UpstreamWait::Explicit),
        Ok(start + Duration::from_secs(2))
    );
    assert_eq!(
        budget.reserve_query(start + Duration::from_secs(1), UpstreamWait::Explicit),
        Ok(start + Duration::from_secs(3))
    );
    // The caller changes DNS settings here; the same resolution owns the budget.
    assert_eq!(
        budget.reserve_query(start + Duration::from_secs(9), UpstreamWait::Explicit),
        Ok(start + Duration::from_secs(10))
    );
    assert_eq!(
        budget.reserve_query(start + Duration::from_secs(9), UpstreamWait::Explicit),
        Err(ResolutionLimit::Queries)
    );
    assert_eq!(
        budget.ensure_live(start + Duration::from_secs(10)),
        Err(ResolutionLimit::Deadline)
    );
}

// @kotowari[REQ-116, REQ-122]
#[test]
fn host_dns_uses_the_whole_deadline_and_separate_resolutions_have_separate_budgets() {
    let start = Instant::now();
    let limits = NetworkLimits {
        dns_max_upstream_queries: 1,
        ..NetworkLimits::default()
    };
    let mut ipv4 = ResolutionBudget::new(start, &limits).unwrap();
    let mut ipv6 = ResolutionBudget::new(start, &limits).unwrap();
    assert_eq!(
        ipv4.reserve_query(start, UpstreamWait::HostResolver),
        Ok(start + Duration::from_secs(10))
    );
    assert_eq!(
        ipv4.reserve_query(start, UpstreamWait::HostResolver),
        Err(ResolutionLimit::Queries)
    );
    assert!(ipv6
        .reserve_query(start, UpstreamWait::HostResolver)
        .is_ok());
    let mut expired = ResolutionBudget::new(start, &limits).unwrap();
    assert_eq!(
        expired.reserve_query(start + Duration::from_secs(10), UpstreamWait::HostResolver),
        Err(ResolutionLimit::Deadline)
    );
}
