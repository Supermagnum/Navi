//! Region pack converter: local PBF (+ optional DEM) → graph + poi/barrier archives.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use osmpbf::Element;
use rkyv::rancor::Error as RkyvError;

use super::graph_pack::{FlatGraphPack, GRAPH_FORMAT_VERSION, MAGIC_GRAPH};
use super::header::Preamble;
use super::io::{discard_partial, write_archive_atomic};
use super::manifest::{
    graph_pack_filename, graph_tile_filename, manifest_path, pbf_fingerprint,
    poi_barrier_pack_filename, profile_key, wetland_pack_filename, wetland_tile_filename,
    GraphTileEntry, NaviManifest,
};
use super::poi_barrier_pack::{FlatPoiBarrierPack, MAGIC_POI_BARRIER, POI_BARRIER_FORMAT_VERSION};
use super::wetland_pack::{FlatWetlandPack, MAGIC_WETLAND, WETLAND_FORMAT_VERSION};
use crate::download::progress as download_progress;
use crate::download::DownloadControl;
use crate::poi::{classify_tags, osm_icon_key, PoiRecord};
use crate::routing::elevation::{ElevationCache, ElevationService};
use crate::routing::graph::{RouteGraph, RoutingProfile};
use crate::routing::wetland::WetlandIndex;

#[derive(Clone)]
pub struct ConvertOptions {
    pub data_dir: PathBuf,
    pub pbf: PathBuf,
    /// When set, sample per-edge Δh into the graph pack.
    pub elev_dir: Option<PathBuf>,
    /// Profiles to emit. Default: car + foot (covers motor + hiking).
    pub profiles: Vec<RoutingProfile>,
    pub control: DownloadControl,
}

impl ConvertOptions {
    pub fn new(data_dir: impl Into<PathBuf>, pbf: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            pbf: pbf.into(),
            elev_dir: None,
            profiles: vec![RoutingProfile::Car, RoutingProfile::Foot],
            control: DownloadControl::default(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ConvertReport {
    pub stem: String,
    pub graph_files: Vec<String>,
    pub poi_barrier_file: String,
    pub wetland_file: String,
    pub manifest_file: String,
    pub nodes: usize,
    pub edges: usize,
    pub pois: usize,
    pub barrier_segs: usize,
    pub wetland_rings: usize,
    pub convert_ms: f64,
    pub has_delta_h: bool,
    /// Peak process RSS observed during convert (MiB), when `/proc` is available.
    pub peak_rss_mb: f64,
    /// Number of spatial graph tiles written (0 = monolithic).
    pub graph_tiles: usize,
    /// Wall time for the PBF node-extent bbox scan.
    pub bbox_scan_ms: f64,
    /// Wall time per routing profile graph build (key = `profile_key`).
    pub graph_ms: BTreeMap<String, f64>,
    /// Per-profile tile way-assignment time (tiled convert only).
    pub tile_assign_ms: BTreeMap<String, f64>,
    /// Per-profile parallel tile build+pack time (tiled convert only).
    pub tile_build_ms: BTreeMap<String, f64>,
    /// Wall time for POI node collection.
    pub poi_ms: f64,
    /// Wall time for barrier way + node-coord extraction.
    pub barrier_ms: f64,
    /// Wall time for overnight-building detection (0 when folded into POI).
    pub overnight_ms: f64,
    /// Wall time for wetland extract + pack write.
    pub wetland_ms: f64,
}

fn stem_of(pbf: &Path) -> String {
    let name = pbf
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("region.osm.pbf");
    let stem = name
        .strip_suffix(".osm.pbf")
        .or_else(|| name.strip_suffix(".pbf"))
        .unwrap_or(name);
    stem.to_string()
}

/// Scan PBF node extents so we can use the memory-safe bbox builder.
///
/// Uses 0.5–99.5 percentiles over all node coordinates (parallel merge-sort,
/// order-independent) so a few garbage OSM coordinates do not inflate tiling
/// into hundreds of empty cells.
fn pbf_node_bbox(pbf: &Path) -> anyhow::Result<[f64; 4]> {
    let raw = crate::download::pbf_priority::pbf_latlon_percentile_bounds(pbf, 0.005, 0.995)?;
    // Small pad so boundary ways are not clipped away.
    Ok([raw[0] - 0.02, raw[1] - 0.02, raw[2] + 0.02, raw[3] + 0.02])
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

/// POI nodes plus overnight-building centroids in one pair of PBF scans
/// (replaces a separate `PoiIndex::load_from_pbf_bbox_with_overnight_buildings`).
type PoiCollectOut = (Vec<PoiRecord>, Vec<(f64, f64)>);

fn collect_poi_records(pbf: &Path, bbox: [f64; 4]) -> anyhow::Result<PoiCollectOut> {
    let in_bbox =
        |lat: f64, lon: f64| lat >= bbox[0] && lat <= bbox[2] && lon >= bbox[1] && lon <= bbox[3];

    let mut building_ways: Vec<Vec<i64>> = Vec::new();
    let mut needed: HashSet<i64> = HashSet::new();
    crate::download::pbf_priority::for_each_pbf_elements(pbf, |element| {
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

    let mut out = Vec::new();
    let mut overnight = Vec::new();
    let mut coords: HashMap<i64, (f64, f64)> = HashMap::with_capacity(needed.len().max(1024));
    crate::download::pbf_priority::for_each_pbf_elements(pbf, |element| {
        let (id, lat, lon, tags) = match element {
            Element::Node(n) => (n.id(), n.lat(), n.lon(), tags_map_if_any(n.tags())),
            Element::DenseNode(n) => (n.id, n.lat(), n.lon(), tags_map_if_any(n.tags())),
            _ => return,
        };
        if needed.contains(&id) {
            coords.insert(id, (lat, lon));
        }
        let Some(tags) = tags else {
            return;
        };
        if in_bbox(lat, lon) && tags.get("building").is_some_and(|v| v != "no") {
            overnight.push((lat, lon));
        }
        let categories = classify_tags(&tags);
        if categories.is_empty() {
            return;
        }
        out.push(PoiRecord {
            osm_id: id,
            lat,
            lon,
            categories,
            icon_key: osm_icon_key(&tags),
            name: tags.get("name").cloned(),
            tags,
        });
    })?;

    for refs in building_ways {
        if let Some(pt) = centroid_in_bbox(&coords, &refs, in_bbox) {
            overnight.push(pt);
        }
    }
    Ok((out, overnight))
}

type BarrierLineBboxes = Vec<(f64, f64, f64, f64)>;
type BarrierPolylines = Vec<Vec<[f64; 2]>>;

fn extract_pbf_barrier_geometry(
    pbf: &Path,
) -> anyhow::Result<(BarrierLineBboxes, BarrierPolylines)> {
    #[derive(Clone, Copy)]
    enum Kind {
        Line,
        Glacier,
    }

    let mut ways: Vec<(Vec<i64>, Kind)> = Vec::new();
    let mut needed: HashSet<i64> = HashSet::new();
    {
        crate::download::pbf_priority::for_each_pbf_elements(pbf, |element| {
            let Element::Way(way) = element else {
                return;
            };
            let tags: HashMap<String, String> = way
                .tags()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            let railway = tags.get("railway").map(String::as_str);
            let waterway = tags.get("waterway").map(String::as_str);
            let natural = tags.get("natural").map(String::as_str);
            let kind = if matches!(
                railway,
                Some(r) if !matches!(r, "abandoned" | "disused" | "razed" | "dismantled")
            ) || matches!(waterway, Some("river" | "canal"))
                || matches!(natural, Some("cliff" | "arete"))
            {
                Some(Kind::Line)
            } else if natural == Some("glacier") {
                Some(Kind::Glacier)
            } else {
                None
            };
            let Some(kind) = kind else {
                return;
            };
            let refs: Vec<i64> = way.refs().collect();
            if refs.len() < 2 {
                return;
            }
            for id in &refs {
                needed.insert(*id);
            }
            ways.push((refs, kind));
        })?;
    }

    let mut coords: HashMap<i64, (f64, f64)> = HashMap::with_capacity(needed.len());
    {
        crate::download::pbf_priority::for_each_pbf_elements(pbf, |element| match element {
            Element::Node(n) => {
                if needed.contains(&n.id()) {
                    coords.insert(n.id(), (n.lat(), n.lon()));
                }
            }
            Element::DenseNode(n) => {
                if needed.contains(&n.id()) {
                    coords.insert(n.id(), (n.lat(), n.lon()));
                }
            }
            _ => {}
        })?;
    }

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
            Kind::Line => {
                for w in ring.windows(2) {
                    segs.push((w[0][0], w[0][1], w[1][0], w[1][1]));
                }
            }
            Kind::Glacier => {
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
    Ok((segs, glaciers))
}

fn highway_barrier_segs(graph: &RouteGraph) -> Vec<(f64, f64, f64, f64)> {
    let mut segs = Vec::new();
    for e in &graph.edges {
        let Some(h) = e.highway.as_deref() else {
            continue;
        };
        if !matches!(h, "motorway" | "motorway_link" | "trunk" | "trunk_link") {
            continue;
        }
        let Some(a) = graph.nodes.get(&e.source) else {
            continue;
        };
        let Some(b) = graph.nodes.get(&e.target) else {
            continue;
        };
        segs.push((a.coord.x, a.coord.y, b.coord.x, b.coord.y));
    }
    segs
}

fn rss_mb() -> f64 {
    let Ok(s) = std::fs::read_to_string("/proc/self/status") else {
        return 0.0;
    };
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
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

fn note_rss(peak: &mut f64, _phase: &str) {
    let mb = rss_mb();
    if mb > *peak {
        *peak = mb;
    }
}

fn note_phase(peak: &mut f64, phase: &str, started: Instant) -> f64 {
    note_rss(peak, phase);
    started.elapsed().as_secs_f64() * 1000.0
}

/// Split a region bbox into a lat/lon grid. Returns logical tile bboxes (no pad).
fn tile_grid(region: [f64; 4], max_cell_deg: f64) -> Vec<(usize, usize, [f64; 4])> {
    let lat_span = (region[2] - region[0]).max(1e-6);
    let lon_span = (region[3] - region[1]).max(1e-6);
    let rows = ((lat_span / max_cell_deg).ceil() as usize).max(1);
    let cols = ((lon_span / max_cell_deg).ceil() as usize).max(1);
    let dlat = lat_span / rows as f64;
    let dlon = lon_span / cols as f64;
    let mut out = Vec::with_capacity(rows * cols);
    for r in 0..rows {
        for c in 0..cols {
            let min_lat = region[0] + r as f64 * dlat;
            let min_lon = region[1] + c as f64 * dlon;
            let max_lat = if r + 1 == rows {
                region[2]
            } else {
                min_lat + dlat
            };
            let max_lon = if c + 1 == cols {
                region[3]
            } else {
                min_lon + dlon
            };
            out.push((r, c, [min_lat, min_lon, max_lat, max_lon]));
        }
    }
    out
}

fn region_needs_tiling(region: [f64; 4]) -> bool {
    let lat_span = region[2] - region[0];
    let lon_span = region[3] - region[1];
    // Corridor extracts stay monolithic; Østlandet-class spans must tile.
    lat_span > 1.25 || lon_span > 1.25 || lat_span * lon_span > 2.0
}

fn write_graph_pack(
    path: &Path,
    graph: &RouteGraph,
    elev: Option<&ElevationService>,
    peak: &mut f64,
) -> anyhow::Result<()> {
    note_rss(peak, "pack_build");
    let pack = FlatGraphPack::from_route_graph(graph, elev);
    note_rss(peak, "pack_ready");
    let payload = rkyv::to_bytes::<RkyvError>(&pack)
        .map_err(|e| anyhow::anyhow!("rkyv graph serialize: {e}"))?;
    drop(pack);
    note_rss(peak, "rkyv_done");
    discard_partial(path);
    write_archive_atomic(
        path,
        Preamble::new(MAGIC_GRAPH, GRAPH_FORMAT_VERSION),
        payload.as_ref(),
    )?;
    drop(payload);
    note_rss(peak, "graph_written");
    Ok(())
}

/// Convert a region PBF into indexed packs under `data_dir`.
///
/// Large extracts are written as spatial graph tiles so peak RAM stays within
/// 4GB-class device budgets. Cancelling mid-write deletes the current `.partial`
/// and leaves prior good archives untouched.
pub fn convert_region_packs(opts: &ConvertOptions) -> anyhow::Result<ConvertReport> {
    let _ch = crate::download::progress::ChannelGuard::enter(
        crate::download::progress::ProgressChannel::Convert,
    );
    let _bg = crate::download::pbf_priority::BackgroundIndexerGuard::enter();
    let t0 = Instant::now();
    let mut peak_rss_mb = rss_mb();
    let stem = stem_of(&opts.pbf);
    let (pbf_sz, pbf_mtime) = pbf_fingerprint(&opts.pbf)?;
    let pbf_filename = opts
        .pbf
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("region.osm.pbf")
        .to_string();

    download_progress::set(0, Some(5), "Building indexed maps: scanning bounds…");
    let t_bbox = Instant::now();
    let region_bbox = pbf_node_bbox(&opts.pbf)?;
    let bbox_scan_ms = note_phase(&mut peak_rss_mb, "bounds", t_bbox);

    // Never warm the full-region DEM into RAM (echoes the earlier DEM-downloader
    // fully-buffered OOM). Sample on demand only; skip Δh entirely when tiling
    // so convert peak stays dominated by one tile graph.
    let use_tiles = region_needs_tiling(region_bbox);
    let elev_svc;
    let elev_ref = if !use_tiles {
        if let Some(dir) = &opts.elev_dir {
            elev_svc = ElevationService::new(ElevationCache::new(dir.clone()));
            // Deliberately no warm_bbox(region) — load tiles as edges sample.
            Some(&elev_svc)
        } else {
            None
        }
    } else {
        None
    };

    let profiles = if opts.profiles.is_empty() {
        vec![RoutingProfile::Car, RoutingProfile::Foot]
    } else {
        opts.profiles.clone()
    };

    let mut graph_files: BTreeMap<String, String> = BTreeMap::new();
    let mut graph_tiles: BTreeMap<String, Vec<GraphTileEntry>> = BTreeMap::new();
    let mut graph_ms: BTreeMap<String, f64> = BTreeMap::new();
    let mut tile_assign_ms: BTreeMap<String, f64> = BTreeMap::new();
    let mut tile_build_ms: BTreeMap<String, f64> = BTreeMap::new();
    let mut nodes = 0usize;
    let mut edges = 0usize;
    let mut barrier_extra: Vec<(f64, f64, f64, f64)> = Vec::new();
    let mut tile_count = 0usize;

    // Truck shares car topology from bbox_build; alias files instead of a second build.
    let build_profiles: Vec<RoutingProfile> = profiles
        .iter()
        .copied()
        .filter(|p| *p != RoutingProfile::Truck)
        .collect();
    let want_truck = profiles.contains(&RoutingProfile::Truck);

    if use_tiles {
        // Drop prior graph packs (monolith or partial tiles) so a rebuild does
        // not mix layouts or leave half-written tiles from a crashed run.
        if let Ok(rd) = std::fs::read_dir(&opts.data_dir) {
            for ent in rd.flatten() {
                let name = ent.file_name();
                let Some(s) = name.to_str() else { continue };
                if s.starts_with(&format!("{stem}.navi-graph-")) && s.ends_with(".rkyv") {
                    let _ = std::fs::remove_file(ent.path());
                }
            }
        }
        // ~1° cells keep a single Ostlandet tile well under ~1 GB host RSS.
        let tiles = tile_grid(region_bbox, 1.0);
        // POI / barrier / wetland are extracted once below — not per profile.
        // Graph tiling: 2 PBF passes per profile with ways spilled to data_dir,
        // then per-tile graphs built+written in parallel (coords shared read-only).
        let total_steps = (build_profiles.len() + 3) as u64;
        let barrier_arc = Arc::new(Mutex::new(barrier_extra));
        {
            crate::download::pbf_priority::yield_if_foreground_plan();
            if opts.control.is_cancelled() {
                anyhow::bail!("cancelled");
            }
            download_progress::set(
                1,
                Some(total_steps),
                "Building indexed maps: graphs (shared PBF, tiled)…",
            );
            let t_graphs = Instant::now();
            let entries_by_profile: Arc<Mutex<BTreeMap<String, Vec<GraphTileEntry>>>> =
                Arc::new(Mutex::new(BTreeMap::new()));
            let max_nodes = Arc::new(AtomicUsize::new(nodes));
            let max_edges = Arc::new(AtomicUsize::new(edges));
            let peak = Arc::new(Mutex::new(peak_rss_mb));
            let barrier_cb = Arc::clone(&barrier_arc);
            let tile_count_atomic = Arc::new(AtomicUsize::new(tile_count));
            let data_dir = opts.data_dir.clone();
            let stem_s = stem.clone();
            let control = opts.control.clone();
            let entries_cb = Arc::clone(&entries_by_profile);
            let max_nodes_cb = Arc::clone(&max_nodes);
            let max_edges_cb = Arc::clone(&max_edges);
            let peak_cb = Arc::clone(&peak);
            let tile_count_cb = Arc::clone(&tile_count_atomic);
            let profile_results = RouteGraph::build_tiled_from_pbf_profiles(
                &opts.pbf,
                &build_profiles,
                &tiles,
                0.05,
                &opts.data_dir,
                move |profile, row, col, logical, graph| {
                    let key_s = profile_key(profile).to_string();
                    max_nodes_cb.fetch_max(graph.nodes.len(), Ordering::Relaxed);
                    max_edges_cb.fetch_max(graph.edges.len(), Ordering::Relaxed);
                    {
                        let mut p = peak_cb.lock().unwrap_or_else(|e| e.into_inner());
                        note_rss(&mut p, &format!("graph_{key_s}_{row}_{col}"));
                    }
                    if profile == RoutingProfile::Car {
                        barrier_cb
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .extend(highway_barrier_segs(&graph));
                    }
                    let name = graph_tile_filename(&stem_s, &key_s, row, col);
                    let path = data_dir.join(&name);
                    if control.is_cancelled() {
                        discard_partial(&path);
                        anyhow::bail!("cancelled");
                    }
                    {
                        let mut p = peak_cb.lock().unwrap_or_else(|e| e.into_inner());
                        write_graph_pack(&path, &graph, None, &mut p)?;
                    }
                    entries_cb
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .entry(key_s)
                        .or_default()
                        .push(GraphTileEntry {
                            file: name,
                            bbox: logical,
                        });
                    tile_count_cb.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                },
            )?;
            nodes = max_nodes.load(Ordering::Relaxed);
            edges = max_edges.load(Ordering::Relaxed);
            peak_rss_mb = *peak.lock().unwrap_or_else(|e| e.into_inner());
            tile_count = tile_count_atomic.load(Ordering::Relaxed);
            let mut all_entries = entries_by_profile.lock().unwrap_or_else(|e| e.into_inner());
            for (profile, _produced, tile_timings) in &profile_results {
                let key = profile_key(*profile).to_string();
                let mut profile_entries = all_entries.remove(&key).unwrap_or_default();
                profile_entries.sort_by(|a, b| a.file.cmp(&b.file));
                for e in &profile_entries {
                    graph_files.insert(e.file.clone(), e.file.clone());
                }
                graph_tiles.insert(key.clone(), profile_entries);
                tile_assign_ms.insert(key.clone(), tile_timings.tile_assign_ms);
                tile_build_ms.insert(key.clone(), tile_timings.tile_build_ms);
            }
            // Shared Pass1+Pass2 time is outside per-profile assign/build; attribute
            // residual wall to each profile's graph_ms for continuity with reports.
            let shared_wall = t_graphs.elapsed().as_secs_f64() * 1000.0;
            let assign_build_sum: f64 =
                tile_assign_ms.values().sum::<f64>() + tile_build_ms.values().sum::<f64>();
            let shared_pass_ms = (shared_wall - assign_build_sum).max(0.0);
            let n_prof = profile_results.len().max(1) as f64;
            for (profile, _, tile_timings) in &profile_results {
                let key = profile_key(*profile).to_string();
                graph_ms.insert(
                    key,
                    tile_timings.tile_assign_ms
                        + tile_timings.tile_build_ms
                        + shared_pass_ms / n_prof,
                );
            }
            note_phase(&mut peak_rss_mb, "graphs_shared_tiled", t_graphs);
        }
        barrier_extra = Arc::try_unwrap(barrier_arc)
            .map_err(|_| anyhow::anyhow!("barrier segments still shared"))?
            .into_inner()
            .map_err(|_| anyhow::anyhow!("barrier segments mutex poisoned"))?;
        if want_truck {
            if let Some(car_tiles) = graph_tiles.get("car").cloned() {
                graph_tiles.insert("truck".into(), car_tiles);
            }
        }
    } else {
        let total_steps = (build_profiles.len() + 3) as u64;
        for (i, profile) in build_profiles.iter().enumerate() {
            crate::download::pbf_priority::yield_if_foreground_plan();
            if opts.control.is_cancelled() {
                anyhow::bail!("cancelled");
            }
            let key = profile_key(*profile).to_string();
            download_progress::set(
                (i + 1) as u64,
                Some(total_steps),
                &format!("Building indexed maps: graph ({key})…"),
            );
            let t_graph = Instant::now();
            let graph = RouteGraph::build_from_pbf_bbox(&opts.pbf, *profile, region_bbox)?;
            nodes = graph.nodes.len().max(nodes);
            edges = graph.edges.len().max(edges);
            graph_ms.insert(
                key.clone(),
                note_phase(&mut peak_rss_mb, &format!("graph_{key}"), t_graph),
            );
            if *profile == RoutingProfile::Car {
                barrier_extra = highway_barrier_segs(&graph);
            }
            let name = graph_pack_filename(&stem, &key);
            let path = opts.data_dir.join(&name);
            if opts.control.is_cancelled() {
                discard_partial(&path);
                anyhow::bail!("cancelled");
            }
            write_graph_pack(&path, &graph, elev_ref, &mut peak_rss_mb)?;
            drop(graph);
            graph_files.insert(key, name);
        }
        if want_truck {
            if let Some(car_name) = graph_files.get("car").cloned() {
                let truck_name = graph_pack_filename(&stem, "truck");
                let src = opts.data_dir.join(&car_name);
                let dst = opts.data_dir.join(&truck_name);
                let _ = std::fs::remove_file(&dst);
                std::fs::hard_link(&src, &dst)
                    .or_else(|_| std::fs::copy(&src, &dst).map(|_| ()))?;
                graph_files.insert("truck".into(), truck_name);
            }
        }
    }

    download_progress::set(90, Some(100), "Building indexed maps: POI + barriers…");
    crate::download::pbf_priority::yield_if_foreground_plan();
    note_rss(&mut peak_rss_mb, "poi_start");
    let t_poi = Instant::now();
    let (records, overnight_buildings) = collect_poi_records(&opts.pbf, region_bbox)?;
    let poi_ms = note_phase(&mut peak_rss_mb, "poi_collect", t_poi);
    let overnight_ms = 0.0;
    let t_barrier = Instant::now();
    let (mut segs, glaciers) = extract_pbf_barrier_geometry(&opts.pbf)?;
    segs.extend(barrier_extra);
    let barrier_ms = note_phase(&mut peak_rss_mb, "barrier", t_barrier);
    let overnight_building_count = overnight_buildings.len();
    let poi_pack = FlatPoiBarrierPack::from_parts(&records, &segs, &glaciers, &overnight_buildings);
    drop(overnight_buildings);
    let poi_payload = rkyv::to_bytes::<RkyvError>(&poi_pack)
        .map_err(|e| anyhow::anyhow!("rkyv poi serialize: {e}"))?;
    drop(poi_pack);
    let poi_name = poi_barrier_pack_filename(&stem);
    let poi_path = opts.data_dir.join(&poi_name);
    discard_partial(&poi_path);
    if opts.control.is_cancelled() {
        discard_partial(&poi_path);
        anyhow::bail!("cancelled");
    }
    write_archive_atomic(
        &poi_path,
        Preamble::new(MAGIC_POI_BARRIER, POI_BARRIER_FORMAT_VERSION),
        poi_payload.as_ref(),
    )?;
    drop(poi_payload);
    note_rss(&mut peak_rss_mb, "poi_done");

    download_progress::set(95, Some(100), "Building indexed maps: wetlands…");
    crate::download::pbf_priority::yield_if_foreground_plan();
    let t_wetland = Instant::now();
    // Clear prior wetland packs (monolith or tiles) so rebuilds do not leave gaps.
    if let Ok(rd) = std::fs::read_dir(&opts.data_dir) {
        for ent in rd.flatten() {
            let name = ent.file_name();
            let Some(s) = name.to_str() else { continue };
            if s.starts_with(&format!("{stem}.navi-wetland")) && s.ends_with(".rkyv") {
                let _ = std::fs::remove_file(ent.path());
            }
        }
    }
    let wet_name = wetland_pack_filename(&stem);
    let mut wetland_tiles_out: Vec<GraphTileEntry> = Vec::new();
    let wetland_rings = if use_tiles {
        // Shared way/coord extract once; emit one tile pack at a time so peak
        // ring RAM stays tile-sized (closes the old full-region OOM skip).
        let tiles = tile_grid(region_bbox, 1.0);
        match crate::routing::wetland::WetlandWayExtract::load(&opts.pbf) {
            Ok(extract) => {
                note_rss(&mut peak_rss_mb, "wetland_extract");
                // Single walk over wetland rings; assign each ring to every
                // tile that contains at least one vertex (same rule as the
                // former per-tile index_for_bbox rewalk).
                let per_tile = extract.indexes_for_tiles(&tiles);
                drop(extract);
                let mut rings_total = 0usize;
                for ((row, col, logical), idx) in tiles.iter().zip(per_tile.into_iter()) {
                    if opts.control.is_cancelled() {
                        anyhow::bail!("cancelled");
                    }
                    let n = idx.ring_count();
                    if n == 0 {
                        continue;
                    }
                    rings_total += n;
                    let wet_pack = FlatWetlandPack::from_wetland_index(&idx);
                    drop(idx);
                    let name = wetland_tile_filename(&stem, *row, *col);
                    let wet_path = opts.data_dir.join(&name);
                    discard_partial(&wet_path);
                    match rkyv::to_bytes::<RkyvError>(&wet_pack) {
                        Ok(wet_payload) => {
                            drop(wet_pack);
                            write_archive_atomic(
                                &wet_path,
                                Preamble::new(MAGIC_WETLAND, WETLAND_FORMAT_VERSION),
                                wet_payload.as_ref(),
                            )?;
                            wetland_tiles_out.push(GraphTileEntry {
                                file: name,
                                bbox: *logical,
                            });
                            note_rss(&mut peak_rss_mb, &format!("wetland_tile_{row}_{col}"));
                        }
                        Err(e) => {
                            log::warn!("wetland tile {row}_{col} serialize skipped: {e}");
                        }
                    }
                }
                note_rss(&mut peak_rss_mb, "wetland_done");
                rings_total
            }
            Err(e) => {
                log::warn!("wetland extract skipped: {e:#}");
                0
            }
        }
    } else {
        match WetlandIndex::load_from_pbf(&opts.pbf) {
            Ok(wetlands) => {
                let rings = wetlands.ring_count();
                note_rss(&mut peak_rss_mb, "wetland_loaded");
                let wet_pack = FlatWetlandPack::from_wetland_index(&wetlands);
                drop(wetlands);
                match rkyv::to_bytes::<RkyvError>(&wet_pack) {
                    Ok(wet_payload) => {
                        drop(wet_pack);
                        let wet_path = opts.data_dir.join(&wet_name);
                        discard_partial(&wet_path);
                        if opts.control.is_cancelled() {
                            discard_partial(&wet_path);
                            anyhow::bail!("cancelled");
                        }
                        write_archive_atomic(
                            &wet_path,
                            Preamble::new(MAGIC_WETLAND, WETLAND_FORMAT_VERSION),
                            wet_payload.as_ref(),
                        )?;
                        drop(wet_payload);
                        note_rss(&mut peak_rss_mb, "wetland_done");
                        rings
                    }
                    Err(e) => {
                        log::warn!("wetland serialize skipped: {e}");
                        0
                    }
                }
            }
            Err(e) => {
                log::warn!("wetland extract skipped: {e:#}");
                0
            }
        }
    };
    let wetland_ms = note_phase(&mut peak_rss_mb, "wetland_done", t_wetland);
    let (wetland_file, wetland_format_version) = if !wetland_tiles_out.is_empty() {
        (None, WETLAND_FORMAT_VERSION)
    } else if opts.data_dir.join(&wet_name).is_file() {
        (Some(wet_name.clone()), WETLAND_FORMAT_VERSION)
    } else {
        (None, 0)
    };
    log::info!(
        "indexed convert overnight_buildings={overnight_building_count} wetland_rings={wetland_rings} wetland_tiles={} bbox_scan_ms={bbox_scan_ms:.1} graph_ms={graph_ms:?} poi_ms={poi_ms:.1} barrier_ms={barrier_ms:.1} overnight_ms={overnight_ms:.1} wetland_ms={wetland_ms:.1}",
        wetland_tiles_out.len()
    );

    let mut all_graph_names: Vec<String> = graph_files.values().cloned().collect();
    for tiles in graph_tiles.values() {
        for t in tiles {
            all_graph_names.push(t.file.clone());
        }
    }
    all_graph_names.sort();
    all_graph_names.dedup();

    let manifest = NaviManifest {
        schema: NaviManifest::SCHEMA,
        stem: stem.clone(),
        pbf_filename,
        pbf_size_bytes: pbf_sz,
        pbf_modified_unix_secs: pbf_mtime,
        graph_files,
        graph_tiles,
        graph_format_version: GRAPH_FORMAT_VERSION,
        poi_barrier_file: poi_name.clone(),
        poi_barrier_format_version: POI_BARRIER_FORMAT_VERSION,
        wetland_file: wetland_file.clone(),
        wetland_tiles: wetland_tiles_out,
        wetland_format_version,
        has_delta_h: elev_ref.is_some(),
        elev_dir: if elev_ref.is_some() {
            opts.elev_dir
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
        } else {
            None
        },
    };
    let man_path = manifest_path(&opts.data_dir, &stem);
    manifest.save(&man_path)?;

    download_progress::set(100, Some(100), "Indexed maps ready");
    note_rss(&mut peak_rss_mb, "done");

    Ok(ConvertReport {
        stem,
        graph_files: all_graph_names,
        poi_barrier_file: poi_name,
        wetland_file: wetland_file.unwrap_or(wet_name),
        manifest_file: man_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("manifest")
            .to_string(),
        nodes,
        edges,
        pois: records.len(),
        barrier_segs: segs.len(),
        wetland_rings,
        convert_ms: t0.elapsed().as_secs_f64() * 1000.0,
        has_delta_h: elev_ref.is_some(),
        peak_rss_mb,
        graph_tiles: tile_count,
        bbox_scan_ms,
        graph_ms,
        tile_assign_ms,
        tile_build_ms,
        poi_ms,
        barrier_ms,
        overnight_ms,
        wetland_ms,
    })
}
