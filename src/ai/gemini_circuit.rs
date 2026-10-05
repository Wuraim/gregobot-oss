//! Process-wide Gemini quota / billing circuit breaker.
//!
//! Production logs showed hundreds of `429 RESOURCE_EXHAUSTED` errors per hour
//! (proactive + RAG embeddings + /ask) after prepayment credits ran out. Once
//! tripped, callers skip Gemini until the cooldown expires or a later call
//! succeeds.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tracing::warn;

use crate::ai::gemini_error::GeminiApiError;

const CREDITS_COOLDOWN: Duration = Duration::from_secs(6 * 3600);
const RATE_LIMIT_COOLDOWN: Duration = Duration::from_secs(20 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TripKind {
    CreditsDepleted,
    RateLimited,
}

#[derive(Debug)]
struct State {
    open_until: Option<Instant>,
    kind: Option<TripKind>,
}

impl State {
    const fn new() -> Self {
        Self {
            open_until: None,
            kind: None,
        }
    }
}

static STATE: Mutex<State> = Mutex::new(State::new());

fn lock_state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// True when Gemini calls should be skipped.
pub fn is_open() -> bool {
    let mut state = lock_state();
    match state.open_until {
        Some(until) if Instant::now() < until => true,
        Some(_) => {
            state.open_until = None;
            state.kind = None;
            false
        }
        None => false,
    }
}

pub fn blocked_error() -> anyhow::Error {
    let kind = lock_state().kind;
    match kind {
        Some(TripKind::CreditsDepleted) => anyhow::anyhow!(
            "Gemini circuit open: prepayment credits depleted (retry after cooldown)"
        ),
        Some(TripKind::RateLimited) => {
            anyhow::anyhow!("Gemini circuit open: rate limited (retry after cooldown)")
        }
        None => anyhow::anyhow!("Gemini circuit open"),
    }
}

/// Classify a failed Gemini HTTP payload. `None` means do not trip.
pub fn classify_failure(http_status: u16, status: Option<&str>, message: &str) -> Option<TripKind> {
    let quota_status = status.is_some_and(|s| s.eq_ignore_ascii_case("RESOURCE_EXHAUSTED"));
    if http_status != 429 && !quota_status {
        return None;
    }
    if is_credits_depleted(message) {
        Some(TripKind::CreditsDepleted)
    } else {
        Some(TripKind::RateLimited)
    }
}

pub fn is_credits_depleted(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("credits are depleted")
        || lower.contains("prepayment credits")
        || (lower.contains("billing") && lower.contains("deplet"))
}

fn cooldown(kind: TripKind) -> Duration {
    match kind {
        TripKind::CreditsDepleted => CREDITS_COOLDOWN,
        TripKind::RateLimited => RATE_LIMIT_COOLDOWN,
    }
}

/// Trip the circuit if this looks like a quota / billing failure.
/// Returns `true` when admins should still be notified (first trip, or a
/// non-quota error that this function ignores).
pub fn record_failure(error: &GeminiApiError) -> bool {
    record_http(error.http_status, error.status.as_deref(), &error.message)
}

/// Same as [`record_failure`] from raw HTTP fields. Returns whether the caller
/// should still emit an admin alert.
pub fn record_http(http_status: u16, status: Option<&str>, message: &str) -> bool {
    let Some(kind) = classify_failure(http_status, status, message) else {
        return true;
    };

    let mut state = lock_state();
    let now = Instant::now();
    let already_open = state.open_until.is_some_and(|until| now < until);

    if already_open {
        return false;
    }

    let until = now + cooldown(kind);
    state.open_until = Some(until);
    state.kind = Some(kind);
    warn!(
        ?kind,
        cooldown_secs = cooldown(kind).as_secs(),
        "Gemini circuit opened — skipping further Gemini calls until cooldown"
    );
    true
}

/// A successful Gemini call means quota is usable again.
pub fn note_success() {
    let mut state = lock_state();
    if state.open_until.is_some() {
        warn!("Gemini circuit closed after a successful call");
    }
    state.open_until = None;
    state.kind = None;
}

#[cfg(test)]
pub fn reset_for_tests() {
    let mut state = lock_state();
    *state = State::new();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn classifies_credits_depleted() {
        let msg = "Your prepayment credits are depleted. Please go to AI Studio";
        assert_eq!(
            classify_failure(429, Some("RESOURCE_EXHAUSTED"), msg),
            Some(TripKind::CreditsDepleted)
        );
        assert!(is_credits_depleted(msg));
    }

    #[test]
    fn classifies_generic_429() {
        let msg = "Resource exhausted. Please try again later.";
        assert_eq!(
            classify_failure(429, Some("RESOURCE_EXHAUSTED"), msg),
            Some(TripKind::RateLimited)
        );
        assert!(!is_credits_depleted(msg));
    }

    #[test]
    fn ignores_non_quota_errors() {
        assert_eq!(
            classify_failure(400, Some("INVALID_ARGUMENT"), "bad request"),
            None
        );
        assert_eq!(classify_failure(503, None, "upstream overloaded"), None);
    }

    #[test]
    fn record_failure_alerts_only_on_first_trip() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_for_tests();
        let err = GeminiApiError::from_http(
            429,
            r#"{"error":{"code":429,"message":"Your prepayment credits are depleted.","status":"RESOURCE_EXHAUSTED"}}"#,
            "gemini-2.5-flash",
            "ask",
            Some(1),
        );
        assert!(record_failure(&err));
        assert!(is_open());
        assert!(!record_failure(&err));
        assert!(is_open());
        reset_for_tests();
        assert!(!is_open());
    }

    #[test]
    fn success_closes_circuit() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_for_tests();
        assert!(record_http(
            429,
            Some("RESOURCE_EXHAUSTED"),
            "Resource exhausted. Please try again later."
        ));
        assert!(is_open());
        note_success();
        assert!(!is_open());
        reset_for_tests();
    }
}
