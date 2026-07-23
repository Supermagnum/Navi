use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Handle for pause / resume / cancel during a long-running download job.
#[derive(Clone, Default)]
pub struct DownloadControl {
    paused: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
}

impl DownloadControl {
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Clear both flags so a fresh run can start (e.g. resume after pause).
    pub fn reset(&self) {
        self.paused.store(false, Ordering::SeqCst);
        self.cancelled.store(false, Ordering::SeqCst);
    }
}
