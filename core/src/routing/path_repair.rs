//! Post-plan densify geometry repair: joints as cuts (hairs / loops).
//!
//! Chunked hops are concatenated at densify joints. A 35 km snap at both sides of
//! a joint used to produce 180-degree out-and-backs. [`repair_joint_cuts`] drops
//! those hairs and near-closed loops so the joint is a hop terminal, not a vertex
//! the path may reverse through.

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

/// Drop 180-degree out-and-backs at densify joints (legs 40 m–2.5 km).
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
}
