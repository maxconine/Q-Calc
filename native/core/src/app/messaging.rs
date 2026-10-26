//! Messages between Q Calc users (`src/lib/messaging.ts`), without the network: names, the accepted and blocked
//! lists, the bar's `message <name> <text>` line, its hints and send statuses, and cleaning text from the relay.
//! The relay transport stays in the shells (`Messages.swift` on the Mac).

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::app::js as jstr;
use crate::jsre::{self, Lazy, js};

pub const TEXT_MAX: usize = 500;
pub const INBOX_PROTOCOL: &str = "qcalc-inbox";

/// The user's name, its key, and who they accept and block.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Messaging {
    pub name: String,
    pub key: String,
    pub accepted: Vec<String>,
    pub blocked: Vec<String>,
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
    Messaging {
        name: raw.get("name").map(clean_name).unwrap_or_default(),
        key,
        accepted,
        blocked,
    }
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

/// What the bar says while a message is typed. `relay` is whether an online relay is configured.
pub fn message_hint(line: &MessageLine, m: &Messaging, relay: bool) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
}
