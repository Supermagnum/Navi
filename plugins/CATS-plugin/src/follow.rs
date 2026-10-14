//! Network follow: hysteresis, dwell, pin, leave-network decision C.
//!
//! Runs in the WASM guest. Host only applies VFO programs / interlocks.

use serde::{Deserialize, Serialize};

/// Hysteresis: switch when new site is ≥ 5 km closer OR ≥ 20 % closer.
pub const HYSTERESIS_KM: f64 = 5.0;
pub const HYSTERESIS_FRAC: f64 = 0.20;
/// Minimum dwell on a site before switch (seconds).
pub const DWELL_S: u64 = 30;
/// Minimum gap between switches (seconds).
pub const MIN_GAP_S: u64 = 60;
pub const MAX_RANGE_KM: f64 = 150.0;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FollowState {
    pub network_id: Option<String>,
    pub current_site: Option<String>,
    pub current_dist_km: Option<f64>,
    pub pinned: Option<String>,
    pub last_switch_unix_s: u64,
    pub site_since_unix_s: u64,
    pub stopped_reason: Option<String>,
    /// Decision C: last networked site held after leave-network.
    #[serde(default)]
    pub held_site: Option<String>,
    #[serde(default)]
    pub leave_notified: bool,
}

/// Minimal site fields for follow (guest-side; host DB maps into this).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowSite {
    pub id: String,
    pub callsign: String,
    pub lat: f64,
    pub lon: f64,
    pub freq_out_mhz: f64,
    pub shift_mhz: f64,
    pub ctcss_hz: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FollowTick {
    Hold,
    Switch { site_id: String },
    LeaveNetwork { held_site: String, reason: String },
}

pub fn should_switch(
    state: &FollowState,
    candidate_site_id: &str,
    candidate_dist_km: f64,
    now_unix_s: u64,
) -> bool {
    if state.pinned.is_some() {
        return false;
    }
    if state.stopped_reason.is_some() {
        return false;
    }
    if state.current_site.as_deref() == Some(candidate_site_id) {
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

pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 6371.0;
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * R * a.sqrt().asin()
}

pub fn interpolate_polyline(points: &[(f64, f64)], frac: f64) -> Option<(f64, f64)> {
    if points.is_empty() {
        return None;
    }
    if points.len() == 1 || frac <= 0.0 {
        return Some(points[0]);
    }
    if frac >= 1.0 {
        return Some(*points.last().unwrap());
    }
    let mut seg_lens = Vec::new();
    let mut total = 0.0;
    for w in points.windows(2) {
        let d = haversine_km(w[0].0, w[0].1, w[1].0, w[1].1);
        seg_lens.push(d);
        total += d;
    }
    if total <= 0.0 {
        return Some(points[0]);
    }
    let target = total * frac;
    let mut acc = 0.0;
    for (i, &d) in seg_lens.iter().enumerate() {
        if acc + d >= target {
            let t = if d > 0.0 { (target - acc) / d } else { 0.0 };
            let a = points[i];
            let b = points[i + 1];
            return Some((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
        }
        acc += d;
    }
    Some(*points.last().unwrap())
}

/// Parse Navi `lon,lat;lon,lat;…` into `(lat, lon)` samples.
pub fn parse_navi_polyline(s: &str) -> Vec<(f64, f64)> {
    s.split(';')
        .filter_map(|part| {
            let mut it = part.split(',');
            let lon: f64 = it.next()?.parse().ok()?;
            let lat: f64 = it.next()?.parse().ok()?;
            Some((lat, lon))
        })
        .collect()
}

pub fn tick_at(
    state: &mut FollowState,
    lat: f64,
    lon: f64,
    members: &[FollowSite],
    now_unix_s: u64,
) -> FollowTick {
    if state.network_id.is_none() || state.stopped_reason.is_some() {
        return FollowTick::Hold;
    }
    let mut in_range: Vec<(&FollowSite, f64)> = Vec::new();
    for s in members {
        let d = haversine_km(lat, lon, s.lat, s.lon);
        if d <= MAX_RANGE_KM {
            in_range.push((s, d));
        }
    }
    in_range.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    if in_range.is_empty() {
        let held = state
            .current_site
            .clone()
            .or_else(|| state.held_site.clone())
            .unwrap_or_else(|| "unknown".into());
        let reason = "network left behind: no member within 150 km".to_string();
        state.held_site = Some(held.clone());
        state.leave_notified = true;
        state.stopped_reason = Some(reason.clone());
        state.network_id = None;
        return FollowTick::LeaveNetwork {
            held_site: held,
            reason,
        };
    }

    let (best, best_d) = in_range[0];
    if state.current_site.is_none() {
        state.current_site = Some(best.id.clone());
        state.current_dist_km = Some(best_d);
        state.site_since_unix_s = now_unix_s;
        state.last_switch_unix_s = now_unix_s;
        return FollowTick::Switch {
            site_id: best.id.clone(),
        };
    }
    if let Some(cur_id) = state.current_site.clone() {
        if let Some((_, d)) = in_range.iter().find(|(s, _)| s.id == cur_id) {
            state.current_dist_km = Some(*d);
        }
    }
    if should_switch(state, &best.id, best_d, now_unix_s) {
        state.current_site = Some(best.id.clone());
        state.current_dist_km = Some(best_d);
        state.site_since_unix_s = now_unix_s;
        state.last_switch_unix_s = now_unix_s;
        return FollowTick::Switch {
            site_id: best.id.clone(),
        };
    }
    FollowTick::Hold
}

/// Legacy stub used by `plugin_main` until host wires full member JSON.
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
            held_site: None,
            leave_notified: false,
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
            held_site: None,
            leave_notified: false,
        };
        assert!(!should_switch(&state, "B", 1.0, 1000));
    }

    #[test]
    fn leave_network_decision_c() {
        let mut st = FollowState {
            network_id: Some("LA5MR".into()),
            current_site: Some("LA9AR".into()),
            current_dist_km: Some(10.0),
            ..Default::default()
        };
        let members = vec![FollowSite {
            id: "LA9AR".into(),
            callsign: "LA9AR".into(),
            lat: 62.6,
            lon: 10.0,
            freq_out_mhz: 145.6,
            shift_mhz: -0.6,
            ctcss_hz: Some(88.5),
        }];
        let tick = tick_at(&mut st, 60.0, 11.0, &members, 1000);
        assert!(matches!(tick, FollowTick::LeaveNetwork { .. }));
        assert!(st.leave_notified);
        assert!(st.network_id.is_none());
    }
}
