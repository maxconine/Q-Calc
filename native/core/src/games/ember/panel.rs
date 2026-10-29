//! What the ember & frost panel decided beside the drawing (`src/components/EmberPanel.tsx`): from each frame's
//! view, the hud (level, stage, checkpoint, gems, keys, deaths, the clock against the target), the one line over
//! the picture (a hint, a checkpoint, the way back after a death), the win card and its "new" marks, and the
//! progress file it keeps. Also the select grid's arrow keys. The Mac panel lays these out.

use super::engine::{score_of, stage_of};
use super::progress::{EmberProgress, Improved, improved, record_result};
use super::session::{EmberView, Screen};
use super::types::*;
use super::{Catalog, fmt_par, fmt_time};

/// the select grid: this many cards a row
pub const GRID_COLS: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerKind {
    Hint,
    Flag,
    Back,
    Ouch,
}

/// one line over the top of the picture at a time; the newest wins. each new one gets a new id, which replays
/// its animation
#[derive(Clone, Debug, PartialEq)]
pub struct Banner {
    pub id: u64,
    pub kind: BannerKind,
    pub title: Option<String>,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hud {
    pub screen: Screen,
    pub level: usize,
    pub me: Option<PlayerIndex>,
    pub note: Option<String>,
    pub phase: Option<Phase>,
    pub deaths: i64,
    pub stage: usize,
    pub stages: usize,
    pub cp: i64,
    pub cps: usize,
    pub gems: [i64; 2],
    pub total: [i64; 2],
    pub keys: i64,
    /// the clock has passed the target time
    pub over: bool,
    pub banner: Option<Banner>,
    /// the banner shows now (a death's line stays as long as the death; the others time themselves out)
    pub show_banner: bool,
    /// bumps each time a level begins, to replay the hints
    pub entry: u64,
    pub score: Option<Score>,
    pub fresh: Option<Improved>,
    /// the running clock, 0:07.4
    pub clock: String,
}

fn ouch(cause: DeathCause, p: PlayerIndex) -> &'static str {
    match (cause, p) {
        (DeathCause::Lava, 0) => "ember tripped in lava? odd",
        (DeathCause::Lava, _) => "frost melted",
        (DeathCause::Water, 0) => "ember fizzled out",
        (DeathCause::Water, _) => "frost took a swim",
        (DeathCause::Goo, 0) => "ember got gooed",
        (DeathCause::Goo, _) => "frost got gooed",
        (DeathCause::Crush, 0) => "ember got walled in",
        (DeathCause::Crush, _) => "frost got walled in",
    }
}

/// where a death sends the pair, said two ways: while they're gone, and once they're back
pub fn back_to(s: &GameState, arrived: bool) -> String {
    let at = if arrived { "back at" } else { "back to" };
    if s.cp >= 0 {
        return format!("{at} checkpoint {}", s.cp + 1);
    }
    if s.stage > 0 {
        format!("{at} the start of the stage")
    } else {
        format!("{at} the start")
    }
}

pub fn stage_title(level: &Level, index: usize) -> Option<String> {
    if level.stages.len() < 2 {
        return None;
    }
    let name = level.stages.get(index).and_then(|s| s.def.name.clone());
    Some(format!(
        "stage {} of {}{}",
        index + 1,
        level.stages.len(),
        name.map_or(String::new(), |n| format!(" · {n}"))
    ))
}

/// a level hint about who uses which keys is wrong online, where both sets steer your own character
fn controls_hint(text: &str) -> bool {
    let t = text.to_lowercase();
    t.contains("wasd") || t.contains("arrows")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Seen {
    tick: i64,
    stage: usize,
    cp: i64,
    deaths: i64,
    phase: Phase,
}

/// EmberPlay's frame loop, minus the drawing
#[derive(Debug)]
pub struct PanelTracker {
    online: bool,
    entry: u64,
    last_level: Option<usize>,
    last_screen: Option<Screen>,
    score: Option<Score>,
    fresh: Option<Improved>,
    banner: Option<Banner>,
    banners: u64,
    seen: Option<Seen>,
    owed: bool,
    cause: Option<String>,
    pub progress: EmberProgress,
    /// set when this frame changed the progress file (a finish recorded): save it
    pub progress_changed: bool,
    /// the renderer forgets its particles and camera (a level began, or began again)
    pub reset_renderer: bool,
}

impl PanelTracker {
    pub fn new(online: bool, progress: EmberProgress) -> Self {
        Self {
            online,
            entry: 0,
            last_level: None,
            last_screen: None,
            score: None,
            fresh: None,
            banner: None,
            banners: 0,
            seen: None,
            owed: false,
            cause: None,
            progress,
            progress_changed: false,
            reset_renderer: false,
        }
    }

    fn say(&mut self, kind: BannerKind, text: String, title: Option<String>) {
        self.banner = if !text.is_empty() || title.is_some() {
            self.banners += 1;
            Some(Banner {
                id: self.banners,
                kind,
                title,
                text,
            })
        } else {
            None
        };
    }

    /// M: the mute switch, kept in the progress file
    pub fn toggle_mute(&mut self) {
        self.progress.muted = !self.progress.muted;
        self.progress_changed = true;
    }

    pub fn frame(&mut self, cat: &Catalog, v: &EmberView) -> Hud {
        self.reset_renderer = false;
        self.progress_changed = false;
        let mut entered = false;
        if Some(v.screen) != self.last_screen || Some(v.level) != self.last_level {
            entered = v.screen == Screen::Play;
            self.last_screen = Some(v.screen);
            self.last_level = Some(v.level);
            self.seen = None;
            self.banner = None;
        }
        for e in &v.events {
            if e.k == EventKind::Die {
                self.cause = e.cause.map(|c| ouch(c, e.p.unwrap_or(0)).to_string());
            }
        }
        let level = if v.playing { cat.get(v.level) } else { None };
        let mut sc: Option<Score> = None;
        let mut stage_cps = 0;
        let mut state: Option<&GameState> = None;
        if let (Some(level), Some(s)) = (level, v.state.as_ref()) {
            state = Some(s);
            let stage = stage_of(level, s.stage);
            stage_cps = stage.checkpoints.len();
            let score = score_of(level, s);
            sc = Some(score);
            // the clock never goes back inside a level, and nor do the stage or the deaths: it began again
            let again = self.seen.is_some_and(|seen| {
                s.tick + 20 < seen.tick || s.stage < seen.stage || s.deaths < seen.deaths
            });
            match self.seen {
                _ if entered || again || self.seen.is_none() => {
                    self.entry += 1;
                    self.reset_renderer = true;
                    self.score = None;
                    self.fresh = None;
                    self.owed = false;
                    self.cause = None;
                    let hint = match &level.def.hint {
                        Some(h) if !(self.online && controls_hint(h)) => h.clone(),
                        _ => stage.def.hint.clone().unwrap_or_default(),
                    };
                    self.say(BannerKind::Hint, hint, stage_title(level, s.stage));
                }
                Some(seen) if s.stage != seen.stage => {
                    self.owed = false;
                    self.say(
                        BannerKind::Hint,
                        stage.def.hint.clone().unwrap_or_default(),
                        stage_title(level, s.stage),
                    );
                }
                Some(seen) => {
                    if s.deaths > seen.deaths {
                        self.owed = true;
                    }
                    if s.phase == Phase::Dead && seen.phase != Phase::Dead {
                        self.owed = true;
                        let title = self.cause.take().unwrap_or_else(|| "ouch".into());
                        self.say(BannerKind::Ouch, back_to(s, false), Some(title));
                    } else if self.owed && s.phase == Phase::Play {
                        self.owed = false;
                        self.say(BannerKind::Back, back_to(s, true), None);
                    } else if s.cp != seen.cp && s.cp >= 0 && s.deaths == seen.deaths {
                        let n = stage.checkpoints.len();
                        let text = if n > 1 {
                            format!("checkpoint {} of {n}", s.cp + 1)
                        } else {
                            "checkpoint".into()
                        };
                        self.say(BannerKind::Flag, text, None);
                    }
                }
                None => {}
            }
            if s.phase == Phase::Won && self.score.is_none() {
                // by the state, not the win event: the guest's copy of the event can be late or lost
                self.score = Some(score);
                let id = &level.def.id;
                self.fresh = Some(improved(
                    self.progress.levels.get(id),
                    &score,
                    Some(s.deaths as f64),
                ));
                self.progress = record_result(&self.progress, id, &score, Some(s.deaths as f64));
                self.progress_changed = true;
                self.banner = None;
            }
            self.seen = Some(Seen {
                tick: s.tick,
                stage: s.stage,
                cp: s.cp,
                deaths: s.deaths,
                phase: s.phase,
            });
        }
        let over = match (state, level) {
            (Some(s), Some(l)) => s.tick as f64 / STEP_HZ > l.def.par,
            _ => false,
        };
        let phase = state.map(|s| s.phase);
        let show_banner = match &self.banner {
            Some(b) if b.kind == BannerKind::Ouch => phase == Some(Phase::Dead),
            Some(_) => matches!(phase, Some(Phase::Play | Phase::Dead)),
            None => false,
        };
        Hud {
            screen: v.screen,
            level: v.level,
            me: v.me,
            note: v.note.clone(),
            phase,
            deaths: state.map_or(0, |s| s.deaths),
            stage: state.map_or(0, |s| s.stage),
            stages: level.map_or(1, |l| l.stages.len()),
            cp: state.map_or(-1, |s| s.cp),
            cps: stage_cps,
            gems: sc.map_or([0, 0], |s| s.gems),
            total: sc.map_or([0, 0], |s| s.total),
            keys: state.map_or(0, |s| s.keys),
            over,
            banner: self.banner.clone(),
            show_banner,
            entry: self.entry,
            score: self.score,
            fresh: self.fresh,
            clock: fmt_time(state.map_or(0.0, |s| s.tick as f64 / STEP_HZ)),
        }
    }
}

/// the select grid's arrows: sideways runs on through the rows; down from above a short last row lands on its
/// last card; anything else that would leave the grid stays put
pub fn grid_step(from: i64, dx: i64, dy: i64, count: i64, cols: i64) -> i64 {
    if count < 1 {
        return 0;
    }
    if dx != 0 {
        return (count - 1).min((from + dx).max(0));
    }
    let to = from + dy * cols;
    if to < 0 {
        return from;
    }
    if to < count {
        return to;
    }
    if from.div_euclid(cols) < (count - 1).div_euclid(cols) {
        count - 1
    } else {
        from
    }
}

/// the win card's lines
pub fn win_lines(
    score: &Score,
    par: f64,
    deaths: i64,
    fresh: Option<&Improved>,
    last: bool,
) -> (String, Option<String>, String) {
    let got = score.gems[0] + score.gems[1];
    let all = score.total[0] + score.total[1];
    let mut line = format!("{} / {}", fmt_time(score.secs), fmt_par(par));
    if all > 0 {
        line.push_str(&format!(" · gems {got}/{all}"));
    }
    line.push_str(&format!(" · {}", deaths_text(deaths)));
    let news: Vec<&str> = fresh.map_or(vec![], |f| {
        [
            (f.secs, "best time"),
            (f.gems, "most gems"),
            (f.deaths, "fewest deaths"),
        ]
        .iter()
        .filter(|(on, _)| *on)
        .map(|(_, t)| *t)
        .collect()
    });
    let new = if news.is_empty() {
        None
    } else {
        Some(format!("new: {}", news.join(" · ")))
    };
    let foot = if last {
        "that was the last level · R again · esc levels"
    } else {
        "↵ next level · R again · esc levels"
    };
    (line, new, foot.into())
}

pub fn deaths_text(d: i64) -> String {
    match d {
        0 => "no deaths".into(),
        1 => "1 death".into(),
        n => format!("{n} deaths"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_arrows() {
        // 7 levels in rows of 5
        assert_eq!(grid_step(0, 1, 0, 7, 5), 1);
        assert_eq!(grid_step(4, 1, 0, 7, 5), 5);
        assert_eq!(grid_step(6, 1, 0, 7, 5), 6);
        assert_eq!(grid_step(3, 0, 1, 7, 5), 6);
        assert_eq!(grid_step(1, 0, 1, 7, 5), 6);
        assert_eq!(grid_step(6, 0, 1, 7, 5), 6);
        assert_eq!(grid_step(2, 0, -1, 7, 5), 2);
        assert_eq!(grid_step(0, -7, 0, 7, 5), 0);
        assert_eq!(grid_step(0, 7, 0, 7, 5), 6);
    }

    #[test]
    fn win_card_says_what_was_beaten() {
        let s = Score {
            secs: 41.0,
            gems: [2, 1],
            total: [2, 2],
            stars: 2,
        };
        let f = Improved {
            secs: true,
            stars: false,
            gems: false,
            deaths: true,
        };
        let (line, new, foot) = win_lines(&s, 45.0, 1, Some(&f), false);
        assert_eq!(line, "0:41.0 / 0:45 · gems 3/4 · 1 death");
        assert_eq!(new.as_deref(), Some("new: best time · fewest deaths"));
        assert_eq!(foot, "↵ next level · R again · esc levels");
    }
}
