//! Region pack converter: local PBF (+ optional DEM) → graph + poi/barrier archives.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rkyv::rancor::Error as RkyvError;
use serde::{Deserialize, Serialize};

use super::graph_pack::{FlatGraphPack, GRAPH_FORMAT_VERSION, MAGIC_GRAPH};
use super::header::Preamble;
use super::io::{archive_matches_preamble, discard_partial, write_archive_atomic};
use super::manifest::{
    graph_pack_filename, graph_tile_filename, manifest_path, pbf_fingerprint,
    poi_barrier_pack_filename, profile_key, wetland_pack_filename, wetland_tile_filename,
    GraphTileEntry, NaviManifest,
};
use super::poi_barrier_extract::extract_poi_and_pbf_barriers;
use super::poi_barrier_pack::{FlatPoiBarrierPack, MAGIC_POI_BARRIER, POI_BARRIER_FORMAT_VERSION};
use super::wetland_pack::{FlatWetlandPack, MAGIC_WETLAND, WETLAND_FORMAT_VERSION};
use crate::download::progress as download_progress;
use crate::download::DownloadControl;
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
    /// Wall time for the shared POI + PBF-barrier 2-pass walk (plus centroids).
    pub poi_ms: f64,
    /// Wall time for barrier ring assembly + highway/trunk extras (PBF scans are in `poi_ms`).
    pub barrier_ms: f64,
    /// Wall time for overnight-building detection (0 when folded into POI).
    pub overnight_ms: f64,
    /// Wall time for wetland extract + pack write.
    pub wetland_ms: f64,
}

fn convert_checkpoint_path(data_dir: &Path, stem: &str) -> PathBuf {
    data_dir.join(format!("{stem}.navi-convert-progress.json"))
}

/// Durable convert progress so a force-stop can skip completed archives.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ConvertCheckpoint {
    pbf_filename: String,
    pbf_size_bytes: u64,
    pbf_modified_unix_secs: u64,
    graph_format_version: u32,
    poi_barrier_format_version: u32,
    wetland_format_version: u32,
    region_bbox: [f64; 4],
    graphs_complete: bool,
    graph_files: BTreeMap<String, String>,
    graph_tiles: BTreeMap<String, Vec<GraphTileEntry>>,
    nodes: usize,
    edges: usize,
    poi_complete: bool,
    poi_file: Option<String>,
    pois: usize,
    barrier_segs: usize,
    wetland_complete: bool,
    wetland_file: Option<String>,
    wetland_tiles: Vec<GraphTileEntry>,
    wetland_rings: usize,
}

impl ConvertCheckpoint {
    fn matches_pbf(&self, filename: &str, sz: u64, mtime: u64) -> bool {
        self.pbf_filename == filename
            && self.pbf_size_bytes == sz
            && self.pbf_modified_unix_secs == mtime
            && self.graph_format_version == GRAPH_FORMAT_VERSION
            && self.poi_barrier_format_version == POI_BARRIER_FORMAT_VERSION
            && self.wetland_format_version == WETLAND_FORMAT_VERSION
    }

    fn load(path: &Path) -> Option<Self> {
        let text = fs::read_to_string(path).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)?;
        let tmp = path.with_extension("json.partial");
        fs::write(&tmp, text)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    fn graph_files_present(&self, data_dir: &Path) -> bool {
        if !self.graphs_complete {
            return false;
        }
        let mut any = false;
        for tiles in self.graph_tiles.values() {
            for e in tiles {
                any = true;
                if !archive_matches_preamble(
                    &data_dir.join(&e.file),
                    MAGIC_GRAPH,
                    GRAPH_FORMAT_VERSION,
                ) {
                    return false;
                }
            }
        }
        for name in self.graph_files.values() {
            any = true;
            if !archive_matches_preamble(&data_dir.join(name), MAGIC_GRAPH, GRAPH_FORMAT_VERSION) {
                return false;
            }
        }
        any
    }

    fn poi_present(&self, data_dir: &Path) -> bool {
        let Some(name) = &self.poi_file else {
            return false;
        };
        self.poi_complete
            && archive_matches_preamble(
                &data_dir.join(name),
                MAGIC_POI_BARRIER,
                POI_BARRIER_FORMAT_VERSION,
            )
    }

    fn wetland_present(&self, data_dir: &Path) -> bool {
        if !self.wetland_complete {
            return false;
        }
        if let Some(name) = &self.wetland_file {
            return archive_matches_preamble(
                &data_dir.join(name),
                MAGIC_WETLAND,
                WETLAND_FORMAT_VERSION,
            );
        }
        if self.wetland_tiles.is_empty() {
            return true;
        }
        self.wetland_tiles.iter().all(|e| {
            archive_matches_preamble(
                &data_dir.join(&e.file),
                MAGIC_WETLAND,
                WETLAND_FORMAT_VERSION,
            )
        })
    }
}

fn skip_tiles_from_checkpoint(
    ck: &ConvertCheckpoint,
    data_dir: &Path,
) -> HashSet<(RoutingProfile, usize, usize)> {
    let mut out = HashSet::new();
    for (key, tiles) in &ck.graph_tiles {
        let Some(profile) = profile_from_key(key) else {
            continue;
        };
        for e in tiles {
            if !archive_matches_preamble(&data_dir.join(&e.file), MAGIC_GRAPH, GRAPH_FORMAT_VERSION)
            {
                continue;
            }
            if let Some((row, col)) = parse_graph_tile_rc(&e.file) {
                out.insert((profile, row, col));
            }
        }
    }
    out
}

fn parse_graph_tile_rc(file: &str) -> Option<(usize, usize)> {
    let rest = file.rsplit_once(".t")?.1;
    let rest = rest.strip_suffix(".rkyv")?;
    let (r, c) = rest.split_once('_')?;
    Some((r.parse().ok()?, c.parse().ok()?))
}

fn profile_from_key(key: &str) -> Option<RoutingProfile> {
    match key {
        "car" => Some(RoutingProfile::Car),
        "truck" => Some(RoutingProfile::Truck),
        "foot" => Some(RoutingProfile::Foot),
        "bicycle" => Some(RoutingProfile::Bicycle),
        _ => None,
    }
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

    let ck_path = convert_checkpoint_path(&opts.data_dir, &stem);
    let loaded_ck = ConvertCheckpoint::load(&ck_path)
        .filter(|c| c.matches_pbf(&pbf_filename, pbf_sz, pbf_mtime));
    if loaded_ck.is_none() {
        let _ = fs::remove_file(&ck_path);
    }

    let t_bbox = Instant::now();
    let (region_bbox, bbox_scan_ms) = if let Some(c) = &loaded_ck {
        download_progress::set(0, Some(5), "Resuming indexed maps: using saved bounds…");
        (c.region_bbox, 0.0)
    } else {
        download_progress::set(0, Some(5), "Building indexed maps: scanning bounds…");
        let bbox = pbf_node_bbox(&opts.pbf)?;
        (bbox, note_phase(&mut peak_rss_mb, "bounds", t_bbox))
    };

    let mut ck = loaded_ck.unwrap_or_else(|| ConvertCheckpoint {
        pbf_filename: pbf_filename.clone(),
        pbf_size_bytes: pbf_sz,
        pbf_modified_unix_secs: pbf_mtime,
        graph_format_version: GRAPH_FORMAT_VERSION,
        poi_barrier_format_version: POI_BARRIER_FORMAT_VERSION,
        wetland_format_version: WETLAND_FORMAT_VERSION,
        region_bbox,
        ..Default::default()
    });
    ck.region_bbox = region_bbox;
    let _ = ck.save(&ck_path);

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
        // Drop graph packs that are not resumable current-version tiles for this
        // PBF. A matching convert checkpoint keeps completed tiles; otherwise a
        // rebuild must not mix layouts or leftover files from a crashed run.
        if let Ok(rd) = std::fs::read_dir(&opts.data_dir) {
            for ent in rd.flatten() {
                let name = ent.file_name();
                let Some(s) = name.to_str() else { continue };
                if !(s.starts_with(&format!("{stem}.navi-graph-")) && s.ends_with(".rkyv")) {
                    continue;
                }
                let listed = ck
                    .graph_tiles
                    .values()
                    .any(|tiles| tiles.iter().any(|t| t.file == s))
                    || ck.graph_files.values().any(|n| n == s);
                let keep = listed
                    && archive_matches_preamble(&ent.path(), MAGIC_GRAPH, GRAPH_FORMAT_VERSION);
                if !keep {
                    let _ = fs::remove_file(ent.path());
                }
            }
        }
        // ~1° cells keep a single Ostlandet tile well under ~1 GB host RSS.
        let tiles = tile_grid(region_bbox, 1.0);
        let total_steps = (build_profiles.len() + 3) as u64;
        if ck.graph_files_present(&opts.data_dir) {
            download_progress::set(
                1,
                Some(total_steps),
                "Resuming indexed maps: graphs already on disk…",
            );
            log::info!(
                target: "NaviConvert",
                "CONVERT_PHASE resume skip graphs (checkpoint complete)"
            );
            graph_files = ck.graph_files.clone();
            graph_tiles = ck.graph_tiles.clone();
            nodes = ck.nodes;
            edges = ck.edges;
            tile_count = graph_tiles.values().map(|v| v.len()).sum();
        } else {
            // POI / barrier / wetland are extracted once below — not per profile.
            // Graph tiling: 2 PBF passes per profile with ways spilled to data_dir,
            // then per-tile graphs built+written in parallel (coords shared read-only).
            let skip_tiles = skip_tiles_from_checkpoint(&ck, &opts.data_dir);
            if !skip_tiles.is_empty() {
                download_progress::set(
                    1,
                    Some(total_steps),
                    "Resuming indexed maps: graphs (skipping finished tiles)…",
                );
            }
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
                    Arc::new(Mutex::new(ck.graph_tiles.clone()));
                let max_nodes = Arc::new(AtomicUsize::new(nodes.max(ck.nodes)));
                let max_edges = Arc::new(AtomicUsize::new(edges.max(ck.edges)));
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
                let ck_save = Arc::new(Mutex::new(ck.clone()));
                let ck_path_cb = ck_path.clone();
                let profile_results = RouteGraph::build_tiled_from_pbf_profiles(
                    &opts.pbf,
                    &build_profiles,
                    &tiles,
                    0.05,
                    &opts.data_dir,
                    &skip_tiles,
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
                        let entry = GraphTileEntry {
                            file: name,
                            bbox: logical,
                        };
                        entries_cb
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .entry(key_s.clone())
                            .or_default()
                            .push(entry.clone());
                        {
                            let mut c = ck_save.lock().unwrap_or_else(|e| e.into_inner());
                            let list = c.graph_tiles.entry(key_s).or_default();
                            if !list.iter().any(|e| e.file == entry.file) {
                                list.push(entry);
                            }
                            c.nodes = c.nodes.max(graph.nodes.len());
                            c.edges = c.edges.max(graph.edges.len());
                            let _ = c.save(&ck_path_cb);
                        }
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
                    profile_entries.dedup_by(|a, b| a.file == b.file);
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
            ck.graphs_complete = true;
            ck.graph_files = graph_files.clone();
            ck.graph_tiles = graph_tiles.clone();
            ck.nodes = nodes;
            ck.edges = edges;
            let _ = ck.save(&ck_path);
        }
        if want_truck {
            if let Some(car_tiles) = graph_tiles.get("car").cloned() {
                graph_tiles.insert("truck".into(), car_tiles);
            }
        }
    } else {
        let total_steps = (build_profiles.len() + 3) as u64;
        if ck.graph_files_present(&opts.data_dir) {
            download_progress::set(
                1,
                Some(total_steps),
                "Resuming indexed maps: graphs already on disk…",
            );
            log::info!(
                target: "NaviConvert",
                "CONVERT_PHASE resume skip graphs (checkpoint complete, monolith)"
            );
            graph_files = ck.graph_files.clone();
            nodes = ck.nodes;
            edges = ck.edges;
        } else {
            for (i, profile) in build_profiles.iter().enumerate() {
                crate::download::pbf_priority::yield_if_foreground_plan();
                if opts.control.is_cancelled() {
                    anyhow::bail!("cancelled");
                }
                let key = profile_key(*profile).to_string();
                let name = graph_pack_filename(&stem, &key);
                let path = opts.data_dir.join(&name);
                if archive_matches_preamble(&path, MAGIC_GRAPH, GRAPH_FORMAT_VERSION)
                    && ck.graph_files.get(&key).is_some_and(|n| n == &name)
                {
                    download_progress::set(
                        (i + 1) as u64,
                        Some(total_steps),
                        &format!("Resuming indexed maps: graph ({key}) already on disk…"),
                    );
                    graph_files.insert(key, name);
                    continue;
                }
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
                if opts.control.is_cancelled() {
                    discard_partial(&path);
                    anyhow::bail!("cancelled");
                }
                write_graph_pack(&path, &graph, elev_ref, &mut peak_rss_mb)?;
                drop(graph);
                graph_files.insert(key.clone(), name);
                ck.graph_files = graph_files.clone();
                ck.nodes = nodes;
                ck.edges = edges;
                let _ = ck.save(&ck_path);
            }
            ck.graphs_complete = true;
            ck.graph_files = graph_files.clone();
            ck.nodes = nodes;
            ck.edges = edges;
            let _ = ck.save(&ck_path);
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

    let poi_name = poi_barrier_pack_filename(&stem);
    let mut pois = ck.pois;
    let mut barrier_seg_count = ck.barrier_segs;
    let overnight_ms = 0.0;
    let (poi_ms, barrier_ms, overnight_building_count) = if ck.poi_present(&opts.data_dir) {
        download_progress::set(
            90,
            Some(100),
            "Resuming indexed maps: POI + barriers already on disk…",
        );
        log::info!(
            target: "NaviConvert",
            "CONVERT_PHASE resume skip POI (checkpoint complete)"
        );
        (0.0, 0.0, 0usize)
    } else {
        download_progress::set(90, Some(100), "Building indexed maps: POI + barriers…");
        crate::download::pbf_priority::yield_if_foreground_plan();
        note_rss(&mut peak_rss_mb, "poi_start");
        let t_poi = Instant::now();
        let (records, overnight_buildings, mut segs, glaciers) =
            extract_poi_and_pbf_barriers(&opts.pbf, region_bbox)?;
        let poi_ms = note_phase(&mut peak_rss_mb, "poi_barrier_shared", t_poi);
        let t_barrier = Instant::now();
        segs.extend(barrier_extra);
        let barrier_ms = note_phase(&mut peak_rss_mb, "barrier_highway_extra", t_barrier);
        let overnight_building_count = overnight_buildings.len();
        let poi_pack =
            FlatPoiBarrierPack::from_parts(&records, &segs, &glaciers, &overnight_buildings);
        drop(overnight_buildings);
        let poi_payload = rkyv::to_bytes::<RkyvError>(&poi_pack)
            .map_err(|e| anyhow::anyhow!("rkyv poi serialize: {e}"))?;
        drop(poi_pack);
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
        pois = records.len();
        barrier_seg_count = segs.len();
        ck.poi_complete = true;
        ck.poi_file = Some(poi_name.clone());
        ck.pois = pois;
        ck.barrier_segs = barrier_seg_count;
        let _ = ck.save(&ck_path);
        (poi_ms, barrier_ms, overnight_building_count)
    };

    let wet_name = wetland_pack_filename(&stem);
    let t_wetland = Instant::now();
    let (wetland_tiles_out, wetland_rings, wetland_ms) = if ck.wetland_present(&opts.data_dir) {
        download_progress::set(
            95,
            Some(100),
            "Resuming indexed maps: wetlands already on disk…",
        );
        log::info!(
            target: "NaviConvert",
            "CONVERT_PHASE resume skip wetlands (checkpoint complete)"
        );
        (ck.wetland_tiles.clone(), ck.wetland_rings, 0.0)
    } else {
        download_progress::set(95, Some(100), "Building indexed maps: wetlands…");
        crate::download::pbf_priority::yield_if_foreground_plan();
        if let Ok(rd) = std::fs::read_dir(&opts.data_dir) {
            for ent in rd.flatten() {
                let name = ent.file_name();
                let Some(s) = name.to_str() else { continue };
                if !(s.starts_with(&format!("{stem}.navi-wetland")) && s.ends_with(".rkyv")) {
                    continue;
                }
                let listed = ck.wetland_tiles.iter().any(|t| t.file == s)
                    || ck.wetland_file.as_deref() == Some(s);
                let keep = listed
                    && archive_matches_preamble(&ent.path(), MAGIC_WETLAND, WETLAND_FORMAT_VERSION);
                if !keep {
                    let _ = fs::remove_file(ent.path());
                }
            }
        }
        let mut wetland_tiles_out: Vec<GraphTileEntry> = ck.wetland_tiles.clone();
        let wetland_rings = if use_tiles {
            let tiles = tile_grid(region_bbox, 1.0);
            match crate::routing::wetland::WetlandWayExtract::load(&opts.pbf) {
                Ok(extract) => {
                    note_rss(&mut peak_rss_mb, "wetland_extract");
                    let per_tile = extract.indexes_for_tiles(&tiles);
                    drop(extract);
                    let mut rings_total = 0usize;
                    for ((row, col, logical), idx) in tiles.iter().zip(per_tile) {
                        if opts.control.is_cancelled() {
                            anyhow::bail!("cancelled");
                        }
                        let n = idx.ring_count();
                        if n == 0 {
                            continue;
                        }
                        rings_total += n;
                        let name = wetland_tile_filename(&stem, *row, *col);
                        let wet_path = opts.data_dir.join(&name);
                        if archive_matches_preamble(
                            &wet_path,
                            MAGIC_WETLAND,
                            WETLAND_FORMAT_VERSION,
                        ) {
                            if !wetland_tiles_out.iter().any(|e| e.file == name) {
                                wetland_tiles_out.push(GraphTileEntry {
                                    file: name,
                                    bbox: *logical,
                                });
                            }
                            continue;
                        }
                        let wet_pack = FlatWetlandPack::from_wetland_index(&idx);
                        drop(idx);
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
                                ck.wetland_tiles = wetland_tiles_out.clone();
                                let _ = ck.save(&ck_path);
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
        ck.wetland_complete = true;
        ck.wetland_tiles = wetland_tiles_out.clone();
        ck.wetland_rings = wetland_rings;
        let _ = ck.save(&ck_path);
        (wetland_tiles_out, wetland_rings, wetland_ms)
    };
    let (wetland_file, wetland_format_version) = if !wetland_tiles_out.is_empty() {
        (None, WETLAND_FORMAT_VERSION)
    } else if opts.data_dir.join(&wet_name).is_file() {
        (Some(wet_name.clone()), WETLAND_FORMAT_VERSION)
    } else {
        (None, 0)
    };
    ck.wetland_file = wetland_file.clone();
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
    let _ = fs::remove_file(&ck_path);

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
        pois,
        barrier_segs: barrier_seg_count,
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

#[cfg(test)]
mod resume_tests {
    use super::*;
    use crate::routing::indexed::manifest::manifest_path;

    fn fixture_pbf() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stai-bru-limits.osm.pbf")
    }

    #[test]
    fn resume_skips_complete_graphs_when_checkpoint_matches() {
        let src = fixture_pbf();
        assert!(src.is_file(), "missing {}", src.display());
        let work = tempfile::tempdir().expect("tempdir");
        let data_dir = work.path();
        let pbf = data_dir.join("stai-bru-limits.osm.pbf");
        fs::copy(&src, &pbf).expect("copy pbf");

        let mut opts = ConvertOptions::new(data_dir, &pbf);
        opts.profiles = vec![RoutingProfile::Car, RoutingProfile::Foot];
        convert_region_packs(&opts).expect("initial convert");

        let stem = "stai-bru-limits";
        let graph_name = graph_pack_filename(stem, "car");
        let graph_path = data_dir.join(&graph_name);
        assert!(graph_path.is_file());
        let before_meta = fs::metadata(&graph_path).expect("meta");
        let before_mtime = before_meta.modified().ok();
        let before_len = before_meta.len();

        let man = NaviManifest::load(&manifest_path(data_dir, stem)).expect("manifest");
        let poi_name = poi_barrier_pack_filename(stem);
        let _ = fs::remove_file(manifest_path(data_dir, stem));
        let _ = fs::remove_file(data_dir.join(&poi_name));
        for ent in fs::read_dir(data_dir).unwrap().flatten() {
            let n = ent.file_name();
            let s = n.to_string_lossy();
            if s.contains(".navi-wetland") {
                let _ = fs::remove_file(ent.path());
            }
        }

        let ck = ConvertCheckpoint {
            pbf_filename: man.pbf_filename.clone(),
            pbf_size_bytes: man.pbf_size_bytes,
            pbf_modified_unix_secs: man.pbf_modified_unix_secs,
            graph_format_version: GRAPH_FORMAT_VERSION,
            poi_barrier_format_version: POI_BARRIER_FORMAT_VERSION,
            wetland_format_version: WETLAND_FORMAT_VERSION,
            region_bbox: [61.2, 11.0, 61.4, 11.3],
            graphs_complete: true,
            graph_files: man.graph_files.clone(),
            graph_tiles: man.graph_tiles.clone(),
            nodes: 1,
            edges: 1,
            ..Default::default()
        };
        ck.save(&convert_checkpoint_path(data_dir, stem))
            .expect("plant checkpoint");

        convert_region_packs(&opts).expect("resume convert");

        let after = fs::metadata(&graph_path).expect("meta after");
        assert_eq!(
            after.len(),
            before_len,
            "resume must not rewrite graph pack"
        );
        if let (Some(a), Some(b)) = (after.modified().ok(), before_mtime) {
            assert_eq!(a, b, "resume must keep original graph mtime");
        }
        assert!(
            data_dir.join(&poi_name).is_file(),
            "POI pack must be rebuilt after resume"
        );
        assert!(
            manifest_path(data_dir, stem).is_file(),
            "manifest must be rewritten"
        );
        assert!(
            !convert_checkpoint_path(data_dir, stem).exists(),
            "checkpoint must be removed after success"
        );
    }

    #[test]
    fn parse_graph_tile_rc_reads_row_col() {
        assert_eq!(
            parse_graph_tile_rc("ostlandet-latest.navi-graph-car.t2_5.rkyv"),
            Some((2, 5))
        );
        assert_eq!(parse_graph_tile_rc("no-tile.rkyv"), None);
    }
}
