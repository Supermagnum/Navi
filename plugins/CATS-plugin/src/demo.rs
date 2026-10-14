//! Demo-mode corridor sequencing (developer / test only).
//!
//! Cadence and switch order live in the WASM guest. Host applies VFO programs
//! and enforces interlocks / read-back. Off by default; UI must label clearly.

use crate::follow::{interpolate_polyline, tick_at, FollowSite, FollowState, FollowTick};
use serde::{Deserialize, Serialize};

/// Default dwell between demo switches (issue #149).
pub const DEMO_STEP_SECS: u64 = 5;

/// Tolerance when asserting 5 s cadence in tests (±0.5 s wall, or ±1 sample).
pub const DEMO_CADENCE_TOLERANCE_SECS: f64 = 0.5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoStep {
    pub index: usize,
    pub site_id: String,
    pub callsign: String,
    pub lat: f64,
    pub lon: f64,
    pub label: String,
    /// Simulated unix seconds when this step fires.
    pub at_unix_s: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DemoPlan {
    pub route_name: String,
    pub steps: Vec<DemoStep>,
    pub step_secs: u64,
    /// True when this plan is for developer/test demo mode (never silent product path).
    pub developer_test_only: bool,
}

/// Advance simulated time enough for dwell/gap between polyline samples.
const DWELL_GAP: u64 = 60;

/// Build a demo plan by sampling a polyline and recording follow switches.
pub fn plan_demo_route(
    route_name: &str,
    polyline: &[(f64, f64)],
    members: &[FollowSite],
    network_id: &str,
) -> DemoPlan {
    let mut state = FollowState {
        network_id: Some(network_id.into()),
        ..Default::default()
    };
    let mut steps = Vec::new();
    let n = 200usize;
    for i in 0..=n {
        let frac = i as f64 / n as f64;
        let Some((lat, lon)) = interpolate_polyline(polyline, frac) else {
            continue;
        };
        let now = i as u64 * DEMO_STEP_SECS.max(DWELL_GAP);
        match tick_at(&mut state, lat, lon, members, now) {
            FollowTick::Switch { site_id } => {
                if let Some(s) = members.iter().find(|m| m.id == site_id) {
                    steps.push(DemoStep {
                        index: steps.len(),
                        site_id: site_id.clone(),
                        callsign: s.callsign.clone(),
                        lat,
                        lon,
                        label: format!(
                            "Demo step {}: {} ({})",
                            steps.len() + 1,
                            s.callsign,
                            site_id
                        ),
                        at_unix_s: now,
                    });
                }
            }
            FollowTick::LeaveNetwork { held_site, .. } => {
                steps.push(DemoStep {
                    index: steps.len(),
                    site_id: held_site.clone(),
                    callsign: held_site,
                    lat,
                    lon,
                    label: "Demo: leave network (hold last site, notify)".into(),
                    at_unix_s: now,
                });
                break;
            }
            FollowTick::Hold => {}
        }
    }
    DemoPlan {
        route_name: route_name.into(),
        steps,
        step_secs: DEMO_STEP_SECS,
        developer_test_only: true,
    }
}

/// Wall-clock schedule: step `i` fires at `start + i * DEMO_STEP_SECS`.
pub fn demo_schedule_unix_s(start_unix_s: u64, step_index: usize) -> u64 {
    start_unix_s + step_index as u64 * DEMO_STEP_SECS
}

/// True if observed gap is within [`DEMO_CADENCE_TOLERANCE_SECS`] of [`DEMO_STEP_SECS`].
pub fn cadence_ok(gap_secs: f64) -> bool {
    (gap_secs - DEMO_STEP_SECS as f64).abs() <= DEMO_CADENCE_TOLERANCE_SECS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_plan_has_labelled_steps() {
        let members = vec![
            FollowSite {
                id: "a".into(),
                callsign: "LA5MR".into(),
                lat: 60.85,
                lon: 10.90,
                freq_out_mhz: 145.625,
                shift_mhz: -0.6,
                ctcss_hz: Some(88.5),
            },
            FollowSite {
                id: "b".into(),
                callsign: "LA6JRR".into(),
                lat: 61.90,
                lon: 9.28,
                freq_out_mhz: 145.6625,
                shift_mhz: -0.6,
                ctcss_hz: Some(88.5),
            },
        ];
        let poly = [(60.563, 11.257), (62.076, 9.128)];
        let plan = plan_demo_route("Espa-Dombas", &poly, &members, "LA5MR");
        assert_eq!(plan.step_secs, 5);
        assert!(plan.developer_test_only);
        assert!(!plan.steps.is_empty());
        assert!(plan.steps[0].label.contains("Demo step"));
    }

    #[test]
    fn cadence_tolerance() {
        assert!(cadence_ok(5.0));
        assert!(cadence_ok(5.4));
        assert!(!cadence_ok(6.0));
    }
}
