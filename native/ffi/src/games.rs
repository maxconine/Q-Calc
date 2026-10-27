//! WP-GAMES: the games' logic (`qcalc_core::games`) for the native shells. The shell around every game is here
//! (esc and pause, the clock, full screen, the lobby, nicknames, the relay link); each game's own surface is in
//! `games_play.rs`. Records mirror the TS shapes; times are ms (`gameNow()` or `Date.now()` as the TS takes them).

use std::sync::{Arc, Mutex, MutexGuard};

use qcalc_core::games::{
    self as g, Random, clock, fullscreen, lobby, nickname, peer, peer_online, saves, shell,
};

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

thread_local! {
    static RAND: std::cell::RefCell<Random> = std::cell::RefCell::new(Random::from_entropy());
}

/// `Math.random()`.
pub(crate) fn math_random() -> f64 {
    RAND.with(|r| r.borrow_mut().next_f64())
}

// ---- sides, rooms and the link's events

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum GameSide {
    Host,
    Guest,
}

impl From<g::Side> for GameSide {
    fn from(s: g::Side) -> Self {
        match s {
            g::Side::Host => GameSide::Host,
            g::Side::Guest => GameSide::Guest,
        }
    }
}

impl From<GameSide> for g::Side {
    fn from(s: GameSide) -> Self {
        match s {
            GameSide::Host => g::Side::Host,
            GameSide::Guest => g::Side::Guest,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct GameRoom {
    pub id: String,
    pub name: String,
    pub locked: bool,
}

impl From<peer::RoomInfo> for GameRoom {
    fn from(r: peer::RoomInfo) -> Self {
        GameRoom {
            id: r.id,
            name: r.name,
            locked: r.locked,
        }
    }
}

impl From<GameRoom> for peer::RoomInfo {
    fn from(r: GameRoom) -> Self {
        peer::RoomInfo {
            id: r.id,
            name: r.name,
            locked: r.locked,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PeerCloseReason {
    WrongCode,
    Busy,
    Unreachable,
    Lost,
    Bye,
    Network,
    TooManyTries,
    Expired,
    Gone,
}

impl From<peer::CloseReason> for PeerCloseReason {
    fn from(r: peer::CloseReason) -> Self {
        use peer::CloseReason as C;
        match r {
            C::WrongCode => Self::WrongCode,
            C::Busy => Self::Busy,
            C::Unreachable => Self::Unreachable,
            C::Lost => Self::Lost,
            C::Bye => Self::Bye,
            C::Network => Self::Network,
            C::TooManyTries => Self::TooManyTries,
            C::Expired => Self::Expired,
            C::Gone => Self::Gone,
        }
    }
}

impl From<PeerCloseReason> for peer::CloseReason {
    fn from(r: PeerCloseReason) -> Self {
        use PeerCloseReason as C;
        match r {
            C::WrongCode => Self::WrongCode,
            C::Busy => Self::Busy,
            C::Unreachable => Self::Unreachable,
            C::Lost => Self::Lost,
            C::Bye => Self::Bye,
            C::Network => Self::Network,
            C::TooManyTries => Self::TooManyTries,
            C::Expired => Self::Expired,
            C::Gone => Self::Gone,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PeerWho {
    Peer,
    SelfSide,
}

/// `PeerEvent` in `src/lib/peer.ts`.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum GamePeerEvent {
    Rooms {
        rooms: Vec<GameRoom>,
        offline: bool,
        code_only: bool,
    },
    Hosting {
        code: String,
        renewed: bool,
    },
    Pairing,
    Open {
        role: GameSide,
        peer: String,
        resumed: bool,
    },
    Away {
        who: PeerWho,
    },
    Back {
        who: PeerWho,
    },
    Message {
        data: String,
    },
    Closed {
        reason: PeerCloseReason,
    },
}

fn who(w: peer::Who) -> PeerWho {
    match w {
        peer::Who::Peer => PeerWho::Peer,
        peer::Who::SelfSide => PeerWho::SelfSide,
    }
}

impl From<peer::PeerEvent> for GamePeerEvent {
    fn from(e: peer::PeerEvent) -> Self {
        use peer::PeerEvent as E;
        match e {
            E::Rooms {
                rooms,
                offline,
                code_only,
            } => Self::Rooms {
                rooms: rooms.into_iter().map(Into::into).collect(),
                offline,
                code_only,
            },
            E::Hosting { code, renewed } => Self::Hosting { code, renewed },
            E::Pairing => Self::Pairing,
            E::Open {
                role,
                peer,
                resumed,
                ..
            } => Self::Open {
                role: role.into(),
                peer,
                resumed,
            },
            E::Away { who: w } => Self::Away { who: who(w) },
            E::Back { who: w } => Self::Back { who: who(w) },
            E::Message { data } => Self::Message { data },
            E::Closed { reason } => Self::Closed {
                reason: reason.into(),
            },
        }
    }
}

impl From<GamePeerEvent> for peer::PeerEvent {
    fn from(e: GamePeerEvent) -> Self {
        use GamePeerEvent as E;
        let back = |w: PeerWho| match w {
            PeerWho::Peer => peer::Who::Peer,
            PeerWho::SelfSide => peer::Who::SelfSide,
        };
        match e {
            E::Rooms {
                rooms,
                offline,
                code_only,
            } => Self::Rooms {
                rooms: rooms.into_iter().map(Into::into).collect(),
                offline,
                code_only,
            },
            E::Hosting { code, renewed } => Self::Hosting { code, renewed },
            E::Pairing => Self::Pairing,
            E::Open {
                role,
                peer,
                resumed,
            } => Self::Open {
                role: role.into(),
                peer,
                seat: None,
                resumed,
            },
            E::Away { who: w } => Self::Away { who: back(w) },
            E::Back { who: w } => Self::Back { who: back(w) },
            E::Message { data } => Self::Message { data },
            E::Closed { reason } => Self::Closed {
                reason: reason.into(),
            },
        }
    }
}

/// `closeReasonText(reason, word)`.
#[uniffi::export]
pub fn peer_close_text(reason: PeerCloseReason, word: String) -> String {
    peer::close_reason_text(reason.into(), &word)
}

// ---- nicknames

#[uniffi::export]
pub fn clean_nickname(raw: String) -> String {
    nickname::clean_nickname(&raw)
}

#[uniffi::export]
pub fn random_nickname() -> String {
    nickname::random_nickname(&mut math_random)
}

// ---- esc, pause and full screen

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum GameShellMode {
    Lobby,
    Local,
    Online,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum GameEscAction {
    Unfullscreen,
    Lobby,
    Pause,
    Arm,
    Leave,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum GamePausedKey {
    Pass,
    Fullscreen,
    Resume,
}

/// How long "esc again to leave" waits, in ms.
#[uniffi::export]
pub fn game_arm_ms() -> f64 {
    shell::ARM_MS
}

/// `escAction` in `src/lib/gameShell.ts`.
#[uniffi::export]
pub fn game_esc_action(
    mode: GameShellMode,
    fullscreen: bool,
    paused: bool,
    armed_at: Option<f64>,
    idle: bool,
    now: f64,
) -> GameEscAction {
    let mode = match mode {
        GameShellMode::Lobby => shell::ShellMode::Lobby,
        GameShellMode::Local => shell::ShellMode::Local,
        GameShellMode::Online => shell::ShellMode::Online,
    };
    match shell::esc_action(&shell::EscState {
        mode,
        fullscreen,
        paused,
        armed_at,
        idle,
        now,
    }) {
        shell::EscAction::Unfullscreen => GameEscAction::Unfullscreen,
        shell::EscAction::Lobby => GameEscAction::Lobby,
        shell::EscAction::Pause => GameEscAction::Pause,
        shell::EscAction::Arm => GameEscAction::Arm,
        shell::EscAction::Leave => GameEscAction::Leave,
    }
}

/// `pausedKey`: what a key does while the paused card is up.
#[uniffi::export]
pub fn game_paused_key(key: String, meta: bool, ctrl: bool, alt: bool) -> GamePausedKey {
    match shell::paused_key(&shell::KeyLike {
        key,
        meta,
        ctrl,
        alt,
    }) {
        shell::PausedKey::Pass => GamePausedKey::Pass,
        shell::PausedKey::Fullscreen => GamePausedKey::Fullscreen,
        shell::PausedKey::Resume => GamePausedKey::Resume,
    }
}

/// ⌃F and nothing else (`isFullscreenChord`); `code` is the qwerty key's name (`KeyF`).
#[uniffi::export]
pub fn game_is_fullscreen_chord(
    key: String,
    code: String,
    ctrl: bool,
    meta: bool,
    alt: bool,
    shift: bool,
) -> bool {
    fullscreen::is_fullscreen_chord(&fullscreen::Chord {
        key,
        code,
        ctrl,
        meta,
        alt,
        shift,
    })
}

/// What the full-screen button and f are called (`fullscreenLabel`); the Mac app is `windowed`.
#[uniffi::export]
pub fn game_fullscreen_label(on: bool, windowed: bool) -> String {
    fullscreen::fullscreen_label(on, windowed).to_string()
}

/// The keys a game keeps between launches (`GAME_SAVE_KEYS`), and the most each may hold.
#[uniffi::export]
pub fn game_save_keys() -> Vec<String> {
    saves::GAME_SAVE_KEYS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

#[uniffi::export]
pub fn game_save_max() -> u32 {
    saves::GAME_SAVE_MAX as u32
}

/// The clock games run on (`gameClock.ts`): pausing stops it. `mono` is a monotonic ms clock, `wall` `Date.now()`.
#[derive(Debug, Default, uniffi::Object)]
pub struct GameClock {
    inner: Mutex<clock::GameClock>,
}

#[uniffi::export]
impl GameClock {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(GameClock::default())
    }

    pub fn paused(&self) -> bool {
        lock(&self.inner).paused()
    }

    pub fn now(&self, mono: f64) -> f64 {
        lock(&self.inner).now(mono)
    }

    pub fn date(&self, mono: f64, wall: f64) -> f64 {
        lock(&self.inner).date(mono, wall)
    }

    pub fn pause(&self, mono: f64) {
        lock(&self.inner).pause(mono)
    }

    pub fn resume(&self, mono: f64) {
        lock(&self.inner).resume(mono)
    }
}

// ---- the lobby

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LobbyScreen {
    Start,
    Solo,
    Friend,
}

impl From<lobby::Screen> for LobbyScreen {
    fn from(s: lobby::Screen) -> Self {
        match s {
            lobby::Screen::Start => LobbyScreen::Start,
            lobby::Screen::Solo => LobbyScreen::Solo,
            lobby::Screen::Friend => LobbyScreen::Friend,
        }
    }
}

impl From<LobbyScreen> for lobby::Screen {
    fn from(s: LobbyScreen) -> Self {
        match s {
            LobbyScreen::Start => lobby::Screen::Start,
            LobbyScreen::Solo => lobby::Screen::Solo,
            LobbyScreen::Friend => lobby::Screen::Friend,
        }
    }
}

/// `LobbyState` in `src/lib/lobby.ts`.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LobbyState {
    Home {
        screen: LobbyScreen,
        rooms: Vec<GameRoom>,
        pick: u32,
        note: Option<String>,
        offline: bool,
    },
    Setup {
        locked: bool,
    },
    Hosting {
        locked: bool,
        code: Option<String>,
        renewed: bool,
    },
    Code {
        room: GameRoom,
        digits: String,
        note: Option<String>,
    },
    Pairing {
        room: GameRoom,
    },
    Rename {
        draft: String,
        from: LobbyScreen,
    },
    Game {
        role: GameSide,
        peer: String,
    },
    Local {
        option: u32,
    },
}

impl From<lobby::LobbyState> for LobbyState {
    fn from(s: lobby::LobbyState) -> Self {
        use lobby::LobbyState as S;
        match s {
            S::Home {
                screen,
                rooms,
                pick,
                note,
                offline,
            } => Self::Home {
                screen: screen.into(),
                rooms: rooms.into_iter().map(Into::into).collect(),
                pick: pick as u32,
                note,
                offline,
            },
            S::Setup { locked } => Self::Setup { locked },
            S::Hosting {
                locked,
                code,
                renewed,
            } => Self::Hosting {
                locked,
                code,
                renewed,
            },
            S::Code { room, digits, note } => Self::Code {
                room: room.into(),
                digits,
                note,
            },
            S::Pairing { room } => Self::Pairing { room: room.into() },
            S::Rename { draft, from } => Self::Rename {
                draft,
                from: from.into(),
            },
            S::Game { role, peer } => Self::Game {
                role: role.into(),
                peer,
            },
            S::Local { option } => Self::Local {
                option: option as u32,
            },
        }
    }
}

impl From<LobbyState> for lobby::LobbyState {
    fn from(s: LobbyState) -> Self {
        use LobbyState as S;
        match s {
            S::Home {
                screen,
                rooms,
                pick,
                note,
                offline,
            } => Self::Home {
                screen: screen.into(),
                rooms: rooms.into_iter().map(Into::into).collect(),
                pick: pick as usize,
                note,
                offline,
            },
            S::Setup { locked } => Self::Setup { locked },
            S::Hosting {
                locked,
                code,
                renewed,
            } => Self::Hosting {
                locked,
                code,
                renewed,
            },
            S::Code { room, digits, note } => Self::Code {
                room: room.into(),
                digits,
                note,
            },
            S::Pairing { room } => Self::Pairing { room: room.into() },
            S::Rename { draft, from } => Self::Rename {
                draft,
                from: from.into(),
            },
            S::Game { role, peer } => Self::Game {
                role: role.into(),
                peer,
            },
            S::Local { option } => Self::Local {
                option: option as usize,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LobbyEffect {
    Host { locked: bool },
    Join { room: GameRoom, code: String },
    Cancel,
    Rename { name: String },
    Fullscreen,
}

impl From<lobby::LobbyEffect> for LobbyEffect {
    fn from(e: lobby::LobbyEffect) -> Self {
        use lobby::LobbyEffect as E;
        match e {
            E::Host { locked } => Self::Host { locked },
            E::Join { room, code } => Self::Join {
                room: room.into(),
                code,
            },
            E::Cancel => Self::Cancel,
            E::Rename { name } => Self::Rename { name },
            E::Fullscreen => Self::Fullscreen,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LobbyStep {
    pub state: LobbyState,
    pub effect: Option<LobbyEffect>,
    pub handled: bool,
}

impl From<lobby::LobbyStep> for LobbyStep {
    fn from(s: lobby::LobbyStep) -> Self {
        LobbyStep {
            state: s.state.into(),
            effect: s.effect.map(Into::into),
            handled: s.handled,
        }
    }
}

/// `LobbyCtx`: the link (`online`, `code_only`, `code_length`, `peer_word`) and the game's local options.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LobbyContext {
    pub online: bool,
    pub code_only: bool,
    pub code_length: u32,
    pub solo: u32,
    pub together: u32,
    pub peer_word: String,
}

impl From<LobbyContext> for lobby::LobbyCtx {
    fn from(c: LobbyContext) -> Self {
        lobby::LobbyCtx {
            online: c.online,
            code_only: c.code_only,
            code_length: c.code_length as usize,
            solo: c.solo as usize,
            together: c.together as usize,
            peer_word: c.peer_word,
        }
    }
}

/// `homeScreen(screen, note)`.
#[uniffi::export]
pub fn lobby_home(screen: LobbyScreen, note: Option<String>) -> LobbyState {
    lobby::home_screen(screen.into(), note).into()
}

#[uniffi::export]
pub fn lobby_first_screen(online: bool, solo: u32, together: u32) -> LobbyScreen {
    lobby::first_screen(online, solo as usize, together as usize).into()
}

#[uniffi::export]
pub fn lobby_option_screen(option: u32, solo: u32) -> LobbyScreen {
    lobby::option_screen(option as usize, solo as usize).into()
}

/// A plain key on the lobby; tab on the rename screen offers a fresh random nickname.
#[uniffi::export]
pub fn lobby_key(
    state: LobbyState,
    key: String,
    ctx: LobbyContext,
    locked_pref: bool,
    name: String,
) -> LobbyStep {
    let random = random_nickname();
    lobby::lobby_key(
        &state.into(),
        &key,
        &ctx.into(),
        locked_pref,
        &name,
        &random,
    )
    .into()
}

#[uniffi::export]
pub fn lobby_back(state: LobbyState, ctx: LobbyContext) -> Option<LobbyStep> {
    lobby::lobby_back(&state.into(), &ctx.into()).map(Into::into)
}

#[uniffi::export]
pub fn lobby_event(state: LobbyState, event: GamePeerEvent, ctx: LobbyContext) -> LobbyState {
    lobby::lobby_event(&state.into(), &event.into(), &ctx.into()).into()
}

#[uniffi::export]
pub fn lobby_join_room(room: GameRoom) -> LobbyStep {
    lobby::join_room(&room.into()).into()
}

#[uniffi::export]
pub fn lobby_holds_link(state: LobbyState) -> bool {
    lobby::lobby_holds_link(&state.into())
}

// ---- the relay link

/// The relay this build talks to, and how long its room codes are.
#[uniffi::export]
pub fn relay_default_url() -> String {
    peer_online::RELAY_URL.to_string()
}

#[uniffi::export]
pub fn relay_code_length() -> u32 {
    peer_online::ONLINE_CODE_LENGTH as u32
}

/// What the shell does with its websockets.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum RelaySocketCmd {
    Open { id: u64, url: String },
    Send { id: u64, text: String },
    Close { id: u64, code: u16 },
}

impl From<peer_online::SocketCmd> for RelaySocketCmd {
    fn from(c: peer_online::SocketCmd) -> Self {
        use peer_online::SocketCmd as C;
        match c {
            C::Open { id, url } => Self::Open { id, url },
            C::Send { id, text } => Self::Send { id, text },
            C::Close { id, code } => Self::Close { id, code },
        }
    }
}

/// `OnlinePeer` in `src/lib/peerOnline.ts`, without its sockets and timers: after every call the shell runs
/// `take_commands`, hands `take_events` to whoever subscribed, and calls `tick` by `next_deadline`. `now` is
/// `Date.now()`.
#[derive(Debug, uniffi::Object)]
pub struct RelayPeer {
    inner: Mutex<peer_online::OnlinePeer>,
}

#[uniffi::export]
impl RelayPeer {
    #[uniffi::constructor]
    pub fn new(base: String) -> Arc<Self> {
        Arc::new(RelayPeer {
            inner: Mutex::new(peer_online::OnlinePeer::new(&base)),
        })
    }

    pub fn watch(&self, game: String, now: f64) {
        lock(&self.inner).watch(&game, now)
    }
    pub fn unwatch(&self) {
        lock(&self.inner).unwatch()
    }
    pub fn host(&self, game: String, name: String, locked: bool, now: f64) {
        lock(&self.inner).host(&game, &name, locked, now)
    }
    pub fn join(&self, room_id: String, code: String, name: String, now: f64) {
        lock(&self.inner).join(&room_id, &code, &name, now)
    }
    pub fn send(&self, data: String) {
        lock(&self.inner).send(&data)
    }
    pub fn step_away(&self, now: f64) {
        lock(&self.inner).step_away(now)
    }
    pub fn come_back(&self, now: f64) {
        lock(&self.inner).come_back(now)
    }
    pub fn retry_now(&self, now: f64) {
        lock(&self.inner).retry_now(now)
    }
    pub fn close(&self) {
        lock(&self.inner).close()
    }
    pub fn page_hide(&self) {
        lock(&self.inner).page_hide()
    }
    pub fn opened(&self, id: u64, now: f64) {
        lock(&self.inner).opened(id, now)
    }
    pub fn message(&self, id: u64, text: String, now: f64) {
        lock(&self.inner).message(id, &text, now)
    }
    pub fn closed(&self, id: u64, now: f64) {
        lock(&self.inner).closed(id, now)
    }
    pub fn tick(&self, now: f64) {
        lock(&self.inner).tick(now)
    }
    pub fn next_deadline(&self) -> Option<f64> {
        lock(&self.inner).next_deadline()
    }
    pub fn take_commands(&self) -> Vec<RelaySocketCmd> {
        lock(&self.inner)
            .take_commands()
            .into_iter()
            .map(Into::into)
            .collect()
    }
    pub fn take_events(&self) -> Vec<GamePeerEvent> {
        lock(&self.inner)
            .take_events()
            .into_iter()
            .map(Into::into)
            .collect()
    }
}

// ---- full screen's layout (`fullscreenLayout`, `letterbox`), for a game with a picture of its own shape

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct GameBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub scale: f64,
}

impl From<fullscreen::GameBox> for GameBox {
    fn from(b: fullscreen::GameBox) -> Self {
        GameBox {
            x: b.x,
            y: b.y,
            w: b.w,
            h: b.h,
            scale: b.scale,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct GameFullscreenLayout {
    /// Where the picture goes, inside the viewport.
    pub stage: GameBox,
    /// How much larger the hud and text are drawn.
    pub ui: f64,
}

/// `fullscreenLayout(viewport, { base, chrome, pad, maxUi, integer })`.
#[uniffi::export]
#[allow(clippy::too_many_arguments)]
pub fn game_fullscreen_layout(
    view_w: f64,
    view_h: f64,
    base_w: f64,
    base_h: f64,
    chrome: f64,
    pad: f64,
    max_ui: f64,
    integer: bool,
) -> GameFullscreenLayout {
    let o = fullscreen::FullscreenLayoutOptions {
        base: fullscreen::Size {
            w: base_w,
            h: base_h,
        },
        chrome,
        pad,
        max_ui,
        integer,
    };
    let l = fullscreen::fullscreen_layout(
        fullscreen::Size {
            w: view_w,
            h: view_h,
        },
        &o,
    );
    GameFullscreenLayout {
        stage: l.stage.into(),
        ui: l.ui,
    }
}

/// `letterbox(container, base, { integer })`.
#[uniffi::export]
pub fn game_letterbox(w: f64, h: f64, base_w: f64, base_h: f64, integer: bool) -> GameBox {
    fullscreen::letterbox(
        fullscreen::Size { w, h },
        fullscreen::Size {
            w: base_w,
            h: base_h,
        },
        integer,
    )
    .into()
}
