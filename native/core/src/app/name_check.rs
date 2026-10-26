//! Whether a name typed after `message` is anyone's (`src/lib/nameCheck.ts`), without the network: the shell asks
//! the relay (`POST /lookup`) a moment after typing rests, and this keeps the answers a while, so a name typed
//! again or a backspace back to it doesn't ask again. Only the relay's own answers are kept: an ask that couldn't
//! be made says nothing and is tried again next time, so offline never reads as "not found".
//!
//! The TS class holds the ask's promise; here the shell calls `start` before asking and `finish` with the answer,
//! and `start` says no while an ask for that name is out (one ask per name at a time). Times are ms, passed in.

use std::collections::HashSet;

use crate::app::messaging::SendStatus;

/// A held name stays held for weeks; a free one can be claimed any minute.
pub const FOUND_MS: f64 = 10.0 * 60_000.0;
pub const MISSING_MS: f64 = 30_000.0;
/// How long typing rests before a name is asked about.
pub const CHECK_DELAY_MS: f64 = 350.0;
const CACHE_MAX: usize = 200;

#[derive(Clone, Debug, Default)]
pub struct NameCheck {
    /// (name, found, when), the oldest first, as the TS map's insertion order
    kept: Vec<(String, bool, f64)>,
    asking: HashSet<String>,
}

impl NameCheck {
    pub fn new() -> Self {
        Self::default()
    }

    /// The kept answer, or `None` when there's none still fresh.
    pub fn cached(&mut self, name: &str, now: f64) -> Option<bool> {
        let i = self.kept.iter().position(|(n, _, _)| n == name)?;
        let (_, found, at) = self.kept[i];
        if now - at > if found { FOUND_MS } else { MISSING_MS } {
            self.kept.remove(i);
            return None;
        }
        Some(found)
    }

    /// What the relay said, kept as the newest.
    pub fn note(&mut self, name: &str, found: bool, now: f64) {
        self.kept.retain(|(n, _, _)| n != name);
        self.kept.push((name.to_string(), found, now));
        if self.kept.len() > CACHE_MAX {
            let extra = self.kept.len() - CACHE_MAX;
            self.kept.drain(..extra);
        }
    }

    /// What a send found out on the way: delivered or offline means someone holds it, unknown that nobody does.
    pub fn note_send(&mut self, name: &str, status: SendStatus, now: f64) {
        match status {
            SendStatus::Delivered | SendStatus::Offline => self.note(name, true, now),
            SendStatus::Unknown => self.note(name, false, now),
            _ => {}
        }
    }

    /// Whether to ask the relay about `name` now: not when a fresh answer is kept or an ask is already out.
    /// A yes counts the ask as out until `finish`.
    pub fn start(&mut self, name: &str, now: f64) -> bool {
        if self.cached(name, now).is_some() || self.asking.contains(name) {
            return false;
        }
        self.asking.insert(name.to_string());
        true
    }

    /// The ask came back: an answer is kept, `None` (offline, too many asks, your own name lapsed) keeps nothing.
    pub fn finish(&mut self, name: &str, found: Option<bool>, now: f64) {
        self.asking.remove(name);
        if let Some(found) = found {
            self.note(name, found, now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_once_keeps_the_answer_and_asks_again_when_stale() {
        let mut c = NameCheck::new();
        assert_eq!(c.cached("max", 0.0), None);
        assert!(c.start("max", 0.0));
        c.finish("max", Some(true), 0.0);
        assert!(!c.start("max", 0.0));
        assert!(c.start("zed", 0.0));
        c.finish("zed", Some(false), 0.0);
        assert_eq!(c.cached("zed", MISSING_MS + 1.0), None);
        assert_eq!(c.cached("max", MISSING_MS + 1.0), Some(true));
        assert_eq!(c.cached("max", FOUND_MS + 1.0), None);
        assert!(c.start("max", FOUND_MS + 1.0));
    }

    #[test]
    fn keeps_nothing_when_the_relay_couldnt_say() {
        let mut c = NameCheck::new();
        assert!(c.start("max", 0.0));
        c.finish("max", None, 0.0);
        assert_eq!(c.cached("max", 0.0), None);
        assert!(c.start("max", 1.0));
    }

    #[test]
    fn one_ask_per_name_while_its_out() {
        let mut c = NameCheck::new();
        assert!(c.start("max", 0.0));
        assert!(!c.start("max", 10.0));
        c.finish("max", Some(true), 20.0);
        assert_eq!(c.cached("max", 20.0), Some(true));
    }

    #[test]
    fn takes_what_a_send_found_out() {
        let mut c = NameCheck::new();
        c.note_send("zed", SendStatus::Unknown, 0.0);
        assert_eq!(c.cached("zed", 0.0), Some(false));
        c.note_send("zed", SendStatus::Offline, 0.0);
        assert_eq!(c.cached("zed", 0.0), Some(true));
        c.note_send("amy", SendStatus::Network, 0.0);
        assert_eq!(c.cached("amy", 0.0), None);
    }

    #[test]
    fn forgets_the_oldest_past_the_cap() {
        let mut c = NameCheck::new();
        for i in 0..=CACHE_MAX {
            c.note(&format!("n{i}"), true, 0.0);
        }
        assert_eq!(c.cached("n0", 0.0), None);
        assert_eq!(c.cached("n1", 0.0), Some(true));
    }
}
