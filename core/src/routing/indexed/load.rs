//! Load + validate indexed packs (never interpret mismatched versions).

use std::fs::File;
use std::path::{Path, PathBuf};

use memmap2::Mmap;
use rkyv::rancor::Error as RkyvError;
use thiserror::Error;

use super::graph_pack::{ArchivedFlatGraphPack, FlatGraphPack, GRAPH_FORMAT_VERSION, MAGIC_GRAPH};
use super::header::Preamble;
use super::io::archive_payload_offset;
use super::manifest::{bbox_intersects, manifest_path, NaviManifest, PackStatus};
use super::poi_barrier_pack::{
    ArchivedFlatPoiBarrierPack, FlatPoiBarrierPack, MAGIC_POI_BARRIER, POI_BARRIER_FORMAT_VERSION,
};
use super::wetland_pack::{
    ArchivedFlatWetlandPack, FlatWetlandPack, MAGIC_WETLAND, WETLAND_FORMAT_VERSION,
};
use crate::poi::PoiIndex;
use crate::routing::graph::{RouteGraph, RoutingProfile};
use crate::routing::safety::DangerBarrierIndex;
use crate::routing::wetland::WetlandIndex;
use std::collections::{HashMap, HashSet};

use rayon::prelude::*;

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
    let packed = fingerprint_pbf_for_packs(data_dir, planning_pbf, man)?;
    Ok(man.status_for_pbf(data_dir, &packed))
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
    let stem = pbf
        .file_name()
        .and_then(|s| s.to_str())
        .map(|name| {
            name.strip_suffix(".osm.pbf")
                .or_else(|| name.strip_suffix(".pbf"))
                .unwrap_or(name)
                .to_string()
        })
        .ok_or(PackLoadError::Missing)?;
    let man_path = manifest_path(data_dir, &stem);
    if !man_path.is_file() {
        return Err(PackLoadError::Missing);
    }
    let man = NaviManifest::load(&man_path).map_err(|_| PackLoadError::Missing)?;
    match status_for_planning_pbf(data_dir, pbf, &man)? {
        PackStatus::Ready => {}
        PackStatus::Missing => return Err(PackLoadError::Missing),
        PackStatus::StalePbf => return Err(PackLoadError::Stale),
        PackStatus::VersionMismatch => return Err(PackLoadError::VersionMismatch),
    }
    if let Some(tiles) = man.graph_tiles_for(profile) {
        return load_tiled_graph(data_dir, tiles, profile, bbox);
    }
    let path = man
        .graph_path(data_dir, profile)
        .ok_or(PackLoadError::Missing)?;
    load_graph_pack_bbox(&path, profile, bbox)
}

fn load_tiled_graph(
    data_dir: &Path,
    tiles: &[super::manifest::GraphTileEntry],
    profile: RoutingProfile,
    bbox: Option<[f64; 4]>,
) -> Result<RouteGraph, PackLoadError> {
    let mut selected: Vec<&super::manifest::GraphTileEntry> = match bbox {
        Some(b) => tiles
            .iter()
            .filter(|t| bbox_intersects(t.bbox, b))
            .collect(),
        None => tiles.iter().collect(),
    };
    if selected.is_empty() {
        return Err(PackLoadError::Missing);
    }
    // Deterministic merge order: sort by tile filename before parallel load so
    // HashMap insert / edge-id first-wins matches the prior sequential path.
    selected.sort_by(|a, b| a.file.cmp(&b.file));

    // Parallel mmap/deserialize. Rayon uses available parallelism (min-spec
    // floor is 8 cores); merge below stays sorted for deterministic first-wins.
    let graphs: Vec<RouteGraph> = selected
        .par_iter()
        .map(|t| {
            let path = data_dir.join(&t.file);
            load_graph_pack_bbox(&path, profile, bbox)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut nodes = HashMap::new();
    let mut edges = Vec::new();
    let mut seen_edge_ids = HashSet::new();
    // Merge in the same sorted order as the sequential path used.
    for g in graphs {
        for (id, node) in g.nodes {
            nodes.insert(id, node);
        }
        for e in g.edges {
            if seen_edge_ids.insert(e.id.clone()) {
                edges.push(e);
            }
        }
    }
    if edges.is_empty() {
        return Err(PackLoadError::Missing);
    }
    Ok(RouteGraph::from_parts(nodes, edges, profile))
}

pub fn try_load_poi_barrier_for_plan(
    data_dir: &Path,
    pbf: &Path,
) -> Result<(PoiIndex, DangerBarrierIndex), PackLoadError> {
    let stem = pbf
        .file_name()
        .and_then(|s| s.to_str())
        .map(|name| {
            name.strip_suffix(".osm.pbf")
                .or_else(|| name.strip_suffix(".pbf"))
                .unwrap_or(name)
                .to_string()
        })
        .ok_or(PackLoadError::Missing)?;
    let man_path = manifest_path(data_dir, &stem);
    if !man_path.is_file() {
        return Err(PackLoadError::Missing);
    }
    let man = NaviManifest::load(&man_path).map_err(|_| PackLoadError::Missing)?;
    match status_for_planning_pbf(data_dir, pbf, &man)? {
        PackStatus::Ready => {}
        PackStatus::Missing => return Err(PackLoadError::Missing),
        PackStatus::StalePbf => return Err(PackLoadError::Stale),
        PackStatus::VersionMismatch => return Err(PackLoadError::VersionMismatch),
    }
    load_poi_barrier_pack(&man.poi_barrier_path(data_dir))
}

/// Prefer indexed wetland pack when present and valid; else `Err` → PBF fallback.
///
/// Region-scale packs may store wetland as spatial tiles (`wetland_tiles`); those
/// are merged for the plan bbox. Monolith corridors use a single `wetland_file`.
pub fn try_load_wetland_for_plan(
    data_dir: &Path,
    pbf: &Path,
    bbox: Option<[f64; 4]>,
) -> Result<WetlandIndex, PackLoadError> {
    let stem = pbf
        .file_name()
        .and_then(|s| s.to_str())
        .map(|name| {
            name.strip_suffix(".osm.pbf")
                .or_else(|| name.strip_suffix(".pbf"))
                .unwrap_or(name)
                .to_string()
        })
        .ok_or(PackLoadError::Missing)?;
    let man_path = manifest_path(data_dir, &stem);
    if !man_path.is_file() {
        return Err(PackLoadError::Missing);
    }
    let man = NaviManifest::load(&man_path).map_err(|_| PackLoadError::Missing)?;
    match status_for_planning_pbf(data_dir, pbf, &man)? {
        PackStatus::Ready => {}
        PackStatus::Missing => return Err(PackLoadError::Missing),
        PackStatus::StalePbf => return Err(PackLoadError::Stale),
        PackStatus::VersionMismatch => return Err(PackLoadError::VersionMismatch),
    }
    if man.wetland_format_version != WETLAND_FORMAT_VERSION {
        return Err(PackLoadError::VersionMismatch);
    }
    if man.uses_wetland_tiles() {
        return load_tiled_wetland(data_dir, man.wetland_tiles(), bbox);
    }
    let Some(path) = man.wetland_path(data_dir) else {
        return Err(PackLoadError::Missing);
    };
    if !path.is_file() {
        return Err(PackLoadError::Missing);
    }
    load_wetland_pack(&path, bbox)
}

fn load_tiled_wetland(
    data_dir: &Path,
    tiles: &[super::manifest::GraphTileEntry],
    bbox: Option<[f64; 4]>,
) -> Result<WetlandIndex, PackLoadError> {
    let selected: Vec<&super::manifest::GraphTileEntry> = match bbox {
        Some(b) => tiles
            .iter()
            .filter(|t| bbox_intersects(t.bbox, b))
            .collect(),
        None => tiles.iter().collect(),
    };
    if selected.is_empty() {
        return Err(PackLoadError::Missing);
    }
    let mut merged = FlatWetlandPack::empty();
    for t in selected {
        let path = data_dir.join(&t.file);
        let mmap = map_file(&path)?;
        check_preamble(&mmap, MAGIC_WETLAND, WETLAND_FORMAT_VERSION)?;
        let body = &mmap[archive_payload_offset()..];
        let archived = rkyv::access::<ArchivedFlatWetlandPack, RkyvError>(body)
            .map_err(|e| PackLoadError::Rkyv(e.to_string()))?;
        let pack: FlatWetlandPack = rkyv::deserialize::<FlatWetlandPack, RkyvError>(archived)
            .map_err(|e| PackLoadError::Rkyv(e.to_string()))?;
        merged.extend_from(&pack);
    }
    Ok(merged.to_wetland_index(bbox))
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
