//! Stem-level ferry overlay sidecar: build once from PBF at pack install /
//! refresh (background), reuse at plan time.
//!
//! **No PBF parsing may happen inside a plan.** The sidecar is a v9
//! [`FlatGraphPack`] of ferry + pier-approach edges for the stem's region bbox,
//! invalidated when the source PBF size/mtime changes. If a plan needs the
//! overlay before the sidecar is ready, the load path returns
//! [`super::PackLoadError::FerryPreparing`].

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};
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

/// Overlay topology revision. Bump when sidecar contents change independently
/// of the source PBF (inland highway vacuum so clipped terminals merge).
const FERRY_SIDECAR_BUILD: u32 = 2;

fn pbf_fingerprint(pbf: &Path) -> Option<String> {
    let meta = fs::metadata(pbf).ok()?;
    let len = meta.len();
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Some(format!(
        "build={FERRY_SIDECAR_BUILD};len={len};mtime={mtime}"
    ))
}

/// True when the on-disk sidecar meta matches the PBF fingerprint.
/// A fresh meta without a `.rkyv` means the PBF had no ferry edges (no rebuild).
pub fn sidecar_fresh(home: &Path, stem: &str, profile: RoutingProfile, pbf: &Path) -> bool {
    let meta_path = ferry_sidecar_meta_path(home, stem, profile);
    if !meta_path.is_file() {
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

fn region_label(stem: &str) -> String {
    stem.trim_end_matches("-latest")
        .trim_end_matches("-latest.osm")
        .replace(['_', '-'], " ")
}

/// Snapshot of an in-flight / last ferry-sidecar build for UI status lines.
#[derive(Debug, Clone)]
pub struct FerrySidecarProgress {
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

fn set_progress(stem: &str, pct: u8, message: &str) {
    let st = build_state();
    // stem is set under BUILD_LOCK by ensure_*; progress fields are atomic/mutex.
    st.pct.store(pct.min(100), Ordering::Relaxed);
    if let Ok(mut m) = st.message.lock() {
        *m = message.to_string();
    }
    let label = region_label(stem);
    crate::download::progress::set(
        pct as u64,
        Some(100),
        &format!("Preparing ferry data for {label}…"),
    );
    let _ = stem;
}

/// Current ferry-sidecar build progress (empty stem when idle).
pub fn ferry_sidecar_progress() -> FerrySidecarProgress {
    let st = build_state();
    let running = st.running.load(Ordering::Relaxed);
    let pct = st.pct.load(Ordering::Relaxed);
    let message = st.message.lock().map(|m| m.clone()).unwrap_or_default();
    // Stem stored only while holding build lock start; read best-effort via message.
    let stem = {
        // Recover stem from last ensure call stored in a side slot.
        FERRY_ACTIVE_STEM
            .get()
            .and_then(|m| m.lock().ok().map(|g| g.clone()))
            .unwrap_or_default()
    };
    let region = if stem.is_empty() {
        String::new()
    } else {
        region_label(&stem)
    };
    let message = if message.is_empty() && running {
        format!("Preparing ferry data for {region}…")
    } else {
        message
    };
    FerrySidecarProgress {
        stem,
        region_label: region,
        running,
        pct,
        message,
    }
}

static FERRY_ACTIVE_STEM: OnceLock<Mutex<String>> = OnceLock::new();

fn set_active_stem(stem: &str) {
    let slot = FERRY_ACTIVE_STEM.get_or_init(|| Mutex::new(String::new()));
    if let Ok(mut g) = slot.lock() {
        *g = stem.to_string();
    }
}

/// Build or refresh the stem ferry sidecar from `pbf`. Safe to call from a
/// background job at pack install / refresh. Updates download progress.
///
/// Returns `Ok(true)` when a sidecar with ferry edges was written (or already
/// fresh), `Ok(false)` when the PBF had no ferry edges (no file written).
pub fn ensure_ferry_sidecar(
    home: &Path,
    stem: &str,
    profile: RoutingProfile,
    pbf: &Path,
) -> anyhow::Result<bool> {
    if sidecar_fresh(home, stem, profile, pbf) {
        return Ok(true);
    }
    let _guard = build_lock().lock().unwrap_or_else(|e| e.into_inner());
    // Re-check after lock (another worker may have finished).
    if sidecar_fresh(home, stem, profile, pbf) {
        return Ok(true);
    }
    let st = build_state();
    st.running.store(true, Ordering::Relaxed);
    set_active_stem(stem);
    let label = region_label(stem);
    set_progress(stem, 0, &format!("Preparing ferry data for {label}…"));

    let region = stem_region_bbox(stem);
    let pbf_bytes = fs::metadata(pbf).map(|m| m.len()).unwrap_or(0);
    crate::routing::plan_perf::note(
        "ferry_pbf",
        format!(
            "path={};bytes={pbf_bytes};region_build=1;bg=1",
            pbf.display()
        ),
    );
    set_progress(stem, 10, &format!("Preparing ferry data for {label}…"));
    let t_parse = Instant::now();
    let full = match RouteGraph::build_ferry_overlay_from_pbf(pbf, profile, region) {
        Ok(g) => g,
        Err(e) => {
            st.running.store(false, Ordering::Relaxed);
            set_progress(stem, 0, &format!("Ferry data failed for {label}"));
            return Err(e);
        }
    };
    let parse_ms = t_parse.elapsed().as_millis() as u64;
    crate::routing::plan_perf::note_u64("ferry_pbf_parse_ms", parse_ms);
    set_progress(stem, 90, &format!("Preparing ferry data for {label}…"));

    let has_ferry = full.edges.iter().any(|e| e.is_ferry);
    if has_ferry {
        write_sidecar(home, stem, profile, pbf, &full);
    } else if let Some(fp) = pbf_fingerprint(pbf) {
        // Mark fresh with no overlay file — avoid rebuild loops on ferry-free stems.
        let meta_path = ferry_sidecar_meta_path(home, stem, profile);
        let _ = fs::write(&meta_path, fp);
        let side = ferry_sidecar_path(home, stem, profile);
        let _ = fs::remove_file(&side);
        log::info!(
            target: "NaviPlan",
            "ferry_sidecar no ferry edges stem={stem}; meta only"
        );
    }
    set_progress(stem, 100, &format!("Ferry data ready for {label}"));
    st.running.store(false, Ordering::Relaxed);
    st.pct.store(100, Ordering::Relaxed);
    crate::routing::plan_perf::note(
        "ferry_sidecar",
        format!(
            "bg_build;stem={stem};pbf_bytes={pbf_bytes};edges={};ferry={has_ferry};ms={parse_ms}",
            full.edges.len()
        ),
    );
    Ok(has_ferry)
}

/// Whether a plan may load a ferry overlay for this stem (sidecar fresh).
/// Does **not** parse PBF.
pub fn ferry_sidecar_ready(home: &Path, stem: &str, profile: RoutingProfile, pbf: &Path) -> bool {
    sidecar_fresh(home, stem, profile, pbf) && ferry_sidecar_path(home, stem, profile).is_file()
}

/// Load ferry+approach overlay for `stem` clipped to `clips` from the sidecar only.
///
/// Returns `None` when the sidecar is missing/stale — callers must not parse PBF
/// on the plan path; use [`ensure_ferry_sidecar`] in the background and surface
/// [`super::PackLoadError::FerryPreparing`].
///
/// Mode is always `"sidecar"` on success.
pub fn ferry_overlay_for_plan(
    home: &Path,
    stem: &str,
    profile: RoutingProfile,
    pbf: &Path,
    clips: &[[f64; 4]],
) -> Option<(RouteGraph, &'static str)> {
    if clips.is_empty() {
        return None;
    }
    let t0 = Instant::now();
    if !sidecar_fresh(home, stem, profile, pbf) {
        crate::routing::plan_perf::note(
            "ferry_sidecar",
            format!("miss_stale_or_absent;stem={stem};plan_no_pbf=1"),
        );
        crate::routing::plan_perf::note_u64("ferry_pbf_parse_ms", 0);
        return None;
    }
    let side = ferry_sidecar_path(home, stem, profile);
    if !side.is_file() {
        // Fresh meta with empty=1 (no ferries in region).
        crate::routing::plan_perf::note("ferry_sidecar", format!("fresh_empty;stem={stem}"));
        return None;
    }
    let bytes = fs::metadata(&side).map(|m| m.len()).unwrap_or(0);
    let g = load_sidecar_clipped(&side, profile, Some(clips))?;
    crate::routing::plan_perf::note(
        "ferry_sidecar",
        format!(
            "hit;stem={stem};bytes={bytes};edges={};clips={};load_ms={}",
            g.edges.len(),
            clips.len(),
            t0.elapsed().as_millis()
        ),
    );
    crate::routing::plan_perf::note_u64("ferry_pbf_parse_ms", 0);
    crate::routing::plan_perf::note_u64("ferry_pbf_bytes", 0);
    Some((g, "sidecar"))
}

/// Human-readable status when a plan is blocked on ferry sidecar build.
pub fn ferry_preparing_status(stem: &str) -> (String, u8) {
    let label = region_label(stem);
    let prog = ferry_sidecar_progress();
    let pct = if prog.running && (prog.stem == stem || prog.stem.is_empty()) {
        prog.pct
    } else {
        0
    };
    (format!("preparing ferry data for {label}"), pct)
}
