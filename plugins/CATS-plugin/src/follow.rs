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
    /// Switch handed to the host but not yet confirmed programmed. Holds the values
    /// to restore if the host reports that the radio did not accept it.
    #[serde(default)]
    pub pending: Option<PendingSwitch>,
    /// Site the radio refused after all attempts, and when (follow clock). The same
    /// site is not tried again for [`MIN_GAP_S`], so the user is not spammed.
    #[serde(default)]
    pub failed_site: Option<String>,
    #[serde(default)]
    pub failed_at_unix_s: u64,
}

/// State before a switch was committed, kept until the host confirms the program.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingSwitch {
    pub site_id: String,
    pub prev_site: Option<String>,
    pub prev_dist_km: Option<f64>,
    pub prev_site_since_unix_s: u64,
    pub prev_last_switch_unix_s: u64,
}

/// Host report after it tried to program a switch (`navi-cat` `FollowProgramReport`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgramResult {
    pub site_id: String,
    pub ok: bool,
    #[serde(default)]
    pub attempts: u32,
    #[serde(default)]
    pub notified: bool,
    #[serde(default)]
    pub at_unix_s: u64,
}

/// Apply the host program report to the follow state.
///
/// Success confirms the switch. Failure restores the state from before the switch
/// (the site is not "tuned") and remembers the failed site and time, so
/// [`tick_at`] holds off that site for [`MIN_GAP_S`] and then tries it again.
pub fn apply_program_result(state: &mut FollowState, result: &ProgramResult) {
    let pending = match state.pending.take() {
        Some(p) if p.site_id == result.site_id => Some(p),
        other => {
            // A report for another site than the one pending: keep the pending switch.
            state.pending = other;
            None
        }
    };
    if result.ok {
        if state.failed_site.as_deref() == Some(result.site_id.as_str()) {
            state.failed_site = None;
        }
        return;
    }
    match pending {
        Some(p) => {
            state.current_site = p.prev_site;
            state.current_dist_km = p.prev_dist_km;
            state.site_since_unix_s = p.prev_site_since_unix_s;
            state.last_switch_unix_s = p.prev_last_switch_unix_s;
        }
        None => {
            if state.current_site.as_deref() == Some(result.site_id.as_str()) {
                state.current_site = None;
                state.current_dist_km = None;
            }
        }
    }
    state.failed_site = Some(result.site_id.clone());
    state.failed_at_unix_s = result.at_unix_s;
}

/// Parse a host report from plugin KV (empty or invalid text is ignored) and apply it.
/// Returns the applied result so the caller can log it.
pub fn apply_program_result_json(state: &mut FollowState, json: &str) -> Option<ProgramResult> {
    let json = json.trim();
    if json.is_empty() {
        return None;
    }
    let result: ProgramResult = serde_json::from_str(json).ok()?;
    apply_program_result(state, &result);
    Some(result)
}

/// True while `site_id` is inside the cool-down after a failed program.
pub fn in_failure_cooldown(state: &FollowState, site_id: &str, now_unix_s: u64) -> bool {
    state.failed_site.as_deref() == Some(site_id)
        && now_unix_s.saturating_sub(state.failed_at_unix_s) < MIN_GAP_S
}

/// Commit a switch and remember what to restore if the radio refuses it.
fn commit_switch(state: &mut FollowState, site: &FollowSite, dist_km: f64, now_unix_s: u64) {
    state.pending = Some(PendingSwitch {
        site_id: site.id.clone(),
        prev_site: state.current_site.clone(),
        prev_dist_km: state.current_dist_km,
        prev_site_since_unix_s: state.site_since_unix_s,
        prev_last_switch_unix_s: state.last_switch_unix_s,
    });
    state.current_site = Some(site.id.clone());
    state.current_dist_km = Some(dist_km);
    state.site_since_unix_s = now_unix_s;
    state.last_switch_unix_s = now_unix_s;
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

/// The follow state is persisted by the host and outlives the process, but the
/// follow clock does not: a simulated drive stores timestamps ahead of real time,
/// and the next process starts at real time again. Timestamps in the future would
/// make dwell and gap checks fail until real time catches up, so they are clamped
/// to `now_unix_s` (the dwell and gap then restart from now).
fn clamp_future_timestamps(state: &mut FollowState, now_unix_s: u64) {
    state.site_since_unix_s = state.site_since_unix_s.min(now_unix_s);
    state.last_switch_unix_s = state.last_switch_unix_s.min(now_unix_s);
    state.failed_at_unix_s = state.failed_at_unix_s.min(now_unix_s);
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
    clamp_future_timestamps(state, now_unix_s);
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

    // A held site that is not a member in range any more (network changed, standalone to
    // network, member out of range) has no valid distance: its stored value is stale, and the
    // hysteresis rule would keep the old site until a candidate beats that old distance. Drop
    // it so the nearest member is tuned at once.
    if let (Some(cur), None) = (state.current_site.as_deref(), state.pinned.as_ref()) {
        if !in_range.iter().any(|(s, _)| s.id == cur) {
            state.current_site = None;
            state.current_dist_km = None;
        }
    }

    let (best, best_d) = in_range[0];
    // The radio refused this site a moment ago: do not try it again, or tell the user
    // again, until the cool-down is over.
    if in_failure_cooldown(state, &best.id, now_unix_s) {
        return FollowTick::Hold;
    }
    if state.current_site.is_none() {
        commit_switch(state, best, best_d, now_unix_s);
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
        commit_switch(state, best, best_d, now_unix_s);
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

/// Parse host `repeater_query` rows (compact follow JSON) into live members.
/// Rows without id, lat or lon are dropped. The host has already removed rows the
/// rig must not tune (digital-only, invalid shift).
pub fn live_members_from_rows(rows: &[serde_json::Value]) -> Vec<LiveMember> {
    let mut live = Vec::new();
    for row in rows {
        let Some(id) = row.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(lat) = row.get("lat").and_then(|v| v.as_f64()) else {
            continue;
        };
        let Some(lon) = row.get("lon").and_then(|v| v.as_f64()) else {
            continue;
        };
        let str_field = |key: &str| {
            row.get(key)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        live.push(LiveMember {
            site: FollowSite {
                id: id.to_string(),
                callsign: str_field("callsign"),
                lat,
                lon,
                freq_out_mhz: row
                    .get("freq_out_mhz")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0),
                shift_mhz: row.get("shift_mhz").and_then(|v| v.as_f64()).unwrap_or(0.0),
                ctcss_hz: row.get("ctcss_hz").and_then(|v| v.as_f64()),
            },
            network: str_field("network_id"),
            distance_km: row
                .get("distance_km")
                .and_then(|v| v.as_f64())
                .unwrap_or(f64::MAX),
        });
    }
    live
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
/// Host report of the last program attempt (JSON [`ProgramResult`]); the guest
/// applies it at the start of the next tick and then clears it.
pub const FOLLOW_RESULT_KEY: &str = "cat_follow_result";

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
            ..Default::default()
        };
        assert!(!should_switch(&state, "B", 18.0, 1000));
        assert!(should_switch(&state, "B", 14.0, 1000));
    }

    #[test]
    fn timestamps_from_a_faster_clock_do_not_block_switching() {
        let site = |id: &str, lat: f64| FollowSite {
            id: id.into(),
            callsign: id.into(),
            lat,
            lon: 10.0,
            freq_out_mhz: 145.6,
            shift_mhz: -0.6,
            ctcss_hz: None,
        };
        let members = vec![site("A", 60.0), site("B", 61.0)];
        // Stored by an earlier simulated drive: three hours ahead of the real clock.
        let mut st = FollowState {
            network_id: Some("standalone".into()),
            current_site: Some("A".into()),
            current_dist_km: Some(5.0),
            last_switch_unix_s: 1_000_000 + 10_800,
            site_since_unix_s: 1_000_000 + 10_800,
            ..Default::default()
        };
        // Real time now: the vehicle is at B. Dwell and gap restart at `now`.
        assert_eq!(
            tick_at(&mut st, 61.0, 10.0, &members, 1_000_000),
            FollowTick::Hold
        );
        assert_eq!(st.site_since_unix_s, 1_000_000);
        let later = 1_000_000 + MIN_GAP_S + 1;
        assert_eq!(
            tick_at(&mut st, 61.0, 10.0, &members, later),
            FollowTick::Switch {
                site_id: "B".into()
            }
        );
    }

    #[test]
    fn held_site_outside_the_followed_set_is_replaced_at_once() {
        let site = |id: &str, lat: f64| FollowSite {
            id: id.into(),
            callsign: id.into(),
            lat,
            lon: 10.0,
            freq_out_mhz: 145.6,
            shift_mhz: -0.6,
            ctcss_hz: None,
        };
        // Held from an earlier standalone follow (distance 21 km); now following a network
        // whose members do not include it. The nearest member is 15 km away, which is not 20 %
        // closer than the stale 21 km, so the hysteresis rule alone would never switch.
        let members = vec![site("N1", 60.15), site("N2", 61.5)];
        let mut st = FollowState {
            network_id: Some("LA5MR".into()),
            current_site: Some("S".into()),
            current_dist_km: Some(21.0),
            last_switch_unix_s: 500,
            site_since_unix_s: 500,
            ..Default::default()
        };
        assert_eq!(
            tick_at(&mut st, 60.0, 10.0, &members, 1_000),
            FollowTick::Switch {
                site_id: "N1".into()
            }
        );
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
            ..Default::default()
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
        assert!(select_members("auto", std::slice::from_ref(&standalone))
            .iter()
            .any(|s| s.id == "solo"));
        assert!(select_members("LA5MR", &[standalone]).is_empty());
        let (choice, notice) = resolve_network_choice("standalone", &[]);
        assert_eq!(choice, NETWORK_STANDALONE);
        assert!(notice.is_none());
    }
    fn plain_site(id: &str, lat: f64) -> FollowSite {
        FollowSite {
            id: id.into(),
            callsign: id.into(),
            lat,
            lon: 10.0,
            freq_out_mhz: 145.6,
            shift_mhz: -0.6,
            ctcss_hz: None,
        }
    }

    fn failed(site: &str, at: u64) -> ProgramResult {
        ProgramResult {
            site_id: site.into(),
            ok: false,
            attempts: 3,
            notified: true,
            at_unix_s: at,
        }
    }

    #[test]
    fn failed_first_program_leaves_nothing_tuned_and_holds_the_site() {
        let members = vec![plain_site("A", 60.0)];
        let mut st = FollowState {
            network_id: Some("standalone".into()),
            ..Default::default()
        };
        assert_eq!(
            tick_at(&mut st, 60.0, 10.0, &members, 1_000),
            FollowTick::Switch {
                site_id: "A".into()
            }
        );
        assert_eq!(st.current_site.as_deref(), Some("A"));
        assert!(st.pending.is_some());

        apply_program_result(&mut st, &failed("A", 1_000));
        assert_eq!(st.current_site, None, "failed program is not tuned");
        assert!(st.pending.is_none());
        assert_eq!(st.failed_site.as_deref(), Some("A"));

        // Inside the cool-down: no second switch to the same site.
        for t in [1_005, 1_030, 1_000 + MIN_GAP_S - 1] {
            assert_eq!(
                tick_at(&mut st, 60.0, 10.0, &members, t),
                FollowTick::Hold,
                "t={t}"
            );
        }
        // After it: tried again.
        assert_eq!(
            tick_at(&mut st, 60.0, 10.0, &members, 1_000 + MIN_GAP_S),
            FollowTick::Switch {
                site_id: "A".into()
            }
        );
    }

    #[test]
    fn failed_switch_restores_the_previous_site_and_timers() {
        let members = vec![plain_site("A", 60.0), plain_site("B", 61.0)];
        let mut st = FollowState {
            network_id: Some("standalone".into()),
            current_site: Some("A".into()),
            current_dist_km: Some(60.0),
            site_since_unix_s: 100,
            last_switch_unix_s: 100,
            ..Default::default()
        };
        // Vehicle now at B; A is 111 km away, B is at the position.
        assert_eq!(
            tick_at(&mut st, 61.0, 10.0, &members, 1_000),
            FollowTick::Switch {
                site_id: "B".into()
            }
        );
        assert_eq!(st.site_since_unix_s, 1_000);
        apply_program_result(&mut st, &failed("B", 1_000));
        assert_eq!(st.current_site.as_deref(), Some("A"));
        // The distance to A is the one measured on the failed tick (A is 1 degree away).
        let d_a = haversine_km(61.0, 10.0, 60.0, 10.0);
        assert_eq!(st.current_dist_km, Some(d_a));
        assert_eq!(st.site_since_unix_s, 100);
        assert_eq!(st.last_switch_unix_s, 100);
        assert_eq!(
            tick_at(&mut st, 61.0, 10.0, &members, 1_010),
            FollowTick::Hold
        );
    }

    #[test]
    fn success_report_confirms_and_clears_an_old_failure() {
        let members = vec![plain_site("A", 60.0)];
        let mut st = FollowState {
            network_id: Some("standalone".into()),
            failed_site: Some("A".into()),
            failed_at_unix_s: 0,
            ..Default::default()
        };
        assert!(matches!(
            tick_at(&mut st, 60.0, 10.0, &members, 5_000),
            FollowTick::Switch { .. }
        ));
        let ok = ProgramResult {
            site_id: "A".into(),
            ok: true,
            attempts: 2,
            notified: false,
            at_unix_s: 5_000,
        };
        apply_program_result(&mut st, &ok);
        assert_eq!(st.current_site.as_deref(), Some("A"));
        assert!(st.pending.is_none());
        assert!(st.failed_site.is_none());
    }

    #[test]
    fn report_for_another_site_is_ignored() {
        let members = vec![plain_site("A", 60.0)];
        let mut st = FollowState {
            network_id: Some("standalone".into()),
            ..Default::default()
        };
        assert!(matches!(
            tick_at(&mut st, 60.0, 10.0, &members, 1_000),
            FollowTick::Switch { .. }
        ));
        apply_program_result(&mut st, &failed("OTHER", 1_000));
        assert_eq!(st.current_site.as_deref(), Some("A"));
        assert!(st.pending.is_some());
    }

    #[test]
    fn result_json_round_trip_and_garbage_is_ignored() {
        let mut st = FollowState::default();
        assert!(apply_program_result_json(&mut st, "").is_none());
        assert!(apply_program_result_json(&mut st, "not json").is_none());
        let r = apply_program_result_json(
            &mut st,
            r#"{"site_id":"A","ok":false,"attempts":3,"notified":true,"at_unix_s":42}"#,
        )
        .unwrap();
        assert!(!r.ok);
        assert_eq!(st.failed_site.as_deref(), Some("A"));
        assert_eq!(st.failed_at_unix_s, 42);
    }

    #[test]
    fn old_persisted_state_without_new_fields_still_loads() {
        let st: FollowState = serde_json::from_str(
            r#"{"network_id":"auto","current_site":"A","current_dist_km":1.0,"pinned":null,
                "last_switch_unix_s":1,"site_since_unix_s":1,"stopped_reason":null}"#,
        )
        .unwrap();
        assert!(st.pending.is_none() && st.failed_site.is_none());
    }
}
