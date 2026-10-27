//! The lobby every game opens on (`GameLobby.tsx`, the Mac's GameLobby.swift; its rules are the core's `lobby`).
//! First vs computer or vs a friend; vs computer is the game's own options, 1, 2, 3…; vs a friend is host, join
//! with a code and the open rooms for this game, by the host's nickname, 🔒 when they take a code. Once a game
//! starts it takes the panel (`play`) and talks to the link itself; when it ends it hands back a line to show.

use qcalc_core::games::lobby::{
    LobbyCtx, LobbyEffect, LobbyState, Screen, first_screen, home_screen, join_room,
    lobby_back, lobby_event, lobby_key, option_screen,
};
use qcalc_core::games::nickname::random_nickname;
use qcalc_core::games::peer::{CloseReason, PeerEvent, RoomInfo, Who, close_reason_text};
use qcalc_core::games::peer_online::{ONLINE_CODE_LENGTH, OnlinePeer};
use qcalc_core::games::shell::ShellMode;

use super::gfx::{Align, Font, Hit, Rect, Ui};
use super::keys::GameKey;
use super::theme::m;
use super::widgets::{self as w, ButtonSpec, Run};
use super::{Env, GameDef, Play};

const LOCKED_KEY: &str = "qcalc-host-locked";

pub struct Lobby {
    pub def: &'static GameDef,
    pub state: LobbyState,
    /// the relay sent no room list, so rooms go by code
    code_only: bool,
    /// mid-game, whose link dropped: the game stands still until they're back
    away: Option<Who>,
    watching: bool,
    /// the game that's on, on this computer or over the link
    pub play: Option<Box<dyn Play>>,
}

impl Lobby {
    pub fn new(def: &'static GameDef) -> Lobby {
        let first = first_screen(def.online.is_some(), def.options.len(), def.together.len());
        Lobby {
            def,
            state: home_screen(first, None),
            code_only: false,
            away: None,
            watching: false,
            play: None,
        }
    }

    fn online(&self) -> bool {
        self.def.online.is_some()
    }

    pub fn ctx(&self) -> LobbyCtx {
        LobbyCtx {
            online: self.online(),
            code_only: self.code_only,
            code_length: if self.online() { ONLINE_CODE_LENGTH } else { 0 },
            solo: self.def.options.len(),
            together: self.def.together.len(),
            peer_word: OnlinePeer::PEER_WORD.into(),
        }
    }

    fn first(&self) -> Screen {
        first_screen(self.online(), self.def.options.len(), self.def.together.len())
    }

    /// Hooks the lobby to the shell and the link once the window is up.
    pub fn attach(&mut self, env: &mut Env) {
        let s = self.state.clone();
        self.set(s, env);
    }

    pub fn set(&mut self, next: LobbyState, env: &mut Env) {
        let was = std::mem::replace(&mut self.state, next);
        let same_game = matches!((&was, &self.state), (LobbyState::Local { option: a }, LobbyState::Local { option: b }) if a == b)
            || matches!((&was, &self.state), (LobbyState::Game { .. }, LobbyState::Game { .. }));
        if !same_game {
            if let Some(mut p) = self.play.take() {
                p.closing(env);
            }
            match &self.state {
                LobbyState::Local { option } => self.play = Some((self.def.local)(*option, env)),
                LobbyState::Game { role, peer } => {
                    if let Some(make) = self.def.online {
                        self.play = Some(make(*role, peer.clone(), env));
                    }
                }
                _ => {}
            }
        }
        // where the player is, for esc: the lobby, a game on this computer, or one over the link
        env.set_mode(match self.state {
            LobbyState::Game { .. } => ShellMode::Online,
            LobbyState::Local { .. } => ShellMode::Local,
            _ => ShellMode::Lobby,
        });
        // the list is watched while the lobby's own screens show, so the friend screen has it as it opens
        let watch = matches!(self.state, LobbyState::Home { .. }) && self.online();
        if watch != self.watching {
            self.watching = watch;
            let now = env.wall();
            if let Some(r) = env.relay_mut() {
                if watch {
                    r.peer.watch(self.def.kind, now);
                } else {
                    r.peer.unwatch();
                }
            }
        }
    }

    fn run(&mut self, effect: Option<LobbyEffect>, env: &mut Env) {
        let Some(effect) = effect else { return };
        let now = env.wall();
        match effect {
            LobbyEffect::Host { locked } => {
                env.store.set(LOCKED_KEY, Some(if locked { "1" } else { "0" }));
                let name = env.nickname();
                if let Some(r) = env.relay_mut() {
                    r.peer.host(self.def.kind, &name, locked, now);
                }
            }
            LobbyEffect::Join { room, code } => {
                let name = env.nickname();
                if let Some(r) = env.relay_mut() {
                    r.peer.join(&room.id, &code, &name, now);
                }
            }
            LobbyEffect::Cancel => {
                if let Some(r) = env.relay_mut() {
                    r.peer.close();
                }
            }
            LobbyEffect::Rename { name } => {
                let Env { store, rand, .. } = env;
                store.rename(&name, &mut || rand.next_f64());
            }
            LobbyEffect::Fullscreen => env.toggle_fullscreen = true,
        }
    }

    /// Esc on a lobby screen: one step back; false on its first screen, where esc closes the game.
    pub fn back(&mut self, env: &mut Env) -> bool {
        let Some(step) = lobby_back(&self.state, &self.ctx()) else {
            return false;
        };
        self.set(step.state, env);
        self.run(step.effect, env);
        true
    }

    /// Esc's leave from a running game: back to where it was picked.
    pub fn leave(&mut self, env: &mut Env) {
        if let Some(p) = &mut self.play
            && p.leave(env)
        {
            return;
        }
        if let LobbyState::Local { option } = self.state {
            let screen = option_screen(option, self.def.options.len());
            self.set(home_screen(screen, None), env);
        } else {
            self.back_to_friend(None, env);
        }
    }

    /// Nothing is running, so esc leaves without a pause.
    pub fn idle(&self) -> bool {
        self.play.as_ref().is_some_and(|p| p.idle())
    }

    /// The game asked to end, with a line for the lobby.
    pub fn ended(&mut self, note: Option<String>, env: &mut Env) {
        match self.state {
            LobbyState::Local { option } => {
                let screen = option_screen(option, self.def.options.len());
                self.set(home_screen(screen, note), env);
            }
            LobbyState::Game { .. } => self.back_to_friend(note, env),
            _ => {}
        }
    }

    fn back_to_friend(&mut self, note: Option<String>, env: &mut Env) {
        if let Some(r) = env.relay_mut() {
            r.peer.close();
        }
        self.set(home_screen(Screen::Friend, note), env);
    }

    fn locked_pref(env: &Env) -> bool {
        env.store.get(LOCKED_KEY) == Some("1")
    }

    /// A key's worth, from a click on a button that stands for it.
    pub fn click_key(&mut self, key: &str, env: &mut Env) {
        let name = env.nickname();
        let random = random_nickname(&mut || env.rand.next_f64());
        let step = lobby_key(&self.state, key, &self.ctx(), Self::locked_pref(env), &name, &random);
        self.set(step.state, env);
        self.run(step.effect, env);
    }

    pub fn join(&mut self, i: usize, env: &mut Env) {
        let LobbyState::Home { rooms, .. } = &self.state else {
            return;
        };
        let Some(room) = rooms.get(i).cloned() else {
            return;
        };
        let step = join_room(&room);
        self.set(step.state, env);
        self.run(step.effect, env);
    }

    pub fn local(&mut self, option: usize, env: &mut Env) {
        if matches!(self.state, LobbyState::Home { .. }) {
            self.set(LobbyState::Local { option }, env);
        }
    }

    /// The link said something: the lobby moves on, and the game hears it.
    pub fn heard(&mut self, e: &PeerEvent, env: &mut Env) {
        match e {
            PeerEvent::Rooms { code_only, .. } => self.code_only = *code_only,
            PeerEvent::Away { who } => {
                env.clock_pause();
                self.away = Some(*who);
            }
            PeerEvent::Back { .. } | PeerEvent::Closed { .. } => {
                env.clock_resume();
                self.away = None;
            }
            _ => {}
        }
        let next = lobby_event(&self.state, e, &self.ctx());
        if next != self.state {
            self.set(next, env);
        }
        if let Some(p) = &mut self.play
            && matches!(self.state, LobbyState::Game { .. })
        {
            p.heard(e, env);
        }
    }

    /// Keys on the lobby's own screens (they would otherwise reach nothing); a running game's go to it.
    pub fn key(&mut self, k: &GameKey, env: &mut Env) -> bool {
        if matches!(self.state, LobbyState::Game { .. } | LobbyState::Local { .. }) {
            return self.play.as_mut().is_some_and(|p| p.key(k, env));
        }
        if !k.down || !k.plain() || k.key == "Escape" {
            return false;
        }
        if matches!(self.state, LobbyState::Home { .. })
            && let Some(f) = self.def.lobby_key
            && f(k, env)
        {
            return true;
        }
        let name = env.nickname();
        let random = random_nickname(&mut || env.rand.next_f64());
        let step = lobby_key(&self.state, &k.key, &self.ctx(), Self::locked_pref(env), &name, &random);
        if k.repeat && step.effect.is_some() {
            return true;
        }
        if step.state != self.state {
            self.set(step.state, env);
        }
        self.run(step.effect, env);
        true
    }

    // ---- drawing

    /// The lobby's screens, or the game it started; returns the height drawn from y.
    pub fn paint(&mut self, ui: &mut Ui, x: f32, y: f32, width: f32, env: &mut Env) -> f32 {
        match self.state.clone() {
            LobbyState::Local { .. } => match &mut self.play {
                Some(p) => p.paint(ui, x, y, width, env),
                None => 0.0,
            },
            LobbyState::Game { peer, .. } => {
                let mut h = 0.0;
                if !self.def.own_head {
                    h += w::head(ui, x, y, width, self.def.name, &format!("with {peer}"), env.shell.fullscreen);
                }
                if let Some(p) = &mut self.play {
                    h += p.paint(ui, x, y + h, width, env);
                }
                if let Some(away) = self.away {
                    let text = if away == Who::Peer {
                        format!("{peer} stepped away · the game waits for them")
                    } else {
                        "reconnecting to your game…".to_string()
                    };
                    w::toast(ui, x + width / 2.0, y + m::SP3, &text);
                }
                h
            }
            state => {
                let sub = if self.online() {
                    OnlinePeer::LABEL.to_string()
                } else {
                    (self.def.sub)(&env.store)
                };
                let hh = w::head(ui, x, y, width, self.def.name, &sub, env.shell.fullscreen);
                hh + self.body(ui, &state, x, y + hh, width, env)
            }
        }
    }

    /// `.pong-body`: a centred column 12 apart, at least 168 high.
    fn body(&mut self, ui: &mut Ui, state: &LobbyState, x: f32, y: f32, width: f32, env: &mut Env) -> f32 {
        let cx = x + width / 2.0;
        let mut col = Col {
            y: y + m::SP3,
            first: true,
        };
        let def = self.def;
        let nick = env.nickname();
        let fs_label = format!(
            "f {}",
            qcalc_core::games::fullscreen::fullscreen_label(env.shell.fullscreen, true)
        );
        let n = if self.online() { ONLINE_CODE_LENGTH } else { 0 };
        match state {
            LobbyState::Home {
                screen,
                rooms,
                pick,
                note,
                offline,
            } => {
                let back_hint = *screen != self.first();
                let back_runs = |v: &mut Vec<Run>| {
                    if back_hint {
                        v.push(Run::link("⌫ back", Hit::Key("Backspace".into())));
                        v.push(Run::plain(" · "));
                    }
                };
                let you = |v: &mut Vec<Run>, nick: &str| {
                    v.push(Run::plain("you’re "));
                    v.push(Run::ink_link(leak(nick), Hit::Key("n".into())));
                    v.push(Run::plain(" · n renames"));
                };
                col.add(ui, |ui, y| w::line(ui, cx, y, width, def.blurb));
                match screen {
                    Screen::Start => {
                        let items = ["vs computer", "vs a friend"]
                            .iter()
                            .enumerate()
                            .map(|(i, t)| ButtonSpec {
                                title: t,
                                kbd: Some((i + 1).to_string()),
                                on: i == *pick,
                                hit: Hit::Key((i + 1).to_string()),
                            })
                            .collect::<Vec<_>>();
                        col.add(ui, |ui, y| w::button_row(ui, cx, y, width, &items, true));
                        if self.online() {
                            let mut v = Vec::new();
                            you(&mut v, &nick);
                            col.add(ui, |ui, y| note_runs(ui, cx, y, &v));
                        } else {
                            col.add(ui, |ui, y| w::note(ui, cx, y, width, "←→ pick · ↵ go", false));
                        }
                    }
                    Screen::Solo => {
                        let items = def
                            .options
                            .iter()
                            .enumerate()
                            .map(|(i, t)| ButtonSpec {
                                title: t,
                                kbd: Some((i + 1).to_string()),
                                on: i == *pick,
                                hit: Hit::Local(i),
                            })
                            .collect::<Vec<_>>();
                        col.add(ui, |ui, y| {
                            let mut h = 0.0;
                            if !def.prompt.is_empty() {
                                let c = ui.c.muted;
                                h += ui.text_wrap(def.prompt, Font::CAPTION, x, y, width, Align::Center, c) + m::SP2;
                            }
                            h + w::button_row(ui, cx, y + h, width, &items, false)
                        });
                        if let Some(footer) = def.footer {
                            col.add(ui, |ui, y| footer(ui, x, y, width, env));
                        }
                        let mut v = vec![Run::plain(if def.options.len() > 1 { "↑↓ pick · ↵ play · " } else { "↵ plays · " })];
                        back_runs(&mut v);
                        v.push(Run::plain(&fs_label));
                        col.add(ui, |ui, y| note_runs(ui, cx, y, &v));
                    }
                    Screen::Friend => {
                        let solo = def.options.len();
                        let mut items = def
                            .together
                            .iter()
                            .enumerate()
                            .map(|(i, t)| ButtonSpec {
                                title: t,
                                kbd: Some((i + 1).to_string()),
                                on: !self.online() && i == *pick,
                                hit: Hit::Local(solo + i),
                            })
                            .collect::<Vec<_>>();
                        if self.online() {
                            items.push(ButtonSpec {
                                title: "host",
                                kbd: Some("H".into()),
                                on: false,
                                hit: Hit::Key("h".into()),
                            });
                            items.push(ButtonSpec {
                                title: "join with a code",
                                kbd: Some("J".into()),
                                on: false,
                                hit: Hit::Key("j".into()),
                            });
                        }
                        col.add(ui, |ui, y| w::button_row(ui, cx, y, width, &items, false));
                        if self.online() {
                            if self.code_only {
                                let mut v = vec![Run::plain("no room list from the game server yet, so rooms go by code · ")];
                                back_runs(&mut v);
                                you(&mut v, &nick);
                                col.add(ui, |ui, y| note_runs(ui, cx, y, &v));
                            } else {
                                col.add(ui, |ui, y| {
                                    let muted = ui.c.muted;
                                    let mut h = ui.text_wrap("rooms open now", Font::CAPTION, x, y, width, Align::Center, muted) + m::SP2;
                                    if *offline {
                                        let t = close_reason_text(CloseReason::Network, OnlinePeer::PEER_WORD);
                                        h += w::note(ui, cx, y + h, width, &t, true);
                                    } else if !rooms.is_empty() {
                                        h += room_list(ui, cx, y + h, rooms, *pick, def.name);
                                    } else {
                                        h += m::SP2
                                            + w::note(ui, cx, y + h + m::SP2, width, &format!("nobody’s hosting {} right now", def.name), false)
                                            + m::SP2;
                                    }
                                    h += m::SP2;
                                    let mut v = Vec::new();
                                    if !rooms.is_empty() {
                                        v.push(Run::plain("↑↓ pick · ↵ join · "));
                                    }
                                    back_runs(&mut v);
                                    you(&mut v, &nick);
                                    h + note_runs(ui, cx, y + h, &v)
                                });
                            }
                        } else {
                            let mut v = vec![Run::plain(if def.together.len() > 1 { "↑↓ pick · ↵ play · " } else { "↵ plays · " })];
                            back_runs(&mut v);
                            v.push(Run::plain(&fs_label));
                            col.add(ui, |ui, y| note_runs(ui, cx, y, &v));
                        }
                        if let Some(footer) = def.footer {
                            col.add(ui, |ui, y| footer(ui, x, y, width, env));
                        }
                    }
                }
                if let Some(t) = note {
                    col.add(ui, |ui, y| w::note(ui, cx, y, width, t, false));
                }
            }
            LobbyState::Setup { locked } => {
                col.add(ui, |ui, y| w::line(ui, cx, y, width, &format!("host a room as {nick}")));
                let items = [
                    ButtonSpec {
                        title: "open",
                        kbd: Some("O".into()),
                        on: !locked,
                        hit: Hit::Key("o".into()),
                    },
                    ButtonSpec {
                        title: "🔒 locked",
                        kbd: Some("L".into()),
                        on: *locked,
                        hit: Hit::Key("l".into()),
                    },
                ];
                col.add(ui, |ui, y| w::button_row(ui, cx, y, width, &items, false));
                let who = if *locked {
                    "only someone with the code you send can join".to_string()
                } else {
                    format!("anyone who opens {} can join", def.name)
                };
                let v = [Run::plain(&who), Run::plain(" · ↵ host · "), Run::link("⌫ back", Hit::Key("Backspace".into()))];
                col.add(ui, |ui, y| note_runs(ui, cx, y, &v));
            }
            LobbyState::Hosting { locked, code, renewed } => {
                if *locked {
                    let shown: Vec<String> = match code {
                        Some(c) if !c.is_empty() => c.chars().map(|c| c.to_string()).collect(),
                        _ => vec!["·".to_string(); n],
                    };
                    col.add(ui, |ui, y| w::code(ui, cx, y, &shown, false, false, None));
                    let line = format!(
                        "{}{}",
                        if *renewed { "too many wrong codes, so here’s a new one. " } else { "" },
                        if self.code_only {
                            format!("send your friend this code · on their Q Calc: type {}, J, then the code", def.name)
                        } else {
                            format!("send your friend this code · they pick {nick} in the {} list", def.name)
                        }
                    );
                    col.add(ui, |ui, y| w::line(ui, cx, y, width, &line));
                } else {
                    let v = [Run::plain("your room is open · it’s listed as "), Run { text: &nick, hit: None, ink: true, bold: true }];
                    let ink = ui.c.ink;
                    col.add(ui, |ui, y| w::runs(ui, cx, y, Font::BODY, ink, &v));
                }
                col.add(ui, |ui, y| w::note(ui, cx, y, width, "waiting for someone to join…", true));
                let v = [Run::link("⌫ back", Hit::Key("Backspace".into()))];
                col.add(ui, |ui, y| note_runs(ui, cx, y, &v));
            }
            LobbyState::Code { room, digits, note } => {
                code_screen(ui, &mut col, cx, width, n, room, digits, note.as_deref(), false);
            }
            LobbyState::Pairing { room } => {
                if room.locked {
                    code_screen(ui, &mut col, cx, width, n, room, "", None, true);
                } else {
                    col.add(ui, |ui, y| {
                        let a = w::wait_alpha(ui);
                        let was = ui.set_alpha(a);
                        let h = w::line(ui, cx, y, width, &format!("joining {}…", room.name));
                        ui.set_alpha(was);
                        h
                    });
                    let v = [Run::link("⌫ back", Hit::Key("Backspace".into()))];
                    col.add(ui, |ui, y| note_runs(ui, cx, y, &v));
                }
            }
            LobbyState::Rename { draft, .. } => {
                col.add(ui, |ui, y| w::line(ui, cx, y, width, "what the room lists call you"));
                col.add(ui, |ui, y| {
                    let c = ui.c;
                    let (tw, th) = ui.measure(draft, Font::BODY);
                    let bw = (tw + 1.0 + 2.0 * m::SP3).max(220.0);
                    let r = Rect::new(cx - bw / 2.0, y, bw, th.max(m::FS_BODY * 1.4) + 12.0);
                    ui.ring(r, m::RADIUS_S, 1.0, c.ring());
                    let tx = cx - (tw + 1.0) / 2.0;
                    ui.text(draft, Font::BODY, tx, r.y + 6.0, c.ink);
                    // .lobby-caret: a 1 px accent bar blinking once a second
                    ui.animate();
                    if (ui.t * 2.0) as i64 % 2 == 0 {
                        ui.fill(Rect::new(tx + tw + 1.0, r.y + (r.h - m::FS_BODY * 1.1) / 2.0, 1.0, m::FS_BODY * 1.1), c.accent);
                    }
                    r.h
                });
                col.add(ui, |ui, y| w::note(ui, cx, y, width, "↵ keep · tab picks another · esc back", false));
            }
            LobbyState::Game { .. } | LobbyState::Local { .. } => {}
        }
        let content = col.y - y;
        let min = 168.0 - m::SP3 - m::SP1;
        content.max(min) + m::SP1
    }
}

/// A centred column of rows `SP3` apart.
struct Col {
    y: f32,
    first: bool,
}

impl Col {
    fn add(&mut self, ui: &mut Ui, f: impl FnOnce(&mut Ui, f32) -> f32) {
        if !self.first {
            self.y += m::SP3;
        }
        self.first = false;
        self.y += f(ui, self.y);
    }
}

fn note_runs(ui: &mut Ui, cx: f32, y: f32, v: &[Run]) -> f32 {
    let muted = ui.c.muted;
    w::runs(ui, cx, y, Font::SMALL, muted, v)
}

/// A run's text must outlive the frame's run list; the nickname is short and changes rarely, so it is kept.
fn leak(s: &str) -> &'static str {
    thread_local! {
        static NAMES: std::cell::RefCell<Vec<&'static str>> = const { std::cell::RefCell::new(Vec::new()) };
    }
    NAMES.with(|n| {
        let mut n = n.borrow_mut();
        if let Some(&found) = n.iter().find(|&&x| x == s) {
            return found;
        }
        let made: &'static str = Box::leak(s.to_string().into_boxed_str());
        n.push(made);
        made
    })
}

#[allow(clippy::too_many_arguments)]
fn code_screen(
    ui: &mut Ui,
    col: &mut Col,
    cx: f32,
    width: f32,
    n: usize,
    room: &RoomInfo,
    digits: &str,
    note: Option<&str>,
    pairing: bool,
) {
    let line = if room.id.is_empty() {
        "the code your friend sent".to_string()
    } else {
        format!("🔒 the code for {}’s room", room.name)
    };
    col.add(ui, |ui, y| w::line(ui, cx, y, width, &line));
    let d: Vec<char> = digits.chars().collect();
    let tiles: Vec<String> = (0..n)
        .map(|i| {
            if pairing {
                "•".to_string()
            } else {
                d.get(i).map(|c| c.to_string()).unwrap_or_default()
            }
        })
        .collect();
    let caret = (!pairing).then_some(d.len());
    col.add(ui, |ui, y| w::code(ui, cx, y, &tiles, true, pairing, caret));
    let first = if pairing {
        "joining… · ".to_string()
    } else {
        note.map_or(format!("type the {n} digits · "), |t| format!("{t} · "))
    };
    let v = [Run::plain(&first), Run::link("⌫ back", Hit::Key("Backspace".into()))];
    col.add(ui, |ui, y| note_runs(ui, cx, y, &v));
}

/// `.pong-peers.lobby-list`: the rooms, five rows of 36 shown (scrolled to keep the pick in view).
fn room_list(ui: &mut Ui, cx: f32, y: f32, rooms: &[RoomInfo], pick: usize, game: &str) -> f32 {
    let c = ui.c;
    let width = 360.0;
    let x = cx - width / 2.0;
    let shown = rooms.len().min(5);
    let first = if pick >= shown { pick + 1 - shown } else { 0 };
    let mut yy = y;
    for (i, r) in rooms.iter().enumerate().skip(first).take(shown) {
        let rect = Rect::new(x, yy, width, m::TAPE_ROW);
        let on = i == pick;
        if on {
            ui.round(rect, m::RADIUS_S, c.fill_selected());
        } else if ui.hover(rect) {
            ui.round(rect, m::RADIUS_S, c.fill_hover());
        }
        let (gw, gh) = ui.measure(game, Font::SMALL);
        let lock_w = 1.4 * m::FS_BODY;
        let ty = yy + (m::TAPE_ROW - gh) / 2.0;
        let name_w = width - 2.0 * m::SP3 - gw - lock_w - 2.0 * m::SP3;
        let (_, nh) = ui.measure(&r.name, Font::BODY);
        ui.text_in(&r.name, Font::BODY, x + m::SP3, yy + (m::TAPE_ROW - nh) / 2.0, name_w, Align::Left, c.ink);
        ui.text(game, Font::SMALL, x + width - m::SP3 - lock_w - m::SP3 - gw, ty, c.muted);
        if r.locked {
            ui.text_in("🔒", Font::SMALL, x + width - m::SP3 - lock_w, ty, lock_w, Align::Right, c.ink);
        }
        ui.hit(rect, Hit::Join(i));
        yy += m::TAPE_ROW + 2.0;
    }
    yy - y - 2.0
}

/// The lobby's step for a home screen key, for tests.
#[cfg(test)]
fn step_of(state: &LobbyState, key: &str, ctx: &LobbyCtx) -> LobbyState {
    lobby_key(state, key, ctx, false, "teal otter", "red fox").state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_game_with_a_computer_and_a_link_starts_on_the_choice() {
        let ctx = LobbyCtx {
            online: true,
            code_only: false,
            code_length: 6,
            solo: 3,
            together: 0,
            peer_word: "computer".into(),
        };
        let s = home_screen(first_screen(true, 3, 0), None);
        assert!(matches!(s, LobbyState::Home { screen: Screen::Start, .. }));
        let solo = step_of(&s, "1", &ctx);
        assert!(matches!(solo, LobbyState::Home { screen: Screen::Solo, .. }));
        let friend = step_of(&s, "2", &ctx);
        assert!(matches!(friend, LobbyState::Home { screen: Screen::Friend, .. }));
        assert!(matches!(step_of(&friend, "j", &ctx), LobbyState::Code { .. }));
        // ⌫ goes back to the choice
        assert!(matches!(step_of(&solo, "Backspace", &ctx), LobbyState::Home { screen: Screen::Start, .. }));
    }
}
