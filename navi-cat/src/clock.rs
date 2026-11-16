//! Follow clock: real time, or simulation time when a route is simulated.
//!
//! Dwell and gap timers in the follow guest read `now_unix_s` from the host
//! status. When the app plays a route at N times real speed, that value must
//! advance N times faster, otherwise a 30 s dwell and a 60 s gap are measured
//! in wall-clock seconds and switch points drift from real driving.
//!
//! The clock never reads the system time itself. The caller passes the real
//! time, so tests drive it with a fake clock.

/// Smallest and largest accepted time scale.
pub const MIN_TIME_SCALE: f64 = 0.01;
pub const MAX_TIME_SCALE: f64 = 1000.0;

/// Maps real unix seconds to follow (simulation) unix seconds.
///
/// `sim = anchor_sim + (real - anchor_real) * scale`. Changing the scale
/// re-anchors at the current instant so the clock stays continuous: follow state
/// already stored with earlier timestamps never ends up in the future.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimClock {
    scale: f64,
    anchor_real_s: f64,
    anchor_sim_s: f64,
}

impl Default for SimClock {
    /// Scale 1.0 with a zero anchor: follow time equals real time.
    fn default() -> Self {
        Self {
            scale: 1.0,
            anchor_real_s: 0.0,
            anchor_sim_s: 0.0,
        }
    }
}

impl SimClock {
    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// Follow time in fractional unix seconds for the given real time.
    pub fn sim_now_s(&self, real_now_s: f64) -> f64 {
        (self.anchor_sim_s + (real_now_s - self.anchor_real_s) * self.scale).max(0.0)
    }

    /// Follow time in whole unix seconds for the given real time.
    pub fn sim_now_unix_s(&self, real_now_s: f64) -> u64 {
        self.sim_now_s(real_now_s) as u64
    }

    /// Set the time scale at real time `real_now_s`. Returns false and keeps the
    /// current scale when `scale` is not finite or outside the accepted range.
    /// Setting the scale already in force changes nothing.
    pub fn set_scale(&mut self, scale: f64, real_now_s: f64) -> bool {
        if !scale.is_finite() || !(MIN_TIME_SCALE..=MAX_TIME_SCALE).contains(&scale) {
            return false;
        }
        if (scale - self.scale).abs() < f64::EPSILON {
            return true;
        }
        self.anchor_sim_s = self.sim_now_s(real_now_s);
        self.anchor_real_s = real_now_s;
        self.scale = scale;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: f64 = 1_800_000_000.0;

    #[test]
    fn default_is_real_time() {
        let c = SimClock::default();
        assert_eq!(c.scale(), 1.0);
        assert_eq!(c.sim_now_unix_s(T0), T0 as u64);
        assert_eq!(c.sim_now_unix_s(T0 + 5.0), T0 as u64 + 5);
    }

    #[test]
    fn scale_multiplies_elapsed_time() {
        let mut c = SimClock::default();
        assert!(c.set_scale(12.0, T0));
        assert_eq!(c.sim_now_unix_s(T0), T0 as u64);
        assert_eq!(c.sim_now_unix_s(T0 + 5.0), T0 as u64 + 60);
        assert_eq!(c.sim_now_unix_s(T0 + 10.0), T0 as u64 + 120);
    }

    #[test]
    fn changing_scale_keeps_the_clock_continuous() {
        let mut c = SimClock::default();
        c.set_scale(20.0, T0);
        let before = c.sim_now_s(T0 + 30.0);
        c.set_scale(1.0, T0 + 30.0);
        assert!((c.sim_now_s(T0 + 30.0) - before).abs() < 1e-6);
        assert!((c.sim_now_s(T0 + 40.0) - (before + 10.0)).abs() < 1e-6);
    }

    #[test]
    fn same_scale_does_not_reanchor() {
        let mut c = SimClock::default();
        c.set_scale(12.0, T0);
        let snapshot = c;
        assert!(c.set_scale(12.0, T0 + 100.0));
        assert_eq!(c, snapshot);
    }

    #[test]
    fn invalid_scales_are_refused() {
        let mut c = SimClock::default();
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, 0.001, 5000.0] {
            assert!(!c.set_scale(bad, T0), "{bad}");
        }
        assert_eq!(c.scale(), 1.0);
    }
}
