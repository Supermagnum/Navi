//! Cooperative PBF access: background convert / place-index yield while a
//! foreground plan is on the pack-miss fallback path. Bbox graph builds
//! (plan fallback, speed-limit cone, road-near) also serialize here so a new
//! caller cannot scan the PBF in parallel with an active plan.

use std::cell::Cell;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use osmpbf::{BlobDecode, BlobReader, Element, PrimitiveBlock};
use rayon::prelude::*;

use super::progress::{current_channel, ProgressChannel};

static FOREGROUND_PLANS: AtomicU32 = AtomicU32::new(0);
static BBOX_BUILD: Mutex<()> = Mutex::new(());

/// Returned when a non-plan bbox build is skipped so the plan keeps the PBF.
pub const BBOX_BUILD_SKIPPED: &str = "pbf bbox skipped: foreground plan in progress";

thread_local! {
    static BACKGROUND_INDEXER: Cell<bool> = const { Cell::new(false) };
}

/// Held for the duration of a pack-miss plan (or the whole UI plan coroutine).
pub struct ForegroundPlanGuard;

impl ForegroundPlanGuard {
    pub fn acquire() -> Self {
        enter();
        Self
    }
}

impl Drop for ForegroundPlanGuard {
    fn drop(&mut self) {
        leave();
    }
}

pub fn enter() {
    FOREGROUND_PLANS.fetch_add(1, Ordering::SeqCst);
}

pub fn leave() {
    loop {
        let cur = FOREGROUND_PLANS.load(Ordering::SeqCst);
        if cur == 0 {
            return;
        }
        if FOREGROUND_PLANS
            .compare_exchange(cur, cur - 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return;
        }
    }
}

pub fn foreground_plan_active() -> bool {
    FOREGROUND_PLANS.load(Ordering::SeqCst) > 0
}

/// Skip GPS/cone/road-near bbox builds while a plan owns the PBF.
///
/// The plan thread ([`ProgressChannel::Plan`]) must not skip — it is the owner.
/// Everyone else skips rather than queueing: a missed cone update is cheaper
/// than stretching a user-initiated plan, and a queue of deferred cones would
/// stall the HUD after the plan returns.
pub fn skip_non_plan_bbox_build() -> bool {
    current_channel() != ProgressChannel::Plan && foreground_plan_active()
}

/// Exclusive lock for [`crate::routing::graph::load_or_build_reweighted_bbox`].
///
/// Plan waits out an in-flight cone; new non-plan callers should skip first
/// so they never take this lock during a plan.
pub fn lock_bbox_build() -> MutexGuard<'static, ()> {
    BBOX_BUILD.lock().unwrap_or_else(|e| e.into_inner())
}

/// Tests that mutate [`FOREGROUND_PLANS`] must take this so they do not flake
/// under `cargo test` default parallelism.
#[cfg(test)]
pub(crate) fn lock_plan_flag_for_test() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct BackgroundIndexerGuard {
    prev: bool,
}

impl BackgroundIndexerGuard {
    pub fn enter() -> Self {
        let prev = BACKGROUND_INDEXER.with(|c| {
            let p = c.get();
            c.set(true);
            p
        });
        Self { prev }
    }
}

impl Drop for BackgroundIndexerGuard {
    fn drop(&mut self) {
        BACKGROUND_INDEXER.with(|c| c.set(self.prev));
    }
}

pub fn with_background_indexer<R>(f: impl FnOnce() -> R) -> R {
    let _g = BackgroundIndexerGuard::enter();
    f()
}

/// Sleep while a foreground plan owns the PBF. No-op on the plan thread.
pub fn yield_if_foreground_plan() {
    if !BACKGROUND_INDEXER.with(|c| c.get()) {
        return;
    }
    while foreground_plan_active() {
        thread::sleep(Duration::from_millis(20));
    }
}

/// Whether the calling thread is a background indexer (for capturing before Rayon work).
pub fn background_indexer_active() -> bool {
    BACKGROUND_INDEXER.with(|c| c.get())
}

pub(crate) fn yield_if_background_indexer(yield_to_plan: bool) {
    if yield_to_plan {
        while foreground_plan_active() {
            thread::sleep(Duration::from_millis(20));
        }
    }
}

/// Parallel blob decode with **per-blob** callbacks (no global element mutex).
///
/// Cooperative yield runs once per blob on the worker, same as
/// [`for_each_pbf_elements`]. Use this when the caller can process an entire
/// decoded [`PrimitiveBlock`] on the Rayon worker without cross-thread mutation
/// (e.g. thread-local spill buffers).
pub fn for_each_pbf_data_block<F>(path: &Path, f: F) -> anyhow::Result<()>
where
    F: Fn(&PrimitiveBlock) -> anyhow::Result<()> + Send + Sync,
{
    let plan_id = super::plan_cancel::current_plan_id();
    let yield_to_plan = BACKGROUND_INDEXER.with(|c| c.get());
    let blobs = BlobReader::from_path(path)?;
    blobs
        .par_bridge()
        .try_for_each(|blob| -> anyhow::Result<()> {
            super::plan_cancel::abort_if_cancelled_id(plan_id)?;
            if yield_to_plan {
                while foreground_plan_active() {
                    thread::sleep(Duration::from_millis(20));
                }
            }
            match blob?.decode() {
                Ok(BlobDecode::OsmHeader(_)) | Ok(BlobDecode::Unknown(_)) => Ok(()),
                Ok(BlobDecode::OsmData(block)) => {
                    f(&block)?;
                    super::plan_cancel::abort_if_cancelled_id(plan_id)
                }
                Err(e) => Err(e.into()),
            }
        })?;
    Ok(())
}

/// Lat/lon samples collected from a **content-hash thinned** subset of PBF
/// nodes. Membership is a pure function of `(lat, lon)` bit patterns — not
/// arrival order — so parallel blob folds stay deterministic across Rayon
/// widths. Fixed keep rate (~1/64) yields ~200k samples on Hedmark-scale
/// extracts and ~1M on Ostlandet without a counting pre-pass.
struct LatLonSamples {
    lats: Vec<f64>,
    lons: Vec<f64>,
}

/// Keep roughly one in [`BBOX_SAMPLE_PERIOD`] nodes via content hash.
/// Fixed constants (no live N): Hedmark ~13.8M → ~216k kept; Ostlandet
/// ~68.2M → ~1.07M kept — both ample for 0.5/99.5 order statistics.
const BBOX_SAMPLE_PERIOD: u64 = 64;
const BBOX_SAMPLE_KEEP: u64 = 1;

/// Order-independent keep predicate for percentile bbox sampling.
#[inline]
fn keep_bbox_sample(lat: f64, lon: f64) -> bool {
    // SplitMix64-style mix of IEEE bits — deterministic, arrival-order free.
    // Duplicate coordinates share bits and are kept/dropped together (fine for
    // percentiles: identical values do not move order statistics).
    let mut x = lat.to_bits().wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= lon.to_bits().wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    x ^= x >> 32;
    x = x.wrapping_mul(0x1656_67B1_9E37_79F9);
    x ^= x >> 32;
    (x % BBOX_SAMPLE_PERIOD) < BBOX_SAMPLE_KEEP
}

impl LatLonSamples {
    fn empty() -> Self {
        Self {
            lats: Vec::new(),
            lons: Vec::new(),
        }
    }

    fn is_empty(&self) -> bool {
        self.lats.is_empty()
    }

    fn absorb(&mut self, lat: f64, lon: f64) {
        if !lat.is_finite() || !lon.is_finite() {
            return;
        }
        if !keep_bbox_sample(lat, lon) {
            return;
        }
        self.lats.push(lat);
        self.lons.push(lon);
    }

    fn merge(mut self, mut other: Self) -> Self {
        self.lats.append(&mut other.lats);
        self.lons.append(&mut other.lons);
        self
    }

    /// `[min_lat, min_lon, max_lat, max_lon]` at the given sorted percentiles.
    fn percentile_bbox(&mut self, low: f64, high: f64) -> [f64; 4] {
        self.lats
            .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        self.lons
            .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        [
            percentile_sorted(&self.lats, low),
            percentile_sorted(&self.lons, low),
            percentile_sorted(&self.lats, high),
            percentile_sorted(&self.lons, high),
        ]
    }
}

fn percentile_sorted(v: &[f64], p: f64) -> f64 {
    let idx = ((v.len() as f64 - 1.0) * p).round() as usize;
    v[idx.min(v.len().saturating_sub(1))]
}

fn scan_blob_latlon_samples(
    acc: &mut LatLonSamples,
    blob: Result<osmpbf::Blob, osmpbf::Error>,
    yield_to_plan: bool,
) -> anyhow::Result<()> {
    super::plan_cancel::abort_if_cancelled()?;
    if yield_to_plan {
        while foreground_plan_active() {
            thread::sleep(Duration::from_millis(20));
        }
    }
    match blob?.decode() {
        Ok(BlobDecode::OsmHeader(_)) | Ok(BlobDecode::Unknown(_)) => Ok(()),
        Ok(BlobDecode::OsmData(block)) => {
            block.for_each_element(|element| {
                let (lat, lon) = match element {
                    Element::Node(n) => (n.lat(), n.lon()),
                    Element::DenseNode(n) => (n.lat(), n.lon()),
                    _ => return,
                };
                acc.absorb(lat, lon);
            });
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

/// Order-independent node bounding box with percentile trim.
///
/// Parallel blob fold/reduce keeps a **content-hash thinned** subset of
/// coordinates (~1/{[`BBOX_SAMPLE_PERIOD`]}), merges worker samples, sorts
/// once, and takes exact percentiles. Same 0.5/99.5 semantics as a full
/// collect for region tiling, without materializing tens of millions of
/// floats. Membership does not depend on arrival order (unlike reservoir
/// sampling). Cooperative yield matches [`for_each_pbf_elements`].
pub fn pbf_latlon_percentile_bounds(path: &Path, low: f64, high: f64) -> anyhow::Result<[f64; 4]> {
    let yield_to_plan = BACKGROUND_INDEXER.with(|c| c.get());
    let mut samples = BlobReader::from_path(path)?
        .par_bridge()
        .try_fold(
            LatLonSamples::empty,
            |mut acc, blob| -> anyhow::Result<LatLonSamples> {
                scan_blob_latlon_samples(&mut acc, blob, yield_to_plan)?;
                Ok(acc)
            },
        )
        .try_reduce(LatLonSamples::empty, |a, b| Ok(a.merge(b)))?;
    if samples.is_empty() {
        anyhow::bail!("PBF has no nodes: {}", path.display());
    }
    log::info!(
        target: "NaviConvert",
        "CONVERT_PHASE bbox samples kept={} (period={BBOX_SAMPLE_PERIOD})",
        samples.lats.len()
    );
    Ok(samples.percentile_bbox(low, high))
}

/// Parallel blob-decode PBF scan.
///
/// Blob zlib/protobuf decode runs on Rayon workers (`par_bridge`, same as
/// `ElementReader::par_map_reduce`). The visitor is serialized through a mutex
/// so callers can keep `FnMut` accumulators. Cooperative yield runs **once per
/// blob** on the worker, using the caller's background-indexer flag (Rayon
/// threads do not inherit the thread-local).
pub fn for_each_pbf_elements<F>(path: &Path, f: F) -> anyhow::Result<()>
where
    F: for<'a> FnMut(Element<'a>) + Send,
{
    let plan_id = super::plan_cancel::current_plan_id();
    let yield_to_plan = BACKGROUND_INDEXER.with(|c| c.get());
    let blobs = BlobReader::from_path(path)?;
    let f = Mutex::new(f);
    blobs
        .par_bridge()
        .try_for_each(|blob| -> anyhow::Result<()> {
            super::plan_cancel::abort_if_cancelled_id(plan_id)?;
            if yield_to_plan {
                while foreground_plan_active() {
                    thread::sleep(Duration::from_millis(20));
                }
            }
            match blob?.decode() {
                Ok(BlobDecode::OsmHeader(_)) | Ok(BlobDecode::Unknown(_)) => Ok(()),
                Ok(BlobDecode::OsmData(block)) => {
                    let mut guard = f.lock().unwrap_or_else(|e| e.into_inner());
                    block.for_each_element(&mut *guard);
                    drop(guard);
                    super::plan_cancel::abort_if_cancelled_id(plan_id)?;
                    Ok(())
                }
                Err(e) => Err(e.into()),
            }
        })?;
    Ok(())
}

/// Sequential blob walk (file order). Use when a later element depends on
/// earlier ones in the same scan (e.g. way centroids after nodes).
pub fn for_each_pbf_elements_serial<F>(path: &Path, mut f: F) -> anyhow::Result<()>
where
    F: for<'a> FnMut(Element<'a>),
{
    let plan_id = super::plan_cancel::current_plan_id();
    let blobs = BlobReader::from_path(path)?;
    for blob in blobs {
        super::plan_cancel::abort_if_cancelled_id(plan_id)?;
        match blob?.decode() {
            Ok(BlobDecode::OsmHeader(_)) | Ok(BlobDecode::Unknown(_)) => {}
            Ok(BlobDecode::OsmData(block)) => {
                block.for_each_element(&mut f);
                super::plan_cancel::abort_if_cancelled_id(plan_id)?;
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn background_indexer_waits_for_foreground_plan() {
        let _serial = lock_plan_flag_for_test();
        let _fg = ForegroundPlanGuard::acquire();
        let started = Instant::now();
        let h = thread::spawn(|| {
            with_background_indexer(|| {
                yield_if_foreground_plan();
            });
        });
        thread::sleep(Duration::from_millis(60));
        drop(_fg);
        h.join().unwrap();
        assert!(started.elapsed() >= Duration::from_millis(50));
    }

    #[test]
    fn plan_thread_does_not_self_deadlock() {
        let _serial = lock_plan_flag_for_test();
        let _fg = ForegroundPlanGuard::acquire();
        let t0 = Instant::now();
        yield_if_foreground_plan();
        assert!(t0.elapsed() < Duration::from_millis(30));
    }

    #[test]
    fn skip_bbox_when_plan_active_on_other_channels() {
        let _serial = lock_plan_flag_for_test();
        assert!(!skip_non_plan_bbox_build());
        let _fg = ForegroundPlanGuard::acquire();
        assert!(skip_non_plan_bbox_build());
        crate::download::progress::with_channel(ProgressChannel::Plan, || {
            assert!(!skip_non_plan_bbox_build());
        });
        crate::download::progress::with_channel(ProgressChannel::Cone, || {
            assert!(skip_non_plan_bbox_build());
        });
        crate::download::progress::with_channel(ProgressChannel::Download, || {
            assert!(skip_non_plan_bbox_build());
        });
        drop(_fg);
        assert!(!skip_non_plan_bbox_build());
    }

    #[test]
    fn bbox_lock_serializes_callers() {
        use std::sync::atomic::AtomicBool;
        static INSIDE: AtomicBool = AtomicBool::new(false);
        let h = thread::spawn(|| {
            let _g = lock_bbox_build();
            INSIDE.store(true, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(80));
            INSIDE.store(false, Ordering::SeqCst);
        });
        thread::sleep(Duration::from_millis(20));
        let _g = lock_bbox_build();
        assert!(!INSIDE.load(Ordering::SeqCst));
        h.join().unwrap();
    }

    #[test]
    fn bbox_content_hash_is_deterministic_and_near_period() {
        // Same (lat, lon) always keeps or always drops.
        assert_eq!(
            keep_bbox_sample(60.123_456, 11.987_654),
            keep_bbox_sample(60.123_456, 11.987_654)
        );
        // Duplicate coordinates share the keep decision (stacked OSM nodes).
        let keep = keep_bbox_sample(59.9, 10.7);
        assert_eq!(keep, keep_bbox_sample(59.9, 10.7));

        // Grid of distinct points: keep rate ~1/PERIOD (loose bounds).
        let mut kept = 0usize;
        let n = 50_000usize;
        for i in 0..n {
            let lat = 59.0 + (i as f64) * 1e-5;
            let lon = 10.0 + ((i * 7) as f64) * 1e-5;
            if keep_bbox_sample(lat, lon) {
                kept += 1;
            }
        }
        let expected = n as f64 / BBOX_SAMPLE_PERIOD as f64;
        let ratio = kept as f64 / expected;
        assert!(
            (0.85..=1.15).contains(&ratio),
            "kept={kept} expected~{expected} ratio={ratio}"
        );
    }

    #[test]
    fn cancelled_plan_aborts_pbf_element_scan() {
        // Serial walk: do not share the global Rayon pool with Ostlandet bbox
        // tests (those can occupy every worker for >60s and flake a parallel
        // abort assertion).
        let path = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/motor-access-hamar-gjovik.osm.pbf"
        ));
        if !path.is_file() {
            return;
        }
        let blobs = std::sync::atomic::AtomicU32::new(0);
        let g = crate::download::plan_cancel::begin_plan();
        crate::download::plan_cancel::request_cancel_id(g.id());
        assert!(crate::download::plan_cancel::is_cancelled_id(g.id()));
        let t0 = Instant::now();
        let err = for_each_pbf_elements_serial(path, |_| {
            blobs.fetch_add(1, Ordering::Relaxed);
        })
        .expect_err("cancelled scan should not complete");
        let elapsed = t0.elapsed();
        assert!(crate::download::plan_cancel::is_cancel_err(&err), "{err:#}");
        assert!(
            elapsed < Duration::from_secs(2),
            "pre-scan cancel should stop immediately: {elapsed:?}"
        );
        assert_eq!(
            blobs.load(Ordering::Relaxed),
            0,
            "cancelled before first blob should not visit elements"
        );
    }

    #[test]
    fn cancel_during_blob_aborts_after_that_blob() {
        let path = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/motor-access-hamar-gjovik.osm.pbf"
        ));
        if !path.is_file() {
            return;
        }
        let seen = std::sync::atomic::AtomicU32::new(0);
        let g = crate::download::plan_cancel::begin_plan();
        let id = g.id();
        let t0 = Instant::now();
        let err = for_each_pbf_elements_serial(path, |_| {
            if seen.fetch_add(1, Ordering::Relaxed) == 0 {
                crate::download::plan_cancel::request_cancel_id(id);
            }
        })
        .expect_err("cancel mid-blob should fail the scan after the blob");
        let elapsed = t0.elapsed();
        assert!(crate::download::plan_cancel::is_cancel_err(&err), "{err:#}");
        assert!(
            elapsed < Duration::from_secs(2),
            "mid-blob cancel should not finish the file: {elapsed:?}"
        );
        assert!(
            seen.load(Ordering::Relaxed) > 0,
            "at least the blob in flight should have been visited"
        );
    }
}
