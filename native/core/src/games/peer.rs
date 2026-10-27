//! `src/lib/peer.ts`: a link to one other Q Calc through the relay (`peer_online`). The page sees only these
//! events; games and anything else sent between bars sit on top. (The TS also has a dev link between browser
//! tabs, `peerDev.ts`; the native app always goes through the relay.)

use serde_json::Value as Json;

use super::nickname::clean_nickname;

/// A room in a lobby's list: who's hosting, and whether it takes a code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomInfo {
    pub id: String,
    pub name: String,
    pub locked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseReason {
    WrongCode,
    Busy,
    Unreachable,
    Lost,
    Bye,
    Network,
    TooManyTries,
    Expired,
    Gone,
}

impl CloseReason {
    pub fn parse(s: &str) -> Option<CloseReason> {
        Some(match s {
            "wrong-code" => CloseReason::WrongCode,
            "busy" => CloseReason::Busy,
            "unreachable" => CloseReason::Unreachable,
            "lost" => CloseReason::Lost,
            "bye" => CloseReason::Bye,
            "network" => CloseReason::Network,
            "too-many-tries" => CloseReason::TooManyTries,
            "expired" => CloseReason::Expired,
            "gone" => CloseReason::Gone,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Who {
    Peer,
    SelfSide,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PeerEvent {
    /// offline: the list couldn't be reached. code_only: it never could, so the relay may be one from before room
    /// lists; hosting and joining then go by code alone, which every relay understands.
    Rooms {
        rooms: Vec<RoomInfo>,
        offline: bool,
        code_only: bool,
    },
    /// code is "" for an open room.
    Hosting {
        code: String,
        renewed: bool,
    },
    Pairing,
    /// room and token: the seat this side can take back if its link drops; resumed: it just did.
    Open {
        role: super::Side,
        peer: String,
        seat: Option<(String, String)>,
        resumed: bool,
    },
    /// Mid-game, a side's link dropped and its seat is held; Back is that side returning to the same game.
    Away {
        who: Who,
    },
    Back {
        who: Who,
    },
    Message {
        data: String,
    },
    Closed {
        reason: CloseReason,
    },
}

const MAX_ROOMS: usize = 100;

pub fn is_pairing_code(s: &str, length: usize) -> bool {
    s.len() == length && s.bytes().all(|b| b.is_ascii_digit())
}

fn room_of(raw: &Json, locked_default: bool) -> Option<RoomInfo> {
    let o = raw.as_object()?;
    let id = o.get("id")?.as_str()?;
    if id.is_empty() || id.encode_utf16().count() > 64 {
        return None;
    }
    let name = clean_nickname(o.get("name").and_then(Json::as_str).unwrap_or(""));
    Some(RoomInfo {
        id: id.to_string(),
        name: if name.is_empty() {
            "friend".into()
        } else {
            name
        },
        locked: o
            .get("locked")
            .and_then(Json::as_bool)
            .unwrap_or(locked_default),
    })
}

fn is_room_id(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

fn is_token(s: &str) -> bool {
    s.len() == 32
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Events come from the relay but are still checked; a bad one is dropped.
pub fn parse_peer_event(raw: &Json, code_length: usize) -> Option<PeerEvent> {
    let e = raw.as_object()?;
    match e.get("type")?.as_str()? {
        "rooms" => {
            let rooms = e.get("rooms")?.as_array()?;
            let rooms = rooms
                .iter()
                .take(MAX_ROOMS)
                .filter_map(|r| room_of(r, true))
                .collect();
            Some(PeerEvent::Rooms {
                rooms,
                offline: false,
                code_only: false,
            })
        }
        "hosting" => {
            let code = match e.get("code") {
                None | Some(Json::Null) => "",
                Some(c) => c.as_str()?,
            };
            if !code.is_empty() && !is_pairing_code(code, code_length) {
                return None;
            }
            Some(PeerEvent::Hosting {
                code: code.into(),
                renewed: e.get("renewed") == Some(&Json::Bool(true)),
            })
        }
        "pairing" => Some(PeerEvent::Pairing),
        "open" => {
            let role = match e.get("role").and_then(Json::as_str) {
                Some("host") => super::Side::Host,
                Some("guest") => super::Side::Guest,
                _ => return None,
            };
            let peer = clean_nickname(e.get("peer").and_then(Json::as_str).unwrap_or(""));
            let room = e
                .get("room")
                .and_then(Json::as_str)
                .filter(|r| is_room_id(r));
            let token = e
                .get("token")
                .and_then(Json::as_str)
                .filter(|t| is_token(t));
            let seat = room.zip(token).map(|(r, t)| (r.to_string(), t.to_string()));
            Some(PeerEvent::Open {
                role,
                peer,
                seat,
                resumed: e.get("resumed") == Some(&Json::Bool(true)),
            })
        }
        // from the relay these are always about the other side
        "away" => Some(PeerEvent::Away { who: Who::Peer }),
        "back" => Some(PeerEvent::Back { who: Who::Peer }),
        "message" => Some(PeerEvent::Message {
            data: e.get("data")?.as_str()?.to_string(),
        }),
        "closed" => Some(PeerEvent::Closed {
            reason: e
                .get("reason")
                .and_then(Json::as_str)
                .and_then(CloseReason::parse)
                .unwrap_or(CloseReason::Lost),
        }),
        _ => None,
    }
}

/// `word` is what the other end is called: a computer online.
pub fn close_reason_text(reason: CloseReason, word: &str) -> String {
    match reason {
        CloseReason::WrongCode => "wrong code · try again".into(),
        CloseReason::Busy => "someone else just joined that room".into(),
        CloseReason::Gone => "that room has closed".into(),
        CloseReason::Unreachable => format!("couldn’t reach that {word}"),
        CloseReason::Lost => "lost the connection".into(),
        CloseReason::Bye => "your friend left".into(),
        CloseReason::Network => "couldn’t reach the game server · check your internet".into(),
        CloseReason::TooManyTries => "too many tries · wait a minute".into(),
        CloseReason::Expired => "nobody joined for 15 minutes · host again".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(v: Json) -> Option<PeerEvent> {
        parse_peer_event(&v, 4)
    }

    fn room(id: &str, name: &str, locked: bool) -> RoomInfo {
        RoomInfo {
            id: id.into(),
            name: name.into(),
            locked,
        }
    }

    #[test]
    fn pairing_codes_are_that_many_ascii_digits() {
        assert!(is_pairing_code("0420", 4));
        assert!(!is_pairing_code("420", 4));
        assert!(!is_pairing_code("04201", 4));
        assert!(!is_pairing_code("04a0", 4));
        assert!(!is_pairing_code("٠٤٢٠", 4));
    }

    #[test]
    fn reads_each_event_the_relay_sends() {
        assert_eq!(
            parse(
                json!({"type": "rooms", "rooms": [{"id": "a", "name": "teal otter"}, {"id": 1}, null]})
            ),
            Some(PeerEvent::Rooms {
                rooms: vec![room("a", "teal otter", true)],
                offline: false,
                code_only: false
            })
        );
        assert_eq!(
            parse(json!({"type": "rooms", "rooms": [{"id": "r", "name": "", "locked": false}]})),
            Some(PeerEvent::Rooms {
                rooms: vec![room("r", "friend", false)],
                offline: false,
                code_only: false
            })
        );
        assert_eq!(
            parse(json!({"type": "hosting", "code": ""})),
            Some(PeerEvent::Hosting {
                code: "".into(),
                renewed: false
            })
        );
        assert_eq!(
            parse(json!({"type": "hosting", "code": "0007"})),
            Some(PeerEvent::Hosting {
                code: "0007".into(),
                renewed: false
            })
        );
        assert_eq!(
            parse(json!({"type": "hosting", "code": "0007", "renewed": true})),
            Some(PeerEvent::Hosting {
                code: "0007".into(),
                renewed: true
            })
        );
        assert_eq!(
            parse(json!({"type": "open", "role": "guest", "peer": "teal otter"})),
            Some(PeerEvent::Open {
                role: crate::games::Side::Guest,
                peer: "teal otter".into(),
                seat: None,
                resumed: false
            })
        );
        assert_eq!(
            parse(json!({"type": "message", "data": "{}"})),
            Some(PeerEvent::Message { data: "{}".into() })
        );
        assert_eq!(
            parse(json!({"type": "closed", "reason": "wrong-code"})),
            Some(PeerEvent::Closed {
                reason: CloseReason::WrongCode
            })
        );
        assert_eq!(
            parse(json!({"type": "closed", "reason": "weird"})),
            Some(PeerEvent::Closed {
                reason: CloseReason::Lost
            })
        );
        assert_eq!(parse(json!({"type": "pairing"})), Some(PeerEvent::Pairing));
    }

    #[test]
    fn drops_anything_malformed() {
        for raw in [
            json!(null),
            json!("x"),
            json!(3),
            json!({}),
            json!({"type": "hosting", "code": "12"}),
            json!({"type": "open", "role": "boss"}),
            json!({"type": "message", "data": 5}),
            json!({"type": "rooms"}),
        ] {
            assert_eq!(parse(raw), None);
        }
    }

    #[test]
    fn has_words_for_every_close() {
        for r in [
            "wrong-code",
            "busy",
            "unreachable",
            "lost",
            "bye",
            "network",
            "gone",
            "expired",
            "too-many-tries",
        ] {
            assert!(!close_reason_text(CloseReason::parse(r).unwrap(), "computer").is_empty());
        }
    }
}
