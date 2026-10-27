//! The relay link for online play (`OnlinePeer` in `src/lib/peerOnline.ts`; the Mac's RelayTransport). The
//! core's `OnlinePeer` decides everything; this runs its websockets and hands its events to the window.
//!
//! Each socket is a thread of its own (tungstenite over SChannel TLS): it connects, posts `WM_SOCK` to the game
//! window for open / message / closed, and sends what the UI queues. After connecting the stream is non-blocking
//! and the thread waits on its outgoing queue for 5 ms between reads, so a send goes out at once and a message
//! comes in within 5 ms. Sockets exist only while a game's lobby or an online game uses the link; the window's
//! close closes them (the friend hears `bye`).

use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use qcalc_core::games::peer::PeerEvent;
use qcalc_core::games::peer_online::{OnlinePeer, RELAY_URL, SocketCmd};
use tungstenite::protocol::CloseFrame;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

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
                        .spawn(move || socket(id, &url, &rx, hwnd));
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
            SockEvent::Opened => self.peer.opened(id, now),
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

fn set_nonblocking(ws: &mut WebSocket<MaybeTlsStream<TcpStream>>) -> std::io::Result<()> {
    match ws.get_mut() {
        MaybeTlsStream::Plain(s) => s.set_nonblocking(true),
        MaybeTlsStream::NativeTls(s) => s.get_mut().set_nonblocking(true),
        _ => Ok(()),
    }
}

fn would_block(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::Io(io) if io.kind() == ErrorKind::WouldBlock)
}

fn socket(id: u64, url: &str, rx: &Receiver<Out>, hwnd: usize) {
    let mut ws = match tungstenite::connect(url) {
        Ok((ws, _)) => ws,
        Err(e) => {
            crate::qlog!("games: relay socket {id} didn't open: {e}");
            post(hwnd, id, SockEvent::Closed);
            return;
        }
    };
    // closed while connecting: leave quietly
    if let Ok(Out::Close(_)) | Err(mpsc::TryRecvError::Disconnected) = rx.try_recv() {
        let _ = ws.close(None);
        let _ = ws.flush();
        return;
    }
    if set_nonblocking(&mut ws).is_err() {
        post(hwnd, id, SockEvent::Closed);
        return;
    }
    post(hwnd, id, SockEvent::Opened);
    loop {
        // everything that's arrived
        loop {
            match ws.read() {
                Ok(Message::Text(t)) => post(hwnd, id, SockEvent::Message(t.as_str().to_string())),
                Ok(Message::Close(_)) => {
                    post(hwnd, id, SockEvent::Closed);
                    return;
                }
                Ok(_) => {}
                Err(e) if would_block(&e) => break,
                Err(_) => {
                    post(hwnd, id, SockEvent::Closed);
                    return;
                }
            }
        }
        // then what's to go, waiting a moment for it
        match rx.recv_timeout(Duration::from_millis(5)) {
            Ok(Out::Text(text)) => {
                if let Err(e) = ws.send(Message::text(text))
                    && !would_block(&e)
                {
                    post(hwnd, id, SockEvent::Closed);
                    return;
                }
            }
            Ok(Out::Close(code)) => {
                let _ = ws.close(Some(CloseFrame {
                    code: CloseCode::from(code),
                    reason: "".into(),
                }));
                for _ in 0..20 {
                    match ws.flush() {
                        Err(e) if would_block(&e) => std::thread::sleep(Duration::from_millis(5)),
                        _ => break,
                    }
                }
                return;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                let _ = ws.close(None);
                let _ = ws.flush();
                return;
            }
        }
        if let Err(e) = ws.flush()
            && !would_block(&e)
        {
            post(hwnd, id, SockEvent::Closed);
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_relay_is_only_taken_on_loopback() {
        // without the variable, the real relay
        if std::env::var("QCALC_RELAY_URL").is_err() {
            assert_eq!(base(), RELAY_URL);
        }
    }
}
