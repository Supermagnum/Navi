//! Rate-limited sensor hub behind the read-only `sensors_read` capability.
//!
//! The embedder (Android `SensorManager`, Linux IIO, a test double) implements
//! [`SensorBackend`]. The hub adds the policy every embedder needs:
//!
//! * **Availability detection** - only kinds the backend reports are served;
//!   anything else reads as [`SensorStatus::Unavailable`], never as zeros.
//! * **Rate limit** - a guest polling faster than `min_interval_ms` gets the
//!   cached sample, and the backend is registered with that sampling period.
//! * **Unregister when unused** - a listener is registered lazily on the first
//!   read and released by [`SensorHub::release_idle`] once no guest read it for
//!   `idle_timeout_ms` (call after each guest tick), or by
//!   [`SensorHub::release_all`] / drop when the plugin is disabled.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::abi::{SensorAvailabilityView, SensorKind, SensorReadView, SensorSample, SensorStatus};

/// Platform sensor access. Implementations must be cheap to call; `latest` must
/// not block.
pub trait SensorBackend: Send {
    fn is_available(&self, kind: SensorKind) -> bool;
    /// Start delivering `kind` no faster than `sampling_period_us`.
    /// Returns `false` when registration failed (treated as unavailable).
    fn register(&mut self, kind: SensorKind, sampling_period_us: u32) -> bool;
    fn unregister(&mut self, kind: SensorKind);
    /// Most recent sample delivered since registration.
    fn latest(&self, kind: SensorKind) -> Option<SensorSample>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SensorHubConfig {
    /// Minimum spacing between backend polls per sensor (and sampling period).
    pub min_interval_ms: u64,
    /// Unregister a sensor nobody read for this long.
    pub idle_timeout_ms: u64,
}

impl Default for SensorHubConfig {
    fn default() -> Self {
        Self {
            min_interval_ms: 100,
            idle_timeout_ms: 3_000,
        }
    }
}

struct Active {
    last_poll_ms: Option<i64>,
    last_read_ms: i64,
    cached: Option<SensorSample>,
}

struct Inner {
    backend: Box<dyn SensorBackend>,
    active: HashMap<SensorKind, Active>,
}

pub struct SensorHub {
    cfg: SensorHubConfig,
    clock_ms: Box<dyn Fn() -> i64 + Send + Sync>,
    inner: Mutex<Inner>,
}

impl SensorHub {
    pub fn new(
        backend: Box<dyn SensorBackend>,
        cfg: SensorHubConfig,
        clock_ms: Box<dyn Fn() -> i64 + Send + Sync>,
    ) -> Self {
        Self {
            cfg,
            clock_ms,
            inner: Mutex::new(Inner {
                backend,
                active: HashMap::new(),
            }),
        }
    }

    pub fn availability(&self) -> SensorAvailabilityView {
        let inner = self.inner.lock().expect("sensor hub lock");
        SensorAvailabilityView {
            available: SensorKind::ALL
                .iter()
                .copied()
                .filter(|k| inner.backend.is_available(*k))
                .collect(),
        }
    }

    pub fn read(&self, kind: SensorKind) -> SensorReadView {
        let now = (self.clock_ms)();
        let mut inner = self.inner.lock().expect("sensor hub lock");
        if !inner.backend.is_available(kind) {
            return SensorReadView::default();
        }
        if !inner.active.contains_key(&kind) {
            let period_us = (self.cfg.min_interval_ms.saturating_mul(1_000)).min(u32::MAX as u64);
            if !inner.backend.register(kind, period_us as u32) {
                return SensorReadView::default();
            }
            inner.active.insert(
                kind,
                Active {
                    last_poll_ms: None,
                    last_read_ms: now,
                    cached: None,
                },
            );
        }
        let min_interval = self.cfg.min_interval_ms as i64;
        let due = inner
            .active
            .get(&kind)
            .and_then(|a| a.last_poll_ms)
            .is_none_or(|t| now - t >= min_interval);
        let fresh = if due {
            inner.backend.latest(kind)
        } else {
            None
        };
        let entry = inner.active.get_mut(&kind).expect("registered above");
        entry.last_read_ms = now;
        if due {
            entry.last_poll_ms = Some(now);
            if fresh.is_some() {
                entry.cached = fresh;
            }
        }
        match entry.cached.clone() {
            Some(sample) => SensorReadView {
                status: SensorStatus::Ok,
                sample: Some(sample),
            },
            None => SensorReadView {
                status: SensorStatus::NoSample,
                sample: None,
            },
        }
    }

    /// Unregister sensors not read within `idle_timeout_ms`. Returns the kinds released.
    pub fn release_idle(&self) -> Vec<SensorKind> {
        let now = (self.clock_ms)();
        let mut inner = self.inner.lock().expect("sensor hub lock");
        let idle: Vec<SensorKind> = inner
            .active
            .iter()
            .filter(|(_, a)| now - a.last_read_ms >= self.cfg.idle_timeout_ms as i64)
            .map(|(k, _)| *k)
            .collect();
        for k in &idle {
            inner.backend.unregister(*k);
            inner.active.remove(k);
        }
        idle
    }

    /// Unregister everything (plugin disabled, app backgrounded, shutdown).
    pub fn release_all(&self) {
        let mut inner = self.inner.lock().expect("sensor hub lock");
        let kinds: Vec<SensorKind> = inner.active.keys().copied().collect();
        for k in kinds {
            inner.backend.unregister(k);
        }
        inner.active.clear();
    }

    /// Kinds currently registered with the backend.
    pub fn registered(&self) -> Vec<SensorKind> {
        let inner = self.inner.lock().expect("sensor hub lock");
        let mut v: Vec<SensorKind> = inner.active.keys().copied().collect();
        v.sort();
        v
    }
}

impl Drop for SensorHub {
    fn drop(&mut self) {
        self.release_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicI64, Ordering};
    use std::sync::Arc;

    #[derive(Default)]
    struct Shared {
        registered: HashSet<SensorKind>,
        register_calls: Vec<(SensorKind, u32)>,
        latest_calls: u32,
        value: f64,
    }

    struct FakeBackend {
        available: HashSet<SensorKind>,
        shared: Arc<Mutex<Shared>>,
    }

    impl SensorBackend for FakeBackend {
        fn is_available(&self, kind: SensorKind) -> bool {
            self.available.contains(&kind)
        }
        fn register(&mut self, kind: SensorKind, period_us: u32) -> bool {
            let mut s = self.shared.lock().unwrap();
            s.registered.insert(kind);
            s.register_calls.push((kind, period_us));
            true
        }
        fn unregister(&mut self, kind: SensorKind) {
            self.shared.lock().unwrap().registered.remove(&kind);
        }
        fn latest(&self, kind: SensorKind) -> Option<SensorSample> {
            let mut s = self.shared.lock().unwrap();
            s.latest_calls += 1;
            Some(SensorSample {
                kind,
                values: vec![s.value],
                timestamp_ms: 0,
                accuracy: Some(3),
            })
        }
    }

    fn hub(kinds: &[SensorKind]) -> (SensorHub, Arc<Mutex<Shared>>, Arc<AtomicI64>) {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let now = Arc::new(AtomicI64::new(1_000));
        let clock = Arc::clone(&now);
        let hub = SensorHub::new(
            Box::new(FakeBackend {
                available: kinds.iter().copied().collect(),
                shared: Arc::clone(&shared),
            }),
            SensorHubConfig::default(),
            Box::new(move || clock.load(Ordering::SeqCst)),
        );
        (hub, shared, now)
    }

    #[test]
    fn availability_lists_only_backend_sensors() {
        let (hub, _, _) = hub(&[SensorKind::Gravity, SensorKind::Pressure]);
        assert_eq!(
            hub.availability().available,
            vec![SensorKind::Gravity, SensorKind::Pressure]
        );
    }

    #[test]
    fn missing_sensor_reads_unavailable_and_is_never_registered() {
        let (hub, shared, _) = hub(&[SensorKind::Gravity]);
        let v = hub.read(SensorKind::Gyroscope);
        assert_eq!(v.status, SensorStatus::Unavailable);
        assert!(v.sample.is_none());
        assert!(shared.lock().unwrap().register_calls.is_empty());
    }

    #[test]
    fn reads_are_rate_limited_and_registered_with_that_period() {
        let (hub, shared, now) = hub(&[SensorKind::Accelerometer]);
        shared.lock().unwrap().value = 1.0;
        let first = hub.read(SensorKind::Accelerometer);
        assert_eq!(first.status, SensorStatus::Ok);
        assert_eq!(
            shared.lock().unwrap().register_calls,
            vec![(SensorKind::Accelerometer, 100_000)]
        );

        shared.lock().unwrap().value = 2.0;
        now.fetch_add(40, Ordering::SeqCst);
        let cached = hub.read(SensorKind::Accelerometer);
        assert_eq!(cached.sample.unwrap().values, vec![1.0]);
        assert_eq!(shared.lock().unwrap().latest_calls, 1);

        now.fetch_add(70, Ordering::SeqCst);
        let fresh = hub.read(SensorKind::Accelerometer);
        assert_eq!(fresh.sample.unwrap().values, vec![2.0]);
        assert_eq!(shared.lock().unwrap().latest_calls, 2);
    }

    #[test]
    fn idle_sensors_are_unregistered_and_reregistered_on_demand() {
        let (hub, shared, now) = hub(&[SensorKind::Gravity, SensorKind::Light]);
        hub.read(SensorKind::Gravity);
        hub.read(SensorKind::Light);
        assert_eq!(hub.registered().len(), 2);

        now.fetch_add(2_000, Ordering::SeqCst);
        hub.read(SensorKind::Gravity);
        assert!(hub.release_idle().is_empty());

        now.fetch_add(1_500, Ordering::SeqCst);
        // Light unread for 3500 ms, gravity for 1500 ms.
        assert_eq!(hub.release_idle(), vec![SensorKind::Light]);
        assert_eq!(hub.registered(), vec![SensorKind::Gravity]);
        assert!(!shared
            .lock()
            .unwrap()
            .registered
            .contains(&SensorKind::Light));

        hub.read(SensorKind::Light);
        assert_eq!(hub.registered().len(), 2);
    }

    #[test]
    fn release_all_and_drop_unregister_everything() {
        let (hub, shared, _) = hub(&[SensorKind::Gravity, SensorKind::Light]);
        hub.read(SensorKind::Gravity);
        hub.read(SensorKind::Light);
        hub.release_all();
        assert!(shared.lock().unwrap().registered.is_empty());
        hub.read(SensorKind::Gravity);
        assert_eq!(shared.lock().unwrap().registered.len(), 1);
        drop(hub);
        assert!(shared.lock().unwrap().registered.is_empty());
    }
}
