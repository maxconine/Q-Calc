//! ember & frost: `ember` then enter (components/EmberPanel.tsx, the Mac's EmberPanel.swift). The shared lobby picks
//! one keyboard or a room online; the core runs the game (`EmberGame`: the select screen, levels, clock, link) with
//! the panel's tracker (hud, banners, progress), and this feeds it keys and frames, draws with `Renderer` (draw.rs,
//! things.rs, players.rs over canvas.rs) and plays its sounds (sound.rs).

mod draw;
mod fx;
mod players;
mod sound;
mod things;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use qcalc_core::games::Side;
use qcalc_core::games::ember::engine::stage_of;
use qcalc_core::games::ember::panel::{GRID_COLS, Hud, PanelTracker, grid_step, win_lines};
use qcalc_core::games::ember::progress::{
    SAVE_KEY, finished_count, first_unfinished, parse_progress, progress_json,
};
use qcalc_core::games::ember::session::{EmberGame, GuestGame, HostGame, LocalGame, Screen};
use qcalc_core::games::ember::sound::{SoundGate, voices};
use qcalc_core::games::ember::{
    Catalog, EMBER_HINT, GameEvent, GameState, IN_DOWN, IN_LEFT, IN_RIGHT, IN_UP, Phase, fmt_par,
    fmt_time, is_ember_command, level_difficulty, level_length, shipped,
};
use qcalc_core::games::fullscreen::fullscreen_hint;
use qcalc_core::games::peer::{PeerEvent, close_reason_text};
use qcalc_core::games::peer_online::OnlinePeer;

use self::draw::{DrawInput, DrawOpts, Renderer};
use self::sound::Synth;
use super::canvas::{Canvas, Col, Offscreen, Res};
use super::gfx::{Align, Font, Hit, Rect, Ui};
use super::keys::GameKey;
use super::store::GameStore;
use super::theme::{Rgba, m};
use super::widgets as w;
use super::{Env, GameDef, Play};

pub static DEF: GameDef = GameDef {
    kind: "ember",
    title: "ember & frost",
    name: "ember & frost",
    blurb: "two of you, on one keyboard or two computers anywhere. both get to their doors. you’re ember if you host, frost if you join.",
    hint: EMBER_HINT,
    matches: is_ember_command,
    prompt: "",
    options: &[],
    together: &["same keyboard"],
    local: |_, env| Box::new(Ember::new(None, env)),
    online: Some(|role, peer, env| Box::new(Ember::new(Some((role, peer)), env))),
    sub: sub_line,
    footer: Some(controls),
    lobby_key: None,
    own_head: true,
};

fn fire(dark: bool) -> Rgba {
    if dark {
        Rgba::hex(0xff8a47, 1.0)
    } else {
        Rgba::hex(0xe8641c, 1.0)
    }
}

fn frost(dark: bool) -> Rgba {
    if dark {
        Rgba::hex(0x5fcdf2, 1.0)
    } else {
        Rgba::hex(0x1f93c6, 1.0)
    }
}

fn star(dark: bool) -> Rgba {
    if dark {
        Rgba::hex(0xffc44d, 1.0)
    } else {
        Rgba::hex(0xf0a81a, 1.0)
    }
}

/// the head's line: how far the saved progress has got
fn sub_line(store: &GameStore) -> String {
    let cat = shipped();
    let progress = parse_progress(store.get(SAVE_KEY));
    let ids: Vec<&str> = cat.defs.iter().map(|d| d.id.as_str()).collect();
    let done = finished_count(&progress, &ids);
    if done > 0 {
        format!("{done} of {} levels done", cat.len())
    } else {
        String::new()
    }
}

fn kbd(ui: &mut Ui, x: f32, y: f32, k: &str) -> f32 {
    let c = ui.c;
    let (tw, th) = ui.measure(k, Font::CAPTION);
    let bw = (tw + 6.0).max(18.0);
    let r = Rect::new(x + 2.0, y, bw, 18.0f32.max(th));
    ui.round(r, 4.0, c.bg_tape);
    ui.ring(r, 4.0, 1.0, c.line);
    ui.text(
        k,
        Font::CAPTION,
        r.x + (bw - tw) / 2.0,
        r.y + (r.h - th) / 2.0,
        c.ink,
    );
    bw + 2.0
}

/// EmberControls: under the lobby's choices, the levels and the keys
fn controls(ui: &mut Ui, x: f32, y: f32, width: f32, _env: &mut Env) -> f32 {
    let c = ui.c;
    let count = shipped().len();
    let text = format!(
        "{count} long levels, every one open: lights out, gravity upside down, trampolines, keys, and checkpoints on the way."
    );
    let tw = width.min(460.0);
    let mut h = ui.text_wrap(
        &text,
        Font::SMALL,
        x + (width - tw) / 2.0,
        y,
        tw,
        Align::Center,
        c.muted,
    ) + m::SP3;
    // one row: ember's keys, frost's, and the use key, measured then centred
    let groups: [(&str, Rgba, &[&str]); 2] = [
        ("ember ", fire(c.dark), &["W", "A", "S", "D"]),
        ("frost ", frost(c.dark), &["←", "↑", "→", "↓"]),
    ];
    let bold = Font::bold(m::FS_SMALL);
    let kw = |ui: &mut Ui, k: &str| (ui.measure(k, Font::CAPTION).0 + 6.0).max(18.0) + 2.0;
    let mut total = 0.0;
    for (name, _, keys) in &groups {
        total += ui.measure(name, bold).0;
        for k in *keys {
            total += kw(ui, k);
        }
        total += m::SP4;
    }
    let tail = " use a lever, lamp or switch";
    total += kw(ui, "S")
        + ui.measure(" / ", Font::SMALL).0
        + kw(ui, "↓")
        + ui.measure(tail, Font::SMALL).0;
    let mut xx = x + (width - total) / 2.0;
    let ry = y + h;
    let (_, th) = ui.measure("ember", bold);
    let ty = ry + (18.0 - th) / 2.0;
    for (name, col, keys) in &groups {
        xx += ui.text(name, bold, xx, ty, *col);
        for k in *keys {
            xx += kbd(ui, xx, ry, k);
        }
        xx += m::SP4;
    }
    xx += kbd(ui, xx, ry, "S");
    xx += ui.text(" / ", Font::SMALL, xx, ty, c.muted);
    xx += kbd(ui, xx, ry, "↓");
    ui.text(tail, Font::SMALL, xx, ty, c.muted);
    h += 18.0;
    h
}

/// the select screen's card for a level
#[derive(Clone, Debug)]
struct Card {
    name: String,
    stages: usize,
    difficulty: u32,
    length: String,
    par: f64,
    par_text: String,
    gem_total: i64,
    best: Option<Best>,
}

#[derive(Clone, Debug)]
struct Best {
    stars: i64,
    secs_text: Option<String>,
    gems: i64,
    deaths: Option<i64>,
}

fn cards(cat: &Catalog, tracker: &PanelTracker) -> Vec<Card> {
    let n = cat.len();
    cat.defs
        .iter()
        .enumerate()
        .map(|(k, d)| {
            let totals = cat.get(k).map_or([0, 0], |l| l.gem_totals);
            Card {
                name: d.name.clone(),
                stages: d.stages.len(),
                difficulty: level_difficulty(k, n),
                length: level_length(d),
                par: d.par,
                par_text: fmt_par(d.par),
                gem_total: totals[0] + totals[1],
                best: tracker
                    .progress
                    .levels
                    .get(&d.id)
                    .filter(|b| b.completed)
                    .map(|b| Best {
                        stars: b.stars,
                        secs_text: b.secs.map(fmt_time),
                        gems: b.gems,
                        deaths: b.deaths,
                    }),
            }
        })
        .collect()
}

const MOVE: [(&str, usize, i64); 8] = [
    ("KeyA", 0, IN_LEFT),
    ("KeyD", 0, IN_RIGHT),
    ("KeyW", 0, IN_UP),
    ("KeyS", 0, IN_DOWN),
    ("ArrowLeft", 1, IN_LEFT),
    ("ArrowRight", 1, IN_RIGHT),
    ("ArrowUp", 1, IN_UP),
    ("ArrowDown", 1, IN_DOWN),
];

fn move_of(code: &str) -> Option<(usize, i64)> {
    MOVE.iter().find(|m| m.0 == code).map(|m| (m.1, m.2))
}

/// A spring's step response (SwiftUI's interpolatingSpring): 0 → 1, overshooting as it settles.
fn spring(t: f64, stiffness: f64, damping: f64) -> f64 {
    if t <= 0.0 {
        return 0.0;
    }
    let w = stiffness.sqrt();
    let z = (damping / (2.0 * w)).min(0.999);
    let wd = w * (1.0 - z * z).sqrt();
    1.0 - (-z * w * t).exp() * ((wd * t).cos() + z / (1.0 - z * z).sqrt() * (wd * t).sin())
}

struct Ember {
    game: Box<dyn EmberGame>,
    cat: Arc<Catalog>,
    tracker: PanelTracker,
    gate: SoundGate,
    rand: u64,
    online: bool,
    peer: String,
    // what the last tick said
    screen: Screen,
    level: usize,
    me: Option<usize>,
    note: Option<String>,
    hud: Option<Hud>,
    muted: bool,
    state: Option<GameState>,
    prev: Option<GameState>,
    alpha: f64,
    events: Vec<GameEvent>,
    cards: Vec<Card>,
    // drawing
    renderer: Option<Renderer>,
    res: Option<Res>,
    thumbs: HashMap<(usize, bool), Offscreen>,
    synth: Synth,
    held: HashSet<String>,
    asked: Option<(usize, usize)>,
    done: bool,
    origin: Instant,
    /// when each moving part began: the banner, the cp chip, the cards, the full-screen tip
    banner_at: (u64, f64),
    chip_at: (String, f64),
    card_at: (String, f64),
    tip_at: (u64, f64),
    scroll: f32,
}

impl Ember {
    fn new(online: Option<(Side, String)>, env: &mut Env) -> Ember {
        let cat = shipped();
        let progress = parse_progress(env.store.get(SAVE_KEY));
        let ids: Vec<&str> = cat.defs.iter().map(|d| d.id.as_str()).collect();
        let start = first_unfinished(&progress, &ids) as f64;
        let now = env.now();
        let game: Box<dyn EmberGame> = match &online {
            None => Box::new(LocalGame::new(cat.clone(), now, start)),
            Some((Side::Host, _)) => Box::new(HostGame::new(cat.clone(), now, start)),
            Some(_) => Box::new(GuestGame::new(cat.clone(), now, start)),
        };
        let tracker = PanelTracker::new(online.is_some(), progress);
        let cards = cards(&cat, &tracker);
        let muted = tracker.progress.muted;
        let mut e = Ember {
            game,
            tracker,
            gate: SoundGate::default(),
            rand: (env.rand() * 9e15) as u64 | 1,
            online: online.is_some(),
            peer: online.map(|o| o.1).unwrap_or_default(),
            screen: Screen::Select,
            level: 0,
            me: None,
            note: None,
            hud: None,
            muted,
            state: None,
            prev: None,
            alpha: 0.0,
            events: Vec::new(),
            cards,
            cat,
            renderer: None,
            res: None,
            thumbs: HashMap::new(),
            synth: Synth::default(),
            held: HashSet::new(),
            asked: None,
            done: false,
            origin: Instant::now(),
            banner_at: (0, 0.0),
            chip_at: (String::new(), 0.0),
            card_at: (String::new(), 0.0),
            tip_at: (0, 0.0),
            scroll: 0.0,
        };
        e.screen = if e.online {
            Screen::Wait
        } else {
            Screen::Select
        };
        e.tick(env);
        e
    }

    fn secs(&self) -> f64 {
        self.origin.elapsed().as_secs_f64()
    }

    fn flush(&mut self, env: &mut Env) {
        env.sent.extend(self.game.take_outbox());
    }

    fn finish(&mut self, note: Option<String>, env: &mut Env) {
        if !self.done {
            self.done = true;
            self.synth.stop();
            env.end = Some(note.filter(|n| !n.is_empty()));
        }
    }

    /// One frame of the game: tick, send, save, sounds, and what to draw.
    fn tick(&mut self, env: &mut Env) {
        if self.done {
            return;
        }
        let now = env.now();
        let v = self.game.tick(now);
        self.flush(env);
        if let Some(ended) = v.ended.clone() {
            self.finish(Some(ended), env);
            return;
        }
        let hud = self.tracker.frame(&self.cat, &v);
        if self.tracker.progress_changed {
            env.store
                .set(SAVE_KEY, Some(&progress_json(&self.tracker.progress)));
            self.cards = cards(&self.cat, &self.tracker);
        }
        self.muted = self.tracker.progress.muted;
        if self.tracker.reset_renderer
            && let Some(r) = &mut self.renderer
        {
            r.reset();
        }
        if !self.muted {
            let mut list = Vec::new();
            for e in &v.events {
                if self.gate.pass(e, now) {
                    let s = &mut self.rand;
                    let mut r = || {
                        let mut x = *s;
                        x ^= x << 13;
                        x ^= x >> 7;
                        x ^= x << 17;
                        *s = x;
                        (x >> 11) as f64 / (1u64 << 53) as f64
                    };
                    list.extend(voices(e, &mut r));
                }
            }
            self.synth.play(&list);
        }
        let playing = v.playing && v.state.is_some();
        if playing {
            self.events.extend(v.events);
        } else {
            self.events.clear();
        }
        self.screen = v.screen;
        self.level = v.level;
        self.me = v.me;
        self.note = v.note;
        self.state = if playing { v.state } else { None };
        self.prev = v.prev;
        self.alpha = v.alpha;
        // the moving parts start when what they show changes
        let t = self.secs();
        if let Some(b) = hud.banner.as_ref()
            && b.id != self.banner_at.0
        {
            self.banner_at = (b.id, t);
        }
        let chip = format!("cp{}.{}", hud.stage, hud.cp);
        if chip != self.chip_at.0 {
            self.chip_at = (chip, t);
        }
        let card = format!("{:?}|{}", hud.phase, hud.stage);
        if card != self.card_at.0 {
            self.card_at = (card, t);
        }
        if hud.entry != self.tip_at.0 {
            self.tip_at = (hud.entry, t);
        }
        self.hud = Some(hud);
    }

    fn push(&mut self) {
        let mut bits = [0i64; 2];
        for code in &self.held {
            if let Some((p, b)) = move_of(code) {
                bits[p] |= b;
            }
        }
        if self.online {
            if let Some(me) = self.me {
                self.game.keys(me, bits[0] | bits[1]);
            }
        } else {
            self.game.keys(0, bits[0]);
            self.game.keys(1, bits[1]);
        }
    }

    fn pick(&mut self, dx: i64, dy: i64) {
        let level = self.level;
        let base = self.asked.filter(|a| a.0 == level).map_or(level, |a| a.1);
        let next = grid_step(
            base as i64,
            dx,
            dy,
            self.cards.len() as i64,
            GRID_COLS as i64,
        )
        .max(0) as usize;
        if next == base {
            return;
        }
        self.asked = Some((level, next));
        self.game.pick(next as f64);
    }

    fn choose(&mut self, i: usize) {
        self.game.pick(i as f64);
        self.game.play();
    }

    fn thumb(
        &mut self,
        g: &Canvas,
        res: &Res,
        i: usize,
        dark: bool,
        k: f64,
    ) -> Option<windows::Win32::Graphics::Direct2D::ID2D1Bitmap> {
        let key = (i, dark);
        if self
            .thumbs
            .get(&key)
            .is_some_and(|t| t.generation != g.generation || (t.w - 144.0 * k).abs() > 0.5)
        {
            self.thumbs.remove(&key);
        }
        if !self.thumbs.contains_key(&key) {
            let level = self.cat.get(i)?;
            let st = level.stages.first()?;
            let stages = level.stages.len();
            let mut off = g.offscreen(144.0 * k, 81.0 * k)?;
            off.draw(res, |c| players::draw_thumb(c, st, stages, dark));
            self.thumbs.insert(key, off);
        }
        self.thumbs.get_mut(&key)?.image()
    }

    // ---- drawing

    /// EmberHead: the name in its colours, the line beside it, full screen and esc at the right
    fn head(&self, ui: &mut Ui, x: f32, y: f32, width: f32, sub: &str, fs: bool) -> f32 {
        let c = ui.c;
        let bold = Font::bold(m::FS_BODY);
        let h = 22.0;
        let (_, nh) = ui.measure("ember", bold);
        let ty = y + (h - nh) / 2.0;
        let mut xx = x;
        xx += ui.text("ember", bold, xx, ty, fire(c.dark));
        xx += ui.text(" & ", Font::BODY, xx, ty, c.muted);
        xx += ui.text("frost", bold, xx, ty, frost(c.dark));
        let right = x + width + m::SP2;
        let (_, sh) = ui.measure("esc", Font::SMALL);
        let esc_w = w::esc_button(ui, right, y + (h - sh - 6.0) / 2.0, "esc", Hit::Esc);
        let fs_x = right - esc_w - 24.0;
        w::fullscreen_button(ui, fs_x, y + (h - 22.0) / 2.0, fs, false);
        if !sub.is_empty() {
            let sx = xx + m::SP3;
            ui.text_in(
                sub,
                Font::SMALL,
                sx,
                ty + 1.0,
                (fs_x - m::SP3 - sx).max(0.0),
                Align::Left,
                c.muted,
            );
        }
        h
    }

    fn stars(ui: &mut Ui, x: f32, y: f32, n: i64, size: f32) -> f32 {
        let c = ui.c;
        let f = Font::new(size);
        let mut xx = x;
        for i in 0..3 {
            let col = if i < n {
                star(c.dark)
            } else {
                c.muted.opacity(0.45)
            };
            xx += ui.text("★", f, xx, y, col) + 1.0;
        }
        xx - x
    }

    fn pips(ui: &mut Ui, x: f32, y: f32, n: u32) -> f32 {
        let c = ui.c;
        for i in 0..5 {
            let col = if i < n {
                fire(c.dark)
            } else {
                c.muted.opacity(0.3)
            };
            ui.round(Rect::new(x + i as f32 * 5.0, y, 3.0, 9.0), 1.0, col);
        }
        23.0
    }

    fn paint_select(&mut self, ui: &mut Ui, x: f32, y: f32, width: f32, env: &mut Env) -> f32 {
        let c = ui.c;
        let mut yy = y + m::SP2;
        let n = self.cards.len();
        let cols = GRID_COLS;
        let gap = m::SP1;
        let cw = (width - gap * (cols as f32 - 1.0)) / cols as f32;
        let thumb_h = (cw - 10.0) * 360.0 / 640.0;
        let card_h = 5.0 + thumb_h + 3.0 + 17.0 + 3.0 + 15.0 + 6.0;
        let rows = n.div_ceil(cols);
        let full_h = rows as f32 * card_h + (rows.saturating_sub(1)) as f32 * gap;
        let view_h = full_h.min(372.0);
        // keep the picked card in view
        let level = self.level;
        let row = (level / cols) as f32;
        let top = row * (card_h + gap);
        if top < self.scroll {
            self.scroll = top;
        } else if top + card_h > self.scroll + view_h {
            self.scroll = top + card_h - view_h;
        }
        self.scroll = self.scroll.clamp(0.0, (full_h - view_h).max(0.0));
        let grid = Rect::new(x, yy, width, view_h);
        let mut hovered: Option<usize> = None;
        if self.res.is_none() {
            self.res = Some(Res::new(ui));
        }
        let res = self.res.take();
        if let Some(res) = &res {
            ui.clip(grid);
            let k = (2.0 * (ui.px_per_dip.round()).max(1.0)).min(4.0) as f64;
            for (i, card) in self.cards.clone().iter().enumerate() {
                let (cx, cy) = (
                    x + (i % cols) as f32 * (cw + gap),
                    yy + (i / cols) as f32 * (card_h + gap) - self.scroll,
                );
                let r = Rect::new(cx, cy, cw, card_h);
                if r.bottom() < grid.y || r.y > grid.bottom() {
                    continue;
                }
                let on = i == level;
                let hover = ui.hover(r) && ui.hover(grid);
                if hover {
                    hovered = Some(i);
                }
                if on {
                    ui.round(r, m::RADIUS_S, c.fill_selected());
                } else if hover {
                    ui.round(r, m::RADIUS_S, c.fill_hover());
                }
                if on {
                    ui.ring(r, m::RADIUS_S, 1.5, fire(c.dark).opacity(0.55));
                }
                let tr = Rect::new(cx + 5.0, cy + 5.0, cw - 10.0, thumb_h);
                ui.round(tr, 4.0, c.bg.mix(0.6, c.bg_tape));
                {
                    let mut g = Canvas::on(ui, res, tr);
                    let img = self.thumb(&g, res, i, c.dark, k);
                    g.draw_image(img.as_ref(), 0.0, 0.0, g.width, g.height);
                    // the thumbnail's rounded corners
                    let rad = 4.0 * f64::from(ui.px_per_dip);
                    corners(
                        &mut g,
                        rad,
                        Col::rgba(0.0, 0.0, 0.0, 0.0),
                        rgba_col(if on {
                            c.bg.mix(0.92, c.ink)
                        } else if hover {
                            c.bg.mix(0.95, c.ink)
                        } else {
                            c.bg
                        }),
                    );
                    g.finish();
                }
                ui.sync_transform();
                ui.ring(tr, 4.0, 1.0, c.line);
                if card.stages > 1 {
                    let label = format!("{} stages", card.stages);
                    let f = Font::new(10.0);
                    let (lw, lh) = ui.measure(&label, f);
                    let pill = Rect::new(
                        tr.right() - lw - 10.0 - 3.0,
                        tr.bottom() - 15.0 - 3.0,
                        lw + 10.0,
                        15.0,
                    );
                    ui.round(pill, 7.5, c.bg.opacity(0.82));
                    ui.text(&label, f, pill.x + 5.0, pill.y + (15.0 - lh) / 2.0, c.ink);
                }
                let ny = tr.bottom() + 3.0;
                let num = format!("{}", i + 1);
                let fd = Font::digits(m::FS_SMALL, true);
                let nw = ui.text(&num, fd, cx + 5.0, ny, c.muted);
                ui.text_in(
                    &format!(" {}", card.name),
                    Font::digits(m::FS_SMALL, false),
                    cx + 5.0 + nw,
                    ny,
                    cw - 10.0 - nw,
                    Align::Left,
                    c.ink,
                );
                let sy = ny + 17.0 + 3.0;
                let mut sx = cx + 5.0;
                sx += Self::stars(
                    ui,
                    sx,
                    sy,
                    card.best.as_ref().map_or(0, |b| b.stars),
                    m::FS_CAPTION,
                ) + m::SP2;
                if let Some(t) = card.best.as_ref().and_then(|b| b.secs_text.clone()) {
                    ui.text(&t, Font::digits(m::FS_CAPTION, false), sx, sy, c.muted);
                }
                Self::pips(ui, cx + cw - 5.0 - 23.0, sy + 3.0, card.difficulty);
                ui.hit(r, Hit::Act(i as u32));
            }
            ui.unclip();
        }
        self.res = res;
        yy += view_h + m::SP2;
        let shown = hovered.unwrap_or(level);
        if let Some(card) = self.cards.get(shown).cloned() {
            yy += self.detail(ui, x, yy, width, shown, &card) + m::SP2;
        }
        let note = self
            .note
            .as_ref()
            .map(|n| format!("{n} · "))
            .unwrap_or_default();
        let you = match self.me {
            Some(0) => "you’re ember · ",
            Some(_) => "you’re frost · ",
            None => "",
        };
        let line = format!(
            "{note}{you}every level is open · arrows or WASD pick · ↵ play · M {} · {} · esc back",
            if self.muted { "unmute" } else { "mute" },
            fullscreen_hint(false, env.shell.fullscreen, true)
        );
        yy += w::note(ui, x + width / 2.0, yy, width, &line, false);
        yy - y
    }

    fn detail(&self, ui: &mut Ui, x: f32, y: f32, width: f32, i: usize, card: &Card) -> f32 {
        let c = ui.c;
        let f = Font::digits(m::FS_SMALL, false);
        let fb = Font::digits(m::FS_SMALL, true);
        // the parts, measured, then centred
        let mut parts: Vec<(String, Font, Rgba)> = vec![
            (format!("{}", i + 1), fb, c.muted),
            (format!(" {}", card.name), fb, c.ink),
        ];
        let mut tail: Vec<(String, Font, Rgba)> = vec![
            (card.length.clone(), f, c.muted),
            (format!("target {}", card.par_text), f, c.muted),
        ];
        let mut best_stars = None;
        match &card.best {
            Some(b) => {
                tail.push((
                    format!("best {}", b.secs_text.clone().unwrap_or_else(|| "–".into())),
                    f,
                    c.ink,
                ));
                best_stars = Some(b.stars);
                if card.gem_total > 0 {
                    tail.push((
                        format!("gems {}/{}", b.gems.min(card.gem_total), card.gem_total),
                        f,
                        c.muted,
                    ));
                }
                if let Some(d) = b.deaths {
                    tail.push((qcalc_core::games::ember::panel::deaths_text(d), f, c.muted));
                }
            }
            None => tail.push((
                format!(
                    "not finished yet{}",
                    if card.gem_total > 0 {
                        format!(" · {} gems", card.gem_total)
                    } else {
                        String::new()
                    }
                ),
                f,
                c.muted,
            )),
        }
        let gap = m::SP3;
        let mut total = 0.0;
        for p in &parts {
            total += ui.measure(&p.0, p.1).0;
        }
        total += gap + 23.0;
        for (k, p) in tail.iter().enumerate() {
            total += gap + ui.measure(&p.0, p.1).0;
            if k == 2 && best_stars.is_some() {
                total += 4.0 + 3.0 * 10.0;
            }
        }
        let mut xx = x + ((width - total) / 2.0).max(0.0);
        let (_, th) = ui.measure("Ag", f);
        for p in parts.drain(..) {
            xx += ui.text(&p.0, p.1, xx, y, p.2);
        }
        xx += gap;
        Self::pips(ui, xx, y + (th - 9.0) / 2.0 + 1.0, card.difficulty);
        xx += 23.0;
        for (k, p) in tail.iter().enumerate() {
            xx += gap;
            xx += ui.text(&p.0, p.1, xx, y, p.2);
            if k == 2
                && let Some(s) = best_stars
            {
                xx += 4.0;
                xx += Self::stars(ui, xx, y, s, m::FS_CAPTION);
            }
        }
        th.max(18.0)
    }

    fn chip(&self, ui: &mut Ui, x: f32, y: f32, text: &str, on: bool, pop: f64) -> f32 {
        let c = ui.c;
        let f = Font::digits(m::FS_CAPTION, false);
        let (tw, th) = ui.measure(text, f);
        let r = Rect::new(x, y, tw + 12.0, 18.0);
        let k = 1.0 + 0.25 * pop as f32;
        let cx = r.x + r.w / 2.0;
        let cy = r.y + r.h / 2.0;
        ui.push_transform(k, cx - cx * k, cy - cy * k);
        ui.round(
            r,
            9.0,
            if on {
                c.accent.opacity(0.16)
            } else {
                c.fill_track()
            },
        );
        ui.text(
            text,
            f,
            r.x + 6.0,
            r.y + (18.0 - th) / 2.0,
            if on { c.accent } else { c.muted },
        );
        ui.pop_transform();
        r.w
    }

    fn paint_play(&mut self, ui: &mut Ui, x: f32, y: f32, width: f32, env: &mut Env) -> f32 {
        let c = ui.c;
        let dark = c.dark;
        let t = self.secs();
        let mut yy = y;
        let card = self.cards.get(self.level).cloned();
        let hud = self.hud.clone();
        // the hud
        if let Some(h) = &hud {
            let f = Font::SMALL;
            let fb = Font::bold(m::FS_SMALL);
            let row = 20.0;
            let (_, th) = ui.measure("Ag", f);
            let ty = yy + (row - th) / 2.0;
            // the clock at the right first, so the rest knows its room
            let fd = Font::digits(m::FS_SMALL, true);
            let par = if card.is_some() {
                format!(" / {}", fmt_par(card.as_ref().map_or(0.0, |c| c.par)))
            } else {
                String::new()
            };
            let (cw1, _) = ui.measure(&h.clock, fd);
            let (cw2, _) = ui.measure(&par, Font::digits(m::FS_SMALL, false));
            let clock_x = x + width - cw1 - cw2;
            ui.text(
                &h.clock,
                fd,
                clock_x,
                ty,
                if h.over { c.muted } else { c.ink },
            );
            ui.text(
                &par,
                Font::digits(m::FS_SMALL, false),
                clock_x + cw1,
                ty,
                c.muted,
            );
            let mut xx = x;
            let num = format!("{}", self.level + 1);
            xx += ui.text(&num, fb, xx, ty, c.muted);
            let name = format!(
                " {}",
                self.cat
                    .defs
                    .get(self.level)
                    .map_or("", |d| d.name.as_str())
            );
            let (nw, _) = ui.measure(&name, f);
            let nw = nw.min(220.0);
            ui.text_in(&name, f, xx, ty, nw, Align::Left, c.ink);
            xx += nw + m::SP3;
            let cy = yy + (row - 18.0) / 2.0;
            if h.stages > 1 {
                xx += self.chip(
                    ui,
                    xx,
                    cy,
                    &format!("stage {}/{}", h.stage + 1, h.stages),
                    false,
                    0.0,
                ) + m::SP3;
            }
            if h.cps > 0 {
                let on = h.cp >= 0;
                let e = t - self.chip_at.1;
                let pop = if !on {
                    0.0
                } else if e < 0.21 {
                    1.0 - (1.0 - e / 0.21).powi(2)
                } else if e < 0.7 {
                    let u = (e - 0.21) / 0.49;
                    ui.animate();
                    1.0 - u * u * (3.0 - 2.0 * u)
                } else {
                    0.0
                };
                if on && e < 0.7 {
                    ui.animate();
                }
                xx += self.chip(
                    ui,
                    xx,
                    cy,
                    &format!("⚑ {}/{}", (h.cp + 1).max(0), h.cps),
                    on,
                    pop,
                ) + m::SP3;
            }
            let total = h.total[0] + h.total[1];
            if total > 0 {
                xx += gems(ui, xx, yy, row, h.gems, h.total, dark) + m::SP3;
            }
            if h.keys > 0 {
                let sc = star(dark);
                key_glyph(ui, xx, yy + (row - 9.0) / 2.0, sc);
                xx += 15.0 + 3.0;
                if h.keys > 1 {
                    xx += ui.text(&format!("×{}", h.keys), f, xx, ty, sc);
                }
                xx += m::SP3;
            }
            if h.deaths > 0 {
                xx += ui.text(
                    &qcalc_core::games::ember::panel::deaths_text(h.deaths),
                    f,
                    xx,
                    ty,
                    c.muted,
                ) + m::SP3;
            }
            if let Some(me) = self.me {
                let (word, col) = if me == 0 {
                    ("you’re ember", fire(dark))
                } else {
                    ("you’re frost", frost(dark))
                };
                ui.text(word, fb, xx, ty, col);
            }
            yy += row + m::SP2;
        }
        // the picture: as wide as there's room for, or in full screen as tall as leaves room for the rest
        let (mut cw, mut ch) = (width, width * 360.0 / 640.0);
        if let Some(s) = env.stage {
            let max_h = (s.game_h - 20.0 - 2.0 * m::SP2 - 18.0 - 2.0 * m::SP3).max(120.0);
            if ch > max_h {
                ch = max_h;
                cw = ch * 640.0 / 360.0;
            }
        }
        let area = Rect::new(x + (width - cw) / 2.0, yy, cw, ch);
        ui.round(area, m::RADIUS_S, c.bg.mix(0.7, c.bg_tape));
        if self.res.is_none() {
            self.res = Some(Res::new(ui));
        }
        if self.renderer.is_none() {
            self.renderer = Some(Renderer::new(dark));
        }
        let res = self.res.take();
        if let (Some(res), Some(state)) = (&res, self.state.clone()) {
            let level_def = self.cat.get(self.level);
            if let Some(level) = level_def {
                let stage = stage_of(level, state.stage);
                let events = std::mem::take(&mut self.events);
                let prev = self.prev.clone();
                ui.clip(area);
                {
                    let mut g = Canvas::on(ui, res, area);
                    if let Some(r) = &mut self.renderer {
                        r.refresh_colors(dark);
                        let input = DrawInput {
                            level: stage,
                            prev: prev.as_ref(),
                            cur: &state,
                            alpha: self.alpha,
                            events: &events,
                            now_ms: env.now(),
                            opts: DrawOpts {
                                me: self.me,
                                reduce_motion: false,
                            },
                        };
                        r.draw(&mut g, &input);
                    }
                    let rad = f64::from(m::RADIUS_S * ui.px_per_dip);
                    corners(&mut g, rad, Col::CLEAR, rgba_col(c.bg));
                    g.finish();
                }
                ui.unclip();
                ui.sync_transform();
            }
        }
        self.res = res;
        ui.ring(area, m::RADIUS_S, 1.0, c.line);
        if let Some(h) = &hud {
            self.overlays(ui, area, h, card.as_ref(), env);
        }
        yy += ch + m::SP2;
        let line = format!(
            "{} · S/↓ use · R checkpoint · ⇧R restart level · M {} · {}{}",
            if self.online {
                "WASD or arrows"
            } else {
                "ember WASD · frost arrows"
            },
            if self.muted { "unmute" } else { "mute" },
            fullscreen_hint(false, env.shell.fullscreen, true),
            if env.shell.fullscreen {
                ""
            } else {
                " · esc levels"
            }
        );
        let (_, lh) = ui.measure(&line, Font::SMALL);
        ui.text_in(&line, Font::SMALL, x, yy, width, Align::Center, c.muted);
        yy += lh;
        yy - y
    }

    fn overlays(&mut self, ui: &mut Ui, area: Rect, h: &Hud, card: Option<&Card>, env: &mut Env) {
        let c = ui.c;
        let t = self.secs();
        // the banner: in at the top, held, then gone (4.2 s; 2.4 s for a flag or the way back; a death's stays)
        if h.show_banner
            && let Some(b) = &h.banner
            && b.id > 0
        {
            use qcalc_core::games::ember::panel::BannerKind;
            let e = t - self.banner_at.1;
            let (o, dy) = if b.kind == BannerKind::Ouch {
                let k = (e / 0.18).min(1.0);
                (k, 4.0 * (1.0 - k))
            } else {
                let d = if matches!(b.kind, BannerKind::Flag | BannerKind::Back) {
                    2.4
                } else {
                    4.2
                };
                let p = e / d;
                if p < 0.08 {
                    let k = p / 0.08;
                    (k, -6.0 * (1.0 - k))
                } else if p < 0.75 {
                    (1.0, 0.0)
                } else {
                    ((1.0 - (p - 0.75) / 0.25).max(0.0), 0.0)
                }
            };
            if o > 0.0 {
                ui.animate();
                let flag = b.kind == BannerKind::Flag;
                let fbase = if flag {
                    Font::bold(m::FS_BODY)
                } else {
                    Font::BODY
                };
                let mut runs: Vec<(String, Font, Rgba)> = Vec::new();
                let ink = if flag { c.accent } else { c.ink };
                if flag {
                    runs.push(("⚑ ".into(), fbase, ink));
                }
                if let Some(title) = &b.title {
                    runs.push((title.clone(), Font::bold(m::FS_BODY), ink));
                    if !b.text.is_empty() {
                        runs.push((" · ".into(), fbase, c.muted));
                    }
                }
                runs.push((b.text.clone(), fbase, ink));
                let was = ui.set_alpha(o as f32);
                pill_runs(
                    ui,
                    area.x + area.w / 2.0,
                    area.y + m::SP3 + dy as f32,
                    area.w - 2.0 * m::SP3,
                    &runs,
                    true,
                );
                ui.set_alpha(was);
            }
        }
        let phase = h.phase;
        if phase == Some(Phase::Clear) {
            let e = t - self.card_at.1;
            let title = format!("stage {} clear", h.stage + 1);
            let note = if h.stage + 2 >= h.stages {
                "the last stage is next".to_string()
            } else {
                format!("stage {} of {} is next", h.stage + 2, h.stages)
            };
            self.win_card(
                ui,
                area,
                e,
                &title,
                None,
                &[],
                &[(note, Font::SMALL, c.muted)],
            );
        }
        if phase == Some(Phase::Won)
            && let (Some(sc), Some(card)) = (&h.score, card)
        {
            let e = t - self.card_at.1;
            let par = self.cat.defs.get(self.level).map_or(card.par, |d| d.par);
            let (_, new, foot) = win_lines(
                sc,
                par,
                h.deaths,
                h.fresh.as_ref(),
                self.level + 1 >= self.cat.len(),
            );
            let got = sc.gems[0] + sc.gems[1];
            let all = sc.total[0] + sc.total[1];
            let fd = Font::digits(m::FS_BODY, false);
            let fdb = Font::digits(m::FS_BODY, true);
            let fast = sc.secs <= par;
            let mut line: Vec<(String, Font, Rgba)> = vec![
                (
                    fmt_time(sc.secs),
                    if fast { fdb } else { fd },
                    if fast { c.accent } else { c.ink },
                ),
                (format!(" / {}", card.par_text), fd, c.muted),
            ];
            if all > 0 {
                line.push((" · ".into(), fd, c.ink));
                line.push((
                    format!("gems {got}/{all}"),
                    if got == all { fdb } else { fd },
                    if got == all { c.accent } else { c.ink },
                ));
            }
            line.push((
                format!(
                    " · {}",
                    qcalc_core::games::ember::panel::deaths_text(h.deaths)
                ),
                fd,
                c.ink,
            ));
            let mut rest: Vec<(String, Font, Rgba)> = Vec::new();
            if let Some(n) = new {
                rest.push((n, Font::bold(m::FS_SMALL), c.accent));
            }
            rest.push((foot, Font::SMALL, c.muted));
            self.win_card(ui, area, e, "both home", Some(sc.stars), &line, &rest);
        }
        if let Some(note) = &self.note {
            let (_, th) = ui.measure(note, Font::SMALL);
            pill_runs(
                ui,
                area.x + area.w / 2.0,
                area.bottom() - m::SP3 - th - 10.0,
                area.w - 2.0 * m::SP3,
                &[(note.clone(), Font::SMALL, c.muted)],
                true,
            );
        }
        // the full-screen tip in the corner: there for a few seconds, then gone
        let e = t - self.tip_at.1;
        let o = if e < 5.0 {
            1.0
        } else {
            1.0 - ((e - 5.0) / 0.6).min(1.0)
        };
        if o > 0.0 {
            if o < 1.0 || e < 5.0 {
                ui.animate();
            }
            let tip = fullscreen_hint(false, env.shell.fullscreen, true);
            let f = Font::CAPTION;
            let (tw, th) = ui.measure(&tip, f);
            let r = Rect::new(
                area.right() - m::SP2 - tw - 2.0 * m::SP2,
                area.bottom() - m::SP2 - th - 4.0,
                tw + 2.0 * m::SP2,
                th + 4.0,
            );
            let was = ui.set_alpha(o as f32);
            ui.round(r, r.h / 2.0, c.bg.opacity(0.78));
            ui.text(&tip, f, r.x + m::SP2, r.y + 2.0, c.muted);
            ui.set_alpha(was);
        }
    }

    /// EmberCard: springs in; a gradient title, the stars, a line, and the rest
    #[allow(clippy::too_many_arguments)]
    fn win_card(
        &self,
        ui: &mut Ui,
        area: Rect,
        e: f64,
        title: &str,
        stars: Option<i64>,
        line: &[(String, Font, Rgba)],
        rest: &[(String, Font, Rgba)],
    ) {
        let c = ui.c;
        let k = spring(e, 260.0, 18.0);
        if e < 2.0 {
            ui.animate();
        }
        let tf = Font::bold(18.0);
        let (ttw, tth) = ui.measure(title, tf);
        let line_w: f32 = line.iter().map(|p| ui.measure(&p.0, p.1).0).sum();
        let line_h = if line.is_empty() {
            0.0
        } else {
            ui.measure("Ag", Font::digits(m::FS_BODY, false)).1
        };
        let rest_sizes: Vec<(f32, f32)> = rest.iter().map(|p| ui.measure(&p.0, p.1)).collect();
        let star_h = if stars.is_some() { 40.0 } else { 0.0 };
        let mut wdt = ttw
            .max(line_w)
            .max(rest_sizes.iter().map(|s| s.0).fold(0.0, f32::max))
            .max(if stars.is_some() {
                3.0 * 30.0 + 12.0
            } else {
                0.0
            });
        wdt = (wdt + 2.0 * m::SP5).max(260.0).min(area.w - 16.0);
        let mut ht = m::SP4 + tth;
        if stars.is_some() {
            ht += m::SP2 + star_h;
        }
        if !line.is_empty() {
            ht += m::SP2 + line_h;
        }
        for s in &rest_sizes {
            ht += m::SP2 + s.1;
        }
        ht += m::SP4;
        let r = Rect::new(
            area.x + (area.w - wdt) / 2.0,
            area.y + (area.h - ht) / 2.0,
            wdt,
            ht,
        );
        let scale = 0.9 + 0.1 * k as f32;
        let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        let was = ui.set_alpha((k as f32).clamp(0.0, 1.0));
        ui.push_transform(scale, cx - cx * scale, cy - cy * scale);
        // a soft drop under the card
        for i in 0..4 {
            let g = 2.0 + i as f32 * 3.0;
            ui.round(
                Rect::new(r.x - g / 2.0, r.y + 6.0, r.w + g, r.h + g / 2.0),
                10.0 + g,
                Rgba::hex(0x000000, 0.03),
            );
        }
        ui.card_shadow(r, 10.0);
        ui.round(r, 10.0, c.bg.opacity(0.92));
        let mut y = r.y + m::SP4;
        gradient_text(ui, title, tf, cx, y, fire(c.dark), frost(c.dark));
        y += tth;
        if let Some(n) = stars {
            y += m::SP2;
            // the stars pop in one by one
            let f = Font::new(30.0);
            let (sw, sh) = ui.measure("★", f);
            let total = 3.0 * sw + 2.0 * 6.0;
            let mut sx = cx - total / 2.0;
            for i in 0..3 {
                let got = i < n;
                let col = if got {
                    star(c.dark)
                } else {
                    c.muted.opacity(0.45)
                };
                let p = if got {
                    spring(e - 0.18 - f64::from(i as i32) * 0.16, 300.0, 12.0)
                } else {
                    1.0
                };
                if got && p < 1.2 && e < 3.0 {
                    ui.animate();
                }
                let s = (0.2 + 0.8 * p as f32).max(0.0);
                let a = (p as f32).clamp(0.0, 1.0);
                let (scx, scy) = (sx + sw / 2.0, y + star_h / 2.0);
                let inner = ui.set_alpha(a * k as f32);
                ui.push_rotation(-30.0 * (1.0 - p as f32), scx, scy);
                ui.push_transform(s, scx - scx * s, scy - scy * s);
                ui.text("★", f, sx, scy - sh / 2.0, col);
                ui.pop_transform();
                ui.pop_transform();
                ui.set_alpha(inner);
                sx += sw + 6.0;
            }
            y += star_h;
        }
        if !line.is_empty() {
            y += m::SP2;
            let mut lx = cx - line_w / 2.0;
            for p in line {
                lx += ui.text(&p.0, p.1, lx, y, p.2);
            }
            y += line_h;
        }
        for (p, s) in rest.iter().zip(&rest_sizes) {
            y += m::SP2;
            ui.text(&p.0, p.1, cx - s.0 / 2.0, y, p.2);
            y += s.1;
        }
        ui.pop_transform();
        ui.set_alpha(was);
    }
}

fn rgba_col(c: Rgba) -> Col {
    Col::rgba(
        f64::from(c.r) * 255.0,
        f64::from(c.g) * 255.0,
        f64::from(c.b) * 255.0,
        f64::from(c.a),
    )
}

/// The four corners outside a rounded rect of radius `r`, filled with `outside` (the window's background), so a
/// canvas drawn under an axis-aligned clip ends up with rounded corners.
fn corners(g: &mut Canvas, r: f64, _inside: Col, outside: Col) {
    let (w, h) = (g.width, g.height);
    let pi = std::f64::consts::PI;
    g.reset_transform();
    g.set_alpha(1.0);
    g.fill_style(outside);
    for (x0, y0, cx, cy, a0, a1) in [
        (0.0, 0.0, r, r, -pi / 2.0, -pi),
        (w, 0.0, w - r, r, 0.0, -pi / 2.0),
        (w, h, w - r, h - r, pi / 2.0, 0.0),
        (0.0, h, r, h - r, pi, pi / 2.0),
    ] {
        g.begin_path();
        g.move_to(x0, y0);
        let (sx, sy) = (cx + r * a0.cos(), cy + r * a0.sin());
        g.line_to(sx, sy);
        g.arc(cx, cy, r, a0, a1, true);
        g.close_path();
        g.fill();
    }
}

/// A capsule of runs centred on cx with its top at y (the banner, the note).
fn pill_runs(
    ui: &mut Ui,
    cx: f32,
    y: f32,
    max_w: f32,
    runs: &[(String, Font, Rgba)],
    shadow: bool,
) {
    let c = ui.c;
    let sizes: Vec<(f32, f32)> = runs.iter().map(|p| ui.measure(&p.0, p.1)).collect();
    let tw: f32 = sizes.iter().map(|s| s.0).sum();
    let th = sizes.iter().map(|s| s.1).fold(0.0, f32::max);
    let w = (tw + 2.0 * m::SP3).min(max_w);
    let r = Rect::new(cx - w / 2.0, y, w, th + 10.0);
    if shadow {
        ui.card_shadow(r, r.h / 2.0);
    }
    ui.round(r, r.h / 2.0, c.bg);
    ui.clip(Rect::new(
        r.x + m::SP3,
        r.y,
        (w - 2.0 * m::SP3).max(0.0),
        r.h,
    ));
    let mut x = r.x + m::SP3;
    for (p, s) in runs.iter().zip(&sizes) {
        x += ui.text(&p.0, p.1, x, r.y + 5.0 + (th - s.1), p.2);
    }
    ui.unclip();
}

/// The win card's title: bold, its letters going from fire to frost.
fn gradient_text(ui: &mut Ui, text: &str, f: Font, cx: f32, y: f32, a: Rgba, b: Rgba) {
    let (tw, _) = ui.measure(text, f);
    let mut x = cx - tw / 2.0;
    let chars: Vec<char> = text.chars().collect();
    let mut buf = [0u8; 4];
    for ch in chars.iter() {
        let s = ch.encode_utf8(&mut buf);
        let (cw, _) = ui.measure(s, f);
        let k = ((x + cw / 2.0 - (cx - tw / 2.0)) / tw.max(1.0)).clamp(0.0, 1.0);
        ui.text(s, f, x, y, b.mix(k, a));
        x += cw;
    }
}

/// GemGlyph: a little diamond per gem, filled when it's had
fn gem_glyph(ui: &mut Ui, x: f32, cy: f32, col: Rgba, got: bool) {
    let r = Rect::new(x, cy - 3.5, 7.0, 7.0);
    let was = ui.set_alpha(if got { 1.0 } else { 0.55 });
    ui.push_rotation(45.0, x + 3.5, cy);
    if got {
        ui.round(r, 1.0, col);
    }
    ui.ring(r, 1.0, 1.5, col);
    ui.pop_transform();
    ui.set_alpha(was);
}

fn gems(ui: &mut Ui, x: f32, y: f32, row: f32, got: [i64; 2], total: [i64; 2], dark: bool) -> f32 {
    let (f, fr) = (fire(dark), frost(dark));
    let cy = y + row / 2.0;
    let mut xx = x;
    if total[0] + total[1] <= 8 {
        for i in 0..total[0] {
            gem_glyph(ui, xx, cy, f, i < got[0]);
            xx += 10.0;
        }
        for i in 0..total[1] {
            gem_glyph(ui, xx, cy, fr, i < got[1]);
            xx += 10.0;
        }
        return xx - x - 3.0;
    }
    let fd = Font::digits(m::FS_SMALL, false);
    let (_, th) = ui.measure("0", fd);
    for (k, col) in [(0usize, f), (1, fr)] {
        if total[k] > 0 {
            gem_glyph(ui, xx, cy, col, got[k] == total[k]);
            xx += 7.0 + 5.0;
            xx += ui.text(
                &format!("{}/{}", got[k], total[k]),
                fd,
                xx,
                cy - th / 2.0,
                col,
            ) + m::SP2;
        }
    }
    xx - x
}

/// KeyGlyph: a ring, a shank and a bit, 15 × 9
fn key_glyph(ui: &mut Ui, x: f32, y: f32, col: Rgba) {
    ui.circle_stroke(x + 4.5, y + 4.5, 2.5, 2.0, col);
    ui.round(Rect::new(x + 7.0, y + 3.5, 8.0, 2.0), 1.0, col);
    ui.fill(Rect::new(x + 11.5, y + 5.0, 2.0, 2.5), col);
}

impl Play for Ember {
    fn paint(&mut self, ui: &mut Ui, x: f32, y: f32, width: f32, env: &mut Env) -> f32 {
        self.tick(env);
        if self.done {
            return 0.0;
        }
        let sub = if self.online {
            format!("with {}", self.peer)
        } else {
            "same keyboard".to_string()
        };
        match self.screen {
            Screen::Play => {
                ui.animate();
                self.paint_play(ui, x, y, width, env)
            }
            Screen::Select => {
                let h = self.head(ui, x, y, width, &sub, env.shell.fullscreen);
                h + self.paint_select(ui, x, y + h, width, env)
            }
            Screen::Wait => {
                let mut h = self.head(
                    ui,
                    x,
                    y,
                    width,
                    if self.online { &sub } else { "" },
                    env.shell.fullscreen,
                );
                let text = if self.online {
                    format!("waiting for {}…", self.peer)
                } else {
                    "getting ready…".to_string()
                };
                let (_, th) = ui.measure(&text, Font::BODY);
                let a = w::wait_alpha(ui);
                let was = ui.set_alpha(a);
                let c = ui.c;
                ui.text_center(
                    &text,
                    Font::BODY,
                    x + width / 2.0,
                    y + h + (168.0 - th) / 2.0,
                    c.muted,
                );
                ui.set_alpha(was);
                h += 168.0;
                h
            }
        }
    }

    fn key(&mut self, k: &GameKey, env: &mut Env) -> bool {
        if k.key == "Escape" {
            return false;
        }
        let mv = move_of(&k.code);
        if !k.down {
            if !self.held.remove(&k.code) {
                return false;
            }
            self.push();
            return true;
        }
        if !k.plain() && !(mv.is_some() && self.held.contains(&k.code)) {
            return false;
        }
        let screen = self.screen;
        if mv.is_some() && !self.held.contains(&k.code) {
            self.held.insert(k.code.clone());
            self.push();
        }
        let bar_key =
            k.is_char() || k.key == "Enter" || k.key.starts_with("Arrow") || k.key == "Backspace";
        if screen == Screen::Select {
            if let Some((_, b)) = mv {
                match b {
                    IN_LEFT => self.pick(-1, 0),
                    IN_RIGHT => self.pick(1, 0),
                    IN_UP => self.pick(0, -1),
                    _ => self.pick(0, 1),
                }
            } else if k.key == "Home" {
                self.pick(-(self.cards.len() as i64), 0);
            } else if k.key == "End" {
                self.pick(self.cards.len() as i64, 0);
            } else if (k.key == "Enter" || k.key == " ") && !k.repeat {
                self.game.play();
            } else if k.lower() == "m" && !k.repeat {
                self.tracker.toggle_mute();
            }
            self.flush(env);
            return true;
        }
        if screen != Screen::Play {
            return bar_key;
        }
        if mv.is_some() {
            self.flush(env);
            return true;
        }
        if !bar_key {
            return false;
        }
        if k.repeat {
            return true;
        }
        let key = k.lower();
        if key == "r" {
            if k.shift {
                self.game.restart();
            } else {
                self.game.retry();
            }
        } else if key == "m" {
            self.tracker.toggle_mute();
        } else if (k.key == "Enter" || k.key == " ")
            && self.hud.as_ref().and_then(|h| h.phase) == Some(Phase::Won)
        {
            self.game.next();
        }
        self.flush(env);
        true
    }

    fn act(&mut self, id: u32, env: &mut Env) {
        if self.screen == Screen::Select {
            self.choose(id as usize);
            self.flush(env);
        }
    }

    fn heard(&mut self, e: &PeerEvent, env: &mut Env) {
        match e {
            PeerEvent::Message { data } => {
                self.game.receive(data, env.now());
                self.flush(env);
            }
            PeerEvent::Closed { reason } => {
                self.finish(Some(close_reason_text(*reason, OnlinePeer::PEER_WORD)), env)
            }
            _ => {}
        }
    }

    /// online, off the play screen: a slow tick keeps the link moving (the play screen ticks every frame)
    fn ticks(&self) -> bool {
        self.online && !self.done && self.screen != Screen::Play
    }

    fn tick(&mut self, env: &mut Env) {
        Ember::tick(self, env);
    }

    fn idle(&self) -> bool {
        self.screen != Screen::Play
    }

    fn leave(&mut self, env: &mut Env) -> bool {
        if self.screen != Screen::Play {
            return false;
        }
        self.game.menu();
        self.flush(env);
        true
    }

    fn release(&mut self) {
        self.held.clear();
        self.game.release();
    }

    fn closing(&mut self, env: &mut Env) {
        self.synth.stop();
        if !self.done {
            self.done = true;
            self.game.leave();
            self.flush(env);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_names_ember_and_the_keys_map() {
        assert!((DEF.matches)("ember"));
        assert!((DEF.matches)("ember & frost"));
        assert!((DEF.matches)("fire and ice"));
        assert!(!(DEF.matches)("embers"));
        assert_eq!(move_of("KeyA"), Some((0, IN_LEFT)));
        assert_eq!(move_of("ArrowDown"), Some((1, IN_DOWN)));
        assert_eq!(move_of("KeyQ"), None);
        // a spring starts at 0, overshoots, and settles at 1
        assert_eq!(spring(0.0, 260.0, 18.0), 0.0);
        assert!(spring(0.2, 300.0, 12.0) > 1.0);
        assert!((spring(3.0, 260.0, 18.0) - 1.0).abs() < 0.01);
    }
}
