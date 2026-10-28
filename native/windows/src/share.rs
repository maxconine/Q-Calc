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
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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
}

#[derive(Clone, Debug)]
struct Incoming {
    key: String,
    sha256: String,
    size: u64,
    bytes: u64,
    expires: f64,
}

struct Outgoing {
    sealed: PathBuf,
    bytes: u64,
    cancelled: Arc<AtomicBool>,
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
}

thread_local! {
    static STATE: RefCell<Share> = RefCell::new(Share::default());
}

fn with<R>(f: impl FnOnce(&mut Share) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

const RETRYABLE: [&str; 3] = ["failed", "no-network", "network"];
const PART_TRIES: u32 = 3;
const PART_BYTES: u64 = 50 * 1024 * 1024;

fn temp() -> PathBuf {
    let d = std::env::temp_dir().join("QCalcShare");
    let _ = std::fs::create_dir_all(&d);
    d
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
    body: &[u8],
    sink: Option<&mut dyn FnMut(&[u8]) -> bool>,
) -> (String, Value) {
    let Some(a) = auth() else {
        return ("not-yours".into(), Value::Null);
    };
    let mut headers: Vec<(&str, &str)> = a.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    headers.extend_from_slice(extra);
    crate::relay::call(method, path, &headers, body, 120_000, sink)
}

fn forget_offer(id: &str) {
    let path = format!("/file/{id}");
    bg::spawn("qcalc-share", move || {
        let _ = call("DELETE", &path, &[], &[], None);
    });
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
    // the picker takes the keys (our own window, so the bar stays); it runs outside the app's borrow
    let owner = app.bar.hwnd.0 as usize;
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
    if with(|s| !s.pending.is_empty()) {
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
        say(
            app,
            core::share_status_text("empty-clipboard", to, "clip", ""),
        );
        return;
    }
    if text.len() > core::CLIP_MAX {
        let path = temp().join("clipboard.txt");
        if std::fs::write(&path, text.as_bytes()).is_err() {
            say(app, core::file_error_text("disk", to));
            return;
        }
        send_files(app, vec![path], to);
        return;
    }
    send_clip(app, text, to);
}

fn send_clip(app: &mut App, clip: String, to: &str) {
    let to = to.to_string();
    let (me, relay_key) = crate::messaging::credentials();
    if me.is_empty() {
        say(app, core::share_status_text("no-name", &to, "clip", ""));
        return;
    }
    say(app, format!("sharing with {to}…"));
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
            }
            say(app, core::share_status_text(&status, &to, "clip", ""));
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

/// Files to `to`: folders say to zip them, more than 20 send none, each other one is offered.
pub fn send_files(app: &mut App, paths: Vec<PathBuf>, to: &str) {
    let files: Vec<PathBuf> = paths.iter().filter(|p| !p.is_dir()).cloned().collect();
    if files.is_empty() {
        if !paths.is_empty() {
            say(app, core::file_error_text("folder", to));
        }
        return;
    }
    let too_many = core::drop_count_text(files.len() as u32);
    if !too_many.is_empty() {
        say(app, too_many);
        return;
    }
    for f in files {
        send_file(app, f, to);
    }
}

fn send_file(app: &mut App, given: PathBuf, to: &str) {
    let to = to.to_string();
    let (me, relay_key) = crate::messaging::credentials();
    if me.is_empty() {
        say(app, core::share_status_text("no-name", &to, "file", ""));
        return;
    }
    // a symlink sends what it points to
    let path = std::fs::canonicalize(&given).unwrap_or(given);
    let raw_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = core::clean_filename(&raw_name);
    say(app, format!("encrypting {name}…"));
    bg::spawn("qcalc-share", move || {
        let result = offer(&path, &name, &to, &me, &relay_key);
        bg::post(move |app| match result {
            Ok((id, sealed, bytes, size)) => {
                crate::messaging::note_sent(&to);
                with(|s| {
                    s.outgoing.insert(
                        id.clone(),
                        Outgoing {
                            sealed,
                            bytes,
                            cancelled: Arc::new(AtomicBool::new(false)),
                        },
                    );
                    s.transfers.insert(
                        id,
                        Transfer {
                            mine: true,
                            who: to.clone(),
                            name: name.clone(),
                            size,
                            state: "offered".into(),
                            percent: None,
                            error: None,
                        },
                    );
                });
                say(
                    app,
                    core::share_status_text("delivered", &to, "file", &name),
                );
            }
            Err(note) => say(app, note),
        });
    });
}

/// On a worker: encrypt, seal the facts, offer. (id, the sealed file, its length, the plain size) or the words.
fn offer(
    path: &Path,
    name: &str,
    to: &str,
    me: &str,
    relay_key: &str,
) -> Result<(String, PathBuf, u64, u64), String> {
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
    let sha = crypt::encrypt_file(path, &sealed, &key, |_| {}).map_err(|e| {
        let _ = std::fs::remove_file(&sealed);
        core::file_error_text(e.status(), to)
    })?;
    let bytes = crypt::encrypted_length(size);
    let id = crypt::new_id();
    let fail = |status: &str| {
        let _ = std::fs::remove_file(&sealed);
        core::share_status_text(status, to, "file", name)
    };
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
    Ok((id, sealed, bytes, size))
}

/// The recipient said yes: upload with the grant, part by part (Retry passes no grant: the relay says which
/// parts it has).
fn upload(id: &str, grant: &str, part_bytes: u64) {
    let Some((sealed, bytes, cancelled, who)) = with(|s| {
        let o = s.outgoing.get(id)?;
        let t = s.transfers.get(id).filter(|t| t.mine)?;
        Some((
            o.sealed.clone(),
            o.bytes,
            o.cancelled.clone(),
            t.who.clone(),
        ))
    }) else {
        // a file this app doesn't have any more: the relay lets it go
        forget_offer(id);
        return;
    };
    set_state(id, "uploading", Some(0), None);
    let (id, grant) = (id.to_string(), grant.to_string());
    bg::spawn("qcalc-share-up", move || {
        let status = (|| {
            let headers: Vec<(&str, &str)> = if grant.is_empty() {
                vec![]
            } else {
                vec![("X-QCalc-Grant", grant.as_str())]
            };
            let (status, o) = call("POST", &format!("/file/{id}/start"), &headers, &[], None);
            if status != "ok" {
                return status;
            }
            let parts = o["parts"].as_u64().unwrap_or(1);
            let done: HashSet<u64> = o["done"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_u64).collect())
                .unwrap_or_default();
            let part_bytes = o["partBytes"].as_u64().unwrap_or(part_bytes).max(1);
            let Ok(mut f) = std::fs::File::open(&sealed) else {
                return "unreadable".into();
            };
            for n in 1..=parts {
                if cancelled.load(Ordering::Relaxed) {
                    return "cancelled".into();
                }
                if done.contains(&n) {
                    continue;
                }
                let offset = (n - 1) * part_bytes;
                let length = part_bytes.min(bytes.saturating_sub(offset)) as usize;
                let mut data = vec![0u8; length];
                use std::io::{Read, Seek, SeekFrom};
                if f.seek(SeekFrom::Start(offset)).is_err() || f.read_exact(&mut data).is_err() {
                    return "unreadable".into();
                }
                let mut tries = 0;
                loop {
                    let (status, _) = call(
                        "PUT",
                        &format!("/file/{id}/part/{n}"),
                        &[("Content-Type", "application/octet-stream")],
                        &data,
                        None,
                    );
                    if status == "ok" {
                        break;
                    }
                    // a part cut short goes again, a few times, a little later each time
                    tries += 1;
                    if !RETRYABLE.contains(&status.as_str()) || tries >= PART_TRIES {
                        return status;
                    }
                    std::thread::sleep(std::time::Duration::from_secs(2 << tries));
                }
                let pct = ((offset + length as u64) * 100 / bytes.max(1)) as u32;
                let id2 = id.clone();
                bg::post(move |app| {
                    progress(&id2, pct);
                    app.model.touch();
                });
            }
            call("POST", &format!("/file/{id}/finish"), &[], &[], None).0
        })();
        bg::post(move |app| {
            if status == "ok" {
                set_state(&id, "sent", None, None);
                drop_outgoing(&id);
            } else if status != "cancelled" {
                failed(&id, &status, &who);
            }
            refresh_cards(app);
        });
    });
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
                if first {
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
            },
        );
        s.transfers.insert(
            share.id.clone(),
            Transfer {
                mine: false,
                who: from.to_string(),
                name: f.name.clone(),
                size: f.size,
                state: if accepted { "received" } else { "request" }.into(),
                percent: None,
                error: None,
            },
        );
    });
    show_or_ask(app, from, None, Some(share.id.clone()));
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
            let name = with(|s| s.transfers.get(id).map(|t| t.name.clone()));
            core::card_announcement(from, "file", &name.unwrap_or_default())
        }
        _ => core::card_announcement(from, "clip", ""),
    };
    crate::a11y::announce(&said);
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
}

fn progress(id: &str, pct: u32) {
    with(|s| {
        if let Some(t) = s.transfers.get_mut(id)
            && matches!(t.state.as_str(), "uploading" | "downloading")
        {
            t.percent = Some(pct);
        }
    });
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

/// Save: ask the relay for it (the sender uploads it), then download it once it's ready.
fn save(app: &mut App, id: &str) {
    let Some(t) = with(|s| s.transfers.get(id).cloned()) else {
        return;
    };
    if t.mine || !matches!(t.state.as_str(), "received" | "failed") {
        return;
    }
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
        let (status, _) = call("POST", &format!("/file/{id}/accept"), &[], &[], None);
        bg::post(move |app| {
            match status.as_str() {
                "ok" => with(|s| {
                    if let Some(t) = s.transfers.get_mut(&id)
                        && t.state == "accepting"
                    {
                        t.state = "waiting".into();
                    }
                }),
                // already accepted: it may be ready
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

fn download(id: &str) {
    let Some((t, inc)) =
        with(|s| Some((s.transfers.get(id)?.clone(), s.incoming.get(id)?.clone())))
    else {
        return;
    };
    set_state(id, "downloading", Some(0), None);
    let id = id.to_string();
    bg::spawn("qcalc-share-down", move || {
        let sealed = temp().join(crypt::new_id());
        let result = (|| {
            let mut file = std::fs::File::create(&sealed).map_err(|_| "disk".to_string())?;
            let mut got: u64 = 0;
            let mut shown = 0u32;
            let mut over = false;
            let status = {
                let mut sink = |piece: &[u8]| {
                    use std::io::Write;
                    got += piece.len() as u64;
                    // never more than the length that was offered, whatever the relay sends
                    if got > inc.bytes || file.write_all(piece).is_err() {
                        over = true;
                        return false;
                    }
                    let pct = (got * 100 / inc.bytes.max(1)) as u32;
                    if pct >= shown + 5 {
                        shown = pct;
                        let id = id.clone();
                        bg::post(move |app| {
                            progress(&id, pct);
                            refresh_cards(app);
                        });
                    }
                    true
                };
                call("GET", &format!("/file/{id}"), &[], &[], Some(&mut sink)).0
            };
            if over {
                return Err("corrupt".to_string());
            }
            if status != "ok" {
                return Err(status);
            }
            drop(file);
            let key = crypt::file_key(&inc.key).ok_or("corrupt")?;
            let dir = crypt::downloads().ok_or("disk")?;
            crypt::safe_save(&sealed, &key, &inc.sha256, inc.size, &t.name, &dir)
                .map_err(|e| e.status().to_string())
        })();
        let _ = std::fs::remove_file(&sealed);
        bg::post(move |app| {
            match result {
                Ok(path) => {
                    qlog!("share in: saved {}", path.display());
                    set_state(&id, "saved", None, None);
                    with(|s| s.incoming.remove(&id));
                }
                // not uploaded yet: `ready` will come
                Err(s) if s == "not-ready" => set_state(&id, "waiting", None, None),
                Err(s) => set_state(&id, "failed", None, Some(core::file_error_text(&s, &t.who))),
            }
            refresh_cards(app);
        });
    });
}

/// The recipient turns it down.
fn decline(id: &str) {
    with(|s| s.incoming.remove(id));
    set_state(id, "refused", None, None);
    forget_offer(id);
}

/// A file's event from the inbox: accepted (the sender's), ready, cancelled (the recipient's), declined.
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
                upload(id, grant, e["partBytes"].as_u64().unwrap_or(PART_BYTES));
            }
        }
        Some("ready") => {
            if with(|s| {
                s.transfers
                    .get(id)
                    .is_some_and(|t| matches!(t.state.as_str(), "waiting" | "accepting"))
            }) {
                download(id);
            }
        }
        Some("cancelled") => {
            with(|s| {
                s.incoming.remove(id);
                s.held.retain(|h| h.2.as_deref() != Some(id));
            });
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
        Kind::File(id) => {
            if can_retry(&id) {
                return vec![(Act::Retry, "Retry".into())];
            }
            let state = with(|s| s.transfers.get(&id).map(|t| (t.mine, t.state.clone())));
            match state {
                Some((false, st)) if st == "received" || st == "failed" => {
                    vec![(Act::Save, "Save".into())]
                }
                _ => Vec::new(),
            }
        }
    }
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
                "saved" | "sent" | "declined" | "cancelled" | "refused" | "lost"
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
        (Act::Save, Kind::File(id)) => save(app, &id),
        (Act::Retry, Kind::File(id)) => upload(&id, "", PART_BYTES),
        (Act::Trust, Kind::KeyChanged(who)) => {
            if keys::trust(&who) {
                app.model.cards.list.retain(|c| c.card.id != card);
                let code = keys::safety_code(&who);
                say(app, core::safety_code_text(&who, &code));
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
            }
        }
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
                    mine: false,
                    who: "max".into(),
                    name: "notes.pdf".into(),
                    size: 1_200_000,
                    state: "downloading".into(),
                    percent: Some(40),
                    error: None,
                },
            );
        });
        assert_eq!(
            file_text(&"a".repeat(32)),
            "sent you notes.pdf (1.2 MB) · downloading 40%"
        );
        remember_card("c1", Kind::File("a".repeat(32)));
        assert!(holds("c1"));
        set_state(&"a".repeat(32), "received", None, None);
        assert_eq!(acts("c1"), vec![(Act::Save, "Save".to_string())]);
        set_state(&"a".repeat(32), "saved", None, None);
        assert!(!holds("c1"));
        assert!(acts("c1").is_empty());
    }
}
