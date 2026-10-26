//! Anonymous usage counts (`src/lib/analytics.ts`): what kind of thing was done, never what was typed. Counts
//! pile up per day in a stash the host keeps and go out at most every six hours in one small post. The host
//! makes the install id (`newInstallId`, 16 random bytes as hex) and does the sending; the rules are here.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json};

/// The deployed worker's `/e` endpoint; empty keeps it all off.
pub const ANALYTICS_URL: &str = "https://qcalc-analytics.maxconine.workers.dev/e";

/// The only names that ever leave the app; anything else is dropped.
pub const USAGE_EVENTS: [&str; 25] = [
    "launch",
    "open",
    "update",
    "calc.arith",
    "calc.units",
    "calc.solve",
    "calc.system",
    "calc.graph",
    "calc.function",
    "calc.assign",
    "calc.calculus",
    "calc.matrix",
    "calc.words",
    "calc.chain",
    "copy.answer",
    "copy.line",
    "history.insert",
    "history.undo",
    "form.tab",
    "prefix.step",
    "periodic",
    "help",
    "settings.open",
    "error.blank",
    "error.unit",
];

const SEND_EVERY_MS: f64 = 6.0 * 60.0 * 60.0 * 1000.0;
const KEEP_DAYS: usize = 30;

pub type DayCounts = BTreeMap<String, u64>;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stash {
    pub id: String,
    pub version: String,
    pub sent_at: f64,
    pub days: BTreeMap<String, DayCounts>,
}

/// `YYYY-MM-DD` of `at_ms` in a zone `offset_minutes` east of UTC (`localDay`; the host knows its zone).
pub fn local_day(at_ms: f64, offset_minutes: f64) -> String {
    let days = ((at_ms + offset_minutes * 60_000.0) / 86_400_000.0).floor() as i64;
    // civil from days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

fn is_day(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
}

fn is_id(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

fn trim_days(mut days: BTreeMap<String, DayCounts>) -> BTreeMap<String, DayCounts> {
    while days.len() > KEEP_DAYS {
        days.pop_first();
    }
    days
}

/// What the host kept, trusted only field by field (`sanitizeStash`).
pub fn sanitize_stash(raw: &Json) -> Option<Stash> {
    let r = raw.as_object()?;
    let id = r.get("id")?.as_str().filter(|id| is_id(id))?;
    let mut days = BTreeMap::new();
    let empty = Map::new();
    for (day, counts) in r.get("days").and_then(Json::as_object).unwrap_or(&empty) {
        let Some(counts) = counts.as_object().filter(|_| is_day(day)) else {
            continue;
        };
        let clean: DayCounts = counts
            .iter()
            .filter(|(name, _)| USAGE_EVENTS.contains(&name.as_str()))
            .filter_map(|(name, n)| {
                let n = n.as_f64()?;
                (n.fract() == 0.0 && n > 0.0 && n <= 9_007_199_254_740_991.0)
                    .then(|| (name.clone(), n as u64))
            })
            .collect();
        if !clean.is_empty() {
            days.insert(day.clone(), clean);
        }
    }
    Some(Stash {
        id: id.to_string(),
        version: r
            .get("version")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string(),
        sent_at: r
            .get("sentAt")
            .and_then(Json::as_f64)
            .filter(|n| n.is_finite())
            .unwrap_or(0.0),
        days: trim_days(days),
    })
}

pub fn bump(stash: &Stash, event: &str, day: &str) -> Stash {
    let mut days = stash.days.clone();
    *days
        .entry(day.to_string())
        .or_default()
        .entry(event.to_string())
        .or_default() += 1;
    Stash {
        days: trim_days(days),
        ..stash.clone()
    }
}

pub fn due_to_send(stash: &Stash, now: f64) -> bool {
    !stash.days.is_empty() && now - stash.sent_at >= SEND_EVERY_MS
}

/// The server replaces a day's counts, so today stays until it is over and every past day was sent final.
pub fn after_send(stash: &Stash, sent: &[String], today: &str, now: f64) -> Stash {
    let mut days = stash.days.clone();
    for day in sent {
        if day.as_str() < today {
            days.remove(day);
        }
    }
    Stash {
        days,
        sent_at: now,
        ..stash.clone()
    }
}

/// The post's body: the id, platform, version, OS and the counts, never anything typed.
pub fn usage_payload(stash: &Stash, platform: &str, os: Option<&str>) -> Json {
    let mut out = Map::new();
    out.insert("id".into(), stash.id.clone().into());
    out.insert("platform".into(), platform.into());
    out.insert("version".into(), stash.version.clone().into());
    if let Some(os) = os.filter(|o| !o.is_empty()) {
        out.insert("os".into(), os.into());
    }
    out.insert(
        "days".into(),
        serde_json::to_value(&stash.days).unwrap_or_default(),
    );
    Json::Object(out)
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct CalcFacts {
    pub expr: String,
    pub system: bool,
    pub graph: bool,
    #[serde(rename = "fn")]
    pub function: bool,
    pub solve: bool,
    pub assign: bool,
    pub unit: bool,
    pub chained: bool,
    pub native: bool,
}

/// One bucket for a saved calculation (`calcKind`).
pub fn calc_kind(f: &CalcFacts) -> &'static str {
    if f.system {
        return "calc.system";
    }
    if f.graph {
        return "calc.graph";
    }
    if f.function {
        return "calc.function";
    }
    if f.solve {
        return "calc.solve";
    }
    if f.assign {
        return "calc.assign";
    }
    let expr = crate::app::js::trim(&f.expr);
    if crate::calculus::is_calculus_input(expr) {
        return "calc.calculus";
    }
    if crate::matrix::looks_like_matrix(expr, &[]) {
        return "calc.matrix";
    }
    if f.unit {
        return "calc.units";
    }
    if f.native {
        return "calc.words";
    }
    if f.chained {
        return "calc.chain";
    }
    "calc.arith"
}

/// The `lib/analytics` arms of the dispatch.
pub fn golden_dispatch(name: &str, args: &[Json]) -> Option<Json> {
    let a = |i: usize| args.get(i).cloned().unwrap_or(Json::Null);
    let s = |i: usize| a(i).as_str().unwrap_or("").to_string();
    let n = |i: usize| a(i).as_f64().unwrap_or(0.0);
    let stash = |i: usize| serde_json::from_value::<Stash>(a(i)).ok();
    let to = |v: &Stash| serde_json::to_value(v).unwrap_or(Json::Null);
    Some(match name {
        "ANALYTICS_URL" => ANALYTICS_URL.into(),
        "USAGE_EVENTS" => serde_json::to_value(USAGE_EVENTS).ok()?,
        "localDay" => local_day(n(0), n(1)).into(),
        "sanitizeStash" => sanitize_stash(&a(0)).map_or(Json::Null, |s| to(&s)),
        "bump" => to(&bump(&stash(0)?, &s(1), &s(2))),
        "dueToSend" => due_to_send(&stash(0)?, n(1)).into(),
        "afterSend" => {
            let sent: Vec<String> = a(1)
                .as_object()
                .map(|m| m.keys().cloned().collect())
                .unwrap_or_default();
            to(&after_send(&stash(0)?, &sent, &s(2), n(3)))
        }
        "usagePayload" => usage_payload(&stash(0)?, &s(1), a(2).as_str()),
        "calcKind" => calc_kind(&serde_json::from_value(a(0)).ok()?).into(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn empty() -> Stash {
        Stash {
            id: ID.into(),
            version: "2.0.3".into(),
            ..Stash::default()
        }
    }

    #[test]
    fn reads_local_days() {
        // 2026-01-05 23:59 in a zone 8 h west of UTC is 2026-01-06 07:59 UTC
        let at = 1_767_686_340_000.0;
        assert_eq!(local_day(at, -480.0), "2026-01-05");
        assert_eq!(local_day(at, 0.0), "2026-01-06");
        assert_eq!(local_day(0.0, 0.0), "1970-01-01");
    }

    #[test]
    fn keeps_only_known_events_whole_counts_and_real_days() {
        let s = sanitize_stash(&json!({
            "id": ID, "version": "2.0.3", "sentAt": 5,
            "days": { "2026-09-29": { "open": 3, "x = 1+1": 1, "calc.arith": 1.5, "copy": 2 }, "nope": { "open": 1 } }
        }))
        .unwrap();
        assert_eq!(
            to_json(&s),
            json!({ "id": ID, "version": "2.0.3", "sentAt": 5.0, "days": { "2026-09-29": { "open": 3 } } })
        );
        assert!(sanitize_stash(&json!({ "id": "short" })).is_none());
        assert!(sanitize_stash(&Json::Null).is_none());
    }

    fn to_json(s: &Stash) -> Json {
        serde_json::to_value(s).unwrap()
    }

    #[test]
    fn counts_per_day_and_keeps_a_month() {
        let mut s = empty();
        for d in 1..=40 {
            s = bump(&s, "open", &format!("2026-08-{d:02}"));
        }
        s = bump(&s, "open", "2026-08-40");
        assert_eq!(s.days.len(), 30);
        assert_eq!(s.days["2026-08-40"]["open"], 2);
    }

    #[test]
    fn sends_every_six_hours_when_there_is_something() {
        let h = 3_600_000.0;
        assert!(!due_to_send(&empty(), 10.0 * h));
        let s = bump(&empty(), "launch", "2026-09-29");
        assert!(due_to_send(&s, 10.0 * h));
        let sent = Stash {
            sent_at: 5.0 * h,
            ..s
        };
        assert!(!due_to_send(&sent, 10.0 * h));
    }

    #[test]
    fn drops_sent_past_days_but_keeps_today() {
        let mut s = bump(&bump(&empty(), "open", "2026-09-28"), "open", "2026-09-29");
        let sent: Vec<String> = s.days.keys().cloned().collect();
        s = bump(&s, "copy.line", "2026-09-29");
        let after = after_send(&s, &sent, "2026-09-29", 99.0);
        assert_eq!(after.sent_at, 99.0);
        assert_eq!(
            serde_json::to_value(&after.days).unwrap(),
            json!({ "2026-09-29": { "open": 1, "copy.line": 1 } })
        );
    }

    #[test]
    fn never_carries_anything_typed() {
        let body = usage_payload(
            &bump(&empty(), "calc.arith", "2026-09-29"),
            "mac",
            Some("macOS 15.5"),
        );
        let mut keys: Vec<&String> = body.as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(keys, ["days", "id", "os", "platform", "version"]);
        assert_eq!(body["days"], json!({ "2026-09-29": { "calc.arith": 1 } }));
    }

    #[test]
    fn sorts_a_calculation_into_one_bucket() {
        let kind = |v: Json| calc_kind(&serde_json::from_value(v).unwrap());
        assert_eq!(kind(json!({ "expr": "2+2" })), "calc.arith");
        assert_eq!(
            kind(json!({ "expr": "sys", "system": true })),
            "calc.system"
        );
        assert_eq!(
            kind(json!({ "expr": "graph sin(x)", "graph": true })),
            "calc.graph"
        );
        assert_eq!(
            kind(json!({ "expr": "f(x) = x^2", "fn": true })),
            "calc.function"
        );
        assert_eq!(
            kind(json!({ "expr": "x^2 = 4", "solve": true })),
            "calc.solve"
        );
        assert_eq!(
            kind(json!({ "expr": "a = 3", "assign": true })),
            "calc.assign"
        );
        assert_eq!(kind(json!({ "expr": "d/dx x^2" })), "calc.calculus");
        assert_eq!(kind(json!({ "expr": "int(x, x, 0, 1)" })), "calc.calculus");
        assert_eq!(kind(json!({ "expr": "det([[1,2],[3,4]])" })), "calc.matrix");
        assert_eq!(
            kind(json!({ "expr": "5 km to mi", "unit": true })),
            "calc.units"
        );
        assert_eq!(
            kind(json!({ "expr": "20% of 50", "native": true })),
            "calc.words"
        );
        assert_eq!(kind(json!({ "expr": "*2", "chained": true })), "calc.chain");
    }
}
