//! Post-plan densify geometry helpers.
//!
//! FU13: cosmetic vertex deletion ([`repair_joint_cuts`],
//! [`repair_path_much_longer_than_chord`], [`trim_joint_hairs`]) is **not**
//! applied to the exported route. Hop continuity is enforced by starting the
//! next hop at the previous hop's end node; the drawn line, distance, ETA and
//! maneuvers must all describe the search path.

/// Earth radius for local haversine (metres).
fn haversine_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    let r = 6_371_000.0;
    let dlat = (b.0 - a.0).to_radians();
    let dlon = (b.1 - a.1).to_radians();
    let x = (dlat / 2.0).sin().powi(2)
        + a.0.to_radians().cos() * b.0.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * x.sqrt().asin()
}

fn bearing_rad(a: (f64, f64), b: (f64, f64)) -> f64 {
    let y = (b.1 - a.1).to_radians() * a.0.to_radians().cos();
    let x = (b.0 - a.0).to_radians();
    y.atan2(x)
}

fn turn_deg(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    let b1 = bearing_rad(a, b);
    let b2 = bearing_rad(b, c);
    let mut d = (b2 - b1).to_degrees();
    while d > 180.0 {
        d -= 360.0;
    }
    while d < -180.0 {
        d += 360.0;
    }
    d.abs()
}

/// Unused in planning (FU13): cosmetic hair trim. Hop continuity uses the
/// previous hop end node instead.
pub fn trim_joint_hairs(pts: &[(f64, f64)]) -> Vec<(f64, f64)> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(pts.len());
    out.push(pts[0]);
    let mut i = 1usize;
    while i + 1 < pts.len() {
        let prev = *out.last().unwrap();
        let cur = pts[i];
        let next = pts[i + 1];
        let leg_in = haversine_m(prev, cur);
        let leg_out = haversine_m(cur, next);
        let hair = (40.0..=2_500.0).contains(&leg_in)
            && (40.0..=2_500.0).contains(&leg_out)
            && turn_deg(prev, cur, next) >= 150.0;
        if hair {
            i += 1;
            continue;
        }
        out.push(cur);
        i += 1;
    }
    if let Some(&last) = pts.last() {
        if out.last() != Some(&last) {
            out.push(last);
        }
    }
    out
}

/// Drop a near-closed circuit (close < 120 m after 8–80 km of path).
pub fn collapse_near_loops(pts: &[(f64, f64)]) -> Vec<(f64, f64)> {
    if pts.len() < 8 {
        return pts.to_vec();
    }
    let n = pts.len();
    let mut cum = vec![0.0; n];
    for i in 1..n {
        cum[i] = cum[i - 1] + haversine_m(pts[i - 1], pts[i]);
    }
    let mut drop = vec![false; n];
    for i in 0..n {
        for j in (i + 3)..n {
            let path = cum[j] - cum[i];
            if path < 8_000.0 {
                continue;
            }
            if path > 80_000.0 {
                break;
            }
            if haversine_m(pts[i], pts[j]) < 120.0 {
                for k in (i + 1)..j {
                    drop[k] = true;
                }
                break;
            }
        }
    }
    pts.iter()
        .enumerate()
        .filter_map(|(i, p)| if drop[i] { None } else { Some(*p) })
        .collect()
}

/// Joint-as-cut repair: hairs first, then loops.
pub fn repair_joint_cuts(pts: &[(f64, f64)]) -> Vec<(f64, f64)> {
    collapse_near_loops(&trim_joint_hairs(pts))
}

/// Sum of consecutive haversine segments along a `(lat, lon)` polyline (metres).
pub fn polyline_length_m(pts: &[(f64, f64)]) -> f64 {
    pts.windows(2).map(|w| haversine_m(w[0], w[1])).sum()
}

#[cfg(test)]
fn path_len_m(pts: &[(f64, f64)]) -> f64 {
    polyline_length_m(pts)
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

fn window_max_seg_m(pts: &[(f64, f64)]) -> f64 {
    pts.windows(2)
        .map(|w| haversine_m(w[0], w[1]))
        .fold(0.0, f64::max)
}

/// Replace 8–20 km windows whose path is ≥ 2.5× the chord with the window
/// endpoints, unless a single segment is ≥ 3 km (likely a ferry; keep it).
pub fn repair_path_much_longer_than_chord(pts: &[(f64, f64)]) -> Vec<(f64, f64)> {
    if pts.len() < 4 {
        return pts.to_vec();
    }
    let n = pts.len();
    let mut cum = vec![0.0; n];
    for i in 1..n {
        cum[i] = cum[i - 1] + haversine_m(pts[i - 1], pts[i]);
    }
    let mut drop = vec![false; n];
    let mut i = 0usize;
    while i < n {
        let mut j = i + 1;
        let mut used = false;
        while j < n {
            let path = cum[j] - cum[i];
            if path < 8_000.0 {
                j += 1;
                continue;
            }
            if path > 20_000.0 {
                break;
            }
            let chord = haversine_m(pts[i], pts[j]);
            if chord > 1.0 && path / chord >= 2.5 {
                let max_seg = window_max_seg_m(&pts[i..=j]);
                if max_seg < 3_000.0 {
                    for k in (i + 1)..j {
                        drop[k] = true;
                    }
                    i = j;
                    used = true;
                    break;
                }
            }
            j += 1;
        }
        if !used {
            i += 1;
        }
    }
    pts.iter()
        .enumerate()
        .filter_map(|(idx, p)| if drop[idx] { None } else { Some(*p) })
        .collect()
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
    fn trim_joint_hairs_drops_fu3_halland_and_ostlandet() {
        // FU3 hairs: 180-degree out-and-back at densify vertices.
        let halland = (56.93600_f64, 12.49705_f64);
        let ostlandet = (60.79531_f64, 11.06810_f64);
        let mut pts = vec![
            (56.930, 12.490),
            halland,
            (56.930, 12.490),
            (56.950, 12.510),
            (60.790, 11.060),
            ostlandet,
            (60.790, 11.060),
            (60.810, 11.080),
        ];
        let repaired = trim_joint_hairs(&pts);
        assert!(
            !repaired.iter().any(|p| (p.0 - halland.0).abs() < 1e-5
                && (p.1 - halland.1).abs() < 1e-5),
            "Halland hair vertex must not remain; {repaired:?}"
        );
        assert!(
            !repaired.iter().any(|p| (p.0 - ostlandet.0).abs() < 1e-5
                && (p.1 - ostlandet.1).abs() < 1e-5),
            "Ostlandet hair vertex must not remain; {repaired:?}"
        );
        pts.push((61.0, 11.1));
        let again = repair_joint_cuts(&pts);
        assert!(again.len() < pts.len());
    }

    #[test]
    fn collapse_near_loops_drops_island_circuit() {
        // ~12 km around a point, close 50 m.
        let mut pts = vec![(54.40, 11.18)];
        let n = 40;
        for i in 1..=n {
            let t = i as f64 / n as f64 * std::f64::consts::TAU;
            pts.push((54.40 + 0.04 * t.cos(), 11.18 + 0.08 * t.sin()));
        }
        pts.push((54.4002, 11.1801));
        let path: f64 = pts.windows(2).map(|w| haversine_m(w[0], w[1])).sum();
        assert!(path > 8_000.0, "fixture path {path}");
        let repaired = collapse_near_loops(&pts);
        assert!(
            repaired.len() < pts.len() / 2,
            "loop interior dropped; before={} after={}",
            pts.len(),
            repaired.len()
        );
    }

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
        assert!(hop_distance_agrees_with_polyline(poly_m * 1.004, &pts, 0.005));
        assert!(!hop_distance_agrees_with_polyline(poly_m * 1.01, &pts, 0.005));
        assert!(!hop_distance_agrees_with_polyline(0.0, &pts, 0.005));
    }

    #[test]
    fn path_much_longer_than_chord_collapses_detour_keeps_ferry() {
        // ~12 km zigzag, ~4 km chord → ratio ≥ 2.5, all segs short.
        let mut detour = vec![(58.40, 11.29)];
        for i in 0..40 {
            let lon = 11.29 + 0.002 * i as f64;
            let lat = 58.40 + if i % 2 == 0 { 0.008 } else { 0.0 };
            detour.push((lat, lon));
        }
        let path = path_len_m(&detour);
        let chord = haversine_m(detour[0], *detour.last().unwrap());
        assert!(path / chord >= 2.5, "path={path} chord={chord}");
        let repaired = repair_path_much_longer_than_chord(&detour);
        assert!(
            repaired.len() < detour.len() / 2,
            "detour collapsed; before={} after={}",
            detour.len(),
            repaired.len()
        );

        // Long water/ferry segment in the window must survive (roa-florø class).
        let ferry = vec![
            (60.37, 6.72),
            (60.38, 6.70),
            (60.42, 6.62), // ~8 km skip
            (60.43, 6.60),
        ];
        let kept = repair_path_much_longer_than_chord(&ferry);
        assert_eq!(kept.len(), ferry.len(), "ferry vertices must remain");
    }
}
