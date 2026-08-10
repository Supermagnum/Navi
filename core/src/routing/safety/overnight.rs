//! Building and glacier proximity for overnight candidate filtering.
//!
//! Buildings are collected during the same PBF load as [`crate::poi::PoiIndex`]
//! (see `overnight_buildings`). Glacier rings come from
//! [`super::DangerBarrierIndex`] which is already built for break-stop access.
//! Do **not** add a separate full-extract `osmpbf` pass for overnight checks.

use super::DangerBarrierIndex;

/// Geometry used by [`super::check_overnight_candidate`].
#[derive(Debug, Clone, Default)]
pub struct OvernightProximityIndex {
    pub buildings: Vec<(f64, f64)>,
    /// Closed glacier rings as `[lon, lat]` for edge-distance checks.
    pub glacier_rings: Vec<Vec<[f64; 2]>>,
}

impl OvernightProximityIndex {
    pub fn is_empty(&self) -> bool {
        self.buildings.is_empty() && self.glacier_rings.is_empty()
    }

    /// Build from POI-load building points + barrier glacier rings (no PBF I/O).
    pub fn from_poi_buildings_and_barriers(
        buildings: Vec<(f64, f64)>,
        barriers: &DangerBarrierIndex,
    ) -> Self {
        Self {
            buildings,
            glacier_rings: barriers.glacier_rings().to_vec(),
        }
    }

    /// Legacy raw PBF loader — **prefer** [`Self::from_poi_buildings_and_barriers`].
    ///
    /// Kept only so benchmarks can compare redundant-scan cost against the
    /// merged path. Not used by the hiking FFI planner.
    #[doc(hidden)]
    pub fn load_from_pbf_bbox_legacy(
        path: impl AsRef<std::path::Path>,
        bbox: [f64; 4],
    ) -> anyhow::Result<Self> {
        use osmpbf::{Element, ElementReader};
        use std::collections::HashMap;

        let [min_lat, min_lon, max_lat, max_lon] = bbox;
        let in_bbox = |lat: f64, lon: f64| {
            lat >= min_lat && lat <= max_lat && lon >= min_lon && lon <= max_lon
        };

        let mut buildings = Vec::new();
        let mut glaciers = Vec::new();
        let mut node_coord: HashMap<i64, (f64, f64)> = HashMap::new();

        {
            let file = std::fs::File::open(path.as_ref())?;
            let reader = ElementReader::new(file);
            reader.for_each(|element| match element {
                Element::Node(n) => {
                    let lat = n.lat();
                    let lon = n.lon();
                    if !in_bbox(lat, lon) {
                        return;
                    }
                    node_coord.insert(n.id(), (lat, lon));
                    let mut is_building = false;
                    let mut is_glacier = false;
                    for (k, v) in n.tags() {
                        if k == "building" && v != "no" {
                            is_building = true;
                        }
                        if k == "natural" && v == "glacier" {
                            is_glacier = true;
                        }
                    }
                    if is_building {
                        buildings.push((lat, lon));
                    }
                    if is_glacier {
                        glaciers.push(tiny_ring(lat, lon));
                    }
                }
                Element::DenseNode(n) => {
                    let lat = n.lat();
                    let lon = n.lon();
                    if !in_bbox(lat, lon) {
                        return;
                    }
                    node_coord.insert(n.id, (lat, lon));
                    let mut is_building = false;
                    let mut is_glacier = false;
                    for (k, v) in n.tags() {
                        if k == "building" && v != "no" {
                            is_building = true;
                        }
                        if k == "natural" && v == "glacier" {
                            is_glacier = true;
                        }
                    }
                    if is_building {
                        buildings.push((lat, lon));
                    }
                    if is_glacier {
                        glaciers.push(tiny_ring(lat, lon));
                    }
                }
                _ => {}
            })?;
        }

        {
            let file = std::fs::File::open(path.as_ref())?;
            let reader = ElementReader::new(file);
            reader.for_each(|element| {
                if let Element::Way(way) = element {
                    let mut is_building = false;
                    let mut is_glacier = false;
                    for (k, v) in way.tags() {
                        if k == "building" && v != "no" {
                            is_building = true;
                        }
                        if k == "natural" && v == "glacier" {
                            is_glacier = true;
                        }
                    }
                    if !is_building && !is_glacier {
                        return;
                    }
                    let mut ring: Vec<[f64; 2]> = Vec::new();
                    let mut sum_lat = 0.0;
                    let mut sum_lon = 0.0;
                    let mut n = 0usize;
                    for nid in way.refs() {
                        if let Some(&(lat, lon)) = node_coord.get(&nid) {
                            ring.push([lon, lat]);
                            sum_lat += lat;
                            sum_lon += lon;
                            n += 1;
                        }
                    }
                    if n == 0 {
                        return;
                    }
                    let (lat, lon) = (sum_lat / n as f64, sum_lon / n as f64);
                    if !in_bbox(lat, lon) {
                        return;
                    }
                    if is_building {
                        buildings.push((lat, lon));
                    }
                    if is_glacier {
                        if ring.len() >= 3 {
                            let first = ring[0];
                            let last = *ring.last().unwrap();
                            if first != last {
                                ring.push(first);
                            }
                            glaciers.push(ring);
                        } else {
                            glaciers.push(tiny_ring(lat, lon));
                        }
                    }
                }
            })?;
        }

        Ok(Self {
            buildings,
            glacier_rings: glaciers,
        })
    }
}

fn tiny_ring(lat: f64, lon: f64) -> Vec<[f64; 2]> {
    let d = 1e-5;
    vec![
        [lon - d, lat - d],
        [lon + d, lat - d],
        [lon + d, lat + d],
        [lon - d, lat + d],
        [lon - d, lat - d],
    ]
}
