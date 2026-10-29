// emberSession.test.ts: the sessions on small levels of their own. 0 open floor, 1 a water pit right of ember,
// 2 two stages (the doors a step away, then a checkpoint flag at column 8 and a water pit from column 14),
// 5 and 6 each player one step left of their door with a wall behind it (3 and 4 stand in for the tile tests)

use std::sync::Arc;

use super::*;
use crate::games::ember::protocol::decode_ember;

fn row(cells: &[(usize, char)]) -> String {
    let mut s = String::from("#");
    for c in 1..31 {
        s.push(
            cells
                .iter()
                .find(|(x, _)| *x == c)
                .map_or('.', |(_, ch)| *ch),
        );
    }
    s + "#"
}

fn map(r16: &[(usize, char)], floor: &str) -> Vec<String> {
    let mut m = vec!["#".repeat(32)];
    for _ in 0..15 {
        m.push(row(&[]));
    }
    m.push(row(r16));
    m.push(floor.to_string());
    m
}

fn stage(map: Vec<String>) -> StageDef {
    StageDef {
        name: None,
        hint: None,
        dark: None,
        map,
        things: vec![],
    }
}

fn level(id: &str, par: f64, stages: Vec<StageDef>) -> LevelDef {
    LevelDef {
        id: id.into(),
        name: id.into(),
        hint: None,
        par,
        stages,
    }
}

fn catalog() -> Arc<Catalog> {
    let wall = "#".repeat(32);
    let win: &[(usize, char)] = &[
        (1, '1'),
        (2, 'R'),
        (3, '#'),
        (27, '#'),
        (28, '2'),
        (29, 'B'),
        (30, '#'),
    ];
    Arc::new(Catalog::new(vec![
        level(
            "t-open",
            20.0,
            vec![stage(map(
                &[(2, '1'), (13, 'R'), (24, 'B'), (27, '2')],
                &wall,
            ))],
        ),
        level(
            "t-pit",
            20.0,
            vec![stage(map(
                &[(2, '1'), (20, 'R'), (26, 'B'), (29, '2')],
                &format!("####WWWW{}", "#".repeat(24)),
            ))],
        ),
        level(
            "t-two",
            60.0,
            vec![
                stage(map(win, &wall)),
                stage(map(
                    &[(2, '1'), (4, '2'), (8, 'C'), (27, 'R'), (29, 'B')],
                    &format!("{}WWWW{}", "#".repeat(14), "#".repeat(14)),
                )),
            ],
        ),
        level(
            "t-open2",
            20.0,
            vec![stage(map(
                &[(2, '1'), (13, 'R'), (24, 'B'), (27, '2')],
                &wall,
            ))],
        ),
        level(
            "t-open3",
            20.0,
            vec![stage(map(
                &[(2, '1'), (13, 'R'), (24, 'B'), (27, '2')],
                &wall,
            ))],
        ),
        level("t-win", 20.0, vec![stage(map(win, &wall))]),
        level("t-last", 20.0, vec![stage(map(win, &wall))]),
    ]))
}

const FRAME: f64 = 1000.0 / 60.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum To {
    Host,
    Guest,
}

type Drop = Box<dyn FnMut(To, &str) -> bool>;

struct Pair {
    host: Box<dyn EmberGame>,
    guest: Box<dyn EmberGame>,
    now: f64,
    lag: f64,
    queue: Vec<(f64, To, String)>,
    sent: Vec<(f64, To, String)>,
    hv: EmberView,
    gv: EmberView,
    host_events: Vec<GameEvent>,
    guest_events: Vec<GameEvent>,
    drop: Option<Drop>,
    drop_first: usize,
    guest_version: Option<u32>,
}

impl Pair {
    fn new(lag: f64) -> Self {
        Self::with(lag, None, 0, None)
    }

    fn with(lag: f64, drop: Option<Drop>, drop_first: usize, guest_version: Option<u32>) -> Self {
        let now = 1000.0;
        let cat = catalog();
        let mut host: Box<dyn EmberGame> = Box::new(HostGame::new(cat.clone(), now, 0.0));
        let mut guest: Box<dyn EmberGame> = Box::new(GuestGame::new(cat, now, 0.0));
        let hv = host.tick(now);
        let gv = guest.tick(now);
        let mut p = Self {
            host,
            guest,
            now,
            lag,
            queue: vec![],
            sent: vec![],
            hv,
            gv,
            host_events: vec![],
            guest_events: vec![],
            drop,
            drop_first,
            guest_version,
        };
        p.route();
        p
    }

    fn route(&mut self) {
        let mut out: Vec<(To, String)> = self
            .host
            .take_outbox()
            .into_iter()
            .map(|t| (To::Guest, t))
            .collect();
        out.extend(self.guest.take_outbox().into_iter().map(|t| {
            let t = match self.guest_version {
                Some(v) => t.replacen("\"v\":2", &format!("\"v\":{v}"), 1),
                None => t,
            };
            (To::Host, t)
        }));
        for (to, text) in out {
            self.sent.push((self.now, to, text.clone()));
            if self.drop_first > 0 {
                self.drop_first -= 1;
                continue;
            }
            if let Some(d) = self.drop.as_mut()
                && d(to, &text)
            {
                continue;
            }
            self.queue.push((self.now + self.lag, to, text));
        }
    }

    fn run_cut(&mut self, ms: f64, cut: bool) {
        let end = self.now + ms;
        while self.now < end - 1e-9 {
            self.now += FRAME;
            let now = self.now;
            if cut {
                self.queue.clear();
            } else {
                let mut due: Vec<(f64, To, String)> = vec![];
                self.queue.retain(|m| {
                    if m.0 <= now {
                        due.push(m.clone());
                        false
                    } else {
                        true
                    }
                });
                due.sort_by(|a, b| a.0.total_cmp(&b.0));
                for (_, to, text) in due {
                    match to {
                        To::Host => self.host.receive(&text, now),
                        To::Guest => self.guest.receive(&text, now),
                    }
                }
            }
            self.hv = self.host.tick(now);
            self.gv = self.guest.tick(now);
            self.host_events.extend(self.hv.events.iter().cloned());
            self.guest_events.extend(self.gv.events.iter().cloned());
            self.route();
        }
    }

    fn run(&mut self, ms: f64) {
        self.run_cut(ms, false);
    }

    fn start_on(&mut self, level: f64, guest: bool) {
        self.run(500.0);
        if guest {
            self.guest.pick(level)
        } else {
            self.host.pick(level)
        }
        self.route();
        self.run(200.0);
        if guest {
            self.guest.play()
        } else {
            self.host.play()
        }
        self.route();
        self.run(500.0);
    }

    fn host_keys(&mut self, bits: i64) {
        self.host.keys(0, bits);
    }

    fn guest_keys(&mut self, bits: i64) {
        self.guest.keys(1, bits);
    }
}

fn pos(v: &EmberView, p: usize) -> Player {
    v.state.as_ref().unwrap().players[p]
}

fn spawn(i: usize, p: usize) -> f64 {
    catalog().get(i).unwrap().stages[0].spawns[p].x
}

#[test]
fn waits_for_both_then_both_go_to_select() {
    let mut g = Pair::with(5.0, None, 2, None);
    assert_eq!(g.hv.screen, Screen::Wait);
    assert_eq!(g.gv.screen, Screen::Wait);
    g.run(1000.0);
    assert_eq!(g.hv.screen, Screen::Select);
    assert_eq!(g.gv.screen, Screen::Select);
    assert_eq!(g.hv.me, Some(0));
    assert_eq!(g.gv.me, Some(1));
}

#[test]
fn stops_saying_hi_once_heard() {
    let mut g = Pair::with(5.0, None, 1, None);
    g.run(3000.0);
    assert!(
        !g.sent
            .iter()
            .any(|m| m.0 > 2500.0 && m.2.contains("\"hi\""))
    );
}

#[test]
fn refuses_a_different_version() {
    let mut g = Pair::with(5.0, None, 0, Some(99));
    g.run(500.0);
    assert_eq!(
        g.hv.ended.as_deref(),
        Some("update both Q Calcs to play each other")
    );
    assert_eq!(g.gv.ended.as_deref(), Some("your friend left"));
}

#[test]
fn ends_on_bye_and_on_silence() {
    let mut g = Pair::new(5.0);
    g.run(500.0);
    g.guest.leave();
    g.route();
    g.run(100.0);
    assert_eq!(g.hv.ended.as_deref(), Some("your friend left"));
    for play in [false, true] {
        let mut g = Pair::new(5.0);
        if play {
            g.start_on(0.0, false);
        } else {
            g.run(500.0);
        }
        g.run(SILENCE_MS + 500.0);
        assert_eq!(g.hv.ended, None);
        assert_eq!(g.gv.ended, None);
        g.run_cut(SILENCE_MS + 500.0, true);
        assert_eq!(g.hv.ended.as_deref(), Some("lost the connection"));
        assert_eq!(g.gv.ended.as_deref(), Some("lost the connection"));
    }
}

#[test]
fn ignores_junk() {
    let mut g = Pair::new(5.0);
    g.start_on(0.0, false);
    let valid: Vec<String> = g
        .sent
        .iter()
        .filter(|m| m.0 > g.now - 300.0)
        .map(|m| m.2.clone())
        .collect();
    assert!(valid.iter().any(|t| t.starts_with("{\"t\":\"s\"")));
    let mut seed: i64 = 7;
    let mut rand = || {
        seed = (seed * 1103515245 + 12345) % 2147483648;
        seed as f64 / 2147483648.0
    };
    let mut junk: Vec<String> = [
        "",
        "null",
        "[]",
        "{}",
        "\"s\"",
        "{\"t\":\"s\"}",
        "{\"t\":\"pick\",\"l\":99}",
        "{\"t\":\"go\",\"l\":1.5}",
        "1e999",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for _ in 0..300 {
        let base: Vec<char> = valid[(rand() * valid.len() as f64) as usize]
            .chars()
            .collect();
        let at = (rand() * base.len() as f64) as usize;
        let pick = rand();
        if pick < 0.5 {
            junk.push(base[..at].iter().collect());
        } else {
            let mut b = base.clone();
            b[at] = char::from(32 + (rand() * 90.0) as u8);
            junk.push(b.into_iter().collect());
        }
    }
    for text in &junk {
        let now = g.now;
        g.host.receive(text, now);
        g.guest.receive(text, now);
        g.run(1.0);
    }
    g.run(500.0);
    assert_eq!(g.hv.ended, None);
    assert_eq!(g.gv.screen, Screen::Play);
    for v in [&g.hv, &g.gv] {
        for p in &v.state.as_ref().unwrap().players {
            assert!(p.x.is_finite() && p.y.is_finite());
        }
    }
}

#[test]
fn shares_the_highlight() {
    let mut g = Pair::new(5.0);
    g.run(500.0);
    g.guest.pick(2.0);
    let now = g.now;
    assert_eq!(g.guest.tick(now).level, 2);
    g.route();
    g.run(200.0);
    assert_eq!(g.hv.level, 2);
    assert_eq!(g.gv.level, 2);
    g.host.pick(1.0);
    g.run(200.0);
    assert_eq!(g.gv.level, 1);
    g.host.pick(9.0);
    g.run(200.0);
    assert_eq!(g.gv.level, 6);
}

#[test]
fn either_side_can_start_and_menu_takes_both_back() {
    for guest in [false, true] {
        let mut g = Pair::new(5.0);
        g.start_on(1.0, guest);
        assert_eq!(g.hv.screen, Screen::Play);
        assert_eq!(g.gv.screen, Screen::Play);
        assert_eq!((g.hv.level, g.gv.level), (1, 1));
        assert!(g.gv.playing);
        assert_eq!(g.gv.state.as_ref().unwrap().phase, Phase::Play);
        let note = if guest {
            g.hv.note.clone()
        } else {
            g.gv.note.clone()
        };
        assert!(
            note.unwrap()
                .starts_with(if guest { "frost " } else { "ember " })
        );
    }
    let mut g = Pair::new(5.0);
    g.start_on(1.0, false);
    g.guest.menu();
    g.route();
    g.run(300.0);
    assert_eq!(g.hv.screen, Screen::Select);
    assert_eq!(g.gv.screen, Screen::Select);
    assert_eq!(g.hv.note.as_deref(), Some("frost went back to the levels"));
    g.start_on(1.0, false);
    g.host.menu();
    g.route();
    g.run(300.0);
    assert_eq!(g.gv.screen, Screen::Select);
    assert_eq!(g.gv.note.as_deref(), Some("ember went back to the levels"));
}

#[test]
fn host_and_guest_agree_after_both_move() {
    for lag in [0.0, 80.0] {
        let mut g = Pair::new(lag);
        g.start_on(0.0, false);
        g.host_keys(IN_RIGHT);
        g.guest_keys(IN_LEFT | IN_UP);
        g.run(300.0);
        g.guest_keys(IN_LEFT);
        g.run(200.0);
        g.host_keys(IN_RIGHT | IN_UP);
        g.guest_keys(0);
        g.run(250.0);
        g.host_keys(0);
        g.run(1500.0);
        for p in 0..2 {
            assert!(
                (pos(&g.gv, p).x - pos(&g.hv, p).x).abs() < 0.5,
                "lag {lag} p {p}"
            );
            assert!((pos(&g.gv, p).y - pos(&g.hv, p).y).abs() < 0.5);
        }
        assert!(pos(&g.hv, 0).x > spawn(0, 0) + 40.0);
        assert!(pos(&g.hv, 1).x < spawn(0, 1) - 40.0);
    }
}

#[test]
fn the_guest_moves_at_once_under_lag() {
    let mut g = Pair::new(80.0);
    g.start_on(0.0, false);
    g.run(1000.0);
    let x0 = pos(&g.gv, 1).x;
    let h0 = pos(&g.hv, 1).x;
    g.guest_keys(IN_RIGHT);
    g.run(50.0);
    assert!(pos(&g.gv, 1).x > x0 + (RUN_ACCEL * 0.05 * 0.05) / 4.0);
    assert_eq!(pos(&g.hv, 1).x, h0);
    g.run(400.0);
    assert!(pos(&g.hv, 1).x > h0 + 10.0);
}

#[test]
fn each_side_emits_each_jump_once() {
    let mut g = Pair::new(80.0);
    g.start_on(0.0, false);
    g.run(500.0);
    g.host_events.clear();
    g.guest_events.clear();
    g.guest_keys(IN_UP);
    g.run(100.0);
    g.guest_keys(0);
    g.host_keys(IN_UP);
    g.run(100.0);
    g.host_keys(0);
    g.run(1500.0);
    let jumps = |es: &[GameEvent], p: usize| {
        es.iter()
            .filter(|e| e.k == EventKind::Jump && e.p == Some(p))
            .count()
    };
    assert_eq!(jumps(&g.guest_events, 1), 1);
    assert_eq!(jumps(&g.guest_events, 0), 1);
    assert_eq!(jumps(&g.host_events, 1), 1);
    assert_eq!(jumps(&g.host_events, 0), 1);
}

#[test]
fn a_late_guest_tap_still_jumps() {
    let mut now = 1000.0;
    let mut host = HostGame::new(catalog(), now, 0.0);
    host.receive(&format!("{{\"t\":\"hi\",\"v\":{EMBER_VERSION},\"w\":{WIRE_VERSION},\"g\":\"ember\",\"ok\":true}}"), now);
    host.tick(now);
    host.play();
    let mut y0 = f64::INFINITY;
    let mut top = f64::INFINITY;
    for i in 0..120 {
        now += FRAME;
        if i == 40 {
            host.receive(
                &format!("{{\"t\":\"k\",\"a\":1,\"f\":34,\"i\":[[30,{IN_UP}],[34,0]]}}"),
                now,
            );
        }
        let v = host.tick(now);
        if let Some(s) = v.state {
            if i < 40 {
                y0 = s.players[1].y;
            }
            top = top.min(s.players[1].y);
        }
    }
    assert!(top < y0 - 10.0);
}

#[test]
fn a_host_death_shows_on_the_guest_and_comes_back() {
    let mut g = Pair::new(40.0);
    g.start_on(1.0, false);
    g.host_keys(IN_RIGHT);
    let mut saw = false;
    for _ in 0..120 {
        g.run(FRAME);
        if g.gv.state.as_ref().map(|s| s.phase) == Some(Phase::Dead) {
            saw = true;
            break;
        }
    }
    assert!(saw);
    assert!(
        g.guest_events
            .iter()
            .any(|e| e.k == EventKind::Die && e.p == Some(0))
    );
    g.host_keys(0);
    g.run(3000.0);
    let gs = g.gv.state.as_ref().unwrap();
    assert_eq!(gs.phase, Phase::Play);
    assert!(gs.deaths >= 1);
    assert_eq!(gs.deaths, g.hv.state.as_ref().unwrap().deaths);
}

#[test]
fn a_guest_never_dies_alone() {
    let drop: Drop = Box::new(|to, text| to == To::Host && text.starts_with("{\"t\":\"k\""));
    let mut g = Pair::with(40.0, Some(drop), 0, None);
    g.start_on(0.0, false);
    g.guest_keys(IN_LEFT);
    g.run(2000.0);
    assert_eq!(g.gv.state.as_ref().unwrap().phase, Phase::Play);
    assert!((pos(&g.gv, 1).x - spawn(0, 1)).abs() < 45.0);
}

#[test]
fn win_next_and_past_the_last_level() {
    let mut g = Pair::new(30.0);
    g.start_on(5.0, false);
    g.host_keys(IN_RIGHT);
    g.guest_keys(IN_RIGHT);
    g.run(1500.0);
    assert_eq!(g.hv.state.as_ref().unwrap().phase, Phase::Won);
    assert_eq!(g.gv.state.as_ref().unwrap().phase, Phase::Won);
    assert_eq!(
        g.guest_events
            .iter()
            .filter(|e| e.k == EventKind::Win)
            .count(),
        1
    );
    assert_eq!(
        g.host_events
            .iter()
            .filter(|e| e.k == EventKind::Win)
            .count(),
        1
    );
    g.host.release();
    g.guest.release();
    g.guest.next();
    g.route();
    g.run(300.0);
    assert_eq!((g.hv.level, g.gv.level), (6, 6));
    assert_eq!(g.gv.state.as_ref().unwrap().phase, Phase::Play);
    assert_eq!(
        g.hv.note.as_deref(),
        Some("frost went on to the next level")
    );
    g.host_keys(IN_RIGHT);
    g.guest_keys(IN_RIGHT);
    g.run(1500.0);
    assert_eq!(g.gv.state.as_ref().unwrap().phase, Phase::Won);
    g.host.next();
    g.route();
    g.run(300.0);
    assert_eq!(g.hv.screen, Screen::Select);
    assert_eq!(g.gv.screen, Screen::Select);
}

#[test]
fn retry_from_the_guest_takes_both_back() {
    let mut g = Pair::new(30.0);
    g.start_on(0.0, false);
    g.host_keys(IN_RIGHT);
    g.run(500.0);
    g.host_keys(0);
    g.guest.next();
    g.route();
    g.run(200.0);
    assert_eq!(g.hv.level, 0);
    assert!(pos(&g.hv, 0).x > spawn(0, 0) + 20.0);
    g.guest.retry();
    g.route();
    g.run(300.0);
    assert_eq!(
        g.hv.note.as_deref(),
        Some("frost went back to the checkpoint")
    );
    assert!((pos(&g.hv, 0).x - spawn(0, 0)).abs() < 0.5);
    assert!((pos(&g.gv, 0).x - spawn(0, 0)).abs() < 0.5);
}

#[test]
fn message_rates_and_sizes() {
    {
        let mut g = Pair::new(5.0);
        g.start_on(0.0, false);
        g.run(2000.0);
        let t = g.now;
        let states = g
            .sent
            .iter()
            .filter(|m| m.1 == To::Guest && m.0 > t - 1000.0 && m.2.starts_with("{\"t\":\"s\""))
            .count();
        assert!(
            states as f64 >= STATE_HZ - 2.0 && states as f64 <= STATE_HZ + 1.0,
            "{states}"
        );
    }
    let mut g = Pair::new(5.0);
    g.start_on(0.0, false);
    g.guest_keys(IN_RIGHT);
    g.run(100.0);
    let t = g.now;
    g.run(2000.0);
    let from_guest = g.sent.iter().filter(|m| m.1 == To::Host && m.0 > t).count();
    assert!((6..=9).contains(&from_guest), "{from_guest}");
    g.host_keys(IN_RIGHT);
    g.run(500.0);
    let states: Vec<&String> = g
        .sent
        .iter()
        .filter(|m| m.2.starts_with("{\"t\":\"s\""))
        .map(|m| &m.2)
        .collect();
    for s in &states {
        assert!(s.len() < 700);
    }
    assert!(matches!(
        decode_ember(states.last().unwrap()),
        Some(EmberMsg::State(_))
    ));
}

// ---- local

#[test]
fn local_select_clamps_and_plays() {
    let cat = catalog();
    let mut now = 0.0;
    let mut game = LocalGame::new(cat.clone(), now, 0.0);
    let v = game.tick(now);
    assert_eq!(v.screen, Screen::Select);
    game.pick(99.0);
    assert_eq!(game.tick(now).level, 6);
    game.pick(-3.0);
    assert_eq!(game.tick(now).level, 0);
    game.play();
    let mut events = vec![];
    // a tap quicker than a step still jumps
    game.keys(0, IN_UP);
    game.keys(0, 0);
    for _ in 0..30 {
        now += FRAME;
        events.extend(game.tick(now).events);
    }
    assert_eq!(
        events
            .iter()
            .filter(|e| e.k == EventKind::Jump && e.p == Some(0))
            .count(),
        1
    );
    game.menu();
    assert_eq!(game.tick(now).screen, Screen::Select);
}

#[test]
fn local_next_after_a_win() {
    let mut now = 0.0;
    let mut game = LocalGame::new(catalog(), now, 5.0);
    game.play();
    game.keys(0, IN_RIGHT);
    game.keys(1, IN_RIGHT);
    let mut v = game.tick(now);
    for _ in 0..90 {
        now += FRAME;
        v = game.tick(now);
    }
    assert_eq!(v.state.as_ref().unwrap().phase, Phase::Won);
    game.release();
    game.next();
    v = game.tick(now);
    assert_eq!(v.level, 6);
    game.keys(0, IN_RIGHT);
    game.keys(1, IN_RIGHT);
    for _ in 0..90 {
        now += FRAME;
        v = game.tick(now);
    }
    assert_eq!(v.state.as_ref().unwrap().phase, Phase::Won);
    game.next();
    assert_eq!(game.tick(now).screen, Screen::Select);
}

#[test]
fn local_stages_and_checkpoints() {
    let mut now = 0.0;
    let mut game = LocalGame::new(catalog(), now, 2.0);
    game.play();
    game.keys(0, IN_RIGHT);
    game.keys(1, IN_RIGHT);
    let mut v = game.tick(now);
    let mut saw = vec![];
    for _ in 0..300 {
        now += FRAME;
        v = game.tick(now);
        saw.extend(v.events.iter().map(|e| e.k));
        if v.state.as_ref().unwrap().stage == 1 && v.state.as_ref().unwrap().phase == Phase::Play {
            break;
        }
    }
    assert!(saw.contains(&EventKind::Clear));
    assert!(saw.contains(&EventKind::Stage));
    let s = v.state.clone().unwrap();
    assert_eq!(s.stage, 1);
    assert!(s.tick > 0);
    // run right into the pit past the flag: back at the flag
    game.keys(0, IN_RIGHT);
    game.keys(1, IN_RIGHT);
    for _ in 0..600 {
        now += FRAME;
        v = game.tick(now);
        if v.state.as_ref().unwrap().deaths > 0 && v.state.as_ref().unwrap().phase == Phase::Play {
            break;
        }
    }
    let s = v.state.clone().unwrap();
    assert_eq!(s.deaths, 1);
    assert_eq!(s.cp, 0);
    assert!(s.players[0].x > 6.0 * TILE);
    game.release();
    game.restart();
    let v = game.tick(now);
    assert_eq!(v.state.as_ref().unwrap().stage, 0);
    assert_eq!(v.state.as_ref().unwrap().deaths, 0);
}
