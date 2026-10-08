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
