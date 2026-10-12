//! Network-follow hysteresis and leave-network (decision C).
//!
//! Mirrors `plugins/CATS-plugin/src/follow.rs` constants; site identity uses
//! stable site ids (OSM node/way ids), not callsign alone.

use serde::{Deserialize, Serialize};

use crate::repeater::RepeaterSite;

pub const HYSTERESIS_KM: f64 = 5.0;
pub const HYSTERESIS_FRAC: f64 = 0.20;
pub const DWELL_S: u64 = 30;
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
    /// Last networked site held after leave-network (decision C).
    pub held_site: Option<String>,
    pub leave_notified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FollowTick {
    /// No change.
    Hold,
    /// Retune to this site id.
    Switch { site_id: String },
    /// Decision C: hold last site, notify, exit follow.
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

/// Haversine distance in km.
pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 6371.0;
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * R * a.sqrt().asin()
}

pub fn interpolate_great_circle(
    lat1: f64,
    lon1: f64,
    lat2: f64,
    lon2: f64,
    frac: f64,
) -> (f64, f64) {
    // Simple linear lat/lon (adequate for ~200 km Espa→Dombås fixture path).
    (lat1 + (lat2 - lat1) * frac, lon1 + (lon2 - lon1) * frac)
}

/// Sample a polyline by cumulative distance; returns (lat, lon) at `frac` of length.
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

/// One follow tick at a position. Members must already be filtered to the
/// active network and usable NFM (complete freq/shift).
pub fn tick_at(
    state: &mut FollowState,
    lat: f64,
    lon: f64,
    members: &[RepeaterSite],
    now_unix_s: u64,
) -> FollowTick {
    if state.network_id.is_none() || state.stopped_reason.is_some() {
        return FollowTick::Hold;
    }
    let mut in_range: Vec<(&RepeaterSite, f64)> = Vec::new();
    for s in members {
        let (Some(la), Some(lo)) = (s.lat, s.lon) else {
            continue;
        };
        let d = haversine_km(lat, lon, la, lo);
        if d <= MAX_RANGE_KM {
            in_range.push((s, d));
        }
    }
    in_range.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    if in_range.is_empty() {
        // Decision C
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
    // Update distance to current if still tracked
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repeater::{RepeaterSite, RepeaterSource};

    fn site(id: &str, lat: f64, lon: f64) -> RepeaterSite {
        RepeaterSite {
            id: id.into(),
            callsign: id.into(),
            lat: Some(lat),
            lon: Some(lon),
            freq_out_mhz: 145.725,
            shift_mhz: -0.6,
            ctcss_hz: Some(88.5),
            dcs_code: None,
            color_code: None,
            modulation: "FM".into(),
            network_id: Some("LA5MR".into()),
            source: RepeaterSource::Osm,
            conflict: false,
            conflict_note: None,
            distance_km: None,
            position_accurate: true,
        }
    }

    #[test]
    fn leave_network_decision_c() {
        let mut st = FollowState {
            network_id: Some("LA5MR".into()),
            current_site: Some("LA9AR".into()),
            current_dist_km: Some(10.0),
            ..Default::default()
        };
        // Members far away
        let members = vec![site("LA9AR", 62.6, 10.0)];
        let tick = tick_at(&mut st, 60.0, 11.0, &members, 1000);
        match tick {
            FollowTick::LeaveNetwork { held_site, .. } => {
                assert_eq!(held_site, "LA9AR");
            }
            other => panic!("expected LeaveNetwork, got {other:?}"),
        }
        assert!(st.leave_notified);
        assert!(st.network_id.is_none());
        assert!(st.stopped_reason.is_some());
    }

    #[test]
    fn hysteresis_uses_site_id() {
        let state = FollowState {
            network_id: Some("LA5MR".into()),
            current_site: Some("n1".into()),
            current_dist_km: Some(20.0),
            last_switch_unix_s: 0,
            site_since_unix_s: 0,
            ..Default::default()
        };
        assert!(!should_switch(&state, "n2", 18.0, 1000));
        assert!(should_switch(&state, "n2", 14.0, 1000));
    }
}
