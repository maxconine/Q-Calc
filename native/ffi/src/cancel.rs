//! Cancellation for long work (ARCHITECTURE.md §5): a `CancelToken` is an `Arc<AtomicBool>` the engine's worker
//! installs as the thread's cancel flag (`qcalc_core::work::with_cancel`), so every `work::spend` checkpoint
//! inside the evaluation can stop it. No timers: an uncancelled job gives exactly its synchronous answer.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Cancels one async engine call. Make a new one per call; once cancelled it stays cancelled.
#[derive(Debug, Default, uniffi::Object)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
}

#[uniffi::export]
impl CancelToken {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(CancelToken::default())
    }

    /// Stops the call at its next checkpoint (or before it starts); the call throws `CoreError::Cancelled`.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }
}

impl CancelToken {
    pub(crate) fn flag(&self) -> Arc<AtomicBool> {
        self.flag.clone()
    }
}
