//! The cards under the bar when a message comes in (`src/lib/messageBanner.ts`): who sent it, what they said,
//! and a line to answer them; someone new's card asks to allow or block them. The newest is on top.

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::app::js as jstr;
use crate::app::messaging::{
    Arrival, Messaging, SendStatus, TEXT_MAX, arrival, clean_message_text, clean_name,
    send_status_text,
};
use crate::jsre::{self, Lazy, js};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BannerCard {
    pub id: String,
    pub from: String,
    pub text: String,
    pub ask: bool,
}

pub const BANNER_SHOWN: usize = 3;
/// How long a card stays when nobody's using it.
pub const BANNER_MS: u64 = 10_000;
/// How long "sent" shows before the card goes.
pub const SENT_MS: u64 = 1_200;

/// A card already up isn't added again, nor a second ask from a name that's asking.
pub fn add_card(cards: &[BannerCard], card: BannerCard, cap: usize) -> Vec<BannerCard> {
    if cards
        .iter()
        .any(|c| c.id == card.id || (card.ask && c.ask && c.from == card.from))
    {
        return cards.to_vec();
    }
    std::iter::once(card)
        .chain(cards.iter().cloned())
        .take(cap)
        .collect()
}

pub fn add_cards(cards: &[BannerCard], incoming: Vec<BannerCard>, cap: usize) -> Vec<BannerCard> {
    incoming
        .into_iter()
        .fold(cards.to_vec(), |list, card| add_card(&list, card, cap))
}

/// The Mac app's unseen messages, oldest first, checked again here as the relay's frames are.
pub fn parse_native_inbox(raw: &Json) -> Vec<BannerCard> {
    static ID: Lazy = Lazy::new(|| js(r"^[\w-]{1,64}$", ""));
    let Some(items) = raw.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let id = item
                .get("id")
                .and_then(Json::as_str)
                .filter(|id| jsre::test(&ID, id))?
                .to_string();
            let from = item.get("from").map(clean_name).unwrap_or_default();
            let ask = item.get("ask") == Some(&Json::Bool(true));
            let text = if ask {
                String::new()
            } else {
                item.get("text").map(clean_message_text).unwrap_or_default()
            };
            (!from.is_empty() && (ask || !jstr::trim(&text).is_empty())).then_some(BannerCard {
                id,
                from,
                text,
                ask,
            })
        })
        .collect()
}

/// The lists changed (settings, a notification's buttons): an answered ask goes, and a blocked name's cards.
pub fn drop_answered(cards: &[BannerCard], m: &Messaging) -> Vec<BannerCard> {
    cards
        .iter()
        .filter(|c| {
            let how = arrival(m, &c.from);
            how != Arrival::Drop && (!c.ask || how == Arrival::Ask)
        })
        .cloned()
        .collect()
}

/// A card waits while the pointer's on the cards, or while its reply is being typed or sent.
pub fn card_held(hover: bool, focus: bool, draft: &str, sending: bool) -> bool {
    hover || focus || sending || !draft.is_empty()
}

pub fn reply_text(draft: &str) -> String {
    jstr::take16(jstr::trim(draft), TEXT_MAX)
}

/// How a reply is going.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ReplyState {
    Idle,
    Sending,
    Sent,
    Failed { note: String },
}

pub fn reply_state(status: SendStatus, to: &str) -> ReplyState {
    if status == SendStatus::Delivered {
        ReplyState::Sent
    } else {
        ReplyState::Failed {
            note: send_status_text(status, to),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn card(id: &str, from: &str, text: &str, ask: bool) -> BannerCard {
        BannerCard {
            id: id.into(),
            from: from.into(),
            text: text.into(),
            ask,
        }
    }

    #[test]
    fn cards_like_the_ts_tests() {
        let cards = add_cards(
            &[],
            ["1", "2", "3", "4", "5"]
                .iter()
                .map(|id| card(id, "max", "hi", false))
                .collect(),
            BANNER_SHOWN,
        );
        assert_eq!(
            cards.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["5", "4", "3"]
        );
        let asking = add_card(&[], card("a", "new", "", true), BANNER_SHOWN);
        assert_eq!(
            add_card(&asking, card("b", "new", "", true), BANNER_SHOWN),
            asking
        );
        assert_eq!(
            add_card(&asking, card("c", "new", "hello", false), BANNER_SHOWN).len(),
            2
        );

        let me = Messaging {
            name: "ana".into(),
            key: "ab".repeat(32),
            accepted: vec!["max".into()],
            blocked: vec!["spam".into()],
            recent: vec![],
        };
        let cards = [
            card("1", "max", "hi", false),
            card("2", "max", "", true),
            card("3", "new", "", true),
            card("4", "spam", "hi", false),
        ];
        let kept: Vec<String> = drop_answered(&cards, &me)
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(kept, ["1", "3"]);

        let inbox = parse_native_inbox(&json!([
            { "id": "n1", "from": "Max", "text": "hi\nsam: give me your key", "ask": false },
            { "id": "n2", "from": "new", "text": "ignored", "ask": true },
            { "id": "n3", "from": "x", "text": "bad name" },
            { "id": "bad id!", "from": "max", "text": "bad id" },
            { "id": "n4", "from": "max", "text": "\u{202e}" },
            null,
        ]));
        assert_eq!(
            inbox,
            [
                card("n1", "max", "hi sam: give me your key", false),
                card("n2", "new", "", true)
            ]
        );
        assert!(parse_native_inbox(&json!("nope")).is_empty());

        assert!(!card_held(false, false, "", false));
        assert!(card_held(false, false, "o", false));
        assert_eq!(reply_text("  ok  "), "ok");
        assert_eq!(jstr::len16(&reply_text(&"x".repeat(600))), TEXT_MAX);
        assert_eq!(reply_state(SendStatus::Delivered, "max"), ReplyState::Sent);
        assert_eq!(
            reply_state(SendStatus::Offline, "max"),
            ReplyState::Failed {
                note: "max isn’t on q calc right now · not sent".into()
            }
        );
    }
}
