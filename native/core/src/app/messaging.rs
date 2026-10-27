//! Messages between Q Calc users (`src/lib/messaging.ts`), without the network: names, the accepted and blocked
//! lists, the bar's `message <name> <text>` line, its hints and send statuses, and cleaning text from the relay.
//! The relay transport stays in the shells (`Messages.swift` on the Mac). Native only: the clip and file
//! commands, their words, and the checks on what arrives (`MessageFiles.swift` does the sending and saving).

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::app::js as jstr;
use crate::jsre::{self, Lazy, js};

pub const TEXT_MAX: usize = 500;
pub const INBOX_PROTOCOL: &str = "qcalc-inbox";
/// How many recent recipients are kept.
pub const RECENT_MAX: usize = 12;

/// The user's name, its key, and who they accept and block.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Messaging {
    pub name: String,
    pub key: String,
    pub accepted: Vec<String>,
    pub blocked: Vec<String>,
    /// Who you've sent to, the latest first, for `message` to finish their names.
    #[serde(default)]
    pub recent: Vec<String>,
}

/// A name as typed, lowercased; "" when it can't be one.
pub fn clean_name(raw: &Json) -> String {
    static NAME_RULE: Lazy = Lazy::new(|| js(r"^[a-z0-9][a-z0-9_-]{1,19}$", ""));
    let Some(s) = raw.as_str() else {
        return String::new();
    };
    let name = jstr::trim(s).to_lowercase();
    if jsre::test(&NAME_RULE, &name) {
        name
    } else {
        String::new()
    }
}

fn names(raw: Option<&Json>) -> Vec<String> {
    let mut out: Vec<String> = raw
        .and_then(Json::as_array)
        .map(|a| a.iter().map(clean_name).filter(|n| !n.is_empty()).collect())
        .unwrap_or_default();
    out.sort();
    out.dedup();
    out
}

/// Stored messaging read back: a 64-hex key, clean sorted lists, and nobody both accepted and blocked.
pub fn sanitize_messaging(raw: &Json) -> Messaging {
    static KEY: Lazy = Lazy::new(|| js(r"^[0-9a-f]{64}$", ""));
    let key = raw
        .get("key")
        .and_then(Json::as_str)
        .filter(|k| jsre::test(&KEY, k))
        .unwrap_or("")
        .to_string();
    let blocked = names(raw.get("blocked"));
    let accepted = names(raw.get("accepted"))
        .into_iter()
        .filter(|n| !blocked.contains(n))
        .collect();
    let mut recent: Vec<String> = Vec::new();
    for n in raw
        .get("recent")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .map(clean_name)
    {
        if !n.is_empty() && !recent.contains(&n) {
            recent.push(n);
        }
    }
    recent.truncate(RECENT_MAX);
    Messaging {
        name: raw.get("name").map(clean_name).unwrap_or_default(),
        key,
        accepted,
        blocked,
        recent,
    }
}

/// A name sent to goes to the front of the recent ones; a name that can't be one changes nothing.
pub fn with_recent(m: &Messaging, to: &str) -> Messaging {
    let name = clean_name(&Json::String(to.to_string()));
    if name.is_empty() {
        return m.clone();
    }
    let mut recent = vec![name.clone()];
    recent.extend(m.recent.iter().filter(|n| **n != name).cloned());
    recent.truncate(RECENT_MAX);
    Messaging {
        recent,
        ..m.clone()
    }
}

/// The names `message` finishes: who you've sent to, the latest first, then the rest of the accepted ones.
pub fn message_contacts(m: &Messaging) -> Vec<String> {
    let mut out = m.recent.clone();
    out.extend(m.accepted.iter().filter(|n| !m.recent.contains(n)).cloned());
    out
}

/// How a message from someone arrives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Arrival {
    Show,
    Ask,
    Drop,
}

/// An accepted name's message shows; a blocked name's goes unseen; anyone else's is a request.
pub fn arrival(m: &Messaging, from: &str) -> Arrival {
    if m.blocked.iter().any(|n| n == from) {
        Arrival::Drop
    } else if m.accepted.iter().any(|n| n == from) {
        Arrival::Show
    } else {
        Arrival::Ask
    }
}

fn with_lists(m: &Messaging, who: &str, accept: bool, block: bool) -> Messaging {
    let keep = |list: &[String], add: bool| {
        let mut out: Vec<String> = list.iter().filter(|n| *n != who).cloned().collect();
        if add {
            out.push(who.to_string());
            out.sort();
        }
        out
    };
    Messaging {
        accepted: keep(&m.accepted, accept),
        blocked: keep(&m.blocked, block),
        ..m.clone()
    }
}

pub fn allow_sender(m: &Messaging, who: &str) -> Messaging {
    with_lists(m, who, true, false)
}

pub fn block_sender(m: &Messaging, who: &str) -> Messaging {
    with_lists(m, who, false, true)
}

pub fn forget_sender(m: &Messaging, who: &str) -> Messaging {
    with_lists(m, who, false, false)
}

/// The bar's `message <name> <text>`.
pub fn is_message_command(text: &str) -> bool {
    static RE: Lazy = Lazy::new(|| js(r"^\s*message(?:\s|$)", "i"));
    jsre::test(&RE, text)
}

/// `to` as typed, `name` cleaned ("" when it can't be a name), the rest as the message.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageLine {
    pub to: String,
    pub name: String,
    pub text: String,
}

pub fn parse_message(text: &str) -> Option<MessageLine> {
    static RE: Lazy = Lazy::new(|| js(r"^\s*message(?:\s+(\S+))?(?:\s+([\s\S]*))?$", "i"));
    let c = RE.captures(text).ok()??;
    let to = jsre::group(&c, 1).to_string();
    Some(MessageLine {
        name: clean_name(&Json::String(to.clone())),
        text: jstr::take16(jstr::trim(jsre::group(&c, 2)), TEXT_MAX),
        to,
    })
}

/// Where the name sits in `message <name> …` (UTF-16 offsets), for the bar to paint it.
pub fn message_name_span(text: &str) -> Option<(usize, usize)> {
    static RE: Lazy = Lazy::new(|| js(r"^(\s*message\s+)(\S+)", "i"));
    let c = RE.captures(text).ok()??;
    let start = jstr::len16(jsre::group(&c, 1));
    Some((start, start + jstr::len16(jsre::group(&c, 2))))
}

/// What the bar says while a message is typed. `relay` is whether an online relay is configured; `found` is the
/// relay's word on the name: `Some(false)` when nobody holds it, `None` while it's being asked or can't be
/// (offline), so a name is never called missing without the relay.
pub fn message_hint(line: &MessageLine, m: &Messaging, relay: bool, found: Option<bool>) -> String {
    if !relay {
        return "messages need the online relay".into();
    }
    if m.name.is_empty() || m.key.is_empty() {
        return "set your name in settings to send messages · ↵ opens settings".into();
    }
    if line.to.is_empty() {
        return "message <name> <what to say>".into();
    }
    if line.name.is_empty() {
        return "a name is 2–20 letters, digits, - or _".into();
    }
    if found == Some(false) {
        return "username not found".into();
    }
    if line.text.is_empty() {
        return format!("then what to say to {}", line.name);
    }
    format!("↵ sends to {}", line.name)
}

/// What the relay said about a send.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SendStatus {
    Delivered,
    Offline,
    Unknown,
    NotYours,
    TooMany,
    Bad,
    Network,
}

/// What the bar says once a message has gone, or hasn't. The sender isn't told whether they're accepted, so
/// blocking someone is never given away.
pub fn send_status_text(status: SendStatus, to: &str) -> String {
    match status {
        SendStatus::Delivered => format!("sent to {to}"),
        SendStatus::Offline => format!("{to} isn’t on q calc right now · not sent"),
        SendStatus::Unknown => format!("nobody’s called {to} · not sent"),
        SendStatus::NotYours => "your name’s been taken · pick another in settings".into(),
        SendStatus::TooMany => "that’s a lot of messages · try again in a minute".into(),
        SendStatus::Bad => "that message couldn’t be sent".into(),
        SendStatus::Network => "couldn’t reach the relay · not sent".into(),
    }
}

/// Controls and direction overrides become spaces, whatever the relay did, so a line can't fake another sender's.
pub fn clean_message_text(raw: &Json) -> String {
    let Some(s) = raw.as_str() else {
        return String::new();
    };
    let bad = |c: char| matches!(c, '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}' | '\u{200e}' | '\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}');
    let mut out = String::with_capacity(s.len());
    let mut in_run = false;
    for c in s.chars() {
        if bad(c) {
            if !in_run {
                out.push(' ');
            }
            in_run = true;
        } else {
            out.push(c);
            in_run = false;
        }
    }
    jstr::take16(&out, TEXT_MAX)
}

/// An inbox frame from the relay.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum InboxEvent {
    Message { from: String, text: String },
    State { state: String },
}

/// `!{…}` frames: the inbox is ready, the name was taken, or a message.
pub fn parse_inbox_frame(data: &str) -> Option<InboxEvent> {
    let body = data.strip_prefix('!')?;
    let o: Json = serde_json::from_str(body).ok()?;
    match o.get("type").and_then(Json::as_str) {
        Some("inbox") => Some(InboxEvent::State {
            state: "ready".into(),
        }),
        Some("closed")
            if matches!(
                o.get("reason").and_then(Json::as_str),
                Some("taken" | "released")
            ) =>
        {
            Some(InboxEvent::State {
                state: "taken".into(),
            })
        }
        Some("message") => {
            let from = o.get("from").map(clean_name).unwrap_or_default();
            let text = o.get("text").map(clean_message_text).unwrap_or_default();
            (!from.is_empty() && !text.is_empty()).then_some(InboxEvent::Message { from, text })
        }
        _ => None,
    }
}

/// A conversation command (native only): `messages` lists who you've talked with; `messages <name>`,
/// `messages with <name>` and `open messages with <name>` open that thread.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ThreadCommand {
    List,
    /// `to` as typed ("" while it's still to come), `name` cleaned ("" when it can't be a name).
    Open {
        to: String,
        name: String,
    },
}

/// The bar's conversation commands; anything after the name means it's not one.
pub fn parse_messages_command(text: &str) -> Option<ThreadCommand> {
    static LIST: Lazy = Lazy::new(|| js(r"^\s*(?:open\s+)?messages\s*$", "i"));
    static OPEN: Lazy = Lazy::new(|| {
        js(
            r"^\s*(?:open\s+)?messages\s+(?:with(?:\s+|$))?(\S*)\s*$",
            "i",
        )
    });
    if jsre::test(&LIST, text) {
        return Some(ThreadCommand::List);
    }
    let c = OPEN.captures(text).ok()??;
    let to = jsre::group(&c, 1).to_string();
    Some(ThreadCommand::Open {
        name: clean_name(&Json::String(to.clone())),
        to,
    })
}

/// What the bar says while a conversation command is typed.
pub fn messages_hint(cmd: &ThreadCommand) -> String {
    match cmd {
        ThreadCommand::List => "↵ your conversations".into(),
        ThreadCommand::Open { to, .. } if to.is_empty() => "messages with <name>".into(),
        ThreadCommand::Open { name, .. } if name.is_empty() => {
            "a name is 2–20 letters, digits, - or _".into()
        }
        ThreadCommand::Open { name, .. } => format!("↵ opens messages with {name}"),
    }
}

/// The names the conversation commands finish: who's in the history (latest first), then who you've sent to
/// lately, then the rest of the accepted ones, each once.
pub fn thread_contacts(history: &[String], m: &Messaging) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for n in history
        .iter()
        .map(|n| clean_name(&Json::String(n.clone())))
        .chain(message_contacts(m))
    {
        if !n.is_empty() && !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

/// Tab after `messages`, `messages with` or `open messages with`: the rest of a name from `contacts`, else the
/// rest of `with` after a bare `messages`.
pub fn messages_completion(text: &str, contacts: &[String]) -> String {
    static TO: Lazy = Lazy::new(|| js(r"^\s*(open\s+)?messages\s+(with\s+)?([a-z0-9_-]+)$", "i"));
    let Some(c) = TO.captures(text).ok().flatten() else {
        return String::new();
    };
    let typed = jsre::group(&c, 3);
    let lower = typed.to_lowercase();
    if let Some(n) = contacts
        .iter()
        .find(|n| jstr::len16(n) > jstr::len16(typed) && n.starts_with(&lower))
    {
        return jstr::skip16(n, jstr::len16(typed));
    }
    if jsre::group(&c, 2).is_empty() && "with".starts_with(&lower) && lower != "with" {
        return format!("{} ", jstr::skip16("with", jstr::len16(typed)));
    }
    String::new()
}

/// What an empty thread says: nobody's name, or none of their messages yet.
pub fn thread_empty_text(name: &str) -> String {
    if name.is_empty() {
        "no messages yet · message <name> <text> starts one".into()
    } else {
        format!("no messages with {name} yet")
    }
}

// ---- clips and files (native only; relay/src/files.js has the relay's side) ----

/// The most clipboard text a clip carries, in UTF-8 bytes; past it the clipboard goes as a text file.
pub const CLIP_MAX: usize = 64 * 1024;
/// The biggest file that can be sent (its plain bytes).
pub const FILE_MAX: u64 = 1024 * 1024 * 1024;
/// Files are encrypted in chunks of this many plain bytes, each with its own 16-byte GCM tag.
pub const CHUNK_BYTES: u64 = 1024 * 1024;
pub const TAG_BYTES: u64 = 16;
const FILE_NAME_MAX: usize = 120;

/// `send file to <name>`, `paste to <name>` and `send clipboard to <name>`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShareCommand {
    /// "file" or "clip"
    pub what: String,
    /// as typed ("" while it's still to come)
    pub to: String,
    /// cleaned ("" when it can't be a name)
    pub name: String,
}

pub fn parse_share_command(text: &str) -> Option<ShareCommand> {
    static FILE: Lazy = Lazy::new(|| js(r"^\s*send\s+(?:a\s+)?file\s+to(?:\s+(\S*))?\s*$", "i"));
    static CLIP: Lazy = Lazy::new(|| {
        js(
            r"^\s*(?:paste|send\s+(?:my\s+)?clipboard|share\s+(?:my\s+)?clipboard)\s+to(?:\s+(\S*))?\s*$",
            "i",
        )
    });
    let (what, c) = if let Some(c) = FILE.captures(text).ok().flatten() {
        ("file", c)
    } else {
        ("clip", CLIP.captures(text).ok().flatten()?)
    };
    let to = jsre::group(&c, 1).to_string();
    Some(ShareCommand {
        what: what.into(),
        name: clean_name(&Json::String(to.clone())),
        to,
    })
}

/// What the bar says while a share is typed. `file` is a file already dropped on the bar, waiting for a name.
pub fn share_hint(
    cmd: &ShareCommand,
    m: &Messaging,
    relay: bool,
    found: Option<bool>,
    file: &str,
) -> String {
    if !relay {
        return "messages need the online relay".into();
    }
    if m.name.is_empty() || m.key.is_empty() {
        return "set your name in settings to send messages · ↵ opens settings".into();
    }
    let clip = cmd.what == "clip";
    if cmd.to.is_empty() {
        return if clip {
            "paste to <name> · shares what you’ve copied".into()
        } else if !file.is_empty() {
            format!("send {file} to <name>")
        } else {
            "send file to <name>".into()
        };
    }
    if cmd.name.is_empty() {
        return "a name is 2–20 letters, digits, - or _".into();
    }
    if found == Some(false) {
        return "username not found".into();
    }
    if clip {
        format!("↵ shares your clipboard with {}", cmd.name)
    } else if !file.is_empty() {
        format!("↵ offers {file} to {}", cmd.name)
    } else {
        format!("↵ picks a file to send to {}", cmd.name)
    }
}

/// Tab after `send file to` / `paste to`: the rest of a name from `contacts`.
pub fn share_completion(text: &str, contacts: &[String]) -> String {
    static TO: Lazy = Lazy::new(|| {
        js(
            r"^\s*(?:send\s+(?:a\s+)?file|paste|send\s+(?:my\s+)?clipboard|share\s+(?:my\s+)?clipboard)\s+to\s+([a-z0-9_-]+)$",
            "i",
        )
    });
    let Some(c) = TO.captures(text).ok().flatten() else {
        return String::new();
    };
    let typed = jsre::group(&c, 1);
    let lower = typed.to_lowercase();
    contacts
        .iter()
        .find(|n| n.len() > lower.len() && n.starts_with(&lower))
        .map(|n| n[lower.len()..].to_string())
        .unwrap_or_default()
}

/// What the bar or the thread says once a clip or an offer has gone, or hasn't. `name` is the file's.
pub fn share_status_text(status: &str, to: &str, what: &str, name: &str) -> String {
    let clip = what == "clip";
    match status {
        "delivered" if clip => format!("clipboard sent to {to}"),
        "delivered" => format!("offered {name} to {to} · waiting for them to accept"),
        "offline" if clip => format!("{to} isn’t on q calc right now · not sent"),
        "offline" => format!("{to} isn’t on q calc right now · not offered"),
        "unknown" => format!("nobody’s called {to} · not sent"),
        "not-yours" => send_status_text(SendStatus::NotYours, to),
        "too-many" => "that’s a lot of sending · try again in a minute".into(),
        "too-big" if clip => "that’s too much to paste · not sent".into(),
        "too-big" => "that file’s over 1 GB · not sent".into(),
        "new-name" => "a new name can send files 10 minutes after it’s set".into(),
        "bad" if clip => "that clipboard couldn’t be sent".into(),
        "bad" => "that file couldn’t be sent".into(),
        _ => send_status_text(SendStatus::Network, to),
    }
}

/// One line about a file on its way, for its card and its line in the thread. `who` is the other side; `percent`
/// is how far an upload or download has got.
pub fn file_state_text(state: &str, who: &str, percent: Option<u32>) -> String {
    let pct = percent
        .map(|p| format!(" {}%", p.min(100)))
        .unwrap_or_default();
    match state {
        "preparing" => "encrypting…".into(),
        "offered" => format!("waiting for {who} to accept"),
        "uploading" => format!("sending{pct}"),
        "sent" => format!("sent to {who}"),
        "received" => "".into(),
        "request" => format!("allow {who} to save it"),
        "accepting" => "asking for it…".into(),
        "waiting" => format!("waiting for {who} to send it"),
        "downloading" => format!("downloading{pct}"),
        "decrypting" => "checking…".into(),
        "saved" => "saved to downloads".into(),
        "declined" => format!("{who} turned it down"),
        "cancelled" => format!("{who} took it back"),
        "withdrawn" => "taken back".into(),
        "refused" => "turned down".into(),
        "lost" => "no longer available".into(),
        _ => String::new(),
    }
}

/// Why a file didn't go or come, from the relay's status (or the app's own: `offline`, `corrupt`, `disk`).
pub fn file_error_text(status: &str, who: &str) -> String {
    match status {
        "too-big" | "over" => "that file’s over 1 GB".into(),
        "expired" => "that file expired · ask for it again".into(),
        "gone" => "that file’s gone".into(),
        "no-network" | "offline" => "you’re offline · try again when you’re back".into(),
        "sender-offline" => format!("{who} isn’t online to send it · try again later"),
        "recipient-full" => format!("{who} has too many files waiting · try later"),
        "sender-full" => "too many of your files are waiting · try later".into(),
        "daily-limit" => "that’s today’s 5 GB of files · try tomorrow".into(),
        "relay-full" => "the relay’s full of files right now · try later".into(),
        "too-many" => "that’s a lot of sending · try again in a minute".into(),
        "new-name" => "a new name can send files 10 minutes after it’s set".into(),
        "not-yours" => send_status_text(SendStatus::NotYours, who),
        "corrupt" => "it didn’t arrive intact · not saved".into(),
        "disk" => "couldn’t write it to downloads".into(),
        "unreadable" => "couldn’t read that file".into(),
        _ => "couldn’t reach the relay · try again".into(),
    }
}

/// A file's size as the thread shows it: bytes, then KB, MB, GB (powers of 1000, as Finder does).
pub fn file_size_text(bytes: u64) -> String {
    const UNITS: [&str; 3] = ["KB", "MB", "GB"];
    if bytes < 1000 {
        return format!("{bytes} byte{}", if bytes == 1 { "" } else { "s" });
    }
    let mut v = bytes as f64 / 1000.0;
    let mut u = 0;
    while v >= 999.95 && u < UNITS.len() - 1 {
        v /= 1000.0;
        u += 1;
    }
    if v < 10.0 {
        format!("{:.1} {}", v, UNITS[u])
    } else {
        format!("{:.0} {}", v, UNITS[u])
    }
}

/// How long a file of `size` plain bytes is once encrypted: a 16-byte tag per chunk, and one chunk at least.
pub fn encrypted_len(size: u64) -> u64 {
    size + TAG_BYTES * size.div_ceil(CHUNK_BYTES).max(1)
}

/// A file's name kept to its last path part, without controls, direction marks, separators or leading dots, at
/// most 120 characters with its extension kept; "file" when nothing's left. The relay does the same, and this
/// app does it again before anything touches the disk.
pub fn clean_filename(raw: &str) -> String {
    let last = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let bad = |c: char| matches!(c, '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}' | '\u{200b}'..='\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}' | ':');
    let mut out = String::new();
    let mut in_run = false;
    for c in last.chars() {
        if bad(c) {
            if !in_run {
                out.push('_');
            }
            in_run = true;
        } else {
            out.push(c);
            in_run = false;
        }
    }
    let out = out
        .trim_start_matches(|c: char| c == '.' || c.is_whitespace())
        .trim_end();
    let chars: Vec<char> = out.chars().collect();
    if chars.len() <= FILE_NAME_MAX {
        return if out.is_empty() {
            "file".into()
        } else {
            out.to_string()
        };
    }
    let ext: Vec<char> = match out.rfind('.') {
        Some(i) if i > 0 => out[i..].chars().collect(),
        _ => vec![],
    };
    if ext.len() > 1 && ext.len() <= 16 {
        let stem: String = chars[..FILE_NAME_MAX - ext.len()].iter().collect();
        format!("{}{}", stem.trim_end(), ext.iter().collect::<String>())
    } else {
        chars[..FILE_NAME_MAX]
            .iter()
            .collect::<String>()
            .trim_end()
            .to_string()
    }
}

/// The `n`th name to try when `name` is taken in Downloads: `notes.pdf`, `notes 2.pdf`, `notes 3.pdf`…
pub fn numbered_name(name: &str, n: u32) -> String {
    if n <= 1 {
        return name.to_string();
    }
    match name.rfind('.') {
        Some(i) if i > 0 => format!("{} {n}{}", &name[..i], &name[i..]),
        _ => format!("{name} {n}"),
    }
}

/// Clipboard text as it was copied, when it fits in a clip; None when it's empty or too long.
pub fn clean_clip(raw: &Json) -> Option<String> {
    let s = raw.as_str()?;
    (!s.is_empty() && s.len() <= CLIP_MAX).then(|| s.to_string())
}

/// A clip's first line, for a card's or a list's one-line preview: controls and direction marks as spaces.
pub fn clip_preview(clip: &str) -> String {
    let first = clip
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    clean_message_text(&Json::String(first.to_string()))
}

/// A file offered in a message, as the app keeps it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FileOffer {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub bytes: u64,
    #[serde(rename = "type")]
    pub kind: String,
    pub sha256: String,
    pub key: String,
    pub expires: f64,
}

/// An offer from the relay checked again: an id, a clean name, sizes that add up, a hash and a key.
pub fn clean_offer(raw: &Json) -> Option<FileOffer> {
    static ID: Lazy = Lazy::new(|| js(r"^[0-9a-f]{32}$", ""));
    static SHA: Lazy = Lazy::new(|| js(r"^[0-9a-f]{64}$", ""));
    static KEY: Lazy = Lazy::new(|| js(r"^[A-Za-z0-9_-]{43}$", ""));
    static TYPE: Lazy = Lazy::new(|| {
        js(
            r"^[a-z0-9][a-z0-9!#$&^_.+-]{0,63}/[a-z0-9][a-z0-9!#$&^_.+-]{0,63}$",
            "",
        )
    });
    let str_of = |k: &str| raw.get(k).and_then(Json::as_str).unwrap_or("");
    let id = str_of("id");
    let sha256 = str_of("sha256");
    let key = str_of("key");
    let size = raw.get("size").and_then(Json::as_u64)?;
    let bytes = raw.get("bytes").and_then(Json::as_u64)?;
    if !jsre::test(&ID, id)
        || !jsre::test(&SHA, sha256)
        || !jsre::test(&KEY, key)
        || size > FILE_MAX
        || bytes != encrypted_len(size)
    {
        return None;
    }
    let kind = str_of("type").to_lowercase();
    Some(FileOffer {
        id: id.into(),
        name: clean_filename(str_of("name")),
        size,
        bytes,
        kind: if jsre::test(&TYPE, &kind) {
            kind
        } else {
            "application/octet-stream".into()
        },
        sha256: sha256.into(),
        key: key.into(),
        expires: raw.get("expires").and_then(Json::as_f64).unwrap_or(0.0),
    })
}

/// Whether an offer is taken without asking: only from an accepted name, only under the size set in settings
/// (0, the default, is never).
pub fn auto_accept(m: &Messaging, from: &str, size: u64, limit: u64) -> bool {
    limit > 0 && size <= limit && arrival(m, from) == Arrival::Show
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_the_conversation_commands() {
        let open = |to: &str, name: &str| {
            Some(ThreadCommand::Open {
                to: to.into(),
                name: name.into(),
            })
        };
        assert_eq!(
            parse_messages_command("messages"),
            Some(ThreadCommand::List)
        );
        assert_eq!(
            parse_messages_command("  Open Messages "),
            Some(ThreadCommand::List)
        );
        assert_eq!(parse_messages_command("messages Max"), open("Max", "max"));
        assert_eq!(
            parse_messages_command("messages with max2"),
            open("max2", "max2")
        );
        assert_eq!(
            parse_messages_command("open messages with max"),
            open("max", "max")
        );
        assert_eq!(
            parse_messages_command("open messages max"),
            open("max", "max")
        );
        assert_eq!(parse_messages_command("messages with"), open("", ""));
        assert_eq!(parse_messages_command("messages with "), open("", ""));
        assert_eq!(parse_messages_command("messages x"), open("x", ""));
        assert_eq!(parse_messages_command("messages max hi"), None);
        assert_eq!(parse_messages_command("message max hi"), None);
        assert_eq!(parse_messages_command("messagesmax"), None);
        assert_eq!(parse_messages_command("open max"), None);
        // `message with max` is still a message to someone called `with`
        assert!(parse_message("message with max").is_some());
        assert_eq!(parse_messages_command("message with max"), None);
    }

    #[test]
    fn hints_and_empty_threads() {
        let hint = |t: &str| messages_hint(&parse_messages_command(t).unwrap());
        assert_eq!(hint("messages"), "↵ your conversations");
        assert_eq!(hint("messages with"), "messages with <name>");
        assert_eq!(hint("messages z"), "a name is 2–20 letters, digits, - or _");
        assert_eq!(hint("messages with Max"), "↵ opens messages with max");
        assert_eq!(thread_empty_text("max2"), "no messages with max2 yet");
        assert!(thread_empty_text("").starts_with("no messages yet"));
    }

    #[test]
    fn finishes_names_from_the_history_first() {
        let m = Messaging {
            recent: vec!["bob".into(), "zoe".into()],
            accepted: vec!["bea".into(), "max".into()],
            ..me()
        };
        let contacts = thread_contacts(&["Zed".into(), "bob".into(), "!".into()], &m);
        assert_eq!(contacts, vec!["zed", "bob", "zoe", "bea", "max"]);
        assert_eq!(messages_completion("messages z", &contacts), "ed");
        assert_eq!(messages_completion("messages with b", &contacts), "ob");
        assert_eq!(messages_completion("open messages with ma", &contacts), "x");
        assert_eq!(messages_completion("messages B", &contacts), "ob");
        assert_eq!(messages_completion("messages w", &contacts), "ith ");
        assert_eq!(messages_completion("messages with", &contacts), "");
        assert_eq!(messages_completion("messages max", &contacts), "");
        assert_eq!(messages_completion("messages max ", &contacts), "");
        assert_eq!(messages_completion("message b", &contacts), "");
    }

    #[test]
    fn cleans_and_parses() {
        assert_eq!(
            clean_message_text(&json!("a\u{85}b\u{2066}c\u{0}d")),
            "a b c d"
        );
        assert_eq!(clean_message_text(&json!(5)), "");
        let line = parse_message("message Max hi there").unwrap();
        assert_eq!(
            (line.name.as_str(), line.text.as_str()),
            ("max", "hi there")
        );
        let m = sanitize_messaging(
            &json!({"name": "Ana", "accepted": ["max", "spam"], "blocked": ["spam"]}),
        );
        assert_eq!(m.accepted, vec!["max"]);
        assert_eq!(arrival(&m, "spam"), Arrival::Drop);
    }

    fn me() -> Messaging {
        Messaging {
            name: "ana".into(),
            key: "ab".repeat(32),
            accepted: vec!["max".into()],
            blocked: vec!["spam".into()],
            recent: vec![],
        }
    }

    #[test]
    fn says_not_found_only_when_the_relay_said_so() {
        let me = me();
        let line = parse_message("message zed hi").unwrap();
        assert_eq!(
            message_hint(&line, &me, true, Some(false)),
            "username not found"
        );
        let bare = parse_message("message zed").unwrap();
        assert_eq!(
            message_hint(&bare, &me, true, Some(false)),
            "username not found"
        );
        assert_eq!(message_hint(&line, &me, true, None), "↵ sends to zed");
        assert_eq!(message_hint(&line, &me, true, Some(true)), "↵ sends to zed");
        let short = parse_message("message z hi").unwrap();
        assert!(message_hint(&short, &me, true, Some(false)).contains("2–20 letters"));
    }

    #[test]
    fn finds_the_name_to_paint() {
        assert_eq!(message_name_span("message Max hi"), Some((8, 11)));
        assert_eq!(message_name_span("  message  max"), Some((11, 14)));
        assert_eq!(message_name_span("message "), None);
    }

    #[test]
    fn keeps_recent_recipients_first() {
        use crate::app::completion::{CompletionNames, completion_for};
        let mut m = with_recent(&with_recent(&with_recent(&me(), "Zoe"), "bob"), "zoe");
        assert_eq!(m.recent, vec!["zoe", "bob"]);
        assert_eq!(with_recent(&m, "x"), m);
        let both = Messaging {
            accepted: vec!["bob".into(), "max".into()],
            ..m.clone()
        };
        assert_eq!(message_contacts(&both), vec!["zoe", "bob", "max"]);
        for i in 0..20 {
            m = with_recent(&m, &format!("n{i}"));
        }
        assert_eq!(m.recent.len(), RECENT_MAX);
        assert_eq!(m.recent[0], "n19");
        let stored = sanitize_messaging(&json!({"recent": ["Zoe", "zoe", "x", "bob", 3]}));
        assert_eq!(stored.recent, vec!["zoe", "bob"]);
        let names = CompletionNames {
            contacts: message_contacts(&Messaging {
                recent: vec!["bea".into(), "bob".into()],
                accepted: vec!["bo".into()],
                ..m
            }),
            ..CompletionNames::default()
        };
        assert_eq!(completion_for("message b", &names), "ea");
    }
}

#[cfg(test)]
mod share_tests {
    use super::*;
    use serde_json::json;

    fn me() -> Messaging {
        Messaging {
            name: "ana".into(),
            key: "ab".repeat(32),
            accepted: vec!["max".into()],
            blocked: vec!["spam".into()],
            recent: vec![],
        }
    }

    fn cmd(what: &str, to: &str, name: &str) -> Option<ShareCommand> {
        Some(ShareCommand {
            what: what.into(),
            to: to.into(),
            name: name.into(),
        })
    }

    #[test]
    fn reads_the_share_commands() {
        assert_eq!(
            parse_share_command("send file to Max"),
            cmd("file", "Max", "max")
        );
        assert_eq!(
            parse_share_command("  Send a File to max "),
            cmd("file", "max", "max")
        );
        assert_eq!(parse_share_command("send file to"), cmd("file", "", ""));
        assert_eq!(parse_share_command("send file to "), cmd("file", "", ""));
        assert_eq!(
            parse_share_command("paste to max"),
            cmd("clip", "max", "max")
        );
        assert_eq!(
            parse_share_command("send clipboard to max"),
            cmd("clip", "max", "max")
        );
        assert_eq!(
            parse_share_command("share my clipboard to bo"),
            cmd("clip", "bo", "bo")
        );
        assert_eq!(parse_share_command("paste to x"), cmd("clip", "x", ""));
        assert_eq!(parse_share_command("paste to max now"), None);
        assert_eq!(parse_share_command("send file"), None);
        assert_eq!(parse_share_command("paste"), None);
        assert_eq!(parse_share_command("message max paste to max"), None);
        // not a message to someone called `file`
        assert!(parse_share_command("message file to max").is_none());
    }

    #[test]
    fn hints_a_share() {
        let m = me();
        let hint = |t: &str, found: Option<bool>, file: &str| {
            share_hint(&parse_share_command(t).unwrap(), &m, true, found, file)
        };
        assert_eq!(hint("send file to", None, ""), "send file to <name>");
        assert_eq!(hint("send file to", None, "a.pdf"), "send a.pdf to <name>");
        assert_eq!(
            hint("paste to", None, ""),
            "paste to <name> · shares what you’ve copied"
        );
        assert_eq!(
            hint("paste to z", None, ""),
            "a name is 2–20 letters, digits, - or _"
        );
        assert_eq!(hint("paste to zed", Some(false), ""), "username not found");
        assert_eq!(
            hint("paste to zed", None, ""),
            "↵ shares your clipboard with zed"
        );
        assert_eq!(
            hint("send file to max", Some(true), ""),
            "↵ picks a file to send to max"
        );
        assert_eq!(
            hint("send file to max", None, "a.pdf"),
            "↵ offers a.pdf to max"
        );
        let off = share_hint(
            &parse_share_command("paste to max").unwrap(),
            &Messaging::default(),
            true,
            None,
            "",
        );
        assert!(off.starts_with("set your name"));
        let none = share_hint(
            &parse_share_command("paste to max").unwrap(),
            &m,
            false,
            None,
            "",
        );
        assert_eq!(none, "messages need the online relay");
    }

    #[test]
    fn finishes_names_after_a_share() {
        let contacts = vec!["max".to_string(), "bea".into()];
        assert_eq!(share_completion("paste to m", &contacts), "ax");
        assert_eq!(share_completion("send file to B", &contacts), "ea");
        assert_eq!(share_completion("send clipboard to max", &contacts), "");
        assert_eq!(share_completion("paste m", &contacts), "");
    }

    #[test]
    fn says_how_a_share_went() {
        assert_eq!(
            share_status_text("delivered", "max", "clip", ""),
            "clipboard sent to max"
        );
        assert_eq!(
            share_status_text("delivered", "max", "file", "a.pdf"),
            "offered a.pdf to max · waiting for them to accept"
        );
        assert_eq!(
            share_status_text("offline", "max", "file", "a.pdf"),
            "max isn’t on q calc right now · not offered"
        );
        assert_eq!(
            share_status_text("too-big", "max", "file", ""),
            "that file’s over 1 GB · not sent"
        );
        assert_eq!(
            share_status_text("new-name", "max", "file", ""),
            "a new name can send files 10 minutes after it’s set"
        );
        assert_eq!(
            share_status_text("weird", "max", "clip", ""),
            "couldn’t reach the relay · not sent"
        );
        assert_eq!(file_state_text("uploading", "max", Some(42)), "sending 42%");
        assert_eq!(
            file_state_text("downloading", "ana", Some(140)),
            "downloading 100%"
        );
        assert_eq!(
            file_state_text("waiting", "ana", None),
            "waiting for ana to send it"
        );
        assert_eq!(
            file_state_text("declined", "max", None),
            "max turned it down"
        );
        assert_eq!(
            file_error_text("expired", "ana"),
            "that file expired · ask for it again"
        );
        assert_eq!(
            file_error_text("offline", "ana"),
            "you’re offline · try again when you’re back"
        );
        assert_eq!(
            file_error_text("no-network", "ana"),
            "you’re offline · try again when you’re back"
        );
        assert_eq!(file_state_text("sent", "max", None), "sent to max");
        assert_eq!(file_error_text("over", "ana"), "that file’s over 1 GB");
        assert_eq!(
            file_error_text("sender-offline", "ana"),
            "ana isn’t online to send it · try again later"
        );
        assert_eq!(
            file_error_text("corrupt", "ana"),
            "it didn’t arrive intact · not saved"
        );
    }

    #[test]
    fn sizes_and_encrypted_lengths() {
        assert_eq!(file_size_text(0), "0 bytes");
        assert_eq!(file_size_text(1), "1 byte");
        assert_eq!(file_size_text(999), "999 bytes");
        assert_eq!(file_size_text(1000), "1.0 KB");
        assert_eq!(file_size_text(1_234_567), "1.2 MB");
        assert_eq!(file_size_text(25_000_000), "25 MB");
        assert_eq!(file_size_text(999_990), "1.0 MB");
        assert_eq!(file_size_text(FILE_MAX), "1.1 GB");
        assert_eq!(encrypted_len(0), 16);
        assert_eq!(encrypted_len(1), 17);
        assert_eq!(encrypted_len(CHUNK_BYTES), CHUNK_BYTES + 16);
        assert_eq!(encrypted_len(CHUNK_BYTES + 1), CHUNK_BYTES + 1 + 32);
        // the relay takes up to 1 GiB plus 1 MiB of tags
        assert!(encrypted_len(FILE_MAX) <= FILE_MAX + 1024 * 1024);
    }

    #[test]
    fn cleans_file_names() {
        assert_eq!(clean_filename("../../etc/passwd"), "passwd");
        assert_eq!(clean_filename("C:\\Users\\me\\a.txt"), "a.txt");
        assert_eq!(clean_filename(".hidden"), "hidden");
        assert_eq!(clean_filename("evil\u{202e}txt.exe"), "evil_txt.exe");
        assert_eq!(clean_filename("a\nb:c"), "a_b_c");
        assert_eq!(clean_filename("..."), "file");
        assert_eq!(clean_filename(""), "file");
        let long = clean_filename(&format!("{}.tar.gz", "x".repeat(300)));
        assert_eq!(long.chars().count(), 120);
        assert!(long.ends_with(".gz"));
        assert_eq!(numbered_name("notes.pdf", 1), "notes.pdf");
        assert_eq!(numbered_name("notes.pdf", 2), "notes 2.pdf");
        assert_eq!(numbered_name("README", 3), "README 3");
    }

    #[test]
    fn takes_clips_as_they_are() {
        assert_eq!(clean_clip(&json!(" a\n\tb ")), Some(" a\n\tb ".into()));
        assert_eq!(clean_clip(&json!("")), None);
        assert_eq!(clean_clip(&json!("x".repeat(CLIP_MAX + 1))), None);
        assert_eq!(clean_clip(&json!(3)), None);
        assert_eq!(
            clip_preview("\n  first line\u{202e}x \nsecond"),
            "first line x"
        );
    }

    #[test]
    fn checks_an_offer_again() {
        let good = json!({"id": "ab".repeat(16), "name": "../a.pdf", "size": 5, "bytes": 21, "type": "Application/PDF",
                          "sha256": "cd".repeat(32), "key": "A".repeat(43), "expires": 9.0});
        let o = clean_offer(&good).unwrap();
        assert_eq!(
            (o.name.as_str(), o.kind.as_str(), o.size, o.bytes),
            ("a.pdf", "application/pdf", 5, 21)
        );
        let with = |k: &str, v: Json| {
            let mut g = good.clone();
            g[k] = v;
            clean_offer(&g)
        };
        assert!(with("bytes", json!(22)).is_none());
        assert!(with("id", json!("nope")).is_none());
        assert!(with("key", json!("short")).is_none());
        assert!(with("size", json!(-1)).is_none());
        assert_eq!(
            with("type", json!("text/html<x>")).unwrap().kind,
            "application/octet-stream"
        );
    }

    #[test]
    fn accepts_on_its_own_only_when_asked_to() {
        let m = me();
        assert!(!auto_accept(&m, "max", 10, 0));
        assert!(auto_accept(&m, "max", 10, 10));
        assert!(!auto_accept(&m, "max", 11, 10));
        assert!(!auto_accept(&m, "zed", 1, 10));
        assert!(!auto_accept(&m, "spam", 1, 10));
    }
}
