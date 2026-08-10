//! Region pack converter: local PBF (+ optional DEM) → graph + poi/barrier archives.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use osmpbf::{Element, ElementReader};
use rkyv::rancor::Error as RkyvError;

use super::graph_pack::{FlatGraphPack, GRAPH_FORMAT_VERSION, MAGIC_GRAPH};
use super::header::Preamble;
use super::io::{discard_partial, write_archive_atomic};
use super::manifest::{
    graph_pack_filename, graph_tile_filename, manifest_path, pbf_fingerprint,
    poi_barrier_pack_filename, profile_key, wetland_pack_filename, GraphTileEntry, NaviManifest,
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
/// Uses 0.5–99.5 percentiles over a reservoir sample so a few garbage OSM
/// coordinates do not inflate tiling into hundreds of empty cells.
fn pbf_node_bbox(pbf: &Path) -> anyhow::Result<[f64; 4]> {
    const SAMPLE_CAP: usize = 250_000;
    let mut lats: Vec<f64> = Vec::with_capacity(SAMPLE_CAP);
    let mut lons: Vec<f64> = Vec::with_capacity(SAMPLE_CAP);
    let mut seen: u64 = 0;
    let file = std::fs::File::open(pbf)?;
    let reader = ElementReader::new(file);
    reader.for_each(|element| {
        let (lat, lon) = match element {
            Element::Node(n) => (n.lat(), n.lon()),
            Element::DenseNode(n) => (n.lat(), n.lon()),
            _ => return,
        };
        if !lat.is_finite() || !lon.is_finite() {
            return;
        }
        seen += 1;
        if lats.len() < SAMPLE_CAP {
            lats.push(lat);
            lons.push(lon);
        } else {
            // Reservoir: replace with decreasing probability.
            let j = (seen - 1) % SAMPLE_CAP as u64;
            // Cheap mix without pulling in a RNG crate.
            let slot = ((seen.wrapping_mul(11400714819323198485)) as usize) % SAMPLE_CAP;
            if j < SAMPLE_CAP as u64 / 4 || slot < SAMPLE_CAP / 8 {
                lats[slot] = lat;
                lons[slot] = lon;
            }
        }
    })?;
    if lats.is_empty() {
        anyhow::bail!("PBF has no nodes: {}", pbf.display());
    }
    lats.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    lons.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pct = |v: &[f64], p: f64| {
        let idx = ((v.len() as f64 - 1.0) * p).round() as usize;
        v[idx.min(v.len() - 1)]
    };
    let min_lat = pct(&lats, 0.005);
    let max_lat = pct(&lats, 0.995);
    let min_lon = pct(&lons, 0.005);
    let max_lon = pct(&lons, 0.995);
    // Small pad so boundary ways are not clipped away.
    Ok([
        min_lat - 0.02,
        min_lon - 0.02,
        max_lat + 0.02,
        max_lon + 0.02,
    ])
}

fn collect_poi_records(pbf: &Path) -> anyhow::Result<Vec<PoiRecord>> {
    let mut out = Vec::new();
    let file = std::fs::File::open(pbf)?;
    let reader = ElementReader::new(file);
    reader.for_each(|element| {
        let (id, lat, lon, tags) = match element {
            Element::Node(n) => (
                n.id(),
                n.lat(),
                n.lon(),
                n.tags()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect::<HashMap<_, _>>(),
            ),
            Element::DenseNode(n) => (
                n.id,
                n.lat(),
                n.lon(),
                n.tags()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect::<HashMap<_, _>>(),
            ),
            _ => return,
        };
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
    Ok(out)
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
        let file = std::fs::File::open(pbf)?;
        let reader = ElementReader::new(file);
        reader.for_each(|element| {
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
        let file = std::fs::File::open(pbf)?;
        let reader = ElementReader::new(file);
        reader.for_each(|element| match element {
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
    let region_bbox = pbf_node_bbox(&opts.pbf)?;
    note_rss(&mut peak_rss_mb, "bounds");

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
        // Graph tiling uses 3 PBF passes per profile (shared across all tiles),
        // not 3 passes × tile count (the previous multi-hour path).
        let total_steps = (build_profiles.len() + 3) as u64;
        for (pi, profile) in build_profiles.iter().enumerate() {
            if opts.control.is_cancelled() {
                anyhow::bail!("cancelled");
            }
            let key = profile_key(*profile).to_string();
            download_progress::set(
                (pi + 1) as u64,
                Some(total_steps),
                &format!("Building indexed maps: graph ({key}, tiled)…"),
            );
            let mut entries = Vec::new();
            let produced = RouteGraph::build_tiled_from_pbf(
                &opts.pbf,
                *profile,
                &tiles,
                0.05,
                |row, col, logical, graph| {
                    nodes = graph.nodes.len().max(nodes);
                    edges = graph.edges.len().max(edges);
                    note_rss(&mut peak_rss_mb, &format!("graph_{key}_{row}_{col}"));
                    if *profile == RoutingProfile::Car {
                        barrier_extra.extend(highway_barrier_segs(&graph));
                    }
                    let name = graph_tile_filename(&stem, &key, row, col);
                    let path = opts.data_dir.join(&name);
                    if opts.control.is_cancelled() {
                        discard_partial(&path);
                        anyhow::bail!("cancelled");
                    }
                    write_graph_pack(&path, &graph, elev_ref, &mut peak_rss_mb)?;
                    entries.push(GraphTileEntry {
                        file: name,
                        bbox: logical,
                    });
                    tile_count += 1;
                    Ok(())
                },
            )?;
            if produced == 0 || entries.is_empty() {
                anyhow::bail!("tiled convert produced no {key} tiles");
            }
            graph_tiles.insert(key, entries);
        }
        if want_truck {
            if let Some(car_tiles) = graph_tiles.get("car").cloned() {
                graph_tiles.insert("truck".into(), car_tiles);
            }
        }
    } else {
        let total_steps = (build_profiles.len() + 3) as u64;
        for (i, profile) in build_profiles.iter().enumerate() {
            if opts.control.is_cancelled() {
                anyhow::bail!("cancelled");
            }
            let key = profile_key(*profile).to_string();
            download_progress::set(
                (i + 1) as u64,
                Some(total_steps),
                &format!("Building indexed maps: graph ({key})…"),
            );
            let graph = RouteGraph::build_from_pbf_bbox(&opts.pbf, *profile, region_bbox)?;
            nodes = graph.nodes.len().max(nodes);
            edges = graph.edges.len().max(edges);
            note_rss(&mut peak_rss_mb, &format!("graph_{key}"));
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
    note_rss(&mut peak_rss_mb, "poi_start");
    let records = collect_poi_records(&opts.pbf)?;
    let (mut segs, glaciers) = extract_pbf_barrier_geometry(&opts.pbf)?;
    segs.extend(barrier_extra);
    let poi_pack = FlatPoiBarrierPack::from_parts(&records, &segs, &glaciers);
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
    // Wetland is optional for motor plans. Tiled (region-scale) converts skip it
    // on purpose: the ring index alone can exceed free RAM on 4GB tablets and
    // abort an otherwise successful graph build. Monolith corridors still try;
    // failures there remain best-effort skips.
    let wet_name = wetland_pack_filename(&stem);
    let wetland_rings = if use_tiles {
        log::info!("wetland extract skipped for tiled region convert");
        0
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
    let wetland_file = if opts.data_dir.join(&wet_name).is_file() {
        Some(wet_name.clone())
    } else {
        None
    };

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
        wetland_format_version: if wetland_file.is_some() {
            WETLAND_FORMAT_VERSION
        } else {
            0
        },
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
    })
}
