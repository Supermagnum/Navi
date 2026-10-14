//! Post-plan densify geometry helpers.
//!
//! FU13 / FU23: cosmetic vertex deletion (joint-cut / hair / chord-collapse
//! repair) is **not** applied to the exported route. Hop continuity is enforced
//! by starting the next hop at the previous hop's end node; the drawn line,
//! distance, ETA and maneuvers must all describe the search path.

/// Earth radius for local haversine (metres).
fn haversine_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    let r = 6_371_000.0;
    let dlat = (b.0 - a.0).to_radians();
    let dlon = (b.1 - a.1).to_radians();
    let x = (dlat / 2.0).sin().powi(2)
        + a.0.to_radians().cos() * b.0.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * x.sqrt().asin()
}

/// Sum of consecutive haversine segments along a `(lat, lon)` polyline (metres).
pub fn polyline_length_m(pts: &[(f64, f64)]) -> f64 {
    pts.windows(2).map(|w| haversine_m(w[0], w[1])).sum()
}

/// True when hop edge-sum distance and polyline length agree within `max_rel`
/// (e.g. 0.005 = 0.5 %). Degenerate empty/zero cases fail.
pub fn hop_distance_agrees_with_polyline(
    distance_m: f64,
    polyline_pts: &[(f64, f64)],
    max_rel: f64,
) -> bool {
    if distance_m <= 0.0 || polyline_pts.len() < 2 {
        return false;
    }
    let poly_m = polyline_length_m(polyline_pts);
    if poly_m <= 0.0 {
        return false;
    }
    ((poly_m - distance_m).abs() / distance_m) <= max_rel
}

/// Same agree check for the full concatenated route (reported km vs polyline).
pub fn total_distance_agrees_with_polyline(
    distance_m: f64,
    polyline_pts: &[(f64, f64)],
    max_rel: f64,
) -> bool {
    hop_distance_agrees_with_polyline(distance_m, polyline_pts, max_rel)
}

/// Minimum out-and-back path length that fails a delivered route.
pub const OUT_AND_BACK_MIN_PATH_M: f64 = 3_000.0;
/// A later point this close to an earlier point counts as a return.
pub const OUT_AND_BACK_RETURN_M: f64 = 200.0;
/// A user via this close to the turn-around explains the excursion.
pub const OUT_AND_BACK_VIA_M: f64 = 2_500.0;

/// One unexplained out-and-back on a delivered `(lat, lon)` line.
#[derive(Debug, Clone, PartialEq)]
pub struct OutAndBack {
    pub from_km: f64,
    pub to_km: f64,
    pub path_km: f64,
    pub turnaround: (f64, f64),
}

/// Windows where the line returns to within [`OUT_AND_BACK_RETURN_M`] of an
/// earlier point after travelling at least [`OUT_AND_BACK_MIN_PATH_M`], and
/// no user via lies within [`OUT_AND_BACK_VIA_M`] of the turn-around.
pub fn unexplained_out_and_backs(pts: &[(f64, f64)], vias: &[(f64, f64)]) -> Vec<OutAndBack> {
    let rs = resample_polyline(pts, 250.0);
    if rs.len() < 3 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < rs.len() {
        let (p, c) = rs[i];
        let mut j = i + 1;
        while j < rs.len() && rs[j].1 - c < OUT_AND_BACK_MIN_PATH_M {
            j += 1;
        }
        let mut found = None;
        while j < rs.len() && rs[j].1 - c <= 40_000.0 {
            if haversine_m(p, rs[j].0) <= OUT_AND_BACK_RETURN_M {
                found = Some(j);
                break;
            }
            j += 1;
        }
        if let Some(ret) = found {
            let mut turn = p;
            let mut turn_d = 0.0_f64;
            for item in rs.iter().take(ret + 1).skip(i) {
                let d = haversine_m(p, item.0);
                if d > turn_d {
                    turn_d = d;
                    turn = item.0;
                }
            }
            let explained = vias
                .iter()
                .any(|&v| haversine_m(v, turn) <= OUT_AND_BACK_VIA_M);
            if !explained {
                out.push(OutAndBack {
                    from_km: c / 1000.0,
                    to_km: rs[ret].1 / 1000.0,
                    path_km: (rs[ret].1 - c) / 1000.0,
                    turnaround: turn,
                });
            }
            i = ret;
        } else {
            i += 1;
        }
    }
    out
}

fn resample_polyline(pts: &[(f64, f64)], step_m: f64) -> Vec<((f64, f64), f64)> {
    let mut out = Vec::new();
    if pts.is_empty() {
        return out;
    }
    out.push((pts[0], 0.0));
    let mut cum = 0.0;
    let mut next = step_m;
    for w in pts.windows(2) {
        let seg = haversine_m(w[0], w[1]);
        while seg > 0.0 && cum + seg >= next {
            let t = (next - cum) / seg;
            out.push((
                (
                    w[0].0 + (w[1].0 - w[0].0) * t,
                    w[0].1 + (w[1].1 - w[0].1) * t,
                ),
                next,
            ));
            next += step_m;
        }
        cum += seg;
    }
    if let Some(last) = pts.last() {
        out.push((*last, cum));
    }
    out
}

/// Encode `(lat, lon)` points as Navi `"lon,lat;…"`.
pub fn encode_lat_lon_polyline(pts: &[(f64, f64)]) -> String {
    let mut s = String::new();
    for (i, (lat, lon)) in pts.iter().enumerate() {
        if i > 0 {
            s.push(';');
        }
        s.push_str(&format!("{lon},{lat}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::parse_route_polyline;

    #[test]
    fn encode_roundtrip_matches_parse() {
        let pts = vec![(53.08, 10.59), (54.40, 11.36)];
        let s = encode_lat_lon_polyline(&pts);
        let back = parse_route_polyline(&s);
        assert_eq!(back.len(), 2);
        assert!((back[0].0 - 53.08).abs() < 1e-6);
    }

    #[test]
    fn hop_polyline_length_agrees_with_distance_within_half_percent() {
        // Straight ~11.1 km east at lat 60: lon delta ≈ 0.2°.
        let pts = vec![(60.0, 10.0), (60.0, 10.2)];
        let poly_m = polyline_length_m(&pts);
        assert!(hop_distance_agrees_with_polyline(poly_m, &pts, 0.005));
        assert!(hop_distance_agrees_with_polyline(
            poly_m * 1.004,
            &pts,
            0.005
        ));
        assert!(!hop_distance_agrees_with_polyline(
            poly_m * 1.01,
            &pts,
            0.005
        ));
        assert!(!hop_distance_agrees_with_polyline(0.0, &pts, 0.005));
        assert!(total_distance_agrees_with_polyline(poly_m, &pts, 0.005));
    }

    #[test]
    fn delivered_out_and_back_without_via_fails() {
        // 5 km east and the same 5 km back. Not a user waypoint.
        let pts = vec![(60.0, 10.0), (60.0, 10.09), (60.0, 10.0)];
        let hits = unexplained_out_and_backs(&pts, &[]);
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert!(hits[0].path_km > 8.0, "{hits:?}");
    }

    #[test]
    fn delivered_out_and_back_at_user_via_is_explained() {
        let pts = vec![(60.0, 10.0), (60.0, 10.09), (60.0, 10.0)];
        let hits = unexplained_out_and_backs(&pts, &[(60.0, 10.09)]);
        assert!(hits.is_empty(), "{hits:?}");
    }

    #[test]
    fn through_route_is_not_an_out_and_back() {
        let pts = vec![(60.0, 10.0), (60.0, 10.09), (60.0, 10.18)];
        assert!(unexplained_out_and_backs(&pts, &[]).is_empty());
    }

    #[test]
    fn cosmetic_path_repair_fns_removed() {
        let src = include_str!("path_repair.rs");
        let hair = format!("fn trim_joint{}", "_hairs");
        let joint = format!("fn repair_joint{}", "_cuts");
        let chord = format!("fn repair_path_much_longer_than{}", "_chord");
        assert!(!src.contains(&hair));
        assert!(!src.contains(&joint));
        assert!(!src.contains(&chord));
    }
}
