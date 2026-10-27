//! `src/lib/gameSaves.ts`: the keys a game keeps between launches, and the line on their size. The web page kept
//! them in local storage and mirrored them to Swift; the native games read and write Swift's copy directly
//! (`AppSettings.saveGame`), so only the rules are here.

use serde_json::Value as Json;

pub const GAME_SAVE_KEYS: [&str; 7] = [
    "qcalc-equal",       // streak, stats, today's daily
    "qcalc-numbers",     // 24 and countdown stats
    "qcalc-crack",       // best level
    "qcalc-crack-muted", //
    "qcalc-ember",       // ember & frost progress and mute
    "qcalc-c4-ghost",    //
    "qcalc-host-locked", // the lobby's last open/locked pick
];

/// Per value, in characters (UTF-16 units); Swift holds the same line.
pub const GAME_SAVE_MAX: usize = 32 * 1024;

pub fn is_game_save_key(key: &str) -> bool {
    GAME_SAVE_KEYS.contains(&key)
}

fn fits(value: &Json) -> Option<&str> {
    value
        .as_str()
        .filter(|s| s.encode_utf16().count() <= GAME_SAVE_MAX)
}

/// What the native copy puts back: known keys and sane values only, and never over what the page already has.
pub fn saves_to_hydrate(injected: &Json, has: impl Fn(&str) -> bool) -> Vec<(String, String)> {
    let Some(o) = injected.as_object() else {
        return vec![];
    };
    o.iter()
        .filter(|(k, _)| is_game_save_key(k) && !has(k))
        .filter_map(|(k, v)| fits(v).map(|v| (k.clone(), v.to_string())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn covers_the_game_keys_and_not_the_rest_of_the_app() {
        for k in [
            "qcalc-equal",
            "qcalc-numbers",
            "qcalc-crack",
            "qcalc-crack-muted",
            "qcalc-ember",
            "qcalc-host-locked",
        ] {
            assert!(is_game_save_key(k));
        }
        for k in [
            "qcalc-history",
            "qcalc-settings",
            "qcalc-draft",
            "qcalc-onboarding",
            "qcalc-nickname",
            "qcalc-crack-x",
        ] {
            assert!(!is_game_save_key(k));
        }
    }

    #[test]
    fn hydrates_known_keys_that_fit_and_never_overwrites() {
        let big = "x".repeat(GAME_SAVE_MAX + 1);
        let got = saves_to_hydrate(
            &json!({"qcalc-equal": "{\"a\":1}", "qcalc-crack": 5, "qcalc-history": "[]", "qcalc-ember": big, "qcalc-crack-muted": "1"}),
            |_| false,
        );
        let mut got = got;
        got.sort();
        assert_eq!(
            got,
            vec![
                ("qcalc-crack-muted".into(), "1".into()),
                ("qcalc-equal".into(), "{\"a\":1}".into())
            ]
        );
        for bad in [
            json!(null),
            json!("qcalc-equal"),
            json!(["qcalc-equal", "x"]),
            json!(3),
        ] {
            assert!(saves_to_hydrate(&bad, |_| false).is_empty());
        }
        assert_eq!(
            saves_to_hydrate(&json!({"qcalc-equal": "old", "qcalc-crack": "3"}), |k| k
                == "qcalc-equal"),
            vec![("qcalc-crack".into(), "3".into())]
        );
    }
}
