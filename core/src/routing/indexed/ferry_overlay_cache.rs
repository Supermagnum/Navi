//! Stem-level ferry overlay sidecar: build once from PBF, reuse at plan time.
//!
//! Plan path must not re-parse Geofabrik / ferry PBFs. The sidecar is a v9
//! [`FlatGraphPack`] of ferry + pier-approach edges for the stem's region bbox,
//! invalidated when the source PBF size/mtime changes.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use memmap2::Mmap;
use rkyv::rancor::Error as RkyvError;

use crate::routing::graph::{RouteGraph, RoutingProfile};
use crate::routing::indexed::graph_pack::{
    ArchivedFlatGraphPack, FlatGraphPack, GRAPH_FORMAT_VERSION, MAGIC_GRAPH,
};
use crate::routing::indexed::header::Preamble;
use crate::routing::indexed::io::{archive_payload_offset, write_archive_atomic};

fn profile_slug(profile: RoutingProfile) -> &'static str {
    match profile {
        RoutingProfile::Car => "car",
        RoutingProfile::Bicycle => "bicycle",
        RoutingProfile::Foot => "foot",
        RoutingProfile::Truck => "truck",
    }
}

pub fn ferry_sidecar_path(home: &Path, stem: &str, profile: RoutingProfile) -> PathBuf {
    home.join(format!(
        "{stem}.navi-ferry-overlay-{}.rkyv",
        profile_slug(profile)
    ))
}

fn ferry_sidecar_meta_path(home: &Path, stem: &str, profile: RoutingProfile) -> PathBuf {
    home.join(format!(
        "{stem}.navi-ferry-overlay-{}.meta",
        profile_slug(profile)
    ))
}

fn pbf_fingerprint(pbf: &Path) -> Option<String> {
    let meta = fs::metadata(pbf).ok()?;
    let len = meta.len();
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Some(format!("len={len};mtime={mtime}"))
}

fn sidecar_fresh(home: &Path, stem: &str, profile: RoutingProfile, pbf: &Path) -> bool {
    let side = ferry_sidecar_path(home, stem, profile);
    let meta_path = ferry_sidecar_meta_path(home, stem, profile);
    if !side.is_file() || !meta_path.is_file() {
        return false;
    }
    let Some(want) = pbf_fingerprint(pbf) else {
        return false;
    };
    let got = fs::read_to_string(&meta_path).unwrap_or_default();
    got.trim() == want
}

fn write_sidecar(home: &Path, stem: &str, profile: RoutingProfile, pbf: &Path, graph: &RouteGraph) {
    let side = ferry_sidecar_path(home, stem, profile);
    let meta_path = ferry_sidecar_meta_path(home, stem, profile);
    let pack = FlatGraphPack::from_route_graph(graph, None);
    let Ok(payload) = rkyv::to_bytes::<RkyvError>(&pack) else {
        log::warn!(target: "NaviPlan", "ferry_sidecar serialize failed stem={stem}");
        return;
    };
    if let Err(e) = write_archive_atomic(
        &side,
        Preamble::new(MAGIC_GRAPH, GRAPH_FORMAT_VERSION),
        &payload,
    ) {
        log::warn!(target: "NaviPlan", "ferry_sidecar write failed stem={stem}: {e:#}");
        return;
    }
    if let Some(fp) = pbf_fingerprint(pbf) {
        let _ = fs::write(&meta_path, fp);
    }
    crate::routing::plan_perf::note(
        "ferry_sidecar_write",
        format!(
            "stem={stem};path={};edges={};bytes={}",
            side.display(),
            graph.edges.len(),
            payload.len()
        ),
    );
}

fn load_sidecar_clipped(
    path: &Path,
    profile: RoutingProfile,
    clips: Option<&[[f64; 4]]>,
) -> Option<RouteGraph> {
    let file = std::fs::File::open(path).ok()?;
    let mmap = unsafe { Mmap::map(&file).ok()? };
    let p = Preamble::from_bytes(&mmap)?;
    if p.magic != MAGIC_GRAPH || p.format_version != GRAPH_FORMAT_VERSION {
        return None;
    }
    let body = &mmap[archive_payload_offset()..];
    let archived = rkyv::access::<ArchivedFlatGraphPack, RkyvError>(body).ok()?;
    Some(archived.to_route_graph_clips(profile, clips))
}

fn stem_region_bbox(stem: &str) -> [f64; 4] {
    crate::routing::basemap::pbf_stem_to_geofabrik_path(stem)
        .and_then(|path| crate::routing::basemap::region_bbox(&path))
        .unwrap_or([-90.0, -180.0, 90.0, 180.0])
}

/// Load ferry+approach overlay for `stem` clipped to `bbox`, building the
/// stem sidecar from `pbf` on first use / stale fingerprint.
///
/// Returns `(graph, mode)` where `mode` is `"sidecar"` (cache hit) or
/// `"pbf_build"` (first use / stale; writes sidecar, no PBF on later plans).
pub fn ferry_overlay_for_plan(
    home: &Path,
    stem: &str,
    profile: RoutingProfile,
    pbf: &Path,
    bbox: [f64; 4],
) -> Option<(RouteGraph, &'static str)> {
    let clips = [bbox];
    let t0 = Instant::now();
    if sidecar_fresh(home, stem, profile, pbf) {
        let side = ferry_sidecar_path(home, stem, profile);
        let bytes = fs::metadata(&side).map(|m| m.len()).unwrap_or(0);
        let g = load_sidecar_clipped(&side, profile, Some(&clips))?;
        crate::routing::plan_perf::note(
            "ferry_sidecar",
            format!(
                "hit;stem={stem};bytes={bytes};edges={};load_ms={}",
                g.edges.len(),
                t0.elapsed().as_millis()
            ),
        );
        crate::routing::plan_perf::note_u64("ferry_pbf_parse_ms", 0);
        crate::routing::plan_perf::note_u64("ferry_pbf_bytes", 0);
        return Some((g, "sidecar"));
    }

    let region = stem_region_bbox(stem);
    let pbf_bytes = fs::metadata(pbf).map(|m| m.len()).unwrap_or(0);
    crate::routing::plan_perf::note(
        "ferry_pbf",
        format!("path={};bytes={pbf_bytes};region_build=1", pbf.display()),
    );
    crate::routing::plan_perf::note_u64("ferry_pbf_bytes", pbf_bytes);
    let t_parse = Instant::now();
    let full = match RouteGraph::build_ferry_overlay_from_pbf(pbf, profile, region) {
        Ok(g) => g,
        Err(e) => {
            log::warn!(target: "NaviPlan", "ferry_sidecar build failed stem={stem}: {e:#}");
            return None;
        }
    };
    let parse_ms = t_parse.elapsed().as_millis() as u64;
    crate::routing::plan_perf::note_u64("ferry_pbf_parse_ms", parse_ms);
    if full.edges.iter().any(|e| e.is_ferry) {
        write_sidecar(home, stem, profile, pbf, &full);
    }
    // Clip for this plan (same predicate as pack hydrate).
    let clipped = {
        let in_box = |lat: f64, lon: f64, b: &[f64; 4]| {
            lat >= b[0] && lat <= b[2] && lon >= b[1] && lon <= b[3]
        };
        let edge_ok = |e: &crate::routing::graph::GraphEdge| {
            in_box(e.start_lat, e.start_lon, &bbox) || in_box(e.end_lat, e.end_lon, &bbox)
        };
        let mut nodes = std::collections::HashMap::new();
        let mut edges = Vec::new();
        for e in &full.edges {
            if !edge_ok(e) {
                continue;
            }
            if let Some(n) = full.nodes.get(&e.source) {
                nodes.insert(e.source, *n);
            }
            if let Some(n) = full.nodes.get(&e.target) {
                nodes.insert(e.target, *n);
            }
            edges.push(e.clone());
        }
        RouteGraph::from_parts(nodes, edges, profile)
    };
    crate::routing::plan_perf::note(
        "ferry_sidecar",
        format!(
            "miss_build;stem={stem};pbf_bytes={pbf_bytes};full_edges={};clip_edges={};ms={}",
            full.edges.len(),
            clipped.edges.len(),
            t0.elapsed().as_millis()
        ),
    );
    Some((clipped, "pbf_build"))
}
