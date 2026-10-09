//! Load + validate indexed packs (never interpret mismatched versions).

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::Instant;

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
use crate::routing::graph::{
    drop_pack_edges_replaced_by_overlay_ferry, is_construction_or_proposed_highway, GraphEdge,
    RouteGraph, RoutingProfile,
};
use crate::routing::pbf_extract::{pbf_is_real_extract, MIN_REAL_PBF_BYTES};
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

/// Ready check for a stem under [profile] (multi-stem corridor / primary re-home).
///
/// Uses profile-scoped graph files so a car-only install (foot tiles omitted)
/// still counts as Ready when planning car — otherwise Trøndelag/Nord-Norge
/// never join Ostlandet corridors on device.
///
/// [`PackStatus::Outdated`] (accepted format behind preferred) still counts as
/// usable for planning — Tools offers an optional update; never block routes.
fn stem_pack_ready(data_dir: &Path, man: &NaviManifest, profile: RoutingProfile) -> bool {
    let usable = |s: PackStatus| matches!(s, PackStatus::Ready | PackStatus::Outdated);
    if server_install_present(data_dir, &man.stem) {
        return usable(man.status_pack_files_for_profile(data_dir, profile));
    }
    let packed = data_dir.join(&man.pbf_filename);
    if packed.is_file() {
        // Fingerprint match still uses full-status for the planning PBF; neighbour
        // stems only need the active profile on disk.
        if usable(man.status_for_pbf(data_dir, &packed)) {
            return true;
        }
        return usable(man.status_pack_files_for_profile(data_dir, profile));
    }
    usable(man.status_pack_files_for_profile(data_dir, profile))
}

/// Geofabrik region ids that already have a Ready pack for [profile] under [dirs].
fn installed_ready_region_ids(dirs: &[&Path], profile: RoutingProfile) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for d in dirs {
        let Ok(entries) = fs::read_dir(d) else {
            continue;
        };
        for ent in entries.flatten() {
            let name = ent.file_name();
            let name = name.to_string_lossy();
            let Some(stem) = name.strip_suffix(".navi-manifest.json") else {
                continue;
            };
            let Ok(man) = load_ready_manifest(d, stem) else {
                continue;
            };
            if !stem_pack_ready(d, &man, profile) {
                continue;
            }
            let Some(path) = pbf_stem_to_geofabrik_path(stem) else {
                continue;
            };
            if seen.insert(path.clone()) {
                out.push(path);
            }
        }
    }
    out
}

/// Fail-fast list: waypoint regions that lack a Ready pack for [profile].
///
/// Does not load skeletons or scan main-road ends. User-facing names come from
/// `named_missing_regions` after the corridor is chosen. Geometric-band ids are
/// logged separately as candidates and never abort a plan.
pub fn missing_ready_regions_for_trip(
    dirs: &[&Path],
    profile: RoutingProfile,
    route_points: &[(f64, f64)],
) -> Vec<String> {
    if route_points.len() < 2 {
        return Vec::new();
    }
    let installed = installed_ready_region_ids(dirs, profile);
    waypoint_uninstalled_regions(route_points, &installed)
}

/// Geometric-band region ids that lack a Ready pack. Log-only candidates.
/// Never used to block or delay a plan.
pub fn missing_region_candidates_for_trip(
    dirs: &[&Path],
    profile: RoutingProfile,
    route_points: &[(f64, f64)],
) -> Vec<String> {
    if route_points.len() < 2 {
        return Vec::new();
    }
    let installed = installed_ready_region_ids(dirs, profile);
    crate::long_trip::geometric_missing_regions_for_trip(route_points, &installed, None)
        .unwrap_or_default()
}

fn waypoint_uninstalled_regions(route_points: &[(f64, f64)], installed: &[String]) -> Vec<String> {
    let mut missing = Vec::new();
    let mut seen = HashSet::new();
    for &(lat, lon) in route_points {
        let Some(id) = crate::long_trip::region_containing(lat, lon, None)
            .map(|s| s.to_string())
            .or_else(|| {
                crate::routing::suggest_geofabrik_path_for_point(lat, lon).map(|s| s.to_string())
            })
        else {
            continue;
        };
        let covered = installed.iter().any(|inst| {
            inst == &id
                || id.starts_with(&format!("{inst}/"))
                || inst.starts_with(&format!("{id}/"))
        });
        if !covered && seen.insert(id.clone()) {
            missing.push(id);
        }
    }
    missing
}

/// Directory among [dirs] where [stem] packs are Ready for [profile].
///
/// When the same stem exists in more than one pack root (legacy `files/` +
/// `long-trip-packs`), prefer the newest accepted graph format (v9 over v8).
/// Ties keep the first Ready dir in [dirs] order.
fn home_dir_for_stem<'a>(
    dirs: &[&'a Path],
    stem: &str,
    profile: RoutingProfile,
) -> Option<&'a Path> {
    let mut best_ready: Option<(&'a Path, u32)> = None;
    let mut best_any: Option<(&'a Path, u32)> = None;
    for d in dirs {
        let Ok(man) = load_ready_manifest(d, stem) else {
            continue;
        };
        let ver = man.graph_format_version;
        if stem_pack_ready(d, &man, profile) {
            match best_ready {
                Some((_, prev)) if prev >= ver => {}
                _ => best_ready = Some((*d, ver)),
            }
        }
        match best_any {
            Some((_, prev)) if prev >= ver => {}
            _ => best_any = Some((*d, ver)),
        }
    }
    best_ready.or(best_any).map(|(d, _)| d)
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

/// True when catalog path `path` should stay as a 2-point hop extra.
///
/// Admin PIP wins over pack-leaf AABBs. MV's Geofabrik box covers Fehmarn
/// (`54.21, 11.025` is SH) and used to steal the 5-tile budget from DK/SH
/// ferry tiles — overlay then left O/D on different components.
fn extra_leaf_justified_for_hop(path: &str, pts: &[(f64, f64)], segs: Option<&[[f64; 4]]>) -> bool {
    if pts.len() != 2 {
        return true;
    }
    let Some(region) = region_bbox(path) else {
        return false;
    };
    let pip_matches = |pip: &str| {
        pip == path || pip.starts_with(&format!("{path}/")) || path.starts_with(&format!("{pip}/"))
    };
    let pip_a = crate::long_trip::region_containing(pts[0].0, pts[0].1, None);
    let pip_b = crate::long_trip::region_containing(pts[1].0, pts[1].1, None);
    if pip_a.is_some_and(pip_matches) || pip_b.is_some_and(pip_matches) {
        return true;
    }
    for &(lat, lon) in pts {
        if crate::routing::basemap::bbox_covers_point(region, lat, lon) {
            if let Some(pip) = crate::long_trip::region_containing(lat, lon, None) {
                if !pip_matches(pip) {
                    return false;
                }
            }
        }
    }
    segs.is_some_and(|segs| segs.iter().any(|s| bbox_intersects(region, *s)))
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
            // Include country extracts (europe/finland) and leaves
            // (europe/sweden/norrbotten). Skipping country extracts left
            // Finnmark→Lapland hops on nord-norge alone → snap_failed.
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
    // PIP hole: endpoint has no catalog region but a Ready pack AABB covers it.
    for &(lat, lon) in pts {
        if crate::long_trip::region_containing(lat, lon, None).is_some() {
            continue;
        }
        return true;
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
    profile: RoutingProfile,
) -> Vec<NaviManifest> {
    extra_corridor_manifests_segs(&[data_dir], primary_stem, &[bbox], profile)
}

fn extra_corridor_manifests_segs(
    dirs: &[&Path],
    primary_stem: &str,
    segs: &[[f64; 4]],
    profile: RoutingProfile,
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
            let Some(home) = home_dir_for_stem(dirs, stem, profile) else {
                continue;
            };
            let Ok(man) = load_ready_manifest(home, stem) else {
                continue;
            };
            if !stem_pack_ready(home, &man, profile) {
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

#[allow(dead_code)]
fn select_tiles_within_budget(
    candidates: Vec<(String, [f64; 4])>,
    route_points: Option<&[(f64, f64)]>,
    max_tiles: usize,
    dirs: &[&Path],
) -> Vec<String> {
    select_tiles_within_budget_opts(candidates, route_points, max_tiles, dirs, false)
}

/// `fill_to_budget`: after corridor-band disconnect, TripAabb must keep every
/// AABB-intersecting candidate up to `max_tiles`. Chord eighth-samples plus two
/// bridges drop the valley/border tiles that sit between the hop ends.
fn select_tiles_within_budget_opts(
    candidates: Vec<(String, [f64; 4])>,
    route_points: Option<&[(f64, f64)]>,
    max_tiles: usize,
    dirs: &[&Path],
    fill_to_budget: bool,
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
    // Band mode: at most two bridge fillers. TripAabb fallback fills to budget
    // so off-chord valley tiles inside the trip box are not dropped.
    let bridge_cap = if fill_to_budget {
        max_tiles
    } else {
        (selected.len() + 2).min(max_tiles)
    };
    for (name, bbox) in rest {
        if selected.len() >= bridge_cap {
            break;
        }
        if selected_names.insert(name.clone()) {
            selected.push((name, bbox));
        }
    }

    // Stage B: never drop a tile that covers any coarse-path sample. Memory is
    // bounded by splitting long hops (`MAX_PATH_TILES_PER_HOP`), not by truncating
    // the corridor mid-path (that produced weak_ok=false on Finland-scale hops).
    let protect_all_samples = crate::routing::plan_bbox::stage_b_active();
    if selected.len() > max_tiles && !protect_all_samples {
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
    } else if selected.len() > max_tiles && protect_all_samples {
        log::info!(
            target: "NaviPlan",
            "stage_b_tile_budget: keeping {} path tiles above max_tiles={max_tiles} \
             (split hops must bound memory)",
            selected.len()
        );
    }
    // Soft disk-byte budget: drop largest non-essential tiles while endpoints stay
    // covered. Prevents six ~70–130 MB car tiles (~1.1M edges) on densify hops.
    // Stage B: do not drop path-covering tiles for the byte soft-cap either.
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
    while !protect_all_samples && total_bytes(&selected) > max_bytes && selected.len() > 2 {
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

fn all_ready_graph_tiles(dirs: &[&Path], profile: RoutingProfile) -> Vec<(String, [f64; 4])> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for dir in dirs {
        let Ok(rd) = fs::read_dir(dir) else {
            continue;
        };
        for ent in rd.flatten() {
            let path = ent.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let Some(stem) = name.strip_suffix(".navi-manifest.json") else {
                continue;
            };
            let Ok(man) = load_ready_manifest(dir, stem) else {
                continue;
            };
            if !stem_pack_ready(dir, &man, profile) {
                continue;
            }
            let Some(tiles) = man.graph_tiles_for(profile) else {
                continue;
            };
            for t in tiles {
                if seen.insert(t.file.clone()) {
                    out.push((t.file.clone(), t.bbox));
                }
            }
        }
    }
    out
}

/// Stage B: the hop must contain every Ready tile that covers the start, the
/// end, or any coarse-path sample. Tiles are added, never dropped. Fails if
/// no Ready tile covers the hop end.
fn ensure_coarse_path_tiles(
    tile_files: &mut Vec<String>,
    all_ready: &[(String, [f64; 4])],
    pts: &[(f64, f64)],
) -> Result<(), PackLoadError> {
    if !crate::routing::plan_bbox::stage_b_active() || pts.len() < 2 {
        return Ok(());
    }
    let mut have: HashSet<String> = tile_files.iter().cloned().collect();
    for &(lat, lon) in pts {
        for (name, bbox) in all_ready {
            if !crate::routing::basemap::bbox_covers_point(*bbox, lat, lon) {
                continue;
            }
            if have.insert(name.clone()) {
                log::info!(
                    target: "NaviPlan",
                    "stage_b_path_tile_include {name} lat={lat:.5} lon={lon:.5}"
                );
                tile_files.push(name.clone());
            }
        }
    }
    let (elat, elon) = pts[pts.len() - 1];
    let end_covered = all_ready.iter().any(|(name, bbox)| {
        tile_files.iter().any(|f| f == name)
            && crate::routing::basemap::bbox_covers_point(*bbox, elat, elon)
    });
    if !end_covered {
        return Err(PackLoadError::MissingEndTile(format!(
            "{elat:.5},{elon:.5}"
        )));
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum PackLoadError {
    #[error("indexed pack missing or incomplete")]
    Missing,
    #[error("indexed pack stale vs source PBF")]
    Stale,
    #[error("indexed pack version/magic mismatch (rebuild required)")]
    VersionMismatch,
    /// Required corridor region(s) have no Ready pack for the profile.
    /// Geofabrik paths in first-crossing order (e.g. `europe/norway/vestlandet`).
    #[error("missing region packs: {}", .0.join(", "))]
    MissingRegions(Vec<String>),
    /// Ferry overlay sidecar is building in the background; retry the plan.
    /// `(region_label, progress_pct)`.
    #[error("preparing ferry data for {0}")]
    FerryPreparing(String, u8),
    /// Corridor skeleton is building in the background; retry the plan.
    /// `(region_label, progress_pct)`.
    #[error("preparing corridor skeleton for {0}")]
    SkeletonPreparing(String, u8),
    /// Hop graph was selected without a Ready tile that covers the hop end.
    #[error("hop built without end-node tile at {0}")]
    MissingEndTile(String),
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
/// Touch every page so later clip/copy timing excludes cold page-in I/O.
/// Only used when plan-perf timing is enabled (Step 1 pack_load breakdown).
fn touch_mmap_pages(mmap: &Mmap) {
    let bytes = mmap.as_ref();
    let mut i = 0usize;
    let step = 4096usize;
    let mut acc = 0u8;
    while i < bytes.len() {
        acc ^= bytes[i];
        i = i.saturating_add(step);
    }
    if !bytes.is_empty() {
        acc ^= bytes[bytes.len() - 1];
    }
    std::hint::black_box(acc);
}

pub fn load_graph_pack_clips(
    path: &Path,
    profile: RoutingProfile,
    clips: Option<&[[f64; 4]]>,
) -> Result<RouteGraph, PackLoadError> {
    let timing = crate::routing::plan_perf::enabled();
    let t_open = std::time::Instant::now();
    let mmap = map_file(path)?;
    let mmap_ms = t_open.elapsed().as_millis() as u64;
    let bytes = mmap.len() as u64;

    // When profiling, force page-in before validate/copy so those stages are
    // CPU-bound rather than mixed with first-touch I/O.
    let pagein_ms = if timing {
        let t = std::time::Instant::now();
        touch_mmap_pages(&mmap);
        t.elapsed().as_millis() as u64
    } else {
        0
    };

    let t_val = std::time::Instant::now();
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

    let g = match format_version {
        super::graph_pack::GRAPH_FORMAT_VERSION_V8 => {
            let archived = rkyv::access::<ArchivedFlatGraphPackV8, RkyvError>(body)
                .map_err(|e| PackLoadError::Rkyv(e.to_string()))?;
            let validate_ms = t_val.elapsed().as_millis() as u64;
            let t_copy = std::time::Instant::now();
            let g = archived.to_route_graph_clips(profile, clips);
            let copy_ms = t_copy.elapsed().as_millis() as u64;
            if timing {
                crate::routing::plan_perf::add_pack_stage_ms(
                    mmap_ms,
                    pagein_ms,
                    validate_ms,
                    copy_ms,
                    bytes,
                );
                crate::routing::plan_perf::note(
                    "tile_stage",
                    format!(
                        "{file};format={format_version};bytes={bytes};edges={};nodes={};\
                         mmap_ms={mmap_ms};pagein_ms={pagein_ms};validate_ms={validate_ms};\
                         copy_ms={copy_ms};threads=1",
                        g.edges.len(),
                        g.nodes.len()
                    ),
                );
            }
            g
        }
        GRAPH_FORMAT_VERSION => {
            let archived = rkyv::access::<ArchivedFlatGraphPack, RkyvError>(body)
                .map_err(|e| PackLoadError::Rkyv(e.to_string()))?;
            let validate_ms = t_val.elapsed().as_millis() as u64;
            let t_copy = std::time::Instant::now();
            let g = archived.to_route_graph_clips(profile, clips);
            let copy_ms = t_copy.elapsed().as_millis() as u64;
            if timing {
                crate::routing::plan_perf::add_pack_stage_ms(
                    mmap_ms,
                    pagein_ms,
                    validate_ms,
                    copy_ms,
                    bytes,
                );
                crate::routing::plan_perf::note(
                    "tile_stage",
                    format!(
                        "{file};format={format_version};bytes={bytes};edges={};nodes={};\
                         mmap_ms={mmap_ms};pagein_ms={pagein_ms};validate_ms={validate_ms};\
                         copy_ms={copy_ms};threads=1",
                        g.edges.len(),
                        g.nodes.len()
                    ),
                );
            }
            g
        }
        _ => return Err(PackLoadError::VersionMismatch),
    };
    let elapsed_ms = t_open.elapsed().as_millis() as u64;
    log::info!(
        target: "NaviPlan",
        "load_graph_pack_bbox file={file} format={format_version} edges={} nodes={} clips={} elapsed_ms={elapsed_ms} mmap_ms={mmap_ms} pagein_ms={pagein_ms}",
        g.edges.len(),
        g.nodes.len(),
        clips.map(|c| c.len()).unwrap_or(0),
    );
    crate::routing::plan_perf::note(
        "tile_load",
        format!(
            "{file};format={format_version};bytes={bytes};edges={};nodes={};ms={elapsed_ms};mmap=1",
            g.edges.len(),
            g.nodes.len()
        ),
    );
    crate::routing::plan_perf::sample_rss();
    Ok(g)
}

pub fn load_poi_barrier_pack(path: &Path) -> Result<(PoiIndex, DangerBarrierIndex), PackLoadError> {
    load_poi_barrier_pack_bbox(path, None, true)
}

/// Load a POI/barrier pack, optionally clipping records to `bbox`
/// `[min_lat, min_lon, max_lat, max_lon]` during hydrate (avoids keeping a
/// whole-region Ostlandet index in RSS for a narrow corridor plan).
///
/// `include_overnight_buildings`: see [`FlatPoiBarrierPack::to_poi_index_bbox`].
pub fn load_poi_barrier_pack_bbox(
    path: &Path,
    bbox: Option<[f64; 4]>,
    include_overnight_buildings: bool,
) -> Result<(PoiIndex, DangerBarrierIndex), PackLoadError> {
    let t0 = std::time::Instant::now();
    let mmap = map_file(path)?;
    let bytes = mmap.len() as u64;
    check_preamble(&mmap, MAGIC_POI_BARRIER, POI_BARRIER_FORMAT_VERSION)?;
    let body = &mmap[archive_payload_offset()..];
    let archived = rkyv::access::<ArchivedFlatPoiBarrierPack, RkyvError>(body)
        .map_err(|e| PackLoadError::Rkyv(e.to_string()))?;
    let pack: FlatPoiBarrierPack = rkyv::deserialize::<FlatPoiBarrierPack, RkyvError>(archived)
        .map_err(|e| PackLoadError::Rkyv(e.to_string()))?;
    let poi = pack.to_poi_index_bbox(bbox, include_overnight_buildings);
    let barriers = pack.to_barrier_index_bbox(bbox);
    let elapsed_ms = t0.elapsed().as_millis() as u64;
    let file = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    crate::routing::plan_perf::note(
        "poi_barrier_load",
        format!(
            "{file};bytes={bytes};poi={};buildings={};glaciers={};clip={};ms={elapsed_ms}",
            poi.len(),
            poi.overnight_buildings().len(),
            barriers.glacier_ring_count(),
            bbox.is_some() as u8
        ),
    );
    log::info!(
        target: "NaviPlan",
        "load_poi_barrier_pack file={file} bytes={bytes} poi={} buildings={} glaciers={} clip={} elapsed_ms={elapsed_ms}",
        poi.len(),
        poi.overnight_buildings().len(),
        barriers.glacier_ring_count(),
        bbox.is_some(),
    );
    Ok((poi, barriers))
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
    Ok(arc_graph_owned(
        try_load_graph_for_plan_corridor_with_pack_dirs(
            data_dir,
            &[],
            pbf,
            profile,
            clip_bbox,
            route_points,
            crate::routing::plan_bbox::PlanEdgeClipMode::CorridorBand,
        )?,
    ))
}

/// Like [`try_load_graph_for_plan_corridor`], but also searches [pack_dirs]
/// (e.g. `files/long-trip-packs` or a removable volume pack root) for Ready
/// manifests and tile files. [data_dir] remains the Tools / ReuseInternal root.
///
/// `edge_clip_mode` selects corridor-band vs trip-AABB edge materialization
/// ([`crate::routing::plan_bbox::PlanEdgeClipMode`]).
///
/// Returns a shared [`std::sync::Arc`] so warm corridor-cache hits do not clone
/// the owned graph (~1 s / hundreds of MiB). Callers must treat the graph as
/// read-only (eco / surface soft costs are per-plan overlays).
pub fn try_load_graph_for_plan_corridor_with_pack_dirs(
    data_dir: &Path,
    pack_dirs: &[PathBuf],
    pbf: &Path,
    profile: RoutingProfile,
    clip_bbox: Option<[f64; 4]>,
    route_points: Option<&[(f64, f64)]>,
    edge_clip_mode: crate::routing::plan_bbox::PlanEdgeClipMode,
) -> Result<std::sync::Arc<RouteGraph>, PackLoadError> {
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
    // Serialize vs densify-skeleton tile loads so the process-wide densify
    // filter cannot starve a plan hydrate (FU22).
    super::graph_pack::with_plan_pack_hydrate(|| {
        try_load_graph_for_plan_corridor_dirs(
            &dirs,
            pbf,
            profile,
            clip_bbox,
            route_points,
            edge_clip_mode,
        )
    })
}

fn arc_graph_owned(graph: std::sync::Arc<RouteGraph>) -> RouteGraph {
    std::sync::Arc::try_unwrap(graph).unwrap_or_else(|a| (*a).clone())
}

fn try_load_graph_for_plan_corridor_dirs(
    dirs: &[&Path],
    pbf: &Path,
    profile: RoutingProfile,
    clip_bbox: Option<[f64; 4]>,
    route_points: Option<&[(f64, f64)]>,
    edge_clip_mode: crate::routing::plan_bbox::PlanEdgeClipMode,
) -> Result<std::sync::Arc<RouteGraph>, PackLoadError> {
    // Fail fast before any tile mmap / materialize when a required region has
    // no Ready pack (classic Ostlandet-only Raufoss→Bergen hang).
    if let Some(pts) = route_points {
        if pts.len() >= 2 {
            let t_pre = Instant::now();
            let missing = missing_ready_regions_for_trip(dirs, profile, pts);
            let candidates = missing_region_candidates_for_trip(dirs, profile, pts);
            let pre_ms = t_pre.elapsed().as_millis();
            if !candidates.is_empty() {
                log::info!(
                    target: "NaviPlan",
                    "missing_region_candidates count={} regions={}",
                    candidates.len(),
                    candidates.join(",")
                );
            }
            log::info!(
                target: "NaviPlan",
                "plan_add_ms missing_ready_preflight={pre_ms} waypoint_missing={} candidates={}",
                missing.len(),
                candidates.len()
            );
            crate::routing::plan_perf::note_u64("missing_ready_preflight_ms", pre_ms as u64);
            if !missing.is_empty() {
                crate::routing::plan_perf::note("missing_regions", missing.join(","));
                // Abort only when a waypoint sits in an uninstalled outline.
                // Candidates the band crosses must not block an installed-only plan.
                let installed = installed_ready_region_ids(dirs, profile);
                let endpoint_missing: Vec<String> = pts
                    .iter()
                    .filter_map(|&(lat, lon)| {
                        let id = crate::long_trip::region_containing(lat, lon, None)?;
                        let id = id.to_string();
                        let covered = installed.iter().any(|inst| {
                            inst == &id
                                || id.starts_with(&format!("{inst}/"))
                                || inst.starts_with(&format!("{id}/"))
                        });
                        (!covered).then_some(id)
                    })
                    .collect();
                let mut seen = HashSet::new();
                let endpoint_missing: Vec<String> = endpoint_missing
                    .into_iter()
                    .filter(|id| seen.insert(id.clone()))
                    .collect();
                if !endpoint_missing.is_empty() {
                    return Err(PackLoadError::MissingRegions(endpoint_missing));
                }
            }
        }
    }
    let pbf_stem = planning_stem(pbf)?;
    // Chunked long-trip legs still pass the origin PBF; re-home primary to the
    // Ready stem that covers the hop start so we do not merge Sachsen-Anhalt
    // tiles into every Norway leg.
    let (stem, man, primary_dir) = pick_primary_manifest(dirs, &pbf_stem, route_points, profile)?;
    if stem == pbf_stem {
        match status_for_planning_pbf(primary_dir, pbf, &man)? {
            PackStatus::Ready | PackStatus::Outdated => {}
            PackStatus::Missing => return Err(PackLoadError::Missing),
            PackStatus::StalePbf => return Err(PackLoadError::Stale),
            PackStatus::VersionMismatch => return Err(PackLoadError::VersionMismatch),
        }
    } else if !stem_pack_ready(primary_dir, &man, profile) {
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
            extra_corridor_manifests_segs(dirs, &stem, segs, profile)
        } else if let Some(b) = clip_bbox {
            extra_corridor_manifests_segs(dirs, &stem, &[b], profile)
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
                extra_leaf_justified_for_hop(&path, pts, segs_ref)
            });
            // Catalog country AABBs spill across borders (Finland over eastern
            // Finnmark). When both hop ends PIP to the same leaf, foreign
            // extras that only match via AABB steal the plan tile budget and
            // disconnect the real leaf network (Bugøynes→first SE densify hop).
            // Do not clear when either end has a PIP hole — the hole fill below
            // must keep finland/norrbotten for Finnmark→Lapland hops.
            let pip_a = crate::long_trip::region_containing(pts[0].0, pts[0].1, None);
            let pip_b = crate::long_trip::region_containing(pts[1].0, pts[1].1, None);
            if let (Some(a), Some(b)) = (pip_a, pip_b) {
                if a == b {
                    extras.clear();
                }
            }
            // PIP holes (Finnish Lapland) still need the Ready pack that covers
            // the point: Nord-Norge AABB may contain it but has no roads there.
            // Force-retain covering Ready country extracts and leaves.
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
                        let Some(home) = home_dir_for_stem(dirs, stem, profile) else {
                            continue;
                        };
                        let Ok(extra_man) = load_ready_manifest(home, stem) else {
                            continue;
                        };
                        if !stem_pack_ready(home, &extra_man, profile) {
                            continue;
                        }
                        let Some(path) = pbf_stem_to_geofabrik_path(&extra_man.stem) else {
                            continue;
                        };
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
    crate::routing::plan_perf::note("primary_stem", &stem);
    crate::routing::plan_perf::note("pbf_stem", &pbf_stem);
    crate::routing::plan_perf::note_u64("extra_stems", extras.len() as u64);
    if !extras.is_empty() {
        let names: Vec<&str> = extras.iter().map(|m| m.stem.as_str()).collect();
        crate::routing::plan_perf::note("extra_stem_list", names.join(","));
        crate::routing::plan_file_log::line(format!(
            "stems primary={stem} pbf_stem={pbf_stem} extra={}",
            names.join(",")
        ));
    } else {
        crate::routing::plan_file_log::line(format!(
            "stems primary={stem} pbf_stem={pbf_stem} extra="
        ));
    }
    crate::routing::plan_perf::note("edge_clip_mode", format!("{edge_clip_mode:?}"));
    crate::routing::plan_perf::note_u64(
        "edge_clips",
        edge_clips.map(|c| c.len() as u64).unwrap_or(0),
    );
    if !extras.is_empty() {
        crate::download::progress::set(0, Some(5), "Combining map data from multiple regions…");
    }

    let mut seen = HashSet::new();
    let mut tile_candidates = Vec::new();
    // TripAabb must not keep corridor-segment tile filters: the band can omit
    // Fehmarn terminals while still reporting extras=DK. Tile pick then uses
    // clip_bbox (pad AABB) like edge clips.
    let tile_segs = match edge_clip_mode {
        crate::routing::plan_bbox::PlanEdgeClipMode::TripAabb => None,
        crate::routing::plan_bbox::PlanEdgeClipMode::CorridorBand => segs_ref,
    };
    if let Some(tiles) = man.graph_tiles_for(profile) {
        append_intersecting_tile_files_corridor(
            &mut tile_candidates,
            &mut seen,
            tiles,
            clip_bbox,
            tile_segs,
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
                tile_segs,
            );
        }
    }
    let fill_aabb = matches!(
        edge_clip_mode,
        crate::routing::plan_bbox::PlanEdgeClipMode::TripAabb
    );
    let mut budget = crate::routing::plan_bbox::effective_max_plan_tiles_for_stems(extras.len());
    let mut tile_files = select_tiles_within_budget_opts(
        tile_candidates.clone(),
        route_points,
        budget,
        dirs,
        fill_aabb,
    );
    // If a tight budget drops an endpoint (classic Raufoss→Bergen with tiles=6),
    // widen selection before materializing so we never hand A* a disconnected
    // corridor. Memory-aware steps match plan_bbox::next_plan_tile_budget.
    if let Some(pts) = route_points {
        if pts.len() >= 2 {
            let covers = |files: &[String], lat: f64, lon: f64| -> bool {
                tile_candidates.iter().any(|(f, b)| {
                    files.iter().any(|x| x == f)
                        && crate::routing::basemap::bbox_covers_point(*b, lat, lon)
                })
            };
            let mut widen_steps = 0u32;
            while widen_steps < 4
                && !(covers(&tile_files, pts[0].0, pts[0].1)
                    && covers(&tile_files, pts[pts.len() - 1].0, pts[pts.len() - 1].1))
            {
                let Some(next) = crate::routing::plan_bbox::next_plan_tile_budget(budget) else {
                    break;
                };
                budget = next;
                crate::routing::plan_bbox::set_plan_tile_budget_at_least(next);
                tile_files = select_tiles_within_budget_opts(
                    tile_candidates.clone(),
                    route_points,
                    budget,
                    dirs,
                    fill_aabb,
                );
                widen_steps += 1;
                crate::routing::plan_perf::note_u64("tile_select_widen_to", next as u64);
                log::info!(
                    target: "NaviPlan",
                    "tile_select_widen_to={next} files={} (endpoint coverage)",
                    tile_files.len()
                );
            }
        }
    }
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
                                tile_files = select_tiles_within_budget_opts(
                                    merged_cands,
                                    route_points,
                                    crate::routing::plan_bbox::effective_max_plan_tiles_for_stems(
                                        extras.len(),
                                    ),
                                    dirs,
                                    fill_aabb,
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
                            tile_files = select_tiles_within_budget_opts(
                                near_cands,
                                route_points,
                                crate::routing::plan_bbox::effective_max_plan_tiles_for_stems(
                                    extras.len(),
                                ),
                                dirs,
                                fill_aabb,
                            );
                        }
                    }
                }
            }
        }
    }
    if crate::routing::plan_bbox::stage_b_active() {
        if let Some(pts) = route_points {
            if pts.len() >= 2 {
                let all_ready = all_ready_graph_tiles(dirs, profile);
                ensure_coarse_path_tiles(&mut tile_files, &all_ready, pts)?;
            }
        }
    }
    log::info!(
        target: "NaviPlan",
        "try_load_graph tile_files={} after primary+extras (budget={})",
        tile_files.len(),
        crate::routing::plan_bbox::effective_max_plan_tiles_for_stems(extras.len())
    );

    if !tile_files.is_empty() {
        let (tiled, cache_hit, pending_key) =
            load_tiled_graph_files(dirs, tile_files, profile, edge_clips)?;
        // City-state packs (e.g. hamburg) are often a single untiled .rkyv.
        // Merge them whether they are the primary stem or an extra — otherwise
        // a hop that starts on a Hamburg densify anchor loads only neighbour
        // tiles and cannot snap (same 6 km miss as a dropped destination pack).
        let primary_tiled = man.graph_tiles_for(profile).is_some_and(|t| !t.is_empty());
        let mut extras_mono = Vec::new();
        if !primary_tiled {
            if let Some(pp) = graph_path_in_dirs(&man, dirs, profile) {
                extras_mono.push(load_graph_pack_clips(&pp, profile, edge_clips)?);
            }
        }
        for extra in &extras {
            let is_tiled = extra
                .graph_tiles_for(profile)
                .is_some_and(|t| !t.is_empty());
            if is_tiled {
                continue;
            }
            if let Some(ep) = graph_path_in_dirs(extra, dirs, profile) {
                extras_mono.push(load_graph_pack_clips(&ep, profile, edge_clips)?);
            }
        }
        let extras_empty = extras_mono.is_empty();
        let graph = if extras_empty {
            // Keep the corridor Arc — no clone. Not yet in the LRU on miss so
            // ferry supplement can try_unwrap without cloning the whole graph.
            tiled
        } else {
            let mut graphs = vec![arc_graph_owned(tiled)];
            graphs.extend(extras_mono);
            let merged = merge_tile_graphs(graphs, profile);
            if merged.edges.is_empty() {
                return Err(PackLoadError::Missing);
            }
            std::sync::Arc::new(merged)
        };
        // Corridor-cache hit: skip ferry overlay. The cached graph already
        // includes any ferry merge from the miss that populated the LRU.
        if cache_hit && extras_empty {
            crate::routing::plan_perf::note("ferry_overlay", "skip_corridor_cache_hit");
            return Ok(graph);
        }
        let final_graph = supplement_pack_ferries_from_pbf(
            graph,
            dirs,
            &man,
            &extras,
            profile,
            clip_bbox,
            edge_clips,
            route_points,
        )?;
        // Insert *after* ferry supplement so the LRU holds the planning graph
        // and ferry merge never clones a still-cached pre-ferry Arc (~2× RSS).
        // Densify skeleton pre-pass must not populate the LRU: hops load full
        // corridors next and stacking skeleton+hop pushed tablet VmHWM to ~1.2 GiB.
        if let Some(key) = pending_key {
            if super::graph_pack::densify_skeleton_only_active() {
                crate::routing::plan_perf::note("corridor_cache", "skip_insert_densify_skeleton");
            } else {
                super::corridor_cache::corridor_cache_insert(
                    key,
                    std::sync::Arc::clone(&final_graph),
                );
            }
        }
        return Ok(final_graph);
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
                    tile_segs,
                );
            } else if let Some(ep) = graph_path_in_dirs(extra, dirs, profile) {
                graphs.push(load_graph_pack_clips(&ep, profile, edge_clips)?);
            }
        }
        let extra_files = select_tiles_within_budget_opts(
            extra_candidates,
            route_points,
            crate::routing::plan_bbox::effective_max_plan_tiles_for_stems(extras.len()),
            dirs,
            fill_aabb,
        );
        if !extra_files.is_empty() {
            graphs.push(arc_graph_owned(
                load_tiled_graph_files(dirs, extra_files, profile, edge_clips)?.0,
            ));
        }
    }
    let merged = merge_tile_graphs(graphs, profile);
    if merged.edges.is_empty() {
        return Err(PackLoadError::Missing);
    }
    supplement_pack_ferries_from_pbf(
        std::sync::Arc::new(merged),
        dirs,
        &man,
        &extras,
        profile,
        clip_bbox,
        edge_clips,
        route_points,
    )
}

/// Only a real water-crossing length counts as ferry coverage (Fehmarn ~19 km).
/// Short `ferry=yes` approach roads must not skip the overlay.
const MIN_LONG_FERRY_M: f64 = 2_000.0;

#[cfg(test)]
fn graph_has_long_ferry(graph: &RouteGraph) -> bool {
    graph
        .edges
        .iter()
        .any(|e| e.is_ferry && e.length_m >= MIN_LONG_FERRY_M)
}

fn point_in_bbox(lat: f64, lon: f64, bbox: [f64; 4]) -> bool {
    lat >= bbox[0] && lat <= bbox[2] && lon >= bbox[1] && lon <= bbox[3]
}

/// Outcome of the ferry-overlay O/D connectivity gate (one snap + component).
enum FerryHopGate {
    /// Same weak component — skip overlay.
    Connected { snap_m: f64 },
    /// Different components after snap — try overlay (packs may omit ferry /
    /// pier approaches; tile widen alone cannot bridge water gaps).
    Disconnected { snap_m: f64 },
    /// Could not snap even with 35 km — try overlay build.
    SnapFailed,
}

/// Snap hop ends for the ferry-overlay connectivity gate.
///
/// Prefer the plan O/D budget ([`crate::routing::max_waypoint_snap_m`]; car
/// 750 m). Only fall back to [`CHUNK_INTERMEDIATE_SNAP_M`] (35 km) when a tight
/// snap fails — that path is for densify joints, not ordinary corridor O/D.
///
/// Snaps use [`SnapRole::Origin`] / [`SnapRole::Destination`] so one-way dead-end
/// stubs do not falsely force (or skip) ferry overlay.
fn ferry_hop_connectivity_gate(graph: &RouteGraph, a: (f64, f64), b: (f64, f64)) -> FerryHopGate {
    use crate::routing::graph::SnapRole;
    let origin_opts = crate::routing::graph::RouteOptions {
        snap_role: SnapRole::Origin,
        ..Default::default()
    };
    let dest_opts = crate::routing::graph::RouteOptions {
        snap_role: SnapRole::Destination,
        ..Default::default()
    };
    let tight = crate::routing::max_waypoint_snap_m(graph.profile());
    let loose = crate::routing::plan_bbox::CHUNK_INTERMEDIATE_SNAP_M;
    let mut start = graph.nearest_routable_with_options_max(a.0, a.1, &origin_opts, false, tight);
    let mut goal = graph.nearest_routable_with_options_max(b.0, b.1, &dest_opts, false, tight);
    let mut snap_m = tight;
    if start.is_err() || goal.is_err() {
        snap_m = loose;
        if start.is_err() {
            start = graph.nearest_routable_with_options_max(a.0, a.1, &origin_opts, false, loose);
        }
        if goal.is_err() {
            goal = graph.nearest_routable_with_options_max(b.0, b.1, &dest_opts, false, loose);
        }
    }
    let (Ok((start, _)), Ok((goal, _))) = (start, goal) else {
        return FerryHopGate::SnapFailed;
    };
    // Prefer O(1) undirected UF first; only then directed BFS. Coastal packs can
    // be weakly linked via one-way/orphan edges while A* cannot travel O→D —
    // those must try ferry overlay (`disconnected_try_overlay`).
    let reach_opts = crate::routing::graph::RouteOptions::default();
    if start == goal {
        FerryHopGate::Connected { snap_m }
    } else if !graph.same_weak_component(start, goal) {
        FerryHopGate::Disconnected { snap_m }
    } else if graph.directed_reachable_with_options(start, goal, &reach_opts) {
        FerryHopGate::Connected { snap_m }
    } else {
        FerryHopGate::Disconnected { snap_m }
    }
}

/// True when hop ends `a`→`b` already have a directed path on the pack graph
/// (ferries allowed). Used by unit tests for the overlay skip gate.
#[cfg(test)]
fn graph_hop_already_connected(graph: &RouteGraph, a: (f64, f64), b: (f64, f64)) -> bool {
    matches!(
        ferry_hop_connectivity_gate(graph, a, b),
        FerryHopGate::Connected { .. }
    )
}

/// Fallback when hop geometry is unavailable: require a long ferry fully inside
/// the plan clip (not merely somewhere in a multi-region merge).
fn graph_has_long_ferry_in_bbox(graph: &RouteGraph, bbox: [f64; 4]) -> bool {
    graph.edges.iter().any(|e| {
        e.is_ferry
            && e.length_m >= MIN_LONG_FERRY_M
            && point_in_bbox(e.start_lat, e.start_lon, bbox)
            && point_in_bbox(e.end_lat, e.end_lon, bbox)
    })
}

fn plan_clip_bbox(
    clip_bbox: Option<[f64; 4]>,
    edge_clips: Option<&[[f64; 4]]>,
) -> Option<[f64; 4]> {
    clip_bbox.or_else(|| {
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
    })
}

/// Resolve an on-disk PBF usable for ferry overlay (real extract only).
///
/// Prefer `{stem}.ferry.osm.pbf` when present (compact ferry retain), else the
/// region `{stem}.osm.pbf` when it is not a pack-server stub. Stub PBFs must
/// **not** trigger a blocking Geofabrik download during plan (observed ~130–300 s
/// hangs on Android chunk legs); use a pre-fetched ferry sidecar or real extract.
fn country_extract_pbf_stem(leaf_stem: &str) -> Option<String> {
    let path = pbf_stem_to_geofabrik_path(leaf_stem)?;
    let extract = crate::routing::osm_update::geofabrik_extract_path(&path);
    if extract == path {
        return None;
    }
    let leaf = extract.rsplit('/').next()?.to_string();
    Some(format!("{leaf}-latest"))
}

fn real_extract_in_dirs(dirs: &[&Path], filename: &str) -> Option<PathBuf> {
    for d in dirs {
        let p = d.join(filename);
        if pbf_is_real_extract(&p) {
            return Some(p);
        }
    }
    None
}

fn resolve_ferry_overlay_pbf(
    dirs: &[&Path],
    home: &Path,
    stem: &str,
    bbox: [f64; 4],
) -> Option<PathBuf> {
    let _ = bbox;
    resolve_ferry_overlay_pbf_in_dirs(dirs, home, stem)
}

/// Resolve the PBF used to build a ferry sidecar for `stem` under `home`
/// (leaf extract, ferry-only extract, or shared country extract).
pub fn resolve_ferry_overlay_pbf_for_stem(home: &Path, stem: &str) -> Option<PathBuf> {
    resolve_ferry_overlay_pbf_in_dirs(&[], home, stem)
}

fn resolve_ferry_overlay_pbf_in_dirs(dirs: &[&Path], home: &Path, stem: &str) -> Option<PathBuf> {
    let search: Vec<&Path> = std::iter::once(home).chain(dirs.iter().copied()).collect();
    let ferry_name = format!("{stem}.ferry.osm.pbf");
    if let Some(p) = real_extract_in_dirs(&search, &ferry_name) {
        return Some(p);
    }
    let ferry_sidecar = home.join(&ferry_name);
    if ferry_sidecar.is_file()
        && ferry_sidecar
            .metadata()
            .map(|m| m.len() > 64 * 1024)
            .unwrap_or(false)
    {
        return Some(ferry_sidecar);
    }
    let region_name = format!("{stem}.osm.pbf");
    if let Some(p) = real_extract_in_dirs(&search, &region_name) {
        return Some(p);
    }
    if let Some(country) = country_extract_pbf_stem(stem) {
        let country_name = format!("{country}.osm.pbf");
        if let Some(p) = real_extract_in_dirs(&search, &country_name) {
            log::info!(
                target: "NaviPlan",
                "ferry_overlay share country PBF stem={stem} path={}",
                p.display()
            );
            return Some(p);
        }
    }
    let region_pbf = home.join(&region_name);
    if region_pbf.is_file() {
        crate::routing::plan_perf::note("ferry_overlay", "stub_skip_no_ensure");
        log::info!(
            target: "NaviPlan",
            "ferry_overlay stub PBF stem={stem} — skip Geofabrik ensure (no spin)"
        );
    } else {
        log::info!(
            target: "NaviPlan",
            "ferry_overlay no PBF stem={stem} — skip overlay for this extra"
        );
    }
    None
}

/// When published packs were baked without `route=ferry` ways, merge OSM ferry
/// (+ pier approach) edges from on-disk Geofabrik / ferry-sidecar extracts so
/// water hops stay connected until packs are rebaked. Does not force any
/// particular crossing — A* chooses ferries under the normal cost model when
/// `avoid_ferries` is off.
///
/// Overlay is skipped when hop ends are already directed-connected on the pack
/// graph (`route_points`, ferries allowed), or — without hop geometry — when a
/// long ferry lies fully inside the plan clip. Disconnected / snap-failed hop
/// ends try overlay, but only for stems whose region bbox intersects the hop
/// clips (a stem that cannot contain a ferry for this hop is not loaded). Each
/// stem sidecar is materialized at most once per supplement call. Orphan /
/// pier-stub ferry edges and unrelated long ferries elsewhere must not suppress
/// pier-approach overlay.
fn supplement_pack_ferries_from_pbf(
    graph: std::sync::Arc<RouteGraph>,
    dirs: &[&Path],
    primary: &NaviManifest,
    extras: &[NaviManifest],
    profile: RoutingProfile,
    clip_bbox: Option<[f64; 4]>,
    edge_clips: Option<&[[f64; 4]]>,
    route_points: Option<&[(f64, f64)]>,
) -> Result<std::sync::Arc<RouteGraph>, PackLoadError> {
    let t0 = std::time::Instant::now();
    let out = supplement_pack_ferries_from_pbf_inner(
        graph,
        dirs,
        primary,
        extras,
        profile,
        clip_bbox,
        edge_clips,
        route_points,
    );
    crate::routing::plan_perf::add_ferry_stage_ms(t0.elapsed().as_millis() as u64);
    out
}

/// Clip boxes for ferry-sidecar materialization.
///
/// Always prefer the OD corridor band when hop geometry is available — even if
/// the pack fell back to [`PlanEdgeClipMode::TripAabb`]. Trip-AABB overlay pulls
/// a coast-length ferry mesh into one A* and admits water shortcuts
/// (`unnamed@195` on Bergen→Stavanger with longTrip off).
fn ferry_overlay_clips(
    clip_bbox: Option<[f64; 4]>,
    edge_clips: Option<&[[f64; 4]]>,
    route_points: Option<&[(f64, f64)]>,
) -> Option<Vec<[f64; 4]>> {
    const PAD: f64 = 0.05;
    if let Some(pts) = route_points.filter(|p| p.len() >= 2) {
        let mut band = crate::routing::plan_bbox::corridor_band_bboxes(
            pts,
            crate::routing::plan_bbox::CORRIDOR_EDGE_HALF_WIDTH_DEG,
            crate::routing::plan_bbox::CORRIDOR_BAND_STEP_DEG,
        );
        if !band.is_empty() {
            for b in &mut band {
                *b = expand_bbox_deg(*b, PAD);
            }
            return Some(band);
        }
    }
    let bbox = plan_clip_bbox(clip_bbox, edge_clips)?;
    Some(vec![expand_bbox_deg(bbox, PAD)])
}

fn supplement_pack_ferries_from_pbf_inner(
    graph: std::sync::Arc<RouteGraph>,
    dirs: &[&Path],
    primary: &NaviManifest,
    extras: &[NaviManifest],
    profile: RoutingProfile,
    clip_bbox: Option<[f64; 4]>,
    edge_clips: Option<&[[f64; 4]]>,
    route_points: Option<&[(f64, f64)]>,
) -> Result<std::sync::Arc<RouteGraph>, PackLoadError> {
    let Some(clips) = ferry_overlay_clips(clip_bbox, edge_clips, route_points) else {
        return Ok(graph);
    };
    let bbox = plan_clip_bbox(None, Some(clips.as_slice())).unwrap_or(clips[0]);
    if let Some(pts) = route_points.filter(|p| p.len() >= 2) {
        let t_snap = std::time::Instant::now();
        let gate = ferry_hop_connectivity_gate(&graph, pts[0], pts[pts.len() - 1]);
        let snap_probe_ms = t_snap.elapsed().as_millis() as u64;
        crate::routing::plan_perf::note_u64("ferry_snap_probe_ms", snap_probe_ms);
        // No second 35 km probe: one snap + component check decides skip/overlay.
        crate::routing::plan_perf::note_u64("ferry_skip_probe_ms", 0);
        match gate {
            FerryHopGate::Connected { snap_m } => {
                crate::routing::plan_perf::note_f64("ferry_snap_m", snap_m);
                crate::routing::plan_perf::note_u64("ferry_connect_check_ms", 0);
                crate::routing::plan_perf::note("ferry_overlay", "skip_already_connected");
                crate::routing::plan_perf::note(
                    "ferry_per_plan",
                    "once_on_corridor_miss;warm_skipped=corridor_cache_hit",
                );
                // Pack already has a directed O→D path (incl. pack ferries).
                // Replacing every hop with every stem's sidecar was ~10× pack_load
                // and ballooned graphs (~10× snap). Load overlay only when needed.
                return Ok(graph);
            }
            FerryHopGate::Disconnected { snap_m } => {
                crate::routing::plan_perf::note_f64("ferry_snap_m", snap_m);
                crate::routing::plan_perf::note_u64("ferry_connect_check_ms", 0);
                crate::routing::plan_perf::note("ferry_overlay", "disconnected_try_overlay");
                log::info!(
                    target: "NaviPlan",
                    "ferry_overlay try: origin/destination on different components \
                     (possible missing ferry/pier in packs)"
                );
            }
            FerryHopGate::SnapFailed => {
                crate::routing::plan_perf::note_f64(
                    "ferry_snap_m",
                    crate::routing::plan_bbox::CHUNK_INTERMEDIATE_SNAP_M,
                );
                crate::routing::plan_perf::note_u64("ferry_connect_check_ms", 0);
                crate::routing::plan_perf::note("ferry_overlay", "snap_failed_try_overlay");
            }
        }
    } else {
        let t_skip = std::time::Instant::now();
        let skip_overlay = graph_has_long_ferry_in_bbox(&graph, bbox);
        crate::routing::plan_perf::note_u64(
            "ferry_skip_probe_ms",
            t_skip.elapsed().as_millis() as u64,
        );
        if skip_overlay {
            crate::routing::plan_perf::note("ferry_overlay", "skip_long_ferry_in_bbox");
            return Ok(graph);
        }
    }
    let mut stems = Vec::with_capacity(1 + extras.len());
    stems.push(primary.stem.clone());
    for e in extras {
        if !stems.iter().any(|s| s == &e.stem) {
            stems.push(e.stem.clone());
        }
    }
    let mut overlays = Vec::new();
    let mut overlay_mode = "none";
    let mut stems_loaded: Vec<String> = Vec::new();
    for stem in &stems {
        // Sidecar only when this region's bbox can intersect the hop clips
        // (otherwise it cannot contribute a ferry for this hop).
        let region = crate::routing::basemap::pbf_stem_to_geofabrik_path(stem)
            .and_then(|path| crate::routing::basemap::region_bbox(&path));
        if let Some(rb) = region {
            if !clips.iter().any(|c| bbox_intersects(rb, *c)) {
                crate::routing::plan_perf::note(
                    "ferry_overlay",
                    format!("skip_stem_no_clip_intersect;stem={stem}"),
                );
                continue;
            }
        }
        let Some(home) = home_dir_for_stem(dirs, stem, profile) else {
            continue;
        };
        let Some(pbf) = resolve_ferry_overlay_pbf(dirs, home, stem, bbox) else {
            continue;
        };
        // Geofabrik-sized extracts: never parse PBF on the plan thread. If the
        // sidecar is not ready, surface FerryPreparing and kick a background
        // ensure. Corridor fixtures / small real extracts (BlobHeader, below
        // 1 MiB) are cheap to parse here so pack load completes (innlandet
        // host tests, local cuts) instead of spinning on preparing.
        if !super::ferry_overlay_cache::sidecar_fresh(home, stem, profile, &pbf) {
            let pbf_len = pbf.metadata().map(|m| m.len()).unwrap_or(0);
            if pbf_is_real_extract(&pbf) && pbf_len < MIN_REAL_PBF_BYTES {
                crate::routing::plan_perf::note("ferry_overlay", "sync_sidecar_small_extract");
                if let Err(e) =
                    super::ferry_overlay_cache::ensure_ferry_sidecar(home, stem, profile, &pbf)
                {
                    log::warn!(
                        target: "NaviPlan",
                        "ferry_overlay small-extract sidecar failed stem={stem}: {e:#}"
                    );
                    continue;
                }
            } else {
                // Densify skeleton must not enqueue sidecar builds: it walks
                // every corridor extra (NDS/DK/MV/SH/NO…) and those jobs share
                // one lock, so the SH→DK hop never sees a finished sidecar.
                if super::graph_pack::densify_skeleton_only_active() {
                    log::info!(
                        target: "NaviPlan",
                        "ferry_overlay skeleton skip preparing stem={stem} — continue without that overlay"
                    );
                    continue;
                }
                let (status, pct) = super::ferry_overlay_cache::ferry_preparing_status(stem);
                crate::routing::plan_perf::note("ferry_overlay", "preparing_sidecar");
                log::info!(
                    target: "NaviPlan",
                    "ferry_overlay preparing stem={stem} — sidecar idle job, not plan thread"
                );
                return Err(PackLoadError::FerryPreparing(status, pct));
            }
        }
        if stems_loaded.iter().any(|s| s == stem) {
            crate::routing::plan_perf::note(
                "ferry_overlay",
                format!("skip_stem_already_loaded;stem={stem}"),
            );
            continue;
        }
        match super::ferry_overlay_cache::ferry_overlay_for_plan(home, stem, profile, &pbf, &clips)
        {
            Some((fg, mode)) if fg.edges.iter().any(|e| e.is_ferry) => {
                overlay_mode = mode;
                stems_loaded.push(stem.clone());
                log::info!(
                    target: "NaviPlan",
                    "ferry_overlay stem={stem} ferry_edges={} nodes={} mode={mode}",
                    fg.edges.iter().filter(|e| e.is_ferry).count(),
                    fg.nodes.len(),
                );
                overlays.push(fg);
            }
            Some(_) => {
                stems_loaded.push(stem.clone());
            }
            None => {
                log::info!(
                    target: "NaviPlan",
                    "ferry_overlay stem={stem} sidecar fresh but empty (no ferry edges)"
                );
            }
        }
    }
    if overlays.is_empty() {
        crate::routing::plan_perf::note("ferry_overlay", "no_overlay_source");
        return Ok(graph);
    }
    crate::routing::plan_perf::note("ferry_overlay", overlay_mode);
    crate::routing::plan_perf::note(
        "ferry_per_plan",
        format!(
            "once_per_needed_stem;stems={};warm_skipped=corridor_cache_hit",
            stems_loaded.join(",")
        ),
    );
    let mut pack = arc_graph_owned(graph);
    for ov in &mut overlays {
        drop_pack_edges_replaced_by_overlay_ferry(&mut pack, ov);
    }
    let mut parts = Vec::with_capacity(1 + overlays.len());
    parts.push(pack);
    parts.extend(overlays);
    Ok(std::sync::Arc::new(merge_tile_graphs(parts, profile)))
}

/// Choose the Ready manifest for planning: prefer a stem whose region covers
/// the first route point when the PBF stem does not (chunked long-trip hops).
///
/// PIP / covering-manifest scan runs before requiring the planning PBF stem to
/// have a Ready pack. Camping `find_planning_pbf` may return a country extract
/// (`sweden-latest.osm.pbf`) with no `sweden-latest` graph pack — only leaf
/// packs — and hard-failing on that stem blocked overnight corridor loads.
fn pick_primary_manifest<'a>(
    dirs: &[&'a Path],
    pbf_stem: &str,
    route_points: Option<&[(f64, f64)]>,
    profile: RoutingProfile,
) -> Result<(String, NaviManifest, &'a Path), PackLoadError> {
    // Keep the planning stem whenever its manifest exists — even VersionMismatch /
    // Stale / incomplete — so try_load can emit the precise PackLoadError instead
    // of collapsing everything to Missing (see graph_pack_v5_to_v6_regen).
    // Ready-only re-home still runs first when route points are present so a
    // country extract without Ready packs (sweden camping corridor) can fall
    // through to a Ready PIP / covering leaf.
    let pbf_pack = home_dir_for_stem(dirs, pbf_stem, profile).and_then(|home| {
        match load_ready_manifest(home, pbf_stem) {
            Ok(man) => Some((pbf_stem.to_string(), man, home)),
            _ => None,
        }
    });
    let pbf_ready = pbf_pack.as_ref().and_then(|(stem, man, home)| {
        if stem_pack_ready(home, man, profile) {
            Some((stem.clone(), man.clone(), *home))
        } else {
            None
        }
    });

    let Some(pts) = route_points.filter(|p| !p.is_empty()) else {
        return pbf_pack.ok_or(PackLoadError::Missing);
    };
    // Host dest-hop probe / measure only. Empty or unknown stem is ignored.
    if let Ok(force) = std::env::var("NAVI_FORCE_PRIMARY_STEM") {
        let force = force.trim();
        if !force.is_empty() {
            if let Some(home) = home_dir_for_stem(dirs, force, profile) {
                if let Ok(man) = load_ready_manifest(home, force) {
                    if stem_pack_ready(home, &man, profile) {
                        return Ok((man.stem.clone(), man, home));
                    }
                }
            }
        }
    }
    let (lat, lon) = pts[0];
    // Prefer Admin/PIP leaf over catalog AABB. Country extracts (Finland) spill
    // over eastern Finnmark and have a smaller AABB than Nord-Norge, so the
    // old "smallest covering bbox" pick made Finland primary for Bugøynes hops
    // and dropped the real Norwegian exit network after same-leaf extras.clear.
    if let Some(pip_path) = crate::long_trip::region_containing(lat, lon, None) {
        // Cross-leaf hops (Sognefjell ostlandet → Dalsøren vestlandet): dest
        // pack as primary so the tile budget covers the fjord approach. Origin
        // remains an extra stem; we do not materialize two full graphs.
        if pts.len() >= 2 {
            let (dlat, dlon) = pts[pts.len() - 1];
            if let Some(dest_path) = crate::long_trip::region_containing(dlat, dlon, None) {
                if hop_prefers_dest_primary(Some(pip_path), Some(dest_path)) {
                    if let Some(hit) = ready_stem_for_geofabrik(dirs, dest_path, profile) {
                        return Ok(hit);
                    }
                }
            }
        }
        if let Some(hit) = ready_stem_for_geofabrik(dirs, pip_path, profile) {
            return Ok(hit);
        }
    }
    if let Some((stem, man, home)) = &pbf_ready {
        if let Some(path) = pbf_stem_to_geofabrik_path(stem) {
            if let Some(region) = region_bbox(&path) {
                if crate::routing::basemap::bbox_covers_point(region, lat, lon) {
                    return Ok((stem.clone(), man.clone(), *home));
                }
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
            let Some(home) = home_dir_for_stem(dirs, stem, profile) else {
                continue;
            };
            let Ok(man) = load_ready_manifest(home, stem) else {
                continue;
            };
            if !stem_pack_ready(home, &man, profile) {
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
    // No Ready re-home: return the planning stem (any status) so try_load can
    // surface VersionMismatch / Stale instead of Missing.
    pbf_pack.ok_or(PackLoadError::Missing)
}

/// Dest leaf as primary when origin and dest PIP to different Geofabrik paths.
pub(crate) fn hop_prefers_dest_primary(origin_path: Option<&str>, dest_path: Option<&str>) -> bool {
    match (origin_path, dest_path) {
        (Some(a), Some(b)) => a != b,
        (None, Some(_)) => true,
        _ => false,
    }
}

fn ready_stem_for_geofabrik<'a>(
    dirs: &[&'a Path],
    geofabrik_path: &str,
    profile: RoutingProfile,
) -> Option<(String, NaviManifest, &'a Path)> {
    let leaf = geofabrik_path.rsplit('/').next().unwrap_or(geofabrik_path);
    let stem = format!("{leaf}-latest");
    let home = home_dir_for_stem(dirs, &stem, profile)?;
    let man = load_ready_manifest(home, &stem).ok()?;
    if stem_pack_ready(home, &man, profile) {
        Some((man.stem.clone(), man, home))
    } else {
        None
    }
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
///
/// Takes ownership of tile graphs (no extra full-graph clone). Reserves node /
/// edge capacity up front so HashMap growth does not temporarily double RSS
/// during multi-tile corridor merges.
pub fn merge_tile_graphs(graphs: Vec<RouteGraph>, profile: RoutingProfile) -> RouteGraph {
    let timing = crate::routing::plan_perf::enabled();
    let t_hash = std::time::Instant::now();
    let mut node_cap = 0usize;
    let mut edge_cap = 0usize;
    for g in &graphs {
        node_cap = node_cap.saturating_add(g.nodes.len());
        edge_cap = edge_cap.saturating_add(g.edges.len());
    }
    let mut nodes = HashMap::with_capacity(node_cap);
    let mut edges = Vec::with_capacity(edge_cap);
    let mut seen_edge_keys = HashSet::with_capacity(edge_cap);
    for g in graphs {
        for (id, node) in g.nodes {
            nodes.insert(id, node);
        }
        for e in g.edges {
            if is_construction_or_proposed_highway(e.highway.as_deref()) {
                continue;
            }
            if seen_edge_keys.insert(graph_edge_tile_merge_key(&e)) {
                edges.push(e);
            }
        }
    }
    let hash_ms = t_hash.elapsed().as_millis() as u64;
    let n_nodes = nodes.len();
    let n_edges = edges.len();
    let t_adj = std::time::Instant::now();
    let out = RouteGraph::from_parts(nodes, edges, profile);
    let adj_ms = t_adj.elapsed().as_millis() as u64;
    if timing {
        crate::routing::plan_perf::add_merge_stage_ms(hash_ms, adj_ms);
        crate::routing::plan_perf::note(
            "merge_stage",
            format!(
                "hash_ms={hash_ms};adj_ms={adj_ms};nodes={n_nodes};edges={n_edges};\
                 border_stitch=full_node_hash;v9_border_marks=false;threads=1"
            ),
        );
    }
    out
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

/// Bounded parallel tile hydrate concurrency.
///
/// Default: **2** when MemTotal ≥ 6 GiB, else **1** (tablet ~3.5 GiB paid
/// +50–80 MiB peak RSS for ~0.3 s). Override with `NAVI_TILE_LOAD_PARALLEL`.
fn tile_load_parallelism() -> usize {
    const HIGH_RAM_PARALLEL: usize = 2;
    const LOW_RAM_PARALLEL: usize = 1;
    const LOW_RAM_MEM_TOTAL: u64 = 6 * 1024 * 1024 * 1024;
    if let Ok(v) = std::env::var("NAVI_TILE_LOAD_PARALLEL") {
        if let Ok(n) = v.trim().parse::<usize>() {
            return n.clamp(1, 8);
        }
    }
    match super::corridor_cache::read_mem_total_bytes() {
        Some(total) if total < LOW_RAM_MEM_TOTAL => LOW_RAM_PARALLEL,
        _ => HIGH_RAM_PARALLEL,
    }
}

/// Load and merge tiled packs. On miss, returns `(graph, false, Some(key))` and
/// does **not** insert into the corridor LRU — the caller must insert after ferry
/// overlay so [`arc_graph_owned`] can unique-unwrap instead of cloning.
fn load_tiled_graph_files(
    dirs: &[&Path],
    tile_files: Vec<String>,
    profile: RoutingProfile,
    clips: Option<&[[f64; 4]]>,
) -> Result<
    (
        std::sync::Arc<RouteGraph>,
        bool,
        Option<super::corridor_cache::CorridorCacheKey>,
    ),
    PackLoadError,
> {
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    let mut tile_files = tile_files;
    if tile_files.is_empty() {
        return Err(PackLoadError::Missing);
    }
    tile_files.sort();

    let cache_key =
        super::corridor_cache::CorridorCacheKey::new(profile, tile_files.clone(), clips);
    if let Some(hit) = super::corridor_cache::corridor_cache_get(&cache_key) {
        crate::routing::plan_perf::note("corridor_cache", "hit");
        crate::routing::plan_perf::note_u64("tiles", tile_files.len() as u64);
        log::info!(
            target: "NaviPlan",
            "corridor_cache hit tiles={} nodes={} edges={} (arc share, no clone)",
            tile_files.len(),
            hit.nodes.len(),
            hit.edges.len()
        );
        return Ok((hit, true, None));
    }
    crate::routing::plan_perf::note("corridor_cache", "miss");
    // Free prior corridors before materializing a new one (single-MRU policy).
    super::corridor_cache::corridor_cache_evict_before_load();

    // Full-tile LRU + in-memory clip, then **one** merge. Tromsø densify hops
    // change clips every leg but reuse tile files — cache skips mmap/copy on hits.
    // Single-pass adjacency (not per-tile rebuild) stays in merge_tile_graphs.
    //
    // 2b: bounded parallel tile hydrate. Peak RSS matches sequential because we
    // already retain every clipped part until the single merge; concurrency only
    // overlaps I/O/copy, not extra retained graphs. Default concurrency falls to
    // 1 under 6 GiB MemTotal (see [`tile_load_parallelism`]).
    let total = tile_files.len() as u64;
    crate::download::progress::set(0, Some(total), "Loading map tiles…");
    super::tile_cache::tile_cache_evict_before_load();
    let tile_cache_on = super::tile_cache::tile_cache_enabled();

    let resolved: Result<Vec<(String, PathBuf)>, PackLoadError> = tile_files
        .iter()
        .map(|file| {
            let path = resolve_pack_file(dirs, file).ok_or(PackLoadError::Missing)?;
            Ok((file.clone(), path))
        })
        .collect();
    let resolved = resolved?;
    let clips_owned: Option<Vec<[f64; 4]>> = clips.map(|c| c.to_vec());
    let tile_hits = AtomicU64::new(0);
    let tile_misses = AtomicU64::new(0);
    let fit_rejects = AtomicU64::new(0);

    let n_threads = tile_load_parallelism().min(resolved.len().max(1));
    crate::routing::plan_perf::set_tile_load_parallel(n_threads as u64);
    if let Some(mem_total) = super::corridor_cache::read_mem_total_bytes() {
        crate::routing::plan_perf::note_u64("mem_total_mb", mem_total / (1024 * 1024));
    }
    crate::routing::plan_perf::note_u64("tile_load_parallel", n_threads as u64);
    crate::routing::plan_perf::note("tile_cache_enabled", if tile_cache_on { "1" } else { "0" });
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(n_threads)
        .build()
        .map_err(|e| PackLoadError::Other(anyhow::anyhow!("tile load pool: {e}")))?;

    let loaded: Result<Vec<(usize, RouteGraph)>, PackLoadError> = pool.install(|| {
        resolved
            .par_iter()
            .enumerate()
            .map(|(i, (file, path))| {
                log::info!(
                    target: "NaviPlan",
                    "load_tiled_graph file={file} ({}/{})",
                    i + 1,
                    total
                );
                let key = super::tile_cache::TileCacheKey::new(profile, path);
                let clips_ref = clips_owned.as_deref();
                let g = if !tile_cache_on {
                    tile_misses.fetch_add(1, Ordering::Relaxed);
                    crate::routing::plan_perf::note(
                        "tile_cache",
                        format!("disabled;clip_hydrate;{file}"),
                    );
                    load_graph_pack_clips(path, profile, clips_ref)?
                } else if let Some(hit) = super::tile_cache::tile_cache_get(&key) {
                    tile_hits.fetch_add(1, Ordering::Relaxed);
                    crate::routing::plan_perf::note("tile_cache", format!("hit;{file}"));
                    super::tile_cache::clip_route_graph(&hit, clips_ref)
                } else if clips_ref.is_some() && !super::tile_cache::tile_likely_fits_cache(path) {
                    // Oversized tiles: clip during hydrate. Full materialize exceeds the
                    // tile LRU (~64–90 MiB) and was discarded after paying extra copy
                    // (Bergen eco cold regress when always loading full).
                    tile_misses.fetch_add(1, Ordering::Relaxed);
                    fit_rejects.fetch_add(1, Ordering::Relaxed);
                    crate::routing::plan_perf::note(
                        "tile_cache",
                        format!("miss_clip_hydrate;fit_reject;{file}"),
                    );
                    load_graph_pack_clips(path, profile, clips_ref)?
                } else {
                    // Small tiles (or unclipped): full materialize + LRU, then clip.
                    tile_misses.fetch_add(1, Ordering::Relaxed);
                    crate::routing::plan_perf::note("tile_cache", format!("miss_full;{file}"));
                    let full = load_graph_pack_clips(path, profile, None)?;
                    let arc = Arc::new(full);
                    super::tile_cache::tile_cache_insert(key, Arc::clone(&arc));
                    super::tile_cache::clip_route_graph(&arc, clips_ref)
                };
                Ok((i, g))
            })
            .collect()
    });
    let mut loaded = loaded?;
    loaded.sort_by_key(|(i, _)| *i);
    let mut parts: Vec<RouteGraph> = Vec::with_capacity(loaded.len());
    for (_, g) in loaded {
        if g.edges.is_empty() && g.nodes.is_empty() {
            continue;
        }
        parts.push(g);
    }
    if parts.is_empty() {
        return Err(PackLoadError::Missing);
    }
    let tile_hits = tile_hits.load(Ordering::Relaxed);
    let tile_misses = tile_misses.load(Ordering::Relaxed);
    let fit_rejects = fit_rejects.load(Ordering::Relaxed);
    let t_merge = std::time::Instant::now();
    let merged = merge_tile_graphs(parts, profile);
    let merge_ms = t_merge.elapsed().as_millis() as u64;
    if merged.edges.is_empty() {
        return Err(PackLoadError::Missing);
    }
    log::info!(
        target: "NaviPlan",
        "load_tiled_graph done tiles={} edges={} nodes={} merge_wall_ms={} merge=single_pass \
         tile_cache_hits={} misses={} fit_rejects={} parallel={} tile_cache={}",
        total,
        merged.edges.len(),
        merged.nodes.len(),
        merge_ms,
        tile_hits,
        tile_misses,
        fit_rejects,
        n_threads,
        if tile_cache_on { "on" } else { "off" }
    );
    if crate::routing::plan_perf::enabled() {
        crate::routing::plan_perf::note_u64("pack_merge_wall_ms", merge_ms);
        crate::routing::plan_perf::note("pack_merge_mode", "single_pass_one_adjacency");
        crate::routing::plan_perf::note_u64("tile_cache_hits", tile_hits);
        crate::routing::plan_perf::note_u64("tile_cache_misses", tile_misses);
        crate::routing::plan_perf::note_u64("tile_cache_fit_rejects", fit_rejects);
        let (cum_hits, cum_misses, bytes, n) = super::tile_cache::tile_cache_stats();
        crate::routing::plan_perf::note(
            "tile_cache_stats",
            format!("cum_hits={cum_hits};cum_misses={cum_misses};bytes={bytes};entries={n}"),
        );
    }
    let arc = Arc::new(merged);
    // Defer corridor LRU insert until after ferry overlay (caller).
    Ok((arc, false, Some(cache_key)))
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
///
/// When `bbox` is set, records are clipped to that corridor (not whole-region)
/// and the clipped index is LRU-cached between plans.
pub fn try_load_poi_barrier_for_plan_bbox(
    data_dir: &Path,
    pbf: &Path,
    bbox: Option<[f64; 4]>,
) -> Result<(PoiIndex, DangerBarrierIndex), PackLoadError> {
    try_load_poi_barrier_for_plan_bbox_with_pack_dirs(data_dir, &[], pbf, bbox)
}

/// Like [`try_load_poi_barrier_for_plan_bbox`], but also searches [pack_dirs]
/// (SD / long-trip-packs) for Ready manifests — same roots as graph load.
pub fn try_load_poi_barrier_for_plan_bbox_with_pack_dirs(
    data_dir: &Path,
    pack_dirs: &[PathBuf],
    pbf: &Path,
    bbox: Option<[f64; 4]>,
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
    try_load_poi_barrier_for_plan_bbox_dirs(&dirs, pbf, bbox)
}

fn try_load_poi_barrier_for_plan_bbox_dirs(
    dirs: &[&Path],
    pbf: &Path,
    bbox: Option<[f64; 4]>,
) -> Result<(PoiIndex, DangerBarrierIndex), PackLoadError> {
    let pbf_stem = planning_stem(pbf)?;
    let (stem, man, primary_dir) =
        pick_primary_manifest(dirs, &pbf_stem, None, RoutingProfile::Car)?;
    if stem == pbf_stem {
        match status_for_planning_pbf(primary_dir, pbf, &man)? {
            PackStatus::Ready | PackStatus::Outdated => {}
            PackStatus::Missing => return Err(PackLoadError::Missing),
            PackStatus::StalePbf => return Err(PackLoadError::Stale),
            PackStatus::VersionMismatch => return Err(PackLoadError::VersionMismatch),
        }
    } else if !stem_pack_ready(primary_dir, &man, RoutingProfile::Car) {
        return Err(PackLoadError::Missing);
    }

    let mut path_list = vec![man
        .poi_barrier_path(primary_dir)
        .to_string_lossy()
        .into_owned()];
    let mut extras = Vec::new();
    if corridor_needs_extra_stems(&stem, bbox) {
        if let Some(b) = bbox {
            // Cap extras: full POI packs are 30–90 MB on disk and inflate peak
            // RSS while the route graph is still live (4 GB Automotive).
            extras = extra_corridor_manifests_segs(dirs, &stem, &[b], RoutingProfile::Car);
            extras.truncate(1);
            for extra in &extras {
                let Some(home) = home_dir_for_stem(dirs, &extra.stem, RoutingProfile::Car) else {
                    continue;
                };
                path_list.push(extra.poi_barrier_path(home).to_string_lossy().into_owned());
            }
        }
    }

    // Motor plan path: skip overnight building hydrate (hiking-only samples).
    const INCLUDE_OVERNIGHT_BUILDINGS: bool = false;
    let cache_key = super::poi_barrier_cache::PoiBarrierCacheKey::new(path_list.clone(), bbox);
    if let Some((poi, barriers)) = super::poi_barrier_cache::poi_barrier_cache_get(&cache_key) {
        crate::routing::plan_perf::note("poi_barrier_cache", "hit");
        log::info!(
            target: "NaviPlan",
            "poi_barrier_cache hit paths={} poi={} glaciers={}",
            path_list.len(),
            poi.len(),
            barriers.glacier_ring_count()
        );
        return Ok(((*poi).clone(), (*barriers).clone()));
    }
    crate::routing::plan_perf::note("poi_barrier_cache", "miss");

    let primary_path = man.poi_barrier_path(primary_dir);
    let (mut poi, mut barriers) =
        load_poi_barrier_pack_bbox(&primary_path, bbox, INCLUDE_OVERNIGHT_BUILDINGS)?;
    for extra in &extras {
        let Some(home) = home_dir_for_stem(dirs, &extra.stem, RoutingProfile::Car) else {
            continue;
        };
        let path = extra.poi_barrier_path(home);
        if !path.is_file() {
            continue;
        }
        let Ok((epoi, ebar)) = load_poi_barrier_pack_bbox(&path, bbox, INCLUDE_OVERNIGHT_BUILDINGS)
        else {
            continue;
        };
        poi.extend_from(&epoi);
        barriers.merge(ebar);
    }

    super::poi_barrier_cache::poi_barrier_cache_insert(
        cache_key,
        std::sync::Arc::new(poi.clone()),
        std::sync::Arc::new(barriers.clone()),
    );
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
            if !stem_pack_ready(data_dir, &man, RoutingProfile::Car) {
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
        PackStatus::Ready | PackStatus::Outdated => {}
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
            for extra in extra_corridor_manifests(data_dir, &stem, b, RoutingProfile::Car) {
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
    use super::{
        ensure_coarse_path_tiles, select_tiles_within_budget, select_tiles_within_budget_opts,
        tile_bboxes_adjacent, tile_counts_as_endpoint_cover, PackLoadError,
    };
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

    #[test]
    fn trip_aabb_fill_keeps_off_chord_valley_tile() {
        let dir = tempfile::tempdir().expect("tmpdir");
        // Start/end on chord tiles; valley tile is south of the chord (covers
        // neither endpoint nor eighth samples) and is dropped unless fill_to_budget.
        let start_bb = [61.5_f64, 8.2, 61.9, 8.5];
        let dest_bb = [61.3_f64, 7.3, 61.6, 7.6];
        let valley_bb = [61.45_f64, 7.7, 61.65, 8.15];
        let cands = vec![
            ("ostlandet-latest.navi-graph-car.t3_1.rkyv".into(), start_bb),
            ("vestlandet-latest.navi-graph-car.t4_3.rkyv".into(), dest_bb),
            (
                "vestlandet-latest.navi-graph-car.t4_2.rkyv".into(),
                valley_bb,
            ),
        ];
        for (name, bytes) in [
            ("ostlandet-latest.navi-graph-car.t3_1.rkyv", 50usize),
            ("vestlandet-latest.navi-graph-car.t4_3.rkyv", 50),
            ("vestlandet-latest.navi-graph-car.t4_2.rkyv", 40),
        ] {
            touch_sized(dir.path(), name, bytes);
        }
        let pts = [(61.67732_f64, 8.30020), (61.44338, 7.46140)];
        let band = select_tiles_within_budget(cands.clone(), Some(&pts), 2, &[dir.path()]);
        assert!(
            !band.iter().any(|f| f.contains("t4_2")),
            "tight chord budget must drop the off-chord valley tile; got {band:?}"
        );
        let aabb = select_tiles_within_budget_opts(cands, Some(&pts), 14, &[dir.path()], true);
        assert!(
            aabb.iter().any(|f| f.contains("t4_2")),
            "TripAabb fill_to_budget must keep the valley tile; got {aabb:?}"
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

    #[test]
    fn hop_end_tile_outside_first_selection_is_forced_in() {
        crate::routing::plan_bbox::set_stage_b_active(true);
        let first = vec![
            "leaf-latest.navi-graph-car.t0_0.rkyv".to_string(),
            "leaf-latest.navi-graph-car.t0_1.rkyv".to_string(),
        ];
        let mut selected = first.clone();
        // First budget kept only the start tiles; the joint lives in t1_0.
        let all_ready = vec![
            (
                "leaf-latest.navi-graph-car.t0_0.rkyv".into(),
                [60.0_f64, 10.0, 60.2, 10.2],
            ),
            (
                "leaf-latest.navi-graph-car.t0_1.rkyv".into(),
                [60.0_f64, 10.2, 60.2, 10.4],
            ),
            (
                "leaf-latest.navi-graph-car.t1_0.rkyv".into(),
                [60.2_f64, 10.0, 60.4, 10.2],
            ),
        ];
        let pts = [(60.1_f64, 10.1), (60.25, 10.05), (60.35, 10.10)];
        ensure_coarse_path_tiles(&mut selected, &all_ready, &pts).expect("end tile exists");
        crate::routing::plan_bbox::set_stage_b_active(false);
        assert!(
            selected
                .iter()
                .any(|f| f.contains("navi-graph-car.t1_0.rkyv")),
            "joint tile t1_0 must be added; got {selected:?}"
        );
        assert!(selected.len() > first.len());
    }

    #[test]
    fn hop_without_end_tile_fails_loudly() {
        crate::routing::plan_bbox::set_stage_b_active(true);
        let mut selected = vec!["leaf-latest.navi-graph-car.t0_0.rkyv".to_string()];
        let all_ready = vec![(
            "leaf-latest.navi-graph-car.t0_0.rkyv".into(),
            [60.0_f64, 10.0, 60.2, 10.2],
        )];
        let pts = [(60.1_f64, 10.1), (61.0, 12.0)];
        let err = ensure_coarse_path_tiles(&mut selected, &all_ready, &pts)
            .expect_err("end is outside every Ready tile");
        crate::routing::plan_bbox::set_stage_b_active(false);
        assert!(
            matches!(err, PackLoadError::MissingEndTile(_)),
            "got {err:?}"
        );
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
            cost_mult: 1.0,
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
            ferry_interval_min: None,
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
        extra_leaf_justified_for_hop, home_dir_for_stem,
    };
    use crate::routing::graph::RoutingProfile;
    use std::fs;

    #[test]
    fn home_dir_prefers_newer_format_when_stem_duplicated() {
        // Legacy files/ root (v8) listed before long-trip-packs (v9): planner must
        // still home on v9.
        let root = tempfile::tempdir().expect("tmpdir");
        let v8 = root.path().join("files");
        let v9 = root.path().join("long-trip-packs");
        fs::create_dir_all(&v8).unwrap();
        fs::create_dir_all(&v9).unwrap();
        let man = |fmt: u32| {
            format!(
                r#"{{"schema":1,"stem":"ostlandet-latest","pbf_filename":"ostlandet-latest.osm.pbf","pbf_size_bytes":1,"pbf_modified_unix_secs":1,"graph_files":{{}},"graph_format_version":{fmt},"poi_barrier_file":"ostlandet-latest.navi-poi-barrier.rkyv","poi_barrier_format_version":2,"wetland_format_version":1}}"#
            )
        };
        fs::write(v8.join("ostlandet-latest.navi-manifest.json"), man(8)).unwrap();
        fs::write(v9.join("ostlandet-latest.navi-manifest.json"), man(9)).unwrap();
        let home = home_dir_for_stem(
            &[v8.as_path(), v9.as_path()],
            "ostlandet-latest",
            RoutingProfile::Car,
        );
        assert_eq!(
            home.map(|p| p.to_path_buf()),
            Some(v9),
            "must prefer v9 long-trip-packs over v8 files/ root"
        );
    }

    #[test]
    fn mv_aabb_spill_does_not_justify_fehmarn_hop_extra() {
        let hop = [(54.21_f64, 11.025), (55.175, 11.700)];
        let clip = [[53.86_f64, 10.675, 55.525, 12.050]];
        assert!(
            extra_leaf_justified_for_hop("europe/denmark", &hop, Some(&clip)),
            "DK must stay on Fehmarn hop"
        );
        assert!(
            extra_leaf_justified_for_hop("europe/germany/schleswig-holstein", &hop, Some(&clip)),
            "SH is the origin leaf"
        );
        assert!(
            !extra_leaf_justified_for_hop(
                "europe/germany/mecklenburg-vorpommern",
                &hop,
                Some(&clip)
            ),
            "MV AABB covers Fehmarn but PIP is SH — must not steal tiles"
        );
    }

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
    fn corridor_needs_extra_when_endpoint_has_pip_hole() {
        // Finnmark→Lapland densify hop: dest PIP is None (Finnish Lapland hole)
        // but a Ready finland extract covers it — must force extras.
        let dir = tempfile::tempdir().expect("tmpdir");
        fs::write(
            dir.path().join("finland-latest.navi-manifest.json"),
            r#"{"schema":1,"stem":"finland-latest","pbf_filename":"finland-latest.osm.pbf","graph_files":{},"graph_format_version":9}"#,
        )
        .unwrap();
        let pts = [(69.67551_f64, 29.14187_f64), (65.70041_f64, 24.66393_f64)];
        assert!(
            corridor_needs_extra_for_endpoint_leaves(
                "nord-norge-latest",
                Some(&pts),
                &[dir.path()]
            ),
            "PIP-hole endpoint must force multi-stem load"
        );
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

    #[test]
    fn missing_regions_names_vestlandet_before_any_graph_load() {
        use super::missing_ready_regions_for_trip;
        use crate::routing::graph::RoutingProfile;
        // Raufoss (Ostlandet) → Bergen (Vestlandet). No Ready packs installed.
        let pts = [(60.726_f64, 10.613_f64), (60.3913_f64, 5.3221_f64)];
        let missing = missing_ready_regions_for_trip(&[], RoutingProfile::Car, &pts);
        assert!(
            missing.iter().any(|r| r == "europe/norway/vestlandet"),
            "expected Vestlandet in missing, got {missing:?}"
        );
        // Empty dirs: never spins; returns immediately with named regions.
        assert!(!missing.is_empty());
    }

    #[test]
    fn try_load_fails_fast_when_destination_region_missing() {
        use super::{try_load_graph_for_plan_corridor_with_pack_dirs, PackLoadError};
        use crate::routing::graph::RoutingProfile;
        use crate::routing::plan_bbox::PlanEdgeClipMode;
        use std::time::Instant;

        let dir = tempfile::tempdir().expect("tmpdir");
        // Ostlandet-only install: manifest present but not Ready (no tiles) —
        // coverage still reports Vestlandet missing for Bergen.
        fs::write(
            dir.path().join("ostlandet-latest.navi-manifest.json"),
            r#"{"schema":1,"stem":"ostlandet-latest","pbf_filename":"ostlandet-latest.osm.pbf","graph_files":{},"graph_format_version":9,"poi_barrier_file":"ostlandet-latest.navi-poi-barrier.rkyv","poi_barrier_format_version":2,"wetland_format_version":1}"#,
        )
        .unwrap();
        let pbf = dir.path().join("ostlandet-latest.osm.pbf");
        fs::write(&pbf, b"stub").unwrap();
        let pts = [(60.726_f64, 10.613_f64), (60.3913_f64, 5.3221_f64)];
        let t0 = Instant::now();
        let result = try_load_graph_for_plan_corridor_with_pack_dirs(
            dir.path(),
            &[],
            &pbf,
            RoutingProfile::Car,
            Some([60.0, 5.0, 61.0, 11.0]),
            Some(&pts),
            PlanEdgeClipMode::CorridorBand,
        );
        let elapsed = t0.elapsed();
        let err = match result {
            Ok(_) => panic!("must fail without Vestlandet"),
            Err(e) => e,
        };
        match err {
            PackLoadError::MissingRegions(regions) => {
                assert!(
                    regions.iter().any(|r| r.contains("vestlandet")),
                    "expected Vestlandet named, got {regions:?}"
                );
            }
            other => panic!("expected MissingRegions, got {other:?}"),
        }
        assert!(
            elapsed.as_secs_f64() < 2.0,
            "fail-fast must return within 2s, took {elapsed:?}"
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

#[cfg(test)]
mod ferry_overlay_tests {
    use super::*;
    use crate::routing::graph::{GraphEdge, RouteGraph, RoutingProfile, SurfaceQuality};
    use geo_types::Coord;
    use osm4routing::{Node, NodeId};
    use std::collections::BTreeMap;

    fn land_only_graph() -> RouteGraph {
        let mut nodes = HashMap::new();
        for (id, lat, lon) in [(1i64, 54.5, 11.2), (2, 54.5, 11.21)] {
            nodes.insert(
                NodeId(id),
                Node {
                    id: NodeId(id),
                    coord: Coord { x: lon, y: lat },
                    uses: 2,
                },
            );
        }
        let len = 700.0;
        let edges = vec![
            GraphEdge {
                id: "a".into(),
                source: NodeId(1),
                target: NodeId(2),
                length_m: len,
                base_weight: len,
                cost_mult: 1.0,
                eco_weight: None,
                start_lat: 54.5,
                start_lon: 11.2,
                end_lat: 54.5,
                end_lon: 11.21,
                shape: Vec::new(),
                highway: Some("primary".into()),
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
                ferry_interval_min: None,
                is_tunnel: false,
                is_boardwalk_crossing: false,
                is_roundabout: false,
                motor_vehicle_conditional: None,
                access_conditional: None,
                maxspeed_conditional: None,
                access_forbidden: false,
                surface_quality: SurfaceQuality::Good,
            },
            GraphEdge {
                id: "b".into(),
                source: NodeId(2),
                target: NodeId(1),
                length_m: len,
                base_weight: len,
                cost_mult: 1.0,
                eco_weight: None,
                start_lat: 54.5,
                start_lon: 11.21,
                end_lat: 54.5,
                end_lon: 11.2,
                shape: Vec::new(),
                highway: Some("primary".into()),
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
                ferry_interval_min: None,
                is_tunnel: false,
                is_boardwalk_crossing: false,
                is_roundabout: false,
                motor_vehicle_conditional: None,
                access_conditional: None,
                maxspeed_conditional: None,
                access_forbidden: false,
                surface_quality: SurfaceQuality::Good,
            },
        ];
        RouteGraph::from_parts(nodes, edges, RoutingProfile::Car)
    }

    #[test]
    fn stub_pbf_is_not_treated_as_real_extract() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let stub = dir.path().join("denmark-latest.osm.pbf");
        std::fs::write(&stub, vec![0u8; 16 * 1024]).expect("stub");
        assert!(!pbf_is_real_extract(&stub));
    }

    #[test]
    fn supplement_without_real_pbf_does_not_invent_ferry_edges() {
        // Offline unit test: stub only, no network Geofabrik. Must not inject
        // hardcoded Fehmarn (or any) ferry — wait for real OSM overlay data.
        let dir = tempfile::tempdir().expect("tmpdir");
        let stem = "denmark-latest";
        std::fs::write(
            dir.path().join(format!("{stem}.osm.pbf")),
            vec![0u8; 16 * 1024],
        )
        .expect("stub pbf");
        // Point stem at a non-Geofabrik name so ensure is skipped.
        let stem = "not-a-geofabrik-stem";
        let man = NaviManifest {
            schema: NaviManifest::SCHEMA,
            stem: stem.into(),
            pbf_filename: format!("{stem}.osm.pbf"),
            pbf_size_bytes: 16 * 1024,
            pbf_modified_unix_secs: 1,
            graph_files: BTreeMap::new(),
            graph_tiles: BTreeMap::new(),
            graph_format_version: GRAPH_FORMAT_VERSION,
            poi_barrier_file: format!("{stem}.navi-poi-barrier.rkyv"),
            poi_barrier_format_version: POI_BARRIER_FORMAT_VERSION,
            wetland_file: None,
            wetland_tiles: Vec::new(),
            wetland_format_version: WETLAND_FORMAT_VERSION,
            has_delta_h: false,
            elev_dir: None,
        };
        std::fs::write(
            dir.path().join(format!("{stem}.osm.pbf")),
            vec![0u8; 16 * 1024],
        )
        .expect("stub");
        let g = land_only_graph();
        let before = g.edges.len();
        let out = supplement_pack_ferries_from_pbf(
            std::sync::Arc::new(g),
            &[dir.path()],
            &man,
            &[],
            RoutingProfile::Car,
            Some([54.15, 10.90, 55.25, 11.85]),
            None,
            Some(&[(54.21, 11.025), (55.175, 11.700)]),
        )
        .expect("stub pbf must not block");
        assert_eq!(out.edges.len(), before);
        assert!(!graph_has_long_ferry(&out));
    }

    fn ferry_edge(
        id: &str,
        source: i64,
        target: i64,
        start: (f64, f64),
        end: (f64, f64),
        length_m: f64,
    ) -> GraphEdge {
        GraphEdge {
            id: id.into(),
            source: NodeId(source),
            target: NodeId(target),
            length_m,
            base_weight: length_m,
            cost_mult: 1.0,
            eco_weight: None,
            start_lat: start.0,
            start_lon: start.1,
            end_lat: end.0,
            end_lon: end.1,
            shape: Vec::new(),
            highway: None,
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
            is_ferry: true,
            ferry_interval_min: None,
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
    fn disconnected_pack_with_orphan_ferry_does_not_skip_overlay() {
        // Legacy whole-graph long-ferry check would skip overlay; hop A* must not.
        let mut nodes = HashMap::new();
        for (id, lat, lon) in [
            (1i64, 54.21, 11.025),
            (2, 54.22, 11.03),
            (3, 55.175, 11.70),
            (4, 55.18, 11.71),
            (5, 54.50, 11.23),
            (6, 54.66, 11.35),
        ] {
            nodes.insert(
                NodeId(id),
                Node {
                    id: NodeId(id),
                    coord: Coord { x: lon, y: lat },
                    uses: 2,
                },
            );
        }
        let mut edges = vec![
            ferry_edge("belt", 5, 6, (54.50, 11.23), (54.66, 11.35), 19_000.0),
            ferry_edge("belt_r", 6, 5, (54.66, 11.35), (54.50, 11.23), 19_000.0),
        ];
        // Land only at hop endpoints — not connected through the orphan ferry.
        let mut la = ferry_edge("la", 1, 2, (54.21, 11.025), (54.22, 11.03), 800.0);
        la.is_ferry = false;
        la.highway = Some("primary".into());
        let mut lb = ferry_edge("lb", 3, 4, (55.175, 11.70), (55.18, 11.71), 800.0);
        lb.is_ferry = false;
        lb.highway = Some("primary".into());
        edges.push(la);
        edges.push(lb);
        let g = RouteGraph::from_parts(nodes, edges, RoutingProfile::Car);
        assert!(graph_has_long_ferry(&g));
        assert!(
            !graph_hop_already_connected(&g, (54.21, 11.025), (55.175, 11.700)),
            "orphan water ferry must leave densify hop disconnected"
        );
    }

    #[test]
    fn skane_leaf_shares_sweden_country_pbf_stem() {
        assert_eq!(
            country_extract_pbf_stem("skane-latest").as_deref(),
            Some("sweden-latest")
        );
        assert_eq!(
            country_extract_pbf_stem("halland-latest").as_deref(),
            Some("sweden-latest")
        );
        assert!(
            country_extract_pbf_stem("denmark-latest").is_none(),
            "country extracts must not recurse"
        );
        assert!(country_extract_pbf_stem("schleswig-holstein-latest").is_none());
    }

    #[test]
    fn connected_hop_skips_overlay_gate() {
        let mut nodes = HashMap::new();
        for (id, lat, lon) in [
            (1i64, 54.21, 11.025),
            (2, 54.50, 11.23),
            (3, 54.66, 11.35),
            (4, 55.175, 11.70),
        ] {
            nodes.insert(
                NodeId(id),
                Node {
                    id: NodeId(id),
                    coord: Coord { x: lon, y: lat },
                    uses: 2,
                },
            );
        }
        let mut edges = vec![
            ferry_edge("belt", 2, 3, (54.50, 11.23), (54.66, 11.35), 19_000.0),
            ferry_edge("belt_r", 3, 2, (54.66, 11.35), (54.50, 11.23), 19_000.0),
        ];
        let mut a = ferry_edge("a", 1, 2, (54.21, 11.025), (54.50, 11.23), 40_000.0);
        a.is_ferry = false;
        a.highway = Some("primary".into());
        let mut b = ferry_edge("b", 3, 4, (54.66, 11.35), (55.175, 11.70), 60_000.0);
        b.is_ferry = false;
        b.highway = Some("primary".into());
        edges.push(a);
        edges.push(b);
        let g = RouteGraph::from_parts(nodes, edges, RoutingProfile::Car);
        assert!(graph_hop_already_connected(
            &g,
            (54.21, 11.025),
            (55.175, 11.700)
        ));
    }

    /// Live pack probe after catalog-union rebake. Run:
    /// `NAVI_FEHMARN_PROBE_DIR=... cargo test -p driver-break-core fehmarn_rebake_tiles_have_puttgarden -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn fehmarn_rebake_tiles_have_puttgarden() {
        let dir = match std::env::var("NAVI_FEHMARN_PROBE_DIR") {
            Ok(d) => PathBuf::from(d),
            Err(_) => {
                eprintln!("skip: set NAVI_FEHMARN_PROBE_DIR");
                return;
            }
        };
        let sh_tile = dir.join("schleswig-holstein-latest.navi-graph-car.t1_3.rkyv");
        let dk_tile = dir.join("denmark-latest.navi-graph-car.t0_3.rkyv");
        let sh_side = dir.join("schleswig-holstein-latest.navi-ferry-overlay-truck.rkyv");
        let dk_side = dir.join("denmark-latest.navi-ferry-overlay-truck.rkyv");
        for p in [&sh_tile, &dk_tile] {
            assert!(p.is_file(), "missing {}", p.display());
        }
        let profile = RoutingProfile::Truck;
        let hop = [(54.21_f64, 11.025), (55.175, 11.700)];
        let clip = [53.86_f64, 10.675, 55.525, 12.050];
        let sh = load_graph_pack_clips(&sh_tile, profile, Some(&[clip])).expect("sh");
        let dk = load_graph_pack_clips(&dk_tile, profile, Some(&[clip])).expect("dk");
        let sh_ferry = sh.edges.iter().filter(|e| e.is_ferry).count();
        let dk_ferry = dk.edges.iter().filter(|e| e.is_ferry).count();
        eprintln!(
            "SH t1_3 clip ferry_edges={sh_ferry} nodes={}",
            sh.nodes.len()
        );
        eprintln!(
            "DK t0_3 clip ferry_edges={dk_ferry} nodes={}",
            dk.nodes.len()
        );
        let merged = merge_tile_graphs(vec![sh, dk], profile);
        let opts = crate::routing::graph::RouteOptions::default();
        let puttgarden_m = merged
            .nearest_routable_with_options_max(54.503, 11.227, &opts, false, 25_000.0)
            .map(|(_, d)| d)
            .unwrap_or(f64::MAX);
        let rodby_m = merged
            .nearest_routable_with_options_max(54.655, 11.352, &opts, false, 25_000.0)
            .map(|(_, d)| d)
            .unwrap_or(f64::MAX);
        eprintln!("nearest pack node Puttgarden={puttgarden_m:.0} m Rødby={rodby_m:.0} m");
        eprintln!(
            "hop_connected_without_overlay={}",
            graph_hop_already_connected(&merged, hop[0], hop[1])
        );
        assert!(
            puttgarden_m < 1_500.0,
            "rebaked SH t1_3 must contain Puttgarden; nearest_m={puttgarden_m:.0}"
        );
        assert!(
            rodby_m < 1_500.0,
            "rebaked DK t0_3 must contain Rødby; nearest_m={rodby_m:.0}"
        );
        if sh_side.is_file() && dk_side.is_file() {
            let sh_ov =
                load_graph_pack_clips(&sh_side, profile, Some(&[clip])).expect("sh overlay");
            let dk_ov =
                load_graph_pack_clips(&dk_side, profile, Some(&[clip])).expect("dk overlay");
            eprintln!(
                "overlay clip SH ferry={} DK ferry={}",
                sh_ov.edges.iter().filter(|e| e.is_ferry).count(),
                dk_ov.edges.iter().filter(|e| e.is_ferry).count()
            );
        }
    }
}

#[cfg(test)]
mod raufoss_bergen_load_probe {
    use super::*;
    use crate::routing::graph::RoutingProfile;
    use crate::routing::plan_bbox::PlanEdgeClipMode;
    use std::path::Path;
    use std::time::Instant;

    #[test]
    #[ignore = "needs /tmp/navi_pack_install with ostlandet+vestlandet v9 packs"]
    fn probe_raufoss_bergen_pack_load() {
        let pack = Path::new("/tmp/navi_pack_install");
        assert!(pack.join("ostlandet-latest.navi-manifest.json").is_file());
        assert!(pack.join("vestlandet-latest.navi-manifest.json").is_file());
        let pbf = pack.join("ostlandet-latest.osm.pbf");
        let pts = [(60.7277483_f64, 10.6109403_f64), (60.388144, 5.3347434)];
        let bbox = crate::routing::plan_bbox::trip_bbox_points(&pts, 0.52);
        eprintln!("bbox={bbox:?}");
        let t0 = Instant::now();
        let g = try_load_graph_for_plan_corridor_with_pack_dirs(
            pack,
            &[],
            &pbf,
            RoutingProfile::Car,
            Some(bbox),
            Some(&pts),
            PlanEdgeClipMode::CorridorBand,
        )
        .expect("pack load");
        eprintln!(
            "nodes={} edges={} ms={}",
            g.nodes.len(),
            g.edges.len(),
            t0.elapsed().as_millis()
        );
        // Destination must be snappable: a node within 2 km of Bergen.
        let (ok, dist) = match g.nearest_routable(pts[1].0, pts[1].1) {
            Ok((_, d)) => (true, d),
            Err(e) => (false, e.nearest_m),
        };
        eprintln!("bergen_snap_ok={ok} nearest_m={dist:.0}");
        assert!(
            ok && dist < 2000.0,
            "Bergen not in loaded graph; nearest_m={dist}"
        );
    }
}

#[cfg(test)]
mod hop_primary_tests {
    use super::hop_prefers_dest_primary;

    #[test]
    fn dest_primary_when_leaves_differ() {
        assert!(hop_prefers_dest_primary(
            Some("europe/norway/ostlandet"),
            Some("europe/norway/vestlandet"),
        ));
        assert!(!hop_prefers_dest_primary(
            Some("europe/norway/ostlandet"),
            Some("europe/norway/ostlandet"),
        ));
        assert!(hop_prefers_dest_primary(
            None,
            Some("europe/norway/vestlandet"),
        ));
        assert!(!hop_prefers_dest_primary(
            Some("europe/norway/ostlandet"),
            None,
        ));
    }
}
