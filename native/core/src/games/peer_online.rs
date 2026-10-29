//! `src/lib/peerOnline.ts`: the link to another Q Calc anywhere. both sides open a websocket to the relay (relay/
//! in this repo). the lobby watches the relay's list of rooms for its game; a host opens a room, open or locked
//! with a code, and a guest joins one from the list. the relay forwards game messages without reading them.
//!
//! The TS owns its sockets and timers; this port is the same machine with neither. The shell opens, writes and
//! closes sockets as `SocketCmd`s say, reports what they do (`opened`, `message`, `closed`), and calls `tick` at
//! `next_deadline`. Every call takes `now`, ms since the epoch (the TS's `Date.now()`); a timer fires at its own
//! time, so a late tick plays out as the TS's timers would have.

use super::peer::{CloseReason, PeerEvent, Who, parse_peer_event};
use super::{Side, parse};

pub const RELAY_URL: &str = "wss://qcalc-relay.maxconine.workers.dev";
pub const ONLINE_CODE_LENGTH: usize = 6;
const PING_MS: f64 = 10_000.0;
// two missed pongs: the connection is gone even if the socket hasn't noticed
const DEAD_MS: f64 = 25_000.0;
// a socket still connecting after this long isn't going to
const CONNECT_MS: f64 = 10_000.0;
// a dropped link mid-game tries for its seat back this long, a little under the relay's hold
const RESUME_MS: f64 = 590_000.0;
const RESUME_RETRY_MS: f64 = 1_000.0;
const RESUME_RETRY_MAX: f64 = 5_000.0;
// what's sent while reconnecting goes once the seat is back
const MAX_OUTBOX: usize = 256;
// a close the relay reads as reconnecting, not leaving (it ends the game only on 1000)
const RECONNECTING: u16 = 4000;
// the room list tries again after this long when it can't reach the relay, up to WATCH_RETRY_MAX
const WATCH_RETRY_MS: f64 = 3_000.0;
const WATCH_RETRY_MAX: f64 = 30_000.0;

/// What the shell does with its sockets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SocketCmd {
    Open { id: u64, url: String },
    Send { id: u64, text: String },
    Close { id: u64, code: u16 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Sock {
    id: u64,
    // readyState 1
    open: bool,
}

#[derive(Debug)]
pub struct OnlinePeer {
    base: String,
    cmds: Vec<SocketCmd>,
    events: Vec<PeerEvent>,
    next_id: u64,
    ws: Option<Sock>,
    watcher: Option<Sock>,
    watcher_heard: bool,
    watching: Option<String>,
    watch_timer: Option<f64>,
    watch_ping: Option<f64>,
    watch_wait: f64,
    // a room list has come down this session, so the relay has them
    listed: bool,
    // the list has never come: host and join the way relays before room lists do, by code alone
    code_only: bool,
    connect_timer: Option<f64>,
    // the game is on: closing now is `lost`, not `network`
    paired: bool,
    // the relay said something, so a close before pairing is the relay's doing, not a failed connection
    heard: Option<f64>,
    ping_timer: Option<f64>,
    // the seat to take back if the link drops mid-game, and while reconnecting, when to give up
    seat: Option<(String, String)>,
    resume_until: f64,
    resume_timer: Option<f64>,
    // the TS's `this.resumeTimer` handle: set when a retry is scheduled, kept after it fires, cleared by name
    resume_handle: bool,
    resume_wait: f64,
    outbox: Vec<String>,
    now: f64,
}

/// `encodeURIComponent`.
pub fn encode_uri_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `new URLSearchParams(pairs).toString()`.
pub fn search_params(pairs: &[(&str, &str)]) -> String {
    let enc = |s: &str| {
        let mut out = String::new();
        for b in s.bytes() {
            if b.is_ascii_alphanumeric() || b"*-._".contains(&b) {
                out.push(b as char);
            } else if b == b' ' {
                out.push('+');
            } else {
                out.push_str(&format!("%{b:02X}"));
            }
        }
        out
    };
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
        .collect::<Vec<_>>()
        .join("&")
}

impl OnlinePeer {
    pub fn new(base: &str) -> OnlinePeer {
        OnlinePeer {
            base: base.trim_end_matches('/').to_string(),
            cmds: vec![],
            events: vec![],
            next_id: 1,
            ws: None,
            watcher: None,
            watcher_heard: false,
            watching: None,
            watch_timer: None,
            watch_ping: None,
            watch_wait: WATCH_RETRY_MS,
            listed: false,
            code_only: false,
            connect_timer: None,
            paired: false,
            heard: None,
            ping_timer: None,
            seat: None,
            resume_until: 0.0,
            resume_timer: None,
            resume_handle: false,
            resume_wait: RESUME_RETRY_MS,
            outbox: vec![],
            now: 0.0,
        }
    }

    pub const LABEL: &'static str = "online";
    pub const PEER_WORD: &'static str = "computer";

    /// The sockets to open, write and close, in order.
    pub fn take_commands(&mut self) -> Vec<SocketCmd> {
        std::mem::take(&mut self.cmds)
    }

    /// What the page hears, in order.
    pub fn take_events(&mut self) -> Vec<PeerEvent> {
        std::mem::take(&mut self.events)
    }

    /// When the next timer is due, if any.
    pub fn next_deadline(&self) -> Option<f64> {
        [
            self.ping_timer,
            self.connect_timer,
            self.watch_ping,
            self.watch_timer,
            self.resume_timer,
        ]
        .into_iter()
        .flatten()
        .reduce(f64::min)
    }

    /// Runs every timer due by `now`, each at its own time.
    pub fn tick(&mut self, now: f64) {
        while let Some(at) = self.next_deadline().filter(|at| *at <= now) {
            self.now = self.now.max(at);
            if self.ping_timer == Some(at) {
                self.ping_timer = Some(at + PING_MS);
                self.ping_fired();
            } else if self.connect_timer == Some(at) {
                self.connect_timer = None;
                if matches!(self.ws, Some(Sock { open: false, .. })) {
                    if self.seat.is_some() {
                        self.lose();
                    } else {
                        self.fail(CloseReason::Network);
                    }
                }
            } else if self.watch_ping == Some(at) {
                self.watch_ping = Some(at + PING_MS * 3.0);
                if let Some(Sock { id, open: true }) = self.watcher {
                    self.cmds.push(SocketCmd::Send {
                        id,
                        text: "ping".into(),
                    });
                }
            } else if self.watch_timer == Some(at) {
                self.watch_timer = None;
                self.open_watcher();
            } else if self.resume_timer == Some(at) {
                self.resume_timer = None;
                self.resume();
            }
        }
        self.now = self.now.max(now);
    }

    fn at(&mut self, now: f64) {
        self.tick(now);
    }

    fn ping_fired(&mut self) {
        let Some(ws) = self.ws else { return };
        if self.heard.is_some_and(|h| self.now - h > DEAD_MS) {
            if self.seat.is_some() {
                self.lose();
            } else {
                self.fail(CloseReason::Lost);
            }
        } else if ws.open {
            self.cmds.push(SocketCmd::Send {
                id: ws.id,
                text: "ping".into(),
            });
        }
    }

    fn emit(&mut self, e: PeerEvent) {
        self.events.push(e);
    }

    fn new_socket(&mut self, url: String) -> Sock {
        let id = self.next_id;
        self.next_id += 1;
        self.cmds.push(SocketCmd::Open { id, url });
        Sock { id, open: false }
    }

    fn connect(&mut self, path: &str, close_code: u16) {
        self.drop_ws(close_code);
        let url = format!("{}{path}", self.base);
        self.ws = Some(self.new_socket(url));
        self.heard = None;
        self.ping_timer = Some(self.now + PING_MS);
        self.connect_timer = Some(self.now + CONNECT_MS);
    }

    // the link dropped mid-game: the relay holds the seat, so keep asking for it back until the time's up
    fn lose(&mut self) {
        let first = self.resume_until == 0.0;
        self.drop_ws(RECONNECTING);
        if first {
            self.resume_until = self.now + RESUME_MS;
            self.resume_wait = RESUME_RETRY_MS;
            self.emit(PeerEvent::Away { who: Who::SelfSide });
        }
        self.resume_timer = None;
        if self.now >= self.resume_until {
            self.forget();
            self.emit(PeerEvent::Closed {
                reason: CloseReason::Lost,
            });
            return;
        }
        self.resume_timer = Some(self.now + if first { 0.0 } else { self.resume_wait });
        self.resume_handle = true;
        if !first {
            self.resume_wait = RESUME_RETRY_MAX.min(self.resume_wait * 2.0);
        }
    }

    fn resume(&mut self) {
        let Some((room, token)) = self.seat.clone() else {
            return;
        };
        let q = search_params(&[("token", &token)]);
        self.connect(
            &format!("/back/{}?{q}", encode_uri_component(&room)),
            RECONNECTING,
        );
    }

    /// The window hid mid-game: the relay holds the seat while nothing here runs, and the game waits for both.
    pub fn step_away(&mut self, now: f64) {
        self.at(now);
        if self.seat.is_none() || !self.paired {
            return;
        }
        self.resume_timer = None;
        self.resume_handle = false;
        self.drop_ws(RECONNECTING);
        self.resume_until = self.now + RESUME_MS;
        self.emit(PeerEvent::Away { who: Who::SelfSide });
    }

    /// Shown again: one try now, however long it's been; the relay says if the game ended meanwhile.
    pub fn come_back(&mut self, now: f64) {
        self.at(now);
        if self.seat.is_none() || self.resume_until == 0.0 || self.ws.is_some() {
            return;
        }
        self.resume_until = self.resume_until.max(self.now + RESUME_RETRY_MAX);
        self.resume_wait = RESUME_RETRY_MS;
        self.resume();
    }

    /// Back sooner than the next try: the computer woke or came back online.
    pub fn retry_now(&mut self, now: f64) {
        self.at(now);
        // a stepped-away seat waits for come_back, not for the network
        if self.seat.is_none()
            || self.resume_until == 0.0
            || !self.resume_handle
            || self.ws.is_some_and(|w| w.open)
        {
            return;
        }
        self.resume_timer = None;
        self.resume_wait = RESUME_RETRY_MS;
        self.resume();
    }

    // the game is over for this side: no seat to go back to
    fn forget(&mut self) {
        self.seat = None;
        self.resume_until = 0.0;
        self.resume_timer = None;
        self.resume_handle = false;
        self.outbox.clear();
    }

    /// The socket finished connecting (readyState 1).
    pub fn opened(&mut self, id: u64, now: f64) {
        self.at(now);
        for s in [&mut self.ws, &mut self.watcher].into_iter().flatten() {
            if s.id == id {
                s.open = true;
            }
        }
    }

    /// A text frame from the relay.
    pub fn message(&mut self, id: u64, text: &str, now: f64) {
        self.at(now);
        if self.ws.is_some_and(|w| w.id == id) {
            self.receive(text);
        } else if self.watcher.is_some_and(|w| w.id == id) {
            self.watcher_message(text);
        }
    }

    /// The socket closed or failed, whoever closed it.
    pub fn closed(&mut self, id: u64, now: f64) {
        self.at(now);
        if self.ws.is_some_and(|w| w.id == id) {
            if self.seat.is_some() {
                self.lose();
            } else {
                self.fail(if self.paired || self.heard.is_some() {
                    CloseReason::Lost
                } else {
                    CloseReason::Network
                });
            }
        } else if self.watcher.is_some_and(|w| w.id == id) {
            self.watcher_closed();
        }
    }

    fn receive(&mut self, text: &str) {
        self.heard = Some(self.now);
        if text == "pong" {
            return;
        }
        if let Some(data) = text.strip_prefix('.') {
            if self.paired {
                self.emit(PeerEvent::Message {
                    data: data.to_string(),
                });
            }
            return;
        }
        let Some(raw) = text.strip_prefix('!').and_then(parse) else {
            return;
        };
        let Some(e) = parse_peer_event(&raw, ONLINE_CODE_LENGTH) else {
            return;
        };
        match e {
            PeerEvent::Closed { .. } => {
                self.forget();
                self.drop_ws(1000);
                self.emit(e);
            }
            PeerEvent::Open {
                role,
                peer,
                seat,
                resumed,
            } => {
                self.paired = true;
                if let Some(seat) = seat.clone() {
                    self.seat = Some(seat);
                }
                if resumed {
                    self.resume_until = 0.0;
                    self.resume_timer = None;
                    self.resume_handle = false;
                    for data in std::mem::take(&mut self.outbox) {
                        self.send(&data);
                    }
                    self.emit(PeerEvent::Back { who: Who::SelfSide });
                    return;
                }
                let peer = if peer.is_empty() {
                    "friend".into()
                } else {
                    peer
                };
                self.emit(PeerEvent::Open {
                    role,
                    peer,
                    seat,
                    resumed,
                });
            }
            PeerEvent::Away { .. } | PeerEvent::Back { .. } | PeerEvent::Hosting { .. } => {
                self.emit(e)
            }
            _ => {}
        }
    }

    fn fail(&mut self, reason: CloseReason) {
        self.forget();
        self.drop_ws(1000);
        self.emit(PeerEvent::Closed { reason });
    }

    // quietly: the relay tells the other side (code 1000 leaves; RECONNECTING keeps the seat)
    fn drop_ws(&mut self, code: u16) {
        let ws = self.ws.take();
        self.paired = false;
        self.ping_timer = None;
        self.connect_timer = None;
        if let Some(ws) = ws {
            self.cmds.push(SocketCmd::Close { id: ws.id, code });
        }
    }

    /// The room list: its own socket, kept apart from the game's, that the relay pushes the list down.
    pub fn watch(&mut self, game: &str, now: f64) {
        self.at(now);
        self.unwatch();
        self.watching = Some(game.to_string());
        self.watch_wait = WATCH_RETRY_MS;
        self.open_watcher();
    }

    fn open_watcher(&mut self) {
        let Some(game) = self.watching.clone() else {
            return;
        };
        let url = format!("{}/rooms/{}", self.base, encode_uri_component(&game));
        self.watcher = Some(self.new_socket(url));
        self.watcher_heard = false;
        // a quiet socket can be dropped along the way; the relay answers pings without waking up
        self.watch_ping = Some(self.now + PING_MS * 3.0);
    }

    fn watcher_message(&mut self, text: &str) {
        let Some(raw) = text.strip_prefix('!').and_then(parse) else {
            return;
        };
        let Some(ev @ PeerEvent::Rooms { .. }) = parse_peer_event(&raw, ONLINE_CODE_LENGTH) else {
            return;
        };
        self.watcher_heard = true;
        self.listed = true;
        self.code_only = false;
        self.watch_wait = WATCH_RETRY_MS;
        self.emit(ev);
    }

    fn watcher_closed(&mut self) {
        self.watcher = None;
        self.watch_ping = None;
        // a relay without lists refuses the socket before saying anything; so does one that can't be reached
        if !self.watcher_heard && !self.listed {
            self.code_only = true;
        }
        let code_only = self.code_only;
        self.emit(PeerEvent::Rooms {
            rooms: vec![],
            offline: true,
            code_only,
        });
        self.watch_timer = Some(self.now + self.watch_wait);
        self.watch_wait = WATCH_RETRY_MAX.min(self.watch_wait * 2.0);
    }

    pub fn unwatch(&mut self) {
        self.watching = None;
        self.watch_timer = None;
        self.watch_ping = None;
        if let Some(ws) = self.watcher.take() {
            self.cmds.push(SocketCmd::Close {
                id: ws.id,
                code: 1000,
            });
        }
    }

    pub fn host(&mut self, game: &str, name: &str, locked: bool, now: f64) {
        self.at(now);
        self.forget();
        // resume=1: this side can take its seat back after a dropped link
        if self.code_only {
            self.connect("/host?resume=1", 1000);
            return;
        }
        let q = search_params(&[
            ("name", name),
            ("locked", if locked { "1" } else { "0" }),
            ("resume", "1"),
        ]);
        self.connect(&format!("/host/{}?{q}", encode_uri_component(game)), 1000);
    }

    pub fn join(&mut self, room_id: &str, code: &str, name: &str, now: f64) {
        self.at(now);
        self.forget();
        self.emit(PeerEvent::Pairing);
        // no room: the code alone, which reaches a locked listed room too
        if room_id.is_empty() {
            let q = search_params(&[("name", name), ("resume", "1")]);
            self.connect(&format!("/join/{}?{q}", encode_uri_component(code)), 1000);
            return;
        }
        let mut pairs = vec![("name", name), ("resume", "1")];
        if !code.is_empty() {
            pairs.push(("code", code));
        }
        let q = search_params(&pairs);
        self.connect(
            &format!("/join/{}?{q}", encode_uri_component(room_id)),
            1000,
        );
    }

    pub fn send(&mut self, data: &str) {
        match self.ws {
            Some(Sock { id, open: true }) if self.paired => self.cmds.push(SocketCmd::Send {
                id,
                text: format!(".{data}"),
            }),
            _ => {
                if self.seat.is_some() && self.outbox.len() < MAX_OUTBOX {
                    self.outbox.push(data.to_string());
                }
            }
        }
    }

    /// Leaves quietly: the other side hears `bye`, this side hears nothing.
    pub fn close(&mut self) {
        self.forget();
        self.drop_ws(1000);
    }

    /// The app is quitting: quitting loses the game itself, so leave and the friend hears at once.
    pub fn page_hide(&mut self) {
        if self.seat.is_some() {
            self.close();
        }
    }

    /// Whether a seat is held (the game must outlive a dropped link).
    pub fn has_seat(&self) -> bool {
        self.seat.is_some()
    }
}

/// Which side a role names, for the shells.
pub fn side_of(role: &str) -> Option<Side> {
    match role {
        "host" => Some(Side::Host),
        "guest" => Some(Side::Guest),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::peer::RoomInfo;
    use std::collections::BTreeMap;

    // the fake sockets of peerOnline.test.ts: each opens at once (readyState 1) unless told otherwise
    #[derive(Default, Debug)]
    struct Fake {
        url: String,
        sent: Vec<String>,
        closed: bool,
        close_code: Option<u16>,
    }

    struct T {
        peer: OnlinePeer,
        sockets: BTreeMap<u64, Fake>,
        order: Vec<u64>,
        events: Vec<PeerEvent>,
        now: f64,
        connecting: bool,
    }

    impl T {
        fn new() -> T {
            T {
                peer: OnlinePeer::new("wss://relay.test/"),
                sockets: BTreeMap::new(),
                order: vec![],
                events: vec![],
                now: 1_000_000.0,
                connecting: false,
            }
        }
        fn pump(&mut self) {
            loop {
                let cmds = self.peer.take_commands();
                if cmds.is_empty() {
                    break;
                }
                for c in cmds {
                    match c {
                        SocketCmd::Open { id, url } => {
                            self.sockets.insert(
                                id,
                                Fake {
                                    url,
                                    ..Default::default()
                                },
                            );
                            self.order.push(id);
                            if !self.connecting {
                                self.peer.opened(id, self.now);
                            }
                        }
                        SocketCmd::Send { id, text } => {
                            self.sockets.get_mut(&id).unwrap().sent.push(text)
                        }
                        SocketCmd::Close { id, code } => {
                            let s = self.sockets.get_mut(&id).unwrap();
                            s.closed = true;
                            s.close_code = Some(code);
                        }
                    }
                }
            }
            self.events.extend(self.peer.take_events());
        }
        fn last_id(&self) -> u64 {
            *self.order.last().unwrap()
        }
        fn last(&self) -> &Fake {
            &self.sockets[&self.last_id()]
        }
        fn nth(&self, i: usize) -> u64 {
            self.order[i]
        }
        fn hear(&mut self, id: u64, text: &str) {
            self.peer.message(id, text, self.now);
            self.pump();
        }
        fn event(&mut self, id: u64, json: &str) {
            self.hear(id, &format!("!{json}"));
        }
        fn drop(&mut self, id: u64) {
            self.peer.closed(id, self.now);
            self.pump();
        }
        fn advance(&mut self, ms: f64) {
            self.now += ms;
            self.peer.tick(self.now);
            self.pump();
        }
        fn host(&mut self, game: &str, name: &str, locked: bool) {
            self.peer.host(game, name, locked, self.now);
            self.pump();
        }
        fn join(&mut self, room: &str, code: &str, name: &str) {
            self.peer.join(room, code, name, self.now);
            self.pump();
        }
        fn send(&mut self, data: &str) {
            self.peer.send(data);
            self.pump();
        }
    }

    fn hosting(code: &str) -> PeerEvent {
        PeerEvent::Hosting {
            code: code.into(),
            renewed: false,
        }
    }
    fn closed(reason: CloseReason) -> PeerEvent {
        PeerEvent::Closed { reason }
    }
    fn open(role: Side, peer: &str) -> PeerEvent {
        PeerEvent::Open {
            role,
            peer: peer.into(),
            seat: None,
            resumed: false,
        }
    }
    fn rooms(rooms: Vec<RoomInfo>, offline: bool, code_only: bool) -> PeerEvent {
        PeerEvent::Rooms {
            rooms,
            offline,
            code_only,
        }
    }

    #[test]
    fn hosts_and_passes_on_the_code() {
        let mut t = T::new();
        t.host("pong", "teal otter", true);
        assert_eq!(
            t.last().url,
            "wss://relay.test/host/pong?name=teal+otter&locked=1&resume=1"
        );
        t.event(t.last_id(), r#"{"type":"hosting","code":"042017"}"#);
        assert_eq!(t.events, vec![hosting("042017")]);
    }

    #[test]
    fn drops_a_code_of_the_wrong_length_and_hosts_open_rooms() {
        let mut t = T::new();
        t.host("pong", "teal otter", true);
        t.event(t.last_id(), r#"{"type":"hosting","code":"0420"}"#);
        assert!(t.events.is_empty());
        let mut t = T::new();
        t.host("connect4", "amber fox", false);
        assert_eq!(
            t.last().url,
            "wss://relay.test/host/connect4?name=amber+fox&locked=0&resume=1"
        );
        t.event(t.last_id(), r#"{"type":"hosting","code":""}"#);
        assert_eq!(t.events, vec![hosting("")]);
    }

    #[test]
    fn joins_and_plays_with_dotted_messages() {
        let mut t = T::new();
        t.join("r1", "123456", "jade owl");
        assert_eq!(
            t.last().url,
            "wss://relay.test/join/r1?name=jade+owl&resume=1&code=123456"
        );
        assert_eq!(t.events, vec![PeerEvent::Pairing]);
        t.send("too early");
        assert!(t.last().sent.is_empty());
        t.event(t.last_id(), r#"{"type":"open","role":"guest","peer":""}"#);
        assert_eq!(t.events.last(), Some(&open(Side::Guest, "friend")));
        t.send(r#"{"t":"hi"}"#);
        assert_eq!(t.last().sent, vec![r#".{"t":"hi"}"#]);
        t.hear(t.last_id(), r#".{"t":"s"}"#);
        assert_eq!(
            t.events.last(),
            Some(&PeerEvent::Message {
                data: r#"{"t":"s"}"#.into()
            })
        );
    }

    #[test]
    fn refusals_failures_and_byes() {
        let mut t = T::new();
        t.join("r1", "000000", "jade owl");
        t.event(t.last_id(), r#"{"type":"closed","reason":"wrong-code"}"#);
        assert_eq!(t.events.last(), Some(&closed(CloseReason::WrongCode)));
        assert!(t.last().closed);

        let mut a = T::new();
        a.host("pong", "teal otter", true);
        a.drop(a.last_id());
        assert_eq!(a.events.last(), Some(&closed(CloseReason::Network)));

        let mut b = T::new();
        b.join("r1", "", "jade owl");
        b.event(b.last_id(), r#"{"type":"open","role":"guest","peer":""}"#);
        b.drop(b.last_id());
        assert_eq!(b.events.last(), Some(&closed(CloseReason::Lost)));

        let mut c = T::new();
        c.host("pong", "teal otter", true);
        c.event(c.last_id(), r#"{"type":"open","role":"host","peer":""}"#);
        c.event(c.last_id(), r#"{"type":"closed","reason":"bye"}"#);
        assert_eq!(c.events.last(), Some(&closed(CloseReason::Bye)));
    }

    #[test]
    fn closing_is_quiet_and_a_stale_socket_is_ignored() {
        let mut t = T::new();
        t.host("pong", "teal otter", true);
        t.peer.close();
        t.pump();
        let first = t.nth(0);
        assert!(t.sockets[&first].closed);
        t.host("pong", "teal otter", true);
        t.event(first, r#"{"type":"hosting","code":"111111"}"#);
        t.drop(first);
        t.event(t.last_id(), r#"{"type":"hosting","code":"222222"}"#);
        assert_eq!(t.events, vec![hosting("222222")]);
    }

    #[test]
    fn pings_and_gives_up_on_a_silent_relay() {
        let mut t = T::new();
        t.host("pong", "teal otter", true);
        t.event(t.last_id(), r#"{"type":"hosting","code":"042017"}"#);
        t.advance(10_000.0);
        assert_eq!(t.last().sent, vec!["ping"]);
        t.hear(t.last_id(), "pong");
        t.advance(20_000.0);
        assert_eq!(t.events.last(), Some(&hosting("042017")));
        t.advance(10_000.0);
        assert_eq!(t.events.last(), Some(&closed(CloseReason::Lost)));
    }

    #[test]
    fn fails_a_socket_that_never_connects() {
        let mut t = T::new();
        t.connecting = true;
        t.host("pong", "teal otter", false);
        t.advance(10_000.0);
        assert_eq!(t.events.last(), Some(&closed(CloseReason::Network)));
    }

    #[test]
    fn watches_rooms_on_a_socket_of_its_own() {
        let mut t = T::new();
        t.peer.watch("pong", t.now);
        t.pump();
        assert_eq!(t.last().url, "wss://relay.test/rooms/pong");
        let long = "x".repeat(80);
        t.event(
            t.last_id(),
            &format!(r#"{{"type":"rooms","rooms":[{{"id":"a","name":"teal otter","locked":false}},{{"id":"b","name":"{long}","locked":true}},{{"id":3}}]}}"#),
        );
        let want = vec![
            RoomInfo {
                id: "a".into(),
                name: "teal otter".into(),
                locked: false,
            },
            RoomInfo {
                id: "b".into(),
                name: "x".repeat(24),
                locked: true,
            },
        ];
        assert_eq!(t.events.last(), Some(&rooms(want, false, false)));
        t.join("a", "", "me");
        assert_eq!(t.order.len(), 2);
        t.drop(t.nth(0));
        assert_eq!(t.events.last(), Some(&rooms(vec![], true, false)));
        t.advance(3_000.0);
        assert_eq!(t.order.len(), 3);
        assert_eq!(t.last().url, "wss://relay.test/rooms/pong");
        t.peer.unwatch();
        t.pump();
        assert!(t.last().closed);
        t.advance(60_000.0);
        assert_eq!(t.order.len(), 3);
    }

    #[test]
    fn falls_back_to_codes_without_a_list() {
        let mut t = T::new();
        t.peer.watch("pong", t.now);
        t.pump();
        t.drop(t.last_id());
        assert_eq!(t.events.last(), Some(&rooms(vec![], true, true)));
        t.host("pong", "teal otter", false);
        assert_eq!(t.last().url, "wss://relay.test/host?resume=1");
        t.join("", "123456", "teal otter");
        assert_eq!(
            t.last().url,
            "wss://relay.test/join/123456?name=teal+otter&resume=1"
        );
        t.advance(3_000.0);
        assert_eq!(t.last().url, "wss://relay.test/rooms/pong");
        t.event(t.last_id(), r#"{"type":"rooms","rooms":[]}"#);
        assert_eq!(t.events.last(), Some(&rooms(vec![], false, false)));
        t.host("pong", "teal otter", false);
        assert_eq!(
            t.last().url,
            "wss://relay.test/host/pong?name=teal+otter&locked=0&resume=1"
        );

        let mut u = T::new();
        u.peer.watch("pong", u.now);
        u.pump();
        u.event(u.last_id(), r#"{"type":"rooms","rooms":[]}"#);
        u.drop(u.last_id());
        assert_eq!(u.events.last(), Some(&rooms(vec![], true, false)));
    }

    #[test]
    fn ignores_junk() {
        let mut t = T::new();
        t.host("pong", "teal otter", true);
        for junk in [
            "",
            "!",
            "!{",
            r#"!{"type":"open","role":"king"}"#,
            "?hello",
            ".before open",
        ] {
            t.hear(t.last_id(), junk);
        }
        assert!(t.events.is_empty());
    }

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    fn playing() -> T {
        let mut t = T::new();
        t.join("room1", "", "jade owl");
        t.event(t.last_id(), &format!(r#"{{"type":"open","role":"guest","peer":"teal otter","room":"room1","token":"{TOKEN}"}}"#));
        t.events.clear();
        t
    }

    fn resumed(t: &mut T) {
        t.event(
            t.last_id(),
            &format!(r#"{{"type":"open","role":"guest","peer":"teal otter","room":"room1","token":"{TOKEN}","resumed":true}}"#),
        );
    }

    #[test]
    fn reconnects_to_the_same_seat() {
        let mut t = playing();
        t.drop(t.last_id());
        assert_eq!(t.events, vec![PeerEvent::Away { who: Who::SelfSide }]);
        t.advance(0.0);
        assert_eq!(t.order.len(), 2);
        assert_eq!(
            t.last().url,
            format!("wss://relay.test/back/room1?token={TOKEN}")
        );
        t.send("move 3");
        assert!(t.last().sent.is_empty());
        resumed(&mut t);
        assert_eq!(
            t.events,
            vec![
                PeerEvent::Away { who: Who::SelfSide },
                PeerEvent::Back { who: Who::SelfSide }
            ]
        );
        assert_eq!(t.last().sent, vec![".move 3"]);
    }

    #[test]
    fn closes_a_quiet_link_as_reconnecting() {
        let mut t = playing();
        let first = t.last_id();
        t.hear(first, "pong");
        t.advance(40_000.0);
        assert_eq!(t.sockets[&first].close_code, Some(4000));
        assert_ne!(t.last_id(), first);
    }

    #[test]
    fn keeps_trying_then_gives_up() {
        let mut t = playing();
        t.drop(t.last_id());
        t.advance(0.0);
        for _ in 0..200 {
            if t.events
                .iter()
                .any(|e| matches!(e, PeerEvent::Closed { .. }))
            {
                break;
            }
            t.drop(t.last_id());
            t.advance(5_000.0);
        }
        assert!(t.order.len() > 10);
        assert_eq!(t.events.last(), Some(&closed(CloseReason::Lost)));
    }

    #[test]
    fn ends_when_the_friend_left_meanwhile() {
        let mut t = playing();
        t.drop(t.last_id());
        t.advance(0.0);
        t.event(t.last_id(), r#"{"type":"closed","reason":"bye"}"#);
        assert_eq!(t.events.last(), Some(&closed(CloseReason::Bye)));
        let n = t.events.len();
        t.advance(60_000.0);
        assert_eq!(t.events.len(), n);
    }

    #[test]
    fn passes_on_the_other_side_stepping_away() {
        let mut t = playing();
        t.event(t.last_id(), r#"{"type":"away"}"#);
        t.event(t.last_id(), r#"{"type":"back"}"#);
        assert_eq!(
            t.events,
            vec![
                PeerEvent::Away { who: Who::Peer },
                PeerEvent::Back { who: Who::Peer }
            ]
        );
    }

    #[test]
    fn leaving_on_purpose_closes_with_1000() {
        let mut t = playing();
        let first = t.last_id();
        t.peer.close();
        t.pump();
        assert_eq!(t.sockets[&first].close_code, Some(1000));
        t.advance(60_000.0);
        assert_eq!(t.order.len(), 1);
    }

    #[test]
    fn hiding_steps_away_until_shown() {
        let mut t = playing();
        let first = t.last_id();
        t.peer.step_away(t.now);
        t.pump();
        assert_eq!(t.sockets[&first].close_code, Some(4000));
        assert_eq!(t.events, vec![PeerEvent::Away { who: Who::SelfSide }]);
        t.advance(30.0 * 60_000.0);
        assert_eq!(t.order.len(), 1);
        t.peer.come_back(t.now);
        t.pump();
        assert_eq!(
            t.last().url,
            format!("wss://relay.test/back/room1?token={TOKEN}")
        );
        resumed(&mut t);
        assert_eq!(
            t.events.last(),
            Some(&PeerEvent::Back { who: Who::SelfSide })
        );

        let mut u = playing();
        u.peer.step_away(u.now);
        u.advance(30.0 * 60_000.0);
        u.peer.come_back(u.now);
        u.pump();
        u.event(u.last_id(), r#"{"type":"closed","reason":"lost"}"#);
        assert_eq!(u.events.last(), Some(&closed(CloseReason::Lost)));
    }

    #[test]
    fn stepping_away_before_a_game_does_nothing() {
        let mut t = T::new();
        t.host("pong", "teal otter", false);
        t.peer.step_away(t.now);
        t.peer.come_back(t.now);
        t.pump();
        assert_eq!(t.order.len(), 1);
        assert!(!t.last().closed);
        assert!(t.events.is_empty());
        let mut u = T::new();
        u.join("room1", "", "jade owl");
        u.drop(u.last_id());
        assert_eq!(u.events.last(), Some(&closed(CloseReason::Network)));
    }
}
