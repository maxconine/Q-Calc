//! The message threads under the bar (the Mac's MessageThreads.swift and MessageHistory.swift; the words are the
//! core's `app::messaging`), and the one-time privacy notice (ShareViews.swift `PrivacyNoticeLine`).
//!
//! - **History.** Every message sent (once delivered) and every one shown from an accepted name, and each clip and
//!   file both ways, per contact in `messages.json` beside settings.json (the Mac's shape), at most 200 a contact.
//!   Read off the UI thread the first time it's needed (changes made before then wait for it) and written off it.
//! - **The panel.** `messages` lists the conversations, latest first (↑↓ ↵, or a click); `messages <name>`,
//!   `messages with <name>`, `open messages with <name>` open one. A thread shows its newest lines (wheel, PgUp /
//!   PgDn for older), each clip with Copy and each file with Save / Retry as its state allows, a reply line with 📎,
//!   the safety code behind ⓘ, and Trust New Key while their key has changed. A share that couldn't be opened leaves
//!   a line saying so. The reply takes the keys when the thread opens; an unsent reply is kept (for this run) when
//!   it closes and is back, caret after it, when it opens again. A new line from them lands here, not as a card, and
//!   a screen reader hears it. Esc or × closes it.
//! - **Drags.** While files are dragged over the thread, the reply line's status says where they'll go or why they
//!   can't (share_drop.rs).
//!
//! Nothing here costs anything until messaging is used: no file is read, no layout made.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use qcalc_core::app::messaging as core;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_LEADING, IDWriteTextFormat,
    IDWriteTextLayout,
};

use crate::app::App;
use crate::input::Field;
use crate::keys::Mods;
use crate::messaging::Act;
use crate::model::BarModel;
use crate::render::{Canvas, Gfx, Rect, caret_x, hit, measure, rgb, size};
use crate::{bg, qlog};

const FILE: &str = "messages.json";
pub const CAP: usize = 200;
const MAX_VIEW: f32 = 260.0;
const FIELD_H: f32 = 24.0;
const BTN_H: f32 = 22.0;
const ROW_H: f32 = size::TAPE_ROW;
const LIST_ROWS: usize = size::TAPE_VISIBLE;

// ---- history ----

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadFile {
    pub id: String,
    pub name: String,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub id: String,
    pub at: f64,
    /// "sent" or "received"
    pub dir: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<ThreadFile>,
}

impl Line {
    pub fn mine(&self) -> bool {
        self.dir == "sent"
    }
}

#[derive(Default, Serialize, Deserialize)]
struct HistoryFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    threads: BTreeMap<String, Vec<Line>>,
}

type Threads = BTreeMap<String, Vec<Line>>;
type Change = Box<dyn FnOnce(&mut Threads)>;

/// Only clean names, lines that are sent or received with clean words, the newest `CAP` each, oldest first.
pub fn sanitized(raw: Threads) -> Threads {
    let mut out = Threads::new();
    for (who, list) in raw {
        let name = core::clean_name(&Value::String(who));
        if name.is_empty() {
            continue;
        }
        let mut kept: Vec<Line> = list
            .into_iter()
            .filter(|l| (l.dir == "sent" || l.dir == "received") && !l.text.is_empty())
            .map(|l| Line {
                text: core::clean_message_text(&Value::String(l.text)),
                clip: l
                    .clip
                    .filter(|c| !c.is_empty() && c.len() <= core::CLIP_MAX),
                file: l.file.map(|f| ThreadFile {
                    name: core::clean_filename(&f.name),
                    ..f
                }),
                ..l
            })
            .collect();
        kept.sort_by(|a, b| a.at.total_cmp(&b.at));
        if kept.len() > CAP {
            kept.drain(..kept.len() - CAP);
        }
        if !kept.is_empty() {
            out.entry(name).or_default().extend(kept);
        }
    }
    out
}

/// Each conversation's latest line, the latest conversation first.
pub fn conversations(t: &Threads) -> Vec<(String, Line)> {
    let mut v: Vec<(String, Line)> = t
        .iter()
        .filter_map(|(n, l)| l.last().map(|last| (n.clone(), last.clone())))
        .collect();
    v.sort_by(|a, b| b.1.at.total_cmp(&a.1.at).then_with(|| a.0.cmp(&b.0)));
    v
}

/// A line says who and when when the sender changes, or 5 minutes have passed.
pub fn headed(lines: &[Line], i: usize) -> bool {
    i == 0 || lines[i - 1].dir != lines[i].dir || lines[i].at - lines[i - 1].at > 5.0 * 60_000.0
}

// ---- time words (ThreadTime) ----

const DAY_MS: i64 = 86_400_000;

fn civil(days: i64) -> (i64, u32, u32) {
    // Howard Hinnant's days → (y, m, d)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn clock(local_ms: i64) -> String {
    let in_day = local_ms.rem_euclid(DAY_MS) / 60_000;
    let (h, m) = (in_day / 60, in_day % 60);
    let h12 = if h % 12 == 0 { 12 } else { h % 12 };
    format!("{h12}:{m:02} {}", if h < 12 { "am" } else { "pm" })
}

/// The list's label: the time today, `yesterday`, the weekday this week, else the date (local ms both).
pub fn time_label(at: i64, now: i64) -> String {
    const WEEK: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let (day, today) = (at.div_euclid(DAY_MS), now.div_euclid(DAY_MS));
    if day == today {
        return clock(at);
    }
    if day == today - 1 {
        return "yesterday".into();
    }
    if day >= today - 6 && day < today {
        return WEEK[(day + 4).rem_euclid(7) as usize].into();
    }
    let (_, m, d) = civil(day);
    format!("{} {d}", MONTHS[(m - 1) as usize])
}

/// A thread line's: the time, with the day when it isn't today.
pub fn time_full(at: i64, now: i64) -> String {
    if at.div_euclid(DAY_MS) == now.div_euclid(DAY_MS) {
        clock(at)
    } else {
        format!("{} {}", time_label(at, now), clock(at))
    }
}

/// ms since 1970 → this zone's wall-clock ms (for the day and hour only).
fn local(ms: f64) -> i64 {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::Storage::FileSystem::FileTimeToLocalFileTime;
    let ticks = ((ms as i64 + 11_644_473_600_000) * 10_000) as u64;
    let ft = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    let mut out = FILETIME::default();
    // SAFETY: two live FILETIMEs
    if unsafe { FileTimeToLocalFileTime(&ft, &mut out) }.is_err() {
        return ms as i64;
    }
    let t = (u64::from(out.dwHighDateTime) << 32) | u64::from(out.dwLowDateTime);
    (t / 10_000) as i64 - 11_644_473_600_000
}

fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_millis() as f64)
}

// ---- state ----

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Panel {
    List(usize),
    Thread(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    Idle,
    Sending,
    Sent,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
enum Hit {
    Close,
    Info,
    Trust,
    Attach,
    Field,
    Copy(String),
    File(String, Act),
    Row(usize),
    Notice,
}

struct State {
    loaded: bool,
    loading: bool,
    threads: Threads,
    pending: Vec<Change>,
    save_after_load: bool,
    panel: Option<Panel>,
    draft: Field,
    drafts: HashMap<String, String>,
    reply: Reply,
    /// the reply line has the keys
    focus: bool,
    info: bool,
    /// how far up from the newest line the thread is scrolled
    scroll: f32,
    drop_hint: Option<String>,
    hover: Option<Hit>,
    /// the line whose Copy says "Copied"
    copied: Option<String>,
    notice: Option<bool>,
    bold: Option<IDWriteTextFormat>,
    /// the open thread's lines, laid out (line_block), by what they show
    blocks: HashMap<String, Block>,
}

impl Default for State {
    fn default() -> State {
        State {
            loaded: false,
            loading: false,
            threads: Threads::new(),
            pending: Vec::new(),
            save_after_load: false,
            panel: None,
            draft: reply_field(""),
            drafts: HashMap::new(),
            reply: Reply::Idle,
            focus: false,
            info: false,
            scroll: 0.0,
            drop_hint: None,
            hover: None,
            copied: None,
            notice: None,
            bold: None,
            blocks: HashMap::new(),
        }
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

fn reply_field(text: &str) -> Field {
    let mut f = Field::default();
    f.keep_words = true;
    f.reset(text);
    f
}

fn path() -> Option<std::path::PathBuf> {
    crate::store::config_dir().map(|d| d.join(FILE))
}

/// The history is read (once, off the UI thread) when something first needs it.
pub fn load() {
    let go = with(|s| {
        let go = !s.loaded && !s.loading;
        s.loading = true;
        go
    });
    if !go {
        return;
    }
    bg::spawn("qcalc-threads-load", || {
        let read = path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice::<HistoryFile>(&b).ok())
            .map(|f| f.threads)
            .unwrap_or_default();
        let threads = sanitized(read);
        bg::post(move |app| {
            let save = with(|s| {
                s.threads = threads;
                for change in std::mem::take(&mut s.pending) {
                    change(&mut s.threads);
                }
                s.loaded = true;
                std::mem::take(&mut s.save_after_load)
            });
            if save {
                save_now();
            }
            app.model.touch();
        });
    });
}

static WRITE: Mutex<Option<String>> = Mutex::new(None);

fn save_now() {
    let text = with(|s| {
        serde_json::to_string(&HistoryFile {
            version: 1,
            threads: s.threads.clone(),
        })
        .ok()
    });
    let Some(text) = text else { return };
    let first = WRITE.lock().is_ok_and(|mut w| w.replace(text).is_none());
    if first {
        bg::spawn("qcalc-threads-save", || {
            while let Some(text) = WRITE.lock().ok().and_then(|mut w| w.take()) {
                if let Some(p) = path() {
                    if let Some(dir) = p.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    if let Err(e) = crate::store::write_atomic(&p, &text) {
                        qlog!("threads: couldn't save: {e}");
                    }
                }
            }
        });
    }
}

fn change(f: impl FnOnce(&mut Threads) + 'static) {
    let now = with(|s| {
        if s.loaded {
            f(&mut s.threads);
            true
        } else {
            s.pending.push(Box::new(f));
            s.save_after_load = true;
            false
        }
    });
    if now {
        save_now();
    } else {
        load();
    }
}

fn add(who: &str, line: Line) {
    let name = core::clean_name(&Value::String(who.to_string()));
    if name.is_empty() || line.text.is_empty() {
        return;
    }
    change(move |t| {
        let list = t.entry(name).or_default();
        list.push(line);
        if list.len() > CAP {
            let extra = list.len() - CAP;
            list.drain(..extra);
        }
    });
}

fn new_line(mine: bool, text: &str) -> Line {
    Line {
        id: crate::share_crypt::new_id(),
        at: now_ms().round(),
        dir: if mine { "sent" } else { "received" }.into(),
        text: core::clean_message_text(&Value::String(text.to_string())),
        clip: None,
        file: None,
    }
}

/// A message to or from `who` (messaging.rs: delivered, or shown from an accepted name).
pub fn record_text(who: &str, mine: bool, text: &str) {
    add(who, new_line(mine, text));
}

/// A clip to or from `who` (share.rs).
pub fn record_clip(who: &str, mine: bool, clip: &str) {
    let first = core::clip_preview(clip);
    let mut line = new_line(
        mine,
        if first.is_empty() {
            "clipboard"
        } else {
            &first
        },
    );
    line.clip = Some(clip.to_string());
    add(who, line);
}

/// A file to or from `who` (share.rs): its id, name and plain size.
pub fn record_file(who: &str, mine: bool, id: &str, name: &str, size: u64) {
    let mut line = new_line(mine, name);
    line.file = Some(ThreadFile {
        id: id.into(),
        name: name.into(),
        size,
    });
    add(who, line);
}

/// Their thread goes (settings' Remove / Unblock).
pub fn forget(who: &str) {
    let who = who.to_string();
    change(move |t| {
        t.remove(&who);
    });
}

fn thread_of(who: &str) -> Vec<Line> {
    with(|s| s.threads.get(who).cloned().unwrap_or_default())
}

/// The names in the history, the latest conversation first (completion).
pub fn names() -> Vec<String> {
    with(|s| {
        conversations(&s.threads)
            .into_iter()
            .map(|(n, _)| n)
            .collect()
    })
}

// ---- the panel's life ----

pub fn panel() -> Option<Panel> {
    with(|s| s.panel.clone())
}

/// Whether `who`'s thread is the one open (their new lines land in it, not as cards).
pub fn is_open(who: &str) -> bool {
    with(|s| s.panel == Some(Panel::Thread(who.to_string())))
}

pub fn open_name() -> Option<String> {
    with(|s| match &s.panel {
        Some(Panel::Thread(n)) => Some(n.clone()),
        _ => None,
    })
}

fn keep_draft(s: &mut State) {
    if let Some(Panel::Thread(name)) = &s.panel {
        let d = s.draft.text().to_string();
        if d.trim().is_empty() {
            s.drafts.remove(name);
        } else {
            s.drafts.insert(name.clone(), d);
        }
    }
}

/// `messages`: the conversations.
pub fn open_list(app: &mut App) {
    load();
    with(|s| {
        keep_draft(s);
        s.panel = Some(Panel::List(0));
        s.focus = true;
        s.hover = None;
    });
    app.model.touch();
}

/// Opens `who`'s thread; the reply takes the keys, with the caret after a kept draft.
pub fn open(app: &mut App, who: &str) {
    let name = core::clean_name(&Value::String(who.to_string()));
    if name.is_empty() {
        return;
    }
    load();
    with(|s| {
        if s.panel != Some(Panel::Thread(name.clone())) {
            keep_draft(s);
            let kept = s.drafts.remove(&name).unwrap_or_default();
            s.draft = reply_field(&kept);
            s.reply = Reply::Idle;
            s.info = false;
            s.scroll = 0.0;
            s.blocks.clear();
        }
        s.panel = Some(Panel::Thread(name.clone()));
        s.focus = true;
        s.hover = None;
        s.drop_hint = None;
    });
    // their cards are in the thread now
    app.model
        .cards
        .list
        .retain(|c| c.card.from != name || c.card.ask);
    if let Some(id) = &app.model.cards.focus
        && !app.model.cards.list.iter().any(|c| &c.card.id == id)
    {
        app.model.cards.focus = None;
    }
    app.model.touch();
}

/// Closes the list or the thread (its unsent reply kept); false when neither was open.
pub fn close(m: &mut BarModel) -> bool {
    let was = with(|s| {
        if s.panel.is_none() {
            return false;
        }
        keep_draft(s);
        s.panel = None;
        s.blocks.clear();
        s.draft = reply_field("");
        s.reply = Reply::Idle;
        s.focus = false;
        s.drop_hint = None;
        s.hover = None;
        true
    });
    if was {
        m.touch();
    }
    was
}

/// Whether the reply line (or the list) has the keys.
pub fn has_keys() -> bool {
    with(|s| s.panel.is_some() && s.focus)
}

/// The bar hid: the reply gives the keys back (it gets them again on the next click or Tab).
pub fn will_hide() {
    with(|s| {
        s.hover = None;
        s.drop_hint = None;
        // the layouts go with the showing (made again, once, at the next)
        s.blocks.clear();
    });
}

/// Tab on an empty line with a thread open: the reply takes the keys.
pub fn tab_in(m: &mut BarModel) -> bool {
    let took = with(|s| {
        let took = s.panel.is_some() && !s.focus;
        s.focus |= took;
        took
    });
    if took {
        m.touch();
    }
    took
}

/// A line came for the open thread (their message, clip, file or a note): a reader hears it, as a card would be.
pub fn heard(who: &str, kind: &str, text: &str) {
    if is_open(who) {
        crate::a11y::announce(&core::card_announcement(who, kind, text));
    }
}

/// While files are dragged over the thread: where they'll go or why not (None when the drag leaves).
pub fn set_drop_hint(hint: Option<String>) -> bool {
    with(|s| {
        let changed = s.drop_hint != hint;
        s.drop_hint = hint;
        changed
    })
}

pub fn drop_hint() -> Option<String> {
    with(|s| s.drop_hint.clone())
}

/// What a drag over `who`'s thread says (the core's dropTarget, as `send file to <who>` would).
pub fn drop_hint_for(who: &str, count: u32) -> String {
    let (me, key) = crate::messaging::credentials();
    let m = core::Messaging {
        name: me,
        key,
        ..core::Messaging::default()
    };
    core::drop_target(&format!("send file to {who}"), &m, true, None, count.max(1)).hint
}

// ---- the privacy notice ----

const NOTICE_FILE: &str = "share-notice.json";

/// The first time a clip or file is sent or saved: the notice shows until it's dismissed, once for good.
pub fn first_share(app: &mut App) {
    let seen = with(|s| s.notice);
    if seen.is_some() {
        return;
    }
    // read once, off the UI thread
    with(|s| s.notice = Some(false));
    bg::spawn("qcalc-notice", || {
        let dismissed = crate::store::config_dir().is_some_and(|d| d.join(NOTICE_FILE).exists());
        bg::post(move |app| {
            if !dismissed {
                with(|s| s.notice = Some(true));
                crate::a11y::announce(&core::privacy_notice());
                app.model.touch();
            }
        });
    });
    app.model.touch();
}

fn dismiss_notice(app: &mut App) {
    with(|s| s.notice = Some(false));
    bg::spawn("qcalc-notice", || {
        if let Some(d) = crate::store::config_dir() {
            let _ = std::fs::create_dir_all(&d);
            let _ = std::fs::write(d.join(NOTICE_FILE), "{\"dismissed\":true}");
        }
    });
    app.model.touch();
}

fn notice_shown() -> bool {
    with(|s| s.notice == Some(true))
}

// ---- sending from the reply ----

/// `s` cut to at most `max` UTF-16 units (the relay's measure), never inside a character.
fn take16(s: &str, max: usize) -> String {
    let mut n = 0;
    s.chars()
        .take_while(|c| {
            n += c.len_utf16();
            n <= max
        })
        .collect()
}

fn send_reply(app: &mut App) {
    let Some(name) = open_name() else { return };
    if !crate::messaging::has_name() {
        // as `message` does with no name: straight to settings (the reply stays)
        with(keep_draft);
        crate::settings_window::open(app);
        return;
    }
    let said = with(|s| {
        if s.reply == Reply::Sending {
            return String::new();
        }
        take16(s.draft.text().trim(), core::TEXT_MAX)
    });
    if said.is_empty() {
        return;
    }
    with(|s| s.reply = Reply::Sending);
    app.model.touch();
    crate::messaging::send_thread(name, said);
}

/// The relay's word on a reply (messaging.rs).
pub fn reply_sent(app: &mut App, to: &str, delivered: bool, note: String) {
    if !is_open(to) {
        if delivered {
            with(|s| {
                s.drafts.remove(to);
            });
        }
        return;
    }
    with(|s| {
        if delivered {
            s.draft = reply_field("");
            s.reply = Reply::Sent;
            s.scroll = 0.0;
        } else {
            s.reply = Reply::Failed(note);
        }
    });
    if delivered {
        crate::later::after("thread-sent", 1200, |app| {
            with(|s| {
                if s.reply == Reply::Sent {
                    s.reply = Reply::Idle;
                }
            });
            app.model.touch();
        });
    }
    app.model.touch();
}

/// A share's word for the open thread's reply status (share.rs `say` with a thread open).
/// The reply line's note goes, if it still says `note` in `to`'s open thread (a step that's over, as encrypting).
pub fn unsay(to: &str, note: &str) {
    if is_open(to) {
        with(|s| {
            if matches!(&s.reply, Reply::Failed(n) if n == note) {
                s.reply = Reply::Idle;
            }
        });
    }
}

pub fn say(to: &str, note: &str) -> bool {
    if !is_open(to) {
        return false;
    }
    with(|s| s.reply = Reply::Failed(note.to_string()));
    true
}

// ---- layout ----

#[derive(Clone, Copy, PartialEq)]
enum Ink {
    Text,
    Muted,
    Answer,
    Accent,
}

#[derive(Clone)]
enum Item {
    Text(IDWriteTextLayout, f32, f32, Ink),
    Fill(Rect, f32, bool),
    Button(Rect, IDWriteTextLayout, Hit),
    Bar(Rect, u32),
}

/// A thread line laid out from its own top, kept while it doesn't change.
#[derive(Clone)]
struct Block {
    h: f32,
    items: Vec<Item>,
    hits: Vec<(Rect, Hit)>,
}

struct Placed {
    height: f32,
    /// panel coordinates; `view` items are drawn clipped to it
    items: Vec<Item>,
    view: Rect,
    view_items: Vec<Item>,
    hits: Vec<(Rect, Hit)>,
    field: Rect,
    /// the thread's content height and the view's (for the wheel)
    content: f32,
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

fn wrapped(
    gfx: &Gfx,
    text: &str,
    fmt: &IDWriteTextFormat,
    w: f32,
    max_lines: f32,
) -> Option<(IDWriteTextLayout, f32)> {
    let wide: Vec<u16> = text.encode_utf16().collect();
    // SAFETY: wide outlives the call; the format is live
    let l = unsafe { gfx.dw.CreateTextLayout(&wide, fmt, w.max(1.0), 100_000.0) }.ok()?;
    let (_, h) = measure(&l);
    Some((l, h.min(17.5 * max_lines)))
}

#[allow(clippy::too_many_arguments)]
fn button(
    gfx: &Gfx,
    label: &str,
    right: f32,
    y: f32,
    hit: Hit,
    out: &mut Vec<Item>,
    hits: &mut Vec<(Rect, Hit)>,
    view: bool,
) -> f32 {
    let Some(l) = gfx.layout(label, &gfx.fonts.small, 0.0, BTN_H) else {
        return right;
    };
    let w = measure(&l).0 + 16.0;
    let r = Rect::new(right - w, y, w, BTN_H);
    out.push(Item::Button(r, l, hit.clone()));
    if !view {
        hits.push((r, hit));
    }
    right - w - 8.0
}

fn place(gfx: &Gfx, width: f32) -> Placed {
    let mut p = Placed {
        height: 0.0,
        items: Vec::new(),
        view: Rect::default(),
        view_items: Vec::new(),
        hits: Vec::new(),
        field: Rect::default(),
        content: 0.0,
    };
    let inner = width - 2.0 * size::GUTTER;
    let g = size::GUTTER;
    let mut y = 0.0;
    if notice_shown() {
        y += 8.0;
        let text = core::privacy_notice().to_string();
        if let Some((l, h)) = wrapped(gfx, &text, &gfx.fonts.small, inner - 30.0, 3.0) {
            p.items.push(Item::Text(l, g, y, Ink::Muted));
            let r = Rect::new(width - g - 22.0, y - 3.0, 22.0, BTN_H);
            if let Some(x) = gfx.layout("×", &gfx.fonts.small, 0.0, BTN_H) {
                p.items.push(Item::Button(r, x, Hit::Notice));
                p.hits.push((r, Hit::Notice));
            }
            y += h.max(BTN_H - 6.0) + 8.0;
        }
    }
    let panel = with(|s| s.panel.clone());
    match panel {
        None => {}
        Some(Panel::List(sel)) => y = place_list(gfx, &mut p, width, y, sel),
        Some(Panel::Thread(name)) => y = place_thread(gfx, &mut p, width, y, &name),
    }
    p.height = y;
    p
}

fn header(gfx: &Gfx, p: &mut Placed, width: f32, y: f32, title: &str, note: &str, info: bool) {
    let g = size::GUTTER;
    let mut x = g;
    if let Some(b) = bold(gfx)
        && let Some(l) = gfx.layout(title, &b, 0.0, BTN_H)
    {
        x += measure(&l).0 + 8.0;
        p.items.push(Item::Text(l, g, y, Ink::Text));
    }
    if !note.is_empty()
        && let Some(l) = gfx.layout(note, &gfx.fonts.small, 0.0, BTN_H)
    {
        p.items.push(Item::Text(l, x, y, Ink::Muted));
    }
    let mut right = button(
        gfx,
        "×",
        width - g,
        y,
        Hit::Close,
        &mut p.items,
        &mut p.hits,
        false,
    );
    if info {
        let on = with(|s| s.info);
        right = button(
            gfx,
            if on { "ⓘ on" } else { "ⓘ" },
            right,
            y,
            Hit::Info,
            &mut p.items,
            &mut p.hits,
            false,
        );
    }
    let _ = right;
}

fn place_list(gfx: &Gfx, p: &mut Placed, width: f32, mut y: f32, sel: usize) -> f32 {
    let list = with(|s| conversations(&s.threads));
    let loaded = with(|s| s.loaded);
    y += 8.0;
    header(
        gfx,
        p,
        width,
        y,
        "messages",
        if list.is_empty() {
            ""
        } else {
            "↑↓ pick · ↵ open"
        },
        false,
    );
    y += BTN_H + 4.0;
    let g = size::GUTTER;
    if list.is_empty() {
        let words = if loaded {
            core::thread_empty_text("")
        } else {
            "…".into()
        };
        if let Some(l) = gfx.layout(&words, &gfx.fonts.body, 0.0, BTN_H) {
            p.items.push(Item::Text(l, g, y, Ink::Muted));
        }
        return y + BTN_H + 10.0;
    }
    let first = sel
        .saturating_sub(LIST_ROWS - 1)
        .min(list.len().saturating_sub(LIST_ROWS));
    let now = local(now_ms());
    for (i, (name, last)) in list.iter().enumerate().skip(first).take(LIST_ROWS) {
        let r = Rect::new(8.0, y, width - 16.0, ROW_H);
        if i == sel {
            p.items.push(Item::Fill(r, size::RADIUS_S, true));
        }
        p.hits.push((r, Hit::Row(i)));
        let mut x = 16.0;
        if let Some(b) = bold(gfx)
            && let Some(l) = gfx.layout(name, &b, 0.0, ROW_H)
        {
            x += measure(&l).0 + 12.0;
            p.items.push(Item::Text(l, 16.0, y, Ink::Text));
        }
        let when = time_label(local(last.at), now);
        let mut right = width - 16.0;
        if let Some(l) = gfx.layout(&when, &gfx.fonts.small, 0.0, ROW_H) {
            right -= measure(&l).0;
            p.items.push(Item::Text(l, right, y, Ink::Muted));
            right -= 12.0;
        }
        let said = format!("{}{}", if last.mine() { "you: " } else { "" }, last.text);
        if let Some(l) = gfx.layout_in(
            &said,
            &gfx.fonts.body,
            (right - x).max(1.0),
            ROW_H,
            DWRITE_TEXT_ALIGNMENT_LEADING,
        ) {
            p.items.push(Item::Text(l, x, y, Ink::Muted));
        }
        y += ROW_H;
    }
    y + 6.0
}

fn place_thread(gfx: &Gfx, p: &mut Placed, width: f32, mut y: f32, name: &str) -> f32 {
    let g = size::GUTTER;
    let inner = width - 2.0 * g;
    let lines = thread_of(name);
    let (info, draft_empty, reply, drop_hint, copied) = with(|s| {
        (
            s.info,
            s.draft.text().is_empty(),
            s.reply.clone(),
            s.drop_hint.clone(),
            s.copied.clone(),
        )
    });
    y += 8.0;
    let note = match lines.len() {
        0 => String::new(),
        1 => "1 message".into(),
        n => format!("{n} messages"),
    };
    header(gfx, p, width, y, name, &note, true);
    y += BTN_H + 4.0;
    // the key: changed, and the safety code
    let changed = crate::share_keys::changed(name);
    if changed {
        let right = button(
            gfx,
            "Trust New Key",
            width - g,
            y,
            Hit::Trust,
            &mut p.items,
            &mut p.hits,
            false,
        );
        if let Some(b) = bold(gfx)
            && let Some(l) = gfx.layout_in(
                &format!("⚠ {}", core::key_changed_text(name)),
                &b,
                right - g,
                BTN_H,
                DWRITE_TEXT_ALIGNMENT_LEADING,
            )
        {
            p.items.push(Item::Text(l, g, y, Ink::Text));
        }
        y += BTN_H + 2.0;
        if let Some((l, h)) = wrapped(
            gfx,
            &core::key_changed_detail(name),
            &gfx.fonts.small,
            inner,
            3.0,
        ) {
            p.items.push(Item::Text(l, g, y, Ink::Muted));
            y += h + 4.0;
        }
    }
    if info || changed {
        let code = crate::share_keys::known_safety_code(name);
        if let Some((l, h)) = wrapped(
            gfx,
            &core::safety_code_text(name, &code),
            &gfx.fonts.small,
            inner,
            2.0,
        ) {
            p.items.push(Item::Text(
                l,
                g,
                y,
                if code.is_empty() {
                    Ink::Muted
                } else {
                    Ink::Answer
                },
            ));
            y += h + 2.0;
        }
        if !code.is_empty()
            && let Some((l, h)) = wrapped(
                gfx,
                &core::safety_code_detail(name),
                &gfx.fonts.small,
                inner,
                3.0,
            )
        {
            p.items.push(Item::Text(l, g, y, Ink::Muted));
            y += h + 4.0;
        }
    }
    // the lines, in content coordinates, then fitted to the view from the bottom
    let mut c = 0.0;
    let mut items = Vec::new();
    let now = local(now_ms());
    if lines.is_empty() {
        if let Some(l) = gfx.layout(&core::thread_empty_text(name), &gfx.fonts.body, 0.0, BTN_H) {
            items.push(Item::Text(l, g, c, Ink::Muted));
        }
        c += BTN_H;
    }
    // each line's block, laid out once and kept while it doesn't change (a file's line follows its transfer, so
    // it's laid out each time); a frame, a hover or a click then costs a lookup per line, not its text layouts
    let mut blocks: Vec<(f32, Block)> = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        let head = headed(&lines, i);
        let key = line.file.is_none().then(|| {
            format!(
                "{}|{head}|{}|{}|{}",
                line.id,
                width.to_bits(),
                copied.as_deref() == Some(line.id.as_str()),
                if head {
                    time_full(local(line.at), now)
                } else {
                    String::new()
                }
            )
        });
        let kept = key
            .as_ref()
            .and_then(|k| with(|s| s.blocks.get(k).cloned()));
        let block = match kept {
            Some(b) => b,
            None => {
                let b = line_block(gfx, line, head, name, width, copied.as_deref(), now);
                if let Some(k) = key {
                    with(|s| {
                        if s.blocks.len() > 4 * CAP {
                            s.blocks.clear();
                        }
                        s.blocks.insert(k, b.clone());
                    });
                }
                b
            }
        };
        let h = block.h;
        blocks.push((c, block));
        c += h;
    }
    let view_h = c.min(MAX_VIEW);
    let scroll = with(|s| {
        s.scroll = s.scroll.clamp(0.0, (c - view_h).max(0.0));
        s.scroll
    });
    let shift = y - (c - view_h - scroll);
    p.view = Rect::new(0.0, y, width, view_h);
    let moved = |it: Item, dy: f32| match it {
        Item::Text(l, x, ty, ink) => Item::Text(l, x, ty + dy, ink),
        Item::Fill(r, rad, sel) => Item::Fill(Rect::new(r.x, r.y + dy, r.w, r.h), rad, sel),
        Item::Button(r, l, h) => Item::Button(Rect::new(r.x, r.y + dy, r.w, r.h), l, h),
        Item::Bar(r, pct) => Item::Bar(Rect::new(r.x, r.y + dy, r.w, r.h), pct),
    };
    for it in items {
        p.view_items.push(moved(it, shift));
    }
    // only the lines in view are drawn and can be clicked
    for (top, b) in blocks {
        let at = top + shift;
        if at + b.h <= y || at >= y + view_h {
            continue;
        }
        for it in b.items {
            p.view_items.push(moved(it, at));
        }
        for (r, h) in b.hits {
            let r = Rect::new(r.x, r.y + at, r.w, r.h);
            if r.bottom() > y && r.y < y + view_h {
                p.hits.push((r, h));
            }
        }
    }
    p.content = c;
    y += view_h + 8.0;
    // the reply line: 📎, the field, its status
    let clip_r = Rect::new(g, y, 22.0, FIELD_H);
    if let Some(l) = gfx.layout("📎", &gfx.fonts.small, 0.0, FIELD_H) {
        p.items.push(Item::Button(clip_r, l, Hit::Attach));
        p.hits.push((clip_r, Hit::Attach));
    }
    let words = drop_hint.or(match &reply {
        Reply::Idle => None,
        Reply::Sending => Some("sending…".into()),
        Reply::Sent => Some("sent".into()),
        Reply::Failed(n) => Some(n.clone()),
    });
    let fx = g + 22.0 + 8.0;
    let mut fw = width - g - fx;
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
        fw -= w + 8.0;
        p.items.push(Item::Text(
            l,
            fx + fw + 8.0,
            y,
            if reply == Reply::Sent {
                Ink::Accent
            } else {
                Ink::Muted
            },
        ));
    }
    p.field = Rect::new(fx, y, fw, FIELD_H);
    p.hits.push((p.field, Hit::Field));
    let _ = draft_empty;
    y + FIELD_H + 12.0
}

/// One line of a thread, laid out from its own top: who and when (if it's `headed`), then its words, clip or file.
fn line_block(
    gfx: &Gfx,
    line: &Line,
    head: bool,
    name: &str,
    width: f32,
    copied: Option<&str>,
    now: i64,
) -> Block {
    let g = size::GUTTER;
    let inner = width - 2.0 * g;
    let mut c = 0.0;
    let mut items = Vec::new();
    let mut hits: Vec<(Rect, Hit)> = Vec::new();
    if head {
        c += 6.0;
        let mut x = g;
        if let Some(b) = bold(gfx)
            && let Some(l) = gfx.layout(if line.mine() { "you" } else { name }, &b, 0.0, 18.0)
        {
            x += measure(&l).0 + 6.0;
            items.push(Item::Text(
                l,
                g,
                c,
                if line.mine() { Ink::Muted } else { Ink::Text },
            ));
        }
        if let Some(l) = gfx.layout(&time_full(local(line.at), now), &gfx.fonts.small, 0.0, 18.0) {
            items.push(Item::Text(l, x, c, Ink::Muted));
        }
        c += 18.0;
    }
    if let Some(clip) = &line.clip {
        let label = if copied == Some(line.id.as_str()) {
            "Copied"
        } else {
            "Copy"
        };
        let right = button(
            gfx,
            label,
            width - g,
            c + 2.0,
            Hit::Copy(line.id.clone()),
            &mut items,
            &mut hits,
            false,
        );
        let bw = right - g;
        if let Some((l, h)) = wrapped(gfx, clip, &gfx.fonts.small, bw - 16.0, 8.0) {
            let r = Rect::new(g, c, bw, h + 12.0);
            items.push(Item::Fill(r, size::RADIUS_S, false));
            items.push(Item::Text(l, g + 8.0, c + 6.0, Ink::Answer));
            c += r.h + 4.0;
        }
    } else if let Some(file) = &line.file {
        let (state, pct) = crate::share::line_state(&file.id, line.mine(), name);
        let mut right = width - g - 8.0;
        for (act, label) in crate::share::file_acts(&file.id).into_iter() {
            right = button(
                gfx,
                &label,
                right,
                c + 6.0,
                Hit::File(file.id.clone(), act),
                &mut items,
                &mut hits,
                false,
            );
        }
        let top = c;
        c += 6.0;
        let head = format!("📄 {}  {}", file.name, core::file_size_text(file.size));
        if let Some(l) = gfx.layout_in(
            &head,
            &gfx.fonts.body,
            (right - g - 8.0).max(1.0),
            BTN_H,
            DWRITE_TEXT_ALIGNMENT_LEADING,
        ) {
            items.push(Item::Text(l, g + 8.0, c, Ink::Answer));
        }
        c += BTN_H;
        if !state.is_empty()
            && let Some(l) = gfx.layout_in(
                &state,
                &gfx.fonts.small,
                inner - 16.0,
                18.0,
                DWRITE_TEXT_ALIGNMENT_LEADING,
            )
        {
            items.push(Item::Text(l, g + 8.0, c, Ink::Muted));
            c += 18.0;
        }
        if let Some(pct) = pct {
            items.push(Item::Bar(
                Rect::new(g + 8.0, c + 2.0, inner - 16.0, 3.0),
                pct,
            ));
            c += 8.0;
        }
        c += 6.0;
        items.insert(
            0,
            Item::Fill(Rect::new(g, top, inner, c - top), size::RADIUS_S, false),
        );
        c += 4.0;
    } else if let Some((l, h)) = wrapped(gfx, &line.text, &gfx.fonts.body, inner, 40.0) {
        items.push(Item::Text(l, g, c, Ink::Answer));
        c += h + 2.0;
    }
    Block { h: c, items, hits }
}

/// The panel's height under the cards (0 when there's nothing to show).
pub fn height(gfx: &Gfx, width: f32) -> f32 {
    if with(|s| s.panel.is_none() && s.notice != Some(true)) {
        return 0.0;
    }
    place(gfx, width).height + 1.0
}

pub fn paint(c: &Canvas, m: &BarModel, r: Rect) {
    let _ = m;
    if r.h <= 0.0 {
        return;
    }
    let t = c.theme;
    c.fill(r, t.bg_tape);
    c.fill(Rect::new(r.x, r.y, r.w, 1.0), t.line);
    let p = place(c.gfx, r.w);
    let y0 = r.y + 1.0;
    let hover = with(|s| s.hover.clone());
    let ink = |i: Ink| match i {
        Ink::Text => t.ink,
        Ink::Muted => t.muted,
        Ink::Answer => t.answer,
        Ink::Accent => t.accent,
    };
    let draw = |it: &Item| match it {
        Item::Text(l, x, y, i) => c.text(l, *x, y + y0, ink(*i)),
        Item::Fill(rr, rad, sel) => c.fill_round(
            Rect::new(rr.x, rr.y + y0, rr.w, rr.h),
            *rad,
            if *sel { t.fill_selected } else { t.fill_hover },
        ),
        Item::Button(rr, l, h) => {
            let rr = Rect::new(rr.x, rr.y + y0, rr.w, rr.h);
            let on = hover.as_ref() == Some(h);
            if on {
                c.fill_round(rr, size::RADIUS_S, t.fill_selected);
            }
            let (w, _) = measure(l);
            c.text(
                l,
                rr.x + (rr.w - w) / 2.0,
                rr.y,
                if on { t.answer } else { t.muted },
            );
        }
        Item::Bar(rr, pct) => {
            let rr = Rect::new(rr.x, rr.y + y0, rr.w, rr.h);
            c.fill_round(rr, 1.5, t.line);
            c.fill_round(
                Rect::new(rr.x, rr.y, rr.w * (*pct).min(100) as f32 / 100.0, rr.h),
                1.5,
                t.accent,
            );
        }
    };
    for it in &p.items {
        draw(it);
    }
    if p.view.h > 0.0 {
        c.push_clip(Rect::new(p.view.x, p.view.y + y0, p.view.w, p.view.h));
        for it in &p.view_items {
            draw(it);
        }
        c.pop_clip();
    }
    if p.field.w > 0.0 {
        let f = Rect::new(p.field.x, p.field.y + y0, p.field.w, p.field.h);
        paint_field(c, f, open_name().unwrap_or_default());
    }
}

fn paint_field(c: &Canvas, f: Rect, name: String) {
    let t = c.theme;
    let control = if t.dark {
        rgb(0x58585c, 1.0)
    } else {
        rgb(0xffffff, 1.0)
    };
    let focused = has_keys();
    if focused {
        c.fill_round(f, size::RADIUS_S, t.accent);
        c.fill_round(f.inset(2.0, 2.0), size::RADIUS_S - 2.0, control);
    } else {
        c.fill_round(f, size::RADIUS_S, rgb(0x000000, 0.12));
        c.fill_round(f.inset(0.5, 0.5), size::RADIUS_S - 0.5, control);
    }
    let inner = f.inset(8.0, 0.0);
    c.push_clip(inner);
    with(|s| {
        let draft = s.draft.text();
        if draft.is_empty() {
            let prompt = core::thread_reply_prompt(&name, crate::messaging::has_name());
            if let Some(l) = c.gfx.layout(&prompt, &c.gfx.fonts.small, 0.0, f.h) {
                c.text(&l, inner.x, inner.y, t.muted);
            }
        }
        if let Some(l) = c.gfx.layout(draft, &c.gfx.fonts.small, 0.0, f.h) {
            let caret = caret_x(&l, s.draft.to_utf16(s.draft.caret()));
            let x0 = inner.x - (caret - inner.w + 4.0).max(0.0);
            if focused && s.draft.has_selection() {
                let (a, b) = s.draft.selection();
                let (xa, xb) = (
                    caret_x(&l, s.draft.to_utf16(a)),
                    caret_x(&l, s.draft.to_utf16(b)),
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
        }
    });
    c.pop_clip();
}

// ---- input ----

fn under(app: &App, x: f32, y: f32) -> Option<Hit> {
    let r = app.bar.layout.threads?;
    if !r.contains(x, y) {
        return None;
    }
    let p = place(&app.gfx, r.w);
    let (px, py) = (x - r.x, y - r.y - 1.0);
    p.hits
        .iter()
        .rev()
        .find(|(hr, _)| hr.contains(px, py))
        .map(|(_, h)| h.clone())
}

/// The pointer over the bar.
pub fn hover(app: &mut App, at: Option<(f32, f32)>) {
    if with(|s| s.panel.is_none() && s.notice != Some(true)) {
        return;
    }
    let h = at
        .and_then(|(x, y)| under(app, x, y))
        .filter(|h| !matches!(h, Hit::Field | Hit::Row(_)));
    if with(|s| {
        let changed = s.hover != h;
        s.hover = h;
        changed
    }) {
        app.model.touch();
    }
}

/// A click in the bar: true when it was the panel's. A click elsewhere gives the keys back to the line.
pub fn click(app: &mut App, x: f32, y: f32) -> bool {
    let Some(r) = app.bar.layout.threads else {
        return false;
    };
    if !r.contains(x, y) {
        if with(|s| std::mem::replace(&mut s.focus, false)) {
            app.model.touch();
        }
        return false;
    }
    let picked = under(app, x, y);
    match picked {
        Some(Hit::Close) => {
            close(&mut app.model);
        }
        Some(Hit::Notice) => dismiss_notice(app),
        Some(Hit::Info) => {
            let on = with(|s| {
                s.info = !s.info;
                s.info
            });
            if on && let Some(name) = open_name() {
                crate::share_keys::learn(&name);
            }
        }
        Some(Hit::Trust) => {
            if let Some(name) = open_name() {
                crate::share::trust(app, &name);
            }
        }
        Some(Hit::Attach) => {
            if let Some(name) = open_name() {
                crate::share::pick_and_send(app, &name);
            }
        }
        Some(Hit::Copy(id)) => copy_line(app, &id),
        Some(Hit::File(id, act)) => crate::share::file_act(app, &id, act),
        Some(Hit::Row(i)) => {
            if let Some((name, _)) = with(|s| conversations(&s.threads)).get(i) {
                open(app, &name.clone());
            }
        }
        Some(Hit::Field) => {
            let p = place(&app.gfx, r.w);
            with(|s| {
                if let Some(l) = app
                    .gfx
                    .layout(s.draft.text(), &app.gfx.fonts.small, 0.0, FIELD_H)
                {
                    let pos = hit(&l, x - r.x - p.field.x - 8.0, 0.0);
                    s.draft.set_caret_utf16(pos, false);
                }
                s.focus = true;
            });
        }
        None => {
            with(|s| s.focus = s.panel.is_some());
        }
    }
    app.model.touch();
    true
}

fn copy_line(app: &mut App, id: &str) {
    let Some(name) = open_name() else { return };
    let clip = thread_of(&name)
        .into_iter()
        .find(|l| l.id == id)
        .and_then(|l| l.clip);
    if let Some(clip) = clip {
        app.model.to_clipboard = Some(clip);
        crate::a11y::announce("copied");
        with(|s| s.copied = Some(id.to_string()));
        crate::later::after("thread-copied", 1200, |app| {
            with(|s| s.copied = None);
            app.model.touch();
        });
    }
}

/// The wheel over the thread scrolls it: true when it was over the panel.
pub fn wheel(
    app: &mut App,
    wp: windows::Win32::Foundation::WPARAM,
    lp: windows::Win32::Foundation::LPARAM,
) -> bool {
    let Some(r) = app.bar.layout.threads else {
        return false;
    };
    let mut pt = windows::Win32::Foundation::POINT {
        x: i32::from((lp.0 & 0xffff) as u16 as i16),
        y: i32::from(((lp.0 >> 16) & 0xffff) as u16 as i16),
    };
    // SAFETY: our window, a live point
    let _ = unsafe { windows::Win32::Graphics::Gdi::ScreenToClient(app.bar.hwnd, &mut pt) };
    let s = app.bar.scale();
    let (x, y) = (pt.x as f32 / s, pt.y as f32 / s);
    let delta = i32::from(((wp.0 >> 16) & 0xffff) as u16 as i16);
    if !r.contains(x, y) || open_name().is_none() {
        return false;
    }
    scroll_by(delta as f32 / 120.0 * 3.0 * 17.5);
    app.model.touch();
    true
}

fn scroll_by(d: f32) {
    with(|s| s.scroll = (s.scroll + d).max(0.0));
}

/// A key while the panel has the keys: true when it was the panel's.
pub fn key(app: &mut App, vk: u32, mods: Mods) -> bool {
    let Some(panel) = with(|s| s.focus.then(|| s.panel.clone()).flatten()) else {
        return false;
    };
    const ENTER: u32 = 0x0D;
    const ESC: u32 = 0x1B;
    const UP: u32 = 0x26;
    const DOWN: u32 = 0x28;
    const PGUP: u32 = 0x21;
    const PGDN: u32 = 0x22;
    let plain = !mods.ctrl && !mods.alt;
    if vk == ESC && plain {
        close(&mut app.model);
        return true;
    }
    if let Panel::List(sel) = panel {
        let n = with(|s| conversations(&s.threads)).len();
        match vk {
            UP | DOWN if plain => {
                let next = if vk == UP {
                    sel.saturating_sub(1)
                } else {
                    (sel + 1).min(n.saturating_sub(1))
                };
                with(|s| s.panel = Some(Panel::List(next)));
                if let Some((name, last)) = with(|s| conversations(&s.threads)).get(next) {
                    crate::a11y::announce(&format!(
                        "{name}: {}{}",
                        if last.mine() { "you: " } else { "" },
                        last.text
                    ));
                }
                app.model.touch();
                true
            }
            ENTER if plain => {
                if let Some((name, _)) = with(|s| conversations(&s.threads)).get(sel) {
                    open(app, &name.clone());
                }
                true
            }
            _ => {
                // anything else goes back to the line
                with(|s| s.focus = false);
                app.model.touch();
                false
            }
        }
    } else {
        let Panel::Thread(name) = panel else {
            return false;
        };
        match vk {
            ENTER if plain => send_reply(app),
            PGUP | UP if plain => scroll_by(if vk == PGUP { 200.0 } else { 17.5 }),
            PGDN | DOWN if plain => scroll_by(if vk == PGDN { -200.0 } else { -17.5 }),
            0x56 if mods.ctrl && !mods.alt && crate::share::paste_files_to(app, &name) => {}
            _ => {
                let host = app.bar.hwnd;
                let mut to_clip = None;
                let took = with(|s| {
                    crate::messaging::edit_field(&mut s.draft, vk, mods, host, &mut to_clip)
                });
                if let Some(c) = to_clip {
                    app.model.to_clipboard = Some(c);
                }
                if !took {
                    return false;
                }
                with(|s| {
                    if matches!(s.reply, Reply::Failed(_)) {
                        s.reply = Reply::Idle;
                    }
                });
            }
        }
        app.model.touch();
        true
    }
}

/// WM_CHAR while the reply has the keys.
pub fn char16(app: &mut App, unit: u16) -> bool {
    let took = with(|s| {
        if !s.focus || !matches!(s.panel, Some(Panel::Thread(_))) {
            return false;
        }
        // Enter, Esc and Tab are keys, not text
        if unit < 0x20 {
            return true;
        }
        if s.draft.text().encode_utf16().count() < core::TEXT_MAX {
            s.draft.char16(unit);
        }
        if matches!(s.reply, Reply::Failed(_)) {
            s.reply = Reply::Idle;
        }
        true
    });
    if took {
        app.model.touch();
    }
    took
}

/// The thread's drop area in screen-free bar DIPs (share_drop.rs): whether (x, y) is over the open thread.
pub fn over_thread(app: &App, x: f32, y: f32) -> bool {
    open_name().is_some() && app.bar.layout.threads.is_some_and(|r| r.contains(x, y))
}

// ---- the e2e suite's view ----

/// How long the panel takes to lay out (µs, the median of `n`): what a frame, a hover and a click each pay.
/// `cold` lays every line out again each time, as before the lines were kept.
pub fn debug_place_us(gfx: &Gfx, width: f32, n: usize, cold: bool) -> f64 {
    let mut times: Vec<f64> = (0..n.max(1))
        .map(|_| {
            if cold {
                with(|s| s.blocks.clear());
            }
            let t = std::time::Instant::now();
            let p = place(gfx, width);
            std::hint::black_box(p.height);
            t.elapsed().as_secs_f64() * 1e6
        })
        .collect();
    times.sort_by(f64::total_cmp);
    times[times.len() / 2]
}

pub fn debug_state() -> Value {
    with(|s| {
        let threads: BTreeMap<&String, Vec<Value>> = s
            .threads
            .iter()
            .map(|(n, l)| {
                (
                    n,
                    l.iter()
                        .map(|l| json!({ "dir": l.dir, "text": l.text, "clip": l.clip, "file": l.file.as_ref().map(|f| json!({"id": f.id, "name": f.name, "size": f.size})) }))
                        .collect(),
                )
            })
            .collect();
        json!({
            "loaded": s.loaded,
            "panel": match &s.panel { Some(Panel::Thread(n)) => json!({"thread": n}), Some(Panel::List(i)) => json!({"list": i}), None => Value::Null },
            "draft": s.draft.text(), "caret": s.draft.to_utf16(s.draft.caret()), "focus": s.focus,
            "reply": format!("{:?}", s.reply), "dropHint": s.drop_hint, "notice": s.notice,
            "drafts": s.drafts.clone(), "threads": threads,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(dir: &str, at: f64, text: &str) -> Line {
        Line {
            id: format!("{at}"),
            at,
            dir: dir.into(),
            text: text.into(),
            clip: None,
            file: None,
        }
    }

    #[test]
    fn history_keeps_clean_names_and_the_newest_lines() {
        let mut raw = Threads::new();
        raw.insert(
            "Max".into(),
            vec![line("sent", 2.0, "b"), line("received", 1.0, "a")],
        );
        raw.insert("not a name!".into(), vec![line("sent", 1.0, "x")]);
        raw.insert(
            "ann".into(),
            (0..250)
                .map(|i| line("sent", f64::from(i), "hi\u{202e}"))
                .collect(),
        );
        let t = sanitized(raw);
        assert_eq!(t.len(), 2);
        assert_eq!(
            t["max"].iter().map(|l| l.text.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(t["ann"].len(), CAP);
        assert_eq!(t["ann"][0].at, 50.0);
        assert_eq!(t["ann"][0].text, "hi ");
        assert_eq!(conversations(&t)[0].0, "ann");
    }

    #[test]
    fn a_line_is_headed_when_the_sender_changes_or_time_passes() {
        let l = vec![
            line("sent", 0.0, "a"),
            line("sent", 60_000.0, "b"),
            line("received", 61_000.0, "c"),
            line("received", 400_000.0, "d"),
        ];
        assert!(headed(&l, 0));
        assert!(!headed(&l, 1));
        assert!(headed(&l, 2));
        assert!(headed(&l, 3));
    }

    #[test]
    fn times_read_as_the_mac_says_them() {
        // 2026-10-27 is a tuesday
        let now = 1_793_059_200_000 + 15 * 3_600_000; // 15:00
        assert_eq!(time_label(now - 3_600_000 * 2, now), "1:00 pm");
        assert_eq!(time_label(now - DAY_MS, now), "yesterday");
        assert_eq!(time_label(now - 3 * DAY_MS, now), "sat");
        assert_eq!(time_label(now - 30 * DAY_MS, now), "sep 27");
        assert_eq!(time_full(now - DAY_MS, now), "yesterday 3:00 pm");
        assert_eq!(clock(0), "12:00 am");
        assert_eq!(clock(12 * 3_600_000 + 5 * 60_000), "12:05 pm");
    }
}
