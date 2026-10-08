//! On-device corridor skeleton build (FU22 Step 1).
//!
//! Host CLI [`navi-corridor-skeleton`] builds skeletons offline; this module
//! mirrors that tile-at-a-time pipeline for idle background work beside installed
//! packs. Peak RSS stays far below 4 GB by loading one tile under
//! [`super::with_corridor_skeleton_hydrate`] then converting to [`FlatGraphPack`]
//! before the next tile.
//!
//! Soft RSS bound: [`SKELETON_BUILD_SOFT_RSS_MB`]. After each tile load, if
//! VmRSS exceeds that budget the tile is skipped (warn) and the build continues.
//! VmHWM is logged for diagnostics. Densify-only materialization is the primary
//! bound mechanism; the soft check aborts pathological tiles.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use crate::routing::corridor_skeleton::{
    build_skeleton_from_pack, major_node_osm_ids, merge_skeleton_files, read_skeleton_file,
    skeleton_path, write_skeleton_file, CorridorSkeletonFile, CORRIDOR_SKELETON_FORMAT_VERSION,
};
use crate::routing::graph::RoutingProfile;
use super::ferry_overlay_cache::{ferry_sidecar_path, load_ferry_overlay_as_flat};
use super::graph_pack::{with_corridor_skeleton_hydrate, FlatGraphPack};
use super::load::load_graph_pack_clips;
use super::manifest::{manifest_path, NaviManifest};

/// Soft peak RSS (VmRSS) after a single densify-only tile load. Above this the
/// tile is skipped with a warn; other tiles still build. Densify-only keep the
/// typical peak far below 4 GB; this is a safety rail for pathological packs.
pub const SKELETON_BUILD_SOFT_RSS_MB: u64 = 1800;

/// Schema / algorithm revision in the `.meta` fingerprint. Bump to invalidate
/// all on-disk skeletons (border rule, densify filter, ferry merge, …).
/// Bumped when border detection / rim-secondary selection changes so installed
/// skeletons rebuild (FU23: jamtland↔dalarna had a 25 km major-only gap).
/// FU24: bump when rim secondary/tertiary/unclassified/residential selection changes.
pub const CORRIDOR_SKELETON_BUILD: u32 = 4;

fn profile_slug(profile: RoutingProfile) -> &'static str {
    match profile {
        RoutingProfile::Car => "car",
        RoutingProfile::Bicycle => "bicycle",
        RoutingProfile::Foot => "foot",
        RoutingProfile::Truck => "truck",
    }
}

fn region_label(stem: &str) -> String {
    stem.trim_end_matches("-latest")
        .trim_end_matches("-latest.osm")
        .replace(['_', '-'], " ")
}

fn leaf_to_region(stem: &str) -> String {
    crate::routing::basemap::pbf_stem_to_geofabrik_path(stem)
        .unwrap_or_else(|| format!("europe/{}", stem.trim_end_matches("-latest")))
}

fn skeleton_meta_path(home: &Path, stem: &str) -> PathBuf {
    home.join(format!("{stem}.navi-corridor-skeleton.meta"))
}

fn peak_rss_mb() -> u64 {
    proc_status_kb("VmHWM:") / 1024
}

fn current_rss_mb() -> u64 {
    proc_status_kb("VmRSS:") / 1024
}

fn proc_status_kb(key: &str) -> u64 {
    let Ok(s) = fs::read_to_string("/proc/self/status") else {
        return 0;
    };
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix(key) {
            return rest
                .split_whitespace()
                .next()
                .and_then(|x| x.parse().ok())
                .unwrap_or(0);
        }
    }
    0
}

fn file_mtime_secs(path: &Path) -> u64 {
    fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn file_len(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn pack_generation(home: &Path, stem: &str) -> String {
    let install = home.join(format!("{stem}.navi-server-install.json"));
    if !install.is_file() {
        return String::new();
    }
    let Ok(text) = fs::read_to_string(&install) else {
        return String::new();
    };
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| {
            v.get("generation")
                .and_then(|g| g.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_default()
}

fn pack_fingerprint_parts(home: &Path, stem: &str, profile: RoutingProfile) -> (u64, u64, String) {
    let man = NaviManifest::load(&manifest_path(home, stem)).ok();
    let (len, mtime) = if let Some(ref m) = man {
        (m.pbf_size_bytes, m.pbf_modified_unix_secs)
    } else {
        let pbf = home.join(format!("{stem}.osm.pbf"));
        (file_len(&pbf), file_mtime_secs(&pbf))
    };
    let mut gen = pack_generation(home, stem);
    if gen.is_empty() {
        // Fall back to primary graph archive mtime/len so local-bake packs still
        // invalidate when tiles are rewritten.
        if let Some(path) = primary_graph_path(home, stem, profile) {
            gen = format!("graph:{}:{}", file_len(&path), file_mtime_secs(&path));
        }
    }
    (len, mtime, gen)
}

fn primary_graph_path(home: &Path, stem: &str, profile: RoutingProfile) -> Option<PathBuf> {
    let man = NaviManifest::load(&manifest_path(home, stem)).ok()?;
    if let Some(tiles) = man.graph_tiles_for(profile) {
        if let Some(first) = tiles.first() {
            return Some(home.join(&first.file));
        }
    }
    let key = profile_slug(profile);
    man.graph_files.get(key).map(|rel| home.join(rel))
}

/// Neighbor skeleton fingerprints: other stems in `home` that already have a
/// skeleton JSON. When a neighbor appears or its mtime changes, this stem's
/// meta no longer matches and rebuilds (border OSM ids need refresh).
fn neighbor_fingerprint(home: &Path, stem: &str) -> String {
    let mut parts: Vec<(String, u64)> = Vec::new();
    let Ok(rd) = fs::read_dir(home) else {
        return String::new();
    };
    for ent in rd.flatten() {
        let name = ent.file_name();
        let name = name.to_string_lossy();
        let Some(other) = name.strip_suffix(".navi-corridor-skeleton.json") else {
            continue;
        };
        if other == stem {
            continue;
        }
        parts.push((other.to_string(), file_mtime_secs(&ent.path())));
    }
    parts.sort_by(|a, b| a.0.cmp(&b.0));
    parts
        .into_iter()
        .map(|(s, m)| format!("{s}:{m}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn want_fingerprint(home: &Path, stem: &str, profile: RoutingProfile) -> String {
    let (len, mtime, gen) = pack_fingerprint_parts(home, stem, profile);
    let neighbors = neighbor_fingerprint(home, stem);
    format!(
        "build={CORRIDOR_SKELETON_BUILD};skel_fmt={CORRIDOR_SKELETON_FORMAT_VERSION};\
         profile={};gen={gen};len={len};mtime={mtime};neighbors={neighbors}",
        profile_slug(profile)
    )
}

/// True when skeleton JSON + meta match the current pack / neighbor fingerprint.
pub fn skeleton_fresh(home: &Path, stem: &str, profile: RoutingProfile) -> bool {
    let json = skeleton_path(home, stem);
    let meta_path = skeleton_meta_path(home, stem);
    if !json.is_file() || !meta_path.is_file() {
        return false;
    }
    let want = want_fingerprint(home, stem, profile);
    let got = fs::read_to_string(&meta_path).unwrap_or_default();
    got.trim() == want
}

/// Whether a plan may use this stem's corridor skeleton (fresh JSON on disk).
pub fn corridor_skeleton_ready(home: &Path, stem: &str, profile: RoutingProfile) -> bool {
    skeleton_fresh(home, stem, profile) && skeleton_path(home, stem).is_file()
}

fn tile_paths(home: &Path, stem: &str, profile: RoutingProfile) -> Vec<PathBuf> {
    let Ok(man) = NaviManifest::load(&manifest_path(home, stem)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(tiles) = man.graph_tiles_for(profile) {
        for rel in tiles {
            out.push(home.join(&rel.file));
        }
    }
    if out.is_empty() {
        let key = profile_slug(profile);
        if let Some(rel) = man.graph_files.get(key) {
            out.push(home.join(rel));
        }
    }
    out
}

/// Load one pack tile for corridor-skeleton build (major + secondary), convert
/// to flat, drop RouteGraph. Returns `None` when load fails or soft RSS is
/// exceeded (tile skipped).
fn load_flat_tile_bounded(path: &Path, profile: RoutingProfile, stem: &str) -> Option<FlatGraphPack> {
    let g = match with_corridor_skeleton_hydrate(|| load_graph_pack_clips(path, profile, None)) {
        Ok(g) => g,
        Err(e) => {
            log::warn!(
                target: "NaviPlan",
                "corridor_skeleton skip tile {}: load failed: {e:#}",
                path.display()
            );
            return None;
        }
    };
    let rss = current_rss_mb();
    let hwm = peak_rss_mb();
    log::info!(
        target: "NaviPlan",
        "corridor_skeleton tile stem={stem} path={} nodes={} edges={} VmRSS_mb={rss} VmHWM_mb={hwm}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
        g.nodes.len(),
        g.edges.len(),
    );
    if rss > SKELETON_BUILD_SOFT_RSS_MB {
        log::warn!(
            target: "NaviPlan",
            "corridor_skeleton abort tile stem={stem} path={}: VmRSS_mb={rss} > \
             SKELETON_BUILD_SOFT_RSS_MB={SKELETON_BUILD_SOFT_RSS_MB} (continue other tiles)",
            path.display()
        );
        drop(g);
        return None;
    }
    let flat = FlatGraphPack::from_route_graph(&g, None);
    drop(g);
    Some(flat)
}

fn collect_major_ids(home: &Path, stem: &str, profile: RoutingProfile) -> HashSet<i64> {
    let mut ids = HashSet::new();
    for p in tile_paths(home, stem, profile) {
        let Some(flat) = load_flat_tile_bounded(&p, profile, stem) else {
            continue;
        };
        ids.extend(major_node_osm_ids(&flat));
        drop(flat);
    }
    ids
}

/// Border OSM ids = intersection of this stem's major ids with major (or
/// skeleton) ids from bbox-adjacent installed neighbours.
///
/// Using neighbour **skeletons** alone was a chicken-and-egg: shared primary
/// nodes only appear as borders after both sides already kept them, so pairs
/// like jamtland↔dalarna stayed disconnected (~25 km gap) and Stage B detoured
/// thousands of km through unrelated packs. Adjacent-pack major ids close that.
fn border_osm_from_neighbor_packs(
    home: &Path,
    stem: &str,
    profile: RoutingProfile,
    major_ids: &HashSet<i64>,
) -> HashSet<i64> {
    let my_region = leaf_to_region(stem);
    let my_bbox = crate::routing::basemap::region_bbox(&my_region);
    let mut neighbor_nodes = HashSet::new();
    let mut seen = HashSet::new();
    let Ok(rd) = fs::read_dir(home) else {
        return HashSet::new();
    };
    for ent in rd.flatten() {
        let name = ent.file_name();
        let name = name.to_string_lossy();
        let Some(other) = name.strip_suffix(".navi-manifest.json") else {
            continue;
        };
        if other == stem || !seen.insert(other.to_string()) {
            continue;
        }
        let other_region = leaf_to_region(other);
        let adjacent = match (
            my_bbox,
            crate::routing::basemap::region_bbox(&other_region),
        ) {
            (Some(a), Some(b)) => crate::long_trip::regions_bbox_adjacent(&a, &b, 0.15),
            _ => true,
        };
        if adjacent {
            neighbor_nodes.extend(collect_major_ids(home, other, profile));
        } else {
            let skel_path = skeleton_path(home, other);
            if let Ok(skel) = read_skeleton_file(&skel_path) {
                neighbor_nodes.extend(skel.node_ids.iter().copied());
            }
        }
    }
    major_ids.intersection(&neighbor_nodes).copied().collect()
}

fn build_stem_skeleton(
    home: &Path,
    stem: &str,
    profile: RoutingProfile,
    border_osm: &HashSet<i64>,
) -> Option<CorridorSkeletonFile> {
    let region = leaf_to_region(stem);
    let prof = profile_slug(profile);
    let mut parts = Vec::new();
    for p in tile_paths(home, stem, profile) {
        let Some(flat) = load_flat_tile_bounded(&p, profile, stem) else {
            continue;
        };
        let part = build_skeleton_from_pack(&flat, &region, stem, prof, border_osm);
        drop(flat);
        parts.push(part);
    }
    // Ferry overlay sidecar (MAGIC_FERRY) — pier/approach stubs packs omit.
    let side = ferry_sidecar_path(home, stem, profile);
    if side.is_file() {
        match load_ferry_overlay_as_flat(home, stem, profile) {
            Some(flat) => {
                let ferry_n = flat.edge_is_ferry.iter().filter(|&&v| v != 0).count();
                log::info!(
                    target: "NaviPlan",
                    "corridor_skeleton overlay stem={stem} nodes={} edges={} ferry_edges≈{ferry_n}",
                    flat.node_ids.len(),
                    flat.edge_src.len(),
                );
                let part = build_skeleton_from_pack(&flat, &region, stem, prof, border_osm);
                drop(flat);
                parts.push(part);
            }
            None => {
                log::warn!(
                    target: "NaviPlan",
                    "corridor_skeleton skip overlay {}: load failed",
                    side.display()
                );
            }
        }
    }
    merge_skeleton_files(parts)
}

/// Snapshot of an in-flight / last corridor-skeleton build for UI status lines.
#[derive(Debug, Clone)]
pub struct CorridorSkeletonProgress {
    pub stem: String,
    pub region_label: String,
    pub running: bool,
    pub pct: u8,
    pub message: String,
}

struct BuildState {
    pct: AtomicU8,
    running: AtomicBool,
    message: Mutex<String>,
}

fn build_state() -> &'static BuildState {
    static STATE: OnceLock<BuildState> = OnceLock::new();
    STATE.get_or_init(|| BuildState {
        pct: AtomicU8::new(0),
        running: AtomicBool::new(false),
        message: Mutex::new(String::new()),
    })
}

static BUILD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn build_lock() -> &'static Mutex<()> {
    BUILD_LOCK.get_or_init(|| Mutex::new(()))
}

static ACTIVE_STEM: OnceLock<Mutex<String>> = OnceLock::new();

fn set_active_stem(stem: &str) {
    let slot = ACTIVE_STEM.get_or_init(|| Mutex::new(String::new()));
    if let Ok(mut g) = slot.lock() {
        *g = stem.to_string();
    }
}

fn set_progress(stem: &str, pct: u8, message: &str) {
    let st = build_state();
    st.pct.store(pct.min(100), Ordering::Relaxed);
    if let Ok(mut m) = st.message.lock() {
        *m = message.to_string();
    }
    let label = region_label(stem);
    crate::download::progress::set(
        pct as u64,
        Some(100),
        &format!("Preparing corridor skeleton for {label}…"),
    );
    let _ = stem;
}

/// Current corridor-skeleton build progress (empty stem when idle).
pub fn corridor_skeleton_progress() -> CorridorSkeletonProgress {
    let st = build_state();
    let running = st.running.load(Ordering::Relaxed);
    let pct = st.pct.load(Ordering::Relaxed);
    let message = st.message.lock().map(|m| m.clone()).unwrap_or_default();
    let stem = ACTIVE_STEM
        .get()
        .and_then(|m| m.lock().ok().map(|g| g.clone()))
        .unwrap_or_default();
    let region = if stem.is_empty() {
        String::new()
    } else {
        region_label(&stem)
    };
    let message = if message.is_empty() && running {
        format!("Preparing corridor skeleton for {region}…")
    } else {
        message
    };
    CorridorSkeletonProgress {
        stem,
        region_label: region,
        running,
        pct,
        message,
    }
}

/// Human-readable status when a plan is blocked on corridor skeleton build.
pub fn corridor_skeleton_preparing_status(stem: &str) -> (String, u8) {
    let label = region_label(stem);
    let prog = corridor_skeleton_progress();
    let pct = if prog.running && (prog.stem == stem || prog.stem.is_empty()) {
        prog.pct
    } else {
        0
    };
    (format!("preparing corridor skeleton for {label}"), pct)
}

/// Build or refresh `{stem}.navi-corridor-skeleton.json` from installed packs.
/// Safe to call from a background job at pack install / idle refresh.
pub fn ensure_corridor_skeleton(
    home: &Path,
    stem: &str,
    profile: RoutingProfile,
) -> anyhow::Result<()> {
    if skeleton_fresh(home, stem, profile) {
        return Ok(());
    }
    let _guard = build_lock().lock().unwrap_or_else(|e| e.into_inner());
    if skeleton_fresh(home, stem, profile) {
        return Ok(());
    }
    let st = build_state();
    st.running.store(true, Ordering::Relaxed);
    set_active_stem(stem);
    let label = region_label(stem);
    set_progress(stem, 0, &format!("Preparing corridor skeleton for {label}…"));
    let t0 = Instant::now();
    let hwm_before = peak_rss_mb();

    set_progress(stem, 15, &format!("Preparing corridor skeleton for {label}…"));
    let major = collect_major_ids(home, stem, profile);
    set_progress(stem, 45, &format!("Preparing corridor skeleton for {label}…"));
    let border = border_osm_from_neighbor_packs(home, stem, profile, &major);
    log::info!(
        target: "NaviPlan",
        "corridor_skeleton major_ids={} border_ids={} stem={stem} VmHWM_mb={}",
        major.len(),
        border.len(),
        peak_rss_mb()
    );
    drop(major);

    set_progress(stem, 55, &format!("Preparing corridor skeleton for {label}…"));
    let Some(skel) = build_stem_skeleton(home, stem, profile, &border) else {
        st.running.store(false, Ordering::Relaxed);
        set_progress(stem, 0, &format!("Corridor skeleton failed for {label}"));
        anyhow::bail!("corridor skeleton build produced no output stem={stem}");
    };
    set_progress(stem, 90, &format!("Preparing corridor skeleton for {label}…"));
    let file_bytes = write_skeleton_file(home, &skel).map_err(|e| {
        st.running.store(false, Ordering::Relaxed);
        anyhow::anyhow!("write skeleton stem={stem}: {e}")
    })?;
    let fp = want_fingerprint(home, stem, profile);
    let _ = fs::write(skeleton_meta_path(home, stem), fp);
    let hwm = peak_rss_mb();
    set_progress(stem, 100, &format!("Corridor skeleton ready for {label}"));
    st.running.store(false, Ordering::Relaxed);
    st.pct.store(100, Ordering::Relaxed);
    crate::routing::plan_perf::note(
        "corridor_skeleton",
        format!(
            "bg_build;stem={stem};nodes={};edges={};border={};file_bytes={file_bytes};\
             build_ms={};wall_ms={};VmHWM_mb={hwm};delta_hwm_mb={}",
            skel.node_count,
            skel.edge_count,
            skel.border_node_count,
            skel.build_ms,
            t0.elapsed().as_millis(),
            hwm.saturating_sub(hwm_before),
        ),
    );
    log::info!(
        target: "NaviPlan",
        "corridor_skeleton ready stem={stem} nodes={} edges={} border={} \
         file_bytes={file_bytes} VmHWM_mb={hwm}",
        skel.node_count,
        skel.edge_count,
        skel.border_node_count,
    );
    Ok(())
}

/// Stems among `pack_dirs` that are on the trip corridor but lack a fresh skeleton.
pub fn stems_missing_corridor_skeleton(
    pack_dirs: &[&Path],
    route_points: &[(f64, f64)],
    profile: RoutingProfile,
) -> Vec<(PathBuf, String)> {
    if route_points.len() < 2 {
        return Vec::new();
    }
    let needed = match crate::long_trip::ordered_needed_regions_for_trip(route_points, &[], None) {
        Ok(v) => v,
        Err(_) => {
            let mut v = Vec::new();
            for &(lat, lon) in route_points {
                if let Some(id) = crate::long_trip::region_containing(lat, lon, None) {
                    v.push(id.to_string());
                }
            }
            v
        }
    };
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for region_id in needed {
        let stem = crate::pack_server::leaf_stem_for_region_id(&region_id);
        if !seen.insert(stem.clone()) {
            continue;
        }
        let Some(home) = pack_dirs.iter().find(|d| {
            manifest_path(d, &stem).is_file() || skeleton_path(d, &stem).is_file()
        }) else {
            continue;
        };
        if !corridor_skeleton_ready(home, &stem, profile) {
            out.push(((*home).to_path_buf(), stem));
        }
    }
    out
}
