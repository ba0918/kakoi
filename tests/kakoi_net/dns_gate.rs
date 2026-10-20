use crate::dns_fixture::{dns_rule, query};
use kakoi_net::dns::{DnsError, DnsGate};

// @kotowari[REQ-027, REQ-003, REQ-010, REQ-130, REQ-149, EX-273, EX-295, EX-044, EX-046, EX-231]
#[test]
fn only_authorized_names_reach_the_resolver_with_the_original_rule_indices() {
    let gate = DnsGate::new(vec![
        dns_rule("*.example.com"),
        dns_rule("API.Example.com"),
        dns_rule("例え.テスト"),
    ]);
    for (name, indices) in [
        ("API.example.com", vec![0, 1]),
        ("nested.api.example.com", vec![0]),
        ("xn--r8jz45g.xn--zckzah", vec![2]),
    ] {
        for kind in [1, 28, 15, 16, 33, 65, 65280] {
            let wire = query(name, kind);
            let mut called = false;
            let answer = gate
                .dispatch(&wire, |request, matched| {
                    called = true;
                    assert_eq!(matched, indices);
                    assert_eq!(request, wire);
                    let mut response = request.to_vec();
                    response[2] |= 0x80;
                    Ok(response)
                })
                .unwrap();
            assert!(called);
            assert_eq!(answer[3] & 15, 0);
        }
    }
    for name in [
        "example.com",
        "evil-example.com",
        "api.example.com.attacker.net",
        "host-v4.kakoi.internal",
    ] {
        let wire = query(name, 1);
        let answer = gate
            .dispatch(&wire, |_, _| panic!("unauthorized name reached upstream"))
            .unwrap();
        assert_eq!(answer[3] & 15, 5);
        assert_eq!(&answer[..2], &wire[..2]);
        assert_eq!(&answer[12..], &wire[12..]);
    }
}

// @kotowari[REQ-123, REQ-021]
#[test]
fn gate_distinguishes_policy_refusal_from_resolution_failure_and_rejects_wrong_answers() {
    let gate = DnsGate::new(vec![dns_rule("api.example.com")]);
    let wire = query("api.example.com", 1);
    for (error, code) in [
        (DnsError::PolicyDenied, 5),
        (DnsError::AliasCycle, 2),
        (DnsError::IncompleteResponse, 2),
    ] {
        let answer = gate.dispatch(&wire, |_, _| Err(error)).unwrap();
        assert_eq!(answer[3] & 15, code);
        assert_ne!(answer[2] & 0x80, 0);
        assert_ne!(answer[2] & 1, 0); // Preserve RD.
        assert_eq!(&answer[6..12], &[0; 6]);
    }
    let answer = gate
        .dispatch(&wire, |_, _| {
            let mut wrong = query("other.example.com", 1);
            wrong[2] |= 0x80;
            Ok(wrong)
        })
        .unwrap();
    assert_eq!(answer[3] & 15, 2);
}

// @kotowari[REQ-125, REQ-126, REQ-127, REQ-128, REQ-129, EX-291, EX-293]
#[test]
fn resolution_slots_and_waiter_limits_are_independent_and_completion_releases_a_slot() {
    use kakoi_net::dns::{Admission, CapacityError, ResolutionPool};
    let mut pool = ResolutionPool::new(1, 2).unwrap();
    let Admission::Start(id) = pool.admit("A:api.example.com:g1", 10).unwrap() else {
        panic!()
    };
    assert_eq!(
        pool.admit("A:api.example.com:g1", 11),
        Ok(Admission::Join(id))
    );
    assert_eq!(
        pool.admit("A:api.example.com:g1", 12),
        Err((CapacityError::Waiters, 12))
    );
    assert_eq!(
        pool.admit("AAAA:api.example.com:g1", 13),
        Err((CapacityError::Resolutions, 13))
    );
    assert_eq!(
        pool.admit("A:api.example.com:g2", 14),
        Err((CapacityError::Resolutions, 14))
    );
    assert_eq!(pool.complete(id), vec![10, 11]);
    let Admission::Start(next) = pool.admit("A:api.example.com:g1", 15).unwrap() else {
        panic!()
    };
    assert_ne!(id, next);
    assert!(pool.complete(id).is_empty());
    assert_eq!(pool.complete(next), vec![15]);
    assert!(ResolutionPool::<String, usize>::new(0, 1).is_err());
    assert!(ResolutionPool::<String, usize>::new(1, 0).is_err());
}

// @kotowari[REQ-127, REQ-124, REQ-108]
#[test]
fn resolution_keys_share_case_and_client_ids_but_separate_dns_conditions() {
    use kakoi_net::dns::Question;
    let wire = query("api.example.com", 1);
    let key = Question::parse(&wire).unwrap().resolution_key(4).unwrap();
    let mut same = query("API.EXAMPLE.COM", 1);
    same[..2].copy_from_slice(&[99, 88]);
    assert_eq!(
        key,
        Question::parse(&same).unwrap().resolution_key(4).unwrap()
    );
    assert_ne!(
        key,
        Question::parse(&wire).unwrap().resolution_key(5).unwrap()
    );
    assert_ne!(
        key,
        Question::parse(&query("api.example.com", 28))
            .unwrap()
            .resolution_key(4)
            .unwrap()
    );
    let mut no_recursion = wire.clone();
    no_recursion[2] &= !1;
    assert_ne!(
        key,
        Question::parse(&no_recursion)
            .unwrap()
            .resolution_key(4)
            .unwrap()
    );
    let mut checking_disabled = wire;
    checking_disabled[3] |= 0x10;
    assert_ne!(
        key,
        Question::parse(&checking_disabled)
            .unwrap()
            .resolution_key(4)
            .unwrap()
    );
}
