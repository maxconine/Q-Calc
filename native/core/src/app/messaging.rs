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

/// A message or file has reached `to`: they're recent, and accepted. Writing to someone first is the handshake, so
/// once they allow you their replies show here without a request of their own (a block of them lifts too: you
/// wrote to them).
pub fn sent_to(m: &Messaging, to: &str) -> Messaging {
    let name = clean_name(&Json::String(to.to_string()));
    if name.is_empty() {
        return m.clone();
    }
    allow_sender(&with_recent(m, &name), &name)
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

/// An accepted name's message shows, and so does a reply from someone you wrote to; a blocked name's goes unseen;
/// anyone else's is a request.
pub fn arrival(m: &Messaging, from: &str) -> Arrival {
    if m.blocked.iter().any(|n| n == from) {
        Arrival::Drop
    } else if m.accepted.iter().any(|n| n == from) || m.recent.iter().any(|n| n == from) {
        // someone you've written to is answering you (`sent_to`; and those written to before it accepted them)
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
/// The thread's empty reply line (native only): who it goes to, or, with no name set, what ↵ does instead.
pub fn thread_reply_prompt(name: &str, named: bool) -> String {
    if named {
        format!("Message {name}")
    } else {
        "set your name to reply · ↵ opens settings".into()
    }
}

pub fn thread_empty_text(name: &str) -> String {
    if name.is_empty() {
        "no messages yet · message <name> <text> starts one".into()
    } else {
        format!("no messages with {name} yet")
    }
}

/// What VoiceOver says when a card arrives (native only): the cards come and go on their own, and nothing moves
/// the VoiceOver cursor to them. `kind` is `ask`, `clip`, `file` (with its name in `text`) or the message's words.
pub fn card_announcement(from: &str, kind: &str, text: &str) -> String {
    match kind {
        "ask" => format!("{from} wants to message you"),
        "clip" => format!("{from} shared their clipboard"),
        "file" if text.is_empty() => format!("{from} sent a file"),
        "file" => format!("{from} sent {text}"),
        _ => format!("message from {from}: {text}"),
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
    let clip = cmd.what == "clip";
    if m.name.is_empty() || m.key.is_empty() {
        // what this line does, not `message`'s words
        return if clip {
            "set your name in settings to share your clipboard · ↵ opens settings".into()
        } else {
            "set your name in settings to send files · ↵ opens settings".into()
        };
    }
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
        "no-name" if clip => "set your name in settings to share your clipboard".into(),
        "no-name" => "set your name in settings to send files".into(),
        "empty-clipboard" => "there’s nothing copied to share".into(),
        "folder" => "a folder can’t be sent · zip it first".into(),
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
        "key-changed" => format!("{to}’s security key changed · not sent"),
        "no-key" => format!("{to} needs a newer q calc to get files · not sent"),
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
        "key-changed" => key_changed_text(who),
        "sealed" => "it couldn’t be opened · not saved".into(),
        "no-key" => format!("{who} needs a newer q calc to get files"),
        "disk" => "couldn’t write it to downloads".into(),
        "unreadable" => "couldn’t read that file".into(),
        "folder" => "a folder can’t be sent · zip it first".into(),
        _ => "couldn’t reach the relay · try again".into(),
    }
}

/// The most files one drop sends: the relay starts 20 uploads an hour per address (relay/src/files.js `CAPS`), so a
/// bigger drop would mostly fail one by one with `that’s a lot of sending`.
pub const DROP_MAX: u32 = 20;

/// Why a drop of `count` files doesn't go (empty when it can): past `DROP_MAX`, nothing is sent.
pub fn drop_count_text(count: u32) -> String {
    if count > DROP_MAX {
        format!("that’s {count} files · up to {DROP_MAX} at a time, or zip them into one")
    } else {
        String::new()
    }
}

/// What the bar calls files dropped on it while the line asks for a name (`send a.pdf to <name>`): the one
/// file's clean name, or how many there are.
pub fn dropped_label(first: &str, count: u32) -> String {
    if count > 1 {
        format!("{count} files")
    } else {
        clean_filename(first)
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

/// A file's name kept to its last path part, safe to show and to save: NFC, no path separators, no leading dots
/// or spaces, no controls, direction marks, invisible or formatting characters (each run becomes `_`), runs of
/// spaces as one, no trailing dots or spaces, at most 120 characters with its extension kept so it always shows;
/// "file" when nothing's left. The sender cleans it before sealing it, and this app does it again after opening
/// it, before anything touches the disk.
pub fn clean_filename(raw: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let nfc: String = raw.nfc().collect();
    let last = nfc.rsplit(['/', '\\']).next().unwrap_or("");
    let bad = |c: char| {
        matches!(c,
            '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}' | '\u{ad}' | '\u{34f}' | '\u{61c}' | '\u{115f}' | '\u{1160}'
            | '\u{17b4}' | '\u{17b5}' | '\u{180b}'..='\u{180f}' | '\u{200b}'..='\u{200f}' | '\u{2028}'..='\u{202e}'
            | '\u{2060}'..='\u{206f}' | '\u{3164}' | '\u{feff}' | '\u{ffa0}' | '\u{fff9}'..='\u{fffb}'
            | '\u{1d173}'..='\u{1d17a}' | '\u{e0000}'..='\u{e0fff}' | ':')
    };
    let mut out = String::new();
    let mut in_run = false;
    let mut in_space = false;
    for c in last.chars() {
        if bad(c) {
            if !in_run {
                out.push('_');
            }
            in_run = true;
            in_space = false;
        } else if c.is_whitespace() {
            if !in_space {
                out.push(' ');
            }
            in_space = true;
            in_run = false;
        } else {
            out.push(c);
            in_run = false;
            in_space = false;
        }
    }
    let out = out
        .trim_start_matches(|c: char| c == '.' || c.is_whitespace())
        .trim_end_matches(|c: char| c == '.' || c.is_whitespace());
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

// ---- end-to-end keys (native only; FileCrypt.swift and ShareCrypt.swift do the crypto, native/PRIVACY.md) ----
//
// Each app has an X25519 key pair; the public half is published with the name on the relay and pinned by each
// contact the first time it's used. A file's or a clip's facts (for a file: its name, type, size, SHA-256 and
// its own AES-256 key) are sealed with AES-256-GCM under a key from HKDF-SHA256 over two X25519 agreements,
// a fresh ephemeral key's with the recipient's and the sender's own with the recipient's, so only the recipient
// can open it and only the sender could have made it. The HKDF info, which is also the GCM associated data, is
// `share_context`: the version, the kind, both names, the id and both public keys. The relay sees only the id,
// the sealed bytes and, for a file, the encrypted length it has to hold.

/// What every derivation and seal is bound to first, so a new scheme can't be confused with this one.
pub const SHARE_VERSION: &str = "qcalc-share-v1";
/// The longest sealed box the relay passes, in base64url: a file's facts are well under 2 KB.
pub const SEALED_FILE_MAX: usize = 4096;
/// A clip's sealed box: a 12-byte nonce, up to CLIP_MAX bytes and a 16-byte tag, in base64url.
pub const SEALED_CLIP_MAX: usize = (CLIP_MAX + 28).div_ceil(3) * 4;

/// SHA-256 (FIPS 180-4), for the safety code; the core has no crypto crates and needs nothing else.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_be_bytes());
    for block in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (i, word) in block.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes(*word);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [
                t1.wrapping_add(t2),
                v[0],
                v[1],
                v[2],
                v[3].wrapping_add(t1),
                v[4],
                v[5],
                v[6],
            ];
        }
        for (a, b) in h.iter_mut().zip(v) {
            *a = a.wrapping_add(b);
        }
    }
    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// base64url without padding, as the apps write keys and sealed boxes; None for anything else.
pub fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    };
    if s.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut n = 0;
    for c in s.bytes() {
        acc = (acc << 6) | u32::from(val(c)?);
        n += 6;
        if n >= 8 {
            n -= 8;
            out.push((acc >> n) as u8);
        }
    }
    // the leftover bits must be zero, so each key has one spelling
    (acc & ((1 << n) - 1) == 0).then_some(out)
}

/// A published X25519 public key: 32 bytes in base64url (43 characters), not all zero; None otherwise.
pub fn clean_public_key(raw: &Json) -> Option<String> {
    let s = raw.as_str()?;
    if s.len() != 43 {
        return None;
    }
    let bytes = b64url_decode(s)?;
    (bytes.len() == 32 && bytes.iter().any(|b| *b != 0)).then(|| s.to_string())
}

/// The safety code two people compare out loud: six groups of five digits from SHA-256 over both public keys,
/// smaller first, so both Macs show the same code. None when either isn't a key.
pub fn safety_code(a: &str, b: &str) -> Option<String> {
    let ka = b64url_decode(&clean_public_key(&Json::String(a.into()))?)?;
    let kb = b64url_decode(&clean_public_key(&Json::String(b.into()))?)?;
    let (lo, hi) = if ka <= kb { (ka, kb) } else { (kb, ka) };
    let mut input = format!("{SHARE_VERSION} safety code\n").into_bytes();
    input.extend_from_slice(&lo);
    input.extend_from_slice(&hi);
    let d = sha256(&input);
    let groups: Vec<String> = d
        .as_chunks::<5>()
        .0
        .iter()
        .take(6)
        .map(|c| {
            let v = c.iter().fold(0u64, |v, b| (v << 8) | u64::from(*b));
            format!("{:05}", v % 100_000)
        })
        .collect();
    Some(groups.join(" "))
}

/// What a seal is bound to (the HKDF info and the GCM associated data): the version, `file` or `clip`, the
/// sender's and recipient's names, the id and both public keys, one per line. None when any isn't one, so a
/// seal is never made over something the other side would read differently.
pub fn share_context(
    kind: &str,
    from: &str,
    to: &str,
    id: &str,
    from_key: &str,
    to_key: &str,
) -> Option<String> {
    static ID: Lazy = Lazy::new(|| js(r"^[0-9a-f]{32}$", ""));
    let name = |n: &str| Some(clean_name(&Json::String(n.into()))).filter(|c| c == n);
    let key = |k: &str| clean_public_key(&Json::String(k.into()));
    if !matches!(kind, "file" | "clip") || !jsre::test(&ID, id) {
        return None;
    }
    Some(format!(
        "{SHARE_VERSION}\n{kind}\n{}\n{}\n{id}\n{}\n{}",
        name(from)?,
        name(to)?,
        key(from_key)?,
        key(to_key)?
    ))
}

/// A sealed share as the relay passes it on: its id, the sender's ephemeral public key and the sealed box, and
/// for a file the encrypted length the relay holds and when the offer lapses.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SealedShare {
    pub id: String,
    pub epk: String,
    pub sealed: String,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub expires: f64,
}

fn clean_sealed(raw: &Json, max: usize) -> Option<SealedShare> {
    static ID: Lazy = Lazy::new(|| js(r"^[0-9a-f]{32}$", ""));
    static BOX: Lazy = Lazy::new(|| js(r"^[A-Za-z0-9_-]+$", ""));
    let str_of = |k: &str| raw.get(k).and_then(Json::as_str).unwrap_or("");
    let id = str_of("id");
    let sealed = str_of("sealed");
    let epk = clean_public_key(raw.get("epk")?)?;
    // a nonce, a tag and at least a byte between them
    let fits = (40..=max).contains(&sealed.len()) && jsre::test(&BOX, sealed);
    if !jsre::test(&ID, id) || !fits || b64url_decode(sealed).is_none() {
        return None;
    }
    Some(SealedShare {
        id: id.into(),
        epk,
        sealed: sealed.into(),
        bytes: 0,
        expires: raw.get("expires").and_then(Json::as_f64).unwrap_or(0.0),
    })
}

/// A file's offer from the relay checked again: an id, an ephemeral key, a sealed box of the right size and an
/// encrypted length no more than a 1 GiB file's. Its facts are inside the box.
pub fn clean_sealed_offer(raw: &Json) -> Option<SealedShare> {
    let bytes = raw.get("bytes").and_then(Json::as_u64)?;
    if !(encrypted_len(0)..=encrypted_len(FILE_MAX)).contains(&bytes) {
        return None;
    }
    Some(SealedShare {
        bytes,
        ..clean_sealed(raw, SEALED_FILE_MAX)?
    })
}

/// A clip from the relay checked again: an id, an ephemeral key and a sealed box no bigger than a clip's.
pub fn clean_sealed_clip(raw: &Json) -> Option<SealedShare> {
    clean_sealed(raw, SEALED_CLIP_MAX)
}

/// A file's facts, as the sender seals them and the recipient reads them once opened.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FileFacts {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub size: u64,
    pub sha256: String,
    /// the file's own AES-256 key, base64url
    pub key: String,
}

fn clean_type(raw: &str) -> String {
    static TYPE: Lazy = Lazy::new(|| {
        js(
            r"^[a-z0-9][a-z0-9!#$&^_.+-]{0,63}/[a-z0-9][a-z0-9!#$&^_.+-]{0,63}$",
            "",
        )
    });
    let kind = jstr::trim(raw.split(';').next().unwrap_or("")).to_lowercase();
    if jsre::test(&TYPE, &kind) {
        kind
    } else {
        "application/octet-stream".into()
    }
}

/// What the sender seals for a file: its facts as one line of JSON, the name and type cleaned first. None when
/// the size is over 1 GiB or the hash or key isn't one.
pub fn seal_file_facts(
    name: &str,
    kind: &str,
    size: u64,
    sha256: &str,
    key: &str,
) -> Option<String> {
    let facts = FileFacts {
        name: clean_filename(name),
        kind: clean_type(kind),
        size,
        sha256: sha256.into(),
        key: key.into(),
    };
    let json = serde_json::to_string(
        &serde_json::json!({"v": 1, "name": facts.name, "type": facts.kind,
        "size": facts.size, "sha256": facts.sha256, "key": facts.key}),
    )
    .ok()?;
    open_file_facts(&json, encrypted_len(size)).map(|_| json)
}

/// A file's facts once opened, checked again on the receiving side: version 1, a clean name and type, a size no
/// more than 1 GiB whose encrypted length is exactly what the relay holds, a SHA-256 and a key. None otherwise.
pub fn open_file_facts(plain: &str, bytes: u64) -> Option<FileFacts> {
    static SHA: Lazy = Lazy::new(|| js(r"^[0-9a-f]{64}$", ""));
    static KEY: Lazy = Lazy::new(|| js(r"^[A-Za-z0-9_-]{43}$", ""));
    let o: Json = serde_json::from_str(plain).ok()?;
    let str_of = |k: &str| o.get(k).and_then(Json::as_str).unwrap_or("");
    let size = o.get("size").and_then(Json::as_u64)?;
    let (sha256, key) = (str_of("sha256"), str_of("key"));
    let key_ok = jsre::test(&KEY, key) && b64url_decode(key).is_some_and(|k| k.len() == 32);
    if o.get("v").and_then(Json::as_u64) != Some(1)
        || size > FILE_MAX
        || encrypted_len(size) != bytes
        || !jsre::test(&SHA, sha256)
        || !key_ok
    {
        return None;
    }
    Some(FileFacts {
        name: clean_filename(str_of("name")),
        kind: clean_type(str_of("type")),
        size,
        sha256: sha256.into(),
        key: key.into(),
    })
}

/// Where a contact's key stands against the one pinned for them: `new` (nothing pinned yet: pin it), `same`, or
/// `changed` (refuse to send or open until it's trusted), or `bad` when it isn't a key.
pub fn pin_state(pins: &Json, who: &str, key: &str) -> &'static str {
    let Some(key) = clean_public_key(&Json::String(key.into())) else {
        return "bad";
    };
    match pins.get(who).and_then(clean_public_key) {
        None => "new",
        Some(p) if p == key => "same",
        Some(_) => "changed",
    }
}

/// The bar's one-line notice, the first time you send or save a file or clip, until it's dismissed.
pub fn privacy_notice() -> String {
    "files and clipboard shares are end-to-end encrypted · q calc’s relay can’t read them, only who sent to \
     whom, the size and when · files are deleted after download or within 24 hours"
        .into()
}

/// Settings › Messages › Privacy: the notice, and what the safety code is for.
pub fn privacy_settings_text() -> Vec<String> {
    vec![
        "Files and clipboard shares are end-to-end encrypted. Q Calc’s relay can’t read them; it only sees who \
         sent to whom, the size and when. Files are deleted after download or within 24 hours."
            .into(),
        "Each contact has a safety code in their thread’s info (ⓘ). Compare it with them out loud or in person: \
         if it’s the same on both Macs, nobody is in between. If someone’s key changes, Q Calc stops sending to \
         them until you trust the new one."
            .into(),
        "Text messages aren’t end-to-end encrypted, so older copies of Q Calc can still read them. The relay \
         passes them on and doesn’t keep them."
            .into(),
    ]
}

/// The lock beside a file or a clip in the thread.
pub fn lock_tooltip(who: &str) -> String {
    format!("end-to-end encrypted · only you and {who} can open it")
}

/// The line a thread shows while a contact's key has changed, and what to do about it.
pub fn key_changed_text(who: &str) -> String {
    format!("{who}’s security key changed")
}

pub fn key_changed_detail(who: &str) -> String {
    format!(
        "nothing goes to or from {who} until you trust it · compare the new safety code with {who} first"
    )
}

/// A thread's info: the safety code, or why there isn't one yet.
pub fn safety_code_text(who: &str, code: &str) -> String {
    if code.is_empty() {
        format!("no safety code with {who} yet · it shows once {who} has a key")
    } else {
        format!("safety code with {who}: {code}")
    }
}

pub fn safety_code_detail(who: &str) -> String {
    format!("compare it with {who} out loud · if it’s the same on both macs, nobody’s in between")
}

/// What a drop on the bar does with the line as it is: the name the files go to and any message typed after
/// it, or (name "") why they can't go, in the words `message` uses. Lines that are neither a message nor a share
/// command give nothing (`hint` ""), and the bar asks for a name as before.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DropTarget {
    pub name: String,
    pub text: String,
    pub hint: String,
}

pub fn drop_target(
    line: &str,
    m: &Messaging,
    relay: bool,
    found: Option<bool>,
    count: u32,
) -> DropTarget {
    let (to, name, text) = if let Some(share) = parse_share_command(line) {
        (share.to, share.name, String::new())
    } else if let Some(msg) = parse_message(line) {
        (msg.to, msg.name, msg.text)
    } else if !line.trim().is_empty() {
        // a calculation on the line stays; a drop there used to swap it for `send file to `
        return DropTarget {
            hint: "clear the line to send files · or drop them on a thread".into(),
            ..DropTarget::default()
        };
    } else {
        return DropTarget::default();
    };
    let why = |hint: String| DropTarget {
        hint,
        ..DropTarget::default()
    };
    if !relay {
        return why("messages need the online relay".into());
    }
    if m.name.is_empty() || m.key.is_empty() {
        return why("set your name in settings to send files".into());
    }
    if to.is_empty() {
        return why("message <name> first, then drop to send it".into());
    }
    if name.is_empty() {
        return why("a name is 2–20 letters, digits, - or _".into());
    }
    if found == Some(false) {
        return why("username not found".into());
    }
    if count > DROP_MAX {
        return why(drop_count_text(count));
    }
    let what = if count > 1 {
        format!("{count} files ")
    } else {
        String::new()
    };
    DropTarget {
        hint: format!("drop to send {what}to {name}"),
        name,
        text,
    }
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
    fn a_share_line_with_no_name_says_what_it_needs_one_for() {
        let none = Messaging::default();
        let file = parse_share_command("send file to max").unwrap();
        let clip = parse_share_command("paste to max").unwrap();
        assert_eq!(
            share_hint(&file, &none, true, None, ""),
            "set your name in settings to send files · ↵ opens settings"
        );
        assert_eq!(
            share_hint(&clip, &none, true, None, ""),
            "set your name in settings to share your clipboard · ↵ opens settings"
        );
    }

    #[test]
    fn says_when_theres_no_name_rather_than_a_taken_one() {
        assert_eq!(
            share_status_text("no-name", "max", "file", "a.pdf"),
            "set your name in settings to send files"
        );
        assert_eq!(
            share_status_text("no-name", "max", "clip", ""),
            "set your name in settings to share your clipboard"
        );
        assert_ne!(
            share_status_text("no-name", "max", "file", ""),
            share_status_text("not-yours", "max", "file", "")
        );
        assert_eq!(
            share_status_text("empty-clipboard", "max", "clip", ""),
            "there’s nothing copied to share"
        );
        assert_eq!(
            share_status_text("folder", "max", "file", ""),
            file_error_text("folder", "max")
        );
    }

    #[test]
    fn the_reply_line_says_when_a_name_is_needed() {
        assert_eq!(thread_reply_prompt("max", true), "Message max");
        assert_eq!(
            thread_reply_prompt("max", false),
            "set your name to reply · ↵ opens settings"
        );
    }

    #[test]
    fn a_huge_drop_is_refused_whole() {
        assert_eq!(drop_count_text(1), "");
        assert_eq!(drop_count_text(DROP_MAX), "");
        assert_eq!(
            drop_count_text(21),
            "that’s 21 files · up to 20 at a time, or zip them into one"
        );
    }

    #[test]
    fn names_what_was_dropped() {
        assert_eq!(dropped_label("a.pdf", 1), "a.pdf");
        assert_eq!(dropped_label("a.pdf", 3), "3 files");
        assert_eq!(
            file_error_text("folder", "max"),
            "a folder can’t be sent · zip it first"
        );
        let m = Messaging {
            name: "me".into(),
            key: "k".into(),
            ..Messaging::default()
        };
        let p = parse_share_command("send file to max").unwrap();
        assert_eq!(
            share_hint(&p, &m, true, None, &dropped_label("x", 2)),
            "↵ offers 2 files to max"
        );
    }

    #[test]
    fn says_what_a_card_is() {
        assert_eq!(
            card_announcement("max", "ask", ""),
            "max wants to message you"
        );
        assert_eq!(
            card_announcement("max", "clip", ""),
            "max shared their clipboard"
        );
        assert_eq!(card_announcement("max", "file", "a.pdf"), "max sent a.pdf");
        assert_eq!(card_announcement("max", "file", ""), "max sent a file");
        assert_eq!(
            card_announcement("max", "text", "hi"),
            "message from max: hi"
        );
    }

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
    fn writing_first_is_the_handshake() {
        // ana writes to zoe: zoe's reply shows, her files can save on their own, and nothing asks
        let m = sent_to(&me(), "Zoe");
        assert_eq!(arrival(&m, "zoe"), Arrival::Show);
        assert!(m.accepted.contains(&"zoe".to_string()));
        assert!(auto_accept(&m, "zoe", 10, 100));
        // a name written to before this kept only as recent still shows
        let before = Messaging {
            recent: vec!["bea".into()],
            ..me()
        };
        assert_eq!(arrival(&before, "bea"), Arrival::Show);
        // writing to someone blocked lifts the block; a stranger still asks; nothing for a name that can't be one
        assert_eq!(arrival(&sent_to(&me(), "spam"), "spam"), Arrival::Show);
        assert_eq!(arrival(&m, "eve"), Arrival::Ask);
        assert_eq!(sent_to(&me(), "!!"), me());
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
        // invisible and formatting characters, nfc, runs of spaces, trailing dots
        assert_eq!(clean_filename("a\u{2060}b\u{e0041}.txt"), "a_b_.txt");
        assert_eq!(clean_filename("cafe\u{301}.txt"), "caf\u{e9}.txt");
        assert_eq!(
            clean_filename("photo.jpg      \u{3000}  .exe"),
            "photo.jpg .exe"
        );
        assert_eq!(clean_filename("report.pdf. . ."), "report.pdf");
        assert_eq!(clean_filename("\u{200b}.bashrc"), "_.bashrc");
        assert_eq!(clean_filename("..\\..\\x.app"), "x.app");
        let long_ext = clean_filename(&format!("{}.exe", "\u{e9}".repeat(200)));
        assert_eq!(long_ext.chars().count(), 120);
        assert!(long_ext.ends_with(".exe"));
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

    const KA: &str = "AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyA";
    const KB: &str = "ISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0-P0A";

    fn hex(d: &[u8]) -> String {
        d.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn hashes_as_sha256_does() {
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&sha256(&[b'a'; 1000])),
            "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3"
        );
    }

    #[test]
    fn reads_keys_in_base64url_only() {
        assert_eq!(b64url_decode(KA).unwrap(), (1..=32).collect::<Vec<u8>>());
        assert_eq!(b64url_decode("").unwrap(), Vec::<u8>::new());
        assert!(b64url_decode("AQ+D").is_none());
        assert!(b64url_decode("AQ==").is_none());
        assert!(b64url_decode("A").is_none());
        // leftover bits must be zero, so a key has one spelling
        assert!(b64url_decode("AR").is_none());
        assert_eq!(clean_public_key(&json!(KA)), Some(KA.into()));
        assert_eq!(clean_public_key(&json!("A".repeat(43))), None);
        assert_eq!(clean_public_key(&json!(&KA[..42])), None);
        assert_eq!(clean_public_key(&json!(3)), None);
    }

    #[test]
    fn derives_one_safety_code_on_both_sides() {
        let code = safety_code(KA, KB).unwrap();
        assert_eq!(code, "61110 50086 04472 66800 58278 65420");
        assert_eq!(safety_code(KB, KA).unwrap(), code);
        assert_eq!(code.split(' ').count(), 6);
        assert!(safety_code(KA, "nope").is_none());
        // a different key, a different code
        let other = "AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyE";
        assert_ne!(safety_code(other, KB).unwrap(), code);
    }

    #[test]
    fn binds_a_seal_to_both_names_the_id_and_both_keys() {
        let id = "ab".repeat(16);
        assert_eq!(
            share_context("file", "ana", "max2", &id, KA, KB).unwrap(),
            format!("qcalc-share-v1\nfile\nana\nmax2\n{id}\n{KA}\n{KB}")
        );
        assert!(share_context("clip", "ana", "max2", &id, KA, KB).is_some());
        assert!(share_context("text", "ana", "max2", &id, KA, KB).is_none());
        assert!(share_context("file", "Ana", "max2", &id, KA, KB).is_none());
        assert!(share_context("file", "ana\nmax", "max2", &id, KA, KB).is_none());
        assert!(share_context("file", "ana", "max2", "../x", KA, KB).is_none());
        assert!(share_context("file", "ana", "max2", &id, KA, "x").is_none());
    }

    #[test]
    fn checks_a_sealed_offer_and_clip_again() {
        let sealed = "Q".repeat(64);
        let good = json!({"id": "ab".repeat(16), "epk": KA, "sealed": sealed, "bytes": 21, "expires": 9.0});
        let o = clean_sealed_offer(&good).unwrap();
        assert_eq!((o.bytes, o.expires, o.epk.as_str()), (21, 9.0, KA));
        let with = |k: &str, v: Json| {
            let mut g = good.clone();
            g[k] = v;
            clean_sealed_offer(&g)
        };
        assert!(with("bytes", json!(15)).is_none());
        assert!(with("bytes", json!(encrypted_len(FILE_MAX) + 1)).is_none());
        assert!(with("bytes", json!(-1)).is_none());
        assert!(with("bytes", json!(1.5)).is_none());
        assert!(with("id", json!("../../etc")).is_none());
        assert!(with("epk", json!("short")).is_none());
        assert!(with("sealed", json!("Q".repeat(SEALED_FILE_MAX + 4))).is_none());
        assert!(with("sealed", json!("a+b/".repeat(16))).is_none());
        assert!(with("sealed", json!("Q".repeat(8))).is_none());
        // a plain offer, key and all, isn't one any more
        let plain = json!({"id": "ab".repeat(16), "name": "a.pdf", "size": 5, "bytes": 21,
                           "sha256": "cd".repeat(32), "key": "A".repeat(43)});
        assert!(clean_sealed_offer(&plain).is_none());
        let clip = json!({"id": "cd".repeat(16), "epk": KB, "sealed": "Q".repeat(SEALED_CLIP_MAX)});
        assert!(clean_sealed_clip(&clip).is_some());
        let big =
            json!({"id": "cd".repeat(16), "epk": KB, "sealed": "Q".repeat(SEALED_CLIP_MAX + 4)});
        assert!(clean_sealed_clip(&big).is_none());
        assert!(clean_sealed_clip(&json!("a plain clip")).is_none());
    }

    #[test]
    fn seals_and_rechecks_a_files_facts() {
        let key = "A".repeat(43);
        let sha = "cd".repeat(32);
        let plain = seal_file_facts(
            "../x/notes\u{202e}fdp.exe",
            "Application/PDF; x=1",
            5,
            &sha,
            &key,
        )
        .unwrap();
        let f = open_file_facts(&plain, 21).unwrap();
        assert_eq!(
            (f.name.as_str(), f.kind.as_str(), f.size),
            ("notes_fdp.exe", "application/pdf", 5)
        );
        // the size the relay holds has to match the sealed size exactly
        assert!(open_file_facts(&plain, 22).is_none());
        assert!(seal_file_facts("a", "x", FILE_MAX + 1, &sha, &key).is_none());
        assert!(seal_file_facts("a", "x", 5, "nope", &key).is_none());
        let with = |k: &str, v: Json| {
            let mut o: Json = serde_json::from_str(&plain).unwrap();
            o[k] = v;
            open_file_facts(&o.to_string(), 21)
        };
        assert!(with("v", json!(2)).is_none());
        assert!(with("size", json!(FILE_MAX + 1)).is_none());
        assert!(with("key", json!("B".repeat(42))).is_none());
        assert_eq!(with("name", json!(".hidden/..")).unwrap().name, "file");
        assert_eq!(
            with("type", json!("text/html<x>")).unwrap().kind,
            "application/octet-stream"
        );
        assert!(open_file_facts("not json", 21).is_none());
    }

    #[test]
    fn pins_a_key_on_first_use() {
        let pins = json!({"max2": KA});
        assert_eq!(pin_state(&pins, "max2", KA), "same");
        assert_eq!(pin_state(&pins, "max2", KB), "changed");
        assert_eq!(pin_state(&pins, "ana", KB), "new");
        assert_eq!(pin_state(&pins, "ana", "x"), "bad");
        assert_eq!(pin_state(&json!(null), "ana", KB), "new");
    }

    #[test]
    fn tells_users_plainly() {
        assert!(
            privacy_notice().starts_with("files and clipboard shares are end-to-end encrypted")
        );
        assert!(privacy_notice().contains("within 24 hours"));
        assert_eq!(privacy_notice(), privacy_notice().to_lowercase());
        let settings = privacy_settings_text();
        assert!(settings[0].starts_with(
            "Files and clipboard shares are end-to-end encrypted. Q Calc’s relay can’t read them; it only sees who sent to whom, the size and when."
        ));
        assert!(settings[1].contains("safety code"));
        assert!(settings[2].contains("Text messages aren’t end-to-end encrypted"));
        assert_eq!(
            lock_tooltip("max2"),
            "end-to-end encrypted · only you and max2 can open it"
        );
        assert_eq!(key_changed_text("max2"), "max2’s security key changed");
        assert_eq!(
            share_status_text("key-changed", "max2", "file", "a"),
            "max2’s security key changed · not sent"
        );
        assert_eq!(
            file_error_text("key-changed", "max2"),
            "max2’s security key changed"
        );
        assert_eq!(
            share_status_text("no-key", "max2", "clip", ""),
            "max2 needs a newer q calc to get files · not sent"
        );
        assert_eq!(
            safety_code_text("max2", "1 2"),
            "safety code with max2: 1 2"
        );
        assert!(safety_code_text("max2", "").starts_with("no safety code"));
    }

    #[test]
    fn sends_a_drop_to_the_name_on_the_line() {
        let m = me();
        let drop = |line: &str, found: Option<bool>, n: u32| drop_target(line, &m, true, found, n);
        let to = |name: &str, text: &str, hint: &str| DropTarget {
            name: name.into(),
            text: text.into(),
            hint: hint.into(),
        };
        assert_eq!(
            drop("message max2", None, 1),
            to("max2", "", "drop to send to max2")
        );
        assert_eq!(
            drop("message Max2 here you go ", Some(true), 1),
            to("max2", "here you go", "drop to send to max2")
        );
        assert_eq!(
            drop("message max2 hi", None, 3),
            to("max2", "hi", "drop to send 3 files to max2")
        );
        assert_eq!(
            drop("send file to max2", None, 1),
            to("max2", "", "drop to send to max2")
        );
        assert_eq!(
            drop("message", None, 1),
            to("", "", "message <name> first, then drop to send it")
        );
        assert_eq!(
            drop("message x", None, 1),
            to("", "", "a name is 2–20 letters, digits, - or _")
        );
        assert_eq!(
            drop("message zed", Some(false), 1),
            to("", "", "username not found")
        );
        assert_eq!(
            drop("2+2", None, 1).hint,
            "clear the line to send files · or drop them on a thread"
        );
        assert!(drop("2+2", None, 1).name.is_empty());
        assert_eq!(drop("", None, 1), DropTarget::default());
        let off = drop_target("message max2", &Messaging::default(), true, None, 1);
        assert_eq!(off.hint, "set your name in settings to send files");
        assert!(off.name.is_empty());
        let no_relay = drop_target("message max2", &m, false, None, 1);
        assert_eq!(no_relay.hint, "messages need the online relay");
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
