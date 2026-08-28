//! Shared blob-parallel POI + PBF danger-barrier extract for region convert.
//!
//! Two `for_each_pbf_data_block` walks (graph pass-1 visitor pattern). Keep-sets
//! match the former sequential mutex scans; highway/trunk extras stay on the
//! caller (car graph).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use osmpbf::Element;

use crate::download::pbf_priority::for_each_pbf_data_block;
use crate::poi::{classify_tags, osm_icon_key, PoiRecord};

pub(crate) type BarrierLineBboxes = Vec<(f64, f64, f64, f64)>;
pub(crate) type BarrierPolylines = Vec<Vec<[f64; 2]>>;

pub(crate) type SharedPoiBarrierOut = (
    Vec<PoiRecord>,
    Vec<(f64, f64)>,
    BarrierLineBboxes,
    BarrierPolylines,
);

#[derive(Clone, Copy)]
enum BarrierKind {
    Line,
    Glacier,
}

fn tags_map_if_any<'a>(
    tags: impl Iterator<Item = (&'a str, &'a str)>,
) -> Option<HashMap<String, String>> {
    let mut iter = tags;
    let (k0, v0) = iter.next()?;
    let mut map = HashMap::new();
    map.insert(k0.to_string(), v0.to_string());
    for (k, v) in iter {
        map.insert(k.to_string(), v.to_string());
    }
    Some(map)
}

fn is_building_way<'a>(tags: impl Iterator<Item = (&'a str, &'a str)>) -> bool {
    for (k, v) in tags {
        if k == "building" && v != "no" {
            return true;
        }
    }
    false
}

fn barrier_kind(tags: &HashMap<String, String>) -> Option<BarrierKind> {
    let railway = tags.get("railway").map(String::as_str);
    let waterway = tags.get("waterway").map(String::as_str);
    let natural = tags.get("natural").map(String::as_str);
    if matches!(
        railway,
        Some(r) if !matches!(r, "abandoned" | "disused" | "razed" | "dismantled")
    ) || matches!(waterway, Some("river" | "canal"))
        || matches!(natural, Some("cliff" | "arete"))
    {
        Some(BarrierKind::Line)
    } else if natural == Some("glacier") {
        Some(BarrierKind::Glacier)
    } else {
        None
    }
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

struct SharedPass1 {
    building_ways: Vec<Vec<i64>>,
    poi_needed: HashSet<i64>,
    barrier_ways: Vec<(Vec<i64>, BarrierKind)>,
    barrier_needed: HashSet<i64>,
}

impl SharedPass1 {
    fn new() -> Self {
        Self {
            building_ways: Vec::new(),
            poi_needed: HashSet::new(),
            barrier_ways: Vec::new(),
            barrier_needed: HashSet::new(),
        }
    }

    fn merge(&mut self, other: Self) {
        self.poi_needed.extend(other.poi_needed);
        self.barrier_needed.extend(other.barrier_needed);
        self.building_ways.extend(other.building_ways);
        self.barrier_ways.extend(other.barrier_ways);
    }

    fn consider_building(&mut self, refs: Vec<i64>) {
        if refs.len() < 2 {
            return;
        }
        for id in &refs {
            self.poi_needed.insert(*id);
        }
        self.building_ways.push(refs);
    }

    fn consider_barrier(&mut self, refs: Vec<i64>, kind: BarrierKind) {
        if refs.len() < 2 {
            return;
        }
        for id in &refs {
            self.barrier_needed.insert(*id);
        }
        self.barrier_ways.push((refs, kind));
    }

    fn is_empty(&self) -> bool {
        self.building_ways.is_empty() && self.barrier_ways.is_empty()
    }
}

struct SharedPass2 {
    records: Vec<PoiRecord>,
    overnight: Vec<(f64, f64)>,
    poi_coords: HashMap<i64, (f64, f64)>,
    barrier_coords: HashMap<i64, (f64, f64)>,
}

impl SharedPass2 {
    fn new(poi_cap: usize, bar_cap: usize) -> Self {
        Self {
            records: Vec::new(),
            overnight: Vec::new(),
            poi_coords: HashMap::with_capacity(poi_cap),
            barrier_coords: HashMap::with_capacity(bar_cap),
        }
    }

    fn merge(&mut self, other: Self) {
        self.poi_coords.extend(other.poi_coords);
        self.barrier_coords.extend(other.barrier_coords);
        self.records.extend(other.records);
        self.overnight.extend(other.overnight);
    }

    fn consider_node(
        &mut self,
        poi_needed: &HashSet<i64>,
        barrier_needed: &HashSet<i64>,
        in_bbox: impl Fn(f64, f64) -> bool,
        id: i64,
        lat: f64,
        lon: f64,
        tags: Option<HashMap<String, String>>,
    ) {
        if poi_needed.contains(&id) {
            self.poi_coords.insert(id, (lat, lon));
        }
        if barrier_needed.contains(&id) {
            self.barrier_coords.insert(id, (lat, lon));
        }
        let Some(tags) = tags else {
            return;
        };
        if in_bbox(lat, lon) && tags.get("building").is_some_and(|v| v != "no") {
            self.overnight.push((lat, lon));
        }
        let categories = classify_tags(&tags);
        if categories.is_empty() {
            return;
        }
        self.records.push(PoiRecord {
            osm_id: id,
            lat,
            lon,
            categories,
            icon_key: osm_icon_key(&tags),
            name: tags.get("name").cloned(),
            tags,
        });
    }

    fn is_empty(&self) -> bool {
        self.records.is_empty()
            && self.overnight.is_empty()
            && self.poi_coords.is_empty()
            && self.barrier_coords.is_empty()
    }
}

fn barrier_geom(
    ways: Vec<(Vec<i64>, BarrierKind)>,
    coords: &HashMap<i64, (f64, f64)>,
) -> (BarrierLineBboxes, BarrierPolylines) {
    let mut segs = Vec::new();
    let mut glaciers = Vec::new();
    for (refs, kind) in ways {
        let mut ring: Vec<[f64; 2]> = Vec::with_capacity(refs.len());
        for id in &refs {
            let Some(&(lat, lon)) = coords.get(id) else {
                continue;
            };
            ring.push([lon, lat]);
        }
        if ring.len() < 2 {
            continue;
        }
        match kind {
            BarrierKind::Line => {
                for w in ring.windows(2) {
                    segs.push((w[0][0], w[0][1], w[1][0], w[1][1]));
                }
            }
            BarrierKind::Glacier => {
                for w in ring.windows(2) {
                    segs.push((w[0][0], w[0][1], w[1][0], w[1][1]));
                }
                if ring.len() >= 3 {
                    let first = ring[0];
                    let last = *ring.last().unwrap();
                    if first != last {
                        segs.push((last[0], last[1], first[0], first[1]));
                        ring.push(first);
                    }
                    glaciers.push(ring);
                }
            }
        }
    }
    (segs, glaciers)
}

/// Shared POI + PBF danger-barrier extract: two blob-parallel file walks.
pub(crate) fn extract_poi_and_pbf_barriers(
    pbf: &Path,
    bbox: [f64; 4],
) -> anyhow::Result<SharedPoiBarrierOut> {
    let in_bbox =
        |lat: f64, lon: f64| lat >= bbox[0] && lat <= bbox[2] && lon >= bbox[1] && lon <= bbox[3];

    let t_p1 = Instant::now();
    let pass1_acc = Mutex::new(SharedPass1::new());
    for_each_pbf_data_block(pbf, |block| {
        let mut acc = SharedPass1::new();
        block.for_each_element(|element| {
            let Element::Way(way) = element else {
                return;
            };
            let building = is_building_way(way.tags());
            let tags: HashMap<String, String> = way
                .tags()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            let kind = barrier_kind(&tags);
            if !building && kind.is_none() {
                return;
            }
            let refs: Vec<i64> = way.refs().collect();
            match (building, kind) {
                (true, Some(k)) => {
                    acc.consider_building(refs.clone());
                    acc.consider_barrier(refs, k);
                }
                (true, None) => acc.consider_building(refs),
                (false, Some(k)) => acc.consider_barrier(refs, k),
                (false, None) => {}
            }
        });
        if !acc.is_empty() {
            pass1_acc
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .merge(acc);
        }
        Ok(())
    })?;
    let pass1 = pass1_acc.into_inner().unwrap_or_else(|e| e.into_inner());
    let pass1_ms = t_p1.elapsed().as_secs_f64() * 1000.0;
    crate::download::plan_cancel::abort_if_cancelled()?;

    let t_p2 = Instant::now();
    let poi_needed = pass1.poi_needed;
    let barrier_needed = pass1.barrier_needed;
    let pass2_acc = Mutex::new(SharedPass2::new(poi_needed.len(), barrier_needed.len()));
    for_each_pbf_data_block(pbf, |block| {
        let mut acc = SharedPass2::new(256, 256);
        block.for_each_element(|element| {
            let (id, lat, lon, tags) = match element {
                Element::Node(n) => (n.id(), n.lat(), n.lon(), tags_map_if_any(n.tags())),
                Element::DenseNode(n) => (n.id, n.lat(), n.lon(), tags_map_if_any(n.tags())),
                _ => return,
            };
            acc.consider_node(&poi_needed, &barrier_needed, in_bbox, id, lat, lon, tags);
        });
        if !acc.is_empty() {
            pass2_acc
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .merge(acc);
        }
        Ok(())
    })?;
    let mut pass2 = pass2_acc.into_inner().unwrap_or_else(|e| e.into_inner());
    let pass2_ms = t_p2.elapsed().as_secs_f64() * 1000.0;

    let t_c = Instant::now();
    for refs in &pass1.building_ways {
        if let Some(pt) = centroid_in_bbox(&pass2.poi_coords, refs, in_bbox) {
            pass2.overnight.push(pt);
        }
    }
    let centroid_ms = t_c.elapsed().as_secs_f64() * 1000.0;
    drop(pass2.poi_coords);

    let t_g = Instant::now();
    let (segs, glaciers) = barrier_geom(pass1.barrier_ways, &pass2.barrier_coords);
    let geom_ms = t_g.elapsed().as_secs_f64() * 1000.0;
    drop(pass2.barrier_coords);

    log::info!(
        target: "NaviConvert",
        "CONVERT_PHASE poi_barrier_shared pass1_ms={pass1_ms:.1} pass2_ms={pass2_ms:.1} centroid_ms={centroid_ms:.1} geom_ms={geom_ms:.1} pois={} overnight={} pbf_barrier_segs={} glaciers={}",
        pass2.records.len(),
        pass2.overnight.len(),
        segs.len(),
        glaciers.len()
    );

    Ok((pass2.records, pass2.overnight, segs, glaciers))
}
