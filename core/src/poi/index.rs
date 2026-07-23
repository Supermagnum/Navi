use std::collections::{HashMap, HashSet};
use std::path::Path;

use geo::{Distance, Haversine, Point};
use osmpbf::{Element, ElementReader};
use rstar::{RTree, RTreeObject, AABB};

use super::{classify_tags, osm_icon_key, PoiCategory};

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
    /// Building nodes / way centroids when loaded via
    /// [`Self::load_from_pbf_bbox_with_overnight_buildings`].
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

    pub fn load_from_pbf(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        Self::load_from_pbf_filtered(path, None, false)
    }

    /// Load only POI nodes inside `bbox` `[min_lat, min_lon, max_lat, max_lon]`.
    /// Single node pass — used by motor route break POIs.
    pub fn load_from_pbf_bbox(path: impl AsRef<Path>, bbox: [f64; 4]) -> anyhow::Result<Self> {
        Self::load_from_pbf_filtered(path, Some(bbox), false)
    }

    /// Like [`Self::load_from_pbf_bbox`], plus overnight building proximity points
    /// in the same PBF I/O (building nodes + building-way centroids). For hiking.
    pub fn load_from_pbf_bbox_with_overnight_buildings(
        path: impl AsRef<Path>,
        bbox: [f64; 4],
    ) -> anyhow::Result<Self> {
        Self::load_from_pbf_filtered(path, Some(bbox), true)
    }

    fn load_from_pbf_filtered(
        path: impl AsRef<Path>,
        bbox: Option<[f64; 4]>,
        collect_overnight_buildings: bool,
    ) -> anyhow::Result<Self> {
        let mut index = Self::new();
        let in_bbox = |lat: f64, lon: f64| match bbox {
            Some(b) => lat >= b[0] && lat <= b[2] && lon >= b[1] && lon <= b[3],
            None => true,
        };

        let mut building_ways: Vec<Vec<i64>> = Vec::new();
        let mut needed: HashSet<i64> = HashSet::new();
        if collect_overnight_buildings {
            // Barrier-style: only resolve nodes that appear on building ways.
            let file = std::fs::File::open(path.as_ref())?;
            let reader = ElementReader::new(file);
            reader.for_each(|element| {
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
        }

        let mut coords: HashMap<i64, (f64, f64)> = HashMap::with_capacity(needed.len());
        {
            let file = std::fs::File::open(path.as_ref())?;
            let reader = ElementReader::new(file);
            reader.for_each(|element| {
                match element {
                    Element::Node(node) => {
                        let lat = node.lat();
                        let lon = node.lon();
                        let id = node.id();
                        if needed.contains(&id) {
                            coords.insert(id, (lat, lon));
                        }
                        if !in_bbox(lat, lon) {
                            return;
                        }
                        let tags: HashMap<String, String> = node
                            .tags()
                            .map(|(k, v)| (k.to_string(), v.to_string()))
                            .collect();
                        if collect_overnight_buildings
                            && tags.get("building").is_some_and(|v| v != "no")
                        {
                            index.overnight_buildings.push((lat, lon));
                        }
                        index.insert_node(id, lat, lon, tags);
                    }
                    Element::DenseNode(node) => {
                        let lat = node.lat();
                        let lon = node.lon();
                        let id = node.id;
                        if needed.contains(&id) {
                            coords.insert(id, (lat, lon));
                        }
                        if !in_bbox(lat, lon) {
                            return;
                        }
                        let tags: HashMap<String, String> = node
                            .tags()
                            .map(|(k, v)| (k.to_string(), v.to_string()))
                            .collect();
                        if collect_overnight_buildings
                            && tags.get("building").is_some_and(|v| v != "no")
                        {
                            index.overnight_buildings.push((lat, lon));
                        }
                        index.insert_node(id, lat, lon, tags);
                    }
                    _ => {}
                }
            })?;
        }

        for refs in building_ways {
            let mut sum_lat = 0.0;
            let mut sum_lon = 0.0;
            let mut n = 0usize;
            let mut any_in = false;
            for id in refs {
                let Some(&(lat, lon)) = coords.get(&id) else {
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
                continue;
            }
            index
                .overnight_buildings
                .push((sum_lat / n as f64, sum_lon / n as f64));
        }

        Ok(index)
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

    pub fn query<'a>(&'a self, category: PoiCategory, lat: f64, lon: f64, radius_m: f64) -> PoiQuery<'a> {
        PoiQuery {
            category,
            lat,
            lon,
            radius_m,
            index: self,
        }
    }

    pub fn nearest(&self, category: PoiCategory, lat: f64, lon: f64, radius_m: f64) -> Vec<&PoiRecord> {
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
            .filter(|rec| {
                Haversine::distance(origin, Point::new(rec.lon, rec.lat)) <= radius_m
            })
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

        let hits = index.nearest(
            PoiCategory::CraftBrewery,
            58.97,
            5.73,
            15_000.0,
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name.as_deref(), Some("Lervig Taproom"));
    }
}
