//! Conservative fetch throttle with scheduled jitter.

use serde::{Deserialize, Serialize};

/// Default active-use interval (45 min — middle of the 30–60 min band).
pub const DEFAULT_ACTIVE_INTERVAL_SECS: i64 = 45 * 60;

/// Manual "refresh now" floor (3 min — middle of the 2–5 min band).
pub const DEFAULT_MANUAL_MIN_INTERVAL_SECS: i64 = 3 * 60;

#[derive(Debug, Clone, Copy)]
pub struct ThrottleConfig {
    pub active_interval_secs: i64,
    pub manual_min_interval_secs: i64,
    /// Extra random delay added on top of the base interval (0..=jitter_secs).
    pub jitter_secs: i64,
}

impl Default for ThrottleConfig {
    fn default() -> Self {
        Self {
            active_interval_secs: DEFAULT_ACTIVE_INTERVAL_SECS,
            manual_min_interval_secs: DEFAULT_MANUAL_MIN_INTERVAL_SECS,
            jitter_secs: 180,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThrottleState {
    pub last_fetch_unix: Option<i64>,
    pub last_manual_unix: Option<i64>,
    pub next_scheduled_unix: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchDecision {
    Fetch { scheduled_next: i64 },
    ServeCache { reason: String },
}

/// Decide whether a network fetch is allowed.
///
/// Scheduled polls never land on exact clock boundaries: the next fire time
/// always includes a positive jitter offset.
pub fn decide_fetch(
    state: &ThrottleState,
    now_unix: i64,
    manual: bool,
    cfg: ThrottleConfig,
) -> FetchDecision {
    if manual {
        if let Some(last) = state.last_manual_unix {
            let elapsed = now_unix.saturating_sub(last);
            if elapsed < cfg.manual_min_interval_secs {
                return FetchDecision::ServeCache {
                    reason: format!(
                        "manual_throttled:{}s_remaining",
                        cfg.manual_min_interval_secs - elapsed
                    ),
                };
            }
        }
        let scheduled_next = now_unix + jittered_interval_secs(cfg);
        return FetchDecision::Fetch { scheduled_next };
    }

    if let Some(next) = state.next_scheduled_unix {
        if now_unix < next {
            return FetchDecision::ServeCache {
                reason: format!("scheduled_wait:{}s", next - now_unix),
            };
        }
    } else if let Some(last) = state.last_fetch_unix {
        let elapsed = now_unix.saturating_sub(last);
        if elapsed < cfg.active_interval_secs {
            return FetchDecision::ServeCache {
                reason: format!("interval_wait:{}s", cfg.active_interval_secs - elapsed),
            };
        }
    }

    let scheduled_next = now_unix + jittered_interval_secs(cfg);
    // Guard against exact hour/minute boundaries for the *next* poll time.
    let scheduled_next = avoid_clock_boundary(scheduled_next);
    FetchDecision::Fetch { scheduled_next }
}

/// Base interval plus 1..=jitter_secs (never zero jitter on the schedule).
pub fn jittered_interval_secs(cfg: ThrottleConfig) -> i64 {
    let j = (pseudo_jitter_u32() % (cfg.jitter_secs.max(1) as u32)) as i64 + 1;
    cfg.active_interval_secs + j
}

fn avoid_clock_boundary(ts: i64) -> i64 {
    let rem = ts.rem_euclid(60);
    if rem == 0 {
        ts + 7 + (pseudo_jitter_u32() % 13) as i64
    } else if rem < 3 {
        ts + 5
    } else {
        ts
    }
}

fn pseudo_jitter_u32() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(1);
    nanos.wrapping_mul(2654435761)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_bypasses_active_interval_but_has_floor() {
        let state = ThrottleState {
            last_fetch_unix: Some(1_000),
            last_manual_unix: Some(1_000),
            next_scheduled_unix: Some(1_000 + DEFAULT_ACTIVE_INTERVAL_SECS),
        };
        let cfg = ThrottleConfig::default();
        match decide_fetch(&state, 1_000 + 30, true, cfg) {
            FetchDecision::ServeCache { reason } => assert!(reason.contains("manual_throttled")),
            other => panic!("expected throttle, got {other:?}"),
        }
        match decide_fetch(&state, 1_000 + cfg.manual_min_interval_secs + 1, true, cfg) {
            FetchDecision::Fetch { .. } => {}
            other => panic!("expected fetch, got {other:?}"),
        }
    }

    #[test]
    fn scheduled_wait_serves_cache() {
        let state = ThrottleState {
            last_fetch_unix: Some(1_000),
            last_manual_unix: None,
            next_scheduled_unix: Some(2_000),
        };
        match decide_fetch(&state, 1_500, false, ThrottleConfig::default()) {
            FetchDecision::ServeCache { .. } => {}
            other => panic!("expected cache, got {other:?}"),
        }
    }

    #[test]
    fn jittered_interval_exceeds_base() {
        let cfg = ThrottleConfig::default();
        for _ in 0..20 {
            let v = jittered_interval_secs(cfg);
            assert!(v > cfg.active_interval_secs);
            assert!(v <= cfg.active_interval_secs + cfg.jitter_secs);
        }
    }

    #[test]
    fn avoid_exact_minute_boundary() {
        assert_ne!(avoid_clock_boundary(3_600), 3_600);
    }
}
