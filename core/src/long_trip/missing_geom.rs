//! Geometric missing-region hints after a coarse path is known.
//!
//! A major road that ends at an installed pack's edge and continues into an
//! uninstalled outline, near the chosen coarse path, names that outline.

use crate::long_trip::region_containing;
use crate::pack_server::region_ids_match_for_catalog;
use crate::routing::corridor_skeleton::CorridorSkeletonFile;
use crate::routing::plan_bbox::haversine_km;

/// A major end on an installed skeleton continues into this uninstalled region.
#[derive(Debug, Clone, PartialEq)]
pub struct MissingRegionHint {
    pub region_id: String,
    pub road_ref: String,
    pub highway: String,
    pub lat: f64,
    pub lon: f64,
}

impl MissingRegionHint {
    /// Why the region is missing. Uses OSM ref / class and the catalog id only.
    pub fn reason(&self) -> String {
        let class = if self.highway.is_empty() {
            "road"
        } else {
            self.highway.as_str()
        };
        if self.road_ref.is_empty() {
            format!(
                "a {class} continues through {}, which is not installed",
                self.region_id
            )
        } else {
            format!(
                "the {} continues through {}, which is not installed",
                self.road_ref, self.region_id
            )
        }
    }
}

/// How close a major end must be to the coarse path (km) to count.
pub const MAJOR_END_PATH_NEAR_KM: f64 = 40.0;

/// Probe past a pack-end along the road, looking for the next outline.
pub const MAJOR_CONTINUE_PROBE_M: f64 = 400.0;

fn is_major(hw: &str) -> bool {
    matches!(
        hw,
        "motorway" | "motorway_link" | "trunk" | "trunk_link" | "primary" | "primary_link"
    )
}

fn destination_point(lat: f64, lon: f64, bearing_rad: f64, dist_km: f64) -> (f64, f64) {
    let r = 6371.0;
    let ang = dist_km / r;
    let lat1 = lat.to_radians();
    let lon1 = lon.to_radians();
    let lat2 = (lat1.sin() * ang.cos() + lat1.cos() * ang.sin() * bearing_rad.cos()).asin();
    let lon2 = lon1
        + (bearing_rad.sin() * ang.sin() * lat1.cos()).atan2(ang.cos() - lat1.sin() * lat2.sin());
    (lat2.to_degrees(), lon2.to_degrees())
}

fn bearing_rad(a: (f64, f64), b: (f64, f64)) -> f64 {
    let lat1 = a.0.to_radians();
    let lat2 = b.0.to_radians();
    let dlon = (b.1 - a.1).to_radians();
    (dlon.sin() * lat2.cos()).atan2(lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos())
}

fn near_path_km(path: &[(f64, f64)], p: (f64, f64)) -> f64 {
    let mut best = f64::MAX;
    for w in path.windows(2) {
        let d = point_seg_km(p, w[0], w[1]);
        if d < best {
            best = d;
        }
    }
    if path.len() == 1 {
        best = best.min(haversine_km(p.0, p.1, path[0].0, path[0].1));
    }
    best
}

fn point_seg_km(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let d = haversine_km(a.0, a.1, b.0, b.1).max(1e-6);
    let mut best = haversine_km(p.0, p.1, a.0, a.1).min(haversine_km(p.0, p.1, b.0, b.1));
    let n = ((d / 5.0).ceil() as i32).clamp(1, 8);
    for i in 1..n {
        let t = i as f64 / n as f64;
        let q = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        best = best.min(haversine_km(p.0, p.1, q.0, q.1));
    }
    best
}

/// Core rule: major pack-ends near the coarse path that PIP (or probe) into
/// an uninstalled outline. `containing` and `installed` are injected so tests
/// can use a synthetic three-region sliver without catalog names.
pub fn major_ends_into_uninstalled(
    ends: &[(f64, f64, &str, &str, Option<(f64, f64)>)],
    coarse_path: &[(f64, f64)],
    path_near_km: f64,
    containing: impl Fn(f64, f64) -> Option<String>,
    installed: impl Fn(&str) -> bool,
) -> Vec<MissingRegionHint> {
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    if coarse_path.is_empty() {
        return out;
    }
    for &(lat, lon, ref_s, hw, probe_from) in ends {
        if near_path_km(coarse_path, (lat, lon)) > path_near_km {
            continue;
        }
        let mut cand = containing(lat, lon);
        if cand.as_deref().map(|id| installed(id)).unwrap_or(true) {
            if let Some(from) = probe_from {
                let brg = bearing_rad(from, (lat, lon));
                let (plat, plon) =
                    destination_point(lat, lon, brg, MAJOR_CONTINUE_PROBE_M / 1000.0);
                cand = containing(plat, plon);
            }
        }
        let Some(id) = cand else {
            continue;
        };
        if installed(&id) {
            continue;
        }
        if !seen.insert(id.clone()) {
            continue;
        }
        out.push(MissingRegionHint {
            region_id: id,
            road_ref: ref_s.to_string(),
            highway: hw.to_string(),
            lat,
            lon,
        });
    }
    out
}

fn is_installed_id(id: &str, installed: &[String]) -> bool {
    installed
        .iter()
        .any(|inst| region_ids_match_for_catalog(inst, id) || inst == id)
}

/// Scan installed skeletons for major ends near `coarse_path` that continue
/// into an outline that is not installed.
pub fn missing_major_continuations(
    skels: &[CorridorSkeletonFile],
    coarse_path: &[(f64, f64)],
    installed: &[String],
) -> Vec<MissingRegionHint> {
    let mut ends = Vec::new();
    for sk in skels {
        let mut undirected: std::collections::HashSet<(u32, u32)> =
            std::collections::HashSet::new();
        let mut adj: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
        let mut ref_at: std::collections::HashMap<u32, (String, String)> =
            std::collections::HashMap::new();
        for i in 0..sk.edge_src.len() {
            if sk.edge_is_ferry.get(i).copied().unwrap_or(0) != 0 {
                continue;
            }
            let hw = sk.edge_highway.get(i).map(|s| s.as_str()).unwrap_or("");
            if !is_major(hw) {
                continue;
            }
            let a = sk.edge_src[i];
            let b = sk.edge_tgt[i];
            let key = if a <= b { (a, b) } else { (b, a) };
            if !undirected.insert(key) {
                continue;
            }
            adj.entry(a).or_default().push(b);
            adj.entry(b).or_default().push(a);
            let rf = sk
                .edge_road_ref
                .get(i)
                .map(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            ref_at.entry(a).or_insert((rf.clone(), hw.to_string()));
            ref_at.entry(b).or_insert((rf, hw.to_string()));
        }
        for (n, neigh) in &adj {
            let border = sk.node_is_border.get(*n as usize).copied().unwrap_or(0) == 1;
            if neigh.len() > 1 && !border {
                continue;
            }
            let i = *n as usize;
            if i >= sk.node_lats.len() {
                continue;
            }
            let p = (sk.node_lats[i], sk.node_lons[i]);
            let (rf, hw) = ref_at
                .get(n)
                .cloned()
                .unwrap_or_else(|| (String::new(), String::new()));
            let probe_from = neigh.first().map(|&o| {
                let j = o as usize;
                (sk.node_lats[j], sk.node_lons[j])
            });
            ends.push((p.0, p.1, rf, hw, probe_from));
        }
    }
    let end_refs: Vec<(f64, f64, &str, &str, Option<(f64, f64)>)> = ends
        .iter()
        .map(|(la, lo, rf, hw, pr)| (*la, *lo, rf.as_str(), hw.as_str(), *pr))
        .collect();
    major_ends_into_uninstalled(
        &end_refs,
        coarse_path,
        MAJOR_END_PATH_NEAR_KM,
        |lat, lon| region_containing(lat, lon, None).map(|s| s.to_string()),
        |id| is_installed_id(id, installed) || super::adjacency::is_installed(id, installed),
    )
}

/// Name a region only when a major road on the chosen corridor ends at an
/// installed edge and continues into it, and the geometric band also crosses
/// that outline. A region that merely lies in the band is not named.
pub fn named_missing_regions(hints: &[MissingRegionHint], band_missing: &[String]) -> Vec<String> {
    let band: std::collections::BTreeSet<&str> = band_missing.iter().map(String::as_str).collect();
    let mut ids = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for h in hints {
        if band.contains(h.region_id.as_str()) && seen.insert(h.region_id.clone()) {
            ids.push(h.region_id.clone());
        }
    }
    ids
}

/// Report lines for a completed plan that found a longer installed-only route.
///
/// Uses [named_missing_regions]: major-end + band. Does not append band-only
/// outlines.
pub fn missing_region_advisory_lines(
    hints: &[MissingRegionHint],
    band_missing: &[String],
) -> String {
    let ids = named_missing_regions(hints, band_missing);
    if ids.is_empty() {
        return String::new();
    }
    let named: std::collections::BTreeSet<&str> = ids.iter().map(String::as_str).collect();
    let mut out = format!("missing_regions={}\n", ids.join(","));
    for h in hints {
        if named.contains(h.region_id.as_str()) {
            out.push_str(&format!("missing_region_why={}\n", h.reason()));
        }
    }
    out.push_str("route_may_be_longer=true\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic west / sliver / east boxes. No catalog or place names.
    fn pip(lat: f64, lon: f64) -> Option<String> {
        if !(0.0..=1.0).contains(&lat) {
            return None;
        }
        if lon < 1.0 {
            Some("synth/west".into())
        } else if lon < 1.2 {
            Some("synth/sliver".into())
        } else if lon < 2.2 {
            Some("synth/east".into())
        } else {
            None
        }
    }

    fn installed(id: &str) -> bool {
        id == "synth/west" || id == "synth/east"
    }

    #[test]
    fn sliver_between_two_installed_regions_is_named() {
        // Major end at the west pack edge; 400 m onward is the sliver.
        let ends = [(0.50, 0.999, "R1", "trunk", Some((0.50, 0.90)))];
        let path = [(0.50, 0.80), (0.50, 0.95), (0.50, 0.999)];
        let hints = major_ends_into_uninstalled(&ends, &path, 40.0, pip, installed);
        assert_eq!(hints.len(), 1, "{hints:?}");
        assert_eq!(hints[0].region_id, "synth/sliver");
        assert!(hints[0].reason().contains("synth/sliver"));
        assert!(hints[0].reason().contains("R1"));
        assert!(!hints[0].reason().contains("Gävleborg"));
        assert!(!hints[0].reason().contains("E 45"));
    }

    #[test]
    fn far_from_coarse_path_is_ignored() {
        let ends = [(0.50, 0.996, "R1", "trunk", Some((0.50, 0.90)))];
        let path = [(0.10, 0.10), (0.10, 0.20)];
        let hints = major_ends_into_uninstalled(&ends, &path, 40.0, pip, installed);
        assert!(hints.is_empty(), "{hints:?}");
    }

    #[test]
    fn installed_continuation_is_silent() {
        let ends = [(0.50, 1.25, "R1", "trunk", Some((0.50, 1.30)))];
        let path = [(0.50, 1.20), (0.50, 1.25)];
        let hints = major_ends_into_uninstalled(&ends, &path, 40.0, pip, installed);
        assert!(hints.is_empty(), "{hints:?}");
    }

    #[test]
    fn advisory_drops_major_end_outside_the_band() {
        let hint = MissingRegionHint {
            region_id: "synth/other".into(),
            road_ref: "R9".into(),
            highway: "primary".into(),
            lat: 0.5,
            lon: 0.5,
        };
        assert!(missing_region_advisory_lines(&[hint], &[]).is_empty());
        let sliver = MissingRegionHint {
            region_id: "synth/sliver".into(),
            road_ref: "R1".into(),
            highway: "trunk".into(),
            lat: 0.5,
            lon: 1.0,
        };
        let text = missing_region_advisory_lines(&[sliver], &["synth/sliver".into()]);
        assert!(text.contains("synth/sliver"));
        assert!(text.contains("R1"));
        assert!(text.contains("route_may_be_longer=true"));
    }

    #[test]
    fn band_only_region_is_not_named() {
        let band = vec!["synth/sliver".into(), "synth/other".into()];
        assert!(named_missing_regions(&[], &band).is_empty());
        let text = missing_region_advisory_lines(&[], &band);
        assert!(text.is_empty(), "{text}");
    }
}
