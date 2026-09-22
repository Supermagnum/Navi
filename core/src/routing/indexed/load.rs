//! Load + validate indexed packs (never interpret mismatched versions).

use std::fs::{self, File};
use std::path::{Path, PathBuf};

use memmap2::Mmap;
use rkyv::rancor::Error as RkyvError;
use thiserror::Error;

use super::graph_pack::{ArchivedFlatGraphPack, FlatGraphPack, GRAPH_FORMAT_VERSION, MAGIC_GRAPH};
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
    extra_corridor_manifests_segs(data_dir, primary_stem, &[bbox])
}

fn extra_corridor_manifests_segs(
    data_dir: &Path,
    primary_stem: &str,
    segs: &[[f64; 4]],
) -> Vec<NaviManifest> {
    let Ok(entries) = fs::read_dir(data_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for ent in entries.flatten() {
        let name = ent.file_name();
        let name = name.to_string_lossy();
        let Some(stem) = name.strip_suffix(".navi-manifest.json") else {
            continue;
        };
        if stem == primary_stem {
            continue;
        }
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
        if !segs.iter().any(|b| bbox_intersects(region, *b)) {
            continue;
        }
        out.push(man);
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
fn select_tiles_within_budget(
    mut candidates: Vec<(String, [f64; 4])>,
    route_points: Option<&[(f64, f64)]>,
    max_tiles: usize,
    data_dir: Option<&Path>,
) -> Vec<String> {
    if candidates.len() <= max_tiles {
        candidates.sort_by(|a, b| a.0.cmp(&b.0));
        return candidates.into_iter().map(|(f, _)| f).collect();
    }
    let pts = route_points.unwrap_or(&[]);
    let file_len = |name: &str| -> u64 {
        data_dir
            .and_then(|d| fs::metadata(d.join(name)).ok())
            .map(|m| m.len())
            .unwrap_or(u64::MAX)
    };

    // Guarantee coverage of each route point and corridor samples so a tight
    // tile budget cannot drop the bridge between start and end (disconnected).
    // One midpoint is not enough when endpoint tiles do not touch (SA t2_1 and
    // NI t2_4 on Stendal→Hannover); quarter-points pull the intervening leaves.
    let mut samples: Vec<(f64, f64)> = pts.to_vec();
    if pts.len() >= 2 {
        for w in pts.windows(2) {
            for &t in &[0.25_f64, 0.5, 0.75] {
                samples.push((
                    w[0].0 + (w[1].0 - w[0].0) * t,
                    w[0].1 + (w[1].1 - w[0].1) * t,
                ));
            }
        }
    }
    let mut selected: Vec<(String, [f64; 4])> = Vec::new();
    let mut selected_names = HashSet::new();
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
        for (_, i) in best_per_stem.values() {
            let (name, bbox) = candidates[*i].clone();
            if selected_names.insert(name.clone()) {
                selected.push((name, bbox));
            }
        }
    }

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

    let mut rest: Vec<(String, [f64; 4])> = candidates
        .into_iter()
        .filter(|(n, _)| !selected_names.contains(n))
        .collect();
    rest.sort_by(|a, b| {
        let sa = score(a.1);
        let sb = score(b.1);
        sa.cmp(&sb)
            .then_with(|| file_len(&a.0).cmp(&file_len(&b.0)))
            .then_with(|| a.0.cmp(&b.0))
    });
    for (name, bbox) in rest {
        if selected.len() >= max_tiles {
            break;
        }
        if selected_names.insert(name.clone()) {
            selected.push((name, bbox));
        }
    }
    // If endpoint guarantees already exceeded budget, keep them anyway — snap
    // failure is worse than a slightly higher peak RSS.
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
    let mmap = map_file(path)?;
    check_preamble(&mmap, MAGIC_GRAPH, GRAPH_FORMAT_VERSION)?;
    let body = &mmap[archive_payload_offset()..];
    let archived = rkyv::access::<ArchivedFlatGraphPack, RkyvError>(body)
        .map_err(|e| PackLoadError::Rkyv(e.to_string()))?;
    let pack: FlatGraphPack = rkyv::deserialize::<FlatGraphPack, RkyvError>(archived)
        .map_err(|e| PackLoadError::Rkyv(e.to_string()))?;
    Ok(pack.to_route_graph_bbox(profile, bbox))
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
    let pbf_stem = planning_stem(pbf)?;
    // Chunked long-trip legs still pass the origin PBF; re-home primary to the
    // Ready stem that covers the hop start so we do not merge Sachsen-Anhalt
    // tiles into every Norway leg.
    let (stem, man) = pick_primary_manifest(data_dir, &pbf_stem, route_points)?;
    if stem == pbf_stem {
        match status_for_planning_pbf(data_dir, pbf, &man)? {
            PackStatus::Ready => {}
            PackStatus::Missing => return Err(PackLoadError::Missing),
            PackStatus::StalePbf => return Err(PackLoadError::Stale),
            PackStatus::VersionMismatch => return Err(PackLoadError::VersionMismatch),
        }
    } else if !stem_pack_ready(data_dir, &man) {
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
    // Modest keep-pad so legal detours near the corridor stay; keep tight for
    // 4 GB peak RSS (wide pads materialize too many edges per tile).
    let edge_clip = corridor_segs
        .as_ref()
        .and_then(|segs| union_bboxes(segs))
        .map(|b| expand_bbox_deg(b, 0.15))
        .or(clip_bbox);

    let need_extra = corridor_needs_extra_stems(&stem, clip_bbox.or(edge_clip));
    let mut extras = if need_extra {
        if let Some(segs) = segs_ref {
            extra_corridor_manifests_segs(data_dir, &stem, segs)
        } else if let Some(b) = clip_bbox {
            extra_corridor_manifests(data_dir, &stem, b)
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
         need_extra={need_extra} extras={} corridor_segs={} data_dir={}",
        extras.len(),
        corridor_segs.as_ref().map(|s| s.len()).unwrap_or(0),
        data_dir.display()
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
        crate::routing::plan_bbox::MAX_PLAN_TILES,
        Some(data_dir),
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
                            let mut seen: HashSet<String> = tile_files.iter().cloned().collect();
                            for t in tiles {
                                let hit = segs_ref.is_some_and(|segs| {
                                    segs.iter().any(|s| bbox_intersects(t.bbox, *s))
                                }) || clip_bbox
                                    .is_some_and(|b| bbox_intersects(t.bbox, b));
                                if hit && seen.insert(t.file.clone()) {
                                    tile_files.push(t.file.clone());
                                }
                            }
                            // Neighbour stems near a densify endpoint (Halland
                            // north of Skåne, Denmark east of SH) must stay even
                            // when the primary stem already filled the tile budget.
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
                                            tile_files.push(t.file.clone());
                                        }
                                    }
                                }
                            }
                            tile_files.sort();
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
        crate::routing::plan_bbox::MAX_PLAN_TILES
    );

    if !tile_files.is_empty() {
        let mut graphs = vec![load_tiled_graph_files(
            data_dir, tile_files, profile, edge_clip,
        )?];
        // City-state packs (e.g. hamburg) are often a single untiled .rkyv.
        // Merge them whether they are the primary stem or an extra — otherwise
        // a hop that starts on a Hamburg densify anchor loads only neighbour
        // tiles and cannot snap (same 6 km miss as a dropped destination pack).
        let primary_tiled = man.graph_tiles_for(profile).is_some_and(|t| !t.is_empty());
        if !primary_tiled {
            if let Some(pp) = man.graph_path(data_dir, profile) {
                graphs.push(load_graph_pack_bbox(&pp, profile, edge_clip)?);
            }
        }
        for extra in &extras {
            let tiled = extra
                .graph_tiles_for(profile)
                .is_some_and(|t| !t.is_empty());
            if tiled {
                continue;
            }
            if let Some(ep) = extra.graph_path(data_dir, profile) {
                graphs.push(load_graph_pack_bbox(&ep, profile, edge_clip)?);
            }
        }
        if graphs.len() == 1 {
            return Ok(graphs.pop().unwrap());
        }
        let merged = merge_tile_graphs(graphs, profile);
        if merged.edges.is_empty() {
            return Err(PackLoadError::Missing);
        }
        return Ok(merged);
    }

    let mut graphs = Vec::new();
    let path = man
        .graph_path(data_dir, profile)
        .ok_or(PackLoadError::Missing)?;
    graphs.push(load_graph_pack_bbox(&path, profile, edge_clip)?);
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
            } else if let Some(ep) = extra.graph_path(data_dir, profile) {
                graphs.push(load_graph_pack_bbox(&ep, profile, edge_clip)?);
            }
        }
        let extra_files = select_tiles_within_budget(
            extra_candidates,
            route_points,
            crate::routing::plan_bbox::MAX_PLAN_TILES,
            Some(data_dir),
        );
        if !extra_files.is_empty() {
            graphs.push(load_tiled_graph_files(
                data_dir,
                extra_files,
                profile,
                edge_clip,
            )?);
        }
    }
    let merged = merge_tile_graphs(graphs, profile);
    if merged.edges.is_empty() {
        return Err(PackLoadError::Missing);
    }
    Ok(merged)
}

/// Choose the Ready manifest for planning: prefer a stem whose region covers
/// the first route point when the PBF stem does not (chunked long-trip hops).
fn pick_primary_manifest(
    data_dir: &Path,
    pbf_stem: &str,
    route_points: Option<&[(f64, f64)]>,
) -> Result<(String, NaviManifest), PackLoadError> {
    let default = load_ready_manifest(data_dir, pbf_stem)?;
    let Some(pts) = route_points else {
        return Ok((pbf_stem.to_string(), default));
    };
    if pts.is_empty() {
        return Ok((pbf_stem.to_string(), default));
    }
    let (lat, lon) = pts[0];
    if let Some(path) = pbf_stem_to_geofabrik_path(pbf_stem) {
        if let Some(region) = region_bbox(&path) {
            if crate::routing::basemap::bbox_covers_point(region, lat, lon) {
                return Ok((pbf_stem.to_string(), default));
            }
        }
    }
    // Scan Ready manifests for a covering stem; pick the smallest covering bbox.
    let Ok(entries) = fs::read_dir(data_dir) else {
        return Ok((pbf_stem.to_string(), default));
    };
    let mut best: Option<(f64, String, NaviManifest)> = None;
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
        match &best {
            Some((a, _, _)) if *a <= area => {}
            _ => best = Some((area, man.stem.clone(), man)),
        }
    }
    if let Some((_, stem, man)) = best {
        return Ok((stem, man));
    }
    Ok((pbf_stem.to_string(), default))
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
    data_dir: &Path,
    mut tile_files: Vec<String>,
    profile: RoutingProfile,
    bbox: Option<[f64; 4]>,
) -> Result<RouteGraph, PackLoadError> {
    if tile_files.is_empty() {
        return Err(PackLoadError::Missing);
    }
    tile_files.sort();

    // Always sequential + incremental merge: never hold all tile graphs in RAM.
    let mut merged: Option<RouteGraph> = None;
    for file in &tile_files {
        let path = data_dir.join(file);
        let g = load_graph_pack_bbox(&path, profile, bbox)?;
        if g.edges.is_empty() && g.nodes.is_empty() {
            continue;
        }
        merged = Some(match merged {
            None => g,
            Some(acc) => merge_tile_graphs(vec![acc, g], profile),
        });
    }
    let merged = merged.ok_or(PackLoadError::Missing)?;
    if merged.edges.is_empty() {
        return Err(PackLoadError::Missing);
    }
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
    let Ok(entries) = fs::read_dir(data_dir) else {
        return Err(PackLoadError::Missing);
    };
    let mut best: Option<(f64, NaviManifest)> = None;
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
        if best.as_ref().is_none_or(|(ba, _)| area < *ba) {
            best = Some((area, man));
        }
    }
    let Some((_, man)) = best else {
        return Err(PackLoadError::Missing);
    };
    load_poi_barrier_pack(&man.poi_barrier_path(data_dir))
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
    use super::{bbox_contained, corridor_needs_extra_stems};

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
}

#[cfg(test)]
mod fingerprint_pbf_tests {
    use super::*;
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
}
