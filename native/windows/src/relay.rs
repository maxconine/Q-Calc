//! The relay's transport (relay/src/index.js): messaging's JSON POSTs (`/claim`, `/release`, `/send`, `/lookup`)
//! and inbox WebSocket (`/inbox/<name>`, the key riding as a protocol so it's never in a URL), and the games'
//! sockets (games/relay.rs, `Socket::open_url`).
//!
//! WinHTTP, the system's HTTP stack: TLS is SChannel, proxies are the system's, and the binary grows by nothing
//! (winhttp.dll loads only when messaging is first used). Everything here blocks, so it runs on messaging.rs's
//! worker threads, never the UI thread.
//!
//! The relay is `https://qcalc-relay.maxconine.workers.dev`, or a local one (`wrangler dev`, a test double) named by
//! `QCALC_RELAY_URL`; only a loopback `http://` address is taken from it, so nothing outside can point the name and
//! key elsewhere (as Messages.swift).

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Networking::WinHttp::{
    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_ADDREQ_FLAG_ADD, WINHTTP_FLAG_SECURE,
    WINHTTP_OPEN_REQUEST_FLAGS, WINHTTP_OPTION_UPGRADE_TO_WEB_SOCKET, WINHTTP_QUERY_FLAG_NUMBER,
    WINHTTP_QUERY_STATUS_CODE, WINHTTP_WEB_SOCKET_BUFFER_TYPE,
    WINHTTP_WEB_SOCKET_CLOSE_BUFFER_TYPE, WINHTTP_WEB_SOCKET_UTF8_FRAGMENT_BUFFER_TYPE,
    WINHTTP_WEB_SOCKET_UTF8_MESSAGE_BUFFER_TYPE, WinHttpAddRequestHeaders, WinHttpCloseHandle,
    WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
    WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetOption, WinHttpSetTimeouts, WinHttpWebSocketCompleteUpgrade, WinHttpWebSocketReceive,
    WinHttpWebSocketSend, WinHttpWebSocketShutdown,
};
use windows::core::{HSTRING, PCWSTR, w};

pub const RELAY_HOST: &str = "qcalc-relay.maxconine.workers.dev";

/// Where the relay is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Base {
    pub secure: bool,
    pub host: String,
    pub port: u16,
}

/// The relay's address: `QCALC_RELAY_URL` when it's a loopback `http://` one, else the real relay.
pub fn base() -> Base {
    std::env::var("QCALC_RELAY_URL")
        .ok()
        .and_then(|u| local_base(&u))
        .unwrap_or_else(|| Base {
            secure: true,
            host: RELAY_HOST.into(),
            port: 443,
        })
}

/// `http://localhost:8787` or `http://127.0.0.1:8787/`; anything else is None.
pub fn local_base(url: &str) -> Option<Base> {
    let rest = url.strip_prefix("http://")?.trim_end_matches('/');
    let (host, port) = match rest.split_once(':') {
        Some((h, p)) => (h, p.parse().ok()?),
        None => (rest, 80),
    };
    (host == "localhost" || host == "127.0.0.1").then(|| Base {
        secure: false,
        host: host.into(),
        port,
    })
}

/// A `ws://` or `wss://` URL as (where, path with its query); None for anything else.
pub fn ws_url(url: &str) -> Option<(Base, String)> {
    let (secure, rest) = if let Some(r) = url.strip_prefix("wss://") {
        (true, r)
    } else {
        (false, url.strip_prefix("ws://")?)
    };
    let (authority, path) = match rest.find(['/', '?']) {
        Some(i) if rest[i..].starts_with('?') => (&rest[..i], format!("/{}", &rest[i..])),
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/".to_string()),
    };
    let default = if secure { 443 } else { 80 };
    // `[::1]:8787` and `[::1]`: the port is after the last colon only outside the brackets
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !p.contains(']') => (h, p.parse().ok()?),
        _ => (authority, default),
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    (!host.is_empty()).then(|| {
        (
            Base {
                secure,
                host: host.into(),
                port,
            },
            path,
        )
    })
}

/// A WinHTTP handle, closed once.
struct Handle(*mut c_void);

impl Handle {
    fn new(h: *mut c_void) -> Option<Handle> {
        (!h.is_null()).then_some(Handle(h))
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: a handle WinHTTP gave us, closed exactly once here
        let _ = unsafe { WinHttpCloseHandle(self.0) };
    }
}

fn session(receive_ms: i32) -> Option<Handle> {
    // SAFETY: plain session creation with static strings; the system's proxy settings
    let s = Handle::new(unsafe {
        WinHttpOpen(
            w!("QCalc"),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        )
    })?;
    // SAFETY: a live session
    let _ = unsafe { WinHttpSetTimeouts(s.0, 10_000, 10_000, 10_000, receive_ms) };
    Some(s)
}

fn request(base: &Base, connect: &Handle, verb: PCWSTR, path: &str) -> Option<Handle> {
    let flags = if base.secure {
        WINHTTP_FLAG_SECURE
    } else {
        WINHTTP_OPEN_REQUEST_FLAGS(0)
    };
    // SAFETY: a live connection; the path's HSTRING outlives the call
    Handle::new(unsafe {
        WinHttpOpenRequest(
            connect.0,
            verb,
            &HSTRING::from(path),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            flags,
        )
    })
}

fn status_code(req: &Handle) -> u32 {
    let mut code = 0u32;
    let mut len = 4u32;
    let mut index = 0u32;
    // SAFETY: a live request with a received response; the out buffer is a live u32
    let _ = unsafe {
        WinHttpQueryHeaders(
            req.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut code as *mut u32).cast()),
            &mut len,
            &mut index,
        )
    };
    code
}

/// POSTs `body` (JSON, sent as text/plain like the page's) and returns the relay's `status`, or "network" when it
/// can't be reached or says nothing readable.
pub fn post(path: &str, body: &serde_json::Value) -> String {
    post_at(&base(), path, body).unwrap_or_else(|| "network".into())
}

fn post_at(base: &Base, path: &str, body: &serde_json::Value) -> Option<String> {
    let s = session(10_000)?;
    // SAFETY: a live session; the host's HSTRING outlives the call
    let c = Handle::new(unsafe {
        WinHttpConnect(s.0, &HSTRING::from(base.host.as_str()), base.port, 0)
    })?;
    let r = request(base, &c, w!("POST"), path)?;
    let text = body.to_string();
    let header: Vec<u16> = "Content-Type: text/plain;charset=UTF-8"
        .encode_utf16()
        .collect();
    // SAFETY: a live request; the header and body outlive the calls
    unsafe {
        WinHttpSendRequest(
            r.0,
            Some(&header),
            Some(text.as_ptr().cast()),
            text.len() as u32,
            text.len() as u32,
            0,
        )
        .ok()?;
        WinHttpReceiveResponse(r.0, std::ptr::null_mut()).ok()?;
    }
    let mut out = Vec::new();
    loop {
        let mut avail = 0u32;
        // SAFETY: a live request; live out locals and a buffer of the size asked for
        unsafe { WinHttpQueryDataAvailable(r.0, &mut avail) }.ok()?;
        if avail == 0 {
            break;
        }
        let mut buf = vec![0u8; avail as usize];
        let mut read = 0u32;
        // SAFETY: as above
        unsafe { WinHttpReadData(r.0, buf.as_mut_ptr().cast(), avail, &mut read) }.ok()?;
        out.extend_from_slice(&buf[..read as usize]);
        if read == 0 || out.len() > 64 * 1024 {
            break;
        }
    }
    let v: serde_json::Value = serde_json::from_slice(&out).ok()?;
    v["status"].as_str().map(String::from)
}

/// An open inbox: one receiver thread blocks in `receive`, another may `send` (pings) or `close` at any time.
pub struct Socket {
    ws: *mut c_void,
    closed: AtomicBool,
    // kept open for the socket's life
    _session: Handle,
    _connect: Handle,
}

// SAFETY: WinHTTP's WebSocket handle allows one send and one receive at once from different threads, and close
// from any thread (it cancels the pending receive); `closed` makes sure it's closed once and not used after.
unsafe impl Send for Socket {}
// SAFETY: as above
unsafe impl Sync for Socket {}

/// What a receive got.
pub enum Frame {
    Text(String),
    Closed,
}

impl Socket {
    /// Opens `/inbox/<name>` with the key as a protocol beside `qcalc-inbox`.
    pub fn open(name: &str, key: &str) -> Option<Socket> {
        let protocol = format!("qcalc-inbox, k{key}");
        Socket::connect(&base(), &format!("/inbox/{name}"), Some(&protocol))
    }

    /// Opens a `ws://` / `wss://` URL as it is (the games' relay sockets; the URL's checks are the caller's).
    pub fn open_url(url: &str) -> Option<Socket> {
        let (base, path) = ws_url(url)?;
        Socket::connect(&base, &path, None)
    }

    fn connect(base: &Base, path: &str, protocol: Option<&str>) -> Option<Socket> {
        // no receive timeout: a socket waits as long as nobody writes (the pings notice a dead link)
        let s = session(0)?;
        // SAFETY: a live session
        let c = Handle::new(unsafe {
            WinHttpConnect(s.0, &HSTRING::from(base.host.as_str()), base.port, 0)
        })?;
        let r = request(base, &c, w!("GET"), path)?;
        // SAFETY: a live request; the header outlives the calls
        unsafe {
            WinHttpSetOption(
                Some(r.0.cast_const()),
                WINHTTP_OPTION_UPGRADE_TO_WEB_SOCKET,
                None,
            )
            .ok()?;
            if let Some(p) = protocol {
                let header: Vec<u16> = format!("Sec-WebSocket-Protocol: {p}")
                    .encode_utf16()
                    .collect();
                WinHttpAddRequestHeaders(r.0, &header, WINHTTP_ADDREQ_FLAG_ADD).ok()?;
            }
            WinHttpSendRequest(r.0, None, None, 0, 0, 0).ok()?;
            WinHttpReceiveResponse(r.0, std::ptr::null_mut()).ok()?;
        }
        if status_code(&r) != 101 {
            return None;
        }
        // SAFETY: a live request that answered 101
        let ws = unsafe { WinHttpWebSocketCompleteUpgrade(r.0, None) };
        if ws.is_null() {
            return None;
        }
        drop(r);
        Some(Socket {
            ws,
            closed: AtomicBool::new(false),
            _session: s,
            _connect: c,
        })
    }

    /// The next whole text message; Closed when the socket ends (or is closed from here).
    pub fn receive(&self) -> Frame {
        let mut message = Vec::new();
        let mut buf = vec![0u8; 8192];
        loop {
            if self.closed.load(Ordering::Acquire) {
                return Frame::Closed;
            }
            let mut read = 0u32;
            let mut kind = WINHTTP_WEB_SOCKET_BUFFER_TYPE(0);
            // SAFETY: a live socket (not closed: checked above, and close only cancels this call); the buffer is ours
            let err = unsafe {
                WinHttpWebSocketReceive(
                    self.ws,
                    buf.as_mut_ptr().cast(),
                    buf.len() as u32,
                    &mut read,
                    &mut kind,
                )
            };
            if err != 0 || kind == WINHTTP_WEB_SOCKET_CLOSE_BUFFER_TYPE {
                return Frame::Closed;
            }
            message.extend_from_slice(&buf[..read as usize]);
            if message.len() > 256 * 1024 {
                return Frame::Closed;
            }
            if kind == WINHTTP_WEB_SOCKET_UTF8_MESSAGE_BUFFER_TYPE {
                return Frame::Text(String::from_utf8_lossy(&message).into_owned());
            }
            if kind != WINHTTP_WEB_SOCKET_UTF8_FRAGMENT_BUFFER_TYPE {
                // a binary frame: not the relay's; skip it
                message.clear();
            }
        }
    }

    pub fn send(&self, text: &str) -> bool {
        if self.closed.load(Ordering::Acquire) {
            return false;
        }
        // SAFETY: a live socket; the text outlives the call
        unsafe {
            WinHttpWebSocketSend(
                self.ws,
                WINHTTP_WEB_SOCKET_UTF8_MESSAGE_BUFFER_TYPE,
                Some(text.as_bytes()),
            ) == 0
        }
    }

    /// Sends the close frame with `code` without waiting for the other side's (a receive waiting on the socket then
    /// gets that, and ends). `close` still has to follow.
    pub fn shutdown(&self, code: u16) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        // SAFETY: a live socket; shutdown may run beside a pending receive
        let _ = unsafe { WinHttpWebSocketShutdown(self.ws, code, None, 0) };
    }

    /// Ends the socket (and a receive waiting on it), once.
    pub fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        // SAFETY: a live socket, closed once; closing the handle cancels a pending receive (a graceful
        // WinHttpWebSocketClose would wait on the receive the other thread is blocked in)
        let _ = unsafe { WinHttpCloseHandle(self.ws) };
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_loopback_relay_is_taken_from_the_environment() {
        assert_eq!(
            local_base("http://localhost:8787/"),
            Some(Base {
                secure: false,
                host: "localhost".into(),
                port: 8787
            })
        );
        assert_eq!(local_base("http://127.0.0.1").map(|b| b.port), Some(80));
        assert_eq!(local_base("https://localhost:8787"), None);
        assert_eq!(local_base("http://evil.example:80"), None);
        assert_eq!(local_base("http://localhost:x"), None);
    }

    #[test]
    fn websocket_urls_split_into_host_port_and_path() {
        let (b, p) = ws_url("wss://qcalc-relay.maxconine.workers.dev/rooms/pong?name=a b").unwrap();
        assert_eq!((b.secure, b.host.as_str(), b.port), (true, RELAY_HOST, 443));
        assert_eq!(p, "/rooms/pong?name=a b");
        let (b, p) = ws_url("ws://127.0.0.1:8787/host").unwrap();
        assert_eq!(
            (b.secure, b.host.as_str(), b.port, p.as_str()),
            (false, "127.0.0.1", 8787, "/host")
        );
        let (b, p) = ws_url("ws://localhost?x=1").unwrap();
        assert_eq!((b.port, p.as_str()), (80, "/?x=1"));
        assert_eq!(ws_url("wss://host").unwrap().1, "/");
        assert_eq!(ws_url("https://host/x"), None);
        assert_eq!(ws_url("ws://:80/x"), None);
        assert_eq!(ws_url("ws://host:nope/x"), None);
        let (b, _) = ws_url("ws://[::1]:8787/x").unwrap();
        assert_eq!((b.host.as_str(), b.port), ("::1", 8787));
        assert_eq!(ws_url("ws://[::1]/x").unwrap().0.port, 80);
    }
}
