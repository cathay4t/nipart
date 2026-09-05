// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use super::next_retry_wait;

const BUDGET: Duration = Duration::from_secs(30);

#[test]
fn test_quick_phase_waits_one_second() {
    assert_eq!(
        next_retry_wait(0, BUDGET, Duration::ZERO),
        Duration::from_secs(1)
    );
    assert_eq!(
        next_retry_wait(5, BUDGET, Duration::from_secs(5)),
        Duration::from_secs(1)
    );
}

#[test]
fn test_backoff_capped_by_max_retry_wait() {
    // 2^(6-5)=2, then capped at 2 forever.
    assert_eq!(
        next_retry_wait(6, BUDGET, Duration::from_secs(6)),
        Duration::from_secs(2)
    );
    assert_eq!(
        next_retry_wait(7, BUDGET, Duration::from_secs(8)),
        Duration::from_secs(2)
    );
    assert_eq!(
        next_retry_wait(8, BUDGET, Duration::from_secs(12)),
        Duration::from_secs(2)
    );
    // A huge retry count must not overflow or exceed the cap.
    assert_eq!(
        next_retry_wait(100, BUDGET, Duration::ZERO),
        Duration::from_secs(2)
    );
}

#[test]
fn test_wait_never_passes_budget_deadline() {
    // Remaining budget is smaller than the backoff: sleep only what is
    // left.
    assert_eq!(
        next_retry_wait(9, BUDGET, Duration::from_secs(25)),
        Duration::from_secs(2)
    );
    assert_eq!(
        next_retry_wait(9, BUDGET, Duration::from_secs(29)),
        Duration::from_secs(1)
    );
    // Budget exhausted: poll again immediately (outer timer gives up).
    assert_eq!(
        next_retry_wait(9, BUDGET, Duration::from_secs(30)),
        Duration::ZERO
    );
}

#[test]
fn test_long_budget_allows_full_backoff() {
    let long_budget = Duration::from_secs(300);
    assert_eq!(
        next_retry_wait(9, long_budget, Duration::from_secs(20)),
        Duration::from_secs(2)
    );
}
