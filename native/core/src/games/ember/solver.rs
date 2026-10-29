//! A conservative reachability check for ember & frost stages (`src/lib/emberSolver.ts`), used by the level tests
//! to catch broken maps. It works on the tile grid, not the physics: a player is a 1x2-tile body whose feet sit
//! in a "standing cell" above something solid. Mechanics are treated optimistically (gates open, plates held,
//! movers anywhere along their path, fans on); ice only melts where fire can reach it and thin water only
//! freezes where frost can, so those run as a small fixed point. Version 2's tiles and things aren't modelled.

use super::types::{Element, StageDef, ThingDef};

#[derive(Clone, Copy, Debug)]
pub struct ReachOpts {
    /// how many tiles a jump can climb
    pub jump: i64,
    /// how many tiles sideways a jump carries at a climb of 2 or less
    pub carry: i64,
}

impl Default for ReachOpts {
    fn default() -> Self {
        Self { jump: 3, carry: 4 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MissedGem {
    pub x: i64,
    pub y: i64,
    pub el: Element,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Reach {
    pub fire_door: bool,
    pub frost_door: bool,
    pub fire_gems: usize,
    pub frost_gems: usize,
    pub gem_totals: [usize; 2],
    pub missed_gems: Vec<MissedGem>,
    /// per tile, row-major: a standing cell (feet) the player reaches, and any cell their body passes
    pub stand: [Vec<bool>; 2],
    pub touched: [Vec<bool>; 2],
    pub melted: usize,
    pub frozen: usize,
}

struct Fan {
    x: i64,
    y: i64,
    w: i64,
    h: i64,
}

struct Ctx {
    w: i64,
    h: i64,
    map: Vec<Vec<char>>,
    ice_at: Vec<usize>,
    melted: Vec<bool>,
    frozen: Vec<bool>,
    open: Vec<bool>,
    floor: Vec<bool>,
    fan_at: Vec<usize>,
    fans: Vec<Fan>,
    portal_at: Vec<usize>,
    portals: Vec<(i64, i64, String)>,
    portal_other: Vec<Option<usize>>,
}

const SOLID: &str = "#LWGw";
const NONE: u8 = 0;
const SAFE: u8 = 1;
const DEADLY: u8 = 2;

fn build(def: &StageDef) -> Ctx {
    let map: Vec<Vec<char>> = def.map.iter().map(|r| r.chars().collect()).collect();
    let h = map.len() as i64;
    let w = map.first().map_or(0, Vec::len) as i64;
    let n = (w * h) as usize;
    let at = |x: i64, y: i64| (y * w + x) as usize;
    let inside = |x: i64, y: i64| x >= 0 && y >= 0 && x < w && y < h;
    let mut c = Ctx {
        w,
        h,
        map,
        ice_at: vec![0; n],
        melted: vec![],
        frozen: vec![false; n],
        open: vec![false; n],
        floor: vec![false; n],
        fan_at: vec![0; n],
        fans: vec![],
        portal_at: vec![0; n],
        portals: vec![],
        portal_other: vec![],
    };
    let mut ices = 0;
    for t in &def.things {
        match t {
            ThingDef::Ice { x, y, w: tw, h: th } => {
                ices += 1;
                let (x, y, tw, th) = (*x as i64, *y as i64, *tw as i64, *th as i64);
                for yy in y..y + th {
                    for xx in x..x + tw {
                        if inside(xx, yy) {
                            c.ice_at[at(xx, yy)] = ices;
                        }
                    }
                }
            }
            ThingDef::Gate {
                x, y, w: tw, h: th, ..
            } => {
                // open: its cells are free. closed: its top is a floor someone might stand on
                let (x, y, tw, th) = (*x as i64, *y as i64, *tw as i64, *th as i64);
                for yy in y..y + th {
                    for xx in x..x + tw {
                        if inside(xx, yy) {
                            c.open[at(xx, yy)] = true;
                        }
                    }
                }
                for xx in x..x + tw {
                    if inside(xx, y) {
                        c.floor[at(xx, y)] = true;
                    }
                }
            }
            ThingDef::Mover {
                x,
                y,
                w: tw,
                h: th,
                to,
                ..
            } => {
                // every tile-step along the path: the platform could be parked there
                let steps = ((to.x - x).abs().max((to.y - y).abs())).ceil().max(1.0);
                let mut i = 0.0;
                while i <= steps {
                    let px = crate::jsnum::round(x + ((to.x - x) * i) / steps) as i64;
                    let py = crate::jsnum::round(y + ((to.y - y) * i) / steps) as i64;
                    for yy in py..py + *th as i64 {
                        for xx in px..px + *tw as i64 {
                            if inside(xx, yy) {
                                c.open[at(xx, yy)] = true;
                                c.floor[at(xx, yy)] = true;
                            }
                        }
                    }
                    i += 1.0;
                }
            }
            ThingDef::Fan {
                x, y, w: tw, h: th, ..
            } => {
                let f = Fan {
                    x: *x as i64,
                    y: *y as i64,
                    w: *tw as i64,
                    h: *th as i64,
                };
                c.fans.push(f);
                let f = c.fans.last().unwrap();
                let (fx, fy, fw, fh, idx) = (f.x, f.y, f.w, f.h, c.fans.len());
                for yy in fy - fh + 1..=fy {
                    for xx in fx..fx + fw {
                        if inside(xx, yy) {
                            c.fan_at[at(xx, yy)] = idx;
                        }
                    }
                }
            }
            ThingDef::Portal { x, y, pair } => {
                c.portals.push((*x as i64, *y as i64, pair.clone()));
                let idx = c.portals.len();
                for yy in [*y as i64, *y as i64 - 1] {
                    if inside(*x as i64, yy) {
                        c.portal_at[at(*x as i64, yy)] = idx;
                    }
                }
            }
            _ => {}
        }
    }
    c.melted = vec![false; ices];
    c.portal_other = (0..c.portals.len())
        .map(|i| (0..c.portals.len()).find(|&j| j != i && c.portals[j].2 == c.portals[i].2))
        .collect();
    c
}

impl Ctx {
    fn chr(&self, x: i64, y: i64) -> char {
        self.map
            .get(y as usize)
            .and_then(|r| r.get(x as usize))
            .copied()
            .unwrap_or('#')
    }

    fn passable(&self, x: i64, y: i64) -> bool {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return false;
        }
        let i = (y * self.w + x) as usize;
        if self.open[i] {
            return true;
        }
        let ice = self.ice_at[i];
        if ice != 0 && !self.melted[ice - 1] {
            return false;
        }
        !SOLID.contains(self.chr(x, y))
    }

    /// what the tile under a player's feet does to them
    fn support(&self, el: Element, x: i64, y: i64) -> u8 {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return SAFE;
        }
        let i = (y * self.w + x) as usize;
        let ice = self.ice_at[i];
        if ice != 0 && !self.melted[ice - 1] {
            return SAFE;
        }
        if self.floor[i] {
            return SAFE;
        }
        match self.chr(x, y) {
            '#' => SAFE,
            'L' => {
                if el == Element::Fire {
                    SAFE
                } else {
                    DEADLY
                }
            }
            'W' => {
                if el == Element::Frost {
                    SAFE
                } else {
                    DEADLY
                }
            }
            'G' => DEADLY,
            'w' => {
                if el == Element::Frost || self.frozen[i] {
                    SAFE
                } else {
                    DEADLY
                }
            }
            _ => NONE,
        }
    }
}

enum Later {
    Fan(usize),
    Portal(usize),
}

struct Run<'a> {
    c: &'a Ctx,
    el: Element,
    stand: Vec<bool>,
    touched: Vec<bool>,
    fan_done: Vec<bool>,
    portal_done: Vec<bool>,
    queue: std::collections::VecDeque<usize>,
    later: std::collections::VecDeque<Later>,
}

impl Run<'_> {
    fn clear(&self, x: i64, y: i64) -> bool {
        self.c.passable(x, y) && self.c.passable(x, y - 1)
    }

    fn add_stand(&mut self, x: i64, y: i64) {
        let i = (y * self.c.w + x) as usize;
        if self.stand[i] {
            return;
        }
        self.stand[i] = true;
        self.queue.push_back(i);
    }

    fn commit(&mut self, path: &[usize]) {
        for &i in path {
            self.touched[i] = true;
        }
    }

    /// a body cell the player is passing through. fans and portals grab whoever touches them
    fn visit(&mut self, path: &mut Vec<usize>, x: i64, y: i64) {
        for yy in [y, y - 1] {
            if yy < 0 {
                continue;
            }
            let i = (yy * self.c.w + x) as usize;
            path.push(i);
            let f = self.c.fan_at[i];
            if f != 0 && !self.fan_done[f - 1] {
                self.fan_done[f - 1] = true;
                self.commit(path);
                self.later.push_back(Later::Fan(f - 1));
            }
            let p = self.c.portal_at[i];
            if p != 0 && !self.portal_done[p - 1] {
                self.portal_done[p - 1] = true;
                self.commit(path);
                self.later.push_back(Later::Portal(p - 1));
            }
        }
    }

    /// falls straight down from feet at (x, y) until something holds them up
    fn fall(&mut self, mut path: Vec<usize>, x: i64, mut y: i64) {
        while y < self.c.h {
            if !self.clear(x, y) {
                return;
            }
            self.visit(&mut path, x, y);
            let s = self.c.support(self.el, x, y + 1);
            if s == SAFE {
                self.commit(&path);
                self.add_stand(x, y);
                return;
            }
            if s == DEADLY {
                return;
            }
            y += 1;
        }
    }

    /// airborne at feet (x, y), drifting up to `carry` tiles one way, landing or falling from any column on the way
    fn drift(&mut self, base: &[usize], x: i64, y: i64, carry: i64) {
        for d in [-1, 1] {
            let mut path = base.to_vec();
            for k in 1..=carry {
                let cx = x + d * k;
                if !self.clear(cx, y) {
                    break;
                }
                self.visit(&mut path, cx, y);
                let s = self.c.support(self.el, cx, y + 1);
                if s == SAFE {
                    self.commit(&path);
                    self.add_stand(cx, y);
                    break;
                }
                if s == DEADLY {
                    break;
                }
                self.fall(path.clone(), cx, y + 1);
            }
        }
    }

    fn fan(&mut self, fi: usize) {
        let (fx, fy, fw, fh) = {
            let f = &self.c.fans[fi];
            (f.x, f.y, f.w, f.h)
        };
        for cx in fx..fx + fw {
            let mut path = vec![];
            // bottom up through the column, plus a tile of overshoot when it blows them out the top
            let mut r = fy;
            while r >= fy - fh {
                if !self.clear(cx, r) {
                    break;
                }
                self.visit(&mut path, cx, r);
                self.commit(&path);
                if self.c.support(self.el, cx, r + 1) == SAFE {
                    self.add_stand(cx, r);
                }
                self.drift(&path.clone(), cx, r, 3);
                r -= 1;
            }
        }
    }

    fn portal(&mut self, pi: usize) {
        if let Some(o) = self.c.portal_other[pi] {
            let (x, y, _) = self.c.portals[o];
            self.fall(vec![], x, y);
        }
    }
}

/// one player's reach on the current ice/thin state
fn reach(c: &Ctx, el: Element, start: (i64, i64), o: ReachOpts) -> (Vec<bool>, Vec<bool>) {
    let n = (c.w * c.h) as usize;
    let mut r = Run {
        c,
        el,
        stand: vec![false; n],
        touched: vec![false; n],
        fan_done: vec![false; c.fans.len()],
        portal_done: vec![false; c.portals.len()],
        queue: Default::default(),
        later: Default::default(),
    };
    r.fall(vec![], start.0, start.1);
    loop {
        while let Some(i) = r.queue.pop_front() {
            let x = i as i64 % c.w;
            let y = (i as i64 - x) / c.w;
            let mut here = vec![];
            r.visit(&mut here, x, y);
            r.commit(&here);
            // walk off either side
            for d in [-1, 1] {
                if r.clear(x + d, y) {
                    r.fall(vec![], x + d, y);
                }
            }
            // jump: rise a tiles, then drift sideways at that height
            let mut up = vec![];
            for a in 1..=o.jump {
                if !r.clear(x, y - a) {
                    break;
                }
                r.visit(&mut up, x, y - a);
                r.commit(&up);
                let carry = if a <= 2 {
                    o.carry
                } else if a == 3 {
                    2
                } else {
                    1
                };
                r.drift(&up.clone(), x, y - a, carry);
            }
        }
        match r.later.pop_front() {
            None => break,
            Some(Later::Fan(f)) => r.fan(f),
            Some(Later::Portal(p)) => r.portal(p),
        }
    }
    (r.stand, r.touched)
}

fn find(def: &StageDef, ch: char) -> Option<(i64, i64)> {
    def.map.iter().enumerate().find_map(|(y, row)| {
        row.chars()
            .position(|c| c == ch)
            .map(|x| (x as i64, y as i64))
    })
}

pub fn reachability(def: &StageDef, opts: ReachOpts) -> Reach {
    let mut c = build(def);
    let n = (c.w * c.h) as usize;
    let spawn = [find(def, '1'), find(def, '2')];
    let mut fire = (vec![false; n], vec![false; n]);
    let mut frost = (vec![false; n], vec![false; n]);
    let merge = |into: &mut (Vec<bool>, Vec<bool>), r: (Vec<bool>, Vec<bool>)| {
        for i in 0..n {
            into.0[i] |= r.0[i];
            into.1[i] |= r.1[i];
        }
    };
    let thin: Vec<usize> = (0..n)
        .filter(|&i| c.chr(i as i64 % c.w, i as i64 / c.w) == 'w')
        .collect();
    // fire melts what it can touch, frost freezes what it gets near; repeat until neither changes
    for _ in 0..50 {
        let mut changed = false;
        if let Some(s) = spawn[0] {
            merge(&mut fire, reach(&c, Element::Fire, s, opts));
        }
        for i in 0..n {
            if !fire.1[i] {
                continue;
            }
            let x = i as i64 % c.w;
            let y = (i as i64 - x) / c.w;
            for (dx, dy) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (xx, yy) = (x + dx, y + dy);
                if xx < 0 || yy < 0 || xx >= c.w || yy >= c.h {
                    continue;
                }
                let ice = c.ice_at[(yy * c.w + xx) as usize];
                if ice != 0 && !c.melted[ice - 1] {
                    c.melted[ice - 1] = true;
                    changed = true;
                }
            }
        }
        if let Some(s) = spawn[1] {
            merge(&mut frost, reach(&c, Element::Frost, s, opts));
        }
        for &t in &thin {
            if c.frozen[t] {
                continue;
            }
            let tx = t as i64 % c.w;
            let ty = (t as i64 - tx) / c.w;
            // frost's centre sits about mid-tile in its feet cell. only standing spots count
            for i in 0..n {
                if c.frozen[t] {
                    break;
                }
                if !frost.0[i] {
                    continue;
                }
                let x = i as i64 % c.w;
                let y = (i as i64 - x) / c.w;
                if crate::games::js_hypot((x - tx) as f64, (y - ty) as f64) <= 2.2 {
                    c.frozen[t] = true;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let mut gems = vec![];
    for (y, row) in def.map.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            if ch == 'r' {
                gems.push(MissedGem {
                    x: x as i64,
                    y: y as i64,
                    el: Element::Fire,
                });
            }
            if ch == 'b' {
                gems.push(MissedGem {
                    x: x as i64,
                    y: y as i64,
                    el: Element::Frost,
                });
            }
        }
    }
    let got = |g: &MissedGem| {
        (if g.el == Element::Fire { &fire } else { &frost }).1[(g.y * c.w + g.x) as usize]
    };
    let door = |ch: char, r: &(Vec<bool>, Vec<bool>)| {
        find(def, ch).is_some_and(|(x, y)| r.0[(y * c.w + x) as usize])
    };
    Reach {
        fire_door: door('R', &fire),
        frost_door: door('B', &frost),
        fire_gems: gems
            .iter()
            .filter(|g| g.el == Element::Fire && got(g))
            .count(),
        frost_gems: gems
            .iter()
            .filter(|g| g.el == Element::Frost && got(g))
            .count(),
        gem_totals: [
            gems.iter().filter(|g| g.el == Element::Fire).count(),
            gems.iter().filter(|g| g.el == Element::Frost).count(),
        ],
        missed_gems: gems.iter().filter(|g| !got(g)).cloned().collect(),
        stand: [fire.0.clone(), frost.0.clone()],
        touched: [fire.1.clone(), frost.1.clone()],
        melted: c.melted.iter().filter(|m| **m).count(),
        frozen: c.frozen.iter().filter(|m| **m).count(),
    }
}

/// the map with standing cells marked: '+' fire stands there, '-' frost, '=' both
pub fn render_reach(def: &StageDef, r: &Reach) -> String {
    def.map
        .iter()
        .enumerate()
        .map(|(y, row)| {
            let w = row.chars().count();
            row.chars()
                .enumerate()
                .map(|(x, ch)| {
                    if ch != '.' && ch != ' ' {
                        return ch;
                    }
                    let i = y * w + x;
                    match (r.stand[0][i], r.stand[1][i]) {
                        (true, true) => '=',
                        (true, false) => '+',
                        (false, true) => '-',
                        _ => ch,
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
