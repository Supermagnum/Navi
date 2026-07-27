//! Documented thread-priority tiers for Navi (host + core workers).
//!
//! | Tier | Role | Priority |
//! |---|---|---|
//! | T0 Sensor | GPS / IMU | Highest |
//! | T1 ECU | Live energy (future) | High |
//! | T2 UI / audio peer | Compose UI, media smoothness | High |
//! | T3 Routing | Graph build, eco-reweight, A* | Medium (below audio) |
//! | T4 DB | SQLite persistence | Lowest |
//!
//! Routing workers must leave headroom for T0–T2 and must not starve audio.

use std::num::NonZeroUsize;
use std::thread::available_parallelism;

/// Detected parallelism and the worker count to use for routing-tier Rayon pools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerPoolPlan {
    pub detected_cores: usize,
    pub routing_workers: usize,
    pub reserved_for_ui_audio: usize,
}

impl WorkerPoolPlan {
    /// Autodetect cores and reserve headroom so routing never saturates every core.
    ///
    /// Reserve at least 1 core (or ~25% on larger machines) for sensor/UI/audio.
    pub fn detect() -> Self {
        let detected = available_parallelism()
            .map(NonZeroUsize::get)
            .unwrap_or(1)
            .max(1);
        let reserved = detected
            .div_ceil(4)
            .max(1)
            .min(detected.saturating_sub(1).max(1));
        let routing_workers = detected.saturating_sub(reserved).max(1);
        Self {
            detected_cores: detected,
            routing_workers,
            reserved_for_ui_audio: reserved.min(detected),
        }
    }

    /// Install as the global Rayon thread-pool size for routing-tier work.
    pub fn install_rayon_pool(&self) -> Result<(), rayon::ThreadPoolBuildError> {
        rayon::ThreadPoolBuilder::new()
            .num_threads(self.routing_workers)
            .thread_name(|i| format!("navi-routing-{i}"))
            .build_global()
    }

    /// Best-effort lower OS niceness for the current thread (routing tier).
    ///
    /// No-op / ignored on platforms without `libc` nice, or when lacking permission.
    pub fn lower_current_thread_priority() {
        #[cfg(unix)]
        {
            // SAFETY: nice() only affects the calling thread/process priority.
            unsafe {
                let _ = libc::nice(5);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_headroom() {
        let plan = WorkerPoolPlan::detect();
        assert!(plan.detected_cores >= 1);
        assert!(plan.routing_workers >= 1);
        assert!(plan.routing_workers <= plan.detected_cores);
        if plan.detected_cores > 1 {
            assert!(plan.routing_workers < plan.detected_cores);
        }
    }
}
