//! `src/lib/nickname.ts`, the pure parts: the name a room list shows for you, a random one like "teal otter",
//! picked once and kept (Swift keeps it in `qcalc.nickname`). never the computer's own name.

use unicode_normalization::UnicodeNormalization;

use crate::app::js as jstr;
use crate::jsre::{self, Lazy, js};

pub const NICKNAME_MAX: usize = 24;

const COLOURS: [&str; 24] = [
    "teal", "amber", "coral", "indigo", "olive", "plum", "rust", "sage", "slate", "ochre",
    "cobalt", "crimson", "jade", "lilac", "mint", "navy", "peach", "ruby", "sandy", "silver",
    "golden", "russet", "violet", "azure",
];
const ANIMALS: [&str; 24] = [
    "otter", "heron", "fox", "badger", "lynx", "owl", "panda", "gecko", "koala", "marten", "newt",
    "orca", "puffin", "quail", "raven", "seal", "tapir", "wombat", "yak", "zebra", "bison",
    "crane", "dingo", "ferret",
];

pub fn random_nickname(rand: super::Rand) -> String {
    let mut pick =
        |list: &[&'static str]| list[((rand() * list.len() as f64).floor() as usize) % list.len()];
    let c = pick(&COLOURS);
    let a = pick(&ANIMALS);
    format!("{c} {a}")
}

/// Letters, digits, spaces and a little punctuation, single-spaced, at most NICKNAME_MAX; "" when nothing's left.
pub fn clean_nickname(raw: &str) -> String {
    static BAD: Lazy = Lazy::new(|| js(r"[^\p{L}\p{N} '’.-]", ""));
    static SPACES: Lazy = Lazy::new(|| js(r"\s+", ""));
    let nfc: String = raw.nfc().collect();
    let kept = jsre::replace_all(&BAD, &nfc, |_| String::new()).into_owned();
    let single = jsre::replace_all(&SPACES, &kept, |_| " ".into()).into_owned();
    let cut = jstr::take16(jstr::trim(&single), NICKNAME_MAX);
    // a cut through a surrogate pair leaves a lone half in the TS; take16 keeps whole characters
    jstr::trim(&cut).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_names_pair_a_colour_and_an_animal() {
        let mut r = || 0.0;
        assert_eq!(random_nickname(&mut r), "teal otter");
        let mut r = || 0.999;
        assert_eq!(random_nickname(&mut r), "azure ferret");
    }

    #[test]
    fn cleans_names() {
        assert_eq!(clean_nickname("  teal   otter  "), "teal otter");
        assert_eq!(clean_nickname("<b>hi</b>"), "bhib");
        assert_eq!(clean_nickname("O’Brien-Smith Jr."), "O’Brien-Smith Jr.");
        assert_eq!(clean_nickname("a".repeat(40).as_str()).len(), NICKNAME_MAX);
        assert_eq!(clean_nickname("🦊"), "");
        assert_eq!(clean_nickname("café"), "café");
    }
}
