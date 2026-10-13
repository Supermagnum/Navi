//! Hiking geometry spike regression: Gjendesheim → Leirvassbu → Skjolden.
//!
//! The 0.3.22-beta densify pads were inserted as waypoints and snapped
//! off-corridor, producing out-and-back spurs. Gap-fill now chains terrain
//! A* without pad waypoints.
//!
//! Spike thresholds: see [`helpers::spike_detect`].

mod helpers;

use helpers::spike_detect::{detect_route_spikes, SpikeKind};

/// Synthetic spur matching the densify-pad failure mode (out-and-back ~225 m).
#[test]
fn spike_detector_flags_densify_style_spur() {
    let tip = (61.5212728, 8.4423387);
    let base = (61.5192571, 8.4418935);
    let coords = vec![(61.5180, 8.4400), base, tip, base, (61.5175, 8.4395)];
    let vias = [
        (61.4943459, 8.8126376), // Gjendesheim
        (61.5485603, 8.2476038), // Leirvassbu
        (61.4908664, 7.598792),  // Skjolden
    ];
    let hits = detect_route_spikes(&coords, &vias);
    assert!(
        hits.iter()
            .any(|h| matches!(h.kind, SpikeKind::OutAndBack { .. })),
        "expected densify-style spur, got {hits:?}"
    );
}

/// Known-bad A3 polyline from evidence must still be detected as spiked
/// (guards the detector; live replan after the fix must not match this).
#[test]
fn a3_evidence_polyline_was_spiked() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../docs/evidence/a3-hiking/cabins-on/route-polyline.txt");
    let Ok(raw) = std::fs::read_to_string(&path) else {
        // Evidence may be absent in shallow checkouts.
        eprintln!("skip: missing {}", path.display());
        return;
    };
    let mut coords = Vec::new();
    for part in raw.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let mut it = part.split(',');
        let (Some(lon), Some(lat)) = (it.next(), it.next()) else {
            continue;
        };
        let (Ok(lon), Ok(lat)) = (lon.parse::<f64>(), lat.parse::<f64>()) else {
            continue;
        };
        coords.push((lat, lon));
    }
    assert!(coords.len() > 100, "polyline too short");
    let vias = [
        (61.4943459, 8.8126376),
        (61.5485603, 8.2476038),
        (61.4908664, 7.598792),
    ];
    let hits = detect_route_spikes(&coords, &vias);
    assert!(
        !hits.is_empty(),
        "pre-fix A3 evidence polyline should contain spikes"
    );
}
