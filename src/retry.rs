//! Retry policy for network requests.
//!
//! The decision-making lives here, apart from the code that performs the I/O,
//! so it can be exercised directly: both functions below are pure, and
//! `backoff_delay` takes its randomness as an argument rather than drawing it
//! internally. `api.rs` supplies the jitter and does the waiting.

use std::time::Duration;

/// How many times a single request is attempted before giving up. Four means
/// three retries after the first try, which is enough to ride out a transient
/// blip without leaving the user watching a spinner through a real outage.
pub const MAX_ATTEMPTS: u32 = 4;

/// Base delay for the backoff schedule.
pub const BACKOFF_BASE: Duration = Duration::from_millis(250);

/// Ceiling on any single backoff delay. This is an interactive TUI: a user who
/// pressed a key is waiting, so the schedule is deliberately shorter than a
/// batch client's would be.
pub const BACKOFF_CAP: Duration = Duration::from_secs(4);

/// The failure classes we distinguish when deciding whether to try again.
///
/// Derived from a `reqwest::Error` at the call site rather than matched on
/// directly, because `reqwest::Error` cannot be constructed in a test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// Timed out, or the connection never established. No response arrived.
    Transport,
    /// The server responded with this status code.
    Status(u16),
    /// A response arrived but could not be decoded into the expected shape.
    Decode,
}

/// Whether a failed attempt is worth repeating.
///
/// Every request this client makes is an idempotent `GET`, so the question is
/// only whether the failure is likely to be transient — not whether repeating
/// it is safe.
pub fn is_retryable(kind: FailureKind) -> bool {
    match kind {
        // Nothing was received, so nothing was learned. Worth another look.
        FailureKind::Transport => true,
        // 5xx is the server having a bad moment; 429 is it asking us to slow
        // down, which is a request to come back later rather than a refusal.
        // Every other status is a definitive answer — a 404 for a species that
        // does not exist is *correct*, and asking three more times wastes the
        // user's time and PokeAPI's.
        FailureKind::Status(code) => code >= 500 || code == 429,
        // Re-fetching will not make a malformed payload parse. That is our bug
        // or a breaking API change, and retrying only delays the report.
        FailureKind::Decode => false,
    }
}

/// Delay before the retry following `attempt` (0-based), using full jitter.
///
/// The exponential ceiling doubles per attempt up to [`BACKOFF_CAP`], and the
/// actual delay is a uniform draw from zero to that ceiling. Picking a point in
/// the range rather than adding a small wobble to a fixed delay is what
/// decorrelates clients — and it matters here even for one user, because a
/// branching evolution chain dispatches its sprite requests together, so their
/// failures and therefore their retries would otherwise land in lockstep.
///
/// `jitter` is the caller's uniform draw from `[0, 1)`; values outside that
/// range are clamped.
pub fn backoff_delay(attempt: u32, jitter: f64) -> Duration {
    let ceiling = BACKOFF_BASE
        .saturating_mul(2u32.saturating_pow(attempt.min(16)))
        .min(BACKOFF_CAP);
    ceiling.mul_f64(jitter.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_species_is_not_retried() {
        assert!(!is_retryable(FailureKind::Status(404)));
    }

    #[test]
    fn client_errors_are_final() {
        for code in [400, 401, 403, 404, 410, 418] {
            assert!(!is_retryable(FailureKind::Status(code)), "{code}");
        }
    }

    #[test]
    fn server_errors_are_retried() {
        for code in [500, 502, 503, 504] {
            assert!(is_retryable(FailureKind::Status(code)), "{code}");
        }
    }

    #[test]
    fn being_rate_limited_is_retried() {
        assert!(is_retryable(FailureKind::Status(429)));
    }

    #[test]
    fn a_success_status_is_never_classified_as_retryable() {
        assert!(!is_retryable(FailureKind::Status(200)));
        assert!(!is_retryable(FailureKind::Status(304)));
    }

    #[test]
    fn timeouts_and_connection_failures_are_retried() {
        assert!(is_retryable(FailureKind::Transport));
    }

    #[test]
    fn a_malformed_body_is_not_retried() {
        // Re-fetching will not make the payload parse; this is our bug or a
        // breaking API change, and retrying only delays the error.
        assert!(!is_retryable(FailureKind::Decode));
    }

    #[test]
    fn the_backoff_ceiling_doubles_per_attempt() {
        // Jitter of 1.0 draws the top of the range, which is the ceiling.
        assert_eq!(backoff_delay(0, 1.0), BACKOFF_BASE);
        assert_eq!(backoff_delay(1, 1.0), BACKOFF_BASE * 2);
        assert_eq!(backoff_delay(2, 1.0), BACKOFF_BASE * 4);
    }

    #[test]
    fn the_delay_never_exceeds_the_cap() {
        for attempt in 0..64 {
            assert!(backoff_delay(attempt, 1.0) <= BACKOFF_CAP, "{attempt}");
        }
    }

    #[test]
    fn every_delay_stays_within_its_jittered_range() {
        for attempt in 0..8 {
            let ceiling = backoff_delay(attempt, 1.0);
            for step in 0..=10 {
                let delay = backoff_delay(attempt, step as f64 / 10.0);
                assert!(delay <= ceiling, "attempt {attempt}, step {step}");
            }
        }
    }

    #[test]
    fn full_jitter_can_draw_the_bottom_of_the_range() {
        assert_eq!(backoff_delay(3, 0.0), Duration::ZERO);
    }

    #[test]
    fn jitter_outside_the_unit_range_is_clamped() {
        assert_eq!(backoff_delay(1, 5.0), backoff_delay(1, 1.0));
        assert_eq!(backoff_delay(1, -5.0), backoff_delay(1, 0.0));
    }

    #[test]
    fn a_large_attempt_number_does_not_overflow() {
        // Saturating arithmetic rather than a panic in release-mode wrapping.
        assert_eq!(backoff_delay(u32::MAX, 1.0), BACKOFF_CAP);
    }
}
