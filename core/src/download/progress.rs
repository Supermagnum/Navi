//! Per-consumer progress slots so download, plan, convert, and cone do not clobber
//! each other. `set` / `snapshot` / `clear` write the **current thread** channel
//! (default: [`ProgressChannel::Download`]).
//!
//! Optional [`set_region_tag`] annotates every subsequent [`set`] label with the
//! active region id (and optional N-of-M) until cleared — so bare phase labels
//! like "Writing map archive…" become region-aware without threading the id
//! through every call site.

use std::cell::{Cell, RefCell};
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
    /// Per-thread so a convert/basemap worker cannot annotate another thread's
    /// place-index / plan progress labels (and so tests stay isolated).
    static REGION_TAG: RefCell<Option<RegionTag>> = const { RefCell::new(None) };
}

struct RegionTag {
    /// Geofabrik path or leaf id shown in progress labels.
    id: String,
    /// 1-based corridor index when known.
    index: Option<u32>,
    /// Corridor length when known.
    total: Option<u32>,
}

/// Set the region identity attached to subsequent [`set`] / [`set_on`] labels.
/// Pass empty `region_id` to clear. [index]/[total] are optional 1-based
/// corridor positions (kept when both are `Some` and total > 0).
pub fn set_region_tag(region_id: &str, index: Option<u32>, total: Option<u32>) {
    let id = region_id.trim().trim_matches('/').to_string();
    REGION_TAG.with(|c| {
        if id.is_empty() {
            *c.borrow_mut() = None;
        } else {
            *c.borrow_mut() = Some(RegionTag { id, index, total });
        }
    });
}

/// Clear the region tag (same as `set_region_tag("", None, None)`).
pub fn clear_region_tag() {
    set_region_tag("", None, None);
}

fn annotate_with_region(label: &str) -> String {
    REGION_TAG.with(|c| {
        let g = c.borrow();
        let Some(tag) = g.as_ref() else {
            return label.to_string();
        };
        if tag.id.is_empty() {
            return label.to_string();
        }
        let leaf = tag.id.rsplit('/').next().unwrap_or(tag.id.as_str());
        let lower = label.to_ascii_lowercase();
        if lower.contains(&tag.id.to_ascii_lowercase())
            || (!leaf.is_empty() && lower.contains(&leaf.to_ascii_lowercase()))
        {
            return label.to_string();
        }
        match (tag.index, tag.total) {
            (Some(i), Some(t)) if t > 0 && i > 0 => {
                format!("{label} (region {i} of {t}: {leaf})")
            }
            _ => format!("{label} ({leaf})"),
        }
    })
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
    let annotated = annotate_with_region(label);
    let s = &slots()[ch.index()];
    s.bytes.store(bytes_or_units, Ordering::Relaxed);
    s.total.store(total.unwrap_or(0), Ordering::Relaxed);
    if let Ok(mut g) = s.label.lock() {
        *g = annotated.clone();
    }
    // Convert phases are long; surface the active label in logcat so device
    // LMK / crash dumps can identify which phase was in progress.
    if ch == ProgressChannel::Convert {
        log::info!(target: "NaviConvert", "CONVERT_PHASE {annotated}");
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
        done.saturating_mul(100)
            .checked_div(t)
            .map(|p| p.min(100) as u32)
            .unwrap_or(100)
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

/// RAII: sets region tag on construct, clears on drop.
pub struct RegionTagGuard;

impl RegionTagGuard {
    pub fn enter(region_id: &str, index: Option<u32>, total: Option<u32>) -> Self {
        set_region_tag(region_id, index, total);
        Self
    }
}

impl Drop for RegionTagGuard {
    fn drop(&mut self) {
        clear_region_tag();
    }
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
        clear_region_tag();
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
        clear_region_tag();
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

    #[test]
    fn region_tag_annotates_bare_labels() {
        let _lock = TEST_LOCK.lock().unwrap();
        clear_on(ProgressChannel::Convert);
        clear_region_tag();
        set_region_tag("europe/sweden/vastra_gotaland", Some(2), Some(4));
        set_on(ProgressChannel::Convert, 0, Some(1), "Writing map archive…");
        let snap = snapshot_on(ProgressChannel::Convert);
        assert!(
            snap.label.contains("Writing map archive"),
            "got {}",
            snap.label
        );
        assert!(snap.label.contains("2 of 4"), "got {}", snap.label);
        assert!(snap.label.contains("vastra_gotaland"), "got {}", snap.label);
        clear_region_tag();
    }
}
