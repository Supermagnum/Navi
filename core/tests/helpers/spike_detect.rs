//! Route geometry spike detector (test helper only).
//!
//! ## Thresholds
//!
//! - **Out-and-back:** consecutive segment turn angle ≥ [`UTURN_DEG`] (165°)
//!   and each leg of the turn longer than [`BACKTRACK_MIN_M`] (50 m), unless the
//!   vertex is within [`VIA_TOLERANCE_M`] (75 m) of a declared via / endpoint
//!   (allowed U-turn / rejoin at a real via).
//! - **Lateral offset:** vertex distance from the chord of its neighbours
//!   exceeds [`OFFSET_ABS_M`] (200 m) **and** is more than
//!   [`OFFSET_REL_OF_CHORD`] (0.15) of the chord length, with chord longer than
//!   [`OFFSET_MIN_CHORD_M`] (100 m). Both adjacent legs must be shorter than
//!   [`OFFSET_MAX_LEG_M`] (800 m) so sparse export shapes (bicycle bbox graphs
//!   with multi-km steps around lakes) do not false-positive on smooth curves.

#![allow(dead_code)]

/// Turn angle at which we treat the vertex as a U-turn candidate (degrees).
pub const UTURN_DEG: f64 = 165.0;
/// Minimum out/back leg length for a U-turn to count as a spike (metres).
pub const BACKTRACK_MIN_M: f64 = 50.0;
/// Via/endpoint proximity that exempts a U-turn (metres).
pub const VIA_TOLERANCE_M: f64 = 75.0;
/// Absolute cross-track threshold for offset spikes (metres).
pub const OFFSET_ABS_M: f64 = 200.0;
/// Relative cross-track vs chord length.
pub const OFFSET_REL_OF_CHORD: f64 = 0.15;
/// Minimum chord length before offset spikes are considered (metres).
pub const OFFSET_MIN_CHORD_M: f64 = 100.0;
/// Skip offset checks when either adjacent leg is longer than this (metres).
pub const OFFSET_MAX_LEG_M: f64 = 800.0;

#[derive(Debug, Clone, PartialEq)]
pub enum SpikeKind {
    OutAndBack {
        turn_deg: f64,
        leg_in_m: f64,
        leg_out_m: f64,
    },
    LateralOffset {
        cross_track_m: f64,
        chord_m: f64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpikeHit {
    pub index: usize,
    pub lat: f64,
    pub lon: f64,
    pub kind: SpikeKind,
}

/// `(lat, lon)` polyline in travel order. `vias` are optional exempt points
/// (start/end/user vias) in the same order units.
pub fn detect_route_spikes(coords: &[(f64, f64)], vias: &[(f64, f64)]) -> Vec<SpikeHit> {
    let mut hits = Vec::new();
    if coords.len() < 3 {
        return hits;
    }
    for i in 1..coords.len() - 1 {
        let prev = coords[i - 1];
        let cur = coords[i];
        let next = coords[i + 1];
        let d_in = haversine_m(prev.0, prev.1, cur.0, cur.1);
        let d_out = haversine_m(cur.0, cur.1, next.0, next.1);
        let turn = turn_deg(prev, cur, next);
        let near_via = vias
            .iter()
            .any(|v| haversine_m(cur.0, cur.1, v.0, v.1) <= VIA_TOLERANCE_M);
        if turn >= UTURN_DEG && d_in > BACKTRACK_MIN_M && d_out > BACKTRACK_MIN_M && !near_via {
            hits.push(SpikeHit {
                index: i,
                lat: cur.0,
                lon: cur.1,
                kind: SpikeKind::OutAndBack {
                    turn_deg: turn,
                    leg_in_m: d_in,
                    leg_out_m: d_out,
                },
            });
        }
        let chord = haversine_m(prev.0, prev.1, next.0, next.1);
        if chord >= OFFSET_MIN_CHORD_M && d_in <= OFFSET_MAX_LEG_M && d_out <= OFFSET_MAX_LEG_M {
            let xt = cross_track_m(prev, cur, next);
            if xt >= OFFSET_ABS_M && xt / chord >= OFFSET_REL_OF_CHORD {
                hits.push(SpikeHit {
                    index: i,
                    lat: cur.0,
                    lon: cur.1,
                    kind: SpikeKind::LateralOffset {
                        cross_track_m: xt,
                        chord_m: chord,
                    },
                });
            }
        }
    }
    hits
}

fn turn_deg(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    let b1 = bearing_deg(a, b);
    let b2 = bearing_deg(b, c);
    let d = (b2 - b1 + 540.0) % 360.0 - 180.0;
    d.abs()
}

fn bearing_deg(a: (f64, f64), b: (f64, f64)) -> f64 {
    let lat1 = a.0.to_radians();
    let lat2 = b.0.to_radians();
    let dlon = (b.1 - a.1).to_radians();
    let y = dlon.sin() * lat2.cos();
    let x = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos();
    y.atan2(x).to_degrees().rem_euclid(360.0)
}

fn cross_track_m(a: (f64, f64), p: (f64, f64), b: (f64, f64)) -> f64 {
    let d_ap = haversine_m(a.0, a.1, p.0, p.1);
    let d_ab = haversine_m(a.0, a.1, b.0, b.1).max(1.0);
    let d_pb = haversine_m(p.0, p.1, b.0, b.1);
    let s = (d_ap + d_ab + d_pb) * 0.5;
    let area_sq = (s * (s - d_ap) * (s - d_ab) * (s - d_pb)).max(0.0);
    2.0 * area_sq.sqrt() / d_ab
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_371_008.8;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat * 0.5).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon * 0.5).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_out_and_back_spur() {
        // Go north 200 m, then reverse south 200 m.
        let coords = vec![
            (60.0, 10.0),
            (60.0018, 10.0), // ~200 m north
            (60.0, 10.0),
        ];
        let hits = detect_route_spikes(&coords, &[]);
        assert!(
            hits.iter()
                .any(|h| matches!(h.kind, SpikeKind::OutAndBack { .. })),
            "{hits:?}"
        );
    }

    #[test]
    fn via_exempts_uturn() {
        let coords = vec![(60.0, 10.0), (60.0018, 10.0), (60.0, 10.0)];
        let vias = vec![(60.0018, 10.0)];
        let hits = detect_route_spikes(&coords, &vias);
        assert!(
            hits.iter()
                .all(|h| !matches!(h.kind, SpikeKind::OutAndBack { .. })),
            "{hits:?}"
        );
    }
}
