//! Demo-mode corridor stepper (5 s per switch, labelled for UI).

use serde::{Deserialize, Serialize};

use crate::follow::{interpolate_polyline, tick_at, FollowState, FollowTick};
use crate::program::{program_vfo1_verified, ProgramRequest};
use crate::repeater::RepeaterSite;
use crate::types::RigBackend;

/// Default dwell between demo switches (issue #149).
pub const DEMO_STEP_SECS: u64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoStep {
    pub index: usize,
    pub site_id: String,
    pub callsign: String,
    pub lat: f64,
    pub lon: f64,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DemoPlan {
    pub route_name: String,
    pub steps: Vec<DemoStep>,
    pub step_secs: u64,
}

/// Build a demo plan by sampling a polyline and recording follow switches.
pub fn plan_demo_route(
    route_name: &str,
    polyline: &[(f64, f64)],
    members: &[RepeaterSite],
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
        // Advance simulated time enough for dwell/gap between samples.
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
    }
}

const DWELL_GAP: u64 = 60;

/// Apply one demo step to a backend (program VFO 1).
pub fn apply_demo_step(
    backend: &mut impl RigBackend,
    site: &RepeaterSite,
) -> Result<(), crate::program::ProgramError> {
    let req = ProgramRequest {
        freq_out_mhz: site.freq_out_mhz,
        shift_mhz: site.shift_mhz,
        ctcss_hz: site.ctcss_hz,
        dcs_code: site.dcs_code,
        mode: "FM".into(),
        passband_hz: 12_500,
        vfo: "VFOA".into(),
        leaves_follow: false,
    };
    program_vfo1_verified(backend, &req)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repeater::RepeaterSource;

    #[test]
    fn demo_plan_espa_dombas_has_steps() {
        let members = vec![
            RepeaterSite {
                id: "a".into(),
                callsign: "LA5MR".into(),
                lat: Some(60.85),
                lon: Some(10.90),
                freq_out_mhz: 145.625,
                shift_mhz: -0.6,
                ctcss_hz: Some(88.5),
                dcs_code: None,
                color_code: None,
                modulation: "11K2F3E".into(),
                network_id: Some("LA5MR".into()),
                source: RepeaterSource::Osm,
                conflict: false,
                conflict_note: None,
                distance_km: None,
                position_accurate: true,
            },
            RepeaterSite {
                id: "b".into(),
                callsign: "LA6JRR".into(),
                lat: Some(61.90),
                lon: Some(9.28),
                freq_out_mhz: 145.6625,
                shift_mhz: -0.6,
                ctcss_hz: Some(88.5),
                dcs_code: None,
                color_code: None,
                modulation: "11K2F3E".into(),
                network_id: Some("LA5MR".into()),
                source: RepeaterSource::Osm,
                conflict: false,
                conflict_note: None,
                distance_km: None,
                position_accurate: true,
            },
        ];
        let poly = [(60.563, 11.257), (62.076, 9.128)];
        let plan = plan_demo_route("Espa-Dombas", &poly, &members, "LA5MR");
        assert_eq!(plan.step_secs, 5);
        assert!(!plan.steps.is_empty());
        assert!(plan.steps[0].label.contains("Demo step"));
    }
}
