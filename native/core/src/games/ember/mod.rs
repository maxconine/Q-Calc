//! Ember & frost, a two-player co-op puzzle platformer (`src/lib/ember*.ts`): the engine, the shipped levels, the
//! camera, saved progress, the wire, the local and online sessions, the reachability solver the level tests use,
//! and which sounds each event makes. The Mac app draws and plays what these say.

pub mod camera;
pub mod engine;
pub mod json;
pub mod progress;
pub mod protocol;
pub mod session;
pub mod solver;
pub mod sound;
pub mod types;

use std::sync::{Arc, LazyLock};

use crate::app::games::{GameKind, game_command};
pub use types::*;

/// opening ember & frost from the bar
pub const EMBER_HINT: &str = "↵ play ember & frost";

/// 'ember', 'ember & frost', 'fire & ice'… whole names only (app/games.rs holds the pattern)
pub fn is_ember_command(text: &str) -> bool {
    game_command(text) == Some(GameKind::Ember)
}

/// every shipped level, in the order the select screen shows them: easiest first. the data is the TS's own
/// `LEVELS`, written out by native/scripts/games-golden/capture.sh
pub static LEVELS: LazyLock<Vec<LevelDef>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../golden/games/ember-levels.json"))
        .expect("the shipped ember levels parse")
});

/// a list of levels, compiled once: the shipped ones, or (in tests) a session's own
#[derive(Debug)]
pub struct Catalog {
    pub defs: Vec<LevelDef>,
    levels: Vec<Option<Level>>,
}

impl Catalog {
    pub fn new(defs: Vec<LevelDef>) -> Self {
        let levels = defs.iter().map(|d| engine::compile_level(d).ok()).collect();
        Self { defs, levels }
    }
    pub fn len(&self) -> usize {
        self.defs.len()
    }
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
    /// compiledAt: None when the level doesn't exist or doesn't compile
    pub fn get(&self, i: usize) -> Option<&Level> {
        self.levels.get(i).and_then(Option::as_ref)
    }
}

static SHIPPED: LazyLock<Arc<Catalog>> = LazyLock::new(|| Arc::new(Catalog::new(LEVELS.clone())));

/// the shipped levels, compiled
pub fn shipped() -> Arc<Catalog> {
    SHIPPED.clone()
}

/// compiledAt over the shipped levels
pub fn compiled_at(i: usize) -> Option<&'static Level> {
    SHIPPED.get(i)
}

// ——— small sums the panel shows

/// the running clock and best times: 0:07.4, 2:10.0
pub fn fmt_time(secs: f64) -> String {
    // by tenths first, so 59.96 reads 1:00.0 and not 0:60.0
    let tenths = crate::jsnum::round(if secs.is_finite() { secs } else { 0.0 }.max(0.0) * 10.0);
    let m = (tenths / 600.0).floor();
    let rest = (tenths - m * 600.0) / 10.0;
    format!(
        "{}:{}{}",
        m,
        if rest < 10.0 { "0" } else { "" },
        crate::jsnum::to_fixed(rest, 1)
    )
}

/// a target time, which is always whole seconds: 0:45, 2:10
pub fn fmt_par(secs: f64) -> String {
    let s = crate::jsnum::round(if secs.is_finite() { secs } else { 0.0 }.max(0.0));
    let rest = s % 60.0;
    format!(
        "{}:{}{}",
        (s / 60.0).floor(),
        if rest < 10.0 { "0" } else { "" },
        rest
    )
}

pub const MAX_DIFFICULTY: f64 = 5.0;

/// levels run easiest first, so a level's difficulty mark is where it stands in the list
pub fn level_difficulty(index: usize, count: usize) -> u32 {
    if count <= 1 || index == 0 {
        return 1;
    }
    let at = index.min(count - 1) as f64 / (count - 1) as f64;
    1 + crate::jsnum::round(at * (MAX_DIFFICULTY - 1.0)) as u32
}

/// how much map a level has, in screens of 32 × 18 tiles over all its stages, to the nearest half
pub fn level_screens(def: &LevelDef) -> f64 {
    let mut n = 0.0;
    for st in &def.stages {
        n +=
            (st.map.first().map_or(0, |r| r.chars().count()) * st.map.len()) as f64 / (32.0 * 18.0);
    }
    (crate::jsnum::round(n * 2.0) / 2.0).max(1.0)
}

/// '1 stage · 1 screen', '3 stages · 8 screens'
pub fn level_length(def: &LevelDef) -> String {
    let stages = def.stages.len();
    let screens = level_screens(def);
    format!(
        "{stages} {} · {} {}",
        if stages == 1 { "stage" } else { "stages" },
        crate::jsnum::to_string(screens),
        if screens == 1.0 { "screen" } else { "screens" }
    )
}
