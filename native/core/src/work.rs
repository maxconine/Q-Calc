//! Budgets count work, not milliseconds (`src/engine/work.ts`): the same input always gets the same answer,
//! however busy the machine is. Per thread, so parallel evaluations don't share a meter.

use std::cell::Cell;

thread_local! {
    static DONE: Cell<u64> = const { Cell::new(0) };
}

/// One unit is about one evaluated character of an expression.
pub fn spend(units: u64) {
    DONE.with(|d| d.set(d.get().saturating_add(units)));
    check_cancelled(); // WP-FFI-API: every budgeted loop is a cancellation point
}

/// A budget of `units` from now.
#[derive(Clone, Copy, Debug)]
pub struct WorkBudget {
    until: u64,
}

impl WorkBudget {
    pub fn new(units: u64) -> Self {
        WorkBudget {
            until: DONE.with(Cell::get).saturating_add(units),
        }
    }

    /// Whether the work done since the budget began is past it.
    pub fn over(&self) -> bool {
        check_cancelled(); // WP-FFI-API
        DONE.with(Cell::get) > self.until
    }
}

// ---- WP-FFI-API: cancellation (ARCHITECTURE.md §5) ----

thread_local! {
    static CANCEL: std::cell::RefCell<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>> =
        const { std::cell::RefCell::new(None) };
}

/// The panic payload a cancelled evaluation unwinds with; the FFI catches it and reports `Cancelled`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cancelled;

/// Runs `f` with `flag` as this thread's cancel flag: once it is set, the next [`spend`] or
/// [`WorkBudget::over`] unwinds with a [`Cancelled`] panic payload. Nothing else changes, so an uncancelled run
/// gives exactly the answer it would without a flag.
pub fn with_cancel<R>(
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    f: impl FnOnce() -> R,
) -> R {
    struct Reset(Option<std::sync::Arc<std::sync::atomic::AtomicBool>>);
    impl Drop for Reset {
        fn drop(&mut self) {
            let old = self.0.take();
            CANCEL.with(|c| *c.borrow_mut() = old);
        }
    }
    let _reset = Reset(CANCEL.with(|c| c.borrow_mut().replace(flag)));
    f()
}

/// Whether this thread's evaluation has been cancelled.
pub fn cancelled() -> bool {
    CANCEL.with(|c| {
        c.borrow()
            .as_ref()
            .is_some_and(|f| f.load(std::sync::atomic::Ordering::Relaxed))
    })
}

fn check_cancelled() {
    if cancelled() {
        std::panic::panic_any(Cancelled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budgets_count_units() {
        let b = WorkBudget::new(10);
        spend(5);
        assert!(!b.over());
        spend(6);
        assert!(b.over());
    }

    #[test]
    fn a_cancelled_flag_unwinds_at_the_next_spend() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        let flag = Arc::new(AtomicBool::new(false));
        with_cancel(flag.clone(), || spend(1));
        flag.store(true, Ordering::Relaxed);
        let out = std::panic::catch_unwind(|| with_cancel(flag.clone(), || spend(1)));
        assert!(out.unwrap_err().downcast_ref::<Cancelled>().is_some());
        // the flag is gone once the run ends, even by unwinding
        assert!(!cancelled());
        spend(1);
    }
}
