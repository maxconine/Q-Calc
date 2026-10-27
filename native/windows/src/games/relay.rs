//! The relay link for online play (`OnlinePeer` in `src/lib/peerOnline.ts`; the Mac's RelayTransport). The
//! core's `OnlinePeer` decides everything; this runs its websockets and hands its events to the window.
//!
//! Each socket is two threads over relay.rs's WinHTTP WebSocket (SChannel TLS, the system's proxy, no crates): one
//! connects and sends what the UI queues, the other waits in the socket's receive; both post `WM_SOCK` to the game
//! window for open / message / closed. Sockets exist only while a game's lobby or an online game uses the link; the
//! window's close closes them (the friend hears `bye`).

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use qcalc_core::games::peer::PeerEvent;
use qcalc_core::games::peer_online::{OnlinePeer, RELAY_URL, SocketCmd};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

use crate::relay::Frame;

/// A socket's news for the UI thread: wParam is the socket's id, lParam a `Box<SockEvent>`.
pub const WM_SOCK: u32 = WM_APP + 0x47;

#[derive(Debug)]
pub enum SockEvent {
    Opened,
    Message(String),
    Closed,
}

enum Out {
    Text(String),
    Close(u16),
}

/// The relay this build talks to: QCALC_RELAY_URL for a local one (loopback only), else the real one.
pub fn base() -> String {
    if let Ok(env) = std::env::var("QCALC_RELAY_URL") {
        let host = env
            .split("://")
            .nth(1)
            .unwrap_or("")
            .split(['/', ':'])
            .next()
            .unwrap_or("");
        if ["localhost", "127.0.0.1", "[::1]"].contains(&host) {
            return env.replacen("http", "ws", 1);
        }
    }
    RELAY_URL.to_string()
}

pub struct Relay {
    pub peer: OnlinePeer,
    socks: HashMap<u64, Sender<Out>>,
    hwnd: usize,
}

impl Relay {
    pub fn new(hwnd: HWND) -> Relay {
        Relay {
            peer: OnlinePeer::new(&base()),
            socks: HashMap::new(),
            hwnd: hwnd.0 as usize,
        }
    }

    /// Runs what the core asked of the sockets; returns what it said.
    pub fn pump(&mut self) -> Vec<PeerEvent> {
        for cmd in self.peer.take_commands() {
            match cmd {
                SocketCmd::Open { id, url } => {
                    let (tx, rx) = mpsc::channel();
                    self.socks.insert(id, tx);
                    let hwnd = self.hwnd;
                    let spawned = std::thread::Builder::new()
                        .name(format!("qcalc-relay-{id}"))
                        .stack_size(256 * 1024)
                        .spawn(move || socket(id, &url, &rx, move |e| post(hwnd, id, e)));
                    if spawned.is_err() {
                        self.socks.remove(&id);
                        post(hwnd, id, SockEvent::Closed);
                    }
                }
                SocketCmd::Send { id, text } => {
                    if let Some(tx) = self.socks.get(&id) {
                        let _ = tx.send(Out::Text(text));
                    }
                }
                SocketCmd::Close { id, code } => {
                    if let Some(tx) = self.socks.remove(&id) {
                        let _ = tx.send(Out::Close(code));
                    }
                }
            }
        }
        self.peer.take_events()
    }

    /// A socket's news, from `WM_SOCK`. One the core already closed is forgotten.
    pub fn heard(&mut self, id: u64, e: SockEvent, now: f64) {
        if !self.socks.contains_key(&id) {
            return;
        }
        match e {
            SockEvent::Opened => {
                crate::qlog!("games: relay socket {id} open");
                self.peer.opened(id, now);
            }
            SockEvent::Message(text) => self.peer.message(id, &text, now),
            SockEvent::Closed => {
                self.socks.remove(&id);
                self.peer.closed(id, now);
            }
        }
    }

    /// Leaves for good (the window closed): the other side hears `bye`, the sockets close.
    pub fn shut(&mut self) {
        self.peer.page_hide();
        self.peer.close();
        let _ = self.pump();
        for (_, tx) in self.socks.drain() {
            let _ = tx.send(Out::Close(1000));
        }
    }
}

fn post(hwnd: usize, id: u64, e: SockEvent) {
    let raw = Box::into_raw(Box::new(e));
    // SAFETY: posting to the game window from a socket thread; the UI thread takes the box back (or, when the
    // window has gone, the post fails and we take it back here)
    let ok = unsafe {
        PostMessageW(
            Some(HWND(hwnd as *mut _)),
            WM_SOCK,
            WPARAM(id as usize),
            LPARAM(raw as isize),
        )
    };
    if ok.is_err() {
        // SAFETY: not posted, so still ours
        drop(unsafe { Box::from_raw(raw) });
    }
}

/// Takes back what `post` sent.
///
/// # Safety
/// `lp` must be a `WM_SOCK` lParam, taken once.
pub unsafe fn take(lp: LPARAM) -> SockEvent {
    // SAFETY: as the caller promises
    *unsafe { Box::from_raw(lp.0 as *mut SockEvent) }
}

/// A socket's thread: connects, then reads on a second thread while this one sends what the UI queues, so a send
/// goes out at once and a message comes in as it arrives. `tell` takes the socket's news (a `WM_SOCK` post).
fn socket(
    id: u64,
    url: &str,
    rx: &Receiver<Out>,
    tell: impl Fn(SockEvent) + Clone + Send + 'static,
) {
    let Some(ws) = crate::relay::Socket::open_url(url).map(Arc::new) else {
        crate::qlog!("games: relay socket {id} didn't open");
        tell(SockEvent::Closed);
        return;
    };
    // closed while connecting: leave quietly
    if let Ok(Out::Close(_)) | Err(mpsc::TryRecvError::Disconnected) = rx.try_recv() {
        ws.shutdown(1000);
        ws.close();
        return;
    }
    tell(SockEvent::Opened);
    let reader = {
        let (ws, tell) = (ws.clone(), tell.clone());
        std::thread::Builder::new()
            .name(format!("qcalc-relay-{id}-in"))
            .stack_size(256 * 1024)
            .spawn(move || {
                while let Frame::Text(text) = ws.receive() {
                    tell(SockEvent::Message(text));
                }
                tell(SockEvent::Closed);
            })
    };
    if reader.is_err() {
        ws.close();
        tell(SockEvent::Closed);
        return;
    }
    let code = loop {
        match rx.recv() {
            Ok(Out::Text(text)) => {
                if !ws.send(&text) {
                    // the reader hears the socket end and says so
                    ws.close();
                    return;
                }
            }
            Ok(Out::Close(code)) => break code,
            Err(_) => break 1000,
        }
    };
    // the close frame with its code (4000 keeps the seat), a moment for the relay's answer, then the handle goes
    ws.shutdown(code);
    if let Ok(reader) = reader {
        for _ in 0..50 {
            if reader.is_finished() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    ws.close();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cargo test -p qcalc-win live_relay -- --ignored --nocapture`: two peers over the real relay (or
    /// QCALC_RELAY_URL's) through these sockets: one hosts a locked room, the other joins by its code, a message
    /// crosses, and leaving with 1000 tells the other side.
    #[test]
    #[ignore]
    fn live_relay_pairs_two_peers() {
        use qcalc_core::games::peer::PeerEvent;
        use std::time::Instant;
        let t0 = Instant::now();
        let now = || t0.elapsed().as_secs_f64() * 1000.0;
        let (tx, rx) = mpsc::channel::<(usize, u64, SockEvent)>();
        let mut peers = [OnlinePeer::new(&base()), OnlinePeer::new(&base())];
        let mut socks: [HashMap<u64, Sender<Out>>; 2] = [HashMap::new(), HashMap::new()];
        let mut heard: [Vec<PeerEvent>; 2] = [vec![], vec![]];
        let mut pump = |peers: &mut [OnlinePeer; 2],
                        heard: &mut [Vec<PeerEvent>; 2],
                        until: &dyn Fn(&[Vec<PeerEvent>; 2]) -> bool| {
            let end = Instant::now() + Duration::from_secs(15);
            loop {
                for (side, peer) in peers.iter_mut().enumerate() {
                    peer.tick(now());
                    for cmd in peer.take_commands() {
                        match cmd {
                            SocketCmd::Open { id, url } => {
                                let (stx, srx) = mpsc::channel();
                                socks[side].insert(id, stx);
                                let tx = tx.clone();
                                std::thread::spawn(move || {
                                    socket(id, &url, &srx, move |e| {
                                        let _ = tx.send((side, id, e));
                                    })
                                });
                            }
                            SocketCmd::Send { id, text } => {
                                if let Some(s) = socks[side].get(&id) {
                                    let _ = s.send(Out::Text(text));
                                }
                            }
                            SocketCmd::Close { id, code } => {
                                if let Some(s) = socks[side].remove(&id) {
                                    let _ = s.send(Out::Close(code));
                                }
                            }
                        }
                    }
                    heard[side].extend(peer.take_events());
                }
                if until(heard) {
                    return;
                }
                assert!(Instant::now() < end, "timed out: {heard:?}");
                if let Ok((side, id, e)) = rx.recv_timeout(Duration::from_millis(20)) {
                    if !socks[side].contains_key(&id) {
                        continue;
                    }
                    match e {
                        SockEvent::Opened => peers[side].opened(id, now()),
                        SockEvent::Message(text) => peers[side].message(id, &text, now()),
                        SockEvent::Closed => {
                            socks[side].remove(&id);
                            peers[side].closed(id, now());
                        }
                    }
                }
            }
        };
        peers[0].host("pong", "qcalc-test", true, now());
        pump(&mut peers, &mut heard, &|h| {
            h[0].iter().any(|e| matches!(e, PeerEvent::Hosting { .. }))
        });
        let code = heard[0]
            .iter()
            .find_map(|e| match e {
                PeerEvent::Hosting { code, .. } => Some(code.clone()),
                _ => None,
            })
            .unwrap();
        println!("hosting {code} after {:.0} ms", now());
        peers[1].join("", &code, "qcalc-test-b", now());
        let open = |h: &Vec<PeerEvent>| h.iter().any(|e| matches!(e, PeerEvent::Open { .. }));
        pump(&mut peers, &mut heard, &|h| open(&h[0]) && open(&h[1]));
        println!("paired after {:.0} ms", now());
        peers[0].send("hi there");
        pump(&mut peers, &mut heard, &|h| {
            h[1].iter()
                .any(|e| matches!(e, PeerEvent::Message { data } if data == "hi there"))
        });
        println!("message crossed after {:.0} ms", now());
        peers[0].close();
        pump(&mut peers, &mut heard, &|h| {
            h[1].iter().any(|e| matches!(e, PeerEvent::Closed { .. }))
        });
        println!(
            "the other side heard the leave after {:.0} ms: {:?}",
            now(),
            heard[1].last()
        );
    }

    #[test]
    fn a_local_relay_is_only_taken_on_loopback() {
        // without the variable, the real relay
        if std::env::var("QCALC_RELAY_URL").is_err() {
            assert_eq!(base(), RELAY_URL);
        }
    }
}
