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
pub const NETWORK_AUTO: &str = "auto";
pub const NETWORK_OFF: &str = "off";
pub const NETWORK_STANDALONE: &str = "standalone";

/// Named network missing from the current region falls back to auto, with a notice.
pub fn resolve_network_choice(selected: &str, present: &[String]) -> (String, Option<String>) {
    let selected = selected.trim();
    if selected.is_empty() || selected.eq_ignore_ascii_case(NETWORK_AUTO) {
        return (NETWORK_AUTO.to_string(), None);
    }
    if selected.eq_ignore_ascii_case(NETWORK_OFF) {
        return (NETWORK_OFF.to_string(), None);
    }
    if selected.eq_ignore_ascii_case(NETWORK_STANDALONE) {
        return (NETWORK_STANDALONE.to_string(), None);
    }
    if present.iter().any(|n| n.eq_ignore_ascii_case(selected)) {
        (selected.to_string(), None)
    } else {
        (
            NETWORK_AUTO.to_string(),
            Some(format!(
                "{selected} has no programmable repeaters in this region; using nearest network"
            )),
        )
    }
}

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

/// One programmable site the host has already parsed. The guest never sees a path.
#[derive(Debug, Clone)]
pub struct LiveMember {
    pub site: FollowSite,
    pub network: String,
    pub distance_km: f64,
}

/// `auto` follows the nearest network that has a programmable member.
/// If none is in range, it follows standalone repeaters (no network).
/// A named network never includes standalones. `standalone` is only those.
/// `off` keeps nothing.
pub fn select_members(choice: &str, sites: &[LiveMember]) -> Vec<FollowSite> {
    let choice = choice.trim();
    if choice.is_empty() || choice.eq_ignore_ascii_case(NETWORK_OFF) {
        return Vec::new();
    }
    let is_standalone =
        |s: &LiveMember| s.network.is_empty() || s.network.eq_ignore_ascii_case(NETWORK_STANDALONE);
    if choice.eq_ignore_ascii_case(NETWORK_STANDALONE) {
        return sites
            .iter()
            .filter(|s| is_standalone(s))
            .map(|s| s.site.clone())
            .collect();
    }
    if choice.eq_ignore_ascii_case(NETWORK_AUTO) {
        let networked: Vec<&LiveMember> = sites.iter().filter(|s| !is_standalone(s)).collect();
        let best_network = networked
            .iter()
            .min_by(|a, b| {
                a.distance_km
                    .partial_cmp(&b.distance_km)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|s| s.network.clone());
        let Some(best_network) = best_network else {
            return sites
                .iter()
                .filter(|s| is_standalone(s))
                .map(|s| s.site.clone())
                .collect();
        };
        return networked
            .into_iter()
            .filter(|s| s.network.eq_ignore_ascii_case(&best_network))
            .map(|s| s.site.clone())
            .collect();
    }
    sites
        .iter()
        .filter(|s| s.network.eq_ignore_ascii_case(choice))
        .map(|s| s.site.clone())
        .collect()
}

/// KV keys. Values are site ids and follow state, never file paths.
pub const FOLLOW_STATE_KEY: &str = "cat_follow_state";
pub const FOLLOW_SWITCH_KEY: &str = "cat_follow_switch";

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

    #[test]
    fn missing_network_falls_back_to_auto_with_notice() {
        let (choice, notice) = resolve_network_choice("LA5MR", &["LA5MR".into()]);
        assert_eq!(choice, "LA5MR");
        assert!(notice.is_none());
        let (choice, notice) = resolve_network_choice("NO-SUCH", &["LA5MR".into()]);
        assert_eq!(choice, NETWORK_AUTO);
        assert!(notice.unwrap().contains("NO-SUCH"));
        assert_eq!(resolve_network_choice("off", &[]).0, NETWORK_OFF);
    }

    #[test]
    fn auto_selects_nearest_network_not_the_word_auto() {
        let sites = vec![
            LiveMember {
                site: FollowSite {
                    id: "far".into(),
                    callsign: "FAR".into(),
                    lat: 62.0,
                    lon: 10.0,
                    freq_out_mhz: 145.0,
                    shift_mhz: -0.6,
                    ctcss_hz: None,
                },
                network: "OTHER".into(),
                distance_km: 40.0,
            },
            LiveMember {
                site: FollowSite {
                    id: "near".into(),
                    callsign: "NEAR".into(),
                    lat: 61.1,
                    lon: 10.4,
                    freq_out_mhz: 145.6,
                    shift_mhz: -0.6,
                    ctcss_hz: None,
                },
                network: "LA5MR".into(),
                distance_km: 3.0,
            },
        ];
        let picked = select_members("auto", &sites);
        assert!(picked.iter().all(|s| s.id == "near"));
        assert!(select_members("off", &sites).is_empty());
        assert_eq!(select_members("LA5MR", &sites).len(), 1);
    }

    #[test]
    fn auto_falls_back_to_standalone_and_a_named_network_does_not() {
        let standalone = LiveMember {
            site: FollowSite {
                id: "solo".into(),
                callsign: "SOLO".into(),
                lat: 61.0,
                lon: 10.0,
                freq_out_mhz: 145.5,
                shift_mhz: -0.6,
                ctcss_hz: None,
            },
            network: String::new(),
            distance_km: 1.0,
        };
        let net = LiveMember {
            site: FollowSite {
                id: "net".into(),
                callsign: "NET".into(),
                lat: 61.2,
                lon: 10.2,
                freq_out_mhz: 145.6,
                shift_mhz: -0.6,
                ctcss_hz: None,
            },
            network: "LA5MR".into(),
            distance_km: 30.0,
        };
        let both = vec![standalone.clone(), net.clone()];
        assert_eq!(select_members("auto", &both)[0].id, "net");
        assert_eq!(select_members("standalone", &both)[0].id, "solo");
        assert_eq!(select_members("LA5MR", &both)[0].id, "net");
        assert!(select_members("auto", &[standalone.clone()])
            .iter()
            .any(|s| s.id == "solo"));
        assert!(select_members("LA5MR", &[standalone]).is_empty());
        let (choice, notice) = resolve_network_choice("standalone", &[]);
        assert_eq!(choice, NETWORK_STANDALONE);
        assert!(notice.is_none());
    }
}
