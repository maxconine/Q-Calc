//! Messages between Q Calc users through the relay's inboxes. Spec: macos/Messages.swift, MessageInbox.swift,
//! BarModel.swift's message line and cards, BarView.swift's `MessageCards`; the rules are the core's
//! (`app::messaging`, `message_banner`, `name_check`); the transport is relay.rs.
//!
//! - **The line.** `message max hello` sends through the relay; while it's typed the answer slot says what ↵ will
//!   do (set your name first, the name's rule, "username not found" once the relay has said so, "↵ sends to max"),
//!   asking the relay about the name 350 ms after typing rests (`check_name`). The send's status shows under the
//!   bar ("sent to max", "max isn't on q calc right now · not sent"...), and a delivered line clears.
//! - **The cards.** An accepted name's message is a card under the bar with a reply line; someone new's first
//!   message is a request card with Allow and Block, and a blocked name's messages are dropped unseen. While the
//!   bar is away a message waits (and a tray balloon says so, the sender as its title, as the Mac's notification
//!   does); the bar shows it on its next open. Cards go after 10 s unless the pointer is on them or a reply is being
//!   typed or sent. Text is sanitised (controls and direction overrides become spaces) by the core.
//! - **The name** is held on the relay by a 64-hex key made here, kept with the lists in `messaging.json` beside
//!   settings.json. The settings window sets and lets go of it (`set_name`, `turn_off`, `status`, the lists).
//!
//! **Cost.** Nothing until a name is set or a `message` line is typed: no thread, no socket, no window, and
//! winhttp.dll isn't loaded. With a name, one thread waits in the socket's receive and another pings every 30 s.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use qcalc_core::app::message_banner::{self as banner, BannerCard};
use qcalc_core::app::messaging::{
    self as core, Arrival, InboxEvent, MessageLine, Messaging, SendStatus,
};
use qcalc_core::app::name_check::{CHECK_DELAY_MS, NameCheck};
use serde_json::{Value, json};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_LEADING, IDWriteTextFormat,
    IDWriteTextLayout,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, HWND_MESSAGE, PostMessageW, RegisterClassExW, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_APP, WNDCLASSEXW,
};
use windows::core::w;

use crate::app::App;
use crate::input::Field;
use crate::keys::Mods;
use crate::model::BarModel;
use crate::relay::{self, Frame, Socket};
use crate::render::{Canvas, Gfx, Rect, caret_x, hit, measure, rgb, size};
use crate::{later, qlog};

const WM_RELAY: u32 = WM_APP + 0x51;
const FILE: &str = "messaging.json";
/// unseen messages kept for the bar while it's away
const INBOX_CAP: usize = 20;
const PING: Duration = Duration::from_secs(30);
const RETRY_MAX_MS: u32 = 60_000;

// ---- what the relay's link is doing ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    Off,
    Connecting,
    Ready,
    Taken,
    Offline,
}

/// What the worker threads hand the UI thread.
enum Event {
    Opened(u64, Arc<Socket>),
    Frame(u64, String),
    Dropped(u64),
    Claimed(String, String),
    Sent(Sent, String, String),
    Looked(String, Option<bool>),
}

#[derive(Clone, Debug)]
enum Sent {
    /// from the bar's `message` line
    Line,
    /// a card's reply
    Reply(String),
}

struct State {
    loaded: bool,
    m: Messaging,
    link: Link,
    note: String,
    busy: bool,
    /// what each asking name said last (memory only), shown once they're allowed
    asking: BTreeMap<String, String>,
    /// unseen while the bar was away, oldest first
    inbox: Vec<BannerCard>,
    next_id: u64,
    socket: Option<Arc<Socket>>,
    generation: u64,
    retry_ms: u32,
    window: Option<HWND>,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    names: NameCheck,
    /// the cards' 13 pt semibold for the sender (made at the first card)
    bold: Option<IDWriteTextFormat>,
}

impl Default for State {
    fn default() -> State {
        let (tx, rx) = channel();
        State {
            loaded: false,
            m: Messaging::default(),
            link: Link::Off,
            note: String::new(),
            busy: false,
            asking: BTreeMap::new(),
            inbox: Vec::new(),
            next_id: 1,
            socket: None,
            generation: 0,
            retry_ms: 3000,
            window: None,
            tx,
            rx,
            names: NameCheck::new(),
            bold: None,
        }
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if !s.loaded {
            s.loaded = true;
            s.m = load();
        }
        f(&mut s)
    })
}

fn now_ms() -> f64 {
    crate::model::now_ms() as f64
}

// ---- the file ----

fn path() -> Option<std::path::PathBuf> {
    crate::store::config_dir().map(|d| d.join(FILE))
}

fn load() -> Messaging {
    path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .map(|v| core::sanitize_messaging(&v))
        .unwrap_or_default()
}

fn save(m: &Messaging) {
    let Some(p) = path() else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_string_pretty(m) {
        Ok(text) => {
            if let Err(e) = crate::store::write_atomic(&p, &text) {
                qlog!("messaging: couldn't save: {e}");
            }
        }
        Err(e) => qlog!("messaging: couldn't save: {e}"),
    }
}

/// A new key: 32 random bytes as hex (the relay keeps only its hash).
fn new_key() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::with_capacity(64);
    for i in 0..4u64 {
        // RandomState is seeded from the system's RNG per process; mixed with the clock and the step
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(i);
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

// ---- the relay window: worker threads post here, and events run on the app ----

fn ensure_window(s: &mut State) -> Option<HWND> {
    if let Some(w) = s.window {
        return Some(w);
    }
    // SAFETY: a message-only window of our own class with our static wndproc
    let hwnd = unsafe {
        let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).ok()?;
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(relay_proc),
            hInstance: instance.into(),
            lpszClassName: w!("QCalcRelay"),
            ..Default::default()
        };
        RegisterClassExW(&class);
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("QCalcRelay"),
            w!(""),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance.into()),
            None,
        )
        .ok()?
    };
    s.window = Some(hwnd);
    Some(hwnd)
}

/// A worker thread's way back: the event, then a nudge to the relay window.
#[derive(Clone)]
struct Poster {
    hwnd: usize,
    tx: Sender<Event>,
}

impl Poster {
    fn send(&self, e: Event) {
        if self.tx.send(e).is_ok() {
            // SAFETY: posting to our own message-only window from a worker thread
            let _ = unsafe {
                PostMessageW(
                    Some(HWND(self.hwnd as *mut _)),
                    WM_RELAY,
                    WPARAM(0),
                    LPARAM(0),
                )
            };
        }
    }
}

fn poster(s: &mut State) -> Option<Poster> {
    let hwnd = ensure_window(s)?.0 as usize;
    Some(Poster {
        hwnd,
        tx: s.tx.clone(),
    })
}

unsafe extern "system" fn relay_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_RELAY {
        loop {
            let ev = STATE.with(|s| s.try_borrow().ok().and_then(|s| s.rx.try_recv().ok()));
            let Some(ev) = ev else { break };
            crate::app::poke(|app| on_event(app, ev));
        }
        return LRESULT(0);
    }
    // SAFETY: the default for our own window
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

/// A POST on a worker thread; its status comes back as `done(status)` made into an event.
fn post_later(
    path: &'static str,
    body: Value,
    done: impl FnOnce(String) -> Event + Send + 'static,
) {
    let Some(post) = with(poster) else {
        return;
    };
    let spawned = std::thread::Builder::new()
        .name("qcalc-relay-post".into())
        .spawn(move || post.send(done(relay::post(path, &body))));
    if let Err(e) = spawned {
        qlog!("messaging: no thread for {path}: {e}");
    }
}

// ---- the inbox socket ----

/// After launch (off the first frame): with a name set, the inbox opens. Without one, nothing at all.
pub fn start() {
    crate::settings_window::set_messages_hook(crate::settings_window::MessagesHook {
        items: settings_items,
        changed: settings_changed,
    });
    let named = with(|s| !s.m.name.is_empty() && !s.m.key.is_empty());
    if named {
        connect();
    }
}

/// Back from sleep: the socket likely died quietly; tried again at once.
pub fn woke() {
    let go = STATE.with(|s| {
        s.try_borrow()
            .is_ok_and(|s| s.loaded && !s.m.name.is_empty() && s.link != Link::Taken)
    });
    if go {
        with(|s| s.retry_ms = 3000);
        connect();
    }
}

fn close(s: &mut State) {
    later::cancel("relay-retry");
    if let Some(sock) = s.socket.take() {
        sock.close();
    }
    s.generation += 1;
}

/// One socket while a name is set, none otherwise.
fn connect() {
    let job = with(|s| {
        close(s);
        if s.m.name.is_empty() || s.m.key.is_empty() {
            s.link = Link::Off;
            return None;
        }
        s.link = Link::Connecting;
        let post = poster(s)?;
        Some((s.generation, s.m.name.clone(), s.m.key.clone(), post))
    });
    let Some((generation, name, key, post)) = job else {
        return;
    };
    let spawned = std::thread::Builder::new()
        .name("qcalc-inbox".into())
        .spawn(move || {
            let Some(sock) = Socket::open(&name, &key).map(Arc::new) else {
                post.send(Event::Dropped(generation));
                return;
            };
            post.send(Event::Opened(generation, sock.clone()));
            let heard = Arc::new(AtomicU64::new(now_ms() as u64));
            // the relay answers "ping" without waking; two missed answers mean the link is gone
            let (pinged, alive) = (sock.clone(), heard.clone());
            let _ = std::thread::Builder::new()
                .name("qcalc-inbox-ping".into())
                .spawn(move || {
                    loop {
                        std::thread::sleep(PING);
                        let quiet = now_ms() as u64 - alive.load(Ordering::Relaxed);
                        if quiet > PING.as_millis() as u64 * 5 / 2 {
                            pinged.close();
                        }
                        if !pinged.send("ping") {
                            return;
                        }
                    }
                });
            loop {
                match sock.receive() {
                    Frame::Text(t) => {
                        heard.store(now_ms() as u64, Ordering::Relaxed);
                        if t != "pong" {
                            post.send(Event::Frame(generation, t));
                        }
                    }
                    Frame::Closed => {
                        sock.close();
                        post.send(Event::Dropped(generation));
                        return;
                    }
                }
            }
        });
    if let Err(e) = spawned {
        qlog!("messaging: no inbox thread: {e}");
    }
}

fn on_event(app: &mut App, ev: Event) {
    on_event_now(app, ev);
    // the Messages page's status and lists follow the relay
    crate::settings_window::refresh(app);
}

fn on_event_now(app: &mut App, ev: Event) {
    match ev {
        Event::Opened(g, sock) => {
            let current = with(|s| {
                let current = g == s.generation;
                if current {
                    s.socket = Some(sock.clone());
                }
                current
            });
            if !current {
                sock.close();
            }
        }
        Event::Frame(g, text) => {
            if with(|s| s.generation) != g {
                return;
            }
            match core::parse_inbox_frame(&text) {
                Some(InboxEvent::State { state }) if state == "ready" => {
                    qlog!("messaging: inbox open");
                    with(|s| {
                        s.link = Link::Ready;
                        s.retry_ms = 3000;
                    });
                }
                Some(InboxEvent::State { .. }) => {
                    // another key holds the name now; trying again won't change that
                    qlog!("messaging: the name was taken");
                    with(|s| {
                        close(s);
                        s.link = Link::Taken;
                    });
                }
                Some(InboxEvent::Message { from, text }) => arrived(app, &from, &text),
                None => {}
            }
        }
        Event::Dropped(g) => {
            let wait = with(|s| {
                if g != s.generation || s.m.name.is_empty() || s.link == Link::Taken {
                    return None;
                }
                s.socket = None;
                s.link = Link::Offline;
                let wait = s.retry_ms;
                s.retry_ms = (s.retry_ms * 2).min(RETRY_MAX_MS);
                Some(wait)
            });
            if let Some(ms) = wait {
                qlog!("messaging: inbox dropped; again in {} s", ms / 1000);
                later::free("relay-retry", ms, connect);
            }
        }
        Event::Claimed(next, status) => claimed(&next, &status),
        Event::Sent(Sent::Line, name, status) => line_sent(app, &name, &status),
        Event::Sent(Sent::Reply(id), name, status) => reply_sent(app, &id, &name, &status),
        Event::Looked(name, found) => {
            with(|s| s.names.finish(&name, found, now_ms()));
            // still typing to them: the hint follows what the relay said
            if found.is_some()
                && let Some(crate::commands::Shown {
                    command: crate::commands::Command::Message { name: now, .. },
                    ..
                }) = &app.model.command
                && *now == name
            {
                app.model.evaluate_live(&app.engine);
            }
        }
    }
}

// ---- arrivals ----

fn arrived(app: &mut App, from: &str, text: &str) {
    let how = with(|s| core::arrival(&s.m, from));
    match how {
        Arrival::Drop => {}
        Arrival::Show => deliver(app, from, text),
        Arrival::Ask => {
            // a name already asking isn't asked about again
            let asked = with(|s| {
                s.asking
                    .insert(from.to_string(), text.to_string())
                    .is_some()
            });
            if asked {
                return;
            }
            let card = new_card(from, "", true);
            if app.bar.visible {
                show_cards(app, vec![card]);
            } else {
                with(|s| keep(s, card));
                app.tray
                    .notify("Q Calc", &format!("{from} wants to message you. Allow?"));
            }
        }
    }
}

fn new_card(from: &str, text: &str, ask: bool) -> BannerCard {
    with(|s| {
        let id = format!("n{}", s.next_id);
        s.next_id += 1;
        BannerCard {
            id,
            from: from.to_string(),
            text: text.to_string(),
            ask,
        }
    })
}

fn keep(s: &mut State, card: BannerCard) {
    if card.ask && s.inbox.iter().any(|c| c.ask && c.from == card.from) {
        return;
    }
    s.inbox.push(card);
    if s.inbox.len() > INBOX_CAP {
        let extra = s.inbox.len() - INBOX_CAP;
        s.inbox.drain(..extra);
    }
}

/// Kept for the bar whatever happens to the balloon, which is only the alert while the bar is away.
fn deliver(app: &mut App, from: &str, text: &str) {
    let card = new_card(from, text, false);
    if app.bar.visible {
        show_cards(app, vec![card]);
    } else {
        with(|s| keep(s, card));
        // the sender as the title, apart from their words, so no text can pass for another name
        app.tray.notify(from, text);
    }
}

/// The bar is showing: what came while it was away becomes cards (the newest on top).
pub fn will_show(app: &mut App) {
    let waiting = STATE.with(|s| {
        s.try_borrow_mut()
            .ok()
            .map(|mut s| std::mem::take(&mut s.inbox))
            .unwrap_or_default()
    });
    if !waiting.is_empty() {
        show_cards(app, waiting);
    }
}

fn show_cards(app: &mut App, incoming: Vec<BannerCard>) {
    let m = &mut app.model;
    let now: Vec<BannerCard> = m.cards.list.iter().map(|c| c.card.clone()).collect();
    let next = banner::add_cards(&now, incoming, banner::BANNER_SHOWN);
    let mut list = Vec::new();
    for card in next {
        match m.cards.list.iter().position(|c| c.card.id == card.id) {
            Some(i) => list.push(m.cards.list.remove(i)),
            None => list.push(Card::new(card)),
        }
    }
    m.cards.list = list;
    m.touch();
    expire_later();
}

// ---- the cards' life ----

/// A card under the bar.
pub struct Card {
    pub card: BannerCard,
    pub draft: Field,
    pub reply: Reply,
    pub shown_at: Instant,
}

impl Card {
    fn new(card: BannerCard) -> Card {
        Card {
            card,
            draft: Field::default(),
            reply: Reply::Idle,
            shown_at: Instant::now(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    Idle,
    Sending,
    Sent,
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Allow,
    Block,
    Dismiss,
    Field,
}

/// The cards under the bar (BarModel's `cards`, `cardsHovered`, `replyFocused`).
#[derive(Default)]
pub struct Cards {
    pub list: Vec<Card>,
    pub hovered: bool,
    /// the card whose reply line has the keys
    pub focus: Option<String>,
    pub hover_act: Option<(String, Act)>,
}

impl Cards {
    fn held(&self, c: &Card) -> bool {
        banner::card_held(
            self.hovered,
            self.focus.as_deref() == Some(c.card.id.as_str()),
            c.draft.text(),
            c.reply == Reply::Sending,
        )
    }
}

fn expire_later() {
    later::after("cards", 500, |app| {
        let now = Instant::now();
        let cards = &mut app.model.cards;
        let held: Vec<bool> = cards.list.iter().map(|c| cards.held(c)).collect();
        for (c, h) in cards.list.iter_mut().zip(held) {
            if h {
                c.shown_at = now;
            }
        }
        let before = cards.list.len();
        cards
            .list
            .retain(|c| now.duration_since(c.shown_at) < Duration::from_millis(banner::BANNER_MS));
        if cards.list.len() != before {
            if let Some(f) = &cards.focus
                && !cards.list.iter().any(|c| &c.card.id == f)
            {
                cards.focus = None;
            }
            app.model.touch();
        }
        if !app.model.cards.list.is_empty() {
            expire_later();
        }
    });
}

/// Esc: a reply line gives the keys back; else the cards go. True when it did either.
pub fn escape(m: &mut BarModel) -> bool {
    if m.cards.focus.take().is_some() {
        m.touch();
        return true;
    }
    if !m.cards.list.is_empty() {
        m.cards.list.clear();
        m.touch();
        return true;
    }
    false
}

/// The bar went away: replies give the keys back.
pub fn will_hide(m: &mut BarModel) {
    m.cards.focus = None;
    m.cards.hovered = false;
}

// ---- sending ----

pub fn has_name() -> bool {
    with(|s| !s.m.name.is_empty() && !s.m.key.is_empty())
}

/// What the answer slot says while `message …` is typed.
pub fn line_hint(to: &str, name: &str, text: &str) -> String {
    let line = MessageLine {
        to: to.into(),
        name: name.into(),
        text: text.into(),
    };
    with(|s| {
        let found = recipient_found(s, name);
        core::message_hint(&line, &s.m, true, found)
    })
}

/// Whether the name is anyone's: None until the relay has said, and whenever it can't (no name of your own, the
/// relay out of reach), so the bar never calls a name missing that the relay didn't.
fn recipient_found(s: &mut State, name: &str) -> Option<bool> {
    if name.is_empty() || s.m.name.is_empty() || s.link == Link::Offline {
        return None;
    }
    s.names.cached(name, now_ms())
}

/// Asks the relay about `name` once typing has rested, unless it's known or already being asked.
pub fn check_name(name: &str) {
    let ask = with(|s| {
        !name.is_empty()
            && !s.m.name.is_empty()
            && s.link != Link::Offline
            && recipient_found(s, name).is_none()
    });
    if !ask {
        later::cancel("name-check");
        return;
    }
    let name = name.to_string();
    later::after("name-check", CHECK_DELAY_MS as u32, move |_| {
        let body = with(|s| {
            s.names
                .start(&name, now_ms())
                .then(|| json!({ "from": s.m.name, "key": s.m.key, "name": name }))
        });
        if let Some(body) = body {
            post_later("/lookup", body, move |status| {
                let found = match status.as_str() {
                    "found" => Some(true),
                    "unknown" => Some(false),
                    _ => None,
                };
                Event::Looked(name, found)
            });
        }
    });
}

fn send_body(s: &State, to: &str, text: &str) -> Value {
    json!({ "from": s.m.name, "key": s.m.key, "to": to, "text": text })
}

fn status_of(s: &str) -> SendStatus {
    match s {
        "delivered" => SendStatus::Delivered,
        "offline" => SendStatus::Offline,
        "unknown" => SendStatus::Unknown,
        "not-yours" => SendStatus::NotYours,
        "too-many" => SendStatus::TooMany,
        "bad" => SendStatus::Bad,
        _ => SendStatus::Network,
    }
}

/// ↵ on `message <name> <text>`.
pub fn send_line(app: &mut App, name: String, text: String) {
    let text = core::clean_message_text(&Value::String(text));
    let text = text.trim().to_string();
    if text.is_empty() {
        return;
    }
    app.model.hint = Some(format!("sending to {name}…"));
    app.model.touch();
    let body = with(|s| send_body(s, &name, &text));
    post_later("/send", body, move |status| {
        Event::Sent(Sent::Line, name, status)
    });
}

fn line_sent(app: &mut App, name: &str, status: &str) {
    let status = status_of(status);
    with(|s| {
        s.names.note_send(name, status, now_ms());
        if status == SendStatus::Delivered {
            s.m = core::with_recent(&s.m, name);
            save(&s.m);
        }
    });
    let said = core::send_status_text(status, name);
    qlog!("messaging: sent to {name}: {status:?}");
    if status == SendStatus::Delivered {
        crate::commands::clear_line(app, false);
    }
    app.model.hint = Some(said);
    app.model.touch();
}

fn send_reply(app: &mut App, id: &str) {
    let Some(card) = app.model.cards.list.iter_mut().find(|c| c.card.id == id) else {
        return;
    };
    let said = banner::reply_text(card.draft.text());
    if said.is_empty() || card.reply == Reply::Sending {
        return;
    }
    card.reply = Reply::Sending;
    let to = card.card.from.clone();
    app.model.touch();
    let body = with(|s| send_body(s, &to, &said));
    let id = id.to_string();
    post_later("/send", body, move |status| {
        Event::Sent(Sent::Reply(id), to, status)
    });
}

fn reply_sent(app: &mut App, id: &str, to: &str, status: &str) {
    let status = status_of(status);
    with(|s| s.names.note_send(to, status, now_ms()));
    let Some(card) = app.model.cards.list.iter_mut().find(|c| c.card.id == id) else {
        return;
    };
    if status == SendStatus::Delivered {
        card.reply = Reply::Sent;
        card.draft.reset("");
        with(|s| {
            s.m = core::with_recent(&s.m, to);
            save(&s.m);
        });
        let id = id.to_string();
        later::after("reply-sent", banner::SENT_MS as u32, move |app| {
            let cards = &mut app.model.cards;
            cards.list.retain(|c| c.card.id != id);
            if cards.focus.as_deref() == Some(id.as_str()) {
                cards.focus = None;
            }
            app.model.touch();
        });
    } else {
        card.reply = Reply::Failed(core::send_status_text(status, to));
    }
    app.model.touch();
}

fn answer_ask(app: &mut App, id: &str, allow: bool) {
    let Some(i) = app.model.cards.list.iter().position(|c| c.card.id == id) else {
        return;
    };
    let who = app.model.cards.list.remove(i).card.from;
    app.model.touch();
    let said = with(|s| {
        s.m = if allow {
            core::allow_sender(&s.m, &who)
        } else {
            core::block_sender(&s.m, &who)
        };
        save(&s.m);
        s.inbox.retain(|c| !(c.from == who && (c.ask || !allow)));
        s.asking.remove(&who)
    });
    if allow && let Some(said) = said {
        deliver(app, &who, &said);
    }
}

// ---- the settings window's side (Messages.swift's name, lists and status) ----

/// What settings says under the name.
pub fn status() -> String {
    with(|s| {
        if !s.note.is_empty() {
            return s.note.clone();
        }
        if s.m.name.is_empty() {
            return "Off. Set a name to send and receive messages.".into();
        }
        let n = &s.m.name;
        match s.link {
            Link::Ready => {
                format!("Receiving as {n}. Type message <name> <text> in the bar to send.")
            }
            Link::Taken => format!("Someone else has {n} now. Pick another name."),
            Link::Offline => format!("Receiving as {n} once the relay can be reached."),
            _ => format!("Connecting as {n}…"),
        }
    })
}

/// Your name ("" when off), whether a claim is out, and the lists.
pub fn name() -> String {
    with(|s| s.m.name.clone())
}

pub fn busy() -> bool {
    with(|s| s.busy)
}

pub fn link() -> Link {
    with(|s| s.link)
}

pub fn accepted() -> Vec<String> {
    with(|s| s.m.accepted.clone())
}

pub fn blocked() -> Vec<String> {
    with(|s| s.m.blocked.clone())
}

/// Names that asked and haven't been answered.
pub fn requests() -> Vec<String> {
    with(|s| s.asking.keys().cloned().collect())
}

/// Holds a new name on the relay first; the old one is let go only once the new one is this app's. An empty name
/// turns messaging off.
pub fn set_name(raw: &str) {
    if raw.trim().is_empty() {
        turn_off();
        return;
    }
    let next = core::clean_name(&Value::String(raw.to_string()));
    let body = with(|s| {
        if next.is_empty() {
            s.note = "A name is 2–20 letters, digits, - or _.".into();
            return None;
        }
        if next == s.m.name && s.link != Link::Taken {
            s.note.clear();
            return None;
        }
        if s.m.key.is_empty() {
            s.m.key = new_key();
            save(&s.m);
        }
        s.busy = true;
        s.note.clear();
        Some(json!({ "name": next, "key": s.m.key }))
    });
    if let Some(body) = body {
        post_later("/claim", body, move |status| Event::Claimed(next, status));
    }
}

fn claimed(next: &str, status: &str) {
    let reconnect = with(|s| {
        s.busy = false;
        match status {
            "ok" => {
                let old = std::mem::replace(&mut s.m.name, next.to_string());
                save(&s.m);
                Some(
                    (!old.is_empty() && old != next)
                        .then(|| json!({ "name": old, "key": s.m.key })),
                )
            }
            "taken" => {
                s.note = format!("{next} is taken. Try another.");
                None
            }
            "too-many" => {
                s.note = "Too many tries. Wait a minute.".into();
                None
            }
            _ => {
                s.note = "Couldn’t reach the relay. Try again.".into();
                None
            }
        }
    });
    if let Some(release) = reconnect {
        qlog!("messaging: claimed {next}");
        if let Some(body) = release {
            post_later("/release", body, |_| Event::Looked(String::new(), None));
        }
        connect();
    }
}

/// Lets the name go and stops receiving.
pub fn turn_off() {
    let body = with(|s| {
        s.note.clear();
        if s.m.name.is_empty() {
            return None;
        }
        let body = json!({ "name": s.m.name, "key": s.m.key });
        s.m.name.clear();
        s.asking.clear();
        s.inbox.clear();
        save(&s.m);
        Some(body)
    });
    if let Some(body) = body {
        post_later("/release", body, |_| Event::Looked(String::new(), None));
        connect();
    }
}

/// Settings' Allow / Block / Remove / Unblock (the bar's cards answer asks themselves).
pub fn allow(who: &str) {
    with(|s| {
        s.m = core::allow_sender(&s.m, who);
        s.asking.remove(who);
        save(&s.m);
    });
}

pub fn block(who: &str) {
    with(|s| {
        s.m = core::block_sender(&s.m, who);
        s.asking.remove(who);
        s.inbox.retain(|c| c.from != who);
        save(&s.m);
    });
}

pub fn forget(who: &str) {
    with(|s| {
        s.m = core::forget_sender(&s.m, who);
        save(&s.m);
    });
}

/// The settings window's Messages page (settings_window.rs `MessagesHook`): the name and its status, turning off,
/// the requests waiting, and the allowed and blocked names.
fn settings_items(_app: &App) -> Vec<crate::settings_window::Item> {
    use crate::settings_window::{Control, Item, row};
    let button = |label: &str| Control::Button {
        label: label.into(),
        enabled: true,
    };
    let name = name();
    let mut out = vec![row(
        "name",
        "Your name",
        &status(),
        Control::Text {
            value: name.clone(),
            placeholder: "2–20 letters, digits, - or _".into(),
        },
    )];
    if !name.is_empty() {
        out.push(row(
            "off",
            "Turn off messages",
            "Lets go of your name on the relay and stops receiving.",
            Control::Button {
                label: "Turn Off".into(),
                enabled: !busy(),
            },
        ));
    }
    let asks = requests();
    if !asks.is_empty() {
        out.push(Item::Heading("Requests".into()));
        for who in &asks {
            out.push(row(
                &format!("ask:{who}"),
                who,
                "wants to message you",
                Control::Seg {
                    options: vec![("allow".into(), "Allow".into()), ("block".into(), "Block".into())],
                    value: String::new(),
                },
            ));
        }
    }
    let allowed = accepted();
    if !allowed.is_empty() {
        out.push(Item::Heading("Allowed".into()));
        for who in &allowed {
            out.push(row(&format!("forget:{who}"), who, "can message you", button("Remove")));
        }
    }
    let blocks = blocked();
    if !blocks.is_empty() {
        out.push(Item::Heading("Blocked".into()));
        for who in &blocks {
            out.push(row(&format!("forget:{who}"), who, "messages are dropped unseen", button("Unblock")));
        }
    }
    if name.is_empty() && allowed.is_empty() && blocks.is_empty() {
        out.push(Item::Note(
            "With a name set, type message <name> <text> in the bar to send. Someone new's first message asks you first."
                .into(),
            false,
        ));
    }
    out
}

/// A change on the Messages page: the row's id and its new value.
fn settings_changed(app: &mut App, id: &str, value: &Value) {
    match id {
        "name" => set_name(value.as_str().unwrap_or_default()),
        "off" => turn_off(),
        _ => {
            if let Some(who) = id.strip_prefix("ask:") {
                match value.as_str() {
                    Some("allow") => allow(who),
                    Some("block") => block(who),
                    _ => {}
                }
            } else if let Some(who) = id.strip_prefix("forget:") {
                forget(who);
            }
        }
    }
    crate::settings_window::refresh(app);
}

// ---- the cards' view (MessageCards in BarView.swift) ----

const PAD_TOP: f32 = 8.0;
const HEAD: f32 = 22.0;
const FIELD_H: f32 = 24.0;
const ACT_H: f32 = 22.0;
const TEXT_LINES: f32 = 4.0;

struct Placed {
    top: f32,
    height: f32,
    from: Option<IDWriteTextLayout>,
    asks: Option<IDWriteTextLayout>,
    acts: Vec<(Act, Rect, IDWriteTextLayout)>,
    text: Option<(IDWriteTextLayout, Rect)>,
    field: Rect,
    status: Option<(IDWriteTextLayout, Rect)>,
}

fn bold(gfx: &Gfx) -> Option<IDWriteTextFormat> {
    with(|s| {
        if s.bold.is_none() {
            s.bold = crate::render::format(
                &gfx.dw,
                &gfx.fonts.text_family,
                size::BODY,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
            )
            .ok();
        }
        s.bold.clone()
    })
}

fn wrapped(gfx: &Gfx, text: &str, w: f32) -> Option<(IDWriteTextLayout, f32)> {
    let wide: Vec<u16> = text.encode_utf16().collect();
    // SAFETY: wide outlives the call; the format is live
    let l = unsafe {
        gfx.dw
            .CreateTextLayout(&wide, &gfx.fonts.body, w.max(1.0), 10_000.0)
    }
    .ok()?;
    let (_, h) = measure(&l);
    let line = 17.5;
    Some((l, h.min(line * TEXT_LINES)))
}

/// Every card's place, top to bottom, panel coordinates.
fn place(gfx: &Gfx, cards: &Cards, width: f32) -> Vec<Placed> {
    let inner = width - 2.0 * size::GUTTER;
    let bold = bold(gfx);
    let mut y = 0.0;
    let mut out = Vec::new();
    for (i, c) in cards.list.iter().enumerate() {
        if i > 0 {
            y += 1.0;
        }
        let top = y;
        let head_y = top + PAD_TOP;
        let from = bold
            .as_ref()
            .and_then(|f| gfx.layout(&c.card.from, f, 0.0, HEAD));
        let mut acts = Vec::new();
        let mut right = width - size::GUTTER;
        let names: &[(Act, &str)] = if c.card.ask {
            &[(Act::Block, "Block"), (Act::Allow, "Allow")]
        } else {
            &[(Act::Dismiss, "×")]
        };
        for (act, label) in names {
            if let Some(l) = gfx.layout(label, &gfx.fonts.small, 0.0, ACT_H) {
                let w = measure(&l).0 + 16.0;
                right -= w;
                acts.push((*act, Rect::new(right, head_y, w, ACT_H), l));
                right -= 8.0;
            }
        }
        let asks = c
            .card
            .ask
            .then(|| gfx.layout("wants to message you", &gfx.fonts.body, 0.0, HEAD))
            .flatten();
        let mut height = PAD_TOP + HEAD;
        let mut text = None;
        let mut field = Rect::default();
        let mut status = None;
        if c.card.ask {
            height += 8.0;
        } else {
            height += 4.0;
            if let Some((l, h)) = wrapped(gfx, &c.card.text, inner) {
                text = Some((l, Rect::new(size::GUTTER, top + height, inner, h)));
                height += h;
            }
            height += 4.0 + 4.0;
            let words = match &c.reply {
                Reply::Idle => None,
                Reply::Sending => Some("sending…".to_string()),
                Reply::Sent => Some("sent".to_string()),
                Reply::Failed(note) => Some(note.clone()),
            };
            let mut fw = inner;
            if let Some(words) = words
                && let Some(l) = gfx.layout_in(
                    &words,
                    &gfx.fonts.small,
                    inner * 0.5,
                    FIELD_H,
                    DWRITE_TEXT_ALIGNMENT_LEADING,
                )
            {
                let w = measure(&l).0.min(inner * 0.5);
                fw = inner - w - 8.0;
                status = Some((
                    l,
                    Rect::new(size::GUTTER + fw + 8.0, top + height, w, FIELD_H),
                ));
            }
            field = Rect::new(size::GUTTER, top + height, fw, FIELD_H);
            height += FIELD_H + 12.0;
        }
        out.push(Placed {
            top,
            height,
            from,
            asks,
            acts,
            text,
            field,
            status,
        });
        y += height;
    }
    out
}

/// The panel's height under the composer (0 without cards).
pub fn height(gfx: &Gfx, m: &BarModel, width: f32) -> f32 {
    if m.cards.list.is_empty() {
        return 0.0;
    }
    place(gfx, &m.cards, width)
        .last()
        .map_or(0.0, |p| p.top + p.height)
        + 1.0
}

pub fn paint(c: &Canvas, m: &BarModel, r: Rect) {
    if m.cards.list.is_empty() || r.h <= 0.0 {
        return;
    }
    let t = c.theme;
    c.fill(r, t.bg_tape);
    c.fill(Rect::new(r.x, r.y, r.w, 1.0), t.line);
    let y0 = r.y + 1.0;
    let control = if t.dark {
        rgb(0x58585c, 1.0)
    } else {
        rgb(0xffffff, 1.0)
    };
    for (card, p) in m.cards.list.iter().zip(place(c.gfx, &m.cards, r.w)) {
        if p.top > 0.0 {
            c.fill(Rect::new(r.x, y0 + p.top - 1.0, r.w, 1.0), t.line);
        }
        let head_y = y0 + p.top + PAD_TOP;
        let mut x = size::GUTTER;
        if let Some(l) = &p.from {
            c.text(l, x, head_y, t.ink);
            x += measure(l).0 + 8.0;
        }
        if let Some(l) = &p.asks {
            c.text(l, x, head_y, t.muted);
        }
        for (act, ar, l) in &p.acts {
            let ar = Rect::new(ar.x, ar.y + y0, ar.w, ar.h);
            let hovered = m.cards.hover_act.as_ref() == Some(&(card.card.id.clone(), *act));
            if hovered {
                c.fill_round(ar, size::RADIUS_S, t.fill_selected);
            }
            c.text(
                l,
                ar.x + 8.0,
                ar.y,
                if hovered { t.answer } else { t.muted },
            );
        }
        if let Some((l, tr)) = &p.text {
            c.push_clip(Rect::new(tr.x, tr.y + y0, tr.w, tr.h));
            c.text(l, tr.x, tr.y + y0, t.answer);
            c.pop_clip();
        }
        if card.card.ask {
            continue;
        }
        let f = Rect::new(p.field.x, p.field.y + y0, p.field.w, p.field.h);
        let focused = m.cards.focus.as_deref() == Some(card.card.id.as_str());
        // the border (accent 2 when it has the keys, a faint 0.5 otherwise), then the control's fill
        if focused {
            c.fill_round(f, size::RADIUS_S, t.accent);
            c.fill_round(f.inset(2.0, 2.0), size::RADIUS_S - 2.0, control);
        } else {
            c.fill_round(f, size::RADIUS_S, rgb(0x000000, 0.12));
            c.fill_round(f.inset(0.5, 0.5), size::RADIUS_S - 0.5, control);
        }
        paint_draft(c, card, f, focused);
        if let Some((l, sr)) = &p.status {
            let ink = if card.reply == Reply::Sent {
                t.accent
            } else {
                t.muted
            };
            c.text(l, sr.x, sr.y + y0, ink);
        }
    }
}

fn paint_draft(c: &Canvas, card: &Card, f: Rect, focused: bool) {
    let t = c.theme;
    let inner = f.inset(8.0, 0.0);
    c.push_clip(inner);
    let draft = card.draft.text();
    if draft.is_empty()
        && let Some(l) = c.gfx.layout(
            &format!("Reply to {}", card.card.from),
            &c.gfx.fonts.small,
            0.0,
            f.h,
        )
    {
        c.text(&l, inner.x, inner.y, t.muted);
    }
    let Some(l) = c.gfx.layout(draft, &c.gfx.fonts.small, 0.0, f.h) else {
        c.pop_clip();
        return;
    };
    let caret = caret_x(&l, card.draft.to_utf16(card.draft.caret()));
    let scroll = (caret - inner.w + 4.0).max(0.0);
    let x0 = inner.x - scroll;
    if focused && card.draft.has_selection() {
        let (a, b) = card.draft.selection();
        let (xa, xb) = (
            caret_x(&l, card.draft.to_utf16(a)),
            caret_x(&l, card.draft.to_utf16(b)),
        );
        c.fill(
            Rect::new(x0 + xa, f.y + 4.0, xb - xa, f.h - 8.0),
            t.selection,
        );
    }
    if !draft.is_empty() {
        c.text(&l, x0, inner.y, t.ink);
    }
    if focused {
        c.line(
            x0 + caret + 0.5,
            f.y + 5.0,
            x0 + caret + 0.5,
            f.bottom() - 5.0,
            1.0,
            t.ink,
        );
    }
    c.pop_clip();
}

/// What's under a point of the panel (`r` is the panel's rect in the bar).
fn under(gfx: &Gfx, m: &BarModel, r: Rect, x: f32, y: f32) -> Option<(String, Act)> {
    if !r.contains(x, y) {
        return None;
    }
    let y0 = r.y + 1.0;
    for (card, p) in m.cards.list.iter().zip(place(gfx, &m.cards, r.w)) {
        for (act, ar, _) in &p.acts {
            if Rect::new(ar.x, ar.y + y0, ar.w, ar.h).contains(x, y) {
                return Some((card.card.id.clone(), *act));
            }
        }
        if !card.card.ask
            && Rect::new(p.field.x, p.field.y + y0, p.field.w, p.field.h).contains(x, y)
        {
            return Some((card.card.id.clone(), Act::Field));
        }
    }
    None
}

/// The pointer moved over the bar (None: it left).
pub fn hover(app: &mut App, at: Option<(f32, f32)>) {
    let Some(r) = app.bar.layout.cards else {
        if app.model.cards.hovered {
            app.model.cards.hovered = false;
            app.model.touch();
        }
        return;
    };
    let over = at.is_some_and(|(x, y)| r.contains(x, y));
    let act = at
        .and_then(|(x, y)| under(&app.gfx, &app.model, r, x, y))
        .filter(|(_, a)| *a != Act::Field);
    let cards = &mut app.model.cards;
    if cards.hovered != over || cards.hover_act != act {
        cards.hovered = over;
        cards.hover_act = act;
        app.model.touch();
    }
}

/// A click in the bar: true when it was the cards'.
pub fn click(app: &mut App, x: f32, y: f32) -> bool {
    let Some(r) = app.bar.layout.cards else {
        return false;
    };
    if !r.contains(x, y) {
        if app.model.cards.focus.take().is_some() {
            app.model.touch();
        }
        return false;
    }
    match under(&app.gfx, &app.model, r, x, y) {
        Some((id, Act::Allow)) => answer_ask(app, &id, true),
        Some((id, Act::Block)) => answer_ask(app, &id, false),
        Some((id, Act::Dismiss)) => {
            app.model.cards.list.retain(|c| c.card.id != id);
            if app.model.cards.focus.as_deref() == Some(id.as_str()) {
                app.model.cards.focus = None;
            }
        }
        Some((id, Act::Field)) => {
            // the caret where the click was
            let y0 = r.y + 1.0;
            let placed = place(&app.gfx, &app.model.cards, r.w);
            if let Some((card, p)) = app
                .model
                .cards
                .list
                .iter_mut()
                .zip(placed)
                .find(|(c, _)| c.card.id == id)
                && let Some(l) =
                    app.gfx
                        .layout(card.draft.text(), &app.gfx.fonts.small, 0.0, FIELD_H)
            {
                let pos = hit(&l, x - p.field.x - 8.0, y - p.field.y - y0);
                card.draft.set_caret_utf16(pos, false);
            }
            app.model.cards.focus = Some(id);
        }
        None => {}
    }
    app.model.touch();
    true
}

/// A key while a reply line has the keys: true when it was the reply's.
pub fn key(app: &mut App, vk: u32, mods: Mods) -> bool {
    let Some(id) = app.model.cards.focus.clone() else {
        return false;
    };
    let host = app.bar.hwnd;
    let ctrl = mods.ctrl && !mods.alt;
    let shift = mods.shift;
    if vk == 0x0D {
        send_reply(app, &id);
        return true;
    }
    if vk == 0x1B {
        app.model.cards.focus = None;
        app.model.touch();
        return true;
    }
    let Some(card) = app.model.cards.list.iter_mut().find(|c| c.card.id == id) else {
        app.model.cards.focus = None;
        return false;
    };
    if card.reply == Reply::Sent {
        return true;
    }
    let f = &mut card.draft;
    match vk {
        0x25 => f.left(ctrl, shift),
        0x27 => f.right(ctrl, shift),
        0x24 => f.home(shift),
        0x23 => f.end(shift),
        0x08 => {
            f.backspace(ctrl);
        }
        0x2E => {
            f.delete(ctrl);
        }
        0x41 if ctrl => f.select_all(),
        0x43 if ctrl => {
            if f.has_selection() {
                app.model.to_clipboard = Some(f.selected_text().to_string());
            }
        }
        0x58 if ctrl => {
            if let Some(cut) = f.cut() {
                app.model.to_clipboard = Some(cut);
            }
        }
        0x56 if ctrl => {
            if let Some(text) = crate::clip::get(host) {
                f.paste(&text);
            }
        }
        0x5A if ctrl && shift => {
            f.redo();
        }
        0x5A if ctrl => {
            f.undo();
        }
        0x59 if ctrl => {
            f.redo();
        }
        // ↑ ↓ Tab stay in the reply
        0x26 | 0x28 | 0x09 => {}
        _ => return false,
    }
    if card.reply != Reply::Sending {
        card.reply = Reply::Idle;
    }
    app.model.touch();
    true
}

/// WM_CHAR while a reply line has the keys: true when it took it.
pub fn char16(app: &mut App, unit: u16) -> bool {
    let Some(id) = app.model.cards.focus.clone() else {
        return false;
    };
    if let Some(card) = app.model.cards.list.iter_mut().find(|c| c.card.id == id) {
        if card.reply != Reply::Sent && card.draft.text().encode_utf16().count() < core::TEXT_MAX {
            card.draft.char16(unit);
        }
        app.model.touch();
        return true;
    }
    false
}

/// For the hint colours of a message line's name: whether the relay says it's someone's.
pub fn found(name: &str) -> Option<bool> {
    with(|s| recipient_found(s, name))
}

/// The accent the composer's caret takes while a reply has the keys: none.
pub fn reply_has_keys(m: &BarModel) -> bool {
    m.cards.focus.is_some()
}
