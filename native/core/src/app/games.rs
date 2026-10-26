//! Which bar lines open a game (`src/lib/games.ts` and each game's `is…Command`/hint). The games themselves stay
//! web (ARCHITECTURE.md §7); only what the bar reads is here. Also `periodic.ts`'s command, which opens the table.

use serde::{Deserialize, Serialize};

use crate::app::js as jstr;
use crate::jsre::{self, Lazy, js};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GameKind {
    Pong,
    Connect4,
    Ember,
    Crack,
    Equal,
    Numbers,
    Trails,
    Ultimate,
}

/// A row of the `games` list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Game {
    pub kind: GameKind,
    pub name: &'static str,
    pub blurb: &'static str,
    /// The line under the bar while its name is typed.
    pub hint: &'static str,
}

/// What `games` lists, in order.
pub const GAMES: [Game; 8] = [
    Game {
        kind: GameKind::Pong,
        name: "pong",
        blurb: "first to 7 · vs the computer or anyone online",
        hint: "↵ play pong",
    },
    Game {
        kind: GameKind::Connect4,
        name: "connect 4",
        blurb: "four in a row · vs the computer or anyone online",
        hint: "↵ play connect 4",
    },
    Game {
        kind: GameKind::Ultimate,
        name: "ultimate tic-tac-toe",
        blurb: "nine boards in one · vs the computer or anyone online",
        hint: "↵ play ultimate tic-tac-toe",
    },
    Game {
        kind: GameKind::Ember,
        name: "ember & frost",
        blurb: "co-op puzzle platformer · one keyboard or online",
        hint: "↵ play ember & frost",
    },
    Game {
        kind: GameKind::Crack,
        name: "crack",
        blurb: "crack the safe · tap when the dial hits the dot",
        hint: "↵ crack the safe",
    },
    Game {
        kind: GameKind::Equal,
        name: "equal",
        blurb: "guess the hidden equation · daily or practice",
        hint: "↵ play equal",
    },
    Game {
        kind: GameKind::Numbers,
        name: "24 / countdown",
        blurb: "make the number · two modes against the clock",
        hint: "↵ play 24 or countdown",
    },
    Game {
        kind: GameKind::Trails,
        name: "trails",
        blurb: "leave a trail, don’t hit one · solo, one keyboard or online",
        hint: "↵ play trails",
    },
];

fn squashed(text: &str) -> String {
    static SPACES: Lazy = Lazy::new(|| js(r"\s+", ""));
    jsre::replace_all(&SPACES, jstr::trim(text), |_| " ".into()).into_owned()
}

fn is(kind: GameKind, text: &str) -> bool {
    static PONG: Lazy = Lazy::new(|| js(r"^pong$", "i"));
    static CONNECT4: Lazy = Lazy::new(|| js(r"^connect[\s-]*(4|four)$", "i"));
    static ULTIMATE: Lazy = Lazy::new(|| {
        js(
            r"^(ultimate([\s-]*(tic[\s-]*tac[\s-]*toe|ttt))?|uttt)$",
            "i",
        )
    });
    static EMBER: Lazy = Lazy::new(|| {
        let and = r"(?: ?[&+] ?| and )";
        js(
            &format!(r"^(?:ember(?:{and}frost)?|frost{and}ember|fire(?:{and}| )ice)$"),
            "i",
        )
    });
    static CRACK: Lazy = Lazy::new(|| js(r"^(?:crack(?: the safe)?|safe ?crack(?:er|ing)?)$", "i"));
    static EQUAL: Lazy = Lazy::new(|| js(r"^(?:equal|nerdle)$", "i"));
    static NUMBERS: Lazy = Lazy::new(|| {
        js(
            r"^(?:24 ?game|game of 24|count ?down|numbers game|24 ?\/ ?countdown)$",
            "i",
        )
    });
    static TRAILS: Lazy = Lazy::new(|| js(r"^(?:trails?|cycles)$", "i"));
    let t = jstr::trim(text);
    match kind {
        GameKind::Pong => jsre::test(&PONG, t),
        GameKind::Connect4 => jsre::test(&CONNECT4, t),
        GameKind::Ultimate => jsre::test(&ULTIMATE, t),
        GameKind::Ember => jsre::test(&EMBER, &squashed(text)),
        GameKind::Crack => jsre::test(&CRACK, &squashed(text)),
        GameKind::Equal => jsre::test(&EQUAL, t),
        GameKind::Numbers => jsre::test(&NUMBERS, &squashed(text)),
        GameKind::Trails => jsre::test(&TRAILS, t),
    }
}

/// The game a typed line opens.
pub fn game_command(text: &str) -> Option<GameKind> {
    GAMES.iter().map(|g| g.kind).find(|&k| is(k, text))
}

pub const GAMES_HINT: &str = "↑↓ pick · ↵ play · or type a name";

/// `games` lists them above the bar, the way `?` lists the shortcuts.
pub fn is_games_command(text: &str) -> bool {
    let t = jstr::trim(text).to_lowercase();
    t == "games" || t == "game"
}

/// The hint under the bar for a game's name.
pub fn game_hint(kind: GameKind) -> &'static str {
    GAMES.iter().find(|g| g.kind == kind).map_or("", |g| g.hint)
}

pub const PERIODIC_HINT: &str = "periodic table";

/// `periodic` or `periodic table` (`src/lib/periodic.ts`).
pub fn is_periodic_command(text: &str) -> bool {
    static RE: Lazy = Lazy::new(|| js(r"^periodic(\s+table)?$", "i"));
    jsre::test(&RE, jstr::trim(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands() {
        assert_eq!(game_command(" Connect-4 "), Some(GameKind::Connect4));
        assert_eq!(game_command("fire  and ice"), Some(GameKind::Ember));
        assert_eq!(game_command("24"), None);
        assert!(is_periodic_command("Periodic Table"));
    }
}
