//! `src/lib/lobby.ts`: the lobby every game opens on, as plain state: its screens, what each key does on them, and
//! what the link's events change. The shell's lobby view draws it and carries out the effects. a game with a
//! computer to play and a friend to play first asks which (1 vs computer, 2 vs a friend); vs computer shows the
//! levels, vs a friend the same-keyboard options, h host, j join with a code, and the open rooms for it over the
//! link (↑↓ pick, ↵ join; a locked room asks for its code first). a game with only one of the two opens on that
//! screen.

use super::Side;
use super::nickname::{NICKNAME_MAX, clean_nickname};
use super::peer::{CloseReason, PeerEvent, RoomInfo, close_reason_text};
use crate::app::js as jstr;

/// The lobby's own screens: the first choice, the game alone on this computer, and playing a friend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Start,
    Solo,
    Friend,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LobbyState {
    /// Rooms are kept on every screen, so the friend screen has them as it opens; pick is the screen's own.
    Home {
        screen: Screen,
        rooms: Vec<RoomInfo>,
        pick: usize,
        note: Option<String>,
        offline: bool,
    },
    /// Hosting: open, or locked with a code.
    Setup {
        locked: bool,
    },
    /// code is "" for an open room, None until the link hands one out.
    Hosting {
        locked: bool,
        code: Option<String>,
        renewed: bool,
    },
    Code {
        room: RoomInfo,
        digits: String,
        note: Option<String>,
    },
    Pairing {
        room: RoomInfo,
    },
    Rename {
        draft: String,
        from: Screen,
    },
    Game {
        role: Side,
        peer: String,
    },
    Local {
        option: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LobbyEffect {
    Host {
        locked: bool,
    },
    Join {
        room: RoomInfo,
        code: String,
    },
    /// Stop hosting or joining; the list watches again.
    Cancel,
    Rename {
        name: String,
    },
    Fullscreen,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LobbyCtx {
    /// Whether there's a link to play over at all.
    pub online: bool,
    /// The relay has no room list: h hosts with a code, j types one.
    pub code_only: bool,
    pub code_length: usize,
    /// The game's options on this computer: alone (levels, modes), then two on one keyboard.
    pub solo: usize,
    pub together: usize,
    /// What the transport calls the other end (`closeText`'s word).
    pub peer_word: String,
}

impl LobbyCtx {
    fn close_text(&self, reason: CloseReason) -> String {
        close_reason_text(reason, &self.peer_word)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LobbyStep {
    pub state: LobbyState,
    pub effect: Option<LobbyEffect>,
    pub handled: bool,
}

/// A room joined by its code alone, when there's no list to pick it from.
pub fn by_code() -> RoomInfo {
    RoomInfo {
        id: String::new(),
        name: "your friend".into(),
        locked: true,
    }
}

pub fn home_screen(screen: Screen, note: Option<String>) -> LobbyState {
    LobbyState::Home {
        screen,
        rooms: vec![],
        pick: 0,
        note: note.filter(|n| !n.is_empty()),
        offline: false,
    }
}

/// The screen the lobby opens on: the first choice only when there's one to make.
pub fn first_screen(online: bool, solo: usize, together: usize) -> Screen {
    let friend = online || together > 0;
    if solo > 0 && friend {
        return Screen::Start;
    }
    if solo > 0 {
        Screen::Solo
    } else {
        Screen::Friend
    }
}

fn first(ctx: &LobbyCtx) -> Screen {
    first_screen(ctx.online, ctx.solo, ctx.together)
}

/// The screen a local option goes back to when it ends.
pub fn option_screen(option: usize, solo: usize) -> Screen {
    if option < solo {
        Screen::Solo
    } else {
        Screen::Friend
    }
}

// the key's name as a lobby reads it: letters lower case, anything longer as it is
fn key_name(key: &str) -> String {
    if jstr::len16(key) == 1 {
        key.to_lowercase()
    } else {
        key.to_string()
    }
}

fn step(state: LobbyState, effect: Option<LobbyEffect>) -> LobbyStep {
    LobbyStep {
        state,
        effect,
        handled: true,
    }
}

// another home screen, keeping the rooms
fn go(rooms: &[RoomInfo], offline: bool, screen: Screen) -> LobbyState {
    LobbyState::Home {
        screen,
        rooms: rooms.to_vec(),
        pick: 0,
        note: None,
        offline,
    }
}

fn ahead(key: &str) -> bool {
    key == "ArrowDown" || key == "ArrowRight"
}
fn behind(key: &str) -> bool {
    key == "ArrowUp" || key == "ArrowLeft"
}

// ↑↓ (and ←→) round n things, ↵ takes the picked one
fn cycle(
    state: &LobbyState,
    key: &str,
    n: usize,
    pick: impl FnOnce(usize) -> LobbyStep,
) -> LobbyStep {
    let LobbyState::Home { pick: at, .. } = state else {
        return step(state.clone(), None);
    };
    let with = |p: usize| {
        let mut s = state.clone();
        if let LobbyState::Home { pick, .. } = &mut s {
            *pick = p;
        }
        step(s, None)
    };
    if n > 0 && ahead(key) {
        return with((at + 1) % n);
    }
    if n > 0 && behind(key) {
        return with((at + n - 1) % n);
    }
    if n > 0 && key == "Enter" {
        return pick((*at).min(n - 1));
    }
    step(state.clone(), None)
}

pub fn join_room(room: &RoomInfo) -> LobbyStep {
    if room.locked {
        return step(
            LobbyState::Code {
                room: room.clone(),
                digits: String::new(),
                note: None,
            },
            None,
        );
    }
    step(
        LobbyState::Pairing { room: room.clone() },
        Some(LobbyEffect::Join {
            room: room.clone(),
            code: String::new(),
        }),
    )
}

// h, j and n, on the first screen and the friend screen
fn friend_key(
    screen: Screen,
    key: &str,
    ctx: &LobbyCtx,
    locked_pref: bool,
    name: &str,
) -> Option<LobbyStep> {
    if !ctx.online {
        return None;
    }
    match key {
        "h" => Some(if ctx.code_only {
            step(
                LobbyState::Hosting {
                    locked: true,
                    code: None,
                    renewed: false,
                },
                Some(LobbyEffect::Host { locked: true }),
            )
        } else {
            step(
                LobbyState::Setup {
                    locked: locked_pref,
                },
                None,
            )
        }),
        "j" => Some(step(
            LobbyState::Code {
                room: by_code(),
                digits: String::new(),
                note: None,
            },
            None,
        )),
        "n" => Some(step(
            LobbyState::Rename {
                draft: name.to_string(),
                from: screen,
            },
            None,
        )),
        _ => None,
    }
}

fn digit_of(key: &str) -> usize {
    match key.as_bytes() {
        [b @ b'1'..=b'9'] => (b - b'0') as usize,
        _ => 0,
    }
}

/// A plain key on the lobby; handled false lets it through (only on the game's own screens). `random_name` is
/// what tab offers on the rename screen (`ctx.randomName()` in the TS).
pub fn lobby_key(
    state: &LobbyState,
    raw_key: &str,
    ctx: &LobbyCtx,
    locked_pref: bool,
    name: &str,
    random_name: &str,
) -> LobbyStep {
    let key = key_name(raw_key);
    let key = key.as_str();
    let same = || step(state.clone(), None);
    match state {
        LobbyState::Home {
            screen,
            rooms,
            offline,
            ..
        } => {
            if key == "f" {
                return step(state.clone(), Some(LobbyEffect::Fullscreen));
            }
            let first = first(ctx);
            if key == "Backspace" {
                return if *screen == first {
                    same()
                } else {
                    step(go(rooms, *offline, first), None)
                };
            }
            let digit = digit_of(key);
            match screen {
                Screen::Start => {
                    let to = |i: usize| {
                        step(
                            go(
                                rooms,
                                *offline,
                                if i > 0 { Screen::Friend } else { Screen::Solo },
                            ),
                            None,
                        )
                    };
                    if digit == 1 || digit == 2 {
                        return to(digit - 1);
                    }
                    friend_key(*screen, key, ctx, locked_pref, name)
                        .unwrap_or_else(|| cycle(state, key, 2, to))
                }
                Screen::Solo => {
                    if digit > 0 && digit <= ctx.solo {
                        return step(LobbyState::Local { option: digit - 1 }, None);
                    }
                    cycle(state, key, ctx.solo, |i| {
                        step(LobbyState::Local { option: i }, None)
                    })
                }
                Screen::Friend => {
                    if digit > 0 && digit <= ctx.together {
                        return step(
                            LobbyState::Local {
                                option: ctx.solo + digit - 1,
                            },
                            None,
                        );
                    }
                    // no link: the arrows and ↵ go through the same-keyboard options
                    if !ctx.online {
                        return cycle(state, key, ctx.together, |i| {
                            step(
                                LobbyState::Local {
                                    option: ctx.solo + i,
                                },
                                None,
                            )
                        });
                    }
                    if let Some(f) = friend_key(*screen, key, ctx, locked_pref, name) {
                        return f;
                    }
                    if ctx.code_only {
                        return same();
                    }
                    cycle(state, key, rooms.len(), |i| join_room(&rooms[i]))
                }
            }
        }
        LobbyState::Setup { locked } => {
            if key == "f" {
                return step(state.clone(), Some(LobbyEffect::Fullscreen));
            }
            if key.starts_with("Arrow") || key == "Tab" {
                return step(LobbyState::Setup { locked: !locked }, None);
            }
            if key == "o" || key == "l" || key == "Enter" {
                let locked = if key == "Enter" { *locked } else { key == "l" };
                return step(
                    LobbyState::Hosting {
                        locked,
                        code: None,
                        renewed: false,
                    },
                    Some(LobbyEffect::Host { locked }),
                );
            }
            if key == "Backspace" {
                return step(home_screen(Screen::Friend, None), None);
            }
            same()
        }
        LobbyState::Hosting { .. } | LobbyState::Pairing { .. } => {
            if key == "f" {
                return step(state.clone(), Some(LobbyEffect::Fullscreen));
            }
            if key == "Backspace" {
                return step(home_screen(Screen::Friend, None), Some(LobbyEffect::Cancel));
            }
            same()
        }
        LobbyState::Code { room, digits, .. } => {
            if key.len() == 1
                && key.as_bytes()[0].is_ascii_digit()
                && digits.len() < ctx.code_length
            {
                let digits = format!("{digits}{key}");
                if digits.len() == ctx.code_length {
                    return step(
                        LobbyState::Pairing { room: room.clone() },
                        Some(LobbyEffect::Join {
                            room: room.clone(),
                            code: digits,
                        }),
                    );
                }
                return step(
                    LobbyState::Code {
                        room: room.clone(),
                        digits,
                        note: None,
                    },
                    None,
                );
            }
            if key == "Backspace" {
                if digits.is_empty() {
                    return step(home_screen(Screen::Friend, None), None);
                }
                let mut d = digits.clone();
                d.pop();
                return step(
                    LobbyState::Code {
                        room: room.clone(),
                        digits: d,
                        note: None,
                    },
                    None,
                );
            }
            if key == "f" {
                return step(state.clone(), Some(LobbyEffect::Fullscreen));
            }
            same()
        }
        LobbyState::Rename { draft, from } => {
            if raw_key == "Enter" {
                let next = clean_nickname(draft);
                return if next.is_empty() {
                    same()
                } else {
                    step(
                        home_screen(*from, None),
                        Some(LobbyEffect::Rename { name: next }),
                    )
                };
            }
            if raw_key == "Tab" {
                return step(
                    LobbyState::Rename {
                        draft: random_name.to_string(),
                        from: *from,
                    },
                    None,
                );
            }
            if raw_key == "Backspace" {
                let mut d: Vec<u16> = draft.encode_utf16().collect();
                d.pop();
                return step(
                    LobbyState::Rename {
                        draft: String::from_utf16_lossy(&d),
                        from: *from,
                    },
                    None,
                );
            }
            if jstr::len16(raw_key) == 1 && jstr::len16(draft) < NICKNAME_MAX {
                let next = format!("{draft}{raw_key}");
                return step(
                    LobbyState::Rename {
                        draft: jstr::trim_start(&next).to_string(),
                        from: *from,
                    },
                    None,
                );
            }
            same()
        }
        LobbyState::Game { .. } | LobbyState::Local { .. } => LobbyStep {
            state: state.clone(),
            effect: None,
            handled: false,
        },
    }
}

/// Esc on a lobby screen: one step back, or None on the screen the lobby opened on, where esc closes the game.
pub fn lobby_back(state: &LobbyState, ctx: &LobbyCtx) -> Option<LobbyStep> {
    match state {
        LobbyState::Home {
            screen,
            rooms,
            offline,
            ..
        } => {
            let first = first(ctx);
            if *screen == first {
                None
            } else {
                Some(step(go(rooms, *offline, first), None))
            }
        }
        LobbyState::Setup { .. } | LobbyState::Code { .. } => {
            Some(step(home_screen(Screen::Friend, None), None))
        }
        LobbyState::Rename { from, .. } => Some(step(home_screen(*from, None), None)),
        LobbyState::Hosting { .. } | LobbyState::Pairing { .. } => Some(step(
            home_screen(Screen::Friend, None),
            Some(LobbyEffect::Cancel),
        )),
        _ => None,
    }
}

/// What the link says, on whichever screen is up.
pub fn lobby_event(state: &LobbyState, e: &PeerEvent, ctx: &LobbyCtx) -> LobbyState {
    match e {
        PeerEvent::Rooms {
            rooms: list,
            offline,
            ..
        } => {
            let LobbyState::Home {
                screen,
                rooms,
                pick,
                note,
                ..
            } = state
            else {
                return state.clone();
            };
            // the picked room stays picked as the list changes; other screens' picks aren't rooms
            if *screen != Screen::Friend {
                return LobbyState::Home {
                    screen: *screen,
                    rooms: list.clone(),
                    pick: *pick,
                    note: note.clone(),
                    offline: *offline,
                };
            }
            let keep = rooms.get(*pick).map(|r| r.id.as_str());
            let found = list
                .iter()
                .position(|r| Some(r.id.as_str()) == keep)
                .unwrap_or(0);
            let pick = found.min(list.len().saturating_sub(1));
            LobbyState::Home {
                screen: *screen,
                rooms: list.clone(),
                pick,
                note: note.clone(),
                offline: *offline,
            }
        }
        PeerEvent::Hosting { code, renewed } => match state {
            LobbyState::Hosting { locked, .. } => LobbyState::Hosting {
                locked: *locked,
                code: Some(code.clone()),
                renewed: *renewed,
            },
            _ => state.clone(),
        },
        PeerEvent::Open { role, peer, .. } => match state {
            LobbyState::Hosting { .. } | LobbyState::Pairing { .. } => LobbyState::Game {
                role: *role,
                peer: if peer.is_empty() {
                    "friend".into()
                } else {
                    peer.clone()
                },
            },
            _ => state.clone(),
        },
        PeerEvent::Closed { reason } => match state {
            LobbyState::Pairing { room } if *reason == CloseReason::WrongCode && room.locked => {
                LobbyState::Code {
                    room: room.clone(),
                    digits: String::new(),
                    note: Some(ctx.close_text(CloseReason::WrongCode)),
                }
            }
            // a game reports its own end
            LobbyState::Hosting { .. } | LobbyState::Pairing { .. } => {
                home_screen(Screen::Friend, Some(ctx.close_text(*reason)))
            }
            _ => state.clone(),
        },
        _ => state.clone(),
    }
}

/// Whether the link must stay up while the window is hidden: someone is waiting to join, or playing.
pub fn lobby_holds_link(state: &LobbyState) -> bool {
    matches!(
        state,
        LobbyState::Hosting { .. } | LobbyState::Pairing { .. } | LobbyState::Game { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> LobbyCtx {
        LobbyCtx {
            online: true,
            code_only: false,
            code_length: 6,
            solo: 3,
            together: 0,
            peer_word: "computer".into(),
        }
    }
    fn open() -> RoomInfo {
        RoomInfo {
            id: "a".into(),
            name: "teal otter".into(),
            locked: false,
        }
    }
    fn locked() -> RoomInfo {
        RoomInfo {
            id: "b".into(),
            name: "amber fox".into(),
            locked: true,
        }
    }
    fn home(screen: Screen) -> LobbyState {
        home_screen(screen, None)
    }
    fn with_rooms(pick: usize) -> LobbyState {
        LobbyState::Home {
            screen: Screen::Friend,
            rooms: vec![open(), locked()],
            pick,
            note: None,
            offline: false,
        }
    }
    fn pick_of(s: &LobbyState) -> usize {
        match s {
            LobbyState::Home { pick, .. } => *pick,
            _ => usize::MAX,
        }
    }

    fn press_with(
        mut state: LobbyState,
        keys: &[&str],
        c: &LobbyCtx,
        pref: bool,
    ) -> (LobbyState, Vec<LobbyEffect>) {
        let mut effects = vec![];
        for k in keys {
            let s = lobby_key(&state, k, c, pref, "teal otter", "plum newt");
            state = s.state;
            effects.extend(s.effect);
        }
        (state, effects)
    }
    fn press(state: LobbyState, keys: &[&str]) -> (LobbyState, Vec<LobbyEffect>) {
        press_with(state, keys, &ctx(), false)
    }

    #[test]
    fn first_screen_is_the_choice_only_with_both() {
        assert_eq!(first_screen(true, 3, 0), Screen::Start);
        assert_eq!(first_screen(false, 3, 1), Screen::Start);
        assert_eq!(first_screen(true, 0, 1), Screen::Friend);
        assert_eq!(first_screen(false, 3, 0), Screen::Solo);
    }

    #[test]
    fn the_choice() {
        let start = home(Screen::Start);
        assert_eq!(press(start.clone(), &["1"]).0, home(Screen::Solo));
        assert_eq!(press(start.clone(), &["2"]).0, home(Screen::Friend));
        assert_eq!(press(start.clone(), &["3"]).0, start);
        assert_eq!(press(start.clone(), &["Enter"]).0, home(Screen::Solo));
        assert_eq!(
            press(start.clone(), &["ArrowRight", "Enter"]).0,
            home(Screen::Friend)
        );
        assert_eq!(
            press(start.clone(), &["ArrowDown", "ArrowDown", "Enter"]).0,
            home(Screen::Solo)
        );
        assert_eq!(pick_of(&press(start.clone(), &["ArrowLeft"]).0), 1);
        assert_eq!(
            press(start.clone(), &["h"]).0,
            LobbyState::Setup { locked: false }
        );
        assert_eq!(
            press_with(
                start.clone(),
                &["h"],
                &LobbyCtx {
                    code_only: true,
                    ..ctx()
                },
                false
            )
            .1,
            vec![LobbyEffect::Host { locked: true }]
        );
        assert_eq!(
            press(start.clone(), &["j"]).0,
            LobbyState::Code {
                room: by_code(),
                digits: "".into(),
                note: None
            }
        );
        assert_eq!(
            press(start.clone(), &["n"]).0,
            LobbyState::Rename {
                draft: "teal otter".into(),
                from: Screen::Start
            }
        );
        assert_eq!(
            press(start.clone(), &["n", "Enter"]),
            (
                start.clone(),
                vec![LobbyEffect::Rename {
                    name: "teal otter".into()
                }]
            )
        );
        assert_eq!(
            press(start.clone(), &["f"]),
            (start.clone(), vec![LobbyEffect::Fullscreen])
        );
        let c = LobbyCtx {
            online: false,
            together: 1,
            ..ctx()
        };
        assert_eq!(
            press_with(start.clone(), &["h", "j", "n"], &c, false).0,
            start
        );
        let heard = lobby_event(
            &start,
            &PeerEvent::Rooms {
                rooms: vec![open(), locked()],
                offline: false,
                code_only: false,
            },
            &ctx(),
        );
        assert!(
            matches!(&heard, LobbyState::Home { screen: Screen::Start, pick: 0, rooms, .. } if rooms.len() == 2)
        );
        assert_eq!(press(heard, &["2"]).0, with_rooms(0));
    }

    #[test]
    fn vs_computer() {
        let solo = home(Screen::Solo);
        assert_eq!(
            press(solo.clone(), &["2"]).0,
            LobbyState::Local { option: 1 }
        );
        assert_eq!(press(solo.clone(), &["4"]).0, solo);
        assert_eq!(
            press(solo.clone(), &["ArrowDown", "ArrowRight", "Enter"]).0,
            LobbyState::Local { option: 2 }
        );
        assert_eq!(
            press(solo.clone(), &["ArrowUp", "Enter"]).0,
            LobbyState::Local { option: 2 }
        );
        assert_eq!(press(solo.clone(), &["Backspace"]).0, home(Screen::Start));
        assert_eq!(press(solo.clone(), &["h", "n"]).0, solo);
        let c = LobbyCtx {
            online: false,
            solo: 2,
            ..ctx()
        };
        assert_eq!(
            press_with(solo.clone(), &["ArrowDown", "Enter"], &c, false).0,
            LobbyState::Local { option: 1 }
        );
        assert_eq!(press_with(solo.clone(), &["Backspace"], &c, false).0, solo);
    }

    #[test]
    fn vs_a_friend() {
        let friend = home(Screen::Friend);
        assert_eq!(
            press(with_rooms(0), &["Enter"]),
            (
                LobbyState::Pairing { room: open() },
                vec![LobbyEffect::Join {
                    room: open(),
                    code: "".into()
                }]
            )
        );
        assert_eq!(
            press(
                with_rooms(0),
                &[
                    "ArrowDown",
                    "Enter",
                    "1",
                    "2",
                    "3",
                    "Backspace",
                    "3",
                    "4",
                    "5",
                    "6"
                ]
            ),
            (
                LobbyState::Pairing { room: locked() },
                vec![LobbyEffect::Join {
                    room: locked(),
                    code: "123456".into()
                }]
            )
        );
        assert_eq!(
            press(with_rooms(0), &["j", "0", "4", "2", "0", "1", "7"]),
            (
                LobbyState::Pairing { room: by_code() },
                vec![LobbyEffect::Join {
                    room: by_code(),
                    code: "042017".into()
                }]
            )
        );
        assert_eq!(pick_of(&press(with_rooms(0), &["ArrowUp"]).0), 1);
        assert_eq!(pick_of(&press(with_rooms(1), &["ArrowDown"]).0), 0);
        assert_eq!(
            press(
                LobbyState::Code {
                    room: locked(),
                    digits: "".into(),
                    note: None
                },
                &["Backspace"]
            )
            .0,
            friend
        );
        assert_eq!(
            press(friend.clone(), &["h"]).0,
            LobbyState::Setup { locked: false }
        );
        assert_eq!(
            press_with(friend.clone(), &["h"], &ctx(), true).0,
            LobbyState::Setup { locked: true }
        );
        assert_eq!(
            press(friend.clone(), &["h", "Enter"]).1,
            vec![LobbyEffect::Host { locked: false }]
        );
        assert_eq!(
            press(friend.clone(), &["h", "ArrowRight", "Enter"]).1,
            vec![LobbyEffect::Host { locked: true }]
        );
        assert_eq!(
            press(friend.clone(), &["h", "l"]).0,
            LobbyState::Hosting {
                locked: true,
                code: None,
                renewed: false
            }
        );
        assert_eq!(
            press(friend.clone(), &["h", "o"]).1,
            vec![LobbyEffect::Host { locked: false }]
        );
        assert_eq!(press(friend.clone(), &["h", "Backspace"]).0, friend);
        for s in [
            LobbyState::Hosting {
                locked: true,
                code: Some("123456".into()),
                renewed: false,
            },
            LobbyState::Pairing { room: open() },
        ] {
            assert_eq!(
                press(s, &["Backspace"]),
                (friend.clone(), vec![LobbyEffect::Cancel])
            );
        }
        assert_eq!(
            press(with_rooms(1), &["Backspace"]).0,
            LobbyState::Home {
                screen: Screen::Start,
                rooms: vec![open(), locked()],
                pick: 0,
                note: None,
                offline: false
            }
        );
        let renamed = press(
            friend.clone(),
            &[
                "n",
                "Backspace",
                "Backspace",
                "Backspace",
                "Backspace",
                "Backspace",
                "s",
                "e",
                "a",
                "l",
                "Enter",
            ],
        );
        assert_eq!(
            renamed,
            (
                friend.clone(),
                vec![LobbyEffect::Rename {
                    name: "teal seal".into()
                }]
            )
        );
        assert_eq!(
            press(friend.clone(), &["n", "Tab", "Enter"]).1,
            vec![LobbyEffect::Rename {
                name: "plum newt".into()
            }]
        );
        let blank = LobbyState::Rename {
            draft: "  ".into(),
            from: Screen::Friend,
        };
        assert_eq!(press(blank.clone(), &["Enter"]).0, blank);
        let c = LobbyCtx {
            together: 1,
            ..ctx()
        };
        assert_eq!(
            press_with(with_rooms(0), &["1"], &c, false).0,
            LobbyState::Local { option: 3 }
        );
        assert_eq!(
            press_with(with_rooms(0), &["2"], &c, false).0,
            with_rooms(0)
        );
        assert_eq!(
            press_with(with_rooms(0), &["ArrowDown", "Enter"], &c, false).0,
            LobbyState::Code {
                room: locked(),
                digits: "".into(),
                note: None
            }
        );
        let c = LobbyCtx {
            solo: 0,
            together: 1,
            ..ctx()
        };
        assert_eq!(
            press_with(friend.clone(), &["1"], &c, false).0,
            LobbyState::Local { option: 0 }
        );
        assert_eq!(
            press_with(friend.clone(), &["Backspace"], &c, false).0,
            friend
        );
        assert_eq!(lobby_back(&friend, &c), None);
        let c = LobbyCtx {
            online: false,
            together: 2,
            ..ctx()
        };
        assert_eq!(
            press_with(friend.clone(), &["ArrowDown", "Enter"], &c, false).0,
            LobbyState::Local { option: 4 }
        );
        assert_eq!(
            press_with(friend.clone(), &["h", "j", "n"], &c, false).0,
            friend
        );
    }

    #[test]
    fn code_only_relays() {
        let old = LobbyCtx {
            code_only: true,
            ..ctx()
        };
        let friend = home(Screen::Friend);
        assert_eq!(
            press_with(friend.clone(), &["h"], &old, false).1,
            vec![LobbyEffect::Host { locked: true }]
        );
        assert_eq!(
            press_with(
                friend.clone(),
                &["j", "0", "4", "2", "0", "1", "7"],
                &old,
                false
            ),
            (
                LobbyState::Pairing { room: by_code() },
                vec![LobbyEffect::Join {
                    room: by_code(),
                    code: "042017".into()
                }]
            )
        );
        assert_eq!(
            press_with(friend.clone(), &["Enter", "ArrowDown"], &old, false).0,
            friend
        );
    }

    #[test]
    fn events() {
        let z = RoomInfo {
            id: "z".into(),
            name: "z".into(),
            locked: false,
        };
        let rooms = |rooms, offline| PeerEvent::Rooms {
            rooms,
            offline,
            code_only: false,
        };
        assert_eq!(
            pick_of(&lobby_event(
                &with_rooms(1),
                &rooms(vec![z, locked()], false),
                &ctx()
            )),
            1
        );
        let empty = lobby_event(&with_rooms(1), &rooms(vec![], false), &ctx());
        assert!(matches!(empty, LobbyState::Home { pick: 0, ref rooms, .. } if rooms.is_empty()));
        assert!(matches!(
            lobby_event(&with_rooms(0), &rooms(vec![], true), &ctx()),
            LobbyState::Home { offline: true, .. }
        ));
        let hosting = LobbyState::Hosting {
            locked: true,
            code: None,
            renewed: false,
        };
        assert_eq!(
            lobby_event(
                &hosting,
                &PeerEvent::Hosting {
                    code: "042017".into(),
                    renewed: false
                },
                &ctx()
            ),
            LobbyState::Hosting {
                locked: true,
                code: Some("042017".into()),
                renewed: false
            }
        );
        let opened = PeerEvent::Open {
            role: Side::Host,
            peer: "jade owl".into(),
            seat: None,
            resumed: false,
        };
        assert_eq!(
            lobby_event(&hosting, &opened, &ctx()),
            LobbyState::Game {
                role: Side::Host,
                peer: "jade owl".into()
            }
        );
        assert_eq!(
            lobby_event(
                &LobbyState::Pairing { room: locked() },
                &PeerEvent::Closed {
                    reason: CloseReason::WrongCode
                },
                &ctx()
            ),
            LobbyState::Code {
                room: locked(),
                digits: "".into(),
                note: Some(close_reason_text(CloseReason::WrongCode, "computer"))
            }
        );
        assert_eq!(
            lobby_event(
                &LobbyState::Pairing { room: open() },
                &PeerEvent::Closed {
                    reason: CloseReason::Gone
                },
                &ctx()
            ),
            home_screen(
                Screen::Friend,
                Some(close_reason_text(CloseReason::Gone, "computer"))
            )
        );
        let game = LobbyState::Game {
            role: Side::Guest,
            peer: "x".into(),
        };
        assert_eq!(
            lobby_event(
                &game,
                &PeerEvent::Closed {
                    reason: CloseReason::Bye
                },
                &ctx()
            ),
            game
        );
    }

    #[test]
    fn back_and_the_link() {
        let start = home(Screen::Start);
        let friend = home(Screen::Friend);
        assert_eq!(lobby_back(&start, &ctx()), None);
        assert_eq!(
            lobby_back(&home(Screen::Solo), &ctx()).map(|s| s.state),
            Some(start.clone())
        );
        assert_eq!(
            lobby_back(&friend, &ctx()).map(|s| s.state),
            Some(start.clone())
        );
        assert_eq!(
            lobby_back(
                &home(Screen::Solo),
                &LobbyCtx {
                    online: false,
                    ..ctx()
                }
            ),
            None
        );
        assert_eq!(
            lobby_back(&LobbyState::Setup { locked: true }, &ctx()).map(|s| s.state),
            Some(friend.clone())
        );
        assert_eq!(
            lobby_back(
                &LobbyState::Code {
                    room: by_code(),
                    digits: "12".into(),
                    note: None
                },
                &ctx()
            )
            .map(|s| s.state),
            Some(friend.clone())
        );
        assert_eq!(
            lobby_back(
                &LobbyState::Rename {
                    draft: "x".into(),
                    from: Screen::Start
                },
                &ctx()
            )
            .map(|s| s.state),
            Some(start.clone())
        );
        assert_eq!(
            lobby_back(
                &LobbyState::Hosting {
                    locked: false,
                    code: Some("".into()),
                    renewed: false
                },
                &ctx()
            ),
            Some(LobbyStep {
                state: friend.clone(),
                effect: Some(LobbyEffect::Cancel),
                handled: true
            })
        );
        assert!(lobby_holds_link(&LobbyState::Hosting {
            locked: false,
            code: Some("".into()),
            renewed: false
        }));
        assert!(lobby_holds_link(&LobbyState::Game {
            role: Side::Host,
            peer: "".into()
        }));
        assert!(!lobby_holds_link(&start));
        assert!(!lobby_holds_link(&LobbyState::Local { option: 0 }));
    }
}
