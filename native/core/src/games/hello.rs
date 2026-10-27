//! `src/lib/hello.ts`: every linked game opens with a hello tagged with its game, so a friend who opened another
//! game is told which. older builds sent pong's and ember's hellos as plain `hi` (ember's with u, or w from ember
//! 2), connect 4's as `c4` and ultimate's as `ut` and trails' as `tr`, so those still read as theirs.

use serde_json::{Map, Value as Json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkedGame {
    Pong,
    Connect4,
    Ultimate,
    Ember,
    Trails,
}

impl LinkedGame {
    pub fn name(self) -> &'static str {
        match self {
            LinkedGame::Pong => "pong",
            LinkedGame::Connect4 => "connect 4",
            LinkedGame::Ultimate => "ultimate",
            LinkedGame::Ember => "ember",
            LinkedGame::Trails => "trails",
        }
    }

    fn from_id(id: &str) -> Option<LinkedGame> {
        Some(match id {
            "pong" => LinkedGame::Pong,
            "connect4" => LinkedGame::Connect4,
            "ultimate" => LinkedGame::Ultimate,
            "ember" => LinkedGame::Ember,
            "trails" => LinkedGame::Trails,
            _ => return None,
        })
    }
}

/// The game a decoded message's hello belongs to, or None when it isn't a hello.
pub fn hello_game(o: &Map<String, Json>) -> Option<LinkedGame> {
    let t = o.get("t").and_then(Json::as_str);
    if let Some(g) = o
        .get("g")
        .and_then(Json::as_str)
        .and_then(LinkedGame::from_id)
        && matches!(t, Some("hi" | "c4" | "ut" | "tr"))
    {
        return Some(g);
    }
    match t {
        Some("c4") => Some(LinkedGame::Connect4),
        Some("tr") => Some(LinkedGame::Trails),
        Some("ut") => Some(LinkedGame::Ultimate),
        Some("hi") => Some(if o.contains_key("u") || o.contains_key("w") {
            LinkedGame::Ember
        } else {
            LinkedGame::Pong
        }),
        _ => None,
    }
}

/// Another game's hello, as the name to show; None for this game's own or for anything else.
pub fn other_hello(o: &Map<String, Json>, mine: LinkedGame) -> Option<&'static str> {
    hello_game(o).filter(|g| *g != mine).map(LinkedGame::name)
}

pub fn other_game_note(other: &str, mine: LinkedGame) -> String {
    format!("your friend opened {other} · both type {}", mine.name())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn o(v: Json) -> Map<String, Json> {
        v.as_object().cloned().unwrap_or_default()
    }

    #[test]
    fn reads_hellos_old_and_new() {
        assert_eq!(
            hello_game(&o(json!({"t": "hi", "v": 2, "g": "pong"}))),
            Some(LinkedGame::Pong)
        );
        assert_eq!(
            hello_game(&o(json!({"t": "hi", "v": 1}))),
            Some(LinkedGame::Pong)
        );
        assert_eq!(
            hello_game(&o(json!({"t": "hi", "u": 1}))),
            Some(LinkedGame::Ember)
        );
        assert_eq!(
            hello_game(&o(json!({"t": "c4", "v": 1}))),
            Some(LinkedGame::Connect4)
        );
        assert_eq!(
            hello_game(&o(json!({"t": "hi", "g": "ultimate"}))),
            Some(LinkedGame::Ultimate)
        );
        assert_eq!(hello_game(&o(json!({"t": "p"}))), None);
        assert_eq!(
            other_hello(&o(json!({"t": "c4", "v": 1})), LinkedGame::Pong),
            Some("connect 4")
        );
        assert_eq!(
            other_hello(&o(json!({"t": "c4", "v": 1})), LinkedGame::Connect4),
            None
        );
        assert_eq!(
            other_game_note("pong", LinkedGame::Connect4),
            "your friend opened pong · both type connect 4"
        );
    }
}
