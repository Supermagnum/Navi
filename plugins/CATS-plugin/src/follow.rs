//! Network follow loop: hysteresis, dwell, pin, PTT/DCD left to host.

use serde::{Deserialize, Serialize};

/// Hysteresis: switch when new site is ≥ 5 km closer OR ≥ 20 % closer.
pub const HYSTERESIS_KM: f64 = 5.0;
pub const HYSTERESIS_FRAC: f64 = 0.20;
/// Minimum dwell on a site before switch (seconds).
pub const DWELL_S: u64 = 30;
/// Minimum gap between switches (seconds).
pub const MIN_GAP_S: u64 = 60;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FollowState {
    pub network_id: Option<String>,
    pub current_site: Option<String>,
    pub current_dist_km: Option<f64>,
    pub pinned: Option<String>,
    pub last_switch_unix_s: u64,
    pub site_since_unix_s: u64,
    pub stopped_reason: Option<String>,
}

pub fn should_switch(
    state: &FollowState,
    candidate_callsign: &str,
    candidate_dist_km: f64,
    now_unix_s: u64,
) -> bool {
    if state.pinned.is_some() {
        return false;
    }
    if state.stopped_reason.is_some() {
        return false;
    }
    if state.current_site.as_deref() == Some(candidate_callsign) {
        return false;
    }
    let Some(cur_d) = state.current_dist_km else {
        return true;
    };
    if now_unix_s.saturating_sub(state.site_since_unix_s) < DWELL_S {
        return false;
    }
    if now_unix_s.saturating_sub(state.last_switch_unix_s) < MIN_GAP_S {
        return false;
    }
    let closer_km = cur_d - candidate_dist_km;
    closer_km >= HYSTERESIS_KM || closer_km >= cur_d * HYSTERESIS_FRAC
}

pub fn tick_follow(lat: f64, lon: f64) -> FollowState {
    let _ = (lat, lon);
    FollowState::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hysteresis_prevents_flap() {
        let state = FollowState {
            network_id: Some("LA5MR".into()),
            current_site: Some("A".into()),
            current_dist_km: Some(20.0),
            pinned: None,
            last_switch_unix_s: 0,
            site_since_unix_s: 0,
            stopped_reason: None,
        };
        assert!(!should_switch(&state, "B", 18.0, 1000));
        assert!(should_switch(&state, "B", 14.0, 1000));
    }

    #[test]
    fn pin_blocks_switch() {
        let state = FollowState {
            network_id: Some("LA5MR".into()),
            current_site: Some("A".into()),
            current_dist_km: Some(20.0),
            pinned: Some("A".into()),
            last_switch_unix_s: 0,
            site_since_unix_s: 0,
            stopped_reason: None,
        };
        assert!(!should_switch(&state, "B", 1.0, 1000));
    }
}
