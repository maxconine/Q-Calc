//! `NameCheck` (`qcalc_core::app::name_check`, from `src/lib/nameCheck.ts`) for the shells: whether the name after
//! `message` is anyone's, as the relay last said. The shell does the debounce and the `POST /lookup`; this keeps
//! the answers and says when to ask.

use std::sync::{Arc, Mutex};

use qcalc_core::app::messaging::SendStatus;
use qcalc_core::app::name_check as core;

/// How long typing rests before a name is asked about, in ms.
#[uniffi::export]
pub fn name_check_delay_ms() -> u32 {
    core::CHECK_DELAY_MS as u32
}

/// The relay's answers about names, kept a while. Times are ms since the epoch.
#[derive(Debug, Default, uniffi::Object)]
pub struct NameCheck {
    inner: Mutex<core::NameCheck>,
}

#[uniffi::export]
impl NameCheck {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(NameCheck::default())
    }

    /// The kept answer, or nil when there's none still fresh.
    pub fn cached(&self, name: String, now_ms: f64) -> Option<bool> {
        self.inner.lock().ok()?.cached(&name, now_ms)
    }

    /// Whether to ask the relay now: no fresh answer and no ask already out. A yes waits for `finish`.
    pub fn start(&self, name: String, now_ms: f64) -> bool {
        self.inner.lock().is_ok_and(|mut c| c.start(&name, now_ms))
    }

    /// The ask came back: true or false from the relay, nil when it couldn't say (kept as nothing).
    pub fn finish(&self, name: String, found: Option<bool>, now_ms: f64) {
        if let Ok(mut c) = self.inner.lock() {
            c.finish(&name, found, now_ms);
        }
    }

    /// What a send's status said about the name (`delivered`, `offline`, `unknown`…, as the relay spells it).
    pub fn note_send(&self, name: String, status: String, now_ms: f64) {
        let Ok(status) = serde_json::from_value::<SendStatus>(serde_json::Value::String(status))
        else {
            return;
        };
        if let Ok(mut c) = self.inner.lock() {
            c.note_send(&name, status, now_ms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_what_the_relay_and_sends_said() {
        let c = NameCheck::new();
        assert!(c.start("max".into(), 0.0));
        assert!(!c.start("max".into(), 1.0));
        c.finish("max".into(), Some(true), 2.0);
        assert_eq!(c.cached("max".into(), 3.0), Some(true));
        c.note_send("zed".into(), "unknown".into(), 0.0);
        assert_eq!(c.cached("zed".into(), 1.0), Some(false));
        c.note_send("amy".into(), "network".into(), 0.0);
        assert_eq!(c.cached("amy".into(), 1.0), None);
        assert_eq!(name_check_delay_ms(), 350);
    }
}
