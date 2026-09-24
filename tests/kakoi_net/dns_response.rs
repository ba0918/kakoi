use kakoi_net::dns::AddressProgress;
use kakoi_net::dns::{DnsError, Question, ResponseDisposition};
use kakoi_net::scope::DnsAdmission;
use std::time::{Duration, Instant};

fn query(kind: u16) -> Vec<u8> {
    query_name("api.example.com", kind)
}

fn name(name: &str) -> Vec<u8> {
    let mut wire = Vec::new();
    for label in name.split('.') {
        wire.push(label.len() as u8);
        wire.extend(label.as_bytes());
    }
    wire.push(0);
    wire
}

fn query_name(owner: &str, kind: u16) -> Vec<u8> {
    let mut wire = vec![0x12, 0x34, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    wire.extend(name(owner));
    wire.extend(kind.to_be_bytes());
    wire.extend([0, 1]);
    wire
}

fn answer(wire: &mut Vec<u8>, owner: &str, kind: u16, ttl: u32, data: &[u8]) {
    wire[7] += 1;
    wire.extend(name(owner));
    wire.extend(kind.to_be_bytes());
    wire.extend([0, 1]);
    wire.extend(ttl.to_be_bytes());
    wire.extend((data.len() as u16).to_be_bytes());
    wire.extend(data);
}

// @kotowari[REQ-019, REQ-022, REQ-024, REQ-389, EX-041]
#[test]
fn cname_candidates_exclude_unrelated_answers_and_keep_deadlines_across_responses() {
    let question = Question::parse(&query(1)).unwrap();
    let mut chain = question.address_chain(16).unwrap();
    let start = Instant::now();
    let grace = Duration::from_secs(1);
    let mut first = response(1, 0x80);
    answer(
        &mut first,
        "api.example.com",
        5,
        2,
        &name("target.example.net"),
    );
    answer(&mut first, "unrelated.example.com", 1, 99, &[8, 8, 8, 8]);
    assert_eq!(
        chain
            .consume(&question.validate_response(&first).unwrap(), start, grace)
            .unwrap(),
        AddressProgress::Follow("target.example.net.".into())
    );
    let query = query_name("target.example.net", 1);
    let follow = Question::parse(&query).unwrap();
    let mut second = query;
    second[2] = 0x81;
    second[3] = 0x80;
    answer(&mut second, "target.example.net", 1, 0, &[1, 1, 1, 1]);
    let result = chain
        .consume(
            &follow.validate_response(&second).unwrap(),
            start + Duration::from_millis(1500),
            grace,
        )
        .unwrap();
    let AddressProgress::Complete(addresses) = result else {
        panic!("no final address")
    };
    assert_eq!(addresses.len(), 1);
    assert_eq!(addresses[0].address.to_string(), "1.1.1.1");
    assert_eq!(addresses[0].deadline, start + Duration::from_secs(2));
    assert_eq!(
        addresses[0].cache_deadline,
        start + Duration::from_millis(1500)
    );
}

// @kotowari[REQ-014, REQ-389]
#[test]
fn zero_ttl_grace_never_extends_a_positive_deadline_in_the_same_answer_set() {
    let question = Question::parse(&query(1)).unwrap();
    let start = Instant::now();
    let mut wire = response(1, 0x80);
    answer(&mut wire, "api.example.com", 1, 0, &[1, 1, 1, 1]);
    answer(&mut wire, "api.example.com", 1, 1, &[8, 8, 8, 8]);
    let response = question.validate_response(&wire).unwrap();
    let result = question
        .address_chain(16)
        .unwrap()
        .consume(&response, start, Duration::from_secs(10))
        .unwrap();
    let AddressProgress::Complete(addresses) = result else {
        panic!("missing candidates")
    };
    assert_eq!(addresses.len(), 2);
    for address in addresses {
        assert_eq!(address.deadline, start + Duration::from_secs(1));
        assert_eq!(address.cache_deadline, start);
    }
}

// @kotowari[REQ-024, REQ-114]
#[test]
fn additional_addresses_and_negative_or_truncated_answers_do_not_grant_candidates() {
    let question = Question::parse(&query(1)).unwrap();
    let start = Instant::now();
    let grace = Duration::from_secs(1);
    let mut wire = response(1, 0x80);
    answer(&mut wire, "api.example.com", 1, 10, &[1, 1, 1, 1]);
    wire[7] = 0;
    wire[11] = 1;
    assert_eq!(
        question
            .address_chain(16)
            .unwrap()
            .consume(&question.validate_response(&wire).unwrap(), start, grace)
            .unwrap(),
        AddressProgress::NoData
    );
    wire[7] = 1;
    wire[11] = 0;
    wire[3] = 0x83;
    assert_eq!(
        question
            .address_chain(16)
            .unwrap()
            .consume(&question.validate_response(&wire).unwrap(), start, grace)
            .unwrap(),
        AddressProgress::NoData
    );
    wire[3] = 0x80;
    wire[2] |= 2;
    assert!(question
        .address_chain(16)
        .unwrap()
        .consume(&question.validate_response(&wire).unwrap(), start, grace)
        .is_err());
}

// @kotowari[REQ-023, REQ-120, REQ-024, EX-038, EX-039]
#[test]
fn cyclic_or_conflicting_aliases_never_leave_partial_candidates_or_chain_state() {
    let question = Question::parse(&query(1)).unwrap();
    let mut chain = question.address_chain(1).unwrap();
    let start = Instant::now();
    let grace = Duration::from_secs(1);
    let mut cycle = response(1, 0x80);
    answer(
        &mut cycle,
        "api.example.com",
        5,
        10,
        &name("other.example.com"),
    );
    answer(
        &mut cycle,
        "other.example.com",
        5,
        10,
        &name("api.example.com"),
    );
    assert!(chain
        .consume(&question.validate_response(&cycle).unwrap(), start, grace)
        .is_err());
    let mut too_deep = response(1, 0x80);
    answer(
        &mut too_deep,
        "api.example.com",
        5,
        10,
        &name("second.example.com"),
    );
    answer(
        &mut too_deep,
        "second.example.com",
        5,
        10,
        &name("third.example.com"),
    );
    assert_eq!(
        chain.consume(
            &question.validate_response(&too_deep).unwrap(),
            start,
            grace
        ),
        Err(DnsError::AliasLimit)
    );
    let mut valid = response(1, 0x80);
    answer(&mut valid, "api.example.com", 1, 10, &[1, 1, 1, 1]);
    assert!(matches!(
        chain
            .consume(&question.validate_response(&valid).unwrap(), start, grace)
            .unwrap(),
        AddressProgress::Complete(_)
    ));
    answer(
        &mut valid,
        "api.example.com",
        5,
        10,
        &name("other.example.com"),
    );
    assert!(question
        .address_chain(16)
        .unwrap()
        .consume(&question.validate_response(&valid).unwrap(), start, grace)
        .is_err());
    assert!(Question::parse(&query(65280))
        .unwrap()
        .address_chain(16)
        .is_err());
}

fn response(kind: u16, flags: u8) -> Vec<u8> {
    let mut wire = query(kind);
    wire[2] = 0x81;
    wire[3] = flags;
    wire
}

// @kotowari[REQ-114, REQ-115]
#[test]
fn response_envelope_matches_the_original_question_before_selecting_an_upstream_result() {
    let question = Question::parse(&query(1)).unwrap();
    assert_eq!(
        question
            .validate_response(&response(1, 0x83))
            .unwrap()
            .disposition(),
        ResponseDisposition::Answer
    );
    for code in [2, 5] {
        assert_eq!(
            question
                .validate_response(&response(1, 0x80 | code))
                .unwrap()
                .disposition(),
            ResponseDisposition::NextUpstream
        );
    }
    for changed_byte in [0, 2, 13, 31, 32] {
        let mut mismatched = response(1, 0x80);
        mismatched[changed_byte] ^= if changed_byte == 2 { 0x80 } else { 1 };
        assert!(
            question.validate_response(&mismatched).is_err(),
            "byte {changed_byte}"
        );
    }
    assert!(question.validate_response(&response(28, 0x80)).is_err());
    let mut truncated = response(1, 0x80);
    truncated[2] |= 2;
    assert_eq!(
        question
            .validate_response(&truncated)
            .unwrap()
            .disposition(),
        ResponseDisposition::RetryTcp
    );
}

// @kotowari[REQ-149, EX-330]
#[test]
fn ordinary_unknown_record_data_is_preserved_without_becoming_an_ip_permission() {
    let question = Question::parse(&query(65280)).unwrap();
    let mut wire = response(65280, 0x80);
    wire[7] = 1;
    wire.extend([
        0xc0, 0x0c, 0xff, 0, 0, 1, 0, 0, 0, 30, 0, 4, 0, 255, 192, 12,
    ]);
    let accepted = question.validate_response(&wire).unwrap();
    assert_eq!(accepted.wire(), wire);
    let mut trailing = wire.clone();
    trailing.push(0);
    assert!(question.validate_response(&trailing).is_err());
    wire.pop();
    assert!(question.validate_response(&wire).is_err());
}

// @kotowari[REQ-130, REQ-149]
#[test]
fn update_transfer_any_and_multiple_questions_are_not_ordinary_read_queries() {
    for kind in [0, 41, 249, 250, 251, 252, 253, 254, 255, 65535] {
        assert!(Question::parse(&query(kind)).is_err());
    }
    let mut update = query(1);
    update[2] = 0x29;
    assert!(Question::parse(&update).is_err());
    let mut many = query(1);
    many[5] = 2;
    many.extend_from_slice(&query(1)[12..]);
    assert!(Question::parse(&many).is_err());
}

// @kotowari[REQ-020, REQ-021, REQ-025, EX-032, EX-034, EX-035]
#[test]
fn mixed_answers_filter_unsigned_data_preserve_signatures_and_refuse_all_denied() {
    let question = Question::parse(&query(1)).unwrap();
    let start = Instant::now();
    for signed in [false, true] {
        let mut wire = response(1, 0x80);
        answer(&mut wire, "api.example.com", 1, 30, &[1, 1, 1, 1]);
        answer(&mut wire, "api.example.com", 1, 30, &[10, 0, 0, 1]);
        if signed {
            // Syntactic RRSIG fixture, not a claim of cryptographic validation.
            answer(
                &mut wire,
                "api.example.com",
                46,
                30,
                &[
                    0, 1, 8, 3, 0, 0, 0, 30, 255, 255, 255, 255, 0, 0, 0, 1, 0, 1, 0, 1, 2, 3, 4,
                ],
            );
        }
        let response = question.validate_response(&wire).unwrap();
        let AddressProgress::Complete(candidates) = question
            .address_chain(16)
            .unwrap()
            .consume(&response, start, Duration::from_secs(1))
            .unwrap()
        else {
            panic!("no addresses")
        };
        let screened = response
            .screen_addresses(&candidates, |ip| {
                if ip.to_string() == "1.1.1.1" {
                    DnsAdmission::Dynamic
                } else {
                    DnsAdmission::Denied
                }
            })
            .unwrap();
        assert_eq!(screened.dynamic_grants.len(), 1);
        assert_eq!(screened.dynamic_grants[0].address.to_string(), "1.1.1.1");
        if signed {
            assert_eq!(screened.wire, wire);
        } else {
            let filtered = question.validate_response(&screened.wire).unwrap();
            let AddressProgress::Complete(remaining) = question
                .address_chain(16)
                .unwrap()
                .consume(&filtered, start, Duration::from_secs(1))
                .unwrap()
            else {
                panic!("no retained address")
            };
            assert_eq!(remaining.len(), 1);
            assert_eq!(remaining[0].address.to_string(), "1.1.1.1");
        }
        assert!(matches!(
            response.screen_addresses(&candidates, |_| DnsAdmission::Denied),
            Err(DnsError::PolicyDenied)
        ));
        let existing = response
            .screen_addresses(&candidates, |_| DnsAdmission::ExistingOnly)
            .unwrap();
        assert!(existing.dynamic_grants.is_empty());
        assert_eq!(existing.wire, wire);
    }
}

// @kotowari[REQ-019, REQ-022, REQ-116, REQ-122]
#[test]
fn resolution_follows_cname_with_one_budget_and_ages_the_assembled_answer() {
    use kakoi_core::network::NetworkLimits;
    use kakoi_net::{
        dns::resolve_addresses,
        resolution::{ResolutionBudget, UpstreamWait},
    };
    let limits = NetworkLimits::default();
    let start = Instant::now();
    let mut budget = ResolutionBudget::new(start, &limits).unwrap();
    let mut calls = 0;
    let resolved = resolve_addresses(&query(1), &limits, &mut budget, |request, budget| {
        let received = start + Duration::from_secs(calls * 4);
        budget
            .reserve_query(received, UpstreamWait::PerCandidate)
            .map_err(|_| DnsError::IncompleteResponse)?;
        let mut response = request.to_vec();
        response[2] |= 0x80;
        if calls == 0 {
            answer(
                &mut response,
                "api.example.com",
                5,
                10,
                &name("target.example.net"),
            );
        } else {
            assert_eq!(&request[12..], &query_name("target.example.net", 1)[12..]);
            answer(&mut response, "target.example.net", 1, 20, &[1, 1, 1, 1]);
        }
        calls += 1;
        Ok((response, received))
    })
    .unwrap();
    assert_eq!(calls, 2);
    assert_eq!(resolved.candidates.len(), 1);
    assert_eq!(
        resolved.candidates[0].deadline,
        start + Duration::from_secs(10)
    );
    assert_eq!(
        resolved.candidates[0].cache_deadline,
        start + Duration::from_secs(10)
    );
    let original = Question::parse(&query(1)).unwrap();
    let response = original
        .validate_response(resolved.response.wire())
        .unwrap();
    assert_eq!(response.wire()[7], 2);
    let mut chain = original.address_chain(16).unwrap();
    let AddressProgress::Complete(candidates) = chain
        .consume(
            &response,
            start + Duration::from_secs(4),
            Duration::from_secs(1),
        )
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(candidates[0].deadline, start + Duration::from_secs(10));
}

// @kotowari[REQ-023, REQ-120, REQ-122]
#[test]
fn a_cname_resolution_cannot_reset_query_limits_or_return_partial_success() {
    use kakoi_core::network::NetworkLimits;
    use kakoi_net::{
        dns::resolve_addresses,
        resolution::{ResolutionBudget, UpstreamWait},
    };
    let limits = NetworkLimits {
        dns_max_upstream_queries: 1,
        ..NetworkLimits::default()
    };
    let start = Instant::now();
    let mut budget = ResolutionBudget::new(start, &limits).unwrap();
    let mut sent = 0;
    let result = resolve_addresses(&query(1), &limits, &mut budget, |request, budget| {
        budget
            .reserve_query(start, UpstreamWait::PerCandidate)
            .map_err(|_| DnsError::IncompleteResponse)?;
        sent += 1;
        let mut response = request.to_vec();
        response[2] |= 0x80;
        answer(
            &mut response,
            "api.example.com",
            5,
            10,
            &name("target.example.net"),
        );
        Ok((response, start))
    });
    assert!(matches!(result, Err(DnsError::IncompleteResponse)));
    assert_eq!(sent, 1);
}

// @kotowari[REQ-019, REQ-020, REQ-025]
#[test]
fn assembled_cname_answers_preserve_signed_rrsets_when_only_some_ips_are_allowed() {
    use kakoi_core::network::NetworkLimits;
    use kakoi_net::{dns::resolve_addresses, resolution::ResolutionBudget};
    let limits = NetworkLimits::default();
    let start = Instant::now();
    let mut budget = ResolutionBudget::new(start, &limits).unwrap();
    // Syntactic signatures only: this tests preservation, not DNSSEC validation.
    let alias_signature = [
        0, 5, 8, 3, 0, 0, 0, 30, 255, 255, 255, 255, 0, 0, 0, 1, 0, 1, 0, 4, 3, 2, 1,
    ];
    let address_signature = [
        0, 1, 8, 3, 0, 0, 0, 30, 255, 255, 255, 255, 0, 0, 0, 1, 0, 1, 0, 1, 2, 3, 4,
    ];
    let mut calls = 0;
    let resolved = resolve_addresses(&query(1), &limits, &mut budget, |request, _| {
        let mut response = request.to_vec();
        response[2] |= 0x80;
        if calls == 0 {
            answer(
                &mut response,
                "api.example.com",
                5,
                30,
                &name("target.example.net"),
            );
            answer(&mut response, "api.example.com", 46, 30, &alias_signature);
        } else {
            answer(&mut response, "target.example.net", 1, 30, &[1, 1, 1, 1]);
            answer(&mut response, "target.example.net", 1, 30, &[10, 0, 0, 1]);
            answer(
                &mut response,
                "target.example.net",
                46,
                30,
                &address_signature,
            );
        }
        calls += 1;
        Ok((response, start))
    })
    .unwrap();
    let assembled = resolved.response.wire().to_vec();
    assert_eq!(assembled[7], 5);
    assert!(assembled
        .windows(alias_signature.len())
        .any(|data| data == alias_signature));
    assert!(assembled
        .windows(address_signature.len())
        .any(|data| data == address_signature));
    let screened = resolved
        .response
        .screen_addresses(&resolved.candidates, |ip| {
            if ip.to_string() == "1.1.1.1" {
                DnsAdmission::Dynamic
            } else {
                DnsAdmission::Denied
            }
        })
        .unwrap();
    assert_eq!(screened.wire, assembled);
    assert_eq!(screened.dynamic_grants.len(), 1);
}

// A zero time to live is granted the default second from the answer's
// reception, not from when the resolution finished checking it.
// @kotowari[EX-718]
#[test]
fn the_zero_ttl_grace_counts_from_the_reception() {
    use kakoi_core::network::NetworkLimits;
    use kakoi_net::{
        dns::resolve_addresses,
        resolution::{ResolutionBudget, UpstreamWait},
    };
    let limits = NetworkLimits::default();
    let start = Instant::now();
    let mut budget = ResolutionBudget::new(start, &limits).unwrap();
    let received = start;
    let resolved = resolve_addresses(&query(1), &limits, &mut budget, |request, budget| {
        budget
            .reserve_query(received, UpstreamWait::PerCandidate)
            .map_err(|_| DnsError::IncompleteResponse)?;
        let mut response = request.to_vec();
        response[2] |= 0x80;
        answer(&mut response, "api.example.com", 1, 0, &[1, 1, 1, 1]);
        // The check finishes well after the reception.
        std::thread::sleep(Duration::from_millis(300));
        Ok((response, received))
    })
    .unwrap();
    assert_eq!(
        resolved.candidates[0].deadline,
        received + Duration::from_millis(1000)
    );
}

/// The time to live of every answer record in `wire`, in order.
fn answer_ttls(wire: &[u8]) -> Vec<u32> {
    fn skip_name(wire: &[u8], mut at: usize) -> usize {
        loop {
            match wire[at] {
                0 => return at + 1,
                length if length & 0xc0 == 0xc0 => return at + 2,
                length => at += 1 + usize::from(length),
            }
        }
    }
    let count = |at: usize| usize::from(u16::from_be_bytes([wire[at], wire[at + 1]]));
    let mut at = 12;
    for _ in 0..count(4) {
        at = skip_name(wire, at) + 4;
    }
    (0..count(6))
        .map(|_| {
            at = skip_name(wire, at) + 4;
            let ttl = u32::from_be_bytes(wire[at..at + 4].try_into().unwrap());
            at += 6 + count(at + 4);
            ttl
        })
        .collect()
}

// @kotowari[REQ-398, EX-734]
#[test]
fn every_answer_reaches_the_application_with_the_shortest_time_to_live() {
    let question = Question::parse(&query(1)).unwrap();
    let start = Instant::now();
    for signed in [false, true] {
        let mut wire = response(1, 0x80);
        answer(&mut wire, "api.example.com", 1, 30, &[1, 1, 1, 1]);
        answer(&mut wire, "api.example.com", 1, 300, &[1, 1, 1, 2]);
        if signed {
            // Syntactic RRSIG fixture: a record signature does not bind its TTL.
            answer(
                &mut wire,
                "api.example.com",
                46,
                300,
                &[
                    0, 1, 8, 3, 0, 0, 1, 44, 255, 255, 255, 255, 0, 0, 0, 1, 0, 1, 0, 1, 2, 3, 4,
                ],
            );
        }
        let response = question.validate_response(&wire).unwrap();
        let AddressProgress::Complete(candidates) = question
            .address_chain(16)
            .unwrap()
            .consume(&response, start, Duration::from_secs(1))
            .unwrap()
        else {
            panic!("no addresses")
        };
        let screened = response
            .screen_addresses(&candidates, |_| DnsAdmission::Dynamic)
            .unwrap();
        let ttls = answer_ttls(&screened.wire);
        assert_eq!(ttls.len(), if signed { 3 } else { 2 });
        assert!(ttls.iter().all(|ttl| *ttl <= 30), "{ttls:?}");
    }
}

// @kotowari[REQ-398, EX-735]
#[test]
fn an_address_behind_a_shorter_alias_reaches_the_application_with_the_alias_time_to_live() {
    use kakoi_core::network::NetworkLimits;
    use kakoi_net::{dns::resolve_addresses, resolution::ResolutionBudget};
    let limits = NetworkLimits::default();
    let start = Instant::now();
    let mut budget = ResolutionBudget::new(start, &limits).unwrap();
    let mut calls = 0;
    let resolved = resolve_addresses(&query(1), &limits, &mut budget, |request, _| {
        let mut response = request.to_vec();
        response[2] |= 0x80;
        if calls == 0 {
            answer(
                &mut response,
                "api.example.com",
                5,
                10,
                &name("target.example.net"),
            );
        } else {
            answer(&mut response, "target.example.net", 1, 300, &[1, 1, 1, 1]);
        }
        calls += 1;
        Ok((response, start))
    })
    .unwrap();
    let screened = resolved
        .response
        .screen_addresses(&resolved.candidates, |_| DnsAdmission::Dynamic)
        .unwrap();
    let ttls = answer_ttls(&screened.wire);
    assert_eq!(ttls.len(), 2);
    assert!(ttls.iter().all(|ttl| *ttl <= 10), "{ttls:?}");
}

// Only the addresses screened for the question reach the application: an
// address record of another name, or of the other family, was never screened.
// @kotowari[REQ-020, EX-031]
#[test]
fn address_records_outside_the_question_do_not_reach_the_application() {
    let question = Question::parse(&query(1)).unwrap();
    let start = Instant::now();
    let mut wire = response(1, 0x80);
    answer(&mut wire, "api.example.com", 1, 30, &[1, 1, 1, 1]);
    answer(&mut wire, "other.example.com", 1, 30, &[10, 0, 0, 1]);
    let mut v6 = [0; 16];
    v6[0] = 0xfd;
    v6[15] = 1;
    answer(&mut wire, "api.example.com", 28, 30, &v6);
    let response = question.validate_response(&wire).unwrap();
    let AddressProgress::Complete(candidates) = question
        .address_chain(16)
        .unwrap()
        .consume(&response, start, Duration::from_secs(1))
        .unwrap()
    else {
        panic!("no addresses")
    };
    let screened = response
        .screen_addresses(&candidates, |_| DnsAdmission::Dynamic)
        .unwrap();
    assert_eq!(answer_ttls(&screened.wire).len(), 1);
    assert!(!screened.wire.windows(4).any(|data| data == [10, 0, 0, 1]));
    assert!(!screened.wire.windows(16).any(|data| data == v6));
}
