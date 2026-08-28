//! Cooperative cancellation for in-flight UniFFI plans.
//!
//! Kotlin cannot interrupt blocking JNI. The host calls [`request_cancel`], and
//! the plan thread plus Rayon blob workers poll [`is_cancelled_id`] at existing
//! PBF / stage / A* boundaries. Each plan has a unique id; cancel targets the
//! plan that last called [`begin_plan`] (the in-flight UniFFI call).

use std::cell::Cell;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

thread_local! {
    static PLAN_ID: Cell<u64> = const { Cell::new(0) };
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static CURRENT_PLAN: AtomicU64 = AtomicU64::new(0);
static CANCELLED: LazyLock<Mutex<HashSet<u64>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// Error used as an early-exit (same shape as other `anyhow` failures).
#[derive(Debug, Clone, Copy)]
pub struct PlanCancelled;

impl std::fmt::Display for PlanCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cancelled")
    }
}

impl std::error::Error for PlanCancelled {}

/// Clears thread-local plan id when the UniFFI call returns (or panics).
pub struct PlanCancelGuard {
    id: u64,
}

impl PlanCancelGuard {
    pub fn id(&self) -> u64 {
        self.id
    }
}

impl Drop for PlanCancelGuard {
    fn drop(&mut self) {
        PLAN_ID.with(|c| {
            if c.get() == self.id {
                c.set(0);
            }
        });
        let _ = CURRENT_PLAN.compare_exchange(self.id, 0, Ordering::SeqCst, Ordering::SeqCst);
    }
}

/// Start a new plan generation on this thread. Pair with the returned guard.
pub fn begin_plan() -> PlanCancelGuard {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    PLAN_ID.with(|c| c.set(id));
    CURRENT_PLAN.store(id, Ordering::SeqCst);
    PlanCancelGuard { id }
}

/// Host (Cancel tap) asks the in-flight plan to stop.
///
/// Called from a JNI thread that is **not** the plan worker, so this uses
/// [`CURRENT_PLAN`] rather than thread-local state.
pub fn request_cancel() {
    let tls = current_plan_id();
    let id = if tls != 0 {
        tls
    } else {
        CURRENT_PLAN.load(Ordering::SeqCst)
    };
    request_cancel_id(id);
}

/// Cancel a specific plan id (tests and Rayon workers that captured the id).
pub fn request_cancel_id(id: u64) {
    if id == 0 {
        return;
    }
    CANCELLED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id);
}

#[inline]
pub fn is_cancelled_id(id: u64) -> bool {
    if id == 0 {
        return false;
    }
    CANCELLED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&id)
}

/// This thread's plan id, or `0` on workers that never called [`begin_plan`].
#[inline]
pub fn current_plan_id() -> u64 {
    PLAN_ID.with(|c| c.get())
}

/// True when this thread is a plan that has been cancelled.
///
/// Rayon blob workers should pass the **captured** id from the plan thread into
/// [`is_cancelled_id`] instead of calling this (thread-local is not inherited).
#[inline]
pub fn is_cancelled() -> bool {
    is_cancelled_id(current_plan_id())
}

pub fn cancelled_err() -> anyhow::Error {
    PlanCancelled.into()
}

pub fn abort_if_cancelled() -> anyhow::Result<()> {
    if is_cancelled() {
        Err(cancelled_err())
    } else {
        Ok(())
    }
}

pub fn abort_if_cancelled_id(id: u64) -> anyhow::Result<()> {
    if is_cancelled_id(id) {
        Err(cancelled_err())
    } else {
        Ok(())
    }
}

pub fn is_cancel_err(e: &anyhow::Error) -> bool {
    e.chain().any(|c| c.is::<PlanCancelled>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_stops_matching_id_not_later_plan() {
        let a = begin_plan();
        let a_id = a.id();
        assert!(!is_cancelled());
        request_cancel();
        assert!(is_cancelled_id(a_id));
        drop(a);
        let b = begin_plan();
        assert!(!is_cancelled_id(b.id()));
        assert!(is_cancelled_id(a_id));
        drop(b);
    }
}
