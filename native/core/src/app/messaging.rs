//! Messages between Q Calc users (`src/lib/messaging.ts`), without the network: names, the accepted and blocked
//! lists, the bar's `message <name> <text>` line, its hints and send statuses, and cleaning text from the relay.
//! The relay transport stays in the shells (`Messages.swift` on the Mac).

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
    Open { to: String, name: String },
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
    static TO: Lazy = Lazy::new(|| {
        js(
            r"^\s*(open\s+)?messages\s+(with\s+)?([a-z0-9_-]+)$",
            "i",
        )
    });
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
        assert_eq!(parse_messages_command("messages"), Some(ThreadCommand::List));
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
        assert_eq!(parse_messages_command("open messages max"), open("max", "max"));
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
