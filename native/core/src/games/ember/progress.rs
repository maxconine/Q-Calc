//! Ember & frost's saved progress (`src/lib/emberProgress.ts`): the best of every finished level, by level id, and
//! the mute switch. Pure parse/merge; the app's game-save store holds the text under SAVE_KEY.
//!
//! Version 2: a finish also keeps the fewest deaths, and only the ids in CARRIED bring a version 1 best along.

use std::collections::BTreeMap;

use serde_json::Value;

use super::types::Score;

pub const PROGRESS_VERSION: i64 = 2;
/// the web build's localStorage key, which the game-save store keeps
pub const SAVE_KEY: &str = "qcalc-ember";

/// levels whose map (and so whose bests) version 1 and version 2 share
const CARRIED: [&str; 1] = ["first-light"];
const MAX_STARS: f64 = 3.0;
const MAX_LEVELS: usize = 200;
const MAX_ID: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub struct LevelBest {
    pub completed: bool,
    /// best stars, 0..3
    pub stars: i64,
    /// best time over all stages; None until completed
    pub secs: Option<f64>,
    /// most gems (both colours, all stages) in one finish
    pub gems: i64,
    /// fewest deaths in one finish; None when not known
    pub deaths: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmberProgress {
    pub v: i64,
    /// by level id. the TS keeps insertion order; this keeps id order, which JSON readers don't mind
    pub levels: BTreeMap<String, LevelBest>,
    pub muted: bool,
}

pub fn empty_progress() -> EmberProgress {
    EmberProgress {
        v: PROGRESS_VERSION,
        levels: BTreeMap::new(),
        muted: false,
    }
}

fn count(v: Option<&Value>, max: f64) -> i64 {
    match v.and_then(Value::as_f64) {
        Some(n) if n.is_finite() && n > 0.0 => n.floor().min(max) as i64,
        _ => 0,
    }
}

fn countf(v: f64, max: f64) -> i64 {
    if v.is_finite() && v > 0.0 {
        v.floor().min(max) as i64
    } else {
        0
    }
}

fn tally(v: Option<f64>) -> Option<i64> {
    v.filter(|n| n.is_finite() && *n >= 0.0)
        .map(|n| n.floor() as i64)
}

/// whatever was stored, made safe and brought up to this version
pub fn sanitize_progress(raw: &Value) -> EmberProgress {
    let mut out = empty_progress();
    let Some(o) = raw.as_object() else { return out };
    out.muted = o.get("muted") == Some(&Value::Bool(true));
    // version 1 had no version field
    let from = match o.get("v").and_then(Value::as_f64) {
        Some(v) if v.is_finite() && v >= 1.0 => v.floor() as i64,
        _ => 1,
    };
    let Some(levels) = o.get("levels").and_then(Value::as_object) else {
        return out;
    };
    let mut n = 0;
    for (id, v) in levels {
        let Some(l) = v.as_object() else { continue };
        if id.is_empty() || crate::app::js::len16(id) > MAX_ID || id == "__proto__" {
            continue;
        }
        if from < 2 && !CARRIED.contains(&id.as_str()) {
            continue;
        }
        if l.get("completed") != Some(&Value::Bool(true)) {
            continue;
        }
        n += 1;
        if n > MAX_LEVELS {
            break;
        }
        let secs = l
            .get("secs")
            .and_then(Value::as_f64)
            .filter(|s| s.is_finite() && *s > 0.0);
        let deaths = if from < 2 {
            None
        } else {
            tally(l.get("deaths").and_then(Value::as_f64))
        };
        out.levels.insert(
            id.clone(),
            LevelBest {
                completed: true,
                stars: count(l.get("stars"), MAX_STARS),
                secs,
                gems: count(l.get("gems"), f64::INFINITY),
                deaths,
            },
        );
    }
    out
}

pub fn parse_progress(text: Option<&str>) -> EmberProgress {
    match text
        .filter(|t| !t.is_empty())
        .map(serde_json::from_str::<Value>)
    {
        Some(Ok(v)) => sanitize_progress(&v),
        _ => empty_progress(),
    }
}

fn opt_num(x: Option<f64>) -> String {
    x.map_or("null".into(), crate::jsnum::to_string)
}

/// what saveProgress writes
pub fn progress_json(p: &EmberProgress) -> String {
    let levels: Vec<String> = p
        .levels
        .iter()
        .map(|(id, l)| {
            format!(
                "{}:{{\"completed\":{},\"stars\":{},\"secs\":{},\"gems\":{},\"deaths\":{}}}",
                serde_json::to_string(id).unwrap_or_default(),
                l.completed,
                l.stars,
                opt_num(l.secs),
                l.gems,
                l.deaths.map_or("null".into(), |d| d.to_string())
            )
        })
        .collect();
    format!(
        "{{\"v\":{},\"levels\":{{{}}},\"muted\":{}}}",
        p.v,
        levels.join(","),
        p.muted
    )
}

/// a finished level: keep the best of each field separately. deaths: checkpoints gone back to on the way
pub fn record_result(
    progress: &EmberProgress,
    level_id: &str,
    score: &Score,
    deaths: Option<f64>,
) -> EmberProgress {
    let old = progress.levels.get(level_id);
    let secs = Some(score.secs).filter(|s| s.is_finite() && *s > 0.0);
    let gems =
        countf(score.gems[0] as f64, f64::INFINITY) + countf(score.gems[1] as f64, f64::INFINITY);
    let died = tally(deaths);
    let best = LevelBest {
        completed: true,
        stars: old
            .map_or(0, |o| o.stars)
            .max(countf(score.stars as f64, MAX_STARS)),
        secs: match old.and_then(|o| o.secs) {
            Some(o) if secs.is_none_or(|s| o <= s) => Some(o),
            _ => secs,
        },
        gems: old.map_or(0, |o| o.gems).max(gems),
        deaths: match old.and_then(|o| o.deaths) {
            Some(o) if died.is_none_or(|d| o <= d) => Some(o),
            _ => died,
        },
    };
    let mut levels = progress.levels.clone();
    levels.insert(level_id.into(), best);
    EmberProgress {
        v: PROGRESS_VERSION,
        levels,
        muted: progress.muted,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Improved {
    pub secs: bool,
    pub stars: bool,
    pub gems: bool,
    pub deaths: bool,
}

/// what this finish beat, for the win card. nothing is "new" on a first finish
pub fn improved(old: Option<&LevelBest>, score: &Score, deaths: Option<f64>) -> Improved {
    let Some(old) = old.filter(|o| o.completed) else {
        return Improved::default();
    };
    let died = tally(deaths);
    Improved {
        secs: old.secs.is_some_and(|o| score.secs > 0.0 && score.secs < o),
        stars: countf(score.stars as f64, MAX_STARS) > old.stars,
        gems: countf(score.gems[0] as f64, f64::INFINITY)
            + countf(score.gems[1] as f64, f64::INFINITY)
            > old.gems,
        deaths: matches!((old.deaths, died), (Some(o), Some(d)) if d < o),
    }
}

/// where the select screen's highlight starts: the first level not finished yet, or the first of all
pub fn first_unfinished(progress: &EmberProgress, ids: &[&str]) -> usize {
    ids.iter()
        .position(|id| !progress.levels.get(*id).is_some_and(|l| l.completed))
        .unwrap_or(0)
}

/// how many of the levels that exist now are finished
pub fn finished_count(progress: &EmberProgress, ids: &[&str]) -> usize {
    ids.iter()
        .filter(|id| progress.levels.get(**id).is_some_and(|l| l.completed))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score(secs: f64, gems: [i64; 2], stars: i64) -> Score {
        Score {
            secs,
            gems,
            total: [3, 3],
            stars,
        }
    }

    // emberProgress.test.ts, case for case where it isn't storage
    #[test]
    fn keeps_each_best_on_its_own() {
        let p = record_result(&empty_progress(), "a", &score(40.0, [3, 3], 3), Some(2.0));
        let p = record_result(&p, "a", &score(30.0, [1, 1], 1), Some(5.0));
        let a = &p.levels["a"];
        assert_eq!(
            (a.stars, a.secs, a.gems, a.deaths),
            (3, Some(30.0), 6, Some(2))
        );
        let imp = improved(Some(a), &score(20.0, [3, 3], 3), Some(1.0));
        assert_eq!(
            imp,
            Improved {
                secs: true,
                stars: false,
                gems: false,
                deaths: true
            }
        );
        assert_eq!(
            improved(None, &score(1.0, [9, 9], 3), Some(0.0)),
            Improved::default()
        );
    }

    #[test]
    fn version_one_brings_only_the_carried_map() {
        let p = parse_progress(Some(
            r#"{"levels":{"first-light":{"completed":true,"stars":2,"secs":9,"gems":2,"deaths":0},"x":{"completed":true}}}"#,
        ));
        assert_eq!(p.levels.len(), 1);
        assert_eq!(p.levels["first-light"].deaths, None);
        assert_eq!(parse_progress(Some("{")), empty_progress());
        assert_eq!(parse_progress(None), empty_progress());
        assert_eq!(first_unfinished(&p, &["first-light", "b"]), 1);
        assert_eq!(finished_count(&p, &["first-light", "b"]), 1);
        assert_eq!(
            progress_json(&p),
            r#"{"v":2,"levels":{"first-light":{"completed":true,"stars":2,"secs":9,"gems":2,"deaths":null}},"muted":false}"#
        );
    }
}
