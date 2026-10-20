use crate::dns_fixture::{a_answer, dns_rule, query};
use kakoi_net::dns::DnsError;

// @kotowari[REQ-027, REQ-123, REQ-125, REQ-126, REQ-127, REQ-128, REQ-129, EX-284]
#[test]
fn authorized_requests_share_work_and_return_individual_answers_without_resetting_deadlines() {
    use kakoi_net::dns::{AcceptedRequest, DnsRequests};
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let mut requests = DnsRequests::new(
        vec![dns_rule("*.example.com")],
        7,
        1,
        2,
        Duration::from_secs(1),
    )
    .unwrap();
    let first = query("api.example.com", 1);
    let AcceptedRequest::Start(task) = requests.accept(&first, 10, now).unwrap() else {
        panic!()
    };
    assert_eq!(task.wire, first);
    assert_eq!(task.deadline, now + Duration::from_secs(1));
    let mut second = query("API.example.com", 1);
    second[..2].copy_from_slice(&[99, 88]);
    assert!(matches!(
        requests
            .accept(&second, 11, now + Duration::from_millis(900))
            .unwrap(),
        AcceptedRequest::Waiting
    ));
    for (wire, peer, code) in [
        (query("api.example.com", 1), 12, 2),
        (query("api.example.com", 28), 13, 2),
        (query("evil.invalid", 1), 14, 5),
    ] {
        let AcceptedRequest::Answer(reply) = requests.accept(&wire, peer, now).unwrap() else {
            panic!()
        };
        assert_eq!(reply.recipient, peer);
        assert_eq!(reply.wire[3] & 15, code);
    }
    // No time to live: the same question afterwards is new work, not cached.
    let answer = a_answer(&first, 0, [1, 1, 1, 1]);
    let replies = requests.complete(task.id, Ok(answer), now + Duration::from_millis(950));
    assert_eq!(replies.len(), 2);
    for (reply, wire, peer) in [(&replies[0], &first, 10), (&replies[1], &second, 11)] {
        assert_eq!(reply.recipient, peer);
        assert_eq!(&reply.wire[..2], &wire[..2]);
        assert_eq!(&reply.wire[12..wire.len()], &wire[12..]);
        kakoi_net::dns::Question::parse(wire)
            .unwrap()
            .validate_response(&reply.wire)
            .unwrap();
        assert_eq!(reply.wire[7], 1);
        assert_eq!(&reply.wire[reply.wire.len() - 4..], &[1, 1, 1, 1]);
    }
    assert!(requests
        .complete(task.id, Err(DnsError::IncompleteResponse), now)
        .is_empty());
    let AcceptedRequest::Start(next) = requests.accept(&first, 15, now).unwrap() else {
        panic!()
    };
    assert_ne!(next.id, task.id);
    assert!(requests.expire(now + Duration::from_millis(999)).is_empty());
    let expired = requests.expire(now + Duration::from_secs(1));
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].recipient, 15);
    assert_eq!(expired[0].wire[3] & 15, 2);
    assert!(requests.complete(next.id, Ok(first), now).is_empty());
}

// @kotowari[REQ-021, REQ-123, REQ-127, EX-289]
#[test]
fn shared_resolution_rejects_wrong_or_late_answers_and_keeps_dns_conditions_separate() {
    use kakoi_net::dns::{AcceptedRequest, DnsRequests};
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let mut requests = DnsRequests::new(
        vec![dns_rule("api.example.com")],
        0,
        4,
        2,
        Duration::from_secs(1),
    )
    .unwrap();
    let wire = query("api.example.com", 1);
    let mut other = wire.clone();
    other[3] |= 0x10;
    let AcceptedRequest::Start(first) = requests.accept(&wire, 1, now).unwrap() else {
        panic!()
    };
    let AcceptedRequest::Start(second) = requests.accept(&other, 2, now).unwrap() else {
        panic!()
    };
    let mut wrong = query("evil.invalid", 1);
    wrong[2] |= 0x80;
    let replies = requests.complete(first.id, Ok(wrong), now);
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0].wire[3] & 15, 2);
    let mut late = other;
    late[2] |= 0x80;
    let replies = requests.complete(second.id, Ok(late), now + Duration::from_secs(1));
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0].wire[3] & 15, 2);
    assert_ne!(replies[0].wire[3] & 0x10, 0);
    assert!(requests.accept(&[0, 1], 3, now).is_err());
}

// A failed resolution is held: the same question is answered SERVFAIL without
// new work until the hold ends, and another question is not held.
// @kotowari[REQ-134, EX-301, EX-302]
#[test]
fn a_failed_resolution_is_held_for_the_failure_cache_time() {
    use kakoi_net::dns::{AcceptedRequest, DnsError, DnsRequests};
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let mut requests = DnsRequests::new(
        vec![dns_rule("*.example.com")],
        7,
        4,
        4,
        Duration::from_secs(10),
    )
    .unwrap()
    .with_failure_hold(Duration::from_secs(5));
    let question = query("api.example.com", 1);
    let AcceptedRequest::Start(task) = requests.accept(&question, 1, now).unwrap() else {
        panic!("the first question starts a resolution")
    };
    let replies = requests.complete(task.id, Err(DnsError::IncompleteResponse), now);
    assert_eq!(replies[0].wire[3] & 15, 2);
    let AcceptedRequest::Answer(reply) = requests
        .accept(&question, 2, now + Duration::from_secs(2))
        .unwrap()
    else {
        panic!("a held failure started new work")
    };
    assert_eq!((reply.recipient, reply.wire[3] & 15), (2, 2));
    assert_eq!(&reply.wire[..2], &question[..2]);
    // Another type of the same name is another question.
    assert!(matches!(
        requests
            .accept(
                &query("api.example.com", 28),
                3,
                now + Duration::from_secs(2)
            )
            .unwrap(),
        AcceptedRequest::Start(_)
    ));
    assert!(matches!(
        requests
            .accept(&question, 4, now + Duration::from_secs(5))
            .unwrap(),
        AcceptedRequest::Start(_)
    ));
}

// A valid answer is used again for the same question until its time to live
// runs out, counted down, with the asker's own ID; an answer with no time to
// live is not kept, and new settings do not use what the old ones cached.
// @kotowari[REQ-133, REQ-389, EX-234]
#[test]
fn a_valid_answer_is_reused_until_its_time_to_live_runs_out() {
    use kakoi_net::dns::{AcceptedRequest, DnsRequests};
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let mut requests = DnsRequests::new(
        vec![dns_rule("*.example.com")],
        7,
        4,
        4,
        Duration::from_secs(10),
    )
    .unwrap();
    let question = query("api.example.com", 1);
    let AcceptedRequest::Start(task) = requests.accept(&question, 1, now).unwrap() else {
        panic!("the first question starts a resolution")
    };
    requests.complete(task.id, Ok(a_answer(&question, 30, [1, 1, 1, 1])), now);
    let mut again = query("API.example.com", 1);
    again[..2].copy_from_slice(&[0x55, 0x66]);
    let AcceptedRequest::Answer(reply) = requests
        .accept(&again, 2, now + Duration::from_millis(10_500))
        .unwrap()
    else {
        panic!("a valid cached answer started new work")
    };
    let validated = kakoi_net::dns::Question::parse(&again)
        .unwrap()
        .validate_response(&reply.wire)
        .unwrap();
    assert_eq!(validated.wire()[3] & 15, 0);
    // 30 seconds less the 11 begun since the answer.
    let len = reply.wire.len();
    assert_eq!(&reply.wire[len - 10..len - 6], &19u32.to_be_bytes());
    assert_eq!(&reply.wire[len - 4..], &[1, 1, 1, 1]);
    assert!(matches!(
        requests
            .accept(&question, 3, now + Duration::from_secs(30))
            .unwrap(),
        AcceptedRequest::Start(_)
    ));
    // Nothing is kept of an answer without a time to live.
    let other = query("www.example.com", 1);
    let AcceptedRequest::Start(task) = requests.accept(&other, 4, now).unwrap() else {
        panic!()
    };
    requests.complete(task.id, Ok(a_answer(&other, 0, [1, 1, 1, 1])), now);
    assert!(matches!(
        requests.accept(&other, 5, now).unwrap(),
        AcceptedRequest::Start(_)
    ));
    // New settings: the cache of the old ones is not used.
    let third = query("cdn.example.com", 1);
    let AcceptedRequest::Start(task) = requests.accept(&third, 6, now).unwrap() else {
        panic!()
    };
    requests.complete(task.id, Ok(a_answer(&third, 30, [1, 1, 1, 1])), now);
    requests.advance_generation();
    assert!(matches!(
        requests.accept(&third, 7, now).unwrap(),
        AcceptedRequest::Start(_)
    ));
}

// Joining a resolution shares its first deadline: at that deadline every
// waiter, the one that joined late included, is answered.
// @kotowari[EX-287]
#[test]
fn a_late_join_keeps_the_shared_deadline() {
    use kakoi_net::dns::{AcceptedRequest, DnsRequests};
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let mut requests = DnsRequests::new(
        vec![dns_rule("*.example.com")],
        7,
        4,
        4,
        Duration::from_secs(2),
    )
    .unwrap();
    let question = query("api.example.com", 1);
    assert!(matches!(
        requests.accept(&question, 1, now).unwrap(),
        AcceptedRequest::Start(_)
    ));
    assert!(matches!(
        requests
            .accept(&question, 2, now + Duration::from_millis(1900))
            .unwrap(),
        AcceptedRequest::Waiting
    ));
    assert!(requests
        .expire(now + Duration::from_millis(1999))
        .is_empty());
    let expired = requests.expire(now + Duration::from_secs(2));
    assert_eq!(
        expired
            .iter()
            .map(|reply| reply.recipient)
            .collect::<Vec<_>>(),
        [1, 2]
    );
}

// Two questions from one application count as two waiters.
// @kotowari[EX-292]
#[test]
fn each_question_of_one_application_is_a_waiter() {
    use kakoi_net::dns::{AcceptedRequest, DnsRequests};
    use std::time::{Duration, Instant};
    let now = Instant::now();
    // One recipient stands for one application's socket.
    let mut requests = DnsRequests::new(
        vec![dns_rule("*.example.com")],
        7,
        4,
        2,
        Duration::from_secs(2),
    )
    .unwrap();
    let question = query("api.example.com", 1);
    assert!(matches!(
        requests.accept(&question, 1, now).unwrap(),
        AcceptedRequest::Start(_)
    ));
    assert!(matches!(
        requests.accept(&question, 1, now).unwrap(),
        AcceptedRequest::Waiting
    ));
    let AcceptedRequest::Answer(reply) = requests.accept(&question, 1, now).unwrap() else {
        panic!("a third waiter was admitted")
    };
    assert_eq!(reply.wire[3] & 15, 2);
}

// Each environment has its own requests: one full of work and one resolving
// the same name share neither slots nor resolutions.
// @kotowari[EX-283, EX-288]
#[test]
fn environments_share_neither_slots_nor_resolutions() {
    use kakoi_net::dns::{AcceptedRequest, DnsRequests};
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let mut first = DnsRequests::new(
        vec![dns_rule("*.example.com")],
        7,
        1,
        4,
        Duration::from_secs(2),
    )
    .unwrap();
    let mut second = DnsRequests::new(
        vec![dns_rule("*.example.com")],
        7,
        1,
        4,
        Duration::from_secs(2),
    )
    .unwrap();
    let question = query("api.example.com", 1);
    assert!(matches!(
        first.accept(&question, 1, now).unwrap(),
        AcceptedRequest::Start(_)
    ));
    // The first is full; the second starts the same name on its own.
    assert!(matches!(
        first.accept(&query("www.example.com", 1), 2, now).unwrap(),
        AcceptedRequest::Answer(_)
    ));
    assert!(matches!(
        second.accept(&question, 3, now).unwrap(),
        AcceptedRequest::Start(_)
    ));
}

// A resolution asked again under new settings belongs to them once it is
// re-keyed: its answer then serves the new settings' questions. Without the
// re-keying it serves nothing, since it may carry the old settings' answer.
// A local overload fails its question without being held as a failure.
// @kotowari[REQ-108, REQ-133, REQ-134]
#[test]
fn only_answers_under_the_current_settings_are_kept() {
    use kakoi_net::dns::{AcceptedRequest, DnsError, DnsRequests};
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let mut requests = DnsRequests::new(
        vec![dns_rule("*.example.com")],
        7,
        4,
        4,
        Duration::from_secs(10),
    )
    .unwrap()
    .with_failure_hold(Duration::from_secs(5));
    for rekeyed in [false, true] {
        let question = query(
            if rekeyed {
                "a.example.com"
            } else {
                "b.example.com"
            },
            1,
        );
        let AcceptedRequest::Start(task) = requests.accept(&question, 1, now).unwrap() else {
            panic!()
        };
        requests.advance_generation();
        if rekeyed {
            requests.rekey(task.id);
        }
        requests.complete(task.id, Ok(a_answer(&question, 30, [1, 1, 1, 1])), now);
        let reused = matches!(
            requests.accept(&question, 2, now).unwrap(),
            AcceptedRequest::Answer(_)
        );
        assert_eq!(reused, rekeyed);
    }
    let question = query("c.example.com", 1);
    let AcceptedRequest::Start(task) = requests.accept(&question, 1, now).unwrap() else {
        panic!()
    };
    let replies = requests.complete(task.id, Err(DnsError::Overloaded), now);
    assert_eq!(replies[0].wire[3] & 15, 2);
    assert!(matches!(
        requests.accept(&question, 2, now).unwrap(),
        AcceptedRequest::Start(_)
    ));
}

// A re-keyed resolution is the one the new settings share: a question asked
// under them joins it instead of starting a second upstream resolution, or
// being refused for want of a slot while the same work is running.
// @kotowari[REQ-127, REQ-135]
#[test]
fn a_question_under_new_settings_joins_the_resolution_re_keyed_for_them() {
    use kakoi_net::dns::{AcceptedRequest, DnsRequests};
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let mut requests = DnsRequests::new(
        vec![dns_rule("*.example.com")],
        7,
        1,
        4,
        Duration::from_secs(10),
    )
    .unwrap();
    let question = query("a.example.com", 1);
    let AcceptedRequest::Start(task) = requests.accept(&question, 1, now).unwrap() else {
        panic!()
    };
    requests.advance_generation();
    requests.rekey(task.id);
    assert!(matches!(
        requests.accept(&question, 2, now).unwrap(),
        AcceptedRequest::Waiting
    ));
    let replies = requests.complete(task.id, Ok(a_answer(&question, 30, [1, 1, 1, 1])), now);
    assert_eq!(replies.len(), 2);
}

// @kotowari[REQ-127]
#[test]
fn a_running_new_settings_resolution_remains_joinable_after_an_older_retry_fails() {
    use kakoi_net::dns::{AcceptedRequest, DnsRequests};
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let mut requests = DnsRequests::new(
        vec![dns_rule("*.example.com")],
        7,
        2,
        4,
        Duration::from_secs(10),
    )
    .unwrap()
    .with_failure_hold(Duration::from_secs(5));
    let question = query("a.example.com", 1);
    let AcceptedRequest::Start(older) = requests.accept(&question, 1, now).unwrap() else {
        panic!("older resolution did not start")
    };
    requests.advance_generation();
    let AcceptedRequest::Start(newer) = requests.accept(&question, 2, now).unwrap() else {
        panic!("new settings did not start a resolution")
    };
    requests.rekey(older.id);
    requests.complete(older.id, Err(DnsError::IncompleteResponse), now);
    assert!(matches!(
        requests.accept(&question, 3, now).unwrap(),
        AcceptedRequest::Waiting
    ));
    assert_eq!(
        requests
            .complete(newer.id, Ok(a_answer(&question, 30, [1, 1, 1, 1])), now)
            .len(),
        2
    );
}

// A question arriving while a resolution it could share is running is checked
// against the allow rules first: an allowed one joins, a refused one is answered
// on its own and never joins, even with every resolution slot taken.
// @kotowari[EX-290]
#[test]
fn every_question_is_authorized_before_it_may_join_a_running_resolution() {
    use kakoi_net::dns::{AcceptedRequest, DnsRequests};
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let mut requests = DnsRequests::new(
        vec![dns_rule("api.example.com")],
        7,
        1,
        4,
        Duration::from_secs(2),
    )
    .unwrap();
    let allowed = query("api.example.com", 1);
    let AcceptedRequest::Start(task) = requests.accept(&allowed, 1, now).unwrap() else {
        panic!("the first question did not start a resolution")
    };

    assert!(matches!(
        requests.accept(&allowed, 2, now).unwrap(),
        AcceptedRequest::Waiting
    ));
    let AcceptedRequest::Answer(refused) =
        requests.accept(&query("evil.invalid", 1), 3, now).unwrap()
    else {
        panic!("a refused question was admitted")
    };
    assert_eq!(refused.recipient, 3);
    assert_eq!(refused.wire[3] & 15, 5);

    let mut answer = allowed.clone();
    answer[2] |= 0x80;
    let replies = requests.complete(task.id, Ok(answer), now);
    assert_eq!(
        replies
            .iter()
            .map(|reply| reply.recipient)
            .collect::<Vec<_>>(),
        [1, 2]
    );
}
