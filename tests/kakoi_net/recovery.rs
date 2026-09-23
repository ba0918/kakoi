use kakoi_net::recovery::{Recovery, RecoveryAction};
use std::time::{Duration, Instant};

// @kotowari[REQ-066, REQ-143, EX-124, EX-125, EX-319]
#[test]
fn recovery_waits_without_overlap_and_resets_after_success() {
    let mut clock = Instant::now();
    let mut recovery = Recovery::new(10).unwrap();
    assert_eq!(recovery.poll(clock), RecoveryAction::Wait);
    recovery.isolate(clock);
    for delay in [1, 2, 4, 8, 16, 30, 30, 30] {
        let RecoveryAction::Start(attempt) = recovery.poll(clock) else {
            panic!("attempt not started")
        };
        assert_eq!(attempt.deadline(), clock + Duration::from_secs(10));
        assert_eq!(recovery.poll(clock), RecoveryAction::Wait);
        assert!(!recovery.complete(attempt, false, clock).unwrap());
        assert!(recovery.complete(attempt, true, clock).is_err());
        assert_eq!(
            recovery.poll(clock + Duration::from_secs(delay) - Duration::from_nanos(1)),
            RecoveryAction::Wait
        );
        clock += Duration::from_secs(delay);
    }
    let RecoveryAction::Start(attempt) = recovery.poll(clock) else {
        panic!("attempt not started")
    };
    assert!(recovery.complete(attempt, true, clock).unwrap());
    assert_eq!(
        recovery.poll(clock + Duration::from_secs(100)),
        RecoveryAction::Wait
    );
    recovery.isolate(clock);
    let RecoveryAction::Start(attempt) = recovery.poll(clock) else {
        panic!("attempt not started")
    };
    assert!(!recovery.complete(attempt, false, clock).unwrap());
    assert_eq!(
        recovery.poll(clock + Duration::from_millis(999)),
        RecoveryAction::Wait
    );
    assert!(matches!(
        recovery.poll(clock + Duration::from_secs(1)),
        RecoveryAction::Start(_)
    ));
}

// @kotowari[REQ-066, REQ-143]
#[test]
fn expired_recovery_is_cancelled_once_and_cannot_overlap_or_adopt_a_late_success() {
    assert!(Recovery::new(0).is_err());
    assert!(Recovery::new(301).is_err());
    for seconds in [1, 10, 300] {
        let clock = Instant::now();
        let mut recovery = Recovery::new(seconds).unwrap();
        recovery.isolate(clock);
        let RecoveryAction::Start(attempt) = recovery.poll(clock) else {
            panic!("attempt not started")
        };
        assert_eq!(
            recovery.poll(attempt.deadline()),
            RecoveryAction::Cancel(attempt)
        );
        recovery.isolate(attempt.deadline());
        assert_eq!(
            recovery.poll(attempt.deadline() + Duration::from_secs(600)),
            RecoveryAction::Wait
        );
        assert!(!recovery
            .complete(attempt, true, attempt.deadline())
            .unwrap());
        let RecoveryAction::Start(next) =
            recovery.poll(attempt.deadline() + Duration::from_secs(1))
        else {
            panic!("retry not started")
        };
        assert_ne!(attempt, next);
        assert!(recovery.complete(attempt, true, next.deadline()).is_err());
        assert_eq!(recovery.poll(next.deadline()), RecoveryAction::Cancel(next));
    }
}
