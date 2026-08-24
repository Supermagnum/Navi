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

use osmpbf::{Element, ElementReader};

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

/// Sequential PBF scan that yields to a foreground plan every few thousand
/// elements (background indexer threads only).
pub fn for_each_pbf_elements<F>(path: &Path, mut f: F) -> anyhow::Result<()>
where
    F: for<'a> FnMut(Element<'a>),
{
    let file = std::fs::File::open(path)?;
    let reader = ElementReader::new(file);
    let mut n = 0u32;
    reader.for_each(|el| {
        n = n.wrapping_add(1);
        if n % 4096 == 0 {
            yield_if_foreground_plan();
        }
        f(el);
    })?;
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
}
