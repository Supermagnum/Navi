use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

use geo::{Distance, Haversine, Point};
use osmpbf::{Element, ElementReader};
use rstar::{RTree, RTreeObject, AABB};

use super::{classify_tags, osm_icon_key, CorridorBand, PoiCategory};

/// Wall-clock breakdown for overnight-aware POI loads (bench / diagnostics).
#[derive(Debug, Clone, Default)]
pub struct PoiOvernightLoadProfile {
    pub band_build_ms: f64,
    pub node_pass_ms: f64,
    pub way_pass_ms: f64,
    pub total_ms: f64,
    /// File opens / full-extract scans performed for this load.
    pub pbf_passes: u32,
    pub nodes_decoded: u64,
    pub nodes_in_bbox: u64,
    pub coords_kept: u64,
    pub building_nodes_kept: u64,
    pub ways_seen: u64,
    pub building_ways_seen: u64,
    pub building_ways_centroid_ok: u64,
    pub building_ways_kept: u64,
    pub corridor_contains_calls: u64,
    pub corridor_contains_hits: u64,
    pub overnight_buildings: usize,
    pub poi_records: usize,
}

#[derive(Debug, Clone)]
pub struct PoiRecord {
    pub osm_id: i64,
    pub lat: f64,
    pub lon: f64,
    pub categories: Vec<PoiCategory>,
    pub icon_key: String,
    pub tags: HashMap<String, String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct PoiEntry {
    osm_id: i64,
    lat: f64,
    lon: f64,
}

impl RTreeObject for PoiEntry {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_point([self.lon, self.lat])
    }
}

pub struct PoiIndex {
    records: HashMap<i64, PoiRecord>,
    tree: RTree<PoiEntry>,
    /// Building nodes / way centroids when loaded via overnight-aware loaders.
    overnight_buildings: Vec<(f64, f64)>,
}

pub struct PoiQuery<'a> {
    pub category: PoiCategory,
    pub lat: f64,
    pub lon: f64,
    pub radius_m: f64,
    pub index: &'a PoiIndex,
}

impl PoiIndex {
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
            tree: RTree::new(),
            overnight_buildings: Vec::new(),
        }
    }

    /// Building sample points from a hiking overnight-aware load (empty otherwise).
    pub fn overnight_buildings(&self) -> &[(f64, f64)] {
        &self.overnight_buildings
    }

    /// Replace overnight building centroids (indexed-pack hydrate path).
    pub fn set_overnight_buildings(&mut self, buildings: Vec<(f64, f64)>) {
        self.overnight_buildings = buildings;
    }

    /// Keep only overnight buildings matching `pred(lat, lon)`.
    pub fn retain_overnight_buildings<F>(&mut self, mut pred: F)
    where
        F: FnMut(f64, f64) -> bool,
    {
        self.overnight_buildings
            .retain(|&(lat, lon)| pred(lat, lon));
    }

    pub fn load_from_pbf(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        Self::load_from_pbf_filtered(path, None, OvernightBuildings::Off)
    }

    /// Load only POI nodes inside `bbox` `[min_lat, min_lon, max_lat, max_lon]`.
    /// Single node pass — used by motor route break POIs.
    pub fn load_from_pbf_bbox(path: impl AsRef<Path>, bbox: [f64; 4]) -> anyhow::Result<Self> {
        Self::load_from_pbf_filtered(path, Some(bbox), OvernightBuildings::Off)
    }

    /// Like [`Self::load_from_pbf_bbox`], plus overnight building proximity points
    /// for every building node/way centroid inside `bbox` (no corridor filter).
    ///
    /// Prefer [`Self::load_from_pbf_bbox_with_overnight_buildings_near_corridor`]
    /// for hiking plans once a route geometry exists.
    pub fn load_from_pbf_bbox_with_overnight_buildings(
        path: impl AsRef<Path>,
        bbox: [f64; 4],
    ) -> anyhow::Result<Self> {
        Self::load_from_pbf_filtered(path, Some(bbox), OvernightBuildings::BboxAll)
    }

    /// Overnight buildings restricted to a generous band around the planned
    /// corridor (`corridor_lat_lon` as `(lat, lon)` vertices, `margin_m`).
    ///
    /// Single full-extract PBF scan: POI nodes + corridor-gated overnight
    /// building nodes/way centroids (no second file pass).
    pub fn load_from_pbf_bbox_with_overnight_buildings_near_corridor(
        path: impl AsRef<Path>,
        bbox: [f64; 4],
        corridor_lat_lon: &[(f64, f64)],
        margin_m: f64,
    ) -> anyhow::Result<Self> {
        Ok(
            Self::load_from_pbf_bbox_with_overnight_buildings_near_corridor_profiled(
                path,
                bbox,
                corridor_lat_lon,
                margin_m,
            )?
            .0,
        )
    }

    /// Same as [`Self::load_from_pbf_bbox_with_overnight_buildings_near_corridor`]
    /// plus a wall-clock / counter profile for benches.
    pub fn load_from_pbf_bbox_with_overnight_buildings_near_corridor_profiled(
        path: impl AsRef<Path>,
        bbox: [f64; 4],
        corridor_lat_lon: &[(f64, f64)],
        margin_m: f64,
    ) -> anyhow::Result<(Self, PoiOvernightLoadProfile)> {
        let t_total = Instant::now();
        let t_band = Instant::now();
        let band = CorridorBand::from_lat_lon(corridor_lat_lon, margin_m);
        let band_build_ms = t_band.elapsed().as_secs_f64() * 1000.0;
        if band.is_empty() {
            let t0 = Instant::now();
            let index = Self::load_from_pbf_bbox_with_overnight_buildings(path, bbox)?;
            let mut profile = PoiOvernightLoadProfile {
                band_build_ms,
                total_ms: t_total.elapsed().as_secs_f64() * 1000.0,
                pbf_passes: 2, // bbox-all legacy path
                overnight_buildings: index.overnight_buildings.len(),
                poi_records: index.len(),
                ..Default::default()
            };
            profile.node_pass_ms = t0.elapsed().as_secs_f64() * 1000.0;
            return Ok((index, profile));
        }
        Self::load_near_corridor_profiled(path.as_ref(), bbox, band, band_build_ms, t_total)
    }

    /// Profiled two-pass NearCorridor load (node file pass + way file pass).
    ///
    /// Kept for before/after benches that isolate the cost of a redundant second
    /// full-extract scan. Production hiking uses the single-pass loader above.
    #[doc(hidden)]
    pub fn load_from_pbf_bbox_with_overnight_buildings_near_corridor_two_pass_profiled(
        path: impl AsRef<Path>,
        bbox: [f64; 4],
        corridor_lat_lon: &[(f64, f64)],
        margin_m: f64,
    ) -> anyhow::Result<(Self, PoiOvernightLoadProfile)> {
        let t_total = Instant::now();
        let t_band = Instant::now();
        let band = CorridorBand::from_lat_lon(corridor_lat_lon, margin_m);
        let band_build_ms = t_band.elapsed().as_secs_f64() * 1000.0;
        if band.is_empty() {
            let index = Self::load_from_pbf_bbox_with_overnight_buildings(path, bbox)?;
            return Ok((
                index,
                PoiOvernightLoadProfile {
                    band_build_ms,
                    total_ms: t_total.elapsed().as_secs_f64() * 1000.0,
                    pbf_passes: 2,
                    ..Default::default()
                },
            ));
        }
        Self::load_near_corridor_two_pass_profiled(
            path.as_ref(),
            bbox,
            band,
            band_build_ms,
            t_total,
        )
    }

    fn load_from_pbf_filtered(
        path: impl AsRef<Path>,
        bbox: Option<[f64; 4]>,
        overnight: OvernightBuildings,
    ) -> anyhow::Result<Self> {
        let mut index = Self::new();
        let in_bbox = |lat: f64, lon: f64| match bbox {
            Some(b) => lat >= b[0] && lat <= b[2] && lon >= b[1] && lon <= b[3],
            None => true,
        };

        let collect_buildings = !matches!(overnight, OvernightBuildings::Off);

        // Bbox-all path needs way-ref prep across the extract (legacy).
        let mut building_ways: Vec<Vec<i64>> = Vec::new();
        let mut needed: HashSet<i64> = HashSet::new();
        if matches!(overnight, OvernightBuildings::BboxAll) {
            crate::download::pbf_priority::for_each_pbf_elements(path.as_ref(), |element| {
                let Element::Way(way) = element else {
                    return;
                };
                let mut is_building = false;
                for (k, v) in way.tags() {
                    if k == "building" && v != "no" {
                        is_building = true;
                        break;
                    }
                }
                if !is_building {
                    return;
                }
                let refs: Vec<i64> = way.refs().collect();
                if refs.len() < 2 {
                    return;
                }
                for id in &refs {
                    needed.insert(*id);
                }
                building_ways.push(refs);
            })?;
            crate::download::plan_cancel::abort_if_cancelled()?;
        }

        let mut coords: HashMap<i64, (f64, f64)> = HashMap::with_capacity(needed.len().max(1024));
        {
            crate::download::pbf_priority::for_each_pbf_elements(path.as_ref(), |element| {
                match element {
                    Element::Node(node) => {
                        let lat = node.lat();
                        let lon = node.lon();
                        let id = node.id();
                        Self::ingest_node(
                            &mut index,
                            &mut coords,
                            &needed,
                            &overnight,
                            collect_buildings,
                            in_bbox,
                            id,
                            lat,
                            lon,
                            node.tags()
                                .map(|(k, v)| (k.to_string(), v.to_string()))
                                .collect(),
                        );
                    }
                    Element::DenseNode(node) => {
                        let lat = node.lat();
                        let lon = node.lon();
                        let id = node.id;
                        Self::ingest_node(
                            &mut index,
                            &mut coords,
                            &needed,
                            &overnight,
                            collect_buildings,
                            in_bbox,
                            id,
                            lat,
                            lon,
                            node.tags()
                                .map(|(k, v)| (k.to_string(), v.to_string()))
                                .collect(),
                        );
                    }
                    _ => {}
                }
            })?;
        }

        if matches!(overnight, OvernightBuildings::BboxAll) {
            for refs in building_ways {
                if let Some(pt) = centroid_in_bbox(&coords, &refs, in_bbox) {
                    index.overnight_buildings.push(pt);
                }
            }
        }

        Ok(index)
    }

    fn load_near_corridor_profiled(
        path: &Path,
        bbox: [f64; 4],
        band: CorridorBand,
        band_build_ms: f64,
        t_total: Instant,
    ) -> anyhow::Result<(Self, PoiOvernightLoadProfile)> {
        // Single full-extract scan (nodes then ways in file order).
        let mut index = Self::new();
        let mut profile = PoiOvernightLoadProfile {
            band_build_ms,
            pbf_passes: 1,
            ..Default::default()
        };
        let in_bbox = |lat: f64, lon: f64| {
            lat >= bbox[0] && lat <= bbox[2] && lon >= bbox[1] && lon <= bbox[3]
        };
        let mut coords: HashMap<i64, (f64, f64)> = HashMap::with_capacity(64_000);
        let t_pass = Instant::now();
        crate::download::pbf_priority::for_each_pbf_elements_serial(
            path,
            |element| match element {
                Element::Node(node) => {
                    Self::ingest_near_corridor_node(
                        &mut index,
                        &mut coords,
                        &band,
                        &mut profile,
                        in_bbox,
                        node.id(),
                        node.lat(),
                        node.lon(),
                        node.tags()
                            .map(|(k, v)| (k.to_string(), v.to_string()))
                            .collect(),
                    );
                }
                Element::DenseNode(node) => {
                    Self::ingest_near_corridor_node(
                        &mut index,
                        &mut coords,
                        &band,
                        &mut profile,
                        in_bbox,
                        node.id,
                        node.lat(),
                        node.lon(),
                        node.tags()
                            .map(|(k, v)| (k.to_string(), v.to_string()))
                            .collect(),
                    );
                }
                Element::Way(way) => {
                    profile.ways_seen += 1;
                    let mut is_building = false;
                    for (k, v) in way.tags() {
                        if k == "building" && v != "no" {
                            is_building = true;
                            break;
                        }
                    }
                    if !is_building {
                        return;
                    }
                    profile.building_ways_seen += 1;
                    let refs: Vec<i64> = way.refs().collect();
                    let Some((lat, lon)) = centroid_in_bbox(&coords, &refs, in_bbox) else {
                        return;
                    };
                    profile.building_ways_centroid_ok += 1;
                    profile.corridor_contains_calls += 1;
                    if band.contains(lat, lon) {
                        profile.corridor_contains_hits += 1;
                        profile.building_ways_kept += 1;
                        index.overnight_buildings.push((lat, lon));
                    }
                }
                _ => {}
            },
        )?;
        // Entire scan is one pass; attribute to node_pass_ms for "parse" and leave
        // way_pass_ms as the share approximated by counters (0 here — combined).
        profile.node_pass_ms = t_pass.elapsed().as_secs_f64() * 1000.0;
        profile.way_pass_ms = 0.0;
        profile.coords_kept = coords.len() as u64;
        profile.overnight_buildings = index.overnight_buildings.len();
        profile.poi_records = index.len();
        profile.total_ms = t_total.elapsed().as_secs_f64() * 1000.0;
        Ok((index, profile))
    }

    fn load_near_corridor_two_pass_profiled(
        path: &Path,
        bbox: [f64; 4],
        band: CorridorBand,
        band_build_ms: f64,
        t_total: Instant,
    ) -> anyhow::Result<(Self, PoiOvernightLoadProfile)> {
        let mut index = Self::new();
        let mut profile = PoiOvernightLoadProfile {
            band_build_ms,
            pbf_passes: 2,
            ..Default::default()
        };
        let in_bbox = |lat: f64, lon: f64| {
            lat >= bbox[0] && lat <= bbox[2] && lon >= bbox[1] && lon <= bbox[3]
        };
        let mut coords: HashMap<i64, (f64, f64)> = HashMap::with_capacity(64_000);

        let t_nodes = Instant::now();
        {
            let file = std::fs::File::open(path)?;
            let reader = ElementReader::new(file);
            reader.for_each(|element| match element {
                Element::Node(node) => {
                    Self::ingest_near_corridor_node(
                        &mut index,
                        &mut coords,
                        &band,
                        &mut profile,
                        in_bbox,
                        node.id(),
                        node.lat(),
                        node.lon(),
                        node.tags()
                            .map(|(k, v)| (k.to_string(), v.to_string()))
                            .collect(),
                    );
                }
                Element::DenseNode(node) => {
                    Self::ingest_near_corridor_node(
                        &mut index,
                        &mut coords,
                        &band,
                        &mut profile,
                        in_bbox,
                        node.id,
                        node.lat(),
                        node.lon(),
                        node.tags()
                            .map(|(k, v)| (k.to_string(), v.to_string()))
                            .collect(),
                    );
                }
                _ => {}
            })?;
        }
        profile.node_pass_ms = t_nodes.elapsed().as_secs_f64() * 1000.0;
        profile.coords_kept = coords.len() as u64;

        let t_ways = Instant::now();
        {
            let file = std::fs::File::open(path)?;
            let reader = ElementReader::new(file);
            reader.for_each(|element| {
                let Element::Way(way) = element else {
                    return;
                };
                profile.ways_seen += 1;
                let mut is_building = false;
                for (k, v) in way.tags() {
                    if k == "building" && v != "no" {
                        is_building = true;
                        break;
                    }
                }
                if !is_building {
                    return;
                }
                profile.building_ways_seen += 1;
                let refs: Vec<i64> = way.refs().collect();
                let Some((lat, lon)) = centroid_in_bbox(&coords, &refs, in_bbox) else {
                    return;
                };
                profile.building_ways_centroid_ok += 1;
                profile.corridor_contains_calls += 1;
                if band.contains(lat, lon) {
                    profile.corridor_contains_hits += 1;
                    profile.building_ways_kept += 1;
                    index.overnight_buildings.push((lat, lon));
                }
            })?;
        }
        profile.way_pass_ms = t_ways.elapsed().as_secs_f64() * 1000.0;
        profile.overnight_buildings = index.overnight_buildings.len();
        profile.poi_records = index.len();
        profile.total_ms = t_total.elapsed().as_secs_f64() * 1000.0;
        Ok((index, profile))
    }

    #[allow(clippy::too_many_arguments)]
    fn ingest_near_corridor_node(
        index: &mut Self,
        coords: &mut HashMap<i64, (f64, f64)>,
        band: &CorridorBand,
        profile: &mut PoiOvernightLoadProfile,
        in_bbox: impl Fn(f64, f64) -> bool,
        id: i64,
        lat: f64,
        lon: f64,
        tags: HashMap<String, String>,
    ) {
        profile.nodes_decoded += 1;
        let inside = in_bbox(lat, lon);
        if inside {
            profile.nodes_in_bbox += 1;
        }
        // Geometry retention gated by corridor envelope (cheaper than full contains).
        if inside && band.in_envelope(lat, lon) {
            coords.insert(id, (lat, lon));
        }
        if !inside {
            return;
        }
        if tags.get("building").is_some_and(|v| v != "no") {
            profile.corridor_contains_calls += 1;
            if band.contains(lat, lon) {
                profile.corridor_contains_hits += 1;
                profile.building_nodes_kept += 1;
                index.overnight_buildings.push((lat, lon));
            }
        }
        index.insert_node(id, lat, lon, tags);
    }

    #[allow(clippy::too_many_arguments)]
    fn ingest_node(
        index: &mut Self,
        coords: &mut HashMap<i64, (f64, f64)>,
        needed: &HashSet<i64>,
        overnight: &OvernightBuildings,
        collect_buildings: bool,
        in_bbox: impl Fn(f64, f64) -> bool,
        id: i64,
        lat: f64,
        lon: f64,
        tags: HashMap<String, String>,
    ) {
        if matches!(overnight, OvernightBuildings::BboxAll) && needed.contains(&id) {
            coords.insert(id, (lat, lon));
        }
        if !in_bbox(lat, lon) {
            return;
        }
        if collect_buildings && tags.get("building").is_some_and(|v| v != "no") {
            index.overnight_buildings.push((lat, lon));
        }
        index.insert_node(id, lat, lon, tags);
    }

    fn insert_node(&mut self, osm_id: i64, lat: f64, lon: f64, tags: HashMap<String, String>) {
        let categories = classify_tags(&tags);
        if categories.is_empty() {
            return;
        }
        let name = tags.get("name").cloned();
        let icon_key = osm_icon_key(&tags);
        self.insert_record(PoiRecord {
            osm_id,
            lat,
            lon,
            categories,
            icon_key,
            tags,
            name,
        });
    }

    /// Insert a fully classified record (used by hosts/tests and live POI writes).
    pub fn insert_record(&mut self, record: PoiRecord) {
        let osm_id = record.osm_id;
        let lat = record.lat;
        let lon = record.lon;
        self.records.insert(osm_id, record);
        self.tree.insert(PoiEntry { osm_id, lat, lon });
    }

    pub fn query<'a>(
        &'a self,
        category: PoiCategory,
        lat: f64,
        lon: f64,
        radius_m: f64,
    ) -> PoiQuery<'a> {
        PoiQuery {
            category,
            lat,
            lon,
            radius_m,
            index: self,
        }
    }

    pub fn nearest(
        &self,
        category: PoiCategory,
        lat: f64,
        lon: f64,
        radius_m: f64,
    ) -> Vec<&PoiRecord> {
        let delta_deg = radius_m / 111_000.0;
        let envelope = AABB::from_corners(
            [lon - delta_deg, lat - delta_deg],
            [lon + delta_deg, lat + delta_deg],
        );
        let origin = Point::new(lon, lat);
        let mut hits: Vec<&PoiRecord> = self
            .tree
            .locate_in_envelope_intersecting(&envelope)
            .filter_map(|entry| self.records.get(&entry.osm_id))
            .filter(|rec| rec.categories.contains(&category))
            .filter(|rec| Haversine::distance(origin, Point::new(rec.lon, rec.lat)) <= radius_m)
            .collect();
        hits.sort_by(|a, b| {
            let da = Haversine::distance(origin, Point::new(a.lon, a.lat));
            let db = Haversine::distance(origin, Point::new(b.lon, b.lat));
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });
        hits
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

enum OvernightBuildings {
    Off,
    BboxAll,
}

fn centroid_in_bbox(
    coords: &HashMap<i64, (f64, f64)>,
    refs: &[i64],
    in_bbox: impl Fn(f64, f64) -> bool,
) -> Option<(f64, f64)> {
    let mut sum_lat = 0.0;
    let mut sum_lon = 0.0;
    let mut n = 0usize;
    let mut any_in = false;
    for id in refs {
        let Some(&(lat, lon)) = coords.get(id) else {
            continue;
        };
        if in_bbox(lat, lon) {
            any_in = true;
        }
        sum_lat += lat;
        sum_lon += lon;
        n += 1;
    }
    if n == 0 || !any_in {
        return None;
    }
    Some((sum_lat / n as f64, sum_lon / n as f64))
}

impl Default for PoiIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> IntoIterator for PoiQuery<'a> {
    type Item = &'a PoiRecord;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.index
            .nearest(self.category, self.lat, self.lon, self.radius_m)
            .into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poi::classify_tags;

    #[test]
    fn craft_brewery_surfaces_in_nearest_query() {
        let mut index = PoiIndex::new();
        let mut tags = HashMap::new();
        tags.insert("name".into(), "Lervig Taproom".into());
        tags.insert("craft".into(), "brewery".into());
        let categories = classify_tags(&tags);
        assert!(categories.contains(&PoiCategory::CraftBrewery));
        index.insert_record(PoiRecord {
            osm_id: 42,
            lat: 58.969975,
            lon: 5.733107,
            categories,
            icon_key: osm_icon_key(&tags),
            tags,
            name: Some("Lervig Taproom".into()),
        });

        let hits = index.nearest(PoiCategory::CraftBrewery, 58.97, 5.73, 15_000.0);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name.as_deref(), Some("Lervig Taproom"));
    }
}
