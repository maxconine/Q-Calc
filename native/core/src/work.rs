//! Budgets count work, not milliseconds (`src/engine/work.ts`): the same input always gets the same answer,
//! however busy the machine is. Per thread, so parallel evaluations don't share a meter.

use std::cell::Cell;

thread_local! {
    static DONE: Cell<u64> = const { Cell::new(0) };
}

/// One unit is about one evaluated character of an expression.
pub fn spend(units: u64) {
    DONE.with(|d| d.set(d.get().saturating_add(units)));
}

/// A budget of `units` from now.
#[derive(Clone, Copy, Debug)]
pub struct WorkBudget {
    until: u64,
}

impl WorkBudget {
    pub fn new(units: u64) -> Self {
        WorkBudget { until: DONE.with(Cell::get).saturating_add(units) }
    }

    /// Whether the work done since the budget began is past it.
    pub fn over(&self) -> bool {
        DONE.with(Cell::get) > self.until
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
}
