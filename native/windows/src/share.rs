//! Clips and files between names, sealed end to end (the Mac's MessageFiles.swift and ShareCommands.swift; the
//! rules and words are the core's `app::messaging`, the crypto share_crypt.rs, the keys share_keys.rs, the relay's
//! side relay/src/files.js; native/PRIVACY.md).
//!
//! - **Sending.** `paste to <name>` / `send clipboard to <name>` share the clipboard: text up to 64 KB as a clip,
//!   longer text as `clipboard.txt`, copied files as files. `send file to <name>` and ↵ picks a file. Files dropped
//!   on the bar (or pasted with Ctrl+V after Explorer's Copy) go to whoever the line names (`message max…`,
//!   `send file to max`), or wait there for a name; more than 20 at once send none and say why, and a folder says
//!   to zip it. A file is encrypted to a temporary file, its facts (name, type, size, SHA-256, its own key) sealed
//!   for the recipient alone and offered; the upload waits for their accept, part by part with the relay's grant,
//!   a part cut short tried again a few times, and a failed upload keeps its sealed file for Retry.
//! - **Receiving.** A sealed clip or offer from a pinned (or first-seen) key opens off the UI thread: an accepted
//!   name's becomes a card (Copy; Save), someone new's waits behind their request card. One that doesn't open is
//!   checked against the relay: a changed key holds it for Trust New Key (on a card, with the safety code); else a
//!   card says it couldn't be opened. Save accepts, downloads (never more than the offered length) and saves
//!   through `share_crypt::safe_save`: checked, marked with the Mark of the Web, never opened on its own.
//!
//! Nothing here runs before messaging has a name; every relay call and every file read or write runs on a worker
//! thread (`bg::spawn`), and results come back through `bg::post`.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use qcalc_core::app::messaging::{self as core, Arrival};
use serde_json::{Value, json};

use crate::app::App;
use crate::messaging::Act;
use crate::model::BarModel;
use crate::share_crypt as crypt;
use crate::share_keys::{self as keys, Found};
use crate::{bg, qlog};

/// What a card carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Clip(String),
    /// a file's id (its transfer)
    File(String),
    /// shares held from this name until their new key is trusted
    KeyChanged(String),
}

#[derive(Clone, Debug)]
struct Transfer {
    mine: bool,
    who: String,
    name: String,
    size: u64,
    state: String,
    percent: Option<u32>,
    error: Option<String>,
    /// where a received file was saved (Show in Folder)
    saved: Option<PathBuf>,
    /// when it was sent or offered, and when it was saved or sent (ms since 1970; the e2e suite's timings)
    started_ms: f64,
    done_ms: f64,
}

impl Transfer {
    fn new(mine: bool, who: &str, name: &str, size: u64, state: &str) -> Transfer {
        Transfer {
            mine,
            who: who.into(),
            name: name.into(),
            size,
            state: state.into(),
            percent: None,
            error: None,
            saved: None,
            started_ms: crate::model::now_ms() as f64,
            done_ms: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
struct Incoming {
    key: String,
    sha256: String,
    size: u64,
    bytes: u64,
    expires: f64,
    /// the part length the relay streams it in (from the accept's answer; 0 until then, or when it doesn't)
    stream: u64,
    /// the parts landed (a `part` event wakes the download), and Cancel or a take-back stopping it
    landed: Arc<Signal>,
}

struct Outgoing {
    sealed: PathBuf,
    bytes: u64,
    cancelled: Arc<AtomicBool>,
    /// how much of `sealed` is written: it's sealed while it's offered and uploaded
    sealing: Arc<Signal>,
    /// an upload is running (Retry, or a second `accepted`, never starts another beside it)
    running: Arc<AtomicBool>,
    /// the accept's grant and stream length, and whether the relay has started the upload: Retry before it has
    /// starts it with the grant again
    grant: String,
    stream: u64,
    started: Arc<AtomicBool>,
}

/// A count worker threads wait on (the sealed bytes written so far, the parts landed), and whether it ended
/// (sealing failed, or the download was stopped).
#[derive(Default, Debug)]
struct Signal {
    m: Mutex<(u64, bool)>,
    cv: Condvar,
}

impl Signal {
    fn set(&self, v: u64) {
        if let Ok(mut g) = self.m.lock() {
            g.0 = v;
        }
        self.cv.notify_all();
    }

    fn bump(&self) {
        if let Ok(mut g) = self.m.lock() {
            g.0 += 1;
        }
        self.cv.notify_all();
    }

    fn end(&self) {
        if let Ok(mut g) = self.m.lock() {
            g.1 = true;
        }
        self.cv.notify_all();
    }

    fn get(&self) -> (u64, bool) {
        self.m.lock().map(|g| *g).unwrap_or((0, true))
    }

    /// Waits up to `ms` for the count to reach `at`: Some(true) when it did, Some(false) when it ended first, None
    /// when time ran out.
    fn wait_for(&self, at: u64, ms: u64) -> Option<bool> {
        let Ok(g) = self.m.lock() else {
            return Some(false);
        };
        let (g, _) = self
            .cv
            .wait_timeout_while(g, std::time::Duration::from_millis(ms), |(v, ended)| {
                *v < at && !*ended
            })
            .ok()?;
        if g.0 >= at {
            Some(true)
        } else if g.1 {
            Some(false)
        } else {
            None
        }
    }
}

#[derive(Default)]
struct Share {
    transfers: BTreeMap<String, Transfer>,
    incoming: BTreeMap<String, Incoming>,
    outgoing: BTreeMap<String, Outgoing>,
    /// card id → what it carries
    cards: BTreeMap<String, Kind>,
    /// shares from someone not allowed yet: (from, clip, file id)
    held: Vec<(String, Option<String>, Option<String>)>,
    /// shares held while their sender's key had changed: (from, the event)
    held_for_key: Vec<(String, Value)>,
    seen: HashSet<String>,
    /// files dropped on the bar, waiting for the line to name someone
    pending: Vec<PathBuf>,
    /// an `accepted` (its grant and stream length) that came before the offer's own answer: a recipient who saves
    /// on their own can accept that fast
    early: BTreeMap<String, (String, u64, std::time::Instant)>,
}

thread_local! {
    static STATE: RefCell<Share> = RefCell::new(Share::default());
}

fn with<R>(f: impl FnOnce(&mut Share) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

const RETRYABLE: [&str; 3] = ["failed", "no-network", "network"];
const PART_TRIES: u32 = 3;
/// parts sent side by side, each on a warm connection of its own (relay.rs `shared`)
const UPLOADS: usize = 4;
/// parts read side by side while a streamed file comes in, and how far ahead of the one being opened
const FETCHES: usize = 3;
/// a streamed part is a whole number of sealed chunks (relay/src/files.js `streamPartBytes`)
const SEALED_CHUNK: u64 = (crypt::CHUNK + crypt::TAG) as u64;
/// partial downloads older than this, left by a run that quit mid-way, are swept from Downloads
const STALE_PARTIAL: std::time::Duration = std::time::Duration::from_secs(24 * 3600);
/// a streamed part that doesn't land in this long stops the download (the sender quit or went offline)
const STALLED: std::time::Duration = std::time::Duration::from_secs(120);

fn temp() -> PathBuf {
    static SWEPT: std::sync::Once = std::sync::Once::new();
    let d = std::env::temp_dir().join("QCalcShare");
    let _ = std::fs::create_dir_all(&d);
    // what a run that quit mid-send left (sealed files, pasted images) goes once it's a day old
    SWEPT.call_once(|| {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let old = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age >= STALE_PARTIAL);
            if old {
                let p = e.path();
                let _ = if p.is_dir() {
                    std::fs::remove_dir_all(&p)
                } else {
                    std::fs::remove_file(&p)
                };
            }
        }
    });
    d
}

/// A folder of its own under `temp()`, for a file whose name is what the recipient sees (`clipboard.png`), so two
/// copies (or two quick pastes) never write over each other.
fn temp_own() -> PathBuf {
    let d = temp().join(crypt::new_id());
    let _ = std::fs::create_dir_all(&d);
    d
}

/// Where Save puts files: Downloads, or `QCALC_DOWNLOADS` (the e2e suite's folder).
fn downloads() -> Option<PathBuf> {
    match std::env::var_os("QCALC_DOWNLOADS") {
        Some(d) if !d.is_empty() => {
            let d = PathBuf::from(d);
            let _ = std::fs::create_dir_all(&d);
            Some(d)
        }
        _ => crypt::downloads(),
    }
}

fn auth() -> Option<[(String, String); 2]> {
    let (name, key) = crate::messaging::credentials();
    (!name.is_empty() && !key.is_empty()).then(|| {
        [
            ("X-QCalc-Name".to_string(), name),
            ("X-QCalc-Key".to_string(), key),
        ]
    })
}

/// A file request to the relay with the name's headers (blocking).
fn call(
    method: &str,
    path: &str,
    extra: &[(&str, &str)],
    body: crate::relay::Body,
    sink: crate::relay::Sink,
) -> (String, Value) {
    let Some(a) = auth() else {
        return ("not-yours".into(), Value::Null);
    };
    let mut headers: Vec<(&str, &str)> = a.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    headers.extend_from_slice(extra);
    crate::relay::call(method, path, &headers, body, 120_000, sink)
}

/// A request with no body.
fn ask(method: &str, path: &str, extra: &[(&str, &str)]) -> (String, Value) {
    call(method, path, extra, crate::relay::Body::Bytes(&[]), None)
}

fn forget_offer(id: &str) {
    let path = format!("/file/{id}");
    bg::spawn("qcalc-share", move || {
        let _ = ask("DELETE", &path, &[]);
    });
}

// ---- settings › messages: files ----

/// An accepted name's files up to this many bytes save on their own (0: never); strangers' always ask.
fn auto_limit(app: &App) -> u64 {
    app.store.settings.auto_accept_bytes
}

/// Settings › Messages' file rows (messaging.rs adds them to the page).
pub fn settings_items(app: &App) -> Vec<crate::settings_window::Item> {
    use crate::settings_window::{Control, Item, row};
    let limit = auto_limit(app);
    let options = [
        ("0", "Off"),
        ("10000000", "Up to 10 MB"),
        ("100000000", "Up to 100 MB"),
        ("1073741824", "Any size"),
    ];
    let value = options
        .iter()
        .rev()
        .find(|(v, _)| v.parse::<u64>().is_ok_and(|v| v <= limit))
        .map_or("0", |(v, _)| v);
    vec![
        Item::Heading("Files".into()),
        row(
            "autoAccept",
            "Save files without asking",
            "Only from allowed names. Files go to Downloads and are never opened. Others wait for Save.",
            Control::Select {
                options: options
                    .iter()
                    .map(|(v, l)| ((*v).to_string(), (*l).to_string()))
                    .collect(),
                value: value.into(),
            },
        ),
        row(
            "receivedToClipboard",
            "Copy received files",
            "A saved file goes on the clipboard, so Ctrl+V pastes it in Explorer or another app.",
            Control::Toggle(app.store.settings.received_to_clipboard),
        ),
    ]
}

/// A file row changed on the Messages page: true when it was one of these.
pub fn settings_changed(app: &mut App, id: &str, value: &Value) -> bool {
    match id {
        "autoAccept" => {
            let n = value
                .as_str()
                .and_then(|s| s.parse::<u64>().ok())
                .or_else(|| value.as_u64())
                .unwrap_or(0);
            set_prefs(app, Some(n), None);
        }
        "receivedToClipboard" => set_prefs(app, None, value.as_bool()),
        _ => return false,
    }
    true
}

/// The file settings changed (Settings › Messages, the e2e suite), saved with the rest.
pub fn set_prefs(app: &mut App, auto: Option<u64>, clipboard: Option<bool>) {
    if let Some(n) = auto {
        app.store.settings.auto_accept_bytes = n.min(crate::store::AUTO_ACCEPT_MAX);
    }
    if let Some(on) = clipboard {
        app.store.settings.received_to_clipboard = on;
    }
    app.save_store();
}

// ---- the line: hints and ↵ ----

/// The line's hint while `paste to …` / `send file to …` is typed (commands.rs).
pub fn hint(what: &str, to: &str, name: &str) -> String {
    let cmd = core::ShareCommand {
        what: what.into(),
        to: to.into(),
        name: name.into(),
    };
    let (me, key) = crate::messaging::credentials();
    let m = core::Messaging {
        name: me,
        key,
        ..core::Messaging::default()
    };
    let pending = with(|s| s.pending.clone());
    let dropped = pending.first().map_or(String::new(), |p| {
        core::dropped_label(
            &p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            pending.len() as u32,
        )
    });
    if !name.is_empty() {
        crate::messaging::check_name(name);
    }
    crate::share_drop::ensure_soon();
    core::share_hint(&cmd, &m, true, crate::messaging::found_name(name), &dropped)
}

/// ↵ on a share line (commands.rs): the clipboard or a file goes to `name`; with no name set, settings open.
pub fn enter(app: &mut App, what: &str, name: &str) {
    if !crate::messaging::has_name() {
        crate::commands::clear_line(app, false);
        crate::settings_window::open(app);
        return;
    }
    if name.is_empty() {
        return;
    }
    let name = name.to_string();
    crate::commands::clear_line(app, false);
    if what == "clip" {
        send_clipboard(app, &name);
        return;
    }
    let dropped = with(|s| std::mem::take(&mut s.pending));
    if !dropped.is_empty() {
        send_files(app, dropped, &name);
        return;
    }
    pick_and_send(app, &name);
}

/// The open dialog (a thread's 📎, `send file to <name>` ↵): the picked file goes to `name`.
pub fn pick_and_send(app: &mut App, name: &str) {
    if !crate::messaging::has_name() {
        crate::settings_window::open(app);
        return;
    }
    // the picker takes the keys (our own window, so the bar stays); it runs outside the app's borrow
    let owner = app.bar.hwnd.0 as usize;
    let name = name.to_string();
    crate::later::free("share-pick", 0, move || {
        let picked = pick_file(owner);
        bg::post(move |app| {
            if let Some(p) = picked {
                send_files(app, vec![p], &name);
            }
        });
    });
}

/// Whether the bar stays up when another window is clicked (a file can be dragged in from Explorer): only for a
/// messaging line (`message …`, `send file to …`, `paste to …`) or files waiting for a name.
pub fn holds_open(m: &BarModel) -> bool {
    if with(|s| !s.pending.is_empty()) || crate::threads::panel().is_some() {
        return true;
    }
    let t = m.field.text();
    core::is_message_command(t) && core::parse_message(t).is_some()
        || core::parse_share_command(t).is_some()
}

fn say(app: &mut App, note: String) {
    crate::a11y::announce(&note);
    app.model.hint = Some(note);
    app.model.touch();
}

/// `say`, on the open thread's reply line when it's `to`'s, else under the bar.
fn say_to(app: &mut App, to: &str, note: String) {
    if crate::threads::say(to, &note) {
        crate::a11y::announce(&note);
        app.model.touch();
    } else {
        say(app, note);
    }
}

// ---- sending a clip ----

/// The clipboard to `to`: copied files as files, text as a clip (a text file past 64 KB).
pub fn send_clipboard(app: &mut App, to: &str) {
    let files = clipboard_files(app.bar.hwnd);
    if !files.is_empty() {
        send_files(app, files, to);
        return;
    }
    let text = crate::clip::get(app.bar.hwnd).unwrap_or_default();
    if text.is_empty() {
        // a copied image goes as clipboard.png
        if let Some(image) = crate::clip_image::read(app.bar.hwnd) {
            let to = to.to_string();
            say_to(app, &to, "encrypting clipboard.png…".into());
            bg::spawn("qcalc-share-png", move || {
                let path = temp_own().join("clipboard.png");
                let written = crate::clip_image::png(image)
                    .is_some_and(|png| std::fs::write(&path, png).is_ok());
                bg::post(move |app| {
                    if written {
                        send_files(app, vec![path], &to);
                    } else {
                        say_to(app, &to, core::file_error_text("disk", &to));
                    }
                });
            });
            return;
        }
        say_to(
            app,
            to,
            core::share_status_text("empty-clipboard", to, "clip", ""),
        );
        return;
    }
    if text.len() > core::CLIP_MAX {
        let to = to.to_string();
        bg::spawn("qcalc-share-txt", move || {
            let path = temp_own().join("clipboard.txt");
            let written = std::fs::write(&path, text.as_bytes()).is_ok();
            bg::post(move |app| {
                if written {
                    send_files(app, vec![path], &to);
                } else {
                    say_to(app, &to, core::file_error_text("disk", &to));
                }
            });
        });
        return;
    }
    send_clip(app, text, to);
}

pub fn send_clip(app: &mut App, clip: String, to: &str) {
    let to = to.to_string();
    let (me, relay_key) = crate::messaging::credentials();
    if me.is_empty() {
        say_to(
            app,
            &to,
            core::share_status_text("no-name", &to, "clip", ""),
        );
        return;
    }
    crate::threads::first_share(app);
    say_to(app, &to, format!("sharing with {to}…"));
    let sent = clip.clone();
    bg::spawn("qcalc-share", move || {
        let status = (|| {
            let (theirs, their_text) = match keys::for_sending(&me, &relay_key, &to) {
                Found::Key(k, t) => (k, t),
                other => return found_status(&other),
            };
            let id = crypt::new_id();
            let Some(secret) = keys::secret() else {
                return "bad".to_string();
            };
            let Some(ctx) =
                core::share_context("clip", &me, &to, &id, &keys::public_text(), &their_text)
            else {
                return "bad".into();
            };
            let Ok((epk, sealed)) = crypt::seal(clip.as_bytes(), &secret, &theirs, &ctx) else {
                return "bad".into();
            };
            let body = json!({ "from": me, "key": relay_key, "to": to, "kind": "clip",
                "clip": { "id": id, "epk": epk, "sealed": sealed } });
            crate::relay::post_value("/send", &body).0
        })();
        bg::post(move |app| {
            if status == "delivered" {
                crate::messaging::note_sent(&to);
                crate::threads::record_clip(&to, true, &sent);
            }
            say_to(app, &to, core::share_status_text(&status, &to, "clip", ""));
        });
    });
}

fn found_status(f: &Found) -> String {
    match f {
        Found::Key(..) => "ok",
        Found::Changed => "key-changed",
        Found::None => "no-key",
        Found::Unknown => "unknown",
        Found::Offline(s) if s == "not-yours" => "not-yours",
        Found::Offline(_) => "network",
    }
    .into()
}

// ---- sending files ----

/// Files to `to`: folders say to zip them, more than 20 send none, each other one is offered. Shortcuts and links
/// are followed first (off the UI thread), so a `.lnk` to a folder says so too.
pub fn send_files(app: &mut App, paths: Vec<PathBuf>, to: &str) {
    if paths.is_empty() {
        return;
    }
    let too_many = core::drop_count_text(paths.len() as u32);
    if !too_many.is_empty() {
        say_to(app, to, too_many);
        return;
    }
    if crate::messaging::credentials().0.is_empty() {
        say_to(app, to, core::share_status_text("no-name", to, "file", ""));
        return;
    }
    crate::threads::first_share(app);
    let to = to.to_string();
    bg::spawn("qcalc-share-paths", move || {
        let resolved: Vec<PathBuf> = paths.iter().map(|p| target_of(p)).collect();
        let files: Vec<PathBuf> = resolved.into_iter().filter(|p| !p.is_dir()).collect();
        bg::post(move |app| {
            if files.is_empty() {
                say_to(app, &to, core::file_error_text("folder", &to));
                return;
            }
            for f in files {
                send_file(app, f, &to);
            }
        });
    });
}

/// What a path is once shortcuts (`.lnk`) and symbolic links are followed (a worker: it reads the disk).
pub fn target_of(given: &Path) -> PathBuf {
    let lnk = given
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("lnk"));
    let followed = if lnk {
        shortcut_target(given).unwrap_or_else(|| given.to_path_buf())
    } else {
        given.to_path_buf()
    };
    // a symlink (or a junction) sends what it points to; `\\?\` comes off for a plain name
    match std::fs::canonicalize(&followed) {
        Ok(p) => {
            let s = p.to_string_lossy();
            match s.strip_prefix(r"\\?\") {
                Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
                _ => p,
            }
        }
        Err(_) => followed,
    }
}

/// A shell shortcut's target (IShellLinkW), None when it isn't one or points at nothing on disk.
fn shortcut_target(lnk: &Path) -> Option<PathBuf> {
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize, IPersistFile, STGM_READ,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    use windows::core::{HSTRING, Interface};
    // SAFETY: COM on this worker thread for the call's length; the buffers outlive the calls
    unsafe {
        let inited = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let out = (|| {
            let link: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
            let file: IPersistFile = link.cast().ok()?;
            file.Load(&HSTRING::from(lnk.as_os_str()), STGM_READ).ok()?;
            let mut buf = [0u16; 32_768];
            // no flags: the target as a full path, environment variables expanded
            link.GetPath(&mut buf, std::ptr::null_mut(), 0).ok()?;
            let len = buf.iter().position(|c| *c == 0).unwrap_or(0);
            let p = PathBuf::from(String::from_utf16_lossy(&buf[..len]));
            (len > 0 && p.exists()).then_some(p)
        })();
        if inited {
            CoUninitialize();
        }
        out
    }
}

fn send_file(app: &mut App, path: PathBuf, to: &str) {
    let to = to.to_string();
    let (me, relay_key) = crate::messaging::credentials();
    if me.is_empty() {
        say_to(
            app,
            &to,
            core::share_status_text("no-name", &to, "file", ""),
        );
        return;
    }
    let raw_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = core::clean_filename(&raw_name);
    let started = crate::model::now_ms() as f64;
    say_to(app, &to, format!("encrypting {name}…"));
    bg::spawn("qcalc-share", move || {
        let result = offer(&path, &name, &to, &me, &relay_key);
        bg::post(move |app| match result {
            Ok(o) => {
                crate::messaging::note_sent(&to);
                crate::threads::record_file(&to, true, &o.id, &name, o.size);
                with(|s| {
                    s.outgoing.insert(
                        o.id.clone(),
                        Outgoing {
                            sealed: o.sealed,
                            bytes: o.bytes,
                            cancelled: o.cancelled,
                            sealing: o.sealing,
                            running: Arc::new(AtomicBool::new(false)),
                            grant: String::new(),
                            stream: 0,
                            started: Arc::new(AtomicBool::new(false)),
                        },
                    );
                    let mut t = Transfer::new(true, &to, &name, o.size, "offered");
                    t.started_ms = started;
                    s.transfers.insert(o.id.clone(), t);
                });
                // the accept may have come while the offer was on its way back: its upload starts now
                let early = with(|s| s.early.remove(&o.id));
                if let Some((grant, stream, _)) = early {
                    upload(&o.id, &grant, stream);
                }
                say_to(
                    app,
                    &to,
                    core::share_status_text("delivered", &to, "file", &name),
                );
                // the bar's line follows it from here: sending, then sent (or why not)
                if !crate::threads::is_open(&to) {
                    hint_about(app, &o.id);
                }
                persist_unfinished();
                refresh_cards(app);
            }
            Err(note) => say_to(app, &to, note),
        });
    });
}

/// An offer that went: its id, the sealed file (still being written: `sealing`), its length and the plain size.
struct Offered {
    id: String,
    sealed: PathBuf,
    bytes: u64,
    size: u64,
    sealing: Arc<Signal>,
    cancelled: Arc<AtomicBool>,
}

/// On a worker: hash, seal the facts, offer; the file is sealed into a temporary file alongside (another thread),
/// so the offer and the recipient's accept don't wait for it, and the upload reads it as it's written.
fn offer(path: &Path, name: &str, to: &str, me: &str, relay_key: &str) -> Result<Offered, String> {
    let meta = std::fs::metadata(path).map_err(|_| core::file_error_text("unreadable", to))?;
    if meta.is_dir() {
        return Err(core::file_error_text("folder", to));
    }
    let size = meta.len();
    if size > core::FILE_MAX {
        return Err(core::share_status_text("too-big", to, "file", name));
    }
    let (theirs, their_text) = match keys::for_sending(me, relay_key, to) {
        Found::Key(k, t) => (k, t),
        other => {
            return Err(core::share_status_text(
                &found_status(&other),
                to,
                "file",
                name,
            ));
        }
    };
    let key = crypt::new_file_key();
    let sealed = temp().join(crypt::new_id());
    let sealing = Arc::new(Signal::default());
    let cancelled = Arc::new(AtomicBool::new(false));
    // the hash the facts carry is read first, here; the sealing pass checks it again, so a file that changed under
    // it is never sent as something else
    let (seal_to, seal_from, progress) = (sealed.clone(), path.to_path_buf(), sealing.clone());
    let sealer = std::thread::Builder::new()
        .name("qcalc-share-seal".into())
        .spawn(move || {
            let p = progress.clone();
            let sha = crypt::encrypt_file(&seal_from, &seal_to, &key, move |n| p.set(n));
            if sha.is_err() {
                progress.end();
            }
            sha
        })
        .map_err(|_| core::file_error_text("disk", to))?;
    let fail = |status: &str| {
        cancelled.store(true, Ordering::Relaxed);
        let _ = std::fs::remove_file(&sealed);
        core::share_status_text(status, to, "file", name)
    };
    let sha = match crypt::hash_file(path) {
        Ok(sha) => sha,
        Err(e) => {
            let _ = sealer.join();
            let _ = std::fs::remove_file(&sealed);
            return Err(core::file_error_text(e.status(), to));
        }
    };
    // a sealing pass whose hash isn't this one (the file changed meanwhile) ends the upload: `sealing` ends
    let check = sealing.clone();
    let want = sha.clone();
    let _ = std::thread::Builder::new()
        .name("qcalc-share-seal-check".into())
        .spawn(move || {
            if !matches!(sealer.join(), Ok(Ok(got)) if got == want) {
                check.end();
            }
        });
    let bytes = crypt::encrypted_length(size);
    let id = crypt::new_id();
    let facts = core::seal_file_facts(
        name,
        "application/octet-stream",
        size,
        &sha,
        &crypt::encode(&key),
    )
    .ok_or_else(|| fail("bad"))?;
    let secret = keys::secret().ok_or_else(|| fail("bad"))?;
    let ctx = core::share_context("file", me, to, &id, &keys::public_text(), &their_text)
        .ok_or_else(|| fail("bad"))?;
    let (epk, boxed) =
        crypt::seal(facts.as_bytes(), &secret, &theirs, &ctx).map_err(|_| fail("bad"))?;
    let body = json!({ "from": me, "key": relay_key, "to": to, "kind": "file",
        "file": { "id": id, "epk": epk, "sealed": boxed, "bytes": bytes } });
    let (status, o) = crate::relay::post_value("/send", &body);
    if status != "delivered" || o["id"].as_str() != Some(id.as_str()) {
        return Err(fail(&status));
    }
    Ok(Offered {
        id,
        sealed,
        bytes,
        size,
        sealing,
        cancelled,
    })
}

/// The recipient said yes: upload with the grant. `stream` (the relay's part length for a streamed upload, 0 from
/// an older relay) sends parts the recipient reads as they land, the first one starting it with the grant; else
/// `start` first, as before. Retry passes no grant: the relay says which parts it has and how they're cut.
fn upload(id: &str, grant: &str, stream: u64) {
    let Some((sealed, bytes, cancelled, sealing, running, started, who, grant, stream)) =
        with(|s| {
            let o = s.outgoing.get_mut(id)?;
            if !grant.is_empty() {
                o.grant = grant.to_string();
                o.stream = stream;
            }
            // Retry: with the grant again if the relay never started it (it was cut before the first part landed)
            let (grant, stream) = if o.started.load(Ordering::Acquire) {
                (String::new(), 0)
            } else {
                (o.grant.clone(), o.stream)
            };
            let t = s.transfers.get(id).filter(|t| t.mine)?;
            Some((
                o.sealed.clone(),
                o.bytes,
                o.cancelled.clone(),
                o.sealing.clone(),
                o.running.clone(),
                o.started.clone(),
                t.who.clone(),
                grant,
                stream,
            ))
        })
    else {
        // a file this app doesn't have any more: the relay lets it go
        forget_offer(id);
        return;
    };
    if running.swap(true, Ordering::AcqRel) {
        return;
    }
    set_state(id, "uploading", Some(0), None);
    let id = id.to_string();
    bg::spawn("qcalc-share-up", move || {
        let job = Upload {
            id: &id,
            sealed: &sealed,
            bytes,
            cancelled: &cancelled,
            sealing: &sealing,
            started: &started,
            sent: AtomicI64::new(0),
            shown: AtomicU32::new(0),
        };
        let status = job.run(&grant, stream);
        running.store(false, Ordering::Release);
        bg::post(move |app| {
            if status == "ok" {
                set_state(&id, "sent", None, None);
                with(|s| {
                    if let Some(t) = s.transfers.get_mut(&id) {
                        t.done_ms = crate::model::now_ms() as f64;
                    }
                });
                drop_outgoing(&id);
            } else if status != "cancelled" {
                failed(&id, &status, &who);
                // the bar says so too: the file's line (with Retry) is in their thread
                if !crate::threads::is_open(&who) {
                    let note = core::file_error_text(&status, &who);
                    say(
                        app,
                        if RETRYABLE.contains(&status.as_str()) {
                            format!("{note} · messages {who} to retry")
                        } else {
                            note
                        },
                    );
                }
            }
            refresh_cards(app);
        });
    });
}

/// One upload's parts, on a worker.
struct Upload<'a> {
    id: &'a str,
    sealed: &'a Path,
    bytes: u64,
    cancelled: &'a AtomicBool,
    sealing: &'a Signal,
    /// the relay has started it (Retry then needs no grant)
    started: &'a AtomicBool,
    /// bytes sent so far (a part cut short takes its back), and the percent last shown
    sent: AtomicI64,
    shown: AtomicU32,
}

impl Upload<'_> {
    fn run(&self, grant: &str, stream: u64) -> String {
        // a streamed upload with the grant starts with its first part; anything else asks `start`
        let fresh = streamable(stream) && !grant.is_empty();
        let (size, parts, done) = if fresh {
            (stream, self.bytes.div_ceil(stream).max(1), HashSet::new())
        } else {
            let headers: Vec<(&str, &str)> = if grant.is_empty() {
                vec![]
            } else {
                vec![("X-QCalc-Grant", grant)]
            };
            let (status, o) = ask("POST", &format!("/file/{}/start", self.id), &headers);
            // already all in and put together
            if status == "done" {
                return "ok".into();
            }
            if status != "ok" {
                return status;
            }
            self.started.store(true, Ordering::Release);
            let size = o["partBytes"].as_u64().unwrap_or(0).max(1);
            let done: HashSet<u64> = o["done"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_u64).collect())
                .unwrap_or_default();
            (size, o["parts"].as_u64().unwrap_or(1).max(1), done)
        };
        for n in &done {
            self.count(self.length(*n, size) as i64);
        }
        let mut todo: Vec<u64> = (1..=parts).filter(|n| !done.contains(n)).collect();
        if fresh && todo.first() == Some(&1) {
            let s = self.part(
                1,
                size,
                &[("X-QCalc-Grant", grant), ("X-QCalc-Stream", "1")],
            );
            if s != "ok" {
                return s;
            }
            self.started.store(true, Ordering::Release);
            todo.remove(0);
        }
        // the rest side by side, each on a connection of its own
        let queue = Mutex::new(todo.into_iter());
        let failure: Mutex<Option<String>> = Mutex::new(None);
        std::thread::scope(|scope| {
            for _ in 0..UPLOADS {
                scope.spawn(|| {
                    loop {
                        if failure.lock().map_or(true, |f| f.is_some()) {
                            return;
                        }
                        let Some(n) = queue.lock().ok().and_then(|mut q| q.next()) else {
                            return;
                        };
                        let s = self.part(n, size, &[]);
                        if s != "ok" {
                            if let Ok(mut f) = failure.lock() {
                                f.get_or_insert(s);
                            }
                            return;
                        }
                    }
                });
            }
        });
        if let Some(s) = failure.into_inner().ok().flatten() {
            return s;
        }
        let (status, _) = ask("POST", &format!("/file/{}/finish", self.id), &[]);
        match status.as_str() {
            "ok" | "done" => "ok".into(),
            // a part the relay didn't keep: Retry sends what's missing
            "incomplete" => "failed".into(),
            _ => status,
        }
    }

    fn length(&self, n: u64, size: u64) -> u64 {
        size.min(self.bytes.saturating_sub((n - 1) * size))
    }

    /// Bytes sent (or taken back), and the percent shown when it moved.
    fn count(&self, delta: i64) {
        let sent = self.sent.fetch_add(delta, Ordering::Relaxed) + delta;
        let pct = (sent.max(0) as u64 * 100 / self.bytes.max(1)).min(100) as u32;
        let was = self.shown.load(Ordering::Relaxed);
        if pct != was
            && self
                .shown
                .compare_exchange(was, pct, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            let id = self.id.to_string();
            bg::post(move |app| {
                progress(&id, pct);
                refresh_cards(app);
            });
        }
    }

    /// Part `n`, read from the sealed file once that much of it is written, a piece at a time; a part cut short goes
    /// again, a few times, a little later each time.
    fn part(&self, n: u64, size: u64, extra: &[(&str, &str)]) -> String {
        let offset = (n - 1) * size;
        let len = self.length(n, size);
        loop {
            if self.cancelled.load(Ordering::Relaxed) {
                return "cancelled".into();
            }
            match self.sealing.wait_for(offset + len, 250) {
                Some(true) => break,
                Some(false) => return "unreadable".into(),
                None => {}
            }
        }
        let path = format!("/file/{}/part/{n}", self.id);
        let mut headers = vec![("Content-Type", "application/octet-stream")];
        headers.extend_from_slice(extra);
        let mut tries = 0;
        loop {
            // the e2e suite's window to take the relay down mid-upload
            if let Some(ms) = std::env::var("QCALC_E2E_SLOW_UPLOAD_MS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|ms| *ms > 0)
            {
                std::thread::sleep(std::time::Duration::from_millis(ms));
            }
            let part_sent = AtomicI64::new(0);
            let sent = |k: u64| {
                part_sent.fetch_add(k as i64, Ordering::Relaxed);
                self.count(k as i64);
            };
            let (status, _) = call(
                "PUT",
                &path,
                &headers,
                crate::relay::Body::File {
                    path: self.sealed,
                    offset,
                    len,
                    stop: self.cancelled,
                    sent: &sent,
                },
                None,
            );
            if status == "ok" {
                return status;
            }
            self.count(-part_sent.load(Ordering::Relaxed));
            if self.cancelled.load(Ordering::Relaxed) {
                return "cancelled".into();
            }
            tries += 1;
            if !RETRYABLE.contains(&status.as_str()) || tries >= PART_TRIES {
                return status;
            }
            // a little later each time, Take Back heard meanwhile
            for _ in 0..(20u64 << tries) {
                if self.cancelled.load(Ordering::Relaxed) {
                    return "cancelled".into();
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }
}

fn drop_outgoing(id: &str) {
    if let Some(o) = with(|s| s.outgoing.remove(id)) {
        o.cancelled.store(true, Ordering::Relaxed);
        let _ = std::fs::remove_file(&o.sealed);
    }
}

fn failed(id: &str, status: &str, who: &str) {
    set_state(id, "failed", None, Some(core::file_error_text(status, who)));
    // an upload that can pick up where it stopped keeps its sealed file for Retry
    let mine = with(|s| s.transfers.get(id).is_some_and(|t| t.mine));
    if mine && !RETRYABLE.contains(&status) {
        drop_outgoing(id);
    }
}

fn can_retry(id: &str) -> bool {
    with(|s| {
        s.transfers
            .get(id)
            .is_some_and(|t| t.mine && t.state == "failed")
            && s.outgoing.contains_key(id)
    })
}

/// The sender takes it back: the upload stops (mid-part), the relay lets it go and the recipient hears.
fn take_back(id: &str) {
    let mine = with(|s| {
        s.transfers.get(id).is_some_and(|t| {
            t.mine && matches!(t.state.as_str(), "offered" | "uploading" | "failed")
        })
    });
    if !mine {
        return;
    }
    drop_outgoing(id);
    with(|s| s.early.remove(id));
    set_state(id, "withdrawn", None, None);
    forget_offer(id);
}

// ---- what arrives ----

/// A frame from the inbox (messaging.rs): a clip or an offer (`message` with a kind) or a file's event. True when
/// it was one of ours.
pub fn frame(app: &mut App, text: &str) -> bool {
    let Some(body) = text.strip_prefix('!') else {
        return false;
    };
    // the usual text message never reaches the parse below
    if !body.contains("\"kind\"") && !body.contains("\"type\":\"file\"") {
        return false;
    }
    let Ok(event) = serde_json::from_str::<Value>(body) else {
        return false;
    };
    match event["type"].as_str() {
        Some("message") if event["kind"].as_str().is_some_and(|k| k != "text") => {
            heard(app, event);
            true
        }
        Some("file") => {
            file_event(app, &event);
            true
        }
        _ => false,
    }
}

/// The inbox connected: the key is published again (off the UI thread).
pub fn inbox_ready() {
    let (name, key) = crate::messaging::credentials();
    bg::spawn("qcalc-share-key", move || keys::publish(&name, &key));
    forget_last_run();
    // a `ready` sent while the inbox was down is never sent again: a file still waiting is asked for now (the relay
    // says `not-ready` if it isn't, and it stays waiting)
    let waiting: Vec<String> = with(|s| {
        s.transfers
            .iter()
            .filter(|(_, t)| !t.mine && t.state == "waiting")
            .map(|(id, _)| id.clone())
            .collect()
    });
    for id in waiting {
        download(&id);
    }
    // a streamed download waiting out a dropped link looks again now
    for inc in with(|s| {
        s.incoming
            .values()
            .map(|i| i.landed.clone())
            .collect::<Vec<_>>()
    }) {
        inc.bump();
    }
}

fn heard(app: &mut App, event: Value) {
    let from = core::clean_name(&event["from"]);
    let kind = event["kind"].as_str().unwrap_or("").to_string();
    if from.is_empty() || crate::messaging::arrival_of(&from) == Arrival::Drop {
        // a blocked name's file is turned down unseen
        if let Some(id) = event["file"]["id"].as_str() {
            forget_offer(id);
        }
        return;
    }
    let share = match kind.as_str() {
        "clip" => core::clean_sealed_clip(&event["clip"]),
        "file" => core::clean_sealed_offer(&event["file"]),
        _ => return,
    };
    let Some(share) = share else {
        qlog!("share in: {kind} from {from}, not sealed");
        unopened(app, &kind, &from);
        return;
    };
    if with(|s| s.seen.contains(&share.id)) {
        return;
    }
    qlog!("share in: {kind} from {from}");
    let (me, relay_key) = crate::messaging::credentials();
    bg::spawn("qcalc-share-in", move || {
        let opened = (|| {
            let Some(secret) = keys::secret() else {
                return Err("no key");
            };
            match keys::for_receiving(&me, &relay_key, &from) {
                Found::Key(theirs, their_text) => {
                    let ctx = core::share_context(
                        &kind,
                        &from,
                        &me,
                        &share.id,
                        &their_text,
                        &keys::public_text(),
                    );
                    if let Some(ctx) = ctx
                        && let Ok(plain) =
                            crypt::open(&share.sealed, &share.epk, &secret, &theirs, &ctx)
                    {
                        return Ok(plain);
                    }
                    // most likely sealed for a key this app had before: the relay gets this one again
                    keys::republish(&me, &relay_key);
                    if keys::recheck(&me, &relay_key, &from) {
                        Err("changed")
                    } else {
                        Err("didn't open")
                    }
                }
                Found::Changed => Err("changed"),
                Found::None => Err("no key"),
                Found::Unknown => Err("unknown name"),
                Found::Offline(_) => Err("relay unreachable"),
            }
        })();
        bg::post(move |app| match opened {
            Ok(plain) => {
                if !with(|s| s.seen.insert(share.id.clone())) {
                    return;
                }
                if kind == "clip" {
                    received_clip(app, &plain, &from);
                } else {
                    received_facts(app, &plain, &share, &from);
                }
            }
            Err("changed") => {
                let first = with(|s| {
                    let first = !s.held_for_key.iter().any(|(w, _)| *w == from);
                    s.held_for_key.push((from.clone(), event));
                    first
                });
                // an open thread shows it (and Trust New Key) itself
                if crate::threads::is_open(&from) {
                    crate::a11y::announce(&core::key_changed_text(&from));
                    app.model.touch();
                } else if first {
                    crate::messaging::share_card(
                        app,
                        &from,
                        &format!(
                            "{} · {}",
                            core::key_changed_text(&from),
                            core::key_changed_detail(&from)
                        ),
                        Some(Kind::KeyChanged(from.clone())),
                    );
                }
            }
            Err(why) => {
                qlog!("share in: {kind} from {from} not opened ({why})");
                if kind == "file" {
                    forget_offer(&share.id);
                }
                unopened(app, &kind, &from);
            }
        });
    });
}

/// A clip or file that came but couldn't be opened: a card says so (never lost without a word).
fn unopened(app: &mut App, kind: &str, from: &str) {
    if crate::messaging::arrival_of(from) != Arrival::Show {
        return;
    }
    let note = if kind == "file" {
        "sent a file that couldn’t be opened on this pc · ask them to send it again"
    } else {
        "shared their clipboard, but it couldn’t be opened on this pc · ask them to send it again"
    };
    // a line is left in their thread, so it's never lost without a word
    crate::threads::record_text(from, false, note);
    if crate::threads::is_open(from) {
        crate::threads::heard(from, "message", note);
        app.model.touch();
        return;
    }
    crate::messaging::share_card(app, from, note, None);
}

fn received_clip(app: &mut App, plain: &[u8], from: &str) {
    let Ok(text) = String::from_utf8(plain.to_vec()) else {
        return;
    };
    let Some(clip) = core::clean_clip(&Value::String(text)) else {
        return;
    };
    show_or_ask(app, from, Some(clip), None);
}

fn received_facts(app: &mut App, plain: &[u8], share: &core::SealedShare, from: &str) {
    let facts = std::str::from_utf8(plain)
        .ok()
        .and_then(|t| core::open_file_facts(t, share.bytes))
        .filter(|f| crypt::file_key(&f.key).is_some());
    let Some(f) = facts else {
        forget_offer(&share.id);
        return;
    };
    let accepted = crate::messaging::arrival_of(from) == Arrival::Show;
    with(|s| {
        s.incoming.insert(
            share.id.clone(),
            Incoming {
                key: f.key.clone(),
                sha256: f.sha256.clone(),
                size: f.size,
                bytes: share.bytes,
                expires: share.expires,
                stream: 0,
                landed: Arc::new(Signal::default()),
            },
        );
        s.transfers.insert(
            share.id.clone(),
            Transfer::new(
                false,
                from,
                &f.name,
                f.size,
                if accepted { "received" } else { "request" },
            ),
        );
    });
    persist_unfinished();
    // an allowed name's file under the size settings says saves on its own, at once (the Mac's autoAccept)
    let auto = accepted && {
        let limit = auto_limit(app);
        crate::messaging::auto_accept(from, f.size, limit)
    };
    show_or_ask(app, from, None, Some(share.id.clone()));
    if auto {
        save(app, &share.id);
        refresh_cards(app);
    }
}

fn show_or_ask(app: &mut App, from: &str, clip: Option<String>, file: Option<String>) {
    if crate::messaging::arrival_of(from) == Arrival::Show {
        show(app, from, clip, file);
    } else {
        with(|s| s.held.push((from.to_string(), clip, file)));
        crate::messaging::ask_share(app, from);
    }
}

fn show(app: &mut App, from: &str, clip: Option<String>, file: Option<String>) {
    let (text, kind) = match (clip, file) {
        (Some(c), _) => {
            let first = core::clip_preview(&c);
            (
                format!(
                    "shared their clipboard · {}",
                    if first.is_empty() {
                        "clipboard"
                    } else {
                        &first
                    }
                ),
                Kind::Clip(c),
            )
        }
        (None, Some(id)) => (file_text(&id), Kind::File(id)),
        (None, None) => return,
    };
    let said = match &kind {
        Kind::File(id) => {
            let t = with(|s| s.transfers.get(id).map(|t| (t.name.clone(), t.size)));
            let (name, size) = t.unwrap_or_default();
            crate::threads::record_file(from, false, id, &name, size);
            core::card_announcement(from, "file", &name)
        }
        Kind::Clip(c) => {
            crate::threads::record_clip(from, false, c);
            core::card_announcement(from, "clip", "")
        }
        Kind::KeyChanged(_) => String::new(),
    };
    crate::a11y::announce(&said);
    // with their thread open it lands there, not as a card
    if crate::threads::is_open(from) {
        app.model.touch();
        return;
    }
    crate::messaging::share_card(app, from, &text, Some(kind));
}

/// Allow or Block on someone's request (messaging.rs): their shares show, or their files are turned down.
pub fn answered(app: &mut App, who: &str, allowed: bool) {
    let theirs: Vec<_> = with(|s| {
        let (theirs, rest) = std::mem::take(&mut s.held)
            .into_iter()
            .partition(|(f, ..)| f == who);
        s.held = rest;
        theirs
    });
    for (_, clip, file) in theirs {
        if allowed {
            if let Some(id) = &file {
                set_state(id, "received", None, None);
            }
            show(app, who, clip, file);
        } else if let Some(id) = file {
            decline(&id);
        }
    }
}

fn file_text(id: &str) -> String {
    with(|s| {
        let Some(t) = s.transfers.get(id) else {
            return String::new();
        };
        let size = core::file_size_text(t.size);
        let state = t
            .error
            .clone()
            .unwrap_or_else(|| core::file_state_text(&t.state, &t.who, t.percent));
        let head = if t.mine {
            format!("{} ({size}) to {}", t.name, t.who)
        } else {
            format!("sent you {} ({size})", t.name)
        };
        if state.is_empty() {
            head
        } else {
            format!("{head} · {state}")
        }
    })
}

fn set_state(id: &str, state: &str, percent: Option<u32>, error: Option<String>) {
    with(|s| {
        if let Some(t) = s.transfers.get_mut(id) {
            t.state = state.into();
            t.percent = percent;
            t.error = error;
        }
    });
    persist_unfinished();
    hint_follows(id);
}

// ---- files on their way when the app quits ----

const UNFINISHED_FILE: &str = "share-unfinished.json";

/// A file still on its way: what an app that quits leaves the other side waiting on.
fn unfinished(t: &Transfer) -> bool {
    if t.mine {
        matches!(t.state.as_str(), "offered" | "uploading" | "failed")
    } else {
        matches!(
            t.state.as_str(),
            "received" | "request" | "accepting" | "waiting" | "downloading"
        )
    }
}

static UNFINISHED: Mutex<(Option<Vec<String>>, Option<String>)> = Mutex::new((None, None));

/// The ids of the files on their way, kept beside settings.json (written off the UI thread, only when they change),
/// so that after a restart, when nothing of them is left in memory (keys, sealed files, progress), the relay lets
/// them go and the other side hears they were taken back or turned down instead of waiting on them.
fn persist_unfinished() {
    let ids: Vec<String> = with(|s| {
        s.transfers
            .iter()
            .filter(|(_, t)| unfinished(t))
            .map(|(id, _)| id.clone())
            .collect()
    });
    let first = {
        let Ok(mut g) = UNFINISHED.lock() else { return };
        if g.0.as_ref() == Some(&ids) {
            return;
        }
        g.0 = Some(ids.clone());
        g.1.replace(serde_json::to_string(&ids).unwrap_or_default())
            .is_none()
    };
    if first {
        bg::spawn("qcalc-share-unfinished", || {
            while let Some(text) = UNFINISHED.lock().ok().and_then(|mut g| g.1.take()) {
                if let Some(d) = crate::store::config_dir() {
                    let _ = std::fs::create_dir_all(&d);
                    let _ = crate::store::write_atomic(&d.join(UNFINISHED_FILE), &text);
                }
            }
        });
    }
}

/// Once the inbox first opens: files left on their way by the last run go (the relay tells the other side).
fn forget_last_run() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        bg::spawn("qcalc-share-unfinished", || {
            let ids: Vec<String> = crate::store::config_dir()
                .and_then(|d| std::fs::read(d.join(UNFINISHED_FILE)).ok())
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default();
            bg::post(move |_| {
                for id in ids {
                    let known = with(|s| s.transfers.contains_key(&id));
                    if !known && id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()) {
                        qlog!("share: letting go of a file the last run left on its way");
                        forget_offer(&id);
                    }
                }
                if let Ok(mut g) = UNFINISHED.lock() {
                    g.0 = None;
                }
                persist_unfinished();
            });
        });
    });
}

// ---- the bar's line follows a file it spoke about ----

/// The file the hint under the bar last spoke about, and what it said.
static HINT: Mutex<Option<(String, String)>> = Mutex::new(None);

/// The hint under the bar spoke about this file: it follows it as it goes.
fn hint_about(app: &mut App, id: &str) {
    if let Some(h) = &app.model.hint
        && let Ok(mut g) = HINT.lock()
    {
        *g = Some((id.to_string(), h.clone()));
    }
}

/// What the hint says for a file of mine as it goes: its words while it's on its way, and how it ended.
fn hint_text(t: &Transfer) -> Option<String> {
    let state = t
        .error
        .clone()
        .unwrap_or_else(|| core::file_state_text(&t.state, &t.who, t.percent));
    match t.state.as_str() {
        "uploading" => Some(format!("{} to {} · {state}", t.name, t.who)),
        "sent" | "declined" | "withdrawn" | "failed" => Some(format!("{} · {state}", t.name)),
        _ => None,
    }
}

/// The hint, if it still says what it last said about `id`, says where that file has got to.
fn hint_follows(id: &str) {
    let Some(text) = with(|s| s.transfers.get(id).filter(|t| t.mine).and_then(hint_text)) else {
        return;
    };
    let ours = HINT
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .filter(|(i, _)| i == id);
    let Some((_, said)) = ours else { return };
    let id = id.to_string();
    bg::post(move |app| {
        if app.model.hint.as_deref() == Some(said.as_str()) {
            app.model.hint = Some(text.clone());
            app.model.touch();
            if let Ok(mut g) = HINT.lock() {
                *g = Some((id, text));
            }
        }
    });
}

fn progress(id: &str, pct: u32) {
    with(|s| {
        if let Some(t) = s.transfers.get_mut(id)
            && matches!(t.state.as_str(), "uploading" | "downloading")
        {
            t.percent = Some(pct);
        }
    });
    hint_follows(id);
}

/// The cards' words follow their files.
fn refresh_cards(app: &mut App) {
    let files: Vec<(String, String)> = with(|s| {
        s.cards
            .iter()
            .filter_map(|(card, k)| match k {
                Kind::File(id) => Some((card.clone(), id.clone())),
                _ => None,
            })
            .collect()
    });
    for (card, id) in files {
        let text = file_text(&id);
        if let Some(c) = app.model.cards.list.iter_mut().find(|c| c.card.id == card)
            && !text.is_empty()
        {
            c.card.text = text;
        }
    }
    app.model.touch();
}

// ---- saving ----

/// Save: accept it (the sender uploads it), then download it. From a relay that streams (`stream` in the answer),
/// the download starts at once and reads each part as it lands; from an older one, once it's `ready`.
fn save(app: &mut App, id: &str) {
    let Some(t) = with(|s| s.transfers.get(id).cloned()) else {
        return;
    };
    if t.mine || !matches!(t.state.as_str(), "received" | "failed") {
        return;
    }
    crate::threads::first_share(app);
    let Some(inc) = with(|s| s.incoming.get(id).cloned()) else {
        set_state(id, "lost", None, None);
        return;
    };
    if inc.expires > 0.0 && inc.expires < crate::model::now_ms() as f64 {
        set_state(
            id,
            "failed",
            None,
            Some(core::file_error_text("expired", &t.who)),
        );
        return;
    }
    set_state(id, "accepting", None, None);
    refresh_cards(app);
    let id = id.to_string();
    bg::spawn("qcalc-share", move || {
        let (status, o) = ask("POST", &format!("/file/{id}/accept"), &[]);
        let stream = o["stream"].as_u64().unwrap_or(0);
        bg::post(move |app| {
            match status.as_str() {
                "ok" => {
                    let streams = streamable(stream);
                    with(|s| {
                        if let Some(inc) = s.incoming.get_mut(&id) {
                            inc.stream = if streams { stream } else { 0 };
                        }
                        if let Some(t) = s.transfers.get_mut(&id)
                            && t.state == "accepting"
                        {
                            t.state = "waiting".into();
                        }
                    });
                    if streams && is_state(&id, "waiting") {
                        download(&id);
                    }
                }
                // already accepted: it may be ready, or streaming
                "busy" => download(&id),
                _ => set_state(
                    &id,
                    "failed",
                    None,
                    Some(core::file_error_text(&status, &t.who)),
                ),
            }
            refresh_cards(app);
        });
    });
}

/// A part length this app can open part by part: whole sealed chunks.
fn streamable(stream: u64) -> bool {
    stream > 0 && stream.is_multiple_of(SEALED_CHUNK)
}

fn is_state(id: &str, state: &str) -> bool {
    with(|s| s.transfers.get(id).is_some_and(|t| t.state == state))
}

/// Downloads it into a hidden partial file beside where it lands, opening each sealed chunk as it comes (so
/// there's no second pass): part by part as they land when it streams, else in one once it's `ready`. Checked
/// (length, size, SHA-256), marked with the Mark of the Web, then moved in whole under a free name. Nothing partial is
/// left behind.
fn download(id: &str) {
    let Some((t, inc)) =
        with(|s| Some((s.transfers.get(id)?.clone(), s.incoming.get(id)?.clone())))
    else {
        return;
    };
    if t.state == "downloading" {
        return;
    }
    set_state(id, "downloading", Some(0), None);
    let id = id.to_string();
    bg::spawn("qcalc-share-down", move || {
        let result = Download {
            id: &id,
            inc: &inc,
            shown: AtomicU32::new(0),
        }
        .run(&t.name);
        bg::post(move |app| {
            match result {
                Ok(path) => saved(app, &id, path),
                // a sender that doesn't stream: it comes whole once it's `ready`
                Err(s) if s == "whole" => {
                    with(|s| {
                        if let Some(inc) = s.incoming.get_mut(&id) {
                            inc.stream = 0;
                        }
                    });
                    set_state(&id, "waiting", None, None);
                    download(&id);
                }
                // not uploaded yet: `ready` will come
                Err(s) if s == "not-ready" => set_state(&id, "waiting", None, None),
                // Cancel, or the sender took it back: its state is already said
                Err(s) if s == "cancelled" => {}
                Err(s) => set_state(&id, "failed", None, Some(core::file_error_text(&s, &t.who))),
            }
            refresh_cards(app);
        });
    });
}

/// It's in Downloads: Show in Folder, and on the clipboard when settings says so.
fn saved(app: &mut App, id: &str, path: PathBuf) {
    qlog!("share in: saved {}", path.display());
    set_state(id, "saved", None, None);
    with(|s| {
        s.incoming.remove(id);
        if let Some(t) = s.transfers.get_mut(id) {
            t.saved = Some(path.clone());
            t.done_ms = crate::model::now_ms() as f64;
        }
    });
    if app.store.settings.received_to_clipboard
        && crate::clip::set_files(app.bar.hwnd, std::slice::from_ref(&path))
    {
        crate::a11y::announce("copied");
    }
}

/// One download, on a worker.
struct Download<'a> {
    id: &'a str,
    inc: &'a Incoming,
    shown: AtomicU32,
}

/// Sealed bytes fed in any pieces, opened a whole chunk at a time.
struct Opener<W: std::io::Write> {
    open: crypt::StreamOpen,
    held: Vec<u8>,
    out: W,
    got: u64,
}

impl<W: std::io::Write> Opener<W> {
    fn feed(&mut self, mut piece: &[u8]) -> Result<(), crypt::Failure> {
        let whole = SEALED_CHUNK as usize;
        self.got += piece.len() as u64;
        if !self.held.is_empty() {
            let take = (whole - self.held.len()).min(piece.len());
            self.held.extend_from_slice(&piece[..take]);
            piece = &piece[take..];
            if self.held.len() == whole {
                self.open.part(&self.held, &mut self.out)?;
                self.held.clear();
            }
        }
        let full = piece.len() / whole * whole;
        if full > 0 {
            self.open.part(&piece[..full], &mut self.out)?;
        }
        self.held.extend_from_slice(&piece[full..]);
        Ok(())
    }

    fn finish(mut self, sha256: &str) -> Result<W, crypt::Failure> {
        if !self.held.is_empty() {
            let held = std::mem::take(&mut self.held);
            self.open.part(&held, &mut self.out)?;
        }
        self.out.flush().map_err(|_| crypt::Failure::Disk)?;
        self.open.finish(sha256)?;
        Ok(self.out)
    }
}

impl Download<'_> {
    fn run(&self, name: &str) -> Result<PathBuf, String> {
        let key = crypt::file_key(&self.inc.key).ok_or("corrupt")?;
        let dir = downloads().ok_or("disk")?;
        crypt::sweep_partials(&dir, STALE_PARTIAL);
        let partial = crypt::partial_in(&dir);
        let result = (|| {
            let file = crypt::create_partial(&partial).map_err(|e| e.status().to_string())?;
            let mut opener = Opener {
                open: crypt::StreamOpen::new(&key, self.inc.size)
                    .map_err(|e| e.status().to_string())?,
                held: Vec::new(),
                out: std::io::BufWriter::with_capacity(1 << 20, file),
                got: 0,
            };
            if self.inc.stream > 0 {
                self.parts(&mut opener)?;
            } else {
                self.whole(&mut opener)?;
            }
            if opener.got != self.inc.bytes {
                return Err("corrupt".into());
            }
            let file = opener
                .finish(&self.inc.sha256)
                .map_err(|e| e.status().to_string())?;
            drop(file);
            crypt::place(&partial, name, &dir).map_err(|e| e.status().to_string())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&partial);
        }
        result
    }

    fn stopped(&self) -> bool {
        self.inc.landed.get().1
    }

    fn shown(&self, got: u64) {
        let pct = (got * 100 / self.inc.bytes.max(1)).min(100) as u32;
        if pct != self.shown.swap(pct, Ordering::Relaxed) {
            let id = self.id.to_string();
            bg::post(move |app| {
                progress(&id, pct);
                refresh_cards(app);
            });
        }
    }

    /// In one, once it's all uploaded (never more than the length that was offered, whatever the relay sends).
    fn whole<W: std::io::Write>(&self, opener: &mut Opener<W>) -> Result<(), String> {
        let mut bad: Option<String> = None;
        let status = {
            let mut sink = |piece: &[u8]| {
                if self.stopped() {
                    bad = Some("cancelled".into());
                    return false;
                }
                if opener.got + piece.len() as u64 > self.inc.bytes {
                    bad = Some("corrupt".into());
                    return false;
                }
                if let Err(e) = opener.feed(piece) {
                    bad = Some(e.status().into());
                    return false;
                }
                self.shown(opener.got);
                true
            };
            ask_into(&format!("/file/{}", self.id), &mut sink)
        };
        if let Some(b) = bad {
            return Err(b);
        }
        if status != "ok" {
            return Err(status);
        }
        Ok(())
    }

    /// Part by part as they land, a few read side by side and opened in order.
    fn parts<W: std::io::Write>(&self, opener: &mut Opener<W>) -> Result<(), String> {
        let size = self.inc.stream;
        let count = self.inc.bytes.div_ceil(size).max(1);
        struct Shelf {
            next: u64,
            opened: u64,
            got: BTreeMap<u64, Result<Vec<u8>, String>>,
            stop: bool,
        }
        let shelf = Mutex::new(Shelf {
            next: 1,
            opened: 0,
            got: BTreeMap::new(),
            stop: false,
        });
        let cv = Condvar::new();
        let length = |n: u64| size.min(self.inc.bytes - (n - 1) * size);
        let fetch = |n: u64| -> Result<Vec<u8>, String> {
            let len = length(n) as usize;
            let mut tries = 0;
            let asked = std::time::Instant::now();
            loop {
                // nothing from the sender for this long: they quit or went offline mid-way (Save asks again)
                if asked.elapsed() > STALLED {
                    return Err("sender-offline".into());
                }
                if self.stopped() || shelf.lock().map_or(true, |s| s.stop) {
                    return Err("cancelled".into());
                }
                let seen = self.inc.landed.get().0;
                let mut buf = Vec::with_capacity(len);
                let mut over = false;
                let status = {
                    let mut sink = |piece: &[u8]| {
                        if buf.len() + piece.len() > len {
                            over = true;
                            return false;
                        }
                        buf.extend_from_slice(piece);
                        true
                    };
                    ask_into(&format!("/file/{}/part/{n}", self.id), &mut sink)
                };
                match status.as_str() {
                    "ok" if !over && buf.len() == len => return Ok(buf),
                    "ok" => return Err("corrupt".into()),
                    // not in yet: its `part` event (or a second and a half) says to look again
                    "not-ready" => {
                        let _ = self.inc.landed.wait_for(seen + 1, 1500);
                    }
                    // the relay out of reach for a moment: looked for again, as long as it hasn't stalled (the
                    // inbox opening again wakes it at once)
                    s if RETRYABLE.contains(&s) => {
                        tries = (tries + 1).min(4);
                        let _ = self.inc.landed.wait_for(seen + 1, 500 << tries);
                    }
                    _ => return Err(status),
                }
            }
        };
        let mut failure = None;
        std::thread::scope(|scope| {
            for _ in 0..FETCHES {
                scope.spawn(|| {
                    loop {
                        let n = {
                            let Ok(mut s) = shelf.lock() else { return };
                            // never more than FETCHES parts ahead of the one being opened
                            while !s.stop && s.next <= count && s.next > s.opened + FETCHES as u64 {
                                s = match cv.wait(s) {
                                    Ok(s) => s,
                                    Err(_) => return,
                                };
                            }
                            if s.stop || s.next > count {
                                return;
                            }
                            s.next += 1;
                            s.next - 1
                        };
                        let r = fetch(n);
                        let failed = r.is_err();
                        if let Ok(mut s) = shelf.lock() {
                            s.got.insert(n, r);
                        }
                        cv.notify_all();
                        if failed {
                            return;
                        }
                    }
                });
            }
            for n in 1..=count {
                let part = {
                    let Ok(mut s) = shelf.lock() else {
                        failure = Some("disk".to_string());
                        break;
                    };
                    loop {
                        if let Some(p) = s.got.remove(&n) {
                            break p;
                        }
                        s = match cv.wait(s) {
                            Ok(s) => s,
                            Err(_) => return,
                        };
                    }
                };
                let opened =
                    part.and_then(|data| opener.feed(&data).map_err(|e| e.status().to_string()));
                if let Err(e) = opened {
                    failure = Some(e);
                    break;
                }
                self.shown(opener.got);
                if let Ok(mut s) = shelf.lock() {
                    s.opened = n;
                }
                cv.notify_all();
            }
            if let Ok(mut s) = shelf.lock() {
                s.stop = true;
            }
            cv.notify_all();
        });
        match failure {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

/// A GET whose 200 goes to `sink`.
fn ask_into(path: &str, sink: &mut dyn FnMut(&[u8]) -> bool) -> String {
    call("GET", path, &[], crate::relay::Body::Bytes(&[]), Some(sink)).0
}

/// The recipient turns it down, or stops it on its way (Cancel): the download stops and the sender hears.
fn decline(id: &str) {
    if let Some(inc) = with(|s| s.incoming.remove(id)) {
        inc.landed.end();
    }
    set_state(id, "refused", None, None);
    forget_offer(id);
}

/// A file's event from the inbox: accepted (the sender's), part and ready, cancelled (the recipient's), declined.
fn file_event(app: &mut App, e: &Value) {
    let Some(id) = e["id"]
        .as_str()
        .filter(|i| i.len() == 32 && i.bytes().all(|b| b.is_ascii_hexdigit()))
    else {
        return;
    };
    match e["event"].as_str() {
        Some("accepted") => {
            if let Some(grant) = e["grant"].as_str() {
                let stream = e["stream"].as_u64().unwrap_or(0);
                if with(|s| s.outgoing.contains_key(id)) {
                    upload(id, grant, stream);
                } else {
                    // accepted before the offer's own answer came back (the recipient saves on their own): kept for
                    // it a little while; a file this app doesn't know at all is let go then
                    with(|s| {
                        s.early.insert(
                            id.to_string(),
                            (grant.to_string(), stream, std::time::Instant::now()),
                        )
                    });
                    forget_early_later();
                }
            }
        }
        Some("part") => {
            if let Some(inc) = with(|s| s.incoming.get(id).cloned()) {
                inc.landed.bump();
            }
        }
        Some("ready") => {
            if let Some(inc) = with(|s| s.incoming.get(id).cloned()) {
                inc.landed.bump();
            }
            if with(|s| {
                s.transfers
                    .get(id)
                    .is_some_and(|t| matches!(t.state.as_str(), "waiting" | "accepting"))
            }) {
                download(id);
            }
        }
        Some("cancelled") => {
            if let Some(inc) = with(|s| {
                s.held.retain(|h| h.2.as_deref() != Some(id));
                s.incoming.remove(id)
            }) {
                inc.landed.end();
            }
            if with(|s| s.transfers.get(id).is_some_and(|t| t.state != "saved")) {
                set_state(id, "cancelled", None, None);
            }
        }
        Some("declined") => {
            drop_outgoing(id);
            set_state(id, "declined", None, None);
        }
        _ => {}
    }
    refresh_cards(app);
}

/// Early accepts nobody claimed in 20 s are for files this app doesn't know (it restarted): the relay lets them go.
fn forget_early_later() {
    crate::later::free("share-early", 20_000, || {
        let old: Vec<String> = with(|s| {
            let old: Vec<String> = s
                .early
                .iter()
                .filter(|(_, (_, _, at))| at.elapsed() >= std::time::Duration::from_secs(19))
                .map(|(id, _)| id.clone())
                .collect();
            for id in &old {
                s.early.remove(id);
            }
            old
        });
        for id in old {
            forget_offer(&id);
        }
        if with(|s| !s.early.is_empty()) {
            forget_early_later();
        }
    });
}

// ---- cards ----

/// A card carries a share (messaging.rs `share_card`).
pub fn remember_card(card: &str, kind: Kind) {
    with(|s| {
        s.cards.insert(card.to_string(), kind);
    });
}

/// The buttons a card has beyond ×, rightmost first.
pub fn acts(card: &str) -> Vec<(Act, String)> {
    let Some(kind) = with(|s| s.cards.get(card).cloned()) else {
        return Vec::new();
    };
    match kind {
        Kind::Clip(_) => vec![(Act::Copy, "Copy".into())],
        Kind::KeyChanged(_) => vec![(Act::Trust, "Trust New Key".into())],
        Kind::File(id) => file_acts(&id),
    }
}

/// A file's buttons, in a card or a thread line, rightmost first (the Mac's FileRow): the sender's Retry and Take
/// Back; the recipient's Save (an accepted name's), Cancel while it's on its way, and Show in Folder once it's in.
pub fn file_acts(id: &str) -> Vec<(Act, String)> {
    with(|s| file_acts_in(s, id))
}

fn file_acts_in(s: &Share, id: &str) -> Vec<(Act, String)> {
    let Some(t) = s.transfers.get(id) else {
        return Vec::new();
    };
    let (mine, st, who, saved) = (t.mine, t.state.as_str(), t.who.as_str(), t.saved.is_some());
    if mine {
        let mut out = Vec::new();
        let retry = st == "failed" && s.outgoing.contains_key(id);
        if retry || st == "offered" || st == "uploading" {
            out.push((Act::TakeBack, "Take Back".into()));
        }
        if retry {
            out.push((Act::Retry, "Retry".into()));
        }
        return out;
    }
    match st {
        "received" | "failed" if crate::messaging::arrival_of(who) == Arrival::Show => {
            vec![(Act::Save, "Save".into())]
        }
        "accepting" | "waiting" | "downloading" => vec![(Act::Cancel, "Cancel".into())],
        "saved" if saved => vec![(Act::Reveal, "Show in Folder".into())],
        _ => Vec::new(),
    }
}

/// A file line's words and progress in a thread: where it's got to; "no longer available" for one this run never
/// heard of (a key and progress live only in memory).
pub fn line_state(id: &str, mine: bool, who: &str) -> (String, Option<u32>) {
    let _ = mine;
    with(|s| match s.transfers.get(id) {
        Some(t) => {
            let words = t
                .error
                .clone()
                .unwrap_or_else(|| core::file_state_text(&t.state, &t.who, t.percent));
            let pct = matches!(t.state.as_str(), "uploading" | "downloading")
                .then_some(t.percent.unwrap_or(0));
            (words, pct)
        }
        None => (core::file_state_text("lost", who, None), None),
    })
}

/// A file's button was pressed (a card's, a thread line's).
pub fn file_act(app: &mut App, id: &str, act: Act) {
    match act {
        Act::Retry => {
            if can_retry(id) {
                upload(id, "", 0);
            }
        }
        Act::Save => save(app, id),
        Act::TakeBack => take_back(id),
        Act::Cancel => {
            if with(|s| {
                s.transfers.get(id).is_some_and(|t| {
                    !t.mine && matches!(t.state.as_str(), "accepting" | "waiting" | "downloading")
                })
            }) {
                decline(id);
            }
        }
        Act::Reveal => {
            if let Some(p) = with(|s| s.transfers.get(id).and_then(|t| t.saved.clone())) {
                reveal(&p);
            }
        }
        _ => {}
    }
    refresh_cards(app);
}

/// Explorer opens on the saved file's folder with it selected (nothing is opened or run).
fn reveal(path: &Path) {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::HSTRING;
    let args = HSTRING::from(format!("/select,\"{}\"", path.display()));
    // SAFETY: live wide strings for the call's length
    unsafe {
        ShellExecuteW(
            None,
            &HSTRING::from("open"),
            &HSTRING::from("explorer.exe"),
            &args,
            windows::core::PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

/// Trust New Key (a card's, the thread's): their new key is pinned, the safety code said, and what they sent while
/// it was held opens now.
pub fn trust(app: &mut App, who: &str) {
    if !keys::trust(who) {
        return;
    }
    let who = who.to_string();
    let cards: Vec<String> = with(|s| {
        s.cards
            .iter()
            .filter(|(_, k)| **k == Kind::KeyChanged(who.clone()))
            .map(|(c, _)| c.clone())
            .collect()
    });
    app.model.cards.list.retain(|c| !cards.contains(&c.card.id));
    let code = keys::safety_code(&who);
    say_to(app, &who, core::safety_code_text(&who, &code));
    let held: Vec<Value> = with(|s| {
        let (theirs, rest) = std::mem::take(&mut s.held_for_key)
            .into_iter()
            .partition(|(w, _)| *w == who);
        s.held_for_key = rest;
        theirs.into_iter().map(|(_, e)| e).collect()
    });
    for e in held {
        heard(app, e);
    }
    refresh_cards(app);
}

/// Ctrl+V in `name`'s thread with files copied: they go to them. False leaves Ctrl+V to paste text.
pub fn paste_files_to(app: &mut App, name: &str) -> bool {
    let files = clipboard_files(app.bar.hwnd);
    if files.is_empty() {
        return false;
    }
    send_files(app, files, name);
    true
}

/// The e2e suite's view of the transfers and the cards.
pub fn debug_state() -> Value {
    with(|s| {
        let transfers: Vec<Value> = s
            .transfers
            .iter()
            .map(|(id, t)| {
                json!({ "id": id, "mine": t.mine, "who": t.who, "name": t.name, "size": t.size,
                    "state": t.state, "percent": t.percent, "error": t.error,
                    "retry": s.outgoing.contains_key(id) && t.mine && t.state == "failed",
                    "saved": t.saved.as_ref().map(|p| p.display().to_string()),
                    "stream": s.incoming.get(id).map_or(0, |i| i.stream),
                    "acts": file_acts_in(s, id).into_iter().map(|(_, l)| l).collect::<Vec<_>>(),
                    "startedMs": t.started_ms, "doneMs": t.done_ms })
            })
            .collect();
        let cards: BTreeMap<&String, String> =
            s.cards.iter().map(|(c, k)| (c, format!("{k:?}"))).collect();
        json!({ "transfers": transfers, "cards": cards, "held": s.held.len(),
            "heldForKey": s.held_for_key.len(), "pending": s.pending.len() })
    })
}

/// A card stays while its file is on its way, or until its Save is answered.
pub fn holds(card: &str) -> bool {
    let Some(Kind::File(id)) = with(|s| s.cards.get(card).cloned()) else {
        return with(|s| matches!(s.cards.get(card), Some(Kind::KeyChanged(_))));
    };
    with(|s| {
        s.transfers.get(&id).is_some_and(|t| {
            !matches!(
                t.state.as_str(),
                "saved" | "sent" | "declined" | "cancelled" | "refused" | "lost" | "withdrawn"
            )
        })
    })
}

/// A card's button was pressed.
pub fn act(app: &mut App, card: &str, act: Act) {
    let Some(kind) = with(|s| s.cards.get(card).cloned()) else {
        return;
    };
    match (act, kind) {
        (Act::Copy, Kind::Clip(c)) => {
            app.model.to_clipboard = Some(c);
            say(app, "copied".into());
        }
        (
            act @ (Act::Save | Act::Retry | Act::TakeBack | Act::Cancel | Act::Reveal),
            Kind::File(id),
        ) => file_act(app, &id, act),
        (Act::Trust, Kind::KeyChanged(who)) => trust(app, &who),
        _ => {}
    }
    refresh_cards(app);
}

// ---- drops, pastes and the picker ----

/// Files dropped on the bar (WM_DROPFILES) or pasted (Ctrl+V after Explorer's Copy): to whoever the line names,
/// or waiting for a name. True when taken.
pub fn drop_files(app: &mut App, paths: Vec<PathBuf>) -> bool {
    if paths.is_empty() {
        return false;
    }
    let files: Vec<PathBuf> = paths.iter().filter(|p| !p.is_dir()).cloned().collect();
    if files.is_empty() {
        say(app, core::file_error_text("folder", ""));
        return true;
    }
    let (me, key) = crate::messaging::credentials();
    let m = core::Messaging {
        name: me,
        key,
        ..core::Messaging::default()
    };
    let line = app.model.field.text().to_string();
    let target = core::drop_target(&line, &m, true, None, files.len() as u32);
    if !target.name.is_empty() {
        crate::commands::clear_line(app, false);
        if !target.text.is_empty() {
            crate::messaging::send_line(app, target.name.clone(), target.text.clone());
        }
        send_files(app, files, &target.name);
        return true;
    }
    if !target.hint.is_empty() {
        say(app, target.hint);
        return true;
    }
    let too_many = core::drop_count_text(files.len() as u32);
    if !too_many.is_empty() {
        say(app, too_many);
        return true;
    }
    // no name yet: the line asks for one, and ↵ sends them
    with(|s| s.pending = files);
    app.model.hint = None;
    app.model.field.set_text("send file to ");
    app.text_changed();
    true
}

/// Ctrl+V with files copied: sent as a drop would be, on a line that names someone (or files waiting for one);
/// false leaves Ctrl+V to paste text.
pub fn paste_files(app: &mut App) -> bool {
    let files = clipboard_files(app.bar.hwnd);
    if files.is_empty() {
        return false;
    }
    let line = app.model.field.text();
    let names = core::parse_share_command(line).is_some()
        || (core::is_message_command(line) && core::parse_message(line).is_some());
    names && drop_files(app, files)
}

/// The files on the clipboard (CF_HDROP), if any.
fn clipboard_files(owner: windows::Win32::Foundation::HWND) -> Vec<PathBuf> {
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows::Win32::UI::Shell::HDROP;
    const CF_HDROP: u32 = 15;
    // SAFETY: the clipboard is opened, read and closed here; the HDROP is the clipboard's, read while it's open
    unsafe {
        if IsClipboardFormatAvailable(CF_HDROP).is_err() || OpenClipboard(Some(owner)).is_err() {
            return Vec::new();
        }
        let out = GetClipboardData(CF_HDROP)
            .map(|h| drop_paths(HDROP(h.0)))
            .unwrap_or_default();
        let _ = CloseClipboard();
        out
    }
}

/// The paths in a drop handle.
pub fn drop_paths(h: windows::Win32::UI::Shell::HDROP) -> Vec<PathBuf> {
    use windows::Win32::UI::Shell::DragQueryFileW;
    // SAFETY: a live HDROP; buffers sized as DragQueryFileW says
    unsafe {
        let n = DragQueryFileW(h, u32::MAX, None);
        (0..n)
            .filter_map(|i| {
                let len = DragQueryFileW(h, i, None) as usize;
                let mut buf = vec![0u16; len + 1];
                let got = DragQueryFileW(h, i, Some(&mut buf)) as usize;
                (got > 0).then(|| PathBuf::from(String::from_utf16_lossy(&buf[..got])))
            })
            .collect()
    }
}

/// WM_DROPFILES on the bar (app.rs).
pub fn dropped(app: &mut App, h: windows::Win32::UI::Shell::HDROP) {
    let paths = drop_paths(h);
    // SAFETY: the drop's handle, finished with
    unsafe { windows::Win32::UI::Shell::DragFinish(h) };
    drop_files(app, paths);
}

/// The open dialog, owned by the bar; None when cancelled. Runs outside the app's borrow (it pumps messages).
fn pick_file(owner: usize) -> Option<PathBuf> {
    use windows::Win32::UI::Controls::Dialogs::{
        GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST,
        OPENFILENAMEW,
    };
    let mut buf = vec![0u16; 32 * 1024];
    let title: Vec<u16> =
        "Pick a file to send (up to 1 GB) · it’s encrypted before it leaves this pc\0"
            .encode_utf16()
            .collect();
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: windows::Win32::Foundation::HWND(owner as *mut _),
        lpstrFile: windows::core::PWSTR(buf.as_mut_ptr()),
        nMaxFile: buf.len() as u32,
        lpstrTitle: windows::core::PCWSTR(title.as_ptr()),
        Flags: OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
        ..Default::default()
    };
    // SAFETY: a live struct whose buffers outlive the modal call
    if !unsafe { GetOpenFileNameW(&mut ofn) }.as_bool() {
        return None;
    }
    let len = buf.iter().position(|c| *c == 0).unwrap_or(0);
    (len > 0).then(|| PathBuf::from(String::from_utf16_lossy(&buf[..len])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_files_card_reads_its_transfer() {
        with(|s| {
            s.transfers.insert(
                "a".repeat(32),
                Transfer {
                    percent: Some(40),
                    ..Transfer::new(false, "max", "notes.pdf", 1_200_000, "downloading")
                },
            );
        });
        assert_eq!(
            file_text(&"a".repeat(32)),
            "sent you notes.pdf (1.2 MB) · downloading 40%"
        );
        remember_card("c1", Kind::File("a".repeat(32)));
        assert!(holds("c1"));
        // on its way, it can be stopped
        assert_eq!(acts("c1"), vec![(Act::Cancel, "Cancel".to_string())]);
        set_state(&"a".repeat(32), "received", None, None);
        // max isn't an accepted name here: no Save until they're allowed (a request's file waits)
        assert!(acts("c1").is_empty());
        set_state(&"a".repeat(32), "failed", None, None);
        assert!(acts("c1").is_empty());
        set_state(
            &"a".repeat(32),
            "failed",
            None,
            Some("that file expired · ask for it again".into()),
        );
        assert_eq!(
            line_state(&"a".repeat(32), false, "max").0,
            "that file expired · ask for it again"
        );
        assert_eq!(
            line_state(&"b".repeat(32), false, "max").0,
            "no longer available"
        );
        set_state(&"a".repeat(32), "saved", None, None);
        assert!(!holds("c1"));
        assert!(acts("c1").is_empty());
        with(|s| s.transfers.get_mut(&"a".repeat(32)).unwrap().saved = Some("x".into()));
        assert_eq!(
            acts("c1"),
            vec![(Act::Reveal, "Show in Folder".to_string())]
        );
        // the sender's: Take Back while it's offered or going, nothing once it's sent
        with(|s| {
            s.transfers.insert(
                "c".repeat(32),
                Transfer::new(true, "max", "a.bin", 5, "offered"),
            )
        });
        assert_eq!(
            file_acts(&"c".repeat(32)),
            vec![(Act::TakeBack, "Take Back".to_string())]
        );
        set_state(&"c".repeat(32), "sent", None, None);
        assert!(file_acts(&"c".repeat(32)).is_empty());
    }

    #[test]
    fn a_download_opens_whatever_pieces_it_comes_in() {
        let dir = std::env::temp_dir().join(format!("qcalc-share-open-{}", crypt::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let key = crypt::new_file_key();
        let plain: Vec<u8> = (0..crypt::CHUNK * 2 + 99)
            .map(|i| (i % 241) as u8)
            .collect();
        let (src, sealed) = (dir.join("p"), dir.join("s"));
        std::fs::write(&src, &plain).unwrap();
        let sha = crypt::encrypt_file(&src, &sealed, &key, |_| {}).unwrap();
        let bytes = std::fs::read(&sealed).unwrap();
        for cut in [1usize, 7, 65_536, crypt::CHUNK + crypt::TAG, 3_000_000] {
            let mut o = Opener {
                open: crypt::StreamOpen::new(&key, plain.len() as u64).unwrap(),
                held: Vec::new(),
                out: Vec::new(),
                got: 0,
            };
            for piece in bytes.chunks(cut) {
                o.feed(piece).unwrap();
            }
            assert_eq!(o.got, bytes.len() as u64);
            assert_eq!(o.finish(&sha).unwrap(), plain);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_signal_wakes_its_waiters() {
        let s = Arc::new(Signal::default());
        assert_eq!(s.wait_for(1, 10), None);
        let t = {
            let s = s.clone();
            std::thread::spawn(move || s.wait_for(3, 5_000))
        };
        s.set(3);
        assert_eq!(t.join().unwrap(), Some(true));
        s.bump();
        assert_eq!(s.get(), (4, false));
        s.end();
        assert_eq!(s.wait_for(10, 5_000), Some(false));
    }
}
