//! Pass-level POI / barrier extract profile (mirrors `convert.rs`).
//!
//! Compares the production mutex visitor (`for_each_pbf_elements`) with a
//! per-blob parallel visitor (`for_each_pbf_data_block`) — the same split that
//! already sped up tiled graph pass 1.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use driver_break_core::download::pbf_priority::{
    for_each_pbf_data_block, for_each_pbf_elements, pbf_latlon_percentile_bounds,
};
use driver_break_core::poi::{classify_tags, osm_icon_key, PoiRecord};
use osmpbf::Element;

fn rss_mb() -> f64 {
    let Ok(s) = std::fs::read_to_string("/proc/self/status") else {
        return 0.0;
    };
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            let kb: f64 = rest
                .split_whitespace()
                .next()
                .and_then(|x| x.parse().ok())
                .unwrap_or(0.0);
            return kb / 1024.0;
        }
    }
    0.0
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

fn in_bbox_fn(bbox: [f64; 4]) -> impl Fn(f64, f64) -> bool {
    move |lat, lon| lat >= bbox[0] && lat <= bbox[2] && lon >= bbox[1] && lon <= bbox[3]
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

#[derive(Clone, Copy)]
enum BarrierKind {
    Line,
    Glacier,
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

fn is_building_way<'a>(tags: impl Iterator<Item = (&'a str, &'a str)>) -> bool {
    for (k, v) in tags {
        if k == "building" && v != "no" {
            return true;
        }
    }
    false
}

struct PoiPass1 {
    building_ways: Vec<Vec<i64>>,
    needed: HashSet<i64>,
}

impl PoiPass1 {
    fn new() -> Self {
        Self {
            building_ways: Vec::new(),
            needed: HashSet::new(),
        }
    }
    fn merge(&mut self, other: Self) {
        self.needed.extend(other.needed);
        self.building_ways.extend(other.building_ways);
    }
    fn consider_way(&mut self, refs: Vec<i64>) {
        if refs.len() < 2 {
            return;
        }
        for id in &refs {
            self.needed.insert(*id);
        }
        self.building_ways.push(refs);
    }
}

struct PoiPass2 {
    records: Vec<PoiRecord>,
    overnight: Vec<(f64, f64)>,
    coords: HashMap<i64, (f64, f64)>,
    tagged_nodes: u64,
}

impl PoiPass2 {
    fn new(needed_cap: usize) -> Self {
        Self {
            records: Vec::new(),
            overnight: Vec::new(),
            coords: HashMap::with_capacity(needed_cap.min(1 << 20)),
            tagged_nodes: 0,
        }
    }
    fn merge(&mut self, other: Self) {
        self.tagged_nodes += other.tagged_nodes;
        self.coords.extend(other.coords);
        self.records.extend(other.records);
        self.overnight.extend(other.overnight);
    }
    fn consider_node(
        &mut self,
        needed: &HashSet<i64>,
        in_bbox: impl Fn(f64, f64) -> bool,
        id: i64,
        lat: f64,
        lon: f64,
        tags: Option<HashMap<String, String>>,
    ) {
        if needed.contains(&id) {
            self.coords.insert(id, (lat, lon));
        }
        let Some(tags) = tags else {
            return;
        };
        self.tagged_nodes += 1;
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
}

struct BarrierPass1 {
    ways: Vec<(Vec<i64>, BarrierKind)>,
    needed: HashSet<i64>,
}

impl BarrierPass1 {
    fn new() -> Self {
        Self {
            ways: Vec::new(),
            needed: HashSet::new(),
        }
    }
    fn merge(&mut self, other: Self) {
        self.needed.extend(other.needed);
        self.ways.extend(other.ways);
    }
    fn consider_way(&mut self, refs: Vec<i64>, kind: BarrierKind) {
        if refs.len() < 2 {
            return;
        }
        for id in &refs {
            self.needed.insert(*id);
        }
        self.ways.push((refs, kind));
    }
}

struct BarrierPass2 {
    coords: HashMap<i64, (f64, f64)>,
}

impl BarrierPass2 {
    fn new(cap: usize) -> Self {
        Self {
            coords: HashMap::with_capacity(cap),
        }
    }
    fn merge(&mut self, other: Self) {
        self.coords.extend(other.coords);
    }
}

fn barrier_geom(
    ways: Vec<(Vec<i64>, BarrierKind)>,
    coords: &HashMap<i64, (f64, f64)>,
) -> (usize, usize) {
    let mut segs = 0usize;
    let mut glaciers = 0usize;
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
            BarrierKind::Line => segs += ring.len().saturating_sub(1),
            BarrierKind::Glacier => {
                segs += ring.len().saturating_sub(1);
                if ring.len() >= 3 {
                    let first = ring[0];
                    let last = *ring.last().unwrap();
                    if first != last {
                        segs += 1;
                    }
                    glaciers += 1;
                }
            }
        }
    }
    (segs, glaciers)
}

fn poi_pass1_mutex(path: &Path) -> anyhow::Result<PoiPass1> {
    let mut acc = PoiPass1::new();
    for_each_pbf_elements(path, |element| {
        let Element::Way(way) = element else {
            return;
        };
        if !is_building_way(way.tags()) {
            return;
        }
        acc.consider_way(way.refs().collect());
    })?;
    Ok(acc)
}

fn poi_pass2_mutex(path: &Path, bbox: [f64; 4], needed: &HashSet<i64>) -> anyhow::Result<PoiPass2> {
    let in_bbox = in_bbox_fn(bbox);
    let mut acc = PoiPass2::new(needed.len());
    for_each_pbf_elements(path, |element| {
        let (id, lat, lon, tags) = match element {
            Element::Node(n) => (n.id(), n.lat(), n.lon(), tags_map_if_any(n.tags())),
            Element::DenseNode(n) => (n.id, n.lat(), n.lon(), tags_map_if_any(n.tags())),
            _ => return,
        };
        acc.consider_node(needed, &in_bbox, id, lat, lon, tags);
    })?;
    Ok(acc)
}

fn poi_pass1_parallel_blob_merge(path: &Path) -> anyhow::Result<PoiPass1> {
    let merged = Mutex::new(PoiPass1::new());
    for_each_pbf_data_block(path, |block| {
        let mut acc = PoiPass1::new();
        block.for_each_element(|element| {
            let Element::Way(way) = element else {
                return;
            };
            if !is_building_way(way.tags()) {
                return;
            }
            acc.consider_way(way.refs().collect());
        });
        if !acc.building_ways.is_empty() {
            merged
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .merge(acc);
        }
        Ok(())
    })?;
    Ok(merged.into_inner().unwrap_or_else(|e| e.into_inner()))
}

fn poi_pass2_parallel(
    path: &Path,
    bbox: [f64; 4],
    needed: &HashSet<i64>,
) -> anyhow::Result<PoiPass2> {
    let in_bbox = in_bbox_fn(bbox);
    let merged = Mutex::new(PoiPass2::new(needed.len()));
    for_each_pbf_data_block(path, |block| {
        let mut acc = PoiPass2::new(256);
        block.for_each_element(|element| {
            let (id, lat, lon, tags) = match element {
                Element::Node(n) => (n.id(), n.lat(), n.lon(), tags_map_if_any(n.tags())),
                Element::DenseNode(n) => (n.id, n.lat(), n.lon(), tags_map_if_any(n.tags())),
                _ => return,
            };
            acc.consider_node(needed, &in_bbox, id, lat, lon, tags);
        });
        merged
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .merge(acc);
        Ok(())
    })?;
    Ok(merged.into_inner().unwrap_or_else(|e| e.into_inner()))
}

fn barrier_pass1_mutex(path: &Path) -> anyhow::Result<BarrierPass1> {
    let mut acc = BarrierPass1::new();
    for_each_pbf_elements(path, |element| {
        let Element::Way(way) = element else {
            return;
        };
        let tags: HashMap<String, String> = way
            .tags()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let Some(kind) = barrier_kind(&tags) else {
            return;
        };
        acc.consider_way(way.refs().collect(), kind);
    })?;
    Ok(acc)
}

fn barrier_pass2_mutex(path: &Path, needed: &HashSet<i64>) -> anyhow::Result<BarrierPass2> {
    let mut acc = BarrierPass2::new(needed.len());
    for_each_pbf_elements(path, |element| match element {
        Element::Node(n) => {
            if needed.contains(&n.id()) {
                acc.coords.insert(n.id(), (n.lat(), n.lon()));
            }
        }
        Element::DenseNode(n) => {
            if needed.contains(&n.id()) {
                acc.coords.insert(n.id(), (n.lat(), n.lon()));
            }
        }
        _ => {}
    })?;
    Ok(acc)
}

fn barrier_pass1_parallel(path: &Path) -> anyhow::Result<BarrierPass1> {
    let merged = Mutex::new(BarrierPass1::new());
    for_each_pbf_data_block(path, |block| {
        let mut acc = BarrierPass1::new();
        block.for_each_element(|element| {
            let Element::Way(way) = element else {
                return;
            };
            let tags: HashMap<String, String> = way
                .tags()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            let Some(kind) = barrier_kind(&tags) else {
                return;
            };
            acc.consider_way(way.refs().collect(), kind);
        });
        if !acc.ways.is_empty() {
            merged
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .merge(acc);
        }
        Ok(())
    })?;
    Ok(merged.into_inner().unwrap_or_else(|e| e.into_inner()))
}

fn barrier_pass2_parallel(path: &Path, needed: &HashSet<i64>) -> anyhow::Result<BarrierPass2> {
    let merged = Mutex::new(BarrierPass2::new(needed.len()));
    for_each_pbf_data_block(path, |block| {
        let mut acc = BarrierPass2::new(256);
        block.for_each_element(|element| match element {
            Element::Node(n) => {
                if needed.contains(&n.id()) {
                    acc.coords.insert(n.id(), (n.lat(), n.lon()));
                }
            }
            Element::DenseNode(n) => {
                if needed.contains(&n.id()) {
                    acc.coords.insert(n.id(), (n.lat(), n.lon()));
                }
            }
            _ => {}
        });
        if !acc.coords.is_empty() {
            merged
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .merge(acc);
        }
        Ok(())
    })?;
    Ok(merged.into_inner().unwrap_or_else(|e| e.into_inner()))
}

fn combined_pass1_parallel(path: &Path) -> anyhow::Result<(PoiPass1, BarrierPass1)> {
    let poi = Mutex::new(PoiPass1::new());
    let bar = Mutex::new(BarrierPass1::new());
    for_each_pbf_data_block(path, |block| {
        let mut p = PoiPass1::new();
        let mut b = BarrierPass1::new();
        block.for_each_element(|element| {
            let Element::Way(way) = element else {
                return;
            };
            if is_building_way(way.tags()) {
                p.consider_way(way.refs().collect());
            }
            let tags: HashMap<String, String> = way
                .tags()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            if let Some(kind) = barrier_kind(&tags) {
                b.consider_way(way.refs().collect(), kind);
            }
        });
        if !p.building_ways.is_empty() {
            poi.lock().unwrap_or_else(|e| e.into_inner()).merge(p);
        }
        if !b.ways.is_empty() {
            bar.lock().unwrap_or_else(|e| e.into_inner()).merge(b);
        }
        Ok(())
    })?;
    Ok((
        poi.into_inner().unwrap_or_else(|e| e.into_inner()),
        bar.into_inner().unwrap_or_else(|e| e.into_inner()),
    ))
}

fn combined_pass2_parallel(
    path: &Path,
    bbox: [f64; 4],
    poi_needed: &HashSet<i64>,
    bar_needed: &HashSet<i64>,
) -> anyhow::Result<(PoiPass2, BarrierPass2)> {
    let in_bbox = in_bbox_fn(bbox);
    let poi = Mutex::new(PoiPass2::new(poi_needed.len()));
    let bar = Mutex::new(BarrierPass2::new(bar_needed.len()));
    for_each_pbf_data_block(path, |block| {
        let mut p = PoiPass2::new(256);
        let mut b = BarrierPass2::new(256);
        block.for_each_element(|element| {
            let (id, lat, lon, tags) = match element {
                Element::Node(n) => (n.id(), n.lat(), n.lon(), tags_map_if_any(n.tags())),
                Element::DenseNode(n) => (n.id, n.lat(), n.lon(), tags_map_if_any(n.tags())),
                _ => return,
            };
            p.consider_node(poi_needed, &in_bbox, id, lat, lon, tags);
            if bar_needed.contains(&id) {
                b.coords.insert(id, (lat, lon));
            }
        });
        poi.lock().unwrap_or_else(|e| e.into_inner()).merge(p);
        if !b.coords.is_empty() {
            bar.lock().unwrap_or_else(|e| e.into_inner()).merge(b);
        }
        Ok(())
    })?;
    Ok((
        poi.into_inner().unwrap_or_else(|e| e.into_inner()),
        bar.into_inner().unwrap_or_else(|e| e.into_inner()),
    ))
}

fn decode_only(path: &Path) -> anyhow::Result<(f64, u64, u64, u64)> {
    let t0 = Instant::now();
    let nodes = AtomicU64::new(0);
    let ways = AtomicU64::new(0);
    let rels = AtomicU64::new(0);
    for_each_pbf_data_block(path, |block| {
        let mut n = 0u64;
        let mut w = 0u64;
        let mut r = 0u64;
        block.for_each_element(|el| match el {
            Element::Node(_) | Element::DenseNode(_) => n += 1,
            Element::Way(_) => w += 1,
            Element::Relation(_) => r += 1,
        });
        nodes.fetch_add(n, Ordering::Relaxed);
        ways.fetch_add(w, Ordering::Relaxed);
        rels.fetch_add(r, Ordering::Relaxed);
        Ok(())
    })?;
    Ok((
        t0.elapsed().as_secs_f64() * 1000.0,
        nodes.load(Ordering::Relaxed),
        ways.load(Ordering::Relaxed),
        rels.load(Ordering::Relaxed),
    ))
}

pub fn cmd_poi_barrier_profile(path: &Path) -> anyhow::Result<()> {
    println!("POI_BARRIER_PROFILE path={}", path.display());
    println!("rss0_mb={:.1}", rss_mb());

    let (decode_ms, nodes, ways, rels) = decode_only(path)?;
    println!(
        "DECODE_ONLY ms={decode_ms:.1} nodes={nodes} ways={ways} rels={rels} rss_mb={:.1}",
        rss_mb()
    );

    let t_bbox = Instant::now();
    let raw = pbf_latlon_percentile_bounds(path, 0.005, 0.995)?;
    let bbox = [raw[0] - 0.02, raw[1] - 0.02, raw[2] + 0.02, raw[3] + 0.02];
    println!(
        "BBOX ms={:.1} bbox={bbox:?} rss_mb={:.1}",
        t_bbox.elapsed().as_secs_f64() * 1000.0,
        rss_mb()
    );

    // --- production mutex path (convert.rs) ---
    let t = Instant::now();
    let poi1 = poi_pass1_mutex(path)?;
    let poi1_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "POI_MUTEX pass1_ms={poi1_ms:.1} building_ways={} needed={} rss_mb={:.1}",
        poi1.building_ways.len(),
        poi1.needed.len(),
        rss_mb()
    );

    let t = Instant::now();
    let mut poi2 = poi_pass2_mutex(path, bbox, &poi1.needed)?;
    let poi2_ms = t.elapsed().as_secs_f64() * 1000.0;
    let t_c = Instant::now();
    let in_bbox = in_bbox_fn(bbox);
    for refs in &poi1.building_ways {
        if let Some(pt) = centroid_in_bbox(&poi2.coords, refs, &in_bbox) {
            poi2.overnight.push(pt);
        }
    }
    let centroid_ms = t_c.elapsed().as_secs_f64() * 1000.0;
    println!(
        "POI_MUTEX pass2_ms={poi2_ms:.1} centroid_ms={centroid_ms:.1} pois={} overnight={} tagged_nodes={} coords={} rss_mb={:.1}",
        poi2.records.len(),
        poi2.overnight.len(),
        poi2.tagged_nodes,
        poi2.coords.len(),
        rss_mb()
    );
    println!(
        "POI_MUTEX total_ms={:.1}",
        poi1_ms + poi2_ms + centroid_ms
    );
    drop(poi1);
    drop(poi2);

    let t = Instant::now();
    let bar1 = barrier_pass1_mutex(path)?;
    let bar1_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "BARRIER_MUTEX pass1_ms={bar1_ms:.1} ways={} needed={} rss_mb={:.1}",
        bar1.ways.len(),
        bar1.needed.len(),
        rss_mb()
    );

    let t = Instant::now();
    let bar2 = barrier_pass2_mutex(path, &bar1.needed)?;
    let bar2_ms = t.elapsed().as_secs_f64() * 1000.0;
    let t_g = Instant::now();
    let (segs, glaciers) = barrier_geom(bar1.ways, &bar2.coords);
    let geom_ms = t_g.elapsed().as_secs_f64() * 1000.0;
    println!(
        "BARRIER_MUTEX pass2_ms={bar2_ms:.1} geom_ms={geom_ms:.1} coords={} segs={segs} glaciers={glaciers} rss_mb={:.1}",
        bar2.coords.len(),
        rss_mb()
    );
    println!("BARRIER_MUTEX total_ms={:.1}", bar1_ms + bar2_ms + geom_ms);
    drop(bar2);

    // --- parallel per-blob (graph-pass1 class) ---
    let t = Instant::now();
    let poi1p = poi_pass1_parallel_blob_merge(path)?;
    let poi1p_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "POI_PAR pass1_ms={poi1p_ms:.1} building_ways={} needed={} rss_mb={:.1}",
        poi1p.building_ways.len(),
        poi1p.needed.len(),
        rss_mb()
    );
    let t = Instant::now();
    let poi2p = poi_pass2_parallel(path, bbox, &poi1p.needed)?;
    let poi2p_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "POI_PAR pass2_ms={poi2p_ms:.1} pois={} overnight_nodes={} tagged_nodes={} coords={} rss_mb={:.1}",
        poi2p.records.len(),
        poi2p.overnight.len(),
        poi2p.tagged_nodes,
        poi2p.coords.len(),
        rss_mb()
    );
    println!("POI_PAR total_ms={:.1}", poi1p_ms + poi2p_ms);
    drop(poi1p);
    drop(poi2p);

    let t = Instant::now();
    let bar1p = barrier_pass1_parallel(path)?;
    let bar1p_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "BARRIER_PAR pass1_ms={bar1p_ms:.1} ways={} needed={} rss_mb={:.1}",
        bar1p.ways.len(),
        bar1p.needed.len(),
        rss_mb()
    );
    let t = Instant::now();
    let bar2p = barrier_pass2_parallel(path, &bar1p.needed)?;
    let bar2p_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "BARRIER_PAR pass2_ms={bar2p_ms:.1} coords={} rss_mb={:.1}",
        bar2p.coords.len(),
        rss_mb()
    );
    println!("BARRIER_PAR total_ms={:.1}", bar1p_ms + bar2p_ms);
    drop(bar1p);
    drop(bar2p);

    // Shared 2-pass (fold POI + PBF-barrier into the same blob-parallel scans).
    let t = Instant::now();
    let (c_poi1, c_bar1) = combined_pass1_parallel(path)?;
    let c1_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "COMBINED_PAR pass1_ms={c1_ms:.1} building_ways={} poi_needed={} barrier_ways={} barrier_needed={} rss_mb={:.1}",
        c_poi1.building_ways.len(),
        c_poi1.needed.len(),
        c_bar1.ways.len(),
        c_bar1.needed.len(),
        rss_mb()
    );
    let t = Instant::now();
    let (c_poi2, c_bar2) = combined_pass2_parallel(path, bbox, &c_poi1.needed, &c_bar1.needed)?;
    let c2_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "COMBINED_PAR pass2_ms={c2_ms:.1} pois={} barrier_coords={} rss_mb={:.1}",
        c_poi2.records.len(),
        c_bar2.coords.len(),
        rss_mb()
    );
    println!("COMBINED_PAR total_ms={:.1}", c1_ms + c2_ms);
    drop((c_poi1, c_bar1, c_poi2, c_bar2));

    println!("peak_rss_mb={:.1}", rss_mb());
    Ok(())
}
