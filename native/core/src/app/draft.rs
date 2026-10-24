//! `src/lib/draft.ts`: what hiding the bar does with the input, and whether a saved draft comes back.

use crate::units::is_improper_unit_conversion;

pub const DEFAULT_DRAFT_SECONDS: f64 = 60.0;
pub const MIN_DRAFT_SECONDS: f64 = 0.0;
pub const MAX_DRAFT_SECONDS: f64 = 3600.0;

/// The draft-keeping setting, in whole seconds within 0…3600; junk is the default.
pub fn clamp_draft_seconds(n: f64) -> f64 {
    if !n.is_finite() || n < MIN_DRAFT_SECONDS {
        return DEFAULT_DRAFT_SECONDS;
    }
    MAX_DRAFT_SECONDS.min(crate::jsnum::round(n))
}

/// What hiding the bar does with what's typed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HideAction {
    Commit,
    Keep,
    Clear,
}

impl HideAction {
    pub fn as_str(self) -> &'static str {
        match self {
            HideAction::Commit => "commit",
            HideAction::Keep => "keep",
            HideAction::Clear => "clear",
        }
    }
}

/// Hiding keeps the input for `draft_seconds`; only with that off does hiding commit.
pub fn hide_action(expr: &str, display: &str, draft_seconds: f64) -> HideAction {
    if expr.trim().is_empty() {
        return HideAction::Clear;
    }
    if draft_seconds > 0.0 {
        return HideAction::Keep;
    }
    if !display.is_empty() && !is_improper_unit_conversion(display) {
        HideAction::Commit
    } else {
        HideAction::Clear
    }
}

/// Whether a draft saved at `saved_at` (ms) is still fresh at `now`.
pub fn should_restore_draft(saved_at: f64, now: f64, draft_seconds: f64) -> bool {
    if draft_seconds <= 0.0 || !saved_at.is_finite() || saved_at <= 0.0 {
        return false;
    }
    now - saved_at < draft_seconds * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drafts() {
        assert_eq!(clamp_draft_seconds(-1.0), 60.0);
        assert_eq!(clamp_draft_seconds(1e9), 3600.0);
        assert_eq!(hide_action("2+2", "4", 0.0), HideAction::Commit);
        assert_eq!(
            hide_action("2 m + 3 s", "improper unit conversion", 0.0),
            HideAction::Clear
        );
        assert!(should_restore_draft(1000.0, 2000.0, 30.0));
    }
}
