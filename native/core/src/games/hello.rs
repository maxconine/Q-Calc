//! Every linked game opens with a hello tagged with its game, so a friend who opened another game is told which
//! (`src/lib/hello.ts`). Older builds sent pong's and ember's hellos as plain `hi` (ember's with u, or w from
//! ember 2), connect 4's as `c4`, ultimate's as `ut` and trails' as `tr`, so those still read as theirs.

use serde_json::{Map, Value};

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

    fn from_tag(g: &str) -> Option<Self> {
        Some(match g {
            "pong" => LinkedGame::Pong,
            "connect4" => LinkedGame::Connect4,
            "ultimate" => LinkedGame::Ultimate,
            "ember" => LinkedGame::Ember,
            "trails" => LinkedGame::Trails,
            _ => return None,
        })
    }
}

/// helloGame: the game a decoded message's hello belongs to, or None when it isn't a hello
pub fn hello_game(o: &Map<String, Value>) -> Option<LinkedGame> {
    let t = o.get("t").and_then(Value::as_str);
    let hi_like = matches!(t, Some("hi" | "c4" | "ut" | "tr"));
    if let Some(g) = o
        .get("g")
        .and_then(Value::as_str)
        .and_then(LinkedGame::from_tag)
        && hi_like
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

/// otherHello: another game's hello, as the name to show; None for this game's own or for anything else
pub fn other_hello(o: &Map<String, Value>, mine: LinkedGame) -> Option<&'static str> {
    hello_game(o).filter(|g| *g != mine).map(LinkedGame::name)
}

pub fn other_game_note(other: &str, mine: LinkedGame) -> String {
    format!("your friend opened {other} · both type {}", mine.name())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(text: &str) -> Map<String, Value> {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn hellos_name_their_game() {
        assert_eq!(
            hello_game(&obj(r#"{"t":"tr","v":1,"g":"trails"}"#)),
            Some(LinkedGame::Trails)
        );
        assert_eq!(
            hello_game(&obj(r#"{"t":"hi","v":2,"w":1}"#)),
            Some(LinkedGame::Ember)
        );
        assert_eq!(
            hello_game(&obj(r#"{"t":"hi","v":2}"#)),
            Some(LinkedGame::Pong)
        );
        assert_eq!(
            hello_game(&obj(r#"{"t":"hi","v":2,"g":"connect4"}"#)),
            Some(LinkedGame::Connect4)
        );
        assert_eq!(hello_game(&obj(r#"{"t":"s"}"#)), None);
        assert_eq!(
            other_hello(&obj(r#"{"t":"c4","v":1}"#), LinkedGame::Trails),
            Some("connect 4")
        );
        assert_eq!(
            other_hello(&obj(r#"{"t":"tr","v":1}"#), LinkedGame::Trails),
            None
        );
        assert_eq!(
            other_game_note("pong", LinkedGame::Ember),
            "your friend opened pong · both type ember"
        );
    }
}
