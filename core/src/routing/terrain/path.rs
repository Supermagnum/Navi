//! Grid A* over DEM slope + wetland costs.

use pathfinding::directed::astar::astar;

use crate::config::EcoConfig;
use crate::routing::elevation::ElevationService;
use crate::routing::wetland::{WetlandClass, WetlandIndex, WETLAND_SOFT_COST_MULT};

/// Nominal cell size for the terrain cost surface (metres).
pub const TERRAIN_CELL_M: f64 = 40.0;

/// Refuse terrain gap-fill when crow-flies distance exceeds this (metres).
pub const TERRAIN_MAX_GAP_M: f64 = 15_000.0;

const STEEP_GRADE: f64 = 0.45;
const IMPASSABLE: f64 = 1.0e12;

#[derive(Debug, Clone)]
pub struct TerrainPath {
    /// `(lat, lon)` vertices including endpoints.
    pub coords: Vec<(f64, f64)>,
    pub length_m: f64,
}

/// Least-cost off-trail path from A to B over DEM + wetlands.
pub fn least_cost_path(
    elevation: &ElevationService,
    wetlands: &WetlandIndex,
    eco: &EcoConfig,
    a_lat: f64,
    a_lon: f64,
    b_lat: f64,
    b_lon: f64,
) -> Result<TerrainPath, String> {
    let crow = haversine_m(a_lat, a_lon, b_lat, b_lon);
    if crow < 1.0 {
        return Ok(TerrainPath {
            coords: vec![(a_lat, a_lon), (b_lat, b_lon)],
            length_m: crow,
        });
    }
    if crow > TERRAIN_MAX_GAP_M {
        return Err(format!(
            "terrain gap too large ({crow:.0} m > {TERRAIN_MAX_GAP_M:.0} m)"
        ));
    }

    let pad = (crow * 0.25).clamp(200.0, 2_000.0);
    // Approximate degrees: 1° lat ≈ 111_320 m; lon scaled by cos(lat).
    let dlat = pad / 111_320.0;
    let mid_lat = (a_lat + b_lat) * 0.5;
    let dlon = pad / (111_320.0 * mid_lat.to_radians().cos().max(0.2));
    let min_lat = a_lat.min(b_lat) - dlat;
    let max_lat = a_lat.max(b_lat) + dlat;
    let min_lon = a_lon.min(b_lon) - dlon;
    let max_lon = a_lon.max(b_lon) + dlon;
    let _ = elevation.warm_bbox([min_lat, min_lon, max_lat, max_lon]);

    let cell_m = TERRAIN_CELL_M;
    let nrows = (((max_lat - min_lat) * 111_320.0) / cell_m).ceil() as usize + 1;
    let ncols = (((max_lon - min_lon) * 111_320.0 * mid_lat.to_radians().cos().max(0.2)) / cell_m)
        .ceil() as usize
        + 1;
    if nrows == 0 || ncols == 0 || nrows * ncols > 250_000 {
        return Err(format!("terrain grid too large ({nrows}x{ncols} cells)"));
    }

    let cell_lat = |r: usize| min_lat + (r as f64 + 0.5) * (max_lat - min_lat) / nrows as f64;
    let cell_lon = |c: usize| min_lon + (c as f64 + 0.5) * (max_lon - min_lon) / ncols as f64;

    let mut elev = vec![None; nrows * ncols];
    for r in 0..nrows {
        for c in 0..ncols {
            elev[r * ncols + c] = elevation.get_elevation(cell_lat(r), cell_lon(c));
        }
    }

    let to_cell = |lat: f64, lon: f64| -> (usize, usize) {
        let rf = ((lat - min_lat) / (max_lat - min_lat).max(1e-12) * nrows as f64).floor();
        let cf = ((lon - min_lon) / (max_lon - min_lon).max(1e-12) * ncols as f64).floor();
        let r = rf.clamp(0.0, (nrows - 1) as f64) as usize;
        let c = cf.clamp(0.0, (ncols - 1) as f64) as usize;
        (r, c)
    };

    let start = to_cell(a_lat, a_lon);
    let goal = to_cell(b_lat, b_lon);

    let neighbors = |&(r, c): &(usize, usize)| {
        let mut out = Vec::with_capacity(8);
        for dr in [-1_isize, 0, 1] {
            for dc in [-1_isize, 0, 1] {
                if dr == 0 && dc == 0 {
                    continue;
                }
                let nr = r as isize + dr;
                let nc = c as isize + dc;
                if nr < 0 || nc < 0 || nr >= nrows as isize || nc >= ncols as isize {
                    continue;
                }
                let nr = nr as usize;
                let nc = nc as usize;
                let lat0 = cell_lat(r);
                let lon0 = cell_lon(c);
                let lat1 = cell_lat(nr);
                let lon1 = cell_lon(nc);
                let step_m = haversine_m(lat0, lon0, lat1, lon1).max(1.0);
                let h0 = elev[r * ncols + c];
                let h1 = elev[nr * ncols + nc];
                let delta_h = match (h0, h1) {
                    (Some(a), Some(b)) => b - a,
                    _ => 0.0,
                };
                let grade = delta_h.abs() / step_m;
                if grade > STEEP_GRADE && delta_h > 0.0 {
                    // Treat extreme climbs as impassable cliffs for hiking.
                    continue;
                }
                let mut cost = eco.segment_energy_joules(step_m, delta_h);
                // Extra steepness penalty beyond energy (discourages side-slopes).
                if grade > 0.20 {
                    cost *= 1.0 + (grade - 0.20) * 8.0;
                }
                match wetlands.class_at(lat1, lon1) {
                    Some(WetlandClass::HardAvoid) => continue,
                    Some(WetlandClass::SoftAvoid) => cost *= WETLAND_SOFT_COST_MULT,
                    None => {}
                }
                if !cost.is_finite() || cost >= IMPASSABLE {
                    continue;
                }
                out.push(((nr, nc), cost_to_u64(cost)));
            }
        }
        out
    };

    let heuristic = |&(r, c): &(usize, usize)| {
        let lat = cell_lat(r);
        let lon = cell_lon(c);
        cost_to_u64(eco.flat_energy_joules(haversine_m(lat, lon, b_lat, b_lon)))
    };

    let Some((cells, _cost)) = astar(&start, neighbors, heuristic, |n| *n == goal) else {
        return Err("no terrain path across gap".into());
    };

    let mut coords: Vec<(f64, f64)> = Vec::with_capacity(cells.len() + 2);
    coords.push((a_lat, a_lon));
    for &(r, c) in &cells {
        let lat = cell_lat(r);
        let lon = cell_lon(c);
        if let Some(&(plat, plon)) = coords.last() {
            if haversine_m(plat, plon, lat, lon) < 1.0 {
                continue;
            }
        }
        coords.push((lat, lon));
    }
    if let Some(&(plat, plon)) = coords.last() {
        if haversine_m(plat, plon, b_lat, b_lon) > 1.0 {
            coords.push((b_lat, b_lon));
        } else {
            *coords.last_mut().unwrap() = (b_lat, b_lon);
        }
    } else {
        coords.push((b_lat, b_lon));
    }

    let mut length_m = 0.0;
    for w in coords.windows(2) {
        length_m += haversine_m(w[0].0, w[0].1, w[1].0, w[1].1);
    }
    Ok(TerrainPath { coords, length_m })
}

fn cost_to_u64(cost: f64) -> u64 {
    (cost.max(0.0) * 1000.0).round() as u64
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_378_100.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::elevation::{ElevationCache, ElevationService};
    use std::path::PathBuf;

    #[test]
    fn refuses_huge_gap() {
        let elev = ElevationService::new(ElevationCache::new(PathBuf::from("/tmp/no-elev")));
        let wet = WetlandIndex::default();
        let eco = EcoConfig::for_profile(crate::config::Profile::Hiking);
        let err = least_cost_path(&elev, &wet, &eco, 60.0, 10.0, 61.0, 11.0).unwrap_err();
        assert!(err.contains("too large"), "{err}");
    }
}
