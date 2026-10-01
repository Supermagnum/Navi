//! Load + validate indexed packs (never interpret mismatched versions).

use std::fs::{self, File};
use std::path::{Path, PathBuf};

use memmap2::Mmap;
use rkyv::rancor::Error as RkyvError;
use thiserror::Error;

use super::graph_pack::{
    graph_format_version_accepted, ArchivedFlatGraphPack, GRAPH_FORMAT_VERSION, MAGIC_GRAPH,
};
use super::graph_pack_v8::ArchivedFlatGraphPackV8;
use super::header::Preamble;
use super::io::archive_payload_offset;
use super::manifest::{
    bbox_intersects, manifest_path, server_install_present, NaviManifest, PackStatus,
};
use super::poi_barrier_pack::{
    ArchivedFlatPoiBarrierPack, FlatPoiBarrierPack, MAGIC_POI_BARRIER, POI_BARRIER_FORMAT_VERSION,
};
use super::wetland_pack::{
    ArchivedFlatWetlandPack, FlatWetlandPack, MAGIC_WETLAND, WETLAND_FORMAT_VERSION,
};
use crate::poi::PoiIndex;
use crate::routing::basemap::{pbf_stem_to_geofabrik_path, region_bbox};
use crate::routing::graph::{GraphEdge, RouteGraph, RoutingProfile};
use crate::routing::safety::DangerBarrierIndex;
use crate::routing::wetland::WetlandIndex;
use std::collections::{HashMap, HashSet};

/// PBF whose size/mtime decide Ready vs Stale for packs under `data_dir`.
///
/// Pack lookup is keyed by the planning PBF **filename** (logical extract). The
/// fingerprint is always the copy in `data_dir` named `man.pbf_filename`, not
/// `planning_pbf.parent()` — a fixture or other clone of the same extract must
/// not silently send lookup to a directory with no manifest.
///
/// Returns [`PackLoadError::Missing`] when the planning filename does not match
/// the manifest (different logical extract).
pub fn fingerprint_pbf_for_packs(
    data_dir: &Path,
    planning_pbf: &Path,
    man: &NaviManifest,
) -> Result<PathBuf, PackLoadError> {
    let planning_name = planning_pbf.file_name().ok_or(PackLoadError::Missing)?;
    let declared = Path::new(&man.pbf_filename)
        .file_name()
        .ok_or(PackLoadError::Missing)?;
    if planning_name != declared {
        return Err(PackLoadError::Missing);
    }
    Ok(data_dir.join(&man.pbf_filename))
}

fn status_for_planning_pbf(
    data_dir: &Path,
    planning_pbf: &Path,
    man: &NaviManifest,
) -> Result<PackStatus, PackLoadError> {
    // Pack-server installs ship graphs without a real extract. A sidecar stamp
    // means digests were verified at install time — skip PBF fingerprint.
    if server_install_present(data_dir, &man.stem) {
        return Ok(man.status_pack_files(data_dir));
    }
    let packed = fingerprint_pbf_for_packs(data_dir, planning_pbf, man)?;
    Ok(man.status_for_pbf(data_dir, &packed))
}

/// Ready check for a stem that is not the planning PBF (multi-stem corridor).
fn stem_pack_ready(data_dir: &Path, man: &NaviManifest) -> bool {
    if server_install_present(data_dir, &man.stem) {
        return man.status_pack_files(data_dir) == PackStatus::Ready;
    }
    let packed = data_dir.join(&man.pbf_filename);
    if packed.is_file() {
        return man.status_for_pbf(data_dir, &packed) == PackStatus::Ready;
    }
    man.status_pack_files(data_dir) == PackStatus::Ready
}

/// First directory among [dirs] where [stem] packs are Ready (manifest + graphs).
fn home_dir_for_stem<'a>(dirs: &[&'a Path], stem: &str) -> Option<&'a Path> {
    for d in dirs {
        if let Ok(man) = load_ready_manifest(d, stem) {
            if stem_pack_ready(d, &man) {
                return Some(*d);
            }
        }
    }
    for d in dirs {
        if load_ready_manifest(d, stem).is_ok() {
            return Some(*d);
        }
    }
    None
}

/// Resolve a pack-relative file across long-trip + internal pack dirs.
fn resolve_pack_file(dirs: &[&Path], relative: &str) -> Option<PathBuf> {
    for d in dirs {
        let p = d.join(relative);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn graph_path_in_dirs(
    man: &NaviManifest,
    dirs: &[&Path],
    profile: RoutingProfile,
) -> Option<PathBuf> {
    for d in dirs {
        if let Some(p) = man.graph_path(d, profile) {
            if p.is_file() {
                return Some(p);
            }
        }
    }
    dirs.first().and_then(|d| man.graph_path(d, profile))
}

fn planning_stem(pbf: &Path) -> Result<String, PackLoadError> {
    pbf.file_name()
        .and_then(|s| s.to_str())
        .map(|name| {
            name.strip_suffix(".osm.pbf")
                .or_else(|| name.strip_suffix(".pbf"))
                .unwrap_or(name)
                .to_string()
        })
        .ok_or(PackLoadError::Missing)
}

/// True when `inner` lies entirely inside `outer` (`[min_lat, min_lon, max_lat, max_lon]`).
fn bbox_contained(inner: [f64; 4], outer: [f64; 4]) -> bool {
    inner[0] >= outer[0] && inner[1] >= outer[1] && inner[2] <= outer[2] && inner[3] <= outer[3]
}

/// Corridor needs tiles beyond the planning stem when the plan bbox is not
/// fully inside that stem's published region bbox (cross-landsdel pad).
fn corridor_needs_extra_stems(primary_stem: &str, bbox: Option<[f64; 4]>) -> bool {
    let Some(bbox) = bbox else {
        return false;
    };
    match pbf_stem_to_geofabrik_path(primary_stem).and_then(|p| region_bbox(&p)) {
        Some(region) => !bbox_contained(bbox, region),
        // Unknown stem / missing pack-leaf bbox: still scan Ready neighbours.
        // Refusing extras here silently plans on a single Bundesland and snaps
        // far vias (e.g. Stendal→Bessheim with only Sachsen-Anhalt tiles).
        None => true,
    }
}

/// True when a hop endpoint lies in a Ready **leaf** that is not the primary
/// stem (and not a child of it). Country AABBs routinely contain foreign leaves
/// (Denmark over Skåne); without this, `corridor_needs_extra_stems` stays false
/// and the destination leaf never loads — densify centroids then snap-fail.
fn corridor_needs_extra_for_endpoint_leaves(
    primary_stem: &str,
    route_points: Option<&[(f64, f64)]>,
    dirs: &[&Path],
) -> bool {
    let Some(pts) = route_points else {
        return false;
    };
    if pts.is_empty() {
        return false;
    }
    let Some(primary_path) = pbf_stem_to_geofabrik_path(primary_stem) else {
        return false;
    };
    let primary_prefix = format!("{primary_path}/");
    for data_dir in dirs {
        let Ok(entries) = fs::read_dir(data_dir) else {
            continue;
        };
        for ent in entries.flatten() {
            let name = ent.file_name();
            let name = name.to_string_lossy();
            let Some(stem) = name.strip_suffix(".navi-manifest.json") else {
                continue;
            };
            if stem == primary_stem {
                continue;
            }
            let Some(path) = pbf_stem_to_geofabrik_path(stem) else {
                continue;
            };
            if path.matches('/').count() < 2 {
                continue;
            }
            if path == primary_path || path.starts_with(&primary_prefix) {
                continue;
            }
            let Some(region) = region_bbox(&path) else {
                continue;
            };
            if pts
                .iter()
                .any(|&(lat, lon)| crate::routing::basemap::bbox_covers_point(region, lat, lon))
            {
                return true;
            }
        }
    }
    false
}

fn load_ready_manifest(data_dir: &Path, stem: &str) -> Result<NaviManifest, PackLoadError> {
    let man_path = manifest_path(data_dir, stem);
    if !man_path.is_file() {
        return Err(PackLoadError::Missing);
    }
    NaviManifest::load(&man_path).map_err(|_| PackLoadError::Missing)
}

/// Other installed Ready manifests whose region bbox intersects `bbox`.
fn extra_corridor_manifests(
    data_dir: &Path,
    primary_stem: &str,
    bbox: [f64; 4],
) -> Vec<NaviManifest> {
    extra_corridor_manifests_segs(&[data_dir], primary_stem, &[bbox])
}

fn extra_corridor_manifests_segs(
    dirs: &[&Path],
    primary_stem: &str,
    segs: &[[f64; 4]],
) -> Vec<NaviManifest> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for data_dir in dirs {
        let Ok(entries) = fs::read_dir(data_dir) else {
            continue;
        };
        for ent in entries.flatten() {
            let name = ent.file_name();
            let name = name.to_string_lossy();
            let Some(stem) = name.strip_suffix(".navi-manifest.json") else {
                continue;
            };
            if stem == primary_stem || seen.contains(stem) {
                continue;
            }
            let Some(home) = home_dir_for_stem(dirs, stem) else {
                continue;
            };
            let Ok(man) = load_ready_manifest(home, stem) else {
                continue;
            };
            if !stem_pack_ready(home, &man) {
                continue;
            }
            let Some(path) = pbf_stem_to_geofabrik_path(&man.stem) else {
                continue;
            };
            let Some(region) = region_bbox(&path) else {
                continue;
            };
            if !segs.iter().any(|b| bbox_intersects(region, *b)) {
                continue;
            }
            seen.insert(stem.to_string());
            out.push(man);
        }
    }
    out.sort_by(|a, b| a.stem.cmp(&b.stem));
    out
}

fn append_intersecting_tile_files(
    files: &mut Vec<String>,
    seen: &mut HashSet<String>,
    tiles: &[super::manifest::GraphTileEntry],
    bbox: Option<[f64; 4]>,
) {
    let mut tmp = Vec::new();
    let mut seen2 = seen.clone();
    append_intersecting_tile_files_corridor(&mut tmp, &mut seen2, tiles, bbox, None);
    for (f, _) in tmp {
        if seen.insert(f.clone()) {
            files.push(f);
        }
    }
}

fn append_intersecting_tile_files_corridor(
    files: &mut Vec<(String, [f64; 4])>,
    seen: &mut HashSet<String>,
    tiles: &[super::manifest::GraphTileEntry],
    clip_bbox: Option<[f64; 4]>,
    corridor_segs: Option<&[[f64; 4]]>,
) {
    for t in tiles {
        if let Some(segs) = corridor_segs {
            if !crate::routing::plan_bbox::tile_intersects_corridor(t.bbox, segs) {
                continue;
            }
        } else if let Some(b) = clip_bbox {
            if !bbox_intersects(t.bbox, b) {
                continue;
            }
        }
        if seen.insert(t.file.clone()) {
            files.push((t.file.clone(), t.bbox));
        }
    }
}

/// Prefer tiles covering hop endpoints, then those closest to the endpoints.
/// Always retain at least one tile per endpoint (4 GB budget must not drop the
/// destination). When scores tie, prefer smaller on-disk tiles.
/// True when two tile bboxes share an edge (or overlap) on the index grid.
///
/// Used to rank bridge fillers: a tile adjoining two already-selected leaves is
/// a true corridor bridge (e.g. Ostlandet `t2_3` between `t1_3` and `t2_4`).
fn tile_bboxes_adjacent(a: [f64; 4], b: [f64; 4]) -> bool {
    const EPS: f64 = 1e-5;
    let lat_overlap = a[0] < b[2] - EPS && b[0] < a[2] - EPS;
    let lon_overlap = a[1] < b[3] - EPS && b[1] < a[3] - EPS;
    if lat_overlap && lon_overlap {
        return true;
    }
    let lat_touch = (a[2] - b[0]).abs() < EPS || (b[2] - a[0]).abs() < EPS;
    let lon_touch = (a[3] - b[1]).abs() < EPS || (b[3] - a[1]).abs() < EPS;
    (lat_touch && lon_overlap) || (lon_touch && lat_overlap)
}

/// True when a selected tile may count as covering a hop endpoint for budget
/// retention. Country extracts whose AABB spills over Ready foreign leaves
/// (e.g. `europe/denmark` over Skåne) must not satisfy endpoint coverage — that
/// let the budget drop the real leaf stem and left densify centroids
/// unsnappable on every sea-adjacent corridor.
fn tile_counts_as_endpoint_cover(
    tile_name: &str,
    tile_bbox: [f64; 4],
    lat: f64,
    lon: f64,
    ready_paths: &[(String, [f64; 4])],
) -> bool {
    if !crate::routing::basemap::bbox_covers_point(tile_bbox, lat, lon) {
        return false;
    }
    let stem = tile_name.split(".navi-graph-").next().unwrap_or(tile_name);
    let Some(path) = pbf_stem_to_geofabrik_path(stem) else {
        return true;
    };
    let leaf_ready_covers = ready_paths.iter().any(|(p, b)| {
        p.matches('/').count() >= 2 && crate::routing::basemap::bbox_covers_point(*b, lat, lon)
    });
    if leaf_ready_covers
        && crate::routing::plan_bbox::densify_skip_country_when_leaves_ready(&path, ready_paths)
    {
        return false;
    }
    true
}

fn select_tiles_within_budget(
    candidates: Vec<(String, [f64; 4])>,
    route_points: Option<&[(f64, f64)]>,
    max_tiles: usize,
    dirs: &[&Path],
) -> Vec<String> {
    let pts = route_points.unwrap_or(&[]);
    if candidates.is_empty() {
        return Vec::new();
    }
    // No route geometry: keep up to max_tiles (deterministic name order).
    if pts.is_empty() {
        let mut names: Vec<String> = candidates.into_iter().map(|(f, _)| f).collect();
        names.sort();
        names.truncate(max_tiles);
        return names;
    }
    let file_len = |name: &str| -> u64 {
        resolve_pack_file(dirs, name)
            .and_then(|p| fs::metadata(p).ok())
            .map(|m| m.len())
            .unwrap_or(u64::MAX)
    };

    // Guarantee coverage of each route point and corridor samples so a tight
    // tile budget cannot drop the bridge between start and end (disconnected).
    // One midpoint is not enough when endpoint tiles do not touch (SA t2_1 and
    // NI t2_4 on Stendal→Hannover). Quarter-points still miss diagonal bridges
    // (Hamar→Dombås: t2_3 and t3_2 only meet at a corner; t2_2 sits at ~0.625).
    // Eighths pull those intervening same-stem leaves without raising max_tiles.
    let mut samples: Vec<(f64, f64)> = pts.to_vec();
    if pts.len() >= 2 {
        for w in pts.windows(2) {
            for &t in &[0.125_f64, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875] {
                samples.push((
                    w[0].0 + (w[1].0 - w[0].0) * t,
                    w[0].1 + (w[1].1 - w[0].1) * t,
                ));
            }
        }
    }
    let mut selected: Vec<(String, [f64; 4])> = Vec::new();
    let mut selected_names = HashSet::new();
    // Ready geofabrik paths for spill-country suppression (DK∩Skåne).
    let ready_paths: Vec<(String, [f64; 4])> = {
        let mut ready = Vec::new();
        let mut seen = HashSet::new();
        for data_dir in dirs {
            let Ok(entries) = fs::read_dir(data_dir) else {
                continue;
            };
            for ent in entries.flatten() {
                let name = ent.file_name();
                let name = name.to_string_lossy();
                let Some(stem) = name.strip_suffix(".navi-manifest.json") else {
                    continue;
                };
                let Some(path) = pbf_stem_to_geofabrik_path(stem) else {
                    continue;
                };
                if !seen.insert(path.clone()) {
                    continue;
                }
                let Some(bbox) = region_bbox(&path) else {
                    continue;
                };
                ready.push((path, bbox));
            }
        }
        ready
    };
    for &(lat, lon) in &samples {
        // Per sample: keep the smallest covering tile from each stem so a
        // border midpoint keeps both neighbour packs (NI+SH), not only one.
        let mut best_per_stem: HashMap<String, (u64, usize)> = HashMap::new();
        for (i, (name, bbox)) in candidates.iter().enumerate() {
            if !crate::routing::basemap::bbox_covers_point(*bbox, lat, lon) {
                continue;
            }
            let stem = name
                .split(".navi-graph-")
                .next()
                .unwrap_or(name)
                .to_string();
            let len = file_len(name);
            match best_per_stem.get(&stem) {
                Some((bl, _)) if len >= *bl => {}
                _ => {
                    best_per_stem.insert(stem, (len, i));
                }
            }
        }
        // When a leaf pack covers the sample, drop country extracts that densify
        // would skip (europe/denmark spilling over Skåne/Halland) so they do not
        // consume the tile budget and leave the real bridge stem unloaded.
        let leaf_covers = best_per_stem.keys().any(|stem| {
            pbf_stem_to_geofabrik_path(stem).is_some_and(|p| p.matches('/').count() >= 2)
        });
        if leaf_covers {
            best_per_stem.retain(|stem, _| match pbf_stem_to_geofabrik_path(stem) {
                Some(path) => !crate::routing::plan_bbox::densify_skip_country_when_leaves_ready(
                    &path,
                    &ready_paths,
                ),
                None => true,
            });
        }
        for (_, i) in best_per_stem.values() {
            let (name, bbox) = candidates[*i].clone();
            if selected_names.insert(name.clone()) {
                selected.push((name, bbox));
            }
        }
    }

    // Cap only — never pad with leftover corridor-overlap candidates up to
    // max_tiles. Loading every intersecting Ostlandet car tile (40–110 MB each)
    // for a short densify hop peaks at ~450k edges and stalls snap/A* on device.
    // Still fill a few bridge tiles toward max_tiles when samples alone leave
    // a same-stem gap (endpoint tiles that only touch at a corner).
    //
    // Bridge ranking prefers tiles that **adjoin the most already-selected
    // tiles** (true spatial bridges on the grid). Chord eighth-samples alone
    // miss off-chord highway corridors (R4b: start→via jumps t1_3→t2_4 and
    // never hits western-E6 t2_3); nearest-to-endpoint scoring then filled
    // t1_4+t2_5 and dropped t2_3, forcing a southern Minnesund pack detour.
    let score = |bbox: [f64; 4]| -> (i32, i64) {
        if pts.is_empty() {
            return (2, 0);
        }
        let mut best_prio = 2_i32;
        let mut best_d = i64::MAX;
        for &(lat, lon) in pts {
            if crate::routing::basemap::bbox_covers_point(bbox, lat, lon) {
                return (0, 0);
            }
            let clat = (bbox[0] + bbox[2]) * 0.5;
            let clon = (bbox[1] + bbox[3]) * 0.5;
            let d = (((clat - lat).abs() + (clon - lon).abs()) * 1e6) as i64;
            if d < best_d {
                best_d = d;
                best_prio = 1;
            }
        }
        (best_prio, best_d)
    };
    let adjacency = |bbox: [f64; 4]| -> i32 {
        selected
            .iter()
            .filter(|(_, sb)| tile_bboxes_adjacent(*sb, bbox))
            .count() as i32
    };
    let mut rest: Vec<(String, [f64; 4])> = candidates
        .iter()
        .filter(|(n, _)| !selected_names.contains(n))
        .cloned()
        .collect();
    rest.sort_by(|a, b| {
        // Higher adjacency first, then nearer-to-endpoints, then smaller file.
        adjacency(b.1)
            .cmp(&adjacency(a.1))
            .then_with(|| score(a.1).cmp(&score(b.1)))
            .then_with(|| file_len(&a.0).cmp(&file_len(&b.0)))
            .then_with(|| a.0.cmp(&b.0))
    });
    // At most two bridge fillers beyond sample coverage — enough for a corner
    // gap, not enough to re-pull every Ostlandet corridor tile.
    let bridge_cap = (selected.len() + 2).min(max_tiles);
    for (name, bbox) in rest {
        if selected.len() >= bridge_cap {
            break;
        }
        if selected_names.insert(name.clone()) {
            selected.push((name, bbox));
        }
    }

    if selected.len() > max_tiles {
        selected.sort_by(|a, b| {
            file_len(&a.0)
                .cmp(&file_len(&b.0))
                .then_with(|| a.0.cmp(&b.0))
        });
        // Keep endpoint coverage: re-run sample picks on the size-sorted prefix
        // is lossy; prefer dropping largest extras while endpoints stay covered.
        // Country AABB spill must not count as covering a leaf endpoint.
        let covers = |files: &[(String, [f64; 4])], lat: f64, lon: f64| -> bool {
            files
                .iter()
                .any(|(n, b)| tile_counts_as_endpoint_cover(n, *b, lat, lon, &ready_paths))
        };
        while selected.len() > max_tiles {
            let mut dropped = false;
            // Drop largest tile that is not the sole cover of any endpoint.
            let order: Vec<usize> = {
                let mut idx: Vec<usize> = (0..selected.len()).collect();
                idx.sort_by(|&i, &j| file_len(&selected[j].0).cmp(&file_len(&selected[i].0)));
                idx
            };
            for i in order {
                let name = selected[i].0.clone();
                let without: Vec<_> = selected
                    .iter()
                    .filter(|(n, _)| n != &name)
                    .cloned()
                    .collect();
                if pts.iter().all(|&(lat, lon)| covers(&without, lat, lon)) {
                    selected = without;
                    selected_names.remove(&name);
                    dropped = true;
                    break;
                }
            }
            if !dropped {
                break;
            }
        }
    }
    // Soft disk-byte budget: drop largest non-essential tiles while endpoints stay
    // covered. Prevents six ~70–130 MB car tiles (~1.1M edges) on densify hops.
    let max_bytes = crate::routing::plan_bbox::MAX_PLAN_TILE_BYTES;
    let total_bytes = |files: &[(String, [f64; 4])]| -> u64 {
        files
            .iter()
            .map(|(n, _)| file_len(n))
            .fold(0u64, u64::saturating_add)
    };
    let covers = |files: &[(String, [f64; 4])], lat: f64, lon: f64| -> bool {
        files
            .iter()
            .any(|(n, b)| tile_counts_as_endpoint_cover(n, *b, lat, lon, &ready_paths))
    };
    while total_bytes(&selected) > max_bytes && selected.len() > 2 {
        let mut dropped = false;
        let order: Vec<usize> = {
            let mut idx: Vec<usize> = (0..selected.len()).collect();
            idx.sort_by(|&i, &j| file_len(&selected[j].0).cmp(&file_len(&selected[i].0)));
            idx
        };
        for i in order {
            let name = selected[i].0.clone();
            let without: Vec<_> = selected
                .iter()
                .filter(|(n, _)| n != &name)
                .cloned()
                .collect();
            if pts.iter().all(|&(lat, lon)| covers(&without, lat, lon)) {
                selected = without;
                selected_names.remove(&name);
                dropped = true;
                break;
            }
        }
        if !dropped {
            break;
        }
    }
    let mut files: Vec<String> = selected.into_iter().map(|(f, _)| f).collect();
    files.sort();
    files
}

#[derive(Debug, Error)]
pub enum PackLoadError {
    #[error("indexed pack missing or incomplete")]
    Missing,
    #[error("indexed pack stale vs source PBF")]
    Stale,
    #[error("indexed pack version/magic mismatch (rebuild required)")]
    VersionMismatch,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("rkyv access failed: {0}")]
    Rkyv(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

fn map_file(path: &Path) -> Result<Mmap, PackLoadError> {
    if path.extension().is_some_and(|e| e == "partial")
        || path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().contains(".partial"))
    {
        return Err(PackLoadError::Missing);
    }
    let file = File::open(path)?;
    // SAFETY: callers must not mutate/truncate the file while mapped. Packs are
    // published via atomic rename and treated as immutable-after-publish.
    let mmap = unsafe { Mmap::map(&file)? };
    Ok(mmap)
}

fn check_preamble(mmap: &Mmap, expect_magic: u32, expect_ver: u32) -> Result<(), PackLoadError> {
    let p = Preamble::from_bytes(mmap).ok_or(PackLoadError::VersionMismatch)?;
    if p.magic != expect_magic || p.format_version != expect_ver {
        return Err(PackLoadError::VersionMismatch);
    }
    Ok(())
}

fn check_graph_preamble(mmap: &Mmap) -> Result<u32, PackLoadError> {
    let p = Preamble::from_bytes(mmap).ok_or(PackLoadError::VersionMismatch)?;
    if p.magic != MAGIC_GRAPH || !graph_format_version_accepted(p.format_version) {
        return Err(PackLoadError::VersionMismatch);
    }
    Ok(p.format_version)
}

/// Deserialize graph pack body after preamble validation. Materializes owned
/// [`RouteGraph`] (adapter to existing planners). Does **not** interpret a
/// mismatched header.
pub fn load_graph_pack(path: &Path, profile: RoutingProfile) -> Result<RouteGraph, PackLoadError> {
    load_graph_pack_bbox(path, profile, None)
}

pub fn load_graph_pack_bbox(
    path: &Path,
    profile: RoutingProfile,
    bbox: Option<[f64; 4]>,
) -> Result<RouteGraph, PackLoadError> {
    match bbox {
        Some(b) => load_graph_pack_clips(path, profile, Some(std::slice::from_ref(&b))),
        None => load_graph_pack_clips(path, profile, None),
    }
}

/// Like [`load_graph_pack_bbox`], keeping edges that touch **any** clip box
/// (corridor band). Prefer this for densify hops so diagonal AABBs do not
/// materialize ~1M edges on 4 GB Automotive.
pub fn load_graph_pack_clips(
    path: &Path,
    profile: RoutingProfile,
    clips: Option<&[[f64; 4]]>,
) -> Result<RouteGraph, PackLoadError> {
    let mmap = map_file(path)?;
    let format_version = check_graph_preamble(&mmap)?;
    let body = &mmap[archive_payload_offset()..];
    // Materialize from the mmap'd archive — do **not** `rkyv::deserialize` into an
    // owned FlatGraphPack first. That temporary peaks at roughly pack-file size in
    // extra RAM (string tables) and thrashing-hangs multi-tile long-trip loads on
    // Automotive devices when a merged graph already occupies hundreds of MB.
    //
    // v8 and v9 have different rkyv layouts (`edge_is_tunnel` only on v9); pick the
    // archived type from the preamble. v8 materializes `is_tunnel = false`.
    let file = path.file_name().and_then(|s| s.to_str()).unwrap_or("?");
    crate::download::progress::set(0, Some(1), &format!("Building graph {file}…"));
    let t0 = std::time::Instant::now();
    let g = match format_version {
        super::graph_pack::GRAPH_FORMAT_VERSION_V8 => {
            let archived = rkyv::access::<ArchivedFlatGraphPackV8, RkyvError>(body)
                .map_err(|e| PackLoadError::Rkyv(e.to_string()))?;
            archived.to_route_graph_clips(profile, clips)
        }
        GRAPH_FORMAT_VERSION => {
            let archived = rkyv::access::<ArchivedFlatGraphPack, RkyvError>(body)
                .map_err(|e| PackLoadError::Rkyv(e.to_string()))?;
            archived.to_route_graph_clips(profile, clips)
        }
        _ => return Err(PackLoadError::VersionMismatch),
    };
    log::info!(
        target: "NaviPlan",
        "load_graph_pack_bbox file={file} format={format_version} edges={} nodes={} clips={} elapsed_ms={}",
        g.edges.len(),
        g.nodes.len(),
        clips.map(|c| c.len()).unwrap_or(0),
        t0.elapsed().as_millis()
    );
    Ok(g)
}

pub fn load_poi_barrier_pack(path: &Path) -> Result<(PoiIndex, DangerBarrierIndex), PackLoadError> {
    let mmap = map_file(path)?;
    check_preamble(&mmap, MAGIC_POI_BARRIER, POI_BARRIER_FORMAT_VERSION)?;
    let body = &mmap[archive_payload_offset()..];
    let archived = rkyv::access::<ArchivedFlatPoiBarrierPack, RkyvError>(body)
        .map_err(|e| PackLoadError::Rkyv(e.to_string()))?;
    let pack: FlatPoiBarrierPack = rkyv::deserialize::<FlatPoiBarrierPack, RkyvError>(archived)
        .map_err(|e| PackLoadError::Rkyv(e.to_string()))?;
    Ok((pack.to_poi_index(), pack.to_barrier_index()))
}

pub fn load_wetland_pack(
    path: &Path,
    bbox: Option<[f64; 4]>,
) -> Result<WetlandIndex, PackLoadError> {
    let mmap = map_file(path)?;
    check_preamble(&mmap, MAGIC_WETLAND, WETLAND_FORMAT_VERSION)?;
    let body = &mmap[archive_payload_offset()..];
    let archived = rkyv::access::<ArchivedFlatWetlandPack, RkyvError>(body)
        .map_err(|e| PackLoadError::Rkyv(e.to_string()))?;
    let pack: FlatWetlandPack = rkyv::deserialize::<FlatWetlandPack, RkyvError>(archived)
        .map_err(|e| PackLoadError::Rkyv(e.to_string()))?;
    Ok(pack.to_wetland_index(bbox))
}

pub struct PackedPlanData {
    pub graph: RouteGraph,
    pub poi: PoiIndex,
    pub barriers: DangerBarrierIndex,
    pub from_pack: bool,
}

/// Try loading region packs for a plan. Returns `Err` variants that callers
/// should treat as “use PBF fallback”.
pub fn try_load_graph_for_plan(
    data_dir: &Path,
    pbf: &Path,
    profile: RoutingProfile,
) -> Result<RouteGraph, PackLoadError> {
    try_load_graph_for_plan_bbox(data_dir, pbf, profile, None)
}

pub fn try_load_graph_for_plan_bbox(
    data_dir: &Path,
    pbf: &Path,
    profile: RoutingProfile,
    bbox: Option<[f64; 4]>,
) -> Result<RouteGraph, PackLoadError> {
    try_load_graph_for_plan_corridor(data_dir, pbf, profile, bbox, None)
}

/// Like [`try_load_graph_for_plan_bbox`], but when `route_points` has ≥2 stops,
/// graph tiles and extra stems are selected against padded **segment** bboxes
/// (polyline corridor) instead of the full trip AABB — critical for long-trip
/// RAM on Automotive devices.
pub fn try_load_graph_for_plan_corridor(
    data_dir: &Path,
    pbf: &Path,
    profile: RoutingProfile,
    clip_bbox: Option<[f64; 4]>,
    route_points: Option<&[(f64, f64)]>,
) -> Result<RouteGraph, PackLoadError> {
    try_load_graph_for_plan_corridor_with_pack_dirs(
        data_dir,
        &[],
        pbf,
        profile,
        clip_bbox,
        route_points,
        crate::routing::plan_bbox::PlanEdgeClipMode::CorridorBand,
    )
}

/// Like [`try_load_graph_for_plan_corridor`], but also searches [pack_dirs]
/// (e.g. `files/long-trip-packs` or a removable volume pack root) for Ready
/// manifests and tile files. [data_dir] remains the Tools / ReuseInternal root.
///
/// `edge_clip_mode` selects corridor-band vs trip-AABB edge materialization
/// ([`crate::routing::plan_bbox::PlanEdgeClipMode`]).
pub fn try_load_graph_for_plan_corridor_with_pack_dirs(
    data_dir: &Path,
    pack_dirs: &[PathBuf],
    pbf: &Path,
    profile: RoutingProfile,
    clip_bbox: Option<[f64; 4]>,
    route_points: Option<&[(f64, f64)]>,
    edge_clip_mode: crate::routing::plan_bbox::PlanEdgeClipMode,
) -> Result<RouteGraph, PackLoadError> {
    let mut owned: Vec<PathBuf> = Vec::new();
    for p in pack_dirs {
        if p.as_os_str().is_empty() {
            continue;
        }
        if p.is_dir() && !owned.iter().any(|x| x == p) {
            owned.push(p.clone());
        }
    }
    if !owned.iter().any(|x| x.as_path() == data_dir) {
        owned.push(data_dir.to_path_buf());
    }
    let dirs: Vec<&Path> = owned.iter().map(|p| p.as_path()).collect();
    try_load_graph_for_plan_corridor_dirs(
        &dirs,
        pbf,
        profile,
        clip_bbox,
        route_points,
        edge_clip_mode,
    )
}

fn try_load_graph_for_plan_corridor_dirs(
    dirs: &[&Path],
    pbf: &Path,
    profile: RoutingProfile,
    clip_bbox: Option<[f64; 4]>,
    route_points: Option<&[(f64, f64)]>,
    edge_clip_mode: crate::routing::plan_bbox::PlanEdgeClipMode,
) -> Result<RouteGraph, PackLoadError> {
    let pbf_stem = planning_stem(pbf)?;
    // Chunked long-trip legs still pass the origin PBF; re-home primary to the
    // Ready stem that covers the hop start so we do not merge Sachsen-Anhalt
    // tiles into every Norway leg.
    let (stem, man, primary_dir) = pick_primary_manifest(dirs, &pbf_stem, route_points)?;
    if stem == pbf_stem {
        match status_for_planning_pbf(primary_dir, pbf, &man)? {
            PackStatus::Ready => {}
            PackStatus::Missing => return Err(PackLoadError::Missing),
            PackStatus::StalePbf => return Err(PackLoadError::Stale),
            PackStatus::VersionMismatch => return Err(PackLoadError::VersionMismatch),
        }
    } else if !stem_pack_ready(primary_dir, &man) {
        return Err(PackLoadError::Missing);
    }

    let corridor_segs: Option<Vec<[f64; 4]>> = route_points.and_then(|pts| {
        if pts.len() < 2 {
            return None;
        }
        Some(crate::routing::plan_bbox::corridor_segment_bboxes(
            pts,
            crate::routing::plan_bbox::CORRIDOR_TILE_PAD_DEG,
        ))
    });
    let segs_ref = corridor_segs.as_deref();
    // Edge materialization: default corridor **band** along the OD (not
    // expand(union(segs)) — that fat AABB pulled ~1.1M edges / ~3 GiB RSS on
    // the Bevensen→SH first densify hop). TripAabb mode uses clip_bbox so pad
    // widen can recover cross-track detours the band permanently excludes.
    let edge_clips_owned =
        crate::routing::plan_bbox::plan_edge_clips(route_points, clip_bbox, edge_clip_mode);
    let edge_clips = edge_clips_owned.as_deref();
    log::info!(
        target: "NaviPlan",
        "edge_clip_mode={edge_clip_mode:?} clips={}",
        edge_clips.map(|c| c.len()).unwrap_or(0)
    );
    // Coarse stem-spill gate still uses a modest expanded corridor AABB.
    let stem_clip = corridor_segs
        .as_ref()
        .and_then(|segs| union_bboxes(segs))
        .map(|b| expand_bbox_deg(b, 0.05))
        .or(clip_bbox);

    let need_extra = corridor_needs_extra_stems(&stem, stem_clip.or(clip_bbox))
        || corridor_needs_extra_for_endpoint_leaves(&stem, route_points, dirs);
    let mut extras = if need_extra {
        if let Some(segs) = segs_ref {
            extra_corridor_manifests_segs(dirs, &stem, segs)
        } else if let Some(b) = clip_bbox {
            extra_corridor_manifests_segs(dirs, &stem, &[b])
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };
    // For short hops (chunked legs): keep only stems covering start and/or end,
    // or intersecting the corridor segment — then prefer endpoint stems when capping.
    if let Some(pts) = route_points {
        if pts.len() == 2 {
            extras.retain(|m| {
                let Some(path) = pbf_stem_to_geofabrik_path(&m.stem) else {
                    return false;
                };
                let Some(region) = region_bbox(&path) else {
                    return false;
                };
                crate::routing::basemap::bbox_covers_point(region, pts[0].0, pts[0].1)
                    || crate::routing::basemap::bbox_covers_point(region, pts[1].0, pts[1].1)
                    || segs_ref.is_some_and(|segs| segs.iter().any(|s| bbox_intersects(region, *s)))
            });
            // Catalog country AABBs spill across borders (Finland over eastern
            // Finnmark). When both hop ends PIP to the same leaf, foreign
            // extras that only match via AABB steal the plan tile budget and
            // disconnect the real leaf network (Bugøynes→first SE densify hop).
            if let (Some(a), Some(b)) = (
                crate::long_trip::region_containing(pts[0].0, pts[0].1, None),
                crate::long_trip::region_containing(pts[1].0, pts[1].1, None),
            ) {
                if a == b {
                    extras.clear();
                }
            }
            // PIP holes (Finnish Lapland) still need the Ready country extract:
            // Nord-Norge AABB covers the point but has no connecting roads.
            // Force-retain covering Ready extras that were cleared or never
            // matched the same-leaf rule.
            for &(lat, lon) in pts {
                if crate::long_trip::region_containing(lat, lon, None).is_some() {
                    continue;
                }
                for data_dir in dirs {
                    let Ok(entries) = fs::read_dir(data_dir) else {
                        continue;
                    };
                    for ent in entries.flatten() {
                        let name = ent.file_name();
                        let name = name.to_string_lossy();
                        let Some(stem) = name.strip_suffix(".navi-manifest.json") else {
                            continue;
                        };
                        if stem == man.stem.as_str() || extras.iter().any(|m| m.stem == stem) {
                            continue;
                        }
                        let Some(home) = home_dir_for_stem(dirs, stem) else {
                            continue;
                        };
                        let Ok(extra_man) = load_ready_manifest(home, stem) else {
                            continue;
                        };
                        if !stem_pack_ready(home, &extra_man) {
                            continue;
                        }
                        let Some(path) = pbf_stem_to_geofabrik_path(&extra_man.stem) else {
                            continue;
                        };
                        // Country extracts only (leaf count < 2) — hole fills
                        // like europe/finland, not every spill leaf.
                        if path.matches('/').count() != 1 {
                            continue;
                        }
                        let Some(region) = region_bbox(&path) else {
                            continue;
                        };
                        if crate::routing::basemap::bbox_covers_point(region, lat, lon) {
                            extras.push(extra_man);
                        }
                    }
                }
            }
            // Cap extras hard for 4 GB: at most three neighbour stems. Prefer
            // endpoint-covering stems, then corridor-intersecting (bridges like
            // niedersachsen between SA and SH).
            if extras.len() > 3 {
                let covers_pt = |m: &NaviManifest, lat: f64, lon: f64| -> bool {
                    pbf_stem_to_geofabrik_path(&m.stem)
                        .and_then(|p| region_bbox(&p))
                        .is_some_and(|r| crate::routing::basemap::bbox_covers_point(r, lat, lon))
                };
                let seg_mid = ((pts[0].0 + pts[1].0) * 0.5, (pts[0].1 + pts[1].1) * 0.5);
                let mid_dist = |m: &NaviManifest| -> f64 {
                    pbf_stem_to_geofabrik_path(&m.stem)
                        .and_then(|p| region_bbox(&p))
                        .map(|r| {
                            let c = ((r[0] + r[2]) * 0.5, (r[1] + r[3]) * 0.5);
                            (c.0 - seg_mid.0).abs() + (c.1 - seg_mid.1).abs()
                        })
                        .unwrap_or(f64::MAX)
                };
                let mut kept: Vec<NaviManifest> = Vec::new();
                for &(lat, lon) in pts {
                    if let Some(m) = extras.iter().find(|m| covers_pt(m, lat, lon)) {
                        if !kept.iter().any(|k| k.stem == m.stem) {
                            kept.push(m.clone());
                        }
                    }
                }
                let mut rest: Vec<NaviManifest> = extras
                    .iter()
                    .filter(|m| !kept.iter().any(|k| k.stem == m.stem))
                    .cloned()
                    .collect();
                rest.sort_by(|a, b| {
                    mid_dist(a)
                        .partial_cmp(&mid_dist(b))
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| a.stem.cmp(&b.stem))
                });
                for m in rest {
                    if kept.len() >= 3 {
                        break;
                    }
                    kept.push(m);
                }
                extras = kept;
            }
        }
    }
    log::info!(
        target: "NaviPlan",
        "try_load_graph stem={stem} (pbf_stem={pbf_stem}) profile={profile:?} \
         need_extra={need_extra} extras={} corridor_segs={} dirs={}",
        extras.len(),
        corridor_segs.as_ref().map(|s| s.len()).unwrap_or(0),
        dirs.iter()
            .map(|d| d.display().to_string())
            .collect::<Vec<_>>()
            .join(";")
    );
    if !extras.is_empty() {
        crate::download::progress::set(0, Some(5), "Combining map data from multiple regions…");
    }

    let mut seen = HashSet::new();
    let mut tile_candidates = Vec::new();
    if let Some(tiles) = man.graph_tiles_for(profile) {
        append_intersecting_tile_files_corridor(
            &mut tile_candidates,
            &mut seen,
            tiles,
            clip_bbox,
            segs_ref,
        );
    } else {
        log::warn!(
            target: "NaviPlan",
            "try_load_graph: no graph tiles for profile={profile:?} on stem={stem}"
        );
    }
    for extra in &extras {
        if let Some(tiles) = extra.graph_tiles_for(profile) {
            append_intersecting_tile_files_corridor(
                &mut tile_candidates,
                &mut seen,
                tiles,
                clip_bbox,
                segs_ref,
            );
        }
    }
    let mut tile_files = select_tiles_within_budget(
        tile_candidates.clone(),
        route_points,
        crate::routing::plan_bbox::effective_max_plan_tiles(),
        dirs,
    );
    // Same-stem short hops (e.g. eastern→western Skåne) need every intersecting
    // primary tile: endpoint tiles may only touch at a corner and A* then reports
    // disconnected under a tight tile budget.
    if let Some(pts) = route_points {
        if pts.len() == 2 {
            if let Some(path) = pbf_stem_to_geofabrik_path(&stem) {
                if let Some(region) = region_bbox(&path) {
                    let same =
                        crate::routing::basemap::bbox_covers_point(region, pts[0].0, pts[0].1)
                            && crate::routing::basemap::bbox_covers_point(
                                region, pts[1].0, pts[1].1,
                            );
                    if same {
                        if let Some(tiles) = man.graph_tiles_for(profile) {
                            let tile_bbox: HashMap<&str, [f64; 4]> =
                                tiles.iter().map(|t| (t.file.as_str(), t.bbox)).collect();
                            let selected_covers = |lat: f64, lon: f64| -> bool {
                                tile_files.iter().any(|f| {
                                    tile_bbox.get(f.as_str()).is_some_and(|b| {
                                        crate::routing::basemap::bbox_covers_point(*b, lat, lon)
                                    })
                                })
                            };
                            // Budget selection already covers both ends: do **not**
                            // pull every corridor-intersecting primary tile (Ostlandet
                            // car tiles are 40–110 MB; four of them → ~450k edges and
                            // multi-minute snap/A* on Automotive). Only fill when an
                            // endpoint is still uncovered.
                            let mut seen: HashSet<String> = tile_files.iter().cloned().collect();
                            if !(selected_covers(pts[0].0, pts[0].1)
                                && selected_covers(pts[1].0, pts[1].1))
                            {
                                let mut extras_cands: Vec<(String, [f64; 4])> = tiles
                                    .iter()
                                    .filter(|t| {
                                        let hit = segs_ref.is_some_and(|segs| {
                                            segs.iter().any(|s| bbox_intersects(t.bbox, *s))
                                        }) || clip_bbox
                                            .is_some_and(|b| bbox_intersects(t.bbox, b));
                                        hit && !seen.contains(&t.file)
                                    })
                                    .map(|t| (t.file.clone(), t.bbox))
                                    .collect();
                                // Prefer already-selected + smallest fillers.
                                let mut merged_cands: Vec<(String, [f64; 4])> = tile_files
                                    .iter()
                                    .filter_map(|f| {
                                        tile_bbox.get(f.as_str()).map(|b| (f.clone(), *b))
                                    })
                                    .collect();
                                merged_cands.append(&mut extras_cands);
                                tile_files = select_tiles_within_budget(
                                    merged_cands,
                                    route_points,
                                    crate::routing::plan_bbox::effective_max_plan_tiles(),
                                    dirs,
                                );
                                seen = tile_files.iter().cloned().collect();
                            }
                            // Neighbour stems near a densify endpoint (Halland
                            // north of Skåne, Denmark east of SH) must stay even
                            // when the primary stem already filled the tile budget.
                            let mut all_bbox: HashMap<String, [f64; 4]> = tile_bbox
                                .iter()
                                .map(|(k, v)| ((*k).to_string(), *v))
                                .collect();
                            for extra in &extras {
                                if let Some(tiles) = extra.graph_tiles_for(profile) {
                                    for t in tiles {
                                        all_bbox.insert(t.file.clone(), t.bbox);
                                    }
                                }
                            }
                            let mut near_cands: Vec<(String, [f64; 4])> = tile_files
                                .iter()
                                .filter_map(|f| all_bbox.get(f).map(|b| (f.clone(), *b)))
                                .collect();
                            for extra in &extras {
                                if let Some(tiles) = extra.graph_tiles_for(profile) {
                                    for t in tiles {
                                        let near_end = pts.iter().any(|&(lat, lon)| {
                                            let dlat = if lat < t.bbox[0] {
                                                t.bbox[0] - lat
                                            } else if lat > t.bbox[2] {
                                                lat - t.bbox[2]
                                            } else {
                                                0.0
                                            };
                                            let dlon = if lon < t.bbox[1] {
                                                t.bbox[1] - lon
                                            } else if lon > t.bbox[3] {
                                                lon - t.bbox[3]
                                            } else {
                                                0.0
                                            };
                                            dlat.max(dlon) <= 0.30
                                        });
                                        if near_end && seen.insert(t.file.clone()) {
                                            near_cands.push((t.file.clone(), t.bbox));
                                        }
                                    }
                                }
                            }
                            // Re-apply count + byte budget so near_end cannot
                            // unbounded-grow past MAX_PLAN_TILES / MAX_PLAN_TILE_BYTES.
                            tile_files = select_tiles_within_budget(
                                near_cands,
                                route_points,
                                crate::routing::plan_bbox::effective_max_plan_tiles(),
                                dirs,
                            );
                        }
                    }
                }
            }
        }
    }
    log::info!(
        target: "NaviPlan",
        "try_load_graph tile_files={} after primary+extras (budget={})",
        tile_files.len(),
        crate::routing::plan_bbox::effective_max_plan_tiles()
    );

    if !tile_files.is_empty() {
        let mut graphs = vec![load_tiled_graph_files(
            dirs, tile_files, profile, edge_clips,
        )?];
        // City-state packs (e.g. hamburg) are often a single untiled .rkyv.
        // Merge them whether they are the primary stem or an extra — otherwise
        // a hop that starts on a Hamburg densify anchor loads only neighbour
        // tiles and cannot snap (same 6 km miss as a dropped destination pack).
        let primary_tiled = man.graph_tiles_for(profile).is_some_and(|t| !t.is_empty());
        if !primary_tiled {
            if let Some(pp) = graph_path_in_dirs(&man, dirs, profile) {
                graphs.push(load_graph_pack_clips(&pp, profile, edge_clips)?);
            }
        }
        for extra in &extras {
            let tiled = extra
                .graph_tiles_for(profile)
                .is_some_and(|t| !t.is_empty());
            if tiled {
                continue;
            }
            if let Some(ep) = graph_path_in_dirs(extra, dirs, profile) {
                graphs.push(load_graph_pack_clips(&ep, profile, edge_clips)?);
            }
        }
        if graphs.len() == 1 {
            let g = graphs.pop().unwrap();
            return Ok(supplement_pack_ferries_from_pbf(
                g, dirs, &man, &extras, profile, clip_bbox, edge_clips,
            ));
        }
        let merged = merge_tile_graphs(graphs, profile);
        if merged.edges.is_empty() {
            return Err(PackLoadError::Missing);
        }
        return Ok(supplement_pack_ferries_from_pbf(
            merged, dirs, &man, &extras, profile, clip_bbox, edge_clips,
        ));
    }

    let mut graphs = Vec::new();
    let path = graph_path_in_dirs(&man, dirs, profile).ok_or(PackLoadError::Missing)?;
    graphs.push(load_graph_pack_clips(&path, profile, edge_clips)?);
    if !extras.is_empty() {
        let mut extra_candidates = Vec::new();
        let mut extra_seen = HashSet::new();
        for extra in &extras {
            if let Some(tiles) = extra.graph_tiles_for(profile) {
                append_intersecting_tile_files_corridor(
                    &mut extra_candidates,
                    &mut extra_seen,
                    tiles,
                    clip_bbox,
                    segs_ref,
                );
            } else if let Some(ep) = graph_path_in_dirs(extra, dirs, profile) {
                graphs.push(load_graph_pack_clips(&ep, profile, edge_clips)?);
            }
        }
        let extra_files = select_tiles_within_budget(
            extra_candidates,
            route_points,
            crate::routing::plan_bbox::effective_max_plan_tiles(),
            dirs,
        );
        if !extra_files.is_empty() {
            graphs.push(load_tiled_graph_files(
                dirs,
                extra_files,
                profile,
                edge_clips,
            )?);
        }
    }
    let merged = merge_tile_graphs(graphs, profile);
    if merged.edges.is_empty() {
        return Err(PackLoadError::Missing);
    }
    Ok(supplement_pack_ferries_from_pbf(
        merged, dirs, &man, &extras, profile, clip_bbox, edge_clips,
    ))
}

/// When published packs were baked without `route=ferry` ways, merge a ferry-only
/// overlay from on-disk Geofabrik `.osm.pbf` extracts so water hops (Fehmarn)
/// stay connected until packs are rebaked.
fn supplement_pack_ferries_from_pbf(
    graph: RouteGraph,
    dirs: &[&Path],
    primary: &NaviManifest,
    extras: &[NaviManifest],
    profile: RoutingProfile,
    clip_bbox: Option<[f64; 4]>,
    edge_clips: Option<&[[f64; 4]]>,
) -> RouteGraph {
    // Short `ferry=yes` approach roads must not skip the overlay — only a
    // real water-crossing length counts as coverage (Fehmarn ~19 km).
    if graph
        .edges
        .iter()
        .any(|e| e.is_ferry && e.length_m >= 2_000.0)
    {
        return graph;
    }
    let bbox = clip_bbox.or_else(|| {
        edge_clips.and_then(|clips| {
            let mut iter = clips.iter();
            let first = *iter.next()?;
            let mut out = first;
            for s in iter {
                out[0] = out[0].min(s[0]);
                out[1] = out[1].min(s[1]);
                out[2] = out[2].max(s[2]);
                out[3] = out[3].max(s[3]);
            }
            Some(out)
        })
    });
    let Some(bbox) = bbox else {
        return graph;
    };
    // Modest pad so ferry terminals just outside the corridor band still load.
    let bbox = expand_bbox_deg(bbox, 0.05);
    let mut stems = Vec::with_capacity(1 + extras.len());
    stems.push(primary.stem.clone());
    for e in extras {
        if !stems.iter().any(|s| s == &e.stem) {
            stems.push(e.stem.clone());
        }
    }
    let mut overlays = Vec::new();
    for stem in &stems {
        let Some(home) = home_dir_for_stem(dirs, stem) else {
            continue;
        };
        let pbf = home.join(format!("{stem}.osm.pbf"));
        if !pbf.is_file() {
            continue;
        }
        match RouteGraph::build_ferry_overlay_from_pbf(&pbf, profile, bbox) {
            Ok(fg) if fg.edges.iter().any(|e| e.is_ferry) => {
                log::info!(
                    target: "NaviPlan",
                    "ferry_overlay stem={stem} ferry_edges={} nodes={}",
                    fg.edges.iter().filter(|e| e.is_ferry).count(),
                    fg.nodes.len()
                );
                overlays.push(fg);
            }
            Ok(_) => {}
            Err(e) => {
                log::warn!(
                    target: "NaviPlan",
                    "ferry_overlay stem={stem} failed: {e:#}"
                );
            }
        }
    }
    if overlays.is_empty() {
        return graph;
    }
    let mut parts = Vec::with_capacity(1 + overlays.len());
    parts.push(graph);
    parts.extend(overlays);
    merge_tile_graphs(parts, profile)
}

/// Choose the Ready manifest for planning: prefer a stem whose region covers
/// the first route point when the PBF stem does not (chunked long-trip hops).
fn pick_primary_manifest<'a>(
    dirs: &[&'a Path],
    pbf_stem: &str,
    route_points: Option<&[(f64, f64)]>,
) -> Result<(String, NaviManifest, &'a Path), PackLoadError> {
    let primary_dir = home_dir_for_stem(dirs, pbf_stem).ok_or(PackLoadError::Missing)?;
    let default = load_ready_manifest(primary_dir, pbf_stem)?;
    let Some(pts) = route_points else {
        return Ok((pbf_stem.to_string(), default, primary_dir));
    };
    if pts.is_empty() {
        return Ok((pbf_stem.to_string(), default, primary_dir));
    }
    let (lat, lon) = pts[0];
    // Prefer Admin/PIP leaf over catalog AABB. Country extracts (Finland) spill
    // over eastern Finnmark and have a smaller AABB than Nord-Norge, so the
    // old "smallest covering bbox" pick made Finland primary for Bugøynes hops
    // and dropped the real Norwegian exit network after same-leaf extras.clear.
    if let Some(pip_path) = crate::long_trip::region_containing(lat, lon, None) {
        let leaf = pip_path.rsplit('/').next().unwrap_or(pip_path);
        let pip_stem = format!("{leaf}-latest");
        if let Some(home) = home_dir_for_stem(dirs, &pip_stem) {
            if let Ok(man) = load_ready_manifest(home, &pip_stem) {
                if stem_pack_ready(home, &man) {
                    return Ok((man.stem.clone(), man, home));
                }
            }
        }
    }
    if let Some(path) = pbf_stem_to_geofabrik_path(pbf_stem) {
        if let Some(region) = region_bbox(&path) {
            if crate::routing::basemap::bbox_covers_point(region, lat, lon) {
                return Ok((pbf_stem.to_string(), default, primary_dir));
            }
        }
    }
    // Scan Ready manifests for a covering stem; pick the smallest covering bbox.
    let mut best: Option<(f64, String, NaviManifest, &'a Path)> = None;
    let mut seen = HashSet::new();
    for data_dir in dirs {
        let Ok(entries) = fs::read_dir(data_dir) else {
            continue;
        };
        for ent in entries.flatten() {
            let name = ent.file_name();
            let name = name.to_string_lossy();
            let Some(stem) = name.strip_suffix(".navi-manifest.json") else {
                continue;
            };
            if !seen.insert(stem.to_string()) {
                continue;
            }
            let Some(home) = home_dir_for_stem(dirs, stem) else {
                continue;
            };
            let Ok(man) = load_ready_manifest(home, stem) else {
                continue;
            };
            if !stem_pack_ready(home, &man) {
                continue;
            }
            let Some(path) = pbf_stem_to_geofabrik_path(&man.stem) else {
                continue;
            };
            let Some(region) = region_bbox(&path) else {
                continue;
            };
            if !crate::routing::basemap::bbox_covers_point(region, lat, lon) {
                continue;
            }
            let area = (region[2] - region[0]).max(0.0) * (region[3] - region[1]).max(0.0);
            match &best {
                Some((a, _, _, _)) if *a <= area => {}
                _ => best = Some((area, man.stem.clone(), man, home)),
            }
        }
    }
    if let Some((_, stem, man, home)) = best {
        return Ok((stem, man, home));
    }
    Ok((pbf_stem.to_string(), default, primary_dir))
}

/// Key for deduplicating the same physical edge repeated on adjacent tile boundaries.
/// Must not collapse parallel edges that share endpoints (indexed packs use
/// `src-tgt` string ids that collide for those pairs).
fn graph_edge_tile_merge_key(edge: &GraphEdge) -> (i64, i64, u64, u64, u64, u64, u64) {
    (
        edge.source.0,
        edge.target.0,
        edge.length_m.to_bits(),
        edge.start_lat.to_bits(),
        edge.start_lon.to_bits(),
        edge.end_lat.to_bits(),
        edge.end_lon.to_bits(),
    )
}

/// Merge tile-local graphs into one routable graph (deterministic order).
pub fn merge_tile_graphs(graphs: Vec<RouteGraph>, profile: RoutingProfile) -> RouteGraph {
    let mut nodes = HashMap::new();
    let mut edges = Vec::new();
    let mut seen_edge_keys = HashSet::new();
    for g in graphs {
        for (id, node) in g.nodes {
            nodes.insert(id, node);
        }
        for e in g.edges {
            if seen_edge_keys.insert(graph_edge_tile_merge_key(&e)) {
                edges.push(e);
            }
        }
    }
    RouteGraph::from_parts(nodes, edges, profile)
}

fn union_bboxes(segs: &[[f64; 4]]) -> Option<[f64; 4]> {
    let mut iter = segs.iter();
    let first = *iter.next()?;
    let mut out = first;
    for s in iter {
        out[0] = out[0].min(s[0]);
        out[1] = out[1].min(s[1]);
        out[2] = out[2].max(s[2]);
        out[3] = out[3].max(s[3]);
    }
    Some(out)
}

fn expand_bbox_deg(b: [f64; 4], pad: f64) -> [f64; 4] {
    [b[0] - pad, b[1] - pad, b[2] + pad, b[3] + pad]
}

fn load_tiled_graph_files(
    dirs: &[&Path],
    mut tile_files: Vec<String>,
    profile: RoutingProfile,
    clips: Option<&[[f64; 4]]>,
) -> Result<RouteGraph, PackLoadError> {
    if tile_files.is_empty() {
        return Err(PackLoadError::Missing);
    }
    tile_files.sort();

    // Always sequential + incremental merge: never hold all tile graphs in RAM.
    let total = tile_files.len() as u64;
    let mut merged: Option<RouteGraph> = None;
    for (i, file) in tile_files.iter().enumerate() {
        crate::download::progress::set(
            i as u64,
            Some(total),
            &format!("Loading map tile {}/{}…", i + 1, total),
        );
        log::info!(
            target: "NaviPlan",
            "load_tiled_graph file={file} ({}/{})",
            i + 1,
            total
        );
        let path = resolve_pack_file(dirs, file).ok_or(PackLoadError::Missing)?;
        let g = load_graph_pack_clips(&path, profile, clips)?;
        if g.edges.is_empty() && g.nodes.is_empty() {
            continue;
        }
        let t_merge = std::time::Instant::now();
        merged = Some(match merged {
            None => g,
            Some(acc) => merge_tile_graphs(vec![acc, g], profile),
        });
        if let Some(ref m) = merged {
            log::info!(
                target: "NaviPlan",
                "load_tiled_graph merged after {}/{} edges={} nodes={} merge_ms={}",
                i + 1,
                total,
                m.edges.len(),
                m.nodes.len(),
                t_merge.elapsed().as_millis()
            );
        }
    }
    let merged = merged.ok_or(PackLoadError::Missing)?;
    if merged.edges.is_empty() {
        return Err(PackLoadError::Missing);
    }
    log::info!(
        target: "NaviPlan",
        "load_tiled_graph done tiles={} edges={} nodes={}",
        total,
        merged.edges.len(),
        merged.nodes.len()
    );
    Ok(merged)
}

pub fn try_load_poi_barrier_for_plan(
    data_dir: &Path,
    pbf: &Path,
) -> Result<(PoiIndex, DangerBarrierIndex), PackLoadError> {
    try_load_poi_barrier_for_plan_bbox(data_dir, pbf, None)
}

/// Load POI/barrier packs for the planning stem, and when `bbox` spills outside
/// that stem's region, also merge Ready packs from other installed stems that
/// intersect the corridor (same gate as graph tile multi-stem load).
pub fn try_load_poi_barrier_for_plan_bbox(
    data_dir: &Path,
    pbf: &Path,
    bbox: Option<[f64; 4]>,
) -> Result<(PoiIndex, DangerBarrierIndex), PackLoadError> {
    let stem = planning_stem(pbf)?;
    let man = load_ready_manifest(data_dir, &stem)?;
    match status_for_planning_pbf(data_dir, pbf, &man)? {
        PackStatus::Ready => {}
        PackStatus::Missing => return Err(PackLoadError::Missing),
        PackStatus::StalePbf => return Err(PackLoadError::Stale),
        PackStatus::VersionMismatch => return Err(PackLoadError::VersionMismatch),
    }
    let (mut poi, mut barriers) = load_poi_barrier_pack(&man.poi_barrier_path(data_dir))?;
    if corridor_needs_extra_stems(&stem, bbox) {
        if let Some(b) = bbox {
            // Cap extras: full POI packs are 30–90 MB on disk and inflate peak
            // RSS while the route graph is still live (4 GB Automotive).
            let mut extras = extra_corridor_manifests(data_dir, &stem, b);
            extras.truncate(1);
            for extra in extras {
                let path = extra.poi_barrier_path(data_dir);
                if !path.is_file() {
                    continue;
                }
                let Ok((epoi, ebar)) = load_poi_barrier_pack(&path) else {
                    continue;
                };
                poi.extend_from(&epoi);
                barriers.merge(ebar);
            }
        }
    }
    Ok((poi, barriers))
}

/// Load a single Ready POI/barrier pack whose region covers `lat,lon` (smallest
/// covering bbox). Used by chunked long-trip soft-break finalization so we never
/// merge many region-wide POI packs while a route graph is still live (4 GB).
pub fn try_load_poi_pack_covering_point(
    data_dir: &Path,
    lat: f64,
    lon: f64,
) -> Result<(PoiIndex, DangerBarrierIndex), PackLoadError> {
    try_load_poi_pack_covering_point_with_pack_dirs(data_dir, &[], lat, lon)
}

/// Like [`try_load_poi_pack_covering_point`], but also searches [pack_dirs]
/// (e.g. `files/long-trip-packs` or a removable volume pack root) for Ready
/// manifests — same multi-dir roots as
/// [`try_load_graph_for_plan_corridor_with_pack_dirs`].
pub fn try_load_poi_pack_covering_point_with_pack_dirs(
    data_dir: &Path,
    pack_dirs: &[PathBuf],
    lat: f64,
    lon: f64,
) -> Result<(PoiIndex, DangerBarrierIndex), PackLoadError> {
    let mut owned: Vec<PathBuf> = Vec::new();
    for p in pack_dirs {
        if p.as_os_str().is_empty() {
            continue;
        }
        if p.is_dir() && !owned.iter().any(|x| x == p) {
            owned.push(p.clone());
        }
    }
    if !owned.iter().any(|x| x.as_path() == data_dir) {
        owned.push(data_dir.to_path_buf());
    }
    let dirs: Vec<&Path> = owned.iter().map(|p| p.as_path()).collect();
    try_load_poi_pack_covering_point_dirs(&dirs, lat, lon)
}

fn try_load_poi_pack_covering_point_dirs(
    dirs: &[&Path],
    lat: f64,
    lon: f64,
) -> Result<(PoiIndex, DangerBarrierIndex), PackLoadError> {
    // Smallest covering bbox across all search roots (same selection as the
    // single-dir scan, extended over pack_dirs + data_dir).
    let mut best: Option<(f64, PathBuf, NaviManifest)> = None;
    for data_dir in dirs {
        let Ok(entries) = fs::read_dir(data_dir) else {
            continue;
        };
        for ent in entries.flatten() {
            let name = ent.file_name();
            let name = name.to_string_lossy();
            let Some(_stem) = name.strip_suffix(".navi-manifest.json") else {
                continue;
            };
            let Ok(man) = NaviManifest::load(&ent.path()) else {
                continue;
            };
            if !stem_pack_ready(data_dir, &man) {
                continue;
            }
            let Some(path) = pbf_stem_to_geofabrik_path(&man.stem) else {
                continue;
            };
            let Some(region) = region_bbox(&path) else {
                continue;
            };
            if !crate::routing::basemap::bbox_covers_point(region, lat, lon) {
                continue;
            }
            let area = (region[2] - region[0]).max(0.0) * (region[3] - region[1]).max(0.0);
            if best.as_ref().is_none_or(|(ba, _, _)| area < *ba) {
                best = Some((area, data_dir.to_path_buf(), man));
            }
        }
    }
    let Some((_, home, man)) = best else {
        return Err(PackLoadError::Missing);
    };
    load_poi_barrier_pack(&man.poi_barrier_path(&home))
}

/// Prefer indexed wetland pack when present and valid; else `Err` → PBF fallback.
///
/// Region-scale packs may store wetland as spatial tiles (`wetland_tiles`); those
/// are merged for the plan bbox. Monolith corridors use a single `wetland_file`.
/// When the corridor bbox leaves the planning stem's region, intersecting wetland
/// tiles/files from other Ready stems are included in the same merge pass.
pub fn try_load_wetland_for_plan(
    data_dir: &Path,
    pbf: &Path,
    bbox: Option<[f64; 4]>,
) -> Result<WetlandIndex, PackLoadError> {
    let stem = planning_stem(pbf)?;
    let man = load_ready_manifest(data_dir, &stem)?;
    match status_for_planning_pbf(data_dir, pbf, &man)? {
        PackStatus::Ready => {}
        PackStatus::Missing => return Err(PackLoadError::Missing),
        PackStatus::StalePbf => return Err(PackLoadError::Stale),
        PackStatus::VersionMismatch => return Err(PackLoadError::VersionMismatch),
    }
    if man.wetland_format_version != WETLAND_FORMAT_VERSION {
        return Err(PackLoadError::VersionMismatch);
    }

    let mut manifests = vec![man];
    if corridor_needs_extra_stems(&stem, bbox) {
        if let Some(b) = bbox {
            for extra in extra_corridor_manifests(data_dir, &stem, b) {
                if extra.wetland_format_version == WETLAND_FORMAT_VERSION {
                    manifests.push(extra);
                }
            }
        }
    }

    let mut seen = HashSet::new();
    let mut tile_files = Vec::new();
    let mut monolith_paths = Vec::new();
    for m in &manifests {
        if m.uses_wetland_tiles() {
            append_intersecting_tile_files(&mut tile_files, &mut seen, m.wetland_tiles(), bbox);
        } else if let Some(path) = m.wetland_path(data_dir) {
            if path.is_file() {
                monolith_paths.push(path);
            }
        }
    }

    if tile_files.is_empty() && monolith_paths.is_empty() {
        return Err(PackLoadError::Missing);
    }

    let mut merged = FlatWetlandPack::empty();
    tile_files.sort();
    for file in &tile_files {
        let path = data_dir.join(file);
        let mmap = map_file(&path)?;
        check_preamble(&mmap, MAGIC_WETLAND, WETLAND_FORMAT_VERSION)?;
        let body = &mmap[archive_payload_offset()..];
        let archived = rkyv::access::<ArchivedFlatWetlandPack, RkyvError>(body)
            .map_err(|e| PackLoadError::Rkyv(e.to_string()))?;
        let pack: FlatWetlandPack = rkyv::deserialize::<FlatWetlandPack, RkyvError>(archived)
            .map_err(|e| PackLoadError::Rkyv(e.to_string()))?;
        merged.extend_from(&pack);
    }

    let mut index = merged.to_wetland_index(bbox);
    for path in &monolith_paths {
        let w = load_wetland_pack(path, bbox)?;
        let mut parts = index.rings_as_parts();
        parts.extend(w.rings_as_parts());
        index = WetlandIndex::from_parts(parts);
    }
    Ok(index)
}

#[cfg(test)]
mod select_tiles_budget_tests {
    use super::{select_tiles_within_budget, tile_bboxes_adjacent, tile_counts_as_endpoint_cover};
    use std::fs;
    use std::path::Path;

    #[test]
    fn country_spill_tile_does_not_cover_foreign_leaf_endpoint() {
        // Denmark country AABB covers western Skåne; with Skåne Ready it must
        // not satisfy endpoint coverage so budget retention keeps Skåne tiles.
        let ready = vec![
            (
                "europe/denmark".to_string(),
                [54.44065_f64, 7.7011, 58.06239, 15.65449],
            ),
            (
                "europe/sweden/skane".to_string(),
                [55.32_f64, 12.45, 56.50, 14.60],
            ),
        ];
        let skane_pt = (55.91_f64, 13.525_f64);
        let dk_tile_bbox = [54.5_f64, 10.0, 56.5, 14.0]; // spills into Skåne
        assert!(
            !tile_counts_as_endpoint_cover(
                "denmark-latest.navi-graph-car.t0_0.rkyv",
                dk_tile_bbox,
                skane_pt.0,
                skane_pt.1,
                &ready,
            ),
            "DK country tile must not count as covering Skåne endpoint"
        );
        let skane_tile_bbox = [55.5_f64, 13.0, 56.2, 14.0];
        assert!(
            tile_counts_as_endpoint_cover(
                "skane-latest.navi-graph-car.t0_0.rkyv",
                skane_tile_bbox,
                skane_pt.0,
                skane_pt.1,
                &ready,
            ),
            "Skåne leaf tile must cover Skåne endpoint"
        );
    }

    /// Ostlandet-like 2×3 car tile grid covering R4b / Espa corridors.
    fn ostlandet_grid() -> Vec<(String, [f64; 4])> {
        vec![
            (
                "ostlandet-latest.navi-graph-car.t1_3.rkyv".into(),
                [59.8216129, 9.95466715, 60.7594758, 10.813263566666668],
            ),
            (
                "ostlandet-latest.navi-graph-car.t1_4.rkyv".into(),
                [
                    59.8216129,
                    10.813263566666668,
                    60.7594758,
                    11.671859983333333,
                ],
            ),
            (
                "ostlandet-latest.navi-graph-car.t1_5.rkyv".into(),
                [59.8216129, 11.671859983333334, 60.7594758, 12.5304564],
            ),
            (
                "ostlandet-latest.navi-graph-car.t2_3.rkyv".into(),
                [60.7594758, 9.95466715, 61.6973387, 10.813263566666668],
            ),
            (
                "ostlandet-latest.navi-graph-car.t2_4.rkyv".into(),
                [
                    60.7594758,
                    10.813263566666668,
                    61.6973387,
                    11.671859983333333,
                ],
            ),
            (
                "ostlandet-latest.navi-graph-car.t2_5.rkyv".into(),
                [60.7594758, 11.671859983333334, 61.6973387, 12.5304564],
            ),
        ]
    }

    fn touch_sized(dir: &Path, name: &str, bytes: usize) {
        fs::write(dir.join(name), vec![0u8; bytes]).expect("touch tile stub");
    }

    #[test]
    fn tile_bboxes_adjacent_detects_shared_grid_edge() {
        let t1_3 = [59.8216129, 9.95466715, 60.7594758, 10.813263566666668];
        let t2_3 = [60.7594758, 9.95466715, 61.6973387, 10.813263566666668];
        let t2_4 = [
            60.7594758,
            10.813263566666668,
            61.6973387,
            11.671859983333333,
        ];
        let t2_5 = [60.7594758, 11.671859983333334, 61.6973387, 12.5304564];
        assert!(tile_bboxes_adjacent(t1_3, t2_3));
        assert!(tile_bboxes_adjacent(t2_3, t2_4));
        assert!(tile_bboxes_adjacent(t2_4, t2_5));
        assert!(
            !tile_bboxes_adjacent(t1_3, t2_5),
            "diagonal-only must not count as adjacent"
        );
    }

    /// R4b: chord samples hit only t1_3+t2_4; bridge fill must still keep t2_3
    /// (western E6 / Vestheim). Pre-fix nearest-to-endpoint fill kept t2_5 instead.
    #[test]
    fn r4b_bridge_keeps_western_e6_t2_3() {
        let dir = tempfile::tempdir().expect("tmpdir");
        // Relative sizes mirror device Ostlandet car tiles (proportional bytes).
        for (name, bytes) in [
            ("ostlandet-latest.navi-graph-car.t1_3.rkyv", 112usize),
            ("ostlandet-latest.navi-graph-car.t1_4.rkyv", 106),
            ("ostlandet-latest.navi-graph-car.t1_5.rkyv", 37),
            ("ostlandet-latest.navi-graph-car.t2_3.rkyv", 60),
            ("ostlandet-latest.navi-graph-car.t2_4.rkyv", 42),
            ("ostlandet-latest.navi-graph-car.t2_5.rkyv", 17),
        ] {
            touch_sized(dir.path(), name, bytes);
        }
        let pts = [
            (60.7278503, 10.6109705), // start
            (60.821469, 11.200060),   // via
            (61.1638011, 11.4539336), // goal
        ];
        let selected = select_tiles_within_budget(ostlandet_grid(), Some(&pts), 6, &[dir.path()]);
        assert!(
            selected
                .iter()
                .any(|f| f.contains("navi-graph-car.t2_3.rkyv")),
            "t2_3 (western E6) must be selected for R4b; got {selected:?}"
        );
        assert!(
            selected.len() <= 6,
            "must stay within MAX_PLAN_TILES; got {}",
            selected.len()
        );
        // Still only four tiles for this OD (samples 2 + bridge 2) — swap in
        // t2_3 rather than raising the tile count / RSS ceiling.
        assert_eq!(
            selected.len(),
            4,
            "R4b should stay at 4 tiles (2 samples + 2 bridges); got {selected:?}"
        );
    }

    /// Espa→Atnbrua chord already samples t2_3; selection must keep it under budget.
    #[test]
    fn espa_atnbrua_keeps_t2_3() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let mut cands = ostlandet_grid();
        // Espa span also pulls row-3 / col-2 neighbours.
        cands.extend([
            (
                "ostlandet-latest.navi-graph-car.t2_2.rkyv".into(),
                [60.7594758, 9.096070733333336, 61.6973387, 9.95466715],
            ),
            (
                "ostlandet-latest.navi-graph-car.t3_2.rkyv".into(),
                [61.6973387, 9.096070733333336, 62.6352016, 9.95466715],
            ),
            (
                "ostlandet-latest.navi-graph-car.t3_3.rkyv".into(),
                [61.6973387, 9.95466715, 62.6352016, 10.813263566666668],
            ),
            (
                "ostlandet-latest.navi-graph-car.t3_4.rkyv".into(),
                [
                    61.6973387,
                    10.813263566666668,
                    62.6352016,
                    11.671859983333333,
                ],
            ),
        ]);
        for (name, _) in &cands {
            touch_sized(dir.path(), name, 40);
        }
        let pts = [(60.5621914, 11.2561239), (61.8512500, 10.2338420)];
        let selected = select_tiles_within_budget(cands, Some(&pts), 6, &[dir.path()]);
        assert!(
            selected
                .iter()
                .any(|f| f.contains("navi-graph-car.t2_3.rkyv")),
            "Espa→Atnbrua must keep t2_3; got {selected:?}"
        );
        assert!(selected.len() <= 6);
    }
}

#[cfg(test)]
mod merge_tile_graphs_tests {
    use super::*;
    use crate::routing::graph::{GraphEdge, RouteGraph, RoutingProfile, SurfaceQuality};
    use geo_types::Coord;
    use osm4routing::{Node, NodeId};

    fn stub_edge(id: &str, src: i64, tgt: i64, len: f64, hw: &str) -> GraphEdge {
        GraphEdge {
            id: id.into(),
            source: NodeId(src),
            target: NodeId(tgt),
            length_m: len,
            base_weight: len,
            eco_weight: Some(len),
            start_lat: 60.0,
            start_lon: 11.0,
            end_lat: 60.0,
            end_lon: 11.001,
            shape: Vec::new(),
            highway: Some(hw.into()),
            maxspeed_kmh: None,
            maxspeed_practical_kmh: None,
            maxspeed_advisory_kmh: None,
            maxspeed_type: None,
            maxspeed_variable: false,
            minspeed_kmh: None,
            name: None,
            road_ref: None,
            is_motorroad: false,
            is_expressway: false,
            is_oneway: false,
            lanes: None,
            maxweight_t: None,
            maxaxleload_t: None,
            maxbogieweight_t: None,
            maxheight_m: None,
            maxwidth_m: None,
            maxlength_m: None,
            is_toll: false,
            is_ferry: false,
            is_tunnel: false,
            is_boardwalk_crossing: false,
            is_roundabout: false,
            motor_vehicle_conditional: None,
            access_conditional: None,
            maxspeed_conditional: None,
            access_forbidden: false,
            surface_quality: SurfaceQuality::Good,
        }
    }

    #[test]
    fn merge_keeps_parallel_edges_with_colliding_string_ids() {
        let mut nodes = HashMap::new();
        for id in [1_i64, 2] {
            nodes.insert(
                NodeId(id),
                Node {
                    id: NodeId(id),
                    coord: Coord { x: 11.0, y: 60.0 },
                    uses: 2,
                },
            );
        }
        let g = RouteGraph::from_parts(
            nodes,
            vec![
                stub_edge("1-2", 1, 2, 200.0, "service"),
                stub_edge("1-2", 1, 2, 64.6, "secondary"),
            ],
            RoutingProfile::Car,
        );
        let merged = merge_tile_graphs(vec![g], RoutingProfile::Car);
        assert_eq!(merged.edges.len(), 2, "parallel edges must survive merge");
    }

    #[test]
    fn merge_dedupes_identical_tile_boundary_edge() {
        let mut nodes = HashMap::new();
        nodes.insert(
            NodeId(1),
            Node {
                id: NodeId(1),
                coord: Coord { x: 11.0, y: 60.0 },
                uses: 2,
            },
        );
        nodes.insert(
            NodeId(2),
            Node {
                id: NodeId(2),
                coord: Coord { x: 11.001, y: 60.0 },
                uses: 2,
            },
        );
        let edge = stub_edge("1-2-0", 1, 2, 100.0, "secondary");
        let g1 = RouteGraph::from_parts(nodes.clone(), vec![edge.clone()], RoutingProfile::Car);
        let g2 = RouteGraph::from_parts(nodes, vec![edge], RoutingProfile::Car);
        let merged = merge_tile_graphs(vec![g1, g2], RoutingProfile::Car);
        assert_eq!(merged.edges.len(), 1, "boundary duplicate must dedupe");
    }
}

#[cfg(test)]
mod multi_stem_corridor_tests {
    use super::{
        bbox_contained, corridor_needs_extra_for_endpoint_leaves, corridor_needs_extra_stems,
    };
    use std::fs;

    #[test]
    fn bbox_contained_requires_full_inclusion() {
        let outer = [58.5, 7.5, 62.8, 13.5];
        assert!(bbox_contained([60.0, 10.0, 61.0, 11.0], outer));
        assert!(!bbox_contained([60.0, 10.0, 63.2, 11.0], outer)); // spills north
        assert!(!bbox_contained([60.0, 6.0, 61.0, 11.0], outer)); // spills west
    }

    #[test]
    fn corridor_needs_extra_only_when_bbox_leaves_primary_region() {
        // Entirely inside Ostlandet → single-stem (no regression).
        assert!(!corridor_needs_extra_stems(
            "ostlandet-latest",
            Some([60.0, 10.0, 61.0, 11.0]),
        ));
        // Raufoss→Aga class pad leaves Ostlandet → multi-stem.
        assert!(corridor_needs_extra_stems(
            "ostlandet-latest",
            Some([58.9, 5.2, 62.1, 12.0]),
        ));
        // No bbox → never pull extras.
        assert!(!corridor_needs_extra_stems("ostlandet-latest", None));
    }

    #[test]
    fn corridor_needs_extra_when_endpoint_in_foreign_ready_leaf() {
        // Denmark AABB contains Skåne; bbox-only gate would skip extras. A Ready
        // Skåne leaf covering the destination must still force multi-stem load.
        let dir = tempfile::tempdir().expect("tmpdir");
        fs::write(
            dir.path().join("skane-latest.navi-manifest.json"),
            r#"{"schema":1,"stem":"skane-latest","pbf_filename":"skane-latest.osm.pbf","graph_files":{},"graph_format_version":9}"#,
        )
        .unwrap();
        let pts = [(55.626_f64, 12.144_f64), (55.910_f64, 13.525_f64)];
        assert!(
            corridor_needs_extra_for_endpoint_leaves("denmark-latest", Some(&pts), &[dir.path()]),
            "Skåne Ready leaf covering hop end must force extras under Denmark primary"
        );
        // Same-stem family: Ostlandet primary with only Ostlandet leaf present.
        fs::write(
            dir.path().join("ostlandet-latest.navi-manifest.json"),
            r#"{"schema":1,"stem":"ostlandet-latest","pbf_filename":"ostlandet-latest.osm.pbf","graph_files":{},"graph_format_version":9}"#,
        )
        .unwrap();
        let no_pts = [(60.5_f64, 10.5_f64), (61.0_f64, 11.0_f64)];
        assert!(
            !corridor_needs_extra_for_endpoint_leaves(
                "ostlandet-latest",
                Some(&no_pts),
                &[dir.path()]
            ),
            "in-stem Ostlandet endpoints must not force foreign extras"
        );
    }
}

#[cfg(test)]
mod fingerprint_pbf_tests {
    use super::*;
    use crate::routing::graph::RoutingProfile;
    use crate::routing::indexed::manifest::{GraphTileEntry, GRAPH_PROFILE_CAR};
    use std::collections::BTreeMap;

    fn man_for(pbf_filename: &str) -> NaviManifest {
        NaviManifest {
            schema: NaviManifest::SCHEMA,
            stem: "ostlandet-latest".into(),
            pbf_filename: pbf_filename.into(),
            pbf_size_bytes: 1,
            pbf_modified_unix_secs: 1,
            graph_files: BTreeMap::new(),
            graph_tiles: BTreeMap::new(),
            graph_format_version: GRAPH_FORMAT_VERSION,
            poi_barrier_file: "ostlandet-latest.navi-poi-barrier.rkyv".into(),
            poi_barrier_format_version: POI_BARRIER_FORMAT_VERSION,
            wetland_file: None,
            wetland_tiles: Vec::new(),
            wetland_format_version: 0,
            has_delta_h: false,
            elev_dir: None,
        }
    }

    #[test]
    fn uses_data_dir_copy_when_filename_matches() {
        let man = man_for("ostlandet-latest.osm.pbf");
        let packed = fingerprint_pbf_for_packs(
            Path::new("/data/user/0/no.navi.app/files"),
            Path::new("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf"),
            &man,
        )
        .expect("same logical extract");
        assert_eq!(
            packed,
            PathBuf::from("/data/user/0/no.navi.app/files/ostlandet-latest.osm.pbf")
        );
    }

    #[test]
    fn rejects_different_logical_extract() {
        let man = man_for("ostlandet-latest.osm.pbf");
        let err = fingerprint_pbf_for_packs(
            Path::new("/data/user/0/no.navi.app/files"),
            Path::new("/data/local/tmp/navi_fixtures/espa-atnbrufossen-corridor.osm.pbf"),
            &man,
        )
        .unwrap_err();
        assert!(matches!(err, PackLoadError::Missing));
    }

    #[test]
    fn truck_graph_tiles_fall_back_to_car_when_truck_key_absent() {
        let mut man = man_for("ostlandet-latest.osm.pbf");
        man.graph_tiles.insert(
            GRAPH_PROFILE_CAR.into(),
            vec![GraphTileEntry {
                file: "ostlandet-latest.navi-graph-car.t0_0.rkyv".into(),
                bbox: [60.0, 10.0, 61.0, 11.0],
            }],
        );
        let truck = man
            .graph_tiles_for(RoutingProfile::Truck)
            .expect("truck must fall back to car tiles");
        assert_eq!(truck.len(), 1);
        assert!(truck[0].file.contains("navi-graph-car"));
        assert!(
            man.graph_tiles_for(RoutingProfile::Foot).is_none(),
            "foot must not fall back to car"
        );
    }
}
