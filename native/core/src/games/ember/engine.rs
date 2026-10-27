//! Ember & frost's simulation (`src/lib/emberEngine.ts`): pure and deterministic. Online play reruns it on both
//! computers, so the same (level, state, inputs) gives a bit-identical state everywhere — here too: the port
//! keeps the TS order of every sum and every loop, so a Rust side and a web side stay in step.
//!
//! One play tick, in order: 1. clocks; 2. interact (levers, lamps, gravity switches, mirrors); 3. channels;
//! 4. gates and movers; 5. thin water and ice; 6. boxes; 7. players; 8. pools, gems, keys, locks, arrows,
//! portals, doors; 9. checkpoints, then the phase. See the TS for the long form.

use std::collections::BTreeSet;
use std::sync::Arc;

use super::types::*;

const EPS: f64 = 0.01;
const RIDE: f64 = 0.5;
const PLATE_H: f64 = 4.0;
const GEM_BOX: f64 = 12.0;
const LAND_VY: f64 = 150.0;
const JUMP_CUT: f64 = 0.5;
const MAX_SEGMENTS: usize = 32;
const ONEWAY_SLOP: f64 = 0.05;

fn tile_char(c: char) -> Option<Tile> {
    Some(match c {
        '#' => T_WALL,
        '.' | ' ' => T_EMPTY,
        'L' => T_LAVA,
        'W' => T_WATER,
        'G' => T_GOO,
        'w' => T_THIN,
        'T' => T_TRAMP,
        '=' => T_ONEWAY,
        '~' => T_CRUMBLE,
        'O' => T_LIGHT,
        'D' => T_DARK,
        '1' | '2' | 'r' | 'b' | 'R' | 'B' | '^' | 'k' | 'C' => T_EMPTY,
        _ => return None,
    })
}

fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.w - EPS && b.x < a.x + a.w - EPS && a.y < b.y + b.h - EPS && b.y < a.y + a.h - EPS
}
fn in_rect(r: &Rect, x: f64, y: f64) -> bool {
    x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h
}
fn tile_rect(tx: f64, ty: f64) -> Rect {
    Rect::new(tx * TILE, ty * TILE, TILE, TILE)
}
pub fn player_rect(p: &Player) -> Rect {
    Rect::new(p.x, p.y, PLAYER_W, PLAYER_H)
}
fn box_rect(x: f64, y: f64) -> Rect {
    Rect::new(x, y, BOX_SIZE, BOX_SIZE)
}

// ---- compiling

fn map_cols(def: &StageDef) -> usize {
    def.map.first().map_or(0, |r| r.chars().count())
}
fn map_rows(def: &StageDef) -> usize {
    def.map.len()
}

/// every map problem of one stage, as lines
fn map_problems(def: &StageDef) -> Vec<String> {
    let mut out = vec![];
    let cols = map_cols(def);
    let rows = map_rows(def);
    if !(MIN_ROWS..=MAX_ROWS).contains(&rows) {
        out.push(format!(
            "map has {rows} rows, needs {MIN_ROWS} to {MAX_ROWS}"
        ));
    }
    if !(MIN_COLS..=MAX_COLS).contains(&cols) {
        out.push(format!(
            "map is {cols} wide, needs {MIN_COLS} to {MAX_COLS}"
        ));
    }
    let mut counts = [0usize; 4];
    const SPECIAL: [char; 4] = ['1', '2', 'R', 'B'];
    for (ty, row) in def.map.iter().enumerate() {
        let len = row.chars().count();
        if len != cols {
            out.push(format!("row {ty} has {len} chars, needs {cols} like row 0"));
        }
        for (tx, c) in row.chars().enumerate() {
            if tile_char(c).is_none() {
                out.push(format!("unknown char '{c}' at ({tx}, {ty})"));
            }
            if let Some(i) = SPECIAL.iter().position(|&s| s == c) {
                counts[i] += 1;
            }
            if (c == 'R' || c == 'B') && ty == 0 {
                out.push(format!(
                    "door '{c}' at ({tx}, {ty}) has no room for its top tile"
                ));
            }
        }
    }
    let names = ["fire spawn", "frost spawn", "fire door", "frost door"];
    for (i, c) in SPECIAL.iter().enumerate() {
        if counts[i] == 0 {
            out.push(format!("missing {} '{c}'", names[i]));
        } else if counts[i] > 1 {
            out.push(format!(
                "{} {}s '{c}', needs exactly 1",
                counts[i], names[i]
            ));
        }
    }
    out
}

/// a checkpoint's zone in tiles: its own, or the default around the flag
fn zone_of(x: f64, y: f64, zone: Option<Rect>) -> Rect {
    zone.unwrap_or(Rect::new(
        x - (CHECKPOINT_ZONE_W - 1.0) / 2.0,
        y - CHECKPOINT_ZONE_H + 1.0,
        CHECKPOINT_ZONE_W,
        CHECKPOINT_ZONE_H,
    ))
}

/// tile rects each thing covers, for bounds checks
fn thing_bounds(t: &ThingDef) -> Vec<Rect> {
    match t {
        ThingDef::Plate { x, y, w, .. } => vec![Rect::new(*x, *y, w.unwrap_or(2.0), 1.0)],
        ThingDef::Button { x, y, w, .. } => vec![Rect::new(*x, *y, w.unwrap_or(1.0), 1.0)],
        ThingDef::Gate { x, y, w, h, .. }
        | ThingDef::Ice { x, y, w, h }
        | ThingDef::Lock { x, y, w, h }
        | ThingDef::Dark { x, y, w, h } => {
            vec![Rect::new(*x, *y, *w, *h)]
        }
        ThingDef::Mover { x, y, w, h, to, .. } => {
            vec![Rect::new(*x, *y, *w, *h), Rect::new(to.x, to.y, *w, *h)]
        }
        ThingDef::Fan { x, y, w, h, .. } => vec![Rect::new(*x, *y - *h + 1.0, *w, *h)],
        ThingDef::Portal { x, y, .. } => vec![Rect::new(*x, *y - 1.0, 1.0, 2.0)],
        ThingDef::Checkpoint { x, y, zone } => {
            let mut v = vec![Rect::new(*x, *y, 1.0, 1.0)];
            if let Some(z) = zone {
                v.push(*z);
            }
            v
        }
        t => {
            let (x, y) = t.xy();
            vec![Rect::new(x, y, 1.0, 1.0)]
        }
    }
}

pub fn cond_channels(c: Option<&Cond>) -> Vec<String> {
    c.map_or(vec![], |c| {
        c.0.iter()
            .map(|n| n.strip_prefix('!').unwrap_or(n).to_string())
            .collect()
    })
}

fn num(x: f64) -> String {
    crate::jsnum::to_string(x)
}

/// !(x > 0), NaN included
fn not_positive(x: f64) -> bool {
    x.is_nan() || x <= 0.0
}

fn thing_problems(def: &StageDef) -> Vec<String> {
    let mut out = vec![];
    let cols = map_cols(def) as f64;
    let rows = map_rows(def) as f64;
    let mut driven: BTreeSet<String> = BTreeSet::from([CH_LIT.to_string()]);
    let mut pairs: std::collections::BTreeMap<String, usize> = Default::default();
    for (i, t) in def.things.iter().enumerate() {
        let k = t.kind();
        let (tx, ty) = t.xy();
        for b in thing_bounds(t) {
            let ok = [b.x, b.y, b.w, b.h].iter().all(|v| v.is_finite())
                && b.w > 0.0
                && b.h > 0.0
                && b.x >= 0.0
                && b.y >= 0.0
                && b.x + b.w <= cols
                && b.y + b.h <= rows;
            if !ok {
                out.push(format!(
                    "thing {i} ({k}) at ({}, {}) is out of bounds",
                    num(tx),
                    num(ty)
                ));
            }
        }
        let ch = match t {
            ThingDef::Plate { ch, .. }
            | ThingDef::Lever { ch, .. }
            | ThingDef::Button { ch, .. }
            | ThingDef::Sensor { ch, .. } => Some(ch),
            _ => None,
        };
        if let Some(ch) = ch {
            if ch == CH_LIT {
                out.push(format!(
                    "thing {i} ({k}) can't drive '{CH_LIT}': that channel is the lights"
                ));
            }
            driven.insert(ch.clone());
        }
        match t {
            ThingDef::Portal { pair, .. } => *pairs.entry(pair.clone()).or_default() += 1,
            ThingDef::Button { secs, .. } if not_positive(*secs) => {
                out.push(format!("thing {i} (button) needs secs > 0"))
            }
            ThingDef::Lamp {
                secs: Some(secs), ..
            } if not_positive(*secs) => {
                out.push(format!("thing {i} (lamp) needs secs > 0, or none"))
            }
            ThingDef::Cannon { power: Some(p), .. } if not_positive(*p) || *p > 60.0 => {
                out.push(format!("thing {i} (cannon) needs power in 1..60 tiles/s"))
            }
            _ => {}
        }
    }
    for (pair, n) in &pairs {
        if *n != 2 {
            out.push(format!(
                "portal pair '{pair}' has {n} portals, needs exactly 2"
            ));
        }
    }
    for (i, t) in def.things.iter().enumerate() {
        let c = match t {
            ThingDef::Gate { open, .. } => Some(open),
            ThingDef::Mover { when, .. }
            | ThingDef::Fan { when, .. }
            | ThingDef::Emitter { when, .. } => when.as_ref(),
            ThingDef::Cannon { fire, .. } => fire.as_ref(),
            _ => None,
        };
        for ch in cond_channels(c) {
            if !driven.contains(&ch) {
                out.push(format!("thing {i} ({}) listens to channel '{ch}' that no plate, lever, button or sensor drives", t.kind()));
            }
        }
    }
    let keys: usize = def
        .map
        .iter()
        .map(|r| r.chars().filter(|&c| c == 'k').count())
        .sum();
    let locks = def
        .things
        .iter()
        .filter(|t| matches!(t, ThingDef::Lock { .. }))
        .count();
    if keys != locks {
        out.push(format!(
            "{keys} keys for {locks} locks: every lock needs its own key"
        ));
    }
    out
}

fn stage_problems(def: &StageDef) -> Vec<String> {
    let mut m = map_problems(def);
    m.extend(thing_problems(def));
    m
}

/// everything wrong with a level definition, as readable lines; empty when it compiles cleanly.
/// lines about a stage of a multi-stage level start 'stage N: ' (N from 1)
pub fn validate_level(def: &LevelDef) -> Vec<String> {
    if def.stages.is_empty() || def.stages.len() > MAX_STAGES {
        return vec![format!("a level needs 1 to {MAX_STAGES} stages")];
    }
    let many = def.stages.len() > 1;
    def.stages
        .iter()
        .enumerate()
        .flat_map(|(i, st)| {
            stage_problems(st).into_iter().map(move |p| {
                if many {
                    format!("stage {}: {p}", i + 1)
                } else {
                    p
                }
            })
        })
        .collect()
}

/// turns a LevelDef into px geometry. Err on a malformed map, a lone portal, or a thing out of bounds; channels
/// nobody drives don't (validate_level reports them)
pub fn compile_level(def: &LevelDef) -> Result<Level, String> {
    let problems: Vec<String> = validate_level(def)
        .into_iter()
        .filter(|p| !p.contains("listens to channel"))
        .collect();
    if !problems.is_empty() {
        return Err(format!("level '{}': {}", def.id, problems.join("; ")));
    }
    let stages: Vec<Stage> = def
        .stages
        .iter()
        .enumerate()
        .map(|(i, st)| compile_stage(st, i))
        .collect();
    let mut gem_totals = [0, 0];
    for st in &stages {
        for g in &st.gems {
            gem_totals[if g.el == Element::Fire { 0 } else { 1 }] += 1;
        }
    }
    Ok(Level {
        def: def.clone(),
        stages,
        gem_totals,
    })
}

fn compile_stage(def: &StageDef, index: usize) -> Stage {
    let cols = map_cols(def);
    let rows = map_rows(def);
    let centre = |tx: f64, ty: f64| Xy {
        x: tx * TILE + TILE / 2.0,
        y: ty * TILE + TILE / 2.0,
    };
    let px = |r: Rect| Rect::new(r.x * TILE, r.y * TILE, r.w * TILE, r.h * TILE);
    let flag = |x: f64, y: f64, zone: Option<Rect>| CheckpointDef {
        x: x * TILE + TILE / 2.0,
        y: (y + 1.0) * TILE,
        zone: px(zone_of(x, y, zone)),
    };
    let mut tiles = Vec::with_capacity(cols * rows);
    let mut spawns = [Xy { x: 0.0, y: 0.0 }; 2];
    let mut doors = [Rect::new(0.0, 0.0, 0.0, 0.0); 2];
    let mut l = Stage {
        index,
        def: def.clone(),
        cols,
        rows,
        w: cols as f64 * TILE,
        h: rows as f64 * TILE,
        tiles: vec![],
        dark: def.dark.unwrap_or(false),
        spawns,
        doors,
        gems: vec![],
        plates: vec![],
        levers: vec![],
        buttons: vec![],
        gates: vec![],
        movers: vec![],
        boxes: vec![],
        fans: vec![],
        ice: vec![],
        emitters: vec![],
        mirrors: vec![],
        sensors: vec![],
        portals: vec![],
        signs: vec![],
        thin: vec![],
        crumble: vec![],
        checkpoints: vec![],
        lamps: vec![],
        gswitches: vec![],
        arrows: vec![],
        keys: vec![],
        locks: vec![],
        cannons: vec![],
        darks: vec![],
        lookup: Lookup::default(),
    };
    for (ty, row) in def.map.iter().enumerate() {
        for (tx, c) in row.chars().enumerate().take(cols) {
            let t = tile_char(c).unwrap_or(T_EMPTY);
            tiles.push(t);
            let (fx, fy) = (tx as f64, ty as f64);
            if t == T_THIN {
                l.thin.push(ty * cols + tx);
            }
            if t == T_CRUMBLE {
                l.crumble.push(ty * cols + tx);
            }
            match c {
                '1' | '2' => {
                    spawns[usize::from(c == '2')] = Xy {
                        x: fx * TILE + (TILE - PLAYER_W) / 2.0,
                        y: (fy + 1.0) * TILE - PLAYER_H,
                    }
                }
                'R' | 'B' => {
                    doors[usize::from(c == 'B')] =
                        Rect::new(fx * TILE, (fy - 1.0) * TILE, TILE, 2.0 * TILE)
                }
                'r' | 'b' => {
                    let p = centre(fx, fy);
                    l.gems.push(Gem {
                        x: p.x,
                        y: p.y,
                        el: if c == 'r' {
                            Element::Fire
                        } else {
                            Element::Frost
                        },
                    });
                }
                '^' => l.arrows.push(centre(fx, fy)),
                'k' => l.keys.push(centre(fx, fy)),
                'C' => l.checkpoints.push(flag(fx, fy, None)),
                _ => {}
            }
        }
    }
    l.tiles = tiles;
    l.spawns = spawns;
    l.doors = doors;
    let mut portal_pairs: Vec<String> = vec![];
    // a plate's or button's strip: the bottom of its row, or the top when it's on the ceiling
    let strip = |x: f64, y: f64, ceil: bool, w: f64| {
        Rect::new(
            x * TILE,
            if ceil {
                y * TILE
            } else {
                (y + 1.0) * TILE - PLATE_H
            },
            w * TILE,
            PLATE_H,
        )
    };
    for t in &def.things {
        match t {
            ThingDef::Plate { x, y, w, ch, ceil } => {
                let ceil = ceil.unwrap_or(false);
                l.plates.push(Strip {
                    r: strip(*x, *y, ceil, w.unwrap_or(2.0)),
                    ch: ch.clone(),
                    ceil,
                    ticks: 0,
                });
            }
            ThingDef::Button {
                x,
                y,
                w,
                ch,
                secs,
                ceil,
            } => {
                let ceil = ceil.unwrap_or(false);
                let ticks = crate::jsnum::round(secs * STEP_HZ).max(1.0) as i64;
                l.buttons.push(Strip {
                    r: strip(*x, *y, ceil, w.unwrap_or(1.0)),
                    ch: ch.clone(),
                    ceil,
                    ticks,
                });
            }
            ThingDef::Lever { x, y, ch, on } => l.levers.push(LeverDef {
                r: tile_rect(*x, *y),
                ch: ch.clone(),
                on: on.unwrap_or(false),
            }),
            ThingDef::Gate {
                x,
                y,
                w,
                h,
                open,
                dir,
            } => l.gates.push(GateDef {
                r: px(Rect::new(*x, *y, *w, *h)),
                open: open.clone(),
                dir: dir.unwrap_or(Dir::Up),
            }),
            ThingDef::Mover {
                x,
                y,
                w,
                h,
                to,
                when,
                speed,
            } => l.movers.push(MoverDef {
                r: px(Rect::new(*x, *y, *w, *h)),
                to: Xy {
                    x: to.x * TILE,
                    y: to.y * TILE,
                },
                when: when.clone(),
                speed: speed.unwrap_or(MOVER_SPEED) * TILE,
            }),
            ThingDef::Box { x, y } => l.boxes.push(Xy {
                x: x * TILE,
                y: y * TILE,
            }),
            ThingDef::Fan { x, y, w, h, when } => l.fans.push(FanDef {
                r: Rect::new(x * TILE, (y - h + 1.0) * TILE, w * TILE, h * TILE),
                when: when.clone(),
            }),
            ThingDef::Ice { x, y, w, h } => l.ice.push(px(Rect::new(*x, *y, *w, *h))),
            ThingDef::Emitter { x, y, dir, when } => {
                let c = centre(*x, *y);
                l.emitters.push(EmitterDef {
                    x: c.x,
                    y: c.y,
                    dir: *dir,
                    when: when.clone(),
                });
            }
            ThingDef::Mirror { x, y, slant } => {
                let c = centre(*x, *y);
                l.mirrors.push(MirrorDef {
                    x: c.x,
                    y: c.y,
                    slant: *slant,
                });
            }
            ThingDef::Sensor { x, y, ch } => l.sensors.push(SensorDef {
                r: tile_rect(*x, *y),
                ch: ch.clone(),
            }),
            ThingDef::Portal { x, y, pair } => {
                l.portals.push(PortalDef {
                    r: Rect::new(x * TILE, (y - 1.0) * TILE, TILE, 2.0 * TILE),
                    pair: pair.clone(),
                    other: usize::MAX,
                });
                portal_pairs.push(pair.clone());
            }
            ThingDef::Sign { x, y, text } => {
                let c = centre(*x, *y);
                l.signs.push(SignDef {
                    x: c.x,
                    y: c.y,
                    text: text.clone(),
                });
            }
            ThingDef::Checkpoint { x, y, zone } => l.checkpoints.push(flag(*x, *y, *zone)),
            ThingDef::Lamp { x, y, secs } => l.lamps.push(LampDef {
                r: tile_rect(*x, *y),
                ticks: secs.map_or(0, |s| crate::jsnum::round(s * STEP_HZ).max(1.0) as i64),
            }),
            ThingDef::Gswitch { x, y, who } => l.gswitches.push(GswitchDef {
                r: tile_rect(*x, *y),
                who: who.unwrap_or(Who::Myself),
            }),
            ThingDef::Lock { x, y, w, h } => l.locks.push(px(Rect::new(*x, *y, *w, *h))),
            ThingDef::Cannon {
                x,
                y,
                aim,
                power,
                fire,
            } => {
                let v = power.unwrap_or(CANNON_POWER) * TILE;
                let c = centre(*x, *y);
                let (ax, ay) = aim.delta();
                l.cannons.push(CannonDef {
                    x: c.x,
                    y: c.y,
                    aim: *aim,
                    vx: ax * v,
                    vy: ay * v,
                    fire: fire.clone(),
                });
            }
            ThingDef::Dark { x, y, w, h } => l.darks.push(px(Rect::new(*x, *y, *w, *h))),
        }
    }
    for i in 0..l.portals.len() {
        let pair = &l.portals[i].pair;
        l.portals[i].other = portal_pairs
            .iter()
            .enumerate()
            .position(|(j, q)| j != i && q == pair)
            .unwrap_or(usize::MAX);
    }
    // the lookups
    let at = |x: f64, y: f64| (y / TILE).floor() as usize * cols + (x / TILE).floor() as usize;
    let mut lk = Lookup {
        one_ways: l.tiles.contains(&T_ONEWAY),
        ..Default::default()
    };
    for (i, m) in l.mirrors.iter().enumerate() {
        lk.mirror_at.insert(at(m.x, m.y), i);
    }
    for (i, m) in l.sensors.iter().enumerate() {
        lk.sensor_at.insert(at(m.r.x, m.r.y), i);
    }
    for (i, t) in l.thin.iter().enumerate() {
        lk.thin_at.insert(*t, i);
    }
    for (i, t) in l.crumble.iter().enumerate() {
        lk.crumble_at.insert(*t, i);
    }
    l.lookup = lk;
    l
}

// ---- state

/// what goes in GameState.on: every channel that's on but the lights' own
fn list_on(on: &BTreeSet<String>) -> Vec<String> {
    on.iter().filter(|c| *c != CH_LIT).cloned().collect()
}

pub fn eval_cond(c: Option<&Cond>, on: &BTreeSet<String>) -> bool {
    let Some(c) = c else { return true };
    c.0.iter().all(|n| match n.strip_prefix('!') {
        Some(rest) => !on.contains(rest),
        None => on.contains(n.as_str()),
    })
}

fn new_player(stage: &Stage, i: PlayerIndex) -> Player {
    let sp = stage.spawns[i];
    Player {
        x: sp.x,
        y: sp.y,
        vx: 0.0,
        vy: 0.0,
        ground: true,
        face: 1,
        alive: true,
        in_door: false,
        coyote: 0,
        buffer: 0,
        prev_in: 0,
        portal_cd: 0,
        gems: 0,
        grav: 1,
        air: 0,
        cannon: -1,
        fly: 0,
        cannon_cd: 0,
    }
}

pub fn mover_pos(m: &MoverDef, t: f64) -> Xy {
    Xy {
        x: m.r.x + (m.to.x - m.r.x) * t,
        y: m.r.y + (m.to.y - m.r.y) * t,
    }
}

/// the stage the state is in
pub fn stage_of(level: &Level, stage: usize) -> &Stage {
    &level.stages[stage.min(level.stages.len() - 1)]
}

/// what a stage start carries over: the clock, the deaths and the gems banked so far
#[derive(Clone, Copy, Debug)]
pub struct Carry {
    pub tick: i64,
    pub deaths: i64,
    pub banked: [i64; 2],
}

/// the beginning of stage `index`. gates and when-driven movers start where their conditions put them
pub fn start_stage(level: &Level, index: usize, from: Option<Carry>) -> GameState {
    let stage = &level.stages[index];
    let mut s = GameState {
        tick: from.map_or(0, |f| f.tick),
        phase: Phase::Play,
        phase_t: 0,
        deaths: from.map_or(0, |f| f.deaths),
        stage: index,
        banked: from.map_or([0, 0], |f| f.banked),
        players: [new_player(stage, 0), new_player(stage, 1)],
        taken: vec![false; stage.gems.len()],
        levers: stage.levers.iter().map(|l| l.on).collect(),
        buttons: vec![0; stage.buttons.len()],
        gates: vec![0.0; stage.gates.len()],
        movers: stage
            .movers
            .iter()
            .map(|m| MoverState {
                x: m.r.x,
                y: m.r.y,
                t: 0.0,
                dir: 1,
            })
            .collect(),
        boxes: stage
            .boxes
            .iter()
            .map(|b| BoxState {
                x: b.x,
                y: b.y,
                vx: 0.0,
                vy: 0.0,
            })
            .collect(),
        ice: vec![0.0; stage.ice.len()],
        thin: vec![0; stage.thin.len()],
        mirrors: stage.mirrors.iter().map(|m| m.slant).collect(),
        lit: !stage.dark,
        lit_t: 0,
        arrows: vec![0; stage.arrows.len()],
        crumble: vec![0; stage.crumble.len()],
        keys: 0,
        key_taken: vec![false; stage.keys.len()],
        locks: vec![false; stage.locks.len()],
        cannons: vec![0; stage.cannons.len()],
        cp: -1,
        saved: None,
        on: vec![],
        events: vec![],
        plates: Some(vec![false; stage.plates.len()]),
        sensors: Some(vec![false; stage.sensors.len()]),
        is_snapshot: false,
    };
    let mut sim = Sim::new(stage);
    let on = sim.channels(&mut s, false);
    for (i, g) in stage.gates.iter().enumerate() {
        s.gates[i] = if eval_cond(Some(&g.open), &on) {
            1.0
        } else {
            0.0
        };
    }
    for (i, m) in stage.movers.iter().enumerate() {
        if m.when.is_none() || !eval_cond(m.when.as_ref(), &on) {
            continue;
        }
        let p = mover_pos(m, 1.0);
        s.movers[i] = MoverState {
            x: p.x,
            y: p.y,
            t: 1.0,
            dir: 1,
        };
    }
    // sensors may see differently once gates are open
    let on = sim.channels(&mut s, false);
    s.on = list_on(&on);
    s.events = vec![];
    s
}

/// a fresh go at a level, from its first stage with the clock at zero
pub fn init_state(level: &Level, deaths: i64) -> GameState {
    start_stage(
        level,
        0,
        Some(Carry {
            tick: 0,
            deaths,
            banked: [0, 0],
        }),
    )
}

/// a copy to step on: `saved` is shared, never copied
fn clone_state(s: &GameState) -> GameState {
    GameState {
        events: vec![],
        is_snapshot: false,
        ..s.clone()
    }
}

/// both players back at the last checkpoint taken in this stage (or the stage's start), the world as it was then.
/// the clock keeps the time of death and the death counts
pub fn respawn(level: &Level, s: &GameState) -> GameState {
    let at = Carry {
        tick: s.tick,
        deaths: s.deaths + 1,
        banked: s.banked,
    };
    let mut n;
    if let Some(saved) = &s.saved {
        n = clone_state(saved);
        n.saved = Some(saved.clone());
        n.tick = at.tick;
        n.deaths = at.deaths;
        n.phase = Phase::Play;
        n.phase_t = 0;
        // a key still held from before the death is not a fresh press
        for p in &mut n.players {
            p.prev_in = IN_LEFT | IN_RIGHT | IN_UP | IN_DOWN;
        }
    } else {
        n = start_stage(level, s.stage, Some(at));
    }
    n.events = vec![GameEvent {
        cp: Some(n.cp),
        ..GameEvent::bare(EventKind::Respawn)
    }];
    n
}

// ---- geometry

/// the tile at (tx, ty) as it currently behaves (see the TS)
pub fn tile_at(stage: &Stage, s: &GameState, tx: i64, ty: i64) -> Tile {
    if tx < 0 || ty < 0 || tx >= stage.cols as i64 || ty >= stage.rows as i64 {
        return T_WALL;
    }
    let ti = ty as usize * stage.cols + tx as usize;
    let t = stage.tiles[ti];
    if t == T_THIN
        && let Some(&i) = stage.lookup.thin_at.get(&ti)
        && s.thin.get(i).copied().unwrap_or(0) > 0
    {
        return T_WALL;
    }
    if t == T_LIGHT {
        return if s.lit { T_WALL } else { T_EMPTY };
    }
    if t == T_DARK {
        return if s.lit { T_EMPTY } else { T_WALL };
    }
    if t == T_CRUMBLE
        && let Some(&i) = stage.lookup.crumble_at.get(&ti)
        && s.crumble.get(i).copied().unwrap_or(0) > CRUMBLE_TICKS
    {
        return T_EMPTY;
    }
    t
}

/// current gate rect (after sliding) for drawing and collision
pub fn gate_rect(stage: &Stage, s: &GameState, i: usize) -> Rect {
    let g = &stage.gates[i];
    let f = s.gates.get(i).copied().unwrap_or(0.0);
    let (dx, dy) = g.dir.delta();
    Rect::new(g.r.x + dx * f * g.r.w, g.r.y + dy * f * g.r.h, g.r.w, g.r.h)
}

fn mover_rect(stage: &Stage, s: &GameState, i: usize) -> Rect {
    Rect::new(
        s.movers[i].x,
        s.movers[i].y,
        stage.movers[i].r.w,
        stage.movers[i].r.h,
    )
}

// body ids: players 0 and 1, boxes 2 + i. solid ids: gates 1000 + i, movers 2000 + i
const BOX0: i64 = 2;
const GATE0: i64 = 1000;
const MOVER0: i64 = 2000;

fn body_rect(s: &GameState, id: i64) -> Rect {
    if id < BOX0 {
        player_rect(&s.players[id as usize])
    } else {
        let b = &s.boxes[(id - BOX0) as usize];
        box_rect(b.x, b.y)
    }
}

/// t: what a tile hit is made of, as the map has it (T_WALL for everything that isn't a tile). ti: its tile index
#[derive(Clone, Copy, Debug)]
struct Hit {
    r: Rect,
    id: i64,
    t: Tile,
    ti: i64,
}

fn solid(r: Rect, id: i64) -> Hit {
    Hit {
        r,
        id,
        t: T_WALL,
        ti: -1,
    }
}

/// a body with gravity g rests on `on`: its feet on the top, or flipped, on the underside
fn rests_on(b: &Rect, g: f64, on: &Rect) -> bool {
    (if g > 0.0 {
        b.y + b.h - on.y
    } else {
        b.y - (on.y + on.h)
    })
    .abs()
        <= RIDE
        && b.x < on.x + on.w - EPS
        && on.x < b.x + b.w - EPS
}

fn reflect(d: Dir, m: Slant) -> Dir {
    match m {
        Slant::Slash => match d {
            Dir::Right => Dir::Up,
            Dir::Up => Dir::Right,
            Dir::Left => Dir::Down,
            Dir::Down => Dir::Left,
        },
        Slant::Back => match d {
            Dir::Right => Dir::Down,
            Dir::Down => Dir::Right,
            Dir::Left => Dir::Up,
            Dir::Up => Dir::Left,
        },
    }
}

fn rect_mid(r: &Rect) -> (f64, f64) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

fn centre_of(p: &Player) -> (f64, f64) {
    (p.x + PLAYER_W / 2.0, p.y + PLAYER_H / 2.0)
}

/// one tick of gravity (g: 1 down, -1 up) and of a fan, which always blows up
fn lift_fall(mut vy: f64, fan: bool, g: f64) -> f64 {
    vy += g * GRAVITY * STEP_S;
    if fan && vy > -FAN_MAX_UP {
        vy = (-FAN_MAX_UP).max(vy - FAN_ACCEL * STEP_S);
    }
    if g > 0.0 {
        MAX_FALL.min(vy)
    } else {
        (-MAX_FALL).max(vy)
    }
}

fn killer(t: Tile, el: PlayerIndex) -> Option<DeathCause> {
    if t == T_GOO {
        return Some(DeathCause::Goo);
    }
    if t == T_LAVA && el == 1 {
        return Some(DeathCause::Lava);
    }
    if (t == T_WATER || t == T_THIN) && el == 0 {
        return Some(DeathCause::Water);
    }
    None
}

/// light segments from every running emitter, for drawing
pub fn trace_beams(stage: &Stage, s: &GameState) -> Vec<Beam> {
    let mut on: BTreeSet<String> = s.on.iter().cloned().collect();
    if s.lit {
        on.insert(CH_LIT.into());
    }
    trace(stage, s, &on, None)
}

/// light segments from every running emitter; also reports which sensors are lit
fn trace(
    stage: &Stage,
    s: &GameState,
    on: &BTreeSet<String>,
    mut lit: Option<&mut Vec<bool>>,
) -> Vec<Beam> {
    let mut beams = vec![];
    if stage.emitters.is_empty() {
        return beams;
    }
    let mut blockers: Vec<Rect> = vec![];
    for i in 0..stage.gates.len() {
        if s.gates[i] < 1.0 {
            blockers.push(gate_rect(stage, s, i));
        }
    }
    for (i, r) in stage.ice.iter().enumerate() {
        if s.ice[i] < 1.0 {
            blockers.push(*r);
        }
    }
    for (i, r) in stage.locks.iter().enumerate() {
        if !s.locks[i] {
            blockers.push(*r);
        }
    }
    for b in &s.boxes {
        blockers.push(box_rect(b.x, b.y));
    }
    for i in 0..stage.movers.len() {
        blockers.push(mover_rect(stage, s, i));
    }
    for p in &s.players {
        if p.alive {
            blockers.push(player_rect(p));
        }
    }
    for e in &stage.emitters {
        if !eval_cond(e.when.as_ref(), on) {
            continue;
        }
        let mut x = e.x;
        let mut y = e.y;
        let mut dir = e.dir;
        for _ in 0..MAX_SEGMENTS {
            let (dx, dy) = dir.delta();
            // nearest blocker rect the ray enters
            let mut d_obs = f64::INFINITY;
            for r in &blockers {
                if if dy == 0.0 {
                    !(y > r.y && y < r.y + r.h)
                } else {
                    !(x > r.x && x < r.x + r.w)
                } {
                    continue;
                }
                let d = if dx > 0.0 {
                    if r.x + r.w > x {
                        r.x - x
                    } else {
                        f64::INFINITY
                    }
                } else if dx < 0.0 {
                    if r.x < x {
                        x - (r.x + r.w)
                    } else {
                        f64::INFINITY
                    }
                } else if dy > 0.0 {
                    if r.y + r.h > y {
                        r.y - y
                    } else {
                        f64::INFINITY
                    }
                } else if r.y < y {
                    y - (r.y + r.h)
                } else {
                    f64::INFINITY
                };
                d_obs = d_obs.min(d.max(0.0));
            }
            // walk tile centres until a wall, a mirror or a sensor
            let mut d_end = f64::INFINITY;
            let mut turn: Option<Slant> = None;
            let mut mirror = false;
            let tx0 = (x / TILE).floor() as i64;
            let ty0 = (y / TILE).floor() as i64;
            for k in 1..=(stage.cols + stage.rows) as i64 {
                let tx = tx0 + dx as i64 * k;
                let ty = ty0 + dy as i64 * k;
                let t = tile_at(stage, s, tx, ty);
                if t == T_WALL || t == T_TRAMP || t == T_CRUMBLE {
                    d_end = (k as f64 - 0.5) * TILE;
                    break;
                }
                let idx = ty as usize * stage.cols + tx as usize;
                if let Some(&si) = stage.lookup.sensor_at.get(&idx) {
                    d_end = k as f64 * TILE;
                    if d_obs >= d_end
                        && let Some(l) = lit.as_deref_mut()
                    {
                        l[si] = true;
                    }
                    break;
                }
                if let Some(&mi) = stage.lookup.mirror_at.get(&idx) {
                    d_end = k as f64 * TILE;
                    turn = Some(s.mirrors[mi]);
                    mirror = true;
                    break;
                }
            }
            let d = d_obs.min(d_end);
            beams.push(Beam {
                x1: x,
                y1: y,
                x2: x + dx * d,
                y2: y + dy * d,
            });
            let Some(m) = turn else { break };
            if d_obs < d_end || !mirror {
                break;
            }
            x += dx * d;
            y += dy * d;
            dir = reflect(dir, m);
        }
    }
    beams
}

/// the step's scratch: the hits a collide fills, and after a moveY that hit something, what it came to rest on
struct Sim<'a> {
    stage: &'a Stage,
    hits: Vec<Hit>,
    support: Vec<Hit>,
}

impl<'a> Sim<'a> {
    fn new(stage: &'a Stage) -> Self {
        Self {
            stage,
            hits: Vec::with_capacity(16),
            support: vec![],
        }
    }

    /// fills `hits` with every solid overlapping r. `me` is the body asking (skipped). players are solid only to
    /// boxes. `skip` lists more ids to ignore
    fn collide(&mut self, s: &GameState, r: &Rect, me: i64, skip: &[i64]) -> usize {
        let stage = self.stage;
        self.hits.clear();
        let x0 = 0i64.max(((r.x + EPS) / TILE).floor() as i64);
        let x1 = (stage.cols as i64 - 1).min(((r.x + r.w - EPS) / TILE).floor() as i64);
        let y0 = 0i64.max(((r.y + EPS) / TILE).floor() as i64);
        let y1 = (stage.rows as i64 - 1).min(((r.y + r.h - EPS) / TILE).floor() as i64);
        // the world's edge is a wall
        if r.x < -EPS {
            self.hits.push(solid(Rect::new(-TILE, r.y, TILE, r.h), -1));
        }
        if r.x + r.w > stage.w + EPS {
            self.hits
                .push(solid(Rect::new(stage.w, r.y, TILE, r.h), -1));
        }
        if r.y < -EPS {
            self.hits.push(solid(Rect::new(r.x, -TILE, r.w, TILE), -1));
        }
        if r.y + r.h > stage.h + EPS {
            self.hits
                .push(solid(Rect::new(r.x, stage.h, r.w, TILE), -1));
        }
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                let t = tile_at(stage, s, tx, ty);
                // one-way platforms are never solid here: moveY alone lands a body on them
                if t == T_EMPTY || t == T_ONEWAY {
                    continue;
                }
                let sink = if t == T_WALL || t == T_TRAMP || t == T_CRUMBLE {
                    0.0
                } else {
                    POOL_SINK
                };
                let ti = ty as usize * stage.cols + tx as usize;
                let h = Hit {
                    r: Rect::new(tx as f64 * TILE, ty as f64 * TILE + sink, TILE, TILE - sink),
                    id: -1,
                    t: stage.tiles[ti],
                    ti: ti as i64,
                };
                if overlaps(r, &h.r) {
                    self.hits.push(h);
                }
            }
        }
        for (i, ice) in stage.ice.iter().enumerate() {
            if s.ice[i] < 1.0 && overlaps(r, ice) {
                self.hits.push(solid(*ice, -1));
            }
        }
        for (i, l) in stage.locks.iter().enumerate() {
            if !s.locks[i] && overlaps(r, l) {
                self.hits.push(solid(*l, -1));
            }
        }
        for i in 0..stage.gates.len() {
            if skip.contains(&(GATE0 + i as i64)) {
                continue;
            }
            let g = gate_rect(stage, s, i);
            if overlaps(r, &g) {
                self.hits.push(solid(g, GATE0 + i as i64));
            }
        }
        for i in 0..stage.movers.len() {
            if skip.contains(&(MOVER0 + i as i64)) {
                continue;
            }
            let m = mover_rect(stage, s, i);
            if overlaps(r, &m) {
                self.hits.push(solid(m, MOVER0 + i as i64));
            }
        }
        for (i, b) in s.boxes.iter().enumerate() {
            let id = BOX0 + i as i64;
            if id == me || skip.contains(&id) {
                continue;
            }
            let b = box_rect(b.x, b.y);
            if overlaps(r, &b) {
                self.hits.push(solid(b, id));
            }
        }
        if me >= BOX0 {
            for p in 0..2 {
                let pl = &s.players[p];
                if !pl.alive || skip.contains(&(p as i64)) {
                    continue;
                }
                let pr = player_rect(pl);
                if overlaps(r, &pr) {
                    self.hits.push(solid(pr, p as i64));
                }
            }
        }
        self.hits.len()
    }

    /// moves a gate or mover (solid id `me`) from rect a to b, carrying everything standing on it (and on those),
    /// and every flipped player standing on its underside. false, moving nothing, when the move would push into a
    /// body that isn't riding or lift a rider into something. `apply` makes the move itself
    fn shove(
        &mut self,
        s: &mut GameState,
        me: i64,
        a: &Rect,
        b: &Rect,
        apply: &mut dyn FnMut(&mut GameState),
    ) -> bool {
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        if dx == 0.0 && dy == 0.0 {
            apply(s);
            return true;
        }
        let mut bodies: Vec<i64> = vec![];
        for p in 0..2 {
            if s.players[p].alive {
                bodies.push(p as i64);
            }
        }
        for i in 0..s.boxes.len() {
            bodies.push(BOX0 + i as i64);
        }
        // riders: standing on a, then on riders, until nothing new joins
        let mut riders: Vec<i64> = vec![];
        let mut tops: Vec<Rect> = vec![*a];
        let mut grew = true;
        while grew {
            grew = false;
            for &id in &bodies {
                if riders.contains(&id) {
                    continue;
                }
                let r = body_rect(s, id);
                let pl = if id < BOX0 {
                    Some(&s.players[id as usize])
                } else {
                    None
                };
                // a player jumping away isn't riding, and one in a cannon is held where it is
                if let Some(pl) = pl
                    && (pl.vy * (pl.grav as f64) < 0.0 || pl.cannon >= 0)
                {
                    continue;
                }
                let g = pl.map_or(1.0, |p| p.grav as f64);
                if tops.iter().any(|t| rests_on(&r, g, t)) {
                    riders.push(id);
                    tops.push(r);
                    grew = true;
                }
            }
        }
        for &id in &bodies {
            if !riders.contains(&id) && overlaps(&body_rect(s, id), b) {
                return false;
            }
        }
        // riders move one at a time, front first, each against those already moved
        let lead = |s: &GameState, id: i64| {
            let r = body_rect(s, id);
            r.x * dx + r.y * dy
        };
        riders.sort_by(|p, q| {
            let d = lead(s, *q) - lead(s, *p);
            d.partial_cmp(&0.0).unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut moved: Vec<(i64, f64, f64)> = vec![];
        for k in 0..riders.len() {
            let id = riders[k];
            // where it ends up: the full shift, or just the vertical part if something stops it sideways
            let mut skip = vec![me];
            skip.extend_from_slice(&riders[k + 1..]);
            let r = body_rect(s, id);
            let mut mx = dx;
            if self.collide(
                s,
                &Rect {
                    x: r.x + dx,
                    y: r.y + dy,
                    ..r
                },
                id,
                &skip,
            ) > 0
            {
                mx = 0.0;
                if dy != 0.0 && self.collide(s, &Rect { y: r.y + dy, ..r }, id, &skip) > 0 {
                    for &(o, x, y) in &moved {
                        set_xy(s, o, x, y);
                    }
                    return false;
                }
                if dy == 0.0 {
                    continue;
                }
            }
            let (ox, oy) = get_xy(s, id);
            moved.push((id, ox, oy));
            set_xy(s, id, ox + mx, oy + dy);
        }
        apply(s);
        true
    }

    /// which channels are on. with `stepping` it also advances buttons and pushes change events; it always records
    /// plates and sensors
    fn channels(&mut self, s: &mut GameState, stepping: bool) -> BTreeSet<String> {
        let stage = self.stage;
        let mut on: BTreeSet<String> = BTreeSet::new();
        if s.lit {
            on.insert(CH_LIT.into());
        }
        // a floor plate takes an upright player or a box; one on the ceiling only a flipped player
        let pressed = |s: &GameState, r: &Rect, ceil: bool| {
            s.players.iter().any(|p| {
                p.alive && p.grav == if ceil { -1 } else { 1 } && overlaps(&player_rect(p), r)
            }) || (!ceil && s.boxes.iter().any(|b| overlaps(&box_rect(b.x, b.y), r)))
        };
        let plates: Vec<bool> = stage
            .plates
            .iter()
            .map(|p| pressed(s, &p.r, p.ceil))
            .collect();
        for (i, p) in stage.plates.iter().enumerate() {
            if plates[i] {
                on.insert(p.ch.clone());
            }
            if stepping
                && let Some(was) = &s.plates
                && was.get(i) != Some(&plates[i])
            {
                let (x, y) = rect_mid(&p.r);
                s.events
                    .push(GameEvent::switched(EventKind::Plate, i, plates[i], x, y));
            }
        }
        s.plates = Some(plates);
        for (i, l) in stage.levers.iter().enumerate() {
            if s.levers[i] {
                on.insert(l.ch.clone());
            }
        }
        for (i, b) in stage.buttons.iter().enumerate() {
            let was = s.buttons[i] > 0;
            if stepping {
                s.buttons[i] = if pressed(s, &b.r, b.ceil) {
                    b.ticks
                } else {
                    (s.buttons[i] - 1).max(0)
                };
            }
            let now = s.buttons[i] > 0;
            if now {
                on.insert(b.ch.clone());
            }
            if stepping && was != now {
                let (x, y) = rect_mid(&b.r);
                s.events
                    .push(GameEvent::switched(EventKind::Button, i, now, x, y));
            }
        }
        if !stage.sensors.is_empty() {
            // emitters see this tick's plates, levers and buttons plus last tick's sensors
            let mut seen = on.clone();
            // a state off the network has no sensors list; its `on` still says which sensor channels were lit
            match &s.sensors {
                Some(sens) => {
                    for (i, m) in stage.sensors.iter().enumerate() {
                        if sens.get(i) == Some(&true) {
                            seen.insert(m.ch.clone());
                        }
                    }
                }
                None => {
                    for m in &stage.sensors {
                        if s.on.contains(&m.ch) {
                            seen.insert(m.ch.clone());
                        }
                    }
                }
            }
            let mut lit = vec![false; stage.sensors.len()];
            trace(stage, s, &seen, Some(&mut lit));
            for (i, m) in stage.sensors.iter().enumerate() {
                if lit[i] {
                    on.insert(m.ch.clone());
                }
                if stepping
                    && let Some(was) = &s.sensors
                    && was.get(i) != Some(&lit[i])
                {
                    let (x, y) = rect_mid(&m.r);
                    s.events
                        .push(GameEvent::switched(EventKind::Sensor, i, lit[i], x, y));
                }
            }
            s.sensors = Some(lit);
        } else {
            s.sensors = Some(vec![]);
        }
        on
    }

    /// fan lift on a body whose centre x is in a running fan's column and whose feet are inside it
    fn fanned(&self, on: &BTreeSet<String>, r: &Rect) -> bool {
        let cx = r.x + r.w / 2.0;
        let bottom = r.y + r.h;
        self.stage.fans.iter().any(|f| {
            cx >= f.r.x
                && cx < f.r.x + f.r.w
                && bottom > f.r.y
                && r.y < f.r.y + f.r.h
                && eval_cond(f.when.as_ref(), on)
        })
    }

    /// the pool under a body's feet, if any: the tile containing a point just above the feet
    fn pool_under(&self, s: &GameState, r: &Rect) -> Tile {
        let y = ((r.y + r.h - 1.0) / TILE).floor() as i64;
        for x in [r.x + 2.0, r.x + r.w / 2.0, r.x + r.w - 2.0] {
            let t = tile_at(self.stage, s, (x / TILE).floor() as i64, y);
            if t == T_LAVA || t == T_WATER || t == T_GOO || t == T_THIN {
                return t;
            }
        }
        T_EMPTY
    }

    /// adds to `hits` the one-way slabs a body with gravity g lands on when it moves from o to r
    fn one_way_hits(&mut self, o: (f64, f64), r: &Rect, g: f64) -> usize {
        let stage = self.stage;
        if !stage.lookup.one_ways {
            return 0;
        }
        let mut n = 0;
        let x0 = 0i64.max(((r.x + EPS) / TILE).floor() as i64);
        let x1 = (stage.cols as i64 - 1).min(((r.x + r.w - EPS) / TILE).floor() as i64);
        let y0 = 0i64.max(((r.y + EPS) / TILE).floor() as i64);
        let y1 = (stage.rows as i64 - 1).min(((r.y + r.h - EPS) / TILE).floor() as i64);
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                let ti = ty as usize * stage.cols + tx as usize;
                if stage.tiles[ti] != T_ONEWAY {
                    continue;
                }
                let top = ty as f64 * TILE;
                if if g > 0.0 {
                    o.1 + r.h > top + ONEWAY_SLOP
                } else {
                    o.1 < top + ONEWAY_H - ONEWAY_SLOP
                } {
                    continue;
                }
                let slab = Hit {
                    r: Rect::new(tx as f64 * TILE, top, TILE, ONEWAY_H),
                    id: -1,
                    t: T_ONEWAY,
                    ti: ti as i64,
                };
                if overlaps(r, &slab.r) {
                    self.hits.push(slab);
                    n += 1;
                }
            }
        }
        n
    }

    /// moves body `id` (at o) along y by dy; true when it hit something (and snaps flush to it), leaving what it
    /// came to rest against in `support`. g: one-way platforms only stop a move along it
    #[allow(clippy::too_many_arguments)]
    fn move_y(
        &mut self,
        s: &GameState,
        id: i64,
        o: &mut (f64, f64),
        w: f64,
        h: f64,
        dy: f64,
        g: f64,
    ) -> bool {
        if dy == 0.0 {
            return false;
        }
        let r = Rect::new(o.0, o.1 + dy, w, h);
        let mut n = self.collide(s, &r, id, &[]);
        if dy * g > 0.0 {
            n += self.one_way_hits(*o, &r, g);
        }
        if n == 0 {
            o.1 += dy;
            return false;
        }
        if dy > 0.0 {
            let mut top = f64::INFINITY;
            for t in &self.hits {
                top = top.min(t.r.y);
            }
            o.1 = o.1.max(top - h);
            self.support = self
                .hits
                .iter()
                .filter(|t| t.r.y <= top + EPS)
                .copied()
                .collect();
        } else {
            let mut bottom = f64::NEG_INFINITY;
            for t in &self.hits {
                bottom = bottom.max(t.r.y + t.r.h);
            }
            o.1 = o.1.min(bottom);
            self.support = self
                .hits
                .iter()
                .filter(|t| t.r.y + t.r.h >= bottom - EPS)
                .copied()
                .collect();
        }
        true
    }

    /// moves body `id` along x by dx, stepping up pool lips; true when it hit something
    /// (`ground`: the body stands upright on something, so it may step up a lip)
    #[allow(clippy::too_many_arguments)]
    fn move_x(
        &mut self,
        s: &GameState,
        id: i64,
        o: &mut (f64, f64),
        w: f64,
        h: f64,
        dx: f64,
        ground: bool,
    ) -> bool {
        if dx == 0.0 {
            return false;
        }
        let r = Rect::new(o.0 + dx, o.1, w, h);
        if self.collide(s, &r, id, &[]) == 0 {
            o.0 += dx;
            return false;
        }
        // climbing out of a pool: everything in the way tops out within POOL_SINK of our feet
        if ground {
            let top = self
                .hits
                .iter()
                .map(|t| t.r.y)
                .fold(f64::INFINITY, f64::min);
            let up = o.1 + h - top;
            if up > 0.0
                && up <= POOL_SINK + EPS
                && self.collide(s, &Rect::new(o.0 + dx, top - h, w, h), id, &[]) == 0
            {
                o.0 += dx;
                o.1 = top - h;
                return false;
            }
            self.collide(s, &r, id, &[]);
        }
        if dx > 0.0 {
            o.0 = o.0.max(
                self.hits
                    .iter()
                    .map(|t| t.r.x)
                    .fold(f64::INFINITY, f64::min)
                    - w,
            );
        } else {
            o.0 = o.0.min(
                self.hits
                    .iter()
                    .map(|t| t.r.x + t.r.w)
                    .fold(f64::NEG_INFINITY, f64::max),
            );
        }
        true
    }

    /// pushes a box sideways by up to dx; true when it moved
    fn push_box(&mut self, s: &mut GameState, i: usize, dx: f64) -> bool {
        let x0 = s.boxes[i].x;
        let mut o = (s.boxes[i].x, s.boxes[i].y);
        self.move_x(s, BOX0 + i as i64, &mut o, BOX_SIZE, BOX_SIZE, dx, false);
        let b = &mut s.boxes[i];
        b.x = o.0;
        b.y = o.1;
        b.vx = (b.x - x0) / STEP_S;
        b.x != x0
    }

    fn step_player(
        &mut self,
        s: &mut GameState,
        pi: PlayerIndex,
        bits: i64,
        on: &BTreeSet<String>,
        prev_box_vx: &[f64],
    ) {
        let stage = self.stage;
        let mut p = s.players[pi];
        // everything up and down below is mirrored by the player's own gravity
        let g = p.grav as f64;
        // a cannon's shot can't be steered
        let flying = p.fly > 0;
        let pressed = bits & !p.prev_in;
        let dir_x: i64 = if flying {
            0
        } else {
            i64::from(bits & IN_RIGHT != 0) - i64::from(bits & IN_LEFT != 0)
        };
        if dir_x != 0 {
            p.face = if dir_x > 0 { 1 } else { -1 };
        }

        // run
        if !flying {
            let target = dir_x as f64 * RUN_SPEED;
            let acc = (if p.ground { RUN_ACCEL } else { AIR_ACCEL }) * STEP_S;
            p.vx = if p.vx < target {
                target.min(p.vx + acc)
            } else {
                target.max(p.vx - acc)
            };
        }

        // jump: buffered presses and coyote time; in mid-air, an arrow's jump if one is held
        if pressed & IN_UP != 0 {
            p.buffer = JUMP_BUFFER_TICKS;
        }
        if p.ground {
            p.coyote = COYOTE_TICKS;
        }
        if p.buffer > 0 && p.coyote > 0 {
            p.vy = -g * JUMP_V;
            p.buffer = 0;
            p.coyote = 0;
            p.ground = false;
            let (x, y) = centre_of(&p);
            s.events
                .push(GameEvent::at_player(EventKind::Jump, pi, x, y));
        } else if p.buffer > 0 && p.air > 0 {
            p.vy = -g * AIR_JUMP_V;
            p.buffer = 0;
            p.air = 0;
            p.fly = 0;
            let (x, y) = centre_of(&p);
            s.events
                .push(GameEvent::at_player(EventKind::Airjump, pi, x, y));
        } else {
            if p.buffer > 0 {
                p.buffer -= 1;
            }
            if !p.ground && p.coyote > 0 {
                p.coyote -= 1;
            }
        }
        // a short hop: letting go while rising
        if !flying && p.prev_in & IN_UP != 0 && bits & IN_UP == 0 && p.vy * g < 0.0 {
            p.vy *= JUMP_CUT;
        }

        // y moves by the average of this tick's start and end velocity
        let v0 = p.vy;
        p.vy = lift_fall(p.vy, self.fanned(on, &player_rect(&p)), g);

        // x, pushing a box that's in the way
        let dx = p.vx * STEP_S;
        let mut hit_x = false;
        if dx != 0.0 {
            let r = Rect::new(p.x + dx, p.y, PLAYER_W, PLAYER_H);
            let mut done = false;
            if self.collide(s, &r, pi as i64, &[]) == 1
                && self.hits[0].id >= BOX0
                && self.hits[0].id < GATE0
            {
                let bi = (self.hits[0].id - BOX0) as usize;
                let b = s.boxes[bi];
                // only a box beside us, not one we're under or on
                if b.y < p.y + PLAYER_H - POOL_SINK - EPS && b.y + BOX_SIZE > p.y + EPS {
                    let amt = dx.signum() * dx.abs().min(PUSH_SPEED * STEP_S);
                    // the gap between us and the box first, then push the rest
                    let gap = if dx > 0.0 {
                        b.x - (p.x + PLAYER_W)
                    } else {
                        p.x - (b.x + BOX_SIZE)
                    };
                    let push_by = amt - dx.signum() * gap.max(0.0);
                    s.players[pi] = p;
                    if js_sign(push_by) == js_sign(dx) && self.push_box(s, bi, push_by) {
                        let b = s.boxes[bi];
                        p.x = if dx > 0.0 {
                            b.x - PLAYER_W
                        } else {
                            b.x + BOX_SIZE
                        };
                        p.vx = dx.signum() * p.vx.abs().min(PUSH_SPEED);
                        if prev_box_vx[bi] == 0.0 {
                            let (x, y) = centre_of(&p);
                            s.events
                                .push(GameEvent::at_player(EventKind::Push, pi, x, y));
                        }
                        done = true;
                        hit_x = true;
                    }
                }
            }
            // pressing into a wall keeps the run speed, so a jump up beside a ledge pops over its corner
            if !done {
                let mut o = (p.x, p.y);
                let hit = self.move_x(
                    s,
                    pi as i64,
                    &mut o,
                    PLAYER_W,
                    PLAYER_H,
                    dx,
                    p.ground && g > 0.0,
                );
                p.x = o.0;
                p.y = o.1;
                if hit {
                    hit_x = true;
                    if js_sign(dx) != dir_x as f64 {
                        p.vx = 0.0;
                    }
                }
            }
        }

        // y
        let was_ground = p.ground;
        let fall = p.vy * g;
        let mut o = (p.x, p.y);
        let hit = self.move_y(
            s,
            pi as i64,
            &mut o,
            PLAYER_W,
            PLAYER_H,
            ((v0 + p.vy) / 2.0) * STEP_S,
            g,
        );
        p.x = o.0;
        p.y = o.1;
        p.ground = hit && fall >= 0.0;
        if hit {
            p.vy = 0.0;
        }
        if p.ground {
            // a trampoline bounces whoever comes down with their middle over it, or with nothing else underfoot
            let cx = p.x + PLAYER_W / 2.0;
            let mut tramp = false;
            let mut other = false;
            let mut mid = false;
            for h in &self.support {
                let under = cx >= h.r.x && cx < h.r.x + h.r.w;
                if h.t == T_TRAMP {
                    tramp = true;
                    if under {
                        mid = true;
                    }
                } else if under {
                    other = true;
                }
            }
            if tramp && (mid || !other) {
                let v = TRAMP_MAX_V.min(TRAMP_MIN_V.max(fall * TRAMP_GAIN + TRAMP_ADD));
                p.vy = -g * v;
                p.ground = false;
                p.coyote = 0;
                p.buffer = 0;
                let (x, y) = centre_of(&p);
                s.events.push(GameEvent {
                    v: Some(v),
                    ..GameEvent::at_player(EventKind::Bounce, pi, x, y)
                });
            } else {
                // landing loses an arrow's jump
                p.air = 0;
                // crumbling blocks underfoot begin to go
                for h in &self.support {
                    if h.t != T_CRUMBLE {
                        continue;
                    }
                    let Some(&i) = stage.lookup.crumble_at.get(&(h.ti as usize)) else {
                        continue;
                    };
                    if s.crumble[i] != 0 {
                        continue;
                    }
                    s.crumble[i] = 1;
                    s.events.push(GameEvent::at_tile(
                        EventKind::Shake,
                        h.ti as usize,
                        h.r.x + TILE / 2.0,
                        h.r.y + TILE / 2.0,
                    ));
                }
                if !was_ground {
                    if fall > LAND_VY {
                        let (x, y) = centre_of(&p);
                        s.events
                            .push(GameEvent::at_player(EventKind::Land, pi, x, y));
                    }
                    // holding jump through a landing hops again
                    if bits & IN_UP != 0 {
                        p.buffer = p.buffer.max(1);
                    }
                }
            }
        }
        // a shot ends on anything it hits
        if p.fly > 0 {
            p.fly = if hit || hit_x { 0 } else { p.fly - 1 };
        }
        p.prev_in = bits;
        s.players[pi] = p;
    }

    /// where a body caught inside a light-only or dark-only block that has just turned solid ends up (see the TS)
    fn eject_y(&mut self, s: &GameState, r: &Rect, id: i64, g: f64) -> Option<f64> {
        self.collide(s, r, id, &[]);
        if !self.hits.iter().any(|h| h.t == T_LIGHT || h.t == T_DARK) {
            return Some(r.y);
        }
        let far = LIGHT_EJECT_TILES * TILE + EPS;
        if g > 0.0 {
            let bottom = r.y + r.h;
            let mut ty = ((bottom - EPS) / TILE).floor();
            while bottom - ty * TILE <= far {
                if self.collide(s, &Rect::new(r.x, ty * TILE - r.h, r.w, r.h), id, &[]) == 0 {
                    return Some(ty * TILE - r.h);
                }
                ty -= 1.0;
            }
        } else {
            let mut ty = ((r.y + EPS) / TILE).ceil();
            while ty * TILE - r.y <= far {
                if self.collide(s, &Rect::new(r.x, ty * TILE, r.w, r.h), id, &[]) == 0 {
                    return Some(ty * TILE);
                }
                ty += 1.0;
            }
        }
        None
    }

    /// turns the lights on or off. (x, y) is where the light event happens
    fn set_lit(&mut self, s: &mut GameState, lit: bool, x: f64, y: f64) {
        if s.lit == lit {
            return;
        }
        s.lit = lit;
        s.events.push(GameEvent {
            on: Some(lit),
            x: Some(x),
            y: Some(y),
            ..GameEvent::bare(EventKind::Light)
        });
        for pi in 0..2 {
            let p = s.players[pi];
            // a player in a cannon is safe from everything
            if !p.alive || p.cannon >= 0 {
                continue;
            }
            match self.eject_y(s, &player_rect(&p), pi as i64, p.grav as f64) {
                None => {
                    let p = &mut s.players[pi];
                    p.alive = false;
                    p.in_door = false;
                    let (cx, cy) = centre_of(p);
                    s.events.push(GameEvent {
                        cause: Some(DeathCause::Crush),
                        ..GameEvent::at_player(EventKind::Die, pi, cx, cy)
                    });
                }
                Some(y1) if y1 != p.y => {
                    let p = &mut s.players[pi];
                    p.y = y1;
                    if p.vy * p.grav as f64 > 0.0 {
                        p.vy = 0.0;
                    }
                }
                _ => {}
            }
        }
        for i in 0..s.boxes.len() {
            let b = s.boxes[i];
            match self.eject_y(s, &box_rect(b.x, b.y), BOX0 + i as i64, 1.0) {
                Some(y1) if y1 != b.y => {
                    s.boxes[i].y = y1;
                    s.boxes[i].vy = 0.0;
                }
                _ => {}
            }
        }
    }
}

/// Math.sign, which keeps -0 apart only by its own sign (compared with === here, so -0 equals 0)
fn js_sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

fn get_xy(s: &GameState, id: i64) -> (f64, f64) {
    if id < BOX0 {
        (s.players[id as usize].x, s.players[id as usize].y)
    } else {
        let b = &s.boxes[(id - BOX0) as usize];
        (b.x, b.y)
    }
}

fn set_xy(s: &mut GameState, id: i64, x: f64, y: f64) {
    if id < BOX0 {
        s.players[id as usize].x = x;
        s.players[id as usize].y = y;
    } else {
        let b = &mut s.boxes[(id - BOX0) as usize];
        b.x = x;
        b.y = y;
    }
}

/// advances exactly one STEP_S tick. pure: returns a new state whose `events` are this tick's only.
/// in 'dead' it counts DEATH_TICKS then respawns; in 'clear' STAGE_TICKS then the next stage; in 'won' it counts
#[allow(clippy::needless_range_loop)]
pub fn step(level: &Level, s0: &GameState, input: Inputs) -> GameState {
    let mut s = clone_state(s0);
    let stage = stage_of(level, s.stage);
    match s.phase {
        Phase::Dead => {
            s.phase_t += 1;
            return if s.phase_t >= DEATH_TICKS {
                respawn(level, &s)
            } else {
                s
            };
        }
        Phase::Clear => {
            s.phase_t += 1;
            if s.phase_t < STAGE_TICKS {
                return s;
            }
            let mut n = start_stage(
                level,
                s.stage + 1,
                Some(Carry {
                    tick: s.tick,
                    deaths: s.deaths,
                    banked: bank(stage, &s),
                }),
            );
            n.events = vec![GameEvent {
                stage: Some(n.stage as i64),
                ..GameEvent::bare(EventKind::Stage)
            }];
            return n;
        }
        Phase::Won => {
            s.phase_t += 1;
            return s;
        }
        Phase::Play => {}
    }
    let mut sim = Sim::new(stage);
    s.tick += 1;
    s.phase_t += 1;
    let bits = [input[0], input[1]];

    // 1. clocks
    for i in 0..stage.crumble.len() {
        let c = s.crumble[i];
        if c == 0 {
            continue;
        }
        let ti = stage.crumble[i];
        let tx = ti % stage.cols;
        let ty = ti / stage.cols;
        let (ax, ay) = (tx as f64 * TILE + TILE / 2.0, ty as f64 * TILE + TILE / 2.0);
        if c < CRUMBLE_TICKS + CRUMBLE_GONE_TICKS {
            s.crumble[i] = c + 1;
            if c == CRUMBLE_TICKS {
                s.events
                    .push(GameEvent::at_tile(EventKind::Break, ti, ax, ay));
            }
            continue;
        }
        // time to grow back, once nothing is in the way
        let r = tile_rect(tx as f64, ty as f64);
        if s.players
            .iter()
            .any(|p| p.alive && overlaps(&player_rect(p), &r))
            || s.boxes.iter().any(|b| overlaps(&box_rect(b.x, b.y), &r))
        {
            continue;
        }
        s.crumble[i] = 0;
        s.events
            .push(GameEvent::at_tile(EventKind::Mend, ti, ax, ay));
    }
    if s.lit_t > 0 {
        s.lit_t -= 1;
        if s.lit_t == 0 {
            sim.set_lit(&mut s, !stage.dark, stage.w / 2.0, stage.h / 2.0);
        }
    }
    for i in 0..stage.arrows.len() {
        if s.arrows[i] <= 0 {
            continue;
        }
        s.arrows[i] -= 1;
        if s.arrows[i] == 0 {
            s.events.push(GameEvent::at_thing(
                EventKind::Arrowback,
                i,
                stage.arrows[i].x,
                stage.arrows[i].y,
            ));
        }
    }

    // 2. interact
    for pi in 0..2 {
        let p = s.players[pi];
        if !p.alive || bits[pi] & IN_DOWN == 0 || p.prev_in & IN_DOWN != 0 {
            continue;
        }
        let r = player_rect(&p);
        if let Some(li) = stage.levers.iter().position(|l| overlaps(&r, &l.r)) {
            s.levers[li] = !s.levers[li];
            let (x, y) = rect_mid(&stage.levers[li].r);
            s.events
                .push(GameEvent::at_thing(EventKind::Lever, li, x, y));
            continue;
        }
        if let Some(la) = stage.lamps.iter().position(|l| overlaps(&r, &l.r)) {
            let lamp = &stage.lamps[la];
            let (x, y) = rect_mid(&lamp.r);
            // a timed lamp: the other state from the one the stage began in, for its ticks. a plain one: the
            // other state from now, for good
            if lamp.ticks > 0 {
                sim.set_lit(&mut s, stage.dark, x, y);
                s.lit_t = lamp.ticks;
            } else {
                let lit = !s.lit;
                sim.set_lit(&mut s, lit, x, y);
                s.lit_t = 0;
            }
            continue;
        }
        if let Some(gi) = stage.gswitches.iter().position(|g| overlaps(&r, &g.r)) {
            let g = &stage.gswitches[gi];
            let (x, y) = rect_mid(&g.r);
            s.events
                .push(GameEvent::at_thing(EventKind::Gswitch, gi, x, y));
            let who: Vec<usize> = match g.who {
                Who::Both => vec![0, 1],
                Who::Fire => vec![0],
                Who::Frost => vec![1],
                Who::Myself => vec![pi],
                Who::Other => vec![1 - pi],
            };
            for qi in who {
                let q = &mut s.players[qi];
                if !q.alive {
                    continue;
                }
                q.grav = if q.grav == 1 { -1 } else { 1 };
                q.ground = false;
                q.coyote = 0;
                let (x, y) = centre_of(q);
                let grav = q.grav;
                s.events.push(GameEvent {
                    grav: Some(grav),
                    ..GameEvent::at_player(EventKind::Flip, qi, x, y)
                });
            }
            continue;
        }
        if let Some(mi) = stage.mirrors.iter().position(|m| {
            overlaps(
                &r,
                &Rect::new(m.x - TILE / 2.0, m.y - TILE / 2.0, TILE, TILE),
            )
        }) {
            s.mirrors[mi] = if s.mirrors[mi] == Slant::Slash {
                Slant::Back
            } else {
                Slant::Slash
            };
            s.events.push(GameEvent::at_thing(
                EventKind::Mirror,
                mi,
                stage.mirrors[mi].x,
                stage.mirrors[mi].y,
            ));
        }
    }

    // 3. channels
    let on = sim.channels(&mut s, true);
    s.on = list_on(&on);

    // 4. gates and movers
    for (i, g) in stage.gates.iter().enumerate() {
        let f = s.gates[i];
        let want = if eval_cond(Some(&g.open), &on) {
            1.0
        } else {
            0.0
        };
        if f == want {
            continue;
        }
        let nf = if want > f {
            (f + GATE_SPEED * STEP_S).min(1.0)
        } else {
            (f - GATE_SPEED * STEP_S).max(0.0)
        };
        let a = gate_rect(stage, &s, i);
        s.gates[i] = nf;
        let b = gate_rect(stage, &s, i);
        s.gates[i] = f;
        sim.shove(&mut s, GATE0 + i as i64, &a, &b, &mut |s| s.gates[i] = nf);
    }
    for (i, m) in stage.movers.iter().enumerate() {
        let st = s.movers[i];
        let len = crate::games::js_hypot(m.to.x - m.r.x, m.to.y - m.r.y);
        if len == 0.0 {
            continue;
        }
        let dt = (m.speed * STEP_S) / len;
        let mut nt;
        let mut nd = st.dir;
        if m.when.is_some() {
            let want = if eval_cond(m.when.as_ref(), &on) {
                1.0
            } else {
                0.0
            };
            if st.t == want {
                continue;
            }
            nd = if want > st.t { 1 } else { -1 };
            nt = if nd > 0 {
                (st.t + dt).min(1.0)
            } else {
                (st.t - dt).max(0.0)
            };
        } else {
            nt = st.t + st.dir as f64 * dt;
            if nt >= 1.0 {
                nt = 1.0;
                nd = -1;
            } else if nt <= 0.0 {
                nt = 0.0;
                nd = 1;
            }
        }
        let to = mover_pos(m, nt);
        let a = mover_rect(stage, &s, i);
        sim.shove(
            &mut s,
            MOVER0 + i as i64,
            &a,
            &Rect {
                x: to.x,
                y: to.y,
                ..a
            },
            &mut |s| {
                s.movers[i] = MoverState {
                    x: to.x,
                    y: to.y,
                    t: nt,
                    dir: nd,
                };
            },
        );
    }

    // 5. thin water and ice
    let frost = s.players[1];
    let fc = centre_of(&frost);
    for (i, &ti) in stage.thin.iter().enumerate() {
        let tx = ti % stage.cols;
        let ty = ti / stage.cols;
        let cx = tx as f64 * TILE + TILE / 2.0;
        let cy = ty as f64 * TILE + TILE / 2.0;
        let near = frost.alive && crate::games::js_hypot(fc.0 - cx, fc.1 - cy) <= FREEZE_RADIUS;
        let was = s.thin[i] > 0;
        if near {
            s.thin[i] = FREEZE_TICKS;
        } else if was {
            s.thin[i] -= 1;
        }
        let now = s.thin[i] > 0;
        if now && !was {
            s.events
                .push(GameEvent::at_tile(EventKind::Freeze, ti, cx, cy));
            // whatever sat sunk in the water rises onto the new ice
            let top = ty as f64 * TILE;
            let left = tx as f64 * TILE;
            let lift = |x: f64, y: &mut f64, w: f64, h: f64| {
                if x < left + TILE - EPS
                    && left < x + w - EPS
                    && *y + h > top + EPS
                    && *y + h <= top + POOL_SINK + EPS
                {
                    *y = top - h;
                }
            };
            for p in &mut s.players {
                lift(p.x, &mut p.y, PLAYER_W, PLAYER_H);
            }
            for b in &mut s.boxes {
                lift(b.x, &mut b.y, BOX_SIZE, BOX_SIZE);
            }
        }
        if was && !now {
            s.events
                .push(GameEvent::at_tile(EventKind::Thaw, ti, cx, cy));
        }
    }
    let fire = s.players[0];
    if fire.alive {
        let fr = Rect::new(fire.x - 1.0, fire.y - 1.0, PLAYER_W + 2.0, PLAYER_H + 2.0);
        for (i, r) in stage.ice.iter().enumerate() {
            if s.ice[i] >= 1.0 || !overlaps(&fr, r) {
                continue;
            }
            s.ice[i] = if s.ice[i] + 1.0 / MELT_TICKS >= 1.0 - 1e-6 {
                1.0
            } else {
                s.ice[i] + 1.0 / MELT_TICKS
            };
            if s.ice[i] == 1.0 {
                let (x, y) = rect_mid(r);
                s.events.push(GameEvent::at_thing(EventKind::Melt, i, x, y));
            }
        }
    }

    // 6. boxes
    let prev_box_vx: Vec<f64> = s.boxes.iter().map(|b| b.vx).collect();
    for i in 0..s.boxes.len() {
        s.boxes[i].vx = 0.0;
        let b = s.boxes[i];
        let v0 = b.vy;
        let vy = lift_fall(b.vy, sim.fanned(&on, &box_rect(b.x, b.y)), 1.0);
        s.boxes[i].vy = vy;
        let mut o = (b.x, b.y);
        let hit = sim.move_y(
            &s,
            BOX0 + i as i64,
            &mut o,
            BOX_SIZE,
            BOX_SIZE,
            ((v0 + vy) / 2.0) * STEP_S,
            1.0,
        );
        s.boxes[i].x = o.0;
        s.boxes[i].y = o.1;
        if hit {
            s.boxes[i].vy = 0.0;
        }
    }

    // 7. players
    for pi in 0..2 {
        if s.players[pi].alive {
            sim.step_player(&mut s, pi, bits[pi], &on, &prev_box_vx);
        }
    }

    // 8. pools, gems, portals, doors
    for pi in 0..2 {
        let p = s.players[pi];
        if !p.alive {
            continue;
        }
        let r = player_rect(&p);
        // pools only harm an upright player standing in them
        let cause = if p.grav == 1 {
            killer(sim.pool_under(&s, &r), pi)
        } else {
            None
        };
        if let Some(cause) = cause {
            let p = &mut s.players[pi];
            p.alive = false;
            p.in_door = false;
            let (x, y) = centre_of(p);
            s.events.push(GameEvent {
                cause: Some(cause),
                ..GameEvent::at_player(EventKind::Die, pi, x, y)
            });
            continue;
        }
        let el = if pi == 0 {
            Element::Fire
        } else {
            Element::Frost
        };
        for (gi, g) in stage.gems.iter().enumerate() {
            if s.taken[gi] || g.el != el {
                continue;
            }
            if !overlaps(
                &r,
                &Rect::new(g.x - GEM_BOX / 2.0, g.y - GEM_BOX / 2.0, GEM_BOX, GEM_BOX),
            ) {
                continue;
            }
            s.taken[gi] = true;
            s.players[pi].gems += 1;
            s.events.push(GameEvent {
                el: Some(el),
                ..GameEvent::at_player(EventKind::Gem, pi, g.x, g.y)
            });
        }
        // keys go to the team; a closed lock within a pixel of a player takes one and opens for good
        for (i, k) in stage.keys.iter().enumerate() {
            if s.key_taken[i]
                || !overlaps(
                    &r,
                    &Rect::new(k.x - GEM_BOX / 2.0, k.y - GEM_BOX / 2.0, GEM_BOX, GEM_BOX),
                )
            {
                continue;
            }
            s.key_taken[i] = true;
            s.keys += 1;
            s.events
                .push(GameEvent::took(EventKind::Key, pi, i, k.x, k.y));
        }
        if s.keys > 0 {
            let reach = Rect::new(r.x - 1.0, r.y - 1.0, r.w + 2.0, r.h + 2.0);
            for (i, l) in stage.locks.iter().enumerate() {
                if s.locks[i] || s.keys <= 0 || !overlaps(&reach, l) {
                    continue;
                }
                s.locks[i] = true;
                s.keys -= 1;
                let (x, y) = rect_mid(l);
                s.events
                    .push(GameEvent::at_thing(EventKind::Unlock, i, x, y));
            }
        }
        // an arrow: only in the air, and only one at a time
        let p = s.players[pi];
        if !p.ground && p.air == 0 {
            for (i, a) in stage.arrows.iter().enumerate() {
                if s.arrows[i] != 0
                    || !overlaps(
                        &r,
                        &Rect::new(
                            a.x - ARROW_BOX / 2.0,
                            a.y - ARROW_BOX / 2.0,
                            ARROW_BOX,
                            ARROW_BOX,
                        ),
                    )
                {
                    continue;
                }
                s.players[pi].air = 1;
                s.arrows[i] = ARROW_RESPAWN_TICKS;
                s.events
                    .push(GameEvent::took(EventKind::Arrow, pi, i, a.x, a.y));
                break;
            }
        }
        let p = s.players[pi];
        let c = centre_of(&p);
        let in_portal = stage.portals.iter().position(|q| in_rect(&q.r, c.0, c.1));
        match in_portal {
            Some(ip) if p.portal_cd == 0 => {
                let o = &stage.portals[stage.portals[ip].other];
                let to = Rect::new(
                    o.r.x + (TILE - PLAYER_W) / 2.0,
                    if p.grav == 1 {
                        o.r.y + o.r.h - PLAYER_H
                    } else {
                        o.r.y
                    },
                    PLAYER_W,
                    PLAYER_H,
                );
                // a crate (or gate, or mover) sitting on the far side blocks the portal
                if sim.collide(&s, &to, pi as i64, &[]) == 0 {
                    let p = &mut s.players[pi];
                    p.x = to.x;
                    p.y = to.y;
                    p.portal_cd = PORTAL_COOLDOWN_TICKS;
                    let (x, y) = centre_of(p);
                    s.events
                        .push(GameEvent::at_player(EventKind::Portal, pi, x, y));
                }
            }
            None if p.portal_cd > 0 => s.players[pi].portal_cd -= 1,
            _ => {}
        }
        let d = stage.doors[pi];
        let p = &mut s.players[pi];
        let cx = p.x + PLAYER_W / 2.0;
        let was = p.in_door;
        p.in_door = p.ground
            && p.grav == 1
            && cx >= d.x
            && cx <= d.x + d.w
            && (p.y + PLAYER_H - (d.y + d.h)).abs() < 1.0;
        if p.in_door && !was {
            s.events.push(GameEvent::at_player(
                EventKind::Door,
                pi,
                d.x + d.w / 2.0,
                d.y + d.h / 2.0,
            ));
        }
    }

    // 9. checkpoints, then the phase
    let [fire0, frost0] = s.players;
    if fire0.alive && frost0.alive {
        let safe = |p: &Player| p.ground && p.cannon < 0;
        for (i, c) in stage.checkpoints.iter().enumerate() {
            let [fire0, frost0] = s.players;
            if i as i64 == s.cp || !safe(&fire0) || !safe(&frost0) {
                continue;
            }
            if !overlaps(&player_rect(&fire0), &c.zone) || !overlaps(&player_rect(&frost0), &c.zone)
            {
                continue;
            }
            s.cp = i as i64;
            // the world as it stands at the end of this tick, flag already lit
            let snap = GameState {
                saved: None,
                events: vec![],
                is_snapshot: true,
                ..s.clone()
            };
            s.saved = Some(Arc::new(snap));
            s.events
                .push(GameEvent::at_thing(EventKind::Checkpoint, i, c.x, c.y));
        }
    }
    if !s.players[0].alive || !s.players[1].alive {
        s.phase = Phase::Dead;
        s.phase_t = 0;
    } else if s.players[0].in_door && s.players[1].in_door {
        s.phase_t = 0;
        if s.stage + 1 < level.stages.len() {
            s.phase = Phase::Clear;
            s.events.push(GameEvent {
                stage: Some(s.stage as i64),
                ..GameEvent::bare(EventKind::Clear)
            });
        } else {
            s.phase = Phase::Won;
            s.events.push(GameEvent::bare(EventKind::Win));
        }
    }
    s
}

/// the banked gems plus this stage's
fn bank(stage: &Stage, s: &GameState) -> [i64; 2] {
    let mut out = s.banked;
    for (i, g) in stage.gems.iter().enumerate() {
        if s.taken.get(i) == Some(&true) {
            out[if g.el == Element::Fire { 0 } else { 1 }] += 1;
        }
    }
    out
}

/// gems over every stage so far against the level's total
pub fn score_of(level: &Level, s: &GameState) -> Score {
    let secs = s.tick as f64 / STEP_HZ;
    let gems = bank(stage_of(level, s.stage), s);
    let total = level.gem_totals;
    let won = s.phase == Phase::Won;
    let all = gems == total;
    let stars = if won {
        1 + i64::from(all) + i64::from(secs <= level.def.par)
    } else {
        0
    };
    Score {
        secs,
        gems,
        total,
        stars,
    }
}
