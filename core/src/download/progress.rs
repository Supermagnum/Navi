//! Per-consumer progress slots so download, plan, convert, and cone do not clobber
//! each other. `set` / `snapshot` / `clear` write the **current thread** channel
//! (default: [`ProgressChannel::Download`]).

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProgressChannel {
    Download = 0,
    Plan = 1,
    Convert = 2,
    Cone = 3,
}

impl ProgressChannel {
    #[cfg(test)]
    const ALL: [ProgressChannel; 4] = [
        ProgressChannel::Download,
        ProgressChannel::Plan,
        ProgressChannel::Convert,
        ProgressChannel::Cone,
    ];

    fn index(self) -> usize {
        self as usize
    }
}

struct Slot {
    bytes: AtomicU64,
    total: AtomicU64,
    label: Mutex<String>,
}

impl Slot {
    fn new() -> Self {
        Self {
            bytes: AtomicU64::new(0),
            total: AtomicU64::new(0),
            label: Mutex::new(String::new()),
        }
    }
}

fn slots() -> &'static [Slot; 4] {
    static SLOTS: OnceLock<[Slot; 4]> = OnceLock::new();
    SLOTS.get_or_init(|| std::array::from_fn(|_| Slot::new()))
}

thread_local! {
    static CURRENT: Cell<ProgressChannel> = const { Cell::new(ProgressChannel::Download) };
}

/// Restores the previous channel when dropped (including panic unwind).
pub struct ChannelGuard {
    prev: ProgressChannel,
}

impl ChannelGuard {
    pub fn enter(ch: ProgressChannel) -> Self {
        let prev = CURRENT.with(|c| {
            let p = c.get();
            c.set(ch);
            p
        });
        Self { prev }
    }
}

impl Drop for ChannelGuard {
    fn drop(&mut self) {
        CURRENT.with(|c| c.set(self.prev));
    }
}

pub fn with_channel<R>(ch: ProgressChannel, f: impl FnOnce() -> R) -> R {
    let _g = ChannelGuard::enter(ch);
    f()
}

pub fn current_channel() -> ProgressChannel {
    CURRENT.with(|c| c.get())
}

/// Update progress for the current thread's channel.
pub fn set(bytes_or_units: u64, total: Option<u64>, label: &str) {
    set_on(current_channel(), bytes_or_units, total, label);
}

pub fn set_on(ch: ProgressChannel, bytes_or_units: u64, total: Option<u64>, label: &str) {
    let s = &slots()[ch.index()];
    s.bytes.store(bytes_or_units, Ordering::Relaxed);
    s.total.store(total.unwrap_or(0), Ordering::Relaxed);
    if let Ok(mut g) = s.label.lock() {
        *g = label.to_string();
    }
    // Convert phases are long; surface the active label in logcat so device
    // LMK / crash dumps can identify which phase was in progress.
    if ch == ProgressChannel::Convert {
        log::info!(target: "NaviConvert", "CONVERT_PHASE {label}");
    }
}

/// Clear the current thread's channel.
pub fn clear() {
    clear_on(current_channel());
}

pub fn clear_on(ch: ProgressChannel) {
    let s = &slots()[ch.index()];
    s.bytes.store(0, Ordering::Relaxed);
    s.total.store(0, Ordering::Relaxed);
    if let Ok(mut g) = s.label.lock() {
        g.clear();
    }
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub units_done: u64,
    pub units_total: Option<u64>,
    pub percent: Option<u32>,
    pub label: String,
}

fn snapshot_slot(s: &Slot) -> Snapshot {
    let done = s.bytes.load(Ordering::Relaxed);
    let total_raw = s.total.load(Ordering::Relaxed);
    let total = if total_raw == 0 {
        None
    } else {
        Some(total_raw)
    };
    let percent = total.map(|t| {
        if t == 0 {
            100
        } else {
            ((done.saturating_mul(100)) / t).min(100) as u32
        }
    });
    let label = s.label.lock().map(|g| g.clone()).unwrap_or_default();
    Snapshot {
        units_done: done,
        units_total: total,
        percent,
        label,
    }
}

pub fn snapshot() -> Snapshot {
    snapshot_on(current_channel())
}

pub fn snapshot_on(ch: ProgressChannel) -> Snapshot {
    snapshot_slot(&slots()[ch.index()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::thread;

    /// Slots are process-global. Serialize these tests against each other; do
    /// not assert the Download slot (bbox/extract tests write it concurrently).
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn channels_do_not_clobber() {
        let _lock = TEST_LOCK.lock().unwrap();
        for ch in ProgressChannel::ALL {
            clear_on(ch);
        }
        set_on(
            ProgressChannel::Plan,
            1,
            Some(4),
            "test-plan: indexing area…",
        );
        set_on(ProgressChannel::Convert, 2, Some(5), "test-convert: graph…");
        set_on(
            ProgressChannel::Cone,
            3,
            Some(4),
            "test-cone: loading geometry…",
        );

        let plan = snapshot_on(ProgressChannel::Plan);
        assert_eq!(plan.label, "test-plan: indexing area…");
        assert_eq!(plan.percent, Some(25));
        let conv = snapshot_on(ProgressChannel::Convert);
        assert_eq!(conv.label, "test-convert: graph…");
        assert_eq!(conv.percent, Some(40));
        let cone = snapshot_on(ProgressChannel::Cone);
        assert_eq!(cone.label, "test-cone: loading geometry…");
        assert_eq!(cone.percent, Some(75));
    }

    #[test]
    fn thread_local_channel_selects_slot() {
        let _lock = TEST_LOCK.lock().unwrap();
        for ch in ProgressChannel::ALL {
            clear_on(ch);
        }
        let h = thread::spawn(|| {
            let _g = ChannelGuard::enter(ProgressChannel::Cone);
            set(2, Some(4), "test-cone: reading roads…");
        });
        let _g = ChannelGuard::enter(ProgressChannel::Plan);
        set(3, Some(4), "test-plan: linking graph…");
        h.join().unwrap();
        assert_eq!(
            snapshot_on(ProgressChannel::Plan).label,
            "test-plan: linking graph…"
        );
        assert_eq!(snapshot_on(ProgressChannel::Plan).percent, Some(75));
        assert_eq!(
            snapshot_on(ProgressChannel::Cone).label,
            "test-cone: reading roads…"
        );
        assert_eq!(snapshot_on(ProgressChannel::Cone).percent, Some(50));
    }
}
