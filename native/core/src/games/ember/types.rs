//! Ember & frost's shared contract (`src/lib/emberTypes.ts`): constants, the level format, the compiled stage,
//! the state and the events. Pixel units throughout; y grows downward.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Deserializer};

/// goes in the online hello: both sides must run the same rules and the same levels
pub const EMBER_VERSION: f64 = 2.0;
/// the LevelDef format below
pub const LEVEL_FORMAT: u32 = 2;

pub const TILE: f64 = 20.0;

pub const VIEW_COLS: usize = 32;
pub const VIEW_ROWS: usize = 18;
pub const VIEW_W: f64 = VIEW_COLS as f64 * TILE;
pub const VIEW_H: f64 = VIEW_ROWS as f64 * TILE;

pub const MIN_COLS: usize = VIEW_COLS;
pub const MIN_ROWS: usize = VIEW_ROWS;
pub const MAX_COLS: usize = 4 * VIEW_COLS;
pub const MAX_ROWS: usize = 4 * VIEW_ROWS;
pub const MAX_STAGES: usize = 6;

pub const STEP_HZ: f64 = 60.0;
pub const STEP_S: f64 = 1.0 / STEP_HZ;

pub const PLAYER_W: f64 = 14.0;
pub const PLAYER_H: f64 = 22.0;
pub const RUN_SPEED: f64 = 150.0;
pub const RUN_ACCEL: f64 = 1400.0;
pub const AIR_ACCEL: f64 = 1000.0;
pub const GRAVITY: f64 = 1400.0;
pub const JUMP_V: f64 = 420.0;
pub const MAX_FALL: f64 = 620.0;
pub const COYOTE_TICKS: i64 = 6;
pub const JUMP_BUFFER_TICKS: i64 = 6;
pub const PUSH_SPEED: f64 = 70.0;
pub const BOX_SIZE: f64 = TILE;
pub const POOL_SINK: f64 = 6.0;
pub const FAN_ACCEL: f64 = 2600.0;
pub const FAN_MAX_UP: f64 = 260.0;
pub const MELT_TICKS: f64 = 45.0;
pub const FREEZE_RADIUS: f64 = 2.2 * TILE;
pub const FREEZE_TICKS: i64 = 150;
pub const PORTAL_COOLDOWN_TICKS: i64 = 24;
pub const GATE_SPEED: f64 = 2.5;
pub const MOVER_SPEED: f64 = 2.0;

pub const DEATH_TICKS: i64 = 72;
pub const STAGE_TICKS: i64 = 70;
pub const CHECKPOINT_ZONE_W: f64 = 3.0;
pub const CHECKPOINT_ZONE_H: f64 = 3.0;

pub const GLOW_EMBER: f64 = 5.5 * TILE;
pub const GLOW_FROST: f64 = 3.0 * TILE;
pub const GLOW_SMALL: f64 = 1.5 * TILE;
pub const LIGHT_EJECT_TILES: f64 = 2.0;
pub const CH_LIT: &str = "lights";

pub const TRAMP_MIN_V: f64 = 460.0;
pub const TRAMP_GAIN: f64 = 0.9;
pub const TRAMP_ADD: f64 = 120.0;
pub const TRAMP_MAX_V: f64 = 700.0;

pub const AIR_JUMP_V: f64 = 380.0;
pub const ARROW_RESPAWN_TICKS: i64 = 150;
pub const ARROW_BOX: f64 = 14.0;

pub const ONEWAY_H: f64 = 6.0;

pub const CRUMBLE_TICKS: i64 = 36;
pub const CRUMBLE_GONE_TICKS: i64 = 180;

pub const CANNON_POWER: f64 = 26.0;
pub const CANNON_FUSE_TICKS: i64 = 40;
pub const CANNON_FLY_TICKS: i64 = 90;
pub const CANNON_COOLDOWN_TICKS: i64 = 30;

pub const CAM_MARGIN_X: f64 = 6.0 * TILE;
pub const CAM_MARGIN_Y: f64 = 4.0 * TILE;
pub const CAM_MIN_ZOOM: f64 = 0.25;
pub const CAM_EASE: f64 = 0.12;

pub const IN_LEFT: i64 = 1;
pub const IN_RIGHT: i64 = 2;
pub const IN_UP: i64 = 4;
pub const IN_DOWN: i64 = 8;
/// [fire, frost]
pub type Inputs = [i64; 2];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Element {
    Fire,
    Frost,
}

impl Element {
    pub fn name(self) -> &'static str {
        match self {
            Element::Fire => "fire",
            Element::Frost => "frost",
        }
    }
}

/// 0 is always fire (ember), 1 is always frost
pub type PlayerIndex = usize;

pub type Tile = u8;
pub const T_EMPTY: Tile = 0;
pub const T_WALL: Tile = 1;
pub const T_LAVA: Tile = 2;
pub const T_WATER: Tile = 3;
pub const T_GOO: Tile = 4;
pub const T_THIN: Tile = 5;
pub const T_TRAMP: Tile = 6;
pub const T_ONEWAY: Tile = 7;
pub const T_CRUMBLE: Tile = 8;
pub const T_LIGHT: Tile = 9;
pub const T_DARK: Tile = 10;

/// a condition: one channel name, or several that must all be on. a leading '!' means "this channel is off".
/// a single name reads the same as a list of one, so it is kept as a list
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cond(pub Vec<String>);

impl<'de> Deserialize<'de> for Cond {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            One(String),
            Many(Vec<String>),
        }
        Ok(match Raw::deserialize(d)? {
            Raw::One(s) => Cond(vec![s]),
            Raw::Many(v) => Cond(v),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    pub fn delta(self) -> (f64, f64) {
        match self {
            Dir::Up => (0.0, -1.0),
            Dir::Down => (0.0, 1.0),
            Dir::Left => (-1.0, 0.0),
            Dir::Right => (1.0, 0.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Aim {
    Up,
    Down,
    Left,
    Right,
    Upleft,
    Upright,
    Downleft,
    Downright,
}

impl Aim {
    pub fn delta(self) -> (f64, f64) {
        let d = std::f64::consts::FRAC_1_SQRT_2;
        match self {
            Aim::Up => (0.0, -1.0),
            Aim::Down => (0.0, 1.0),
            Aim::Left => (-1.0, 0.0),
            Aim::Right => (1.0, 0.0),
            Aim::Upleft => (-d, -d),
            Aim::Upright => (d, -d),
            Aim::Downleft => (-d, d),
            Aim::Downright => (d, d),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Slant {
    #[serde(rename = "/")]
    Slash,
    #[serde(rename = "\\")]
    Back,
}

impl Slant {
    pub fn text(self) -> &'static str {
        match self {
            Slant::Slash => "/",
            Slant::Back => "\\",
        }
    }
}

/// whose gravity a switch flips
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Who {
    Fire,
    Frost,
    Both,
    #[serde(rename = "self")]
    Myself,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct Xy {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }
}

/// things, in TILE coordinates (x, y = the top-left tile; w, h in tiles). see emberTypes.ts for each
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "k", rename_all = "lowercase")]
pub enum ThingDef {
    Plate {
        x: f64,
        y: f64,
        w: Option<f64>,
        ch: String,
        ceil: Option<bool>,
    },
    Lever {
        x: f64,
        y: f64,
        ch: String,
        on: Option<bool>,
    },
    Button {
        x: f64,
        y: f64,
        w: Option<f64>,
        ch: String,
        secs: f64,
        ceil: Option<bool>,
    },
    Gate {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        open: Cond,
        dir: Option<Dir>,
    },
    Mover {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        to: Xy,
        when: Option<Cond>,
        speed: Option<f64>,
    },
    Box {
        x: f64,
        y: f64,
    },
    Fan {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        when: Option<Cond>,
    },
    Ice {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    },
    Emitter {
        x: f64,
        y: f64,
        dir: Dir,
        when: Option<Cond>,
    },
    Mirror {
        x: f64,
        y: f64,
        slant: Slant,
    },
    Sensor {
        x: f64,
        y: f64,
        ch: String,
    },
    Portal {
        x: f64,
        y: f64,
        pair: String,
    },
    Sign {
        x: f64,
        y: f64,
        text: String,
    },
    Checkpoint {
        x: f64,
        y: f64,
        zone: Option<Rect>,
    },
    Lamp {
        x: f64,
        y: f64,
        secs: Option<f64>,
    },
    Gswitch {
        x: f64,
        y: f64,
        who: Option<Who>,
    },
    Lock {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    },
    Cannon {
        x: f64,
        y: f64,
        aim: Aim,
        power: Option<f64>,
        fire: Option<Cond>,
    },
    Dark {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    },
}

impl ThingDef {
    pub fn kind(&self) -> &'static str {
        match self {
            ThingDef::Plate { .. } => "plate",
            ThingDef::Lever { .. } => "lever",
            ThingDef::Button { .. } => "button",
            ThingDef::Gate { .. } => "gate",
            ThingDef::Mover { .. } => "mover",
            ThingDef::Box { .. } => "box",
            ThingDef::Fan { .. } => "fan",
            ThingDef::Ice { .. } => "ice",
            ThingDef::Emitter { .. } => "emitter",
            ThingDef::Mirror { .. } => "mirror",
            ThingDef::Sensor { .. } => "sensor",
            ThingDef::Portal { .. } => "portal",
            ThingDef::Sign { .. } => "sign",
            ThingDef::Checkpoint { .. } => "checkpoint",
            ThingDef::Lamp { .. } => "lamp",
            ThingDef::Gswitch { .. } => "gswitch",
            ThingDef::Lock { .. } => "lock",
            ThingDef::Cannon { .. } => "cannon",
            ThingDef::Dark { .. } => "dark",
        }
    }

    pub fn xy(&self) -> (f64, f64) {
        match *self {
            ThingDef::Plate { x, y, .. }
            | ThingDef::Lever { x, y, .. }
            | ThingDef::Button { x, y, .. }
            | ThingDef::Gate { x, y, .. }
            | ThingDef::Mover { x, y, .. }
            | ThingDef::Box { x, y }
            | ThingDef::Fan { x, y, .. }
            | ThingDef::Ice { x, y, .. }
            | ThingDef::Emitter { x, y, .. }
            | ThingDef::Mirror { x, y, .. }
            | ThingDef::Sensor { x, y, .. }
            | ThingDef::Portal { x, y, .. }
            | ThingDef::Sign { x, y, .. }
            | ThingDef::Checkpoint { x, y, .. }
            | ThingDef::Lamp { x, y, .. }
            | ThingDef::Gswitch { x, y, .. }
            | ThingDef::Lock { x, y, .. }
            | ThingDef::Cannon { x, y, .. }
            | ThingDef::Dark { x, y, .. } => (x, y),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct StageDef {
    pub name: Option<String>,
    pub hint: Option<String>,
    pub dark: Option<bool>,
    pub map: Vec<String>,
    pub things: Vec<ThingDef>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct LevelDef {
    pub id: String,
    pub name: String,
    pub hint: Option<String>,
    pub par: f64,
    pub stages: Vec<StageDef>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Gem {
    pub x: f64,
    pub y: f64,
    pub el: Element,
}

/// a plate's or button's strip
#[derive(Clone, Debug, PartialEq)]
pub struct Strip {
    pub r: Rect,
    pub ch: String,
    pub ceil: bool,
    /// buttons: ticks of on-time a press gives
    pub ticks: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LeverDef {
    pub r: Rect,
    pub ch: String,
    pub on: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GateDef {
    pub r: Rect,
    pub open: Cond,
    pub dir: Dir,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MoverDef {
    pub r: Rect,
    pub to: Xy,
    pub when: Option<Cond>,
    /// px/s
    pub speed: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FanDef {
    /// the whole column the fan blows through, fan tile included
    pub r: Rect,
    pub when: Option<Cond>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmitterDef {
    pub x: f64,
    pub y: f64,
    pub dir: Dir,
    pub when: Option<Cond>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MirrorDef {
    pub x: f64,
    pub y: f64,
    pub slant: Slant,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SensorDef {
    pub r: Rect,
    pub ch: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PortalDef {
    pub r: Rect,
    pub pair: String,
    /// index of the paired portal
    pub other: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SignDef {
    pub x: f64,
    pub y: f64,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CheckpointDef {
    /// the middle of the flag's foot (bottom centre of its tile)
    pub x: f64,
    pub y: f64,
    pub zone: Rect,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LampDef {
    pub r: Rect,
    /// 0 for a plain toggle, else how long a press lasts
    pub ticks: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GswitchDef {
    pub r: Rect,
    pub who: Who,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CannonDef {
    pub x: f64,
    pub y: f64,
    pub aim: Aim,
    pub vx: f64,
    pub vy: f64,
    pub fire: Option<Cond>,
}

/// lookups cached per compiled stage (pure functions of the stage)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lookup {
    pub mirror_at: HashMap<usize, usize>,
    pub sensor_at: HashMap<usize, usize>,
    pub thin_at: HashMap<usize, usize>,
    pub crumble_at: HashMap<usize, usize>,
    pub one_ways: bool,
}

/// a compiled stage, in px. arrays keep map order (row-major) then StageDef.things order
#[derive(Clone, Debug, PartialEq)]
pub struct Stage {
    pub index: usize,
    pub def: StageDef,
    pub cols: usize,
    pub rows: usize,
    pub w: f64,
    pub h: f64,
    pub tiles: Vec<Tile>,
    pub dark: bool,
    /// player top-left px
    pub spawns: [Xy; 2],
    /// [fire door, frost door]
    pub doors: [Rect; 2],
    pub gems: Vec<Gem>,
    pub plates: Vec<Strip>,
    pub levers: Vec<LeverDef>,
    pub buttons: Vec<Strip>,
    pub gates: Vec<GateDef>,
    pub movers: Vec<MoverDef>,
    pub boxes: Vec<Xy>,
    pub fans: Vec<FanDef>,
    pub ice: Vec<Rect>,
    pub emitters: Vec<EmitterDef>,
    pub mirrors: Vec<MirrorDef>,
    pub sensors: Vec<SensorDef>,
    pub portals: Vec<PortalDef>,
    pub signs: Vec<SignDef>,
    pub thin: Vec<usize>,
    pub crumble: Vec<usize>,
    pub checkpoints: Vec<CheckpointDef>,
    pub lamps: Vec<LampDef>,
    pub gswitches: Vec<GswitchDef>,
    pub arrows: Vec<Xy>,
    pub keys: Vec<Xy>,
    pub locks: Vec<Rect>,
    pub cannons: Vec<CannonDef>,
    pub darks: Vec<Rect>,
    pub lookup: Lookup,
}

/// a compiled level
#[derive(Clone, Debug, PartialEq)]
pub struct Level {
    pub def: LevelDef,
    pub stages: Vec<Stage>,
    /// [fire, frost] over all stages
    pub gem_totals: [i64; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Player {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub ground: bool,
    /// 1 or -1
    pub face: i64,
    pub alive: bool,
    pub in_door: bool,
    pub coyote: i64,
    pub buffer: i64,
    pub prev_in: i64,
    pub portal_cd: i64,
    pub gems: i64,
    /// 1: falls down. -1: falls up and stands on ceilings
    pub grav: i64,
    pub air: i64,
    pub cannon: i64,
    pub fly: i64,
    pub cannon_cd: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeathCause {
    Lava,
    Water,
    Goo,
    Crush,
}

pub const CAUSES: [DeathCause; 4] = [
    DeathCause::Lava,
    DeathCause::Water,
    DeathCause::Goo,
    DeathCause::Crush,
];

impl DeathCause {
    pub fn name(self) -> &'static str {
        match self {
            DeathCause::Lava => "lava",
            DeathCause::Water => "water",
            DeathCause::Goo => "goo",
            DeathCause::Crush => "crush",
        }
    }
}

macro_rules! kinds {
    ($($v:ident = $s:literal),* $(,)?) => {
        /// what an event is; the order is the wire's (emberProtocol.ts KINDS)
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum EventKind { $($v),* }
        pub const KINDS: &[EventKind] = &[$(EventKind::$v),*];
        impl EventKind {
            pub fn name(self) -> &'static str { match self { $(EventKind::$v => $s),* } }
        }
    };
}

kinds! {
    Jump = "jump", Land = "land", Die = "die", Door = "door", Portal = "portal", Push = "push", Gem = "gem",
    Lever = "lever", Mirror = "mirror", Plate = "plate", Button = "button", Sensor = "sensor", Melt = "melt",
    Freeze = "freeze", Thaw = "thaw", Win = "win", Airjump = "airjump", Bounce = "bounce", Arrow = "arrow",
    Key = "key", Load = "load", Fire = "fire", Flip = "flip", Gswitch = "gswitch", Arrowback = "arrowback",
    Unlock = "unlock", Checkpoint = "checkpoint", Light = "light", Shake = "shake", Break = "break", Mend = "mend",
    Clear = "clear", Stage = "stage", Respawn = "respawn",
}

impl EventKind {
    pub fn index(self) -> usize {
        KINDS.iter().position(|k| *k == self).unwrap_or(0)
    }
}

/// what happened during a step. one shape for every kind: only the fields the kind has are set, and they are
/// written in this order (the TS objects' own key order for every kind)
#[derive(Clone, Debug, PartialEq)]
pub struct GameEvent {
    pub k: EventKind,
    pub p: Option<PlayerIndex>,
    pub tile: Option<i64>,
    pub i: Option<i64>,
    pub on: Option<bool>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub cause: Option<DeathCause>,
    pub v: Option<f64>,
    pub grav: Option<i64>,
    pub el: Option<Element>,
    pub stage: Option<i64>,
    pub cp: Option<i64>,
}

impl GameEvent {
    pub const fn bare(k: EventKind) -> Self {
        Self {
            k,
            p: None,
            tile: None,
            i: None,
            on: None,
            x: None,
            y: None,
            cause: None,
            v: None,
            grav: None,
            el: None,
            stage: None,
            cp: None,
        }
    }
    /// jump, land, door, portal, push, airjump
    pub fn at_player(k: EventKind, p: PlayerIndex, x: f64, y: f64) -> Self {
        Self {
            p: Some(p),
            x: Some(x),
            y: Some(y),
            ..Self::bare(k)
        }
    }
    /// lever, mirror, gswitch, arrowback, unlock, checkpoint, melt
    pub fn at_thing(k: EventKind, i: usize, x: f64, y: f64) -> Self {
        Self {
            i: Some(i as i64),
            x: Some(x),
            y: Some(y),
            ..Self::bare(k)
        }
    }
    /// arrow, key, load, fire
    pub fn took(k: EventKind, p: PlayerIndex, i: usize, x: f64, y: f64) -> Self {
        Self {
            p: Some(p),
            i: Some(i as i64),
            x: Some(x),
            y: Some(y),
            ..Self::bare(k)
        }
    }
    /// plate, button, sensor
    pub fn switched(k: EventKind, i: usize, on: bool, x: f64, y: f64) -> Self {
        Self {
            i: Some(i as i64),
            on: Some(on),
            x: Some(x),
            y: Some(y),
            ..Self::bare(k)
        }
    }
    /// freeze, thaw, shake, break, mend
    pub fn at_tile(k: EventKind, tile: usize, x: f64, y: f64) -> Self {
        Self {
            tile: Some(tile as i64),
            x: Some(x),
            y: Some(y),
            ..Self::bare(k)
        }
    }
    pub fn px(&self) -> f64 {
        self.x.unwrap_or(0.0)
    }
    pub fn py(&self) -> f64 {
        self.y.unwrap_or(0.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Play,
    Dead,
    Clear,
    Won,
}

impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Phase::Play => "play",
            Phase::Dead => "dead",
            Phase::Clear => "clear",
            Phase::Won => "won",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoverState {
    pub x: f64,
    pub y: f64,
    pub t: f64,
    /// 1 or -1
    pub dir: i64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxState {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
}

/// everything that changes. every per-thing list indexes the current stage's lists
#[derive(Clone, Debug, PartialEq)]
pub struct GameState {
    pub tick: i64,
    pub phase: Phase,
    pub phase_t: i64,
    pub deaths: i64,
    pub stage: usize,
    pub banked: [i64; 2],
    pub players: [Player; 2],
    pub taken: Vec<bool>,
    pub levers: Vec<bool>,
    pub buttons: Vec<i64>,
    pub gates: Vec<f64>,
    pub movers: Vec<MoverState>,
    pub boxes: Vec<BoxState>,
    pub ice: Vec<f64>,
    pub thin: Vec<i64>,
    pub mirrors: Vec<Slant>,
    pub lit: bool,
    pub lit_t: i64,
    pub arrows: Vec<i64>,
    pub crumble: Vec<i64>,
    pub keys: i64,
    pub key_taken: Vec<bool>,
    pub locks: Vec<bool>,
    pub cannons: Vec<i64>,
    pub cp: i64,
    /// the world as it was when checkpoint `cp` was taken: never mutated, shared from state to state, never
    /// sent. a snapshot itself has none and no events (`is_snapshot`)
    pub saved: Option<Arc<GameState>>,
    pub on: Vec<String>,
    pub events: Vec<GameEvent>,
    pub plates: Option<Vec<bool>>,
    pub sensors: Option<Vec<bool>>,
    /// a checkpoint's copy (Snapshot in the TS): has no `saved` and no `events` keys
    pub is_snapshot: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beam {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

/// gems and total count every stage of the level. stars: 1 for finishing, +1 for every gem, +1 for secs <= par
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Score {
    pub secs: f64,
    pub gems: [i64; 2],
    pub total: [i64; 2],
    pub stars: i64,
}

/// the part of the stage on screen, in world px
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub zoom: f64,
}
