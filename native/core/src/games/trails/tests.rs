// trails.test.ts, case for case

use super::*;

struct At {
    x: i32,
    y: i32,
    dir: Dir,
}

// a round with the cycles placed by hand on an empty grid
fn placed(a: At, b: At) -> Round {
    let mut grid = vec![0u8; (COLS * ROWS) as usize];
    let mut cyc = |c: At, p: usize| {
        grid[cell_index(c.x, c.y) as usize] = p as u8 + 1;
        Cycle { x: c.x, y: c.y, dir: c.dir, queue: vec![], alive: true, path: vec![cell_index(c.x, c.y)], crash: None }
    };
    let ca = cyc(a, 0);
    let cb = cyc(b, 1);
    Round { grid, cycles: [ca, cb], tick: 0, result: None }
}

fn at(x: i32, y: i32, dir: Dir) -> At {
    At { x, y, dir }
}

fn wall(r: &Round, cells: &[(i32, i32)], v: u8) -> Round {
    let mut out = r.clone();
    for &(x, y) in cells {
        out.grid[cell_index(x, y) as usize] = v;
    }
    out
}

fn win(w: Option<usize>) -> Option<RoundResult> {
    Some(RoundResult { winner: w })
}

#[test]
fn moves_one_cell_a_tick_and_leaves_a_trail() {
    let r0 = new_round();
    let (a0, b0) = (r0.cycles[0].clone(), r0.cycles[1].clone());
    let r = step(&step(&r0));
    assert_eq!((r.cycles[0].x, r.cycles[0].y, r.cycles[0].alive), (a0.x + 2, a0.y, true));
    assert_eq!((r.cycles[1].x, r.cycles[1].y, r.cycles[1].alive), (b0.x - 2, b0.y, true));
    assert_eq!(r.cycles[0].path.len(), 3);
    for &i in &r.cycles[0].path {
        assert_eq!(r.grid[i as usize], 1);
    }
    for &i in &r.cycles[1].path {
        assert_eq!(r.grid[i as usize], 2);
    }
    assert_eq!(r.tick, 2);
    assert_eq!(r.result, None);
}

#[test]
fn starts_fair() {
    let r = new_round();
    let [a, b] = &r.cycles;
    assert_eq!(a.x + b.x, COLS - 1);
    assert_eq!(a.y + b.y, ROWS - 1);
    assert_ne!(a.y, b.y);
}

#[test]
fn turns_90_and_never_reverses() {
    let mut r = placed(at(10, 10, 1), at(40, 20, 3));
    r = queue_turn(&r, 0, 3);
    r = queue_turn(&r, 0, 1);
    assert!(r.cycles[0].queue.is_empty());
    r = step(&queue_turn(&r, 0, 2));
    assert_eq!((r.cycles[0].x, r.cycles[0].y, r.cycles[0].dir), (10, 11, 2));
}

#[test]
fn queues_two_quick_turns_and_drops_a_third() {
    let mut r = placed(at(10, 10, 1), at(40, 20, 3));
    r = queue_turn(&r, 0, 0);
    r = queue_turn(&r, 0, 2);
    r = queue_turn(&r, 0, 3);
    r = queue_turn(&r, 0, 0);
    assert_eq!(r.cycles[0].queue, vec![0, 3]);
    r = step(&r);
    assert_eq!((r.cycles[0].x, r.cycles[0].y, r.cycles[0].dir), (10, 9, 0));
    r = step(&r);
    assert_eq!((r.cycles[0].x, r.cycles[0].y, r.cycles[0].dir, r.cycles[0].alive), (9, 9, 3, true));
    assert!(r.cycles[0].queue.is_empty());
}

#[test]
fn crashes_into_the_wall() {
    let r = step(&placed(at(COLS - 1, 5, 1), at(10, 20, 1)));
    assert!(!r.cycles[0].alive);
    assert_eq!(r.cycles[0].crash, Some((COLS, 5)));
    assert_eq!(r.result, win(Some(1)));
    assert_eq!(step(&r), r);
}

#[test]
fn crashes_into_trails() {
    let mut r = placed(at(10, 10, 1), at(40, 20, 1));
    r = step(&r);
    r = step(&queue_turn(&r, 0, 2));
    r = step(&queue_turn(&r, 0, 3));
    r = step(&queue_turn(&r, 0, 0));
    assert_eq!(r.result, win(Some(1)));
    let r = wall(&placed(at(10, 10, 1), at(40, 20, 1)), &[(11, 10)], 2);
    assert_eq!(step(&r).result, win(Some(1)));
    let r = wall(&placed(at(10, 10, 1), at(40, 20, 1)), &[(41, 20)], 1);
    assert_eq!(step(&r).result, win(Some(0)));
}

#[test]
fn crashes_into_the_other_cycle() {
    let r2 = step(&placed(at(10, 10, 2), at(11, 10, 3)));
    assert!(!r2.cycles[1].alive);
    assert_eq!(r2.result, win(Some(0)));
}

#[test]
fn draws() {
    let r = step(&placed(at(10, 10, 1), at(12, 10, 3)));
    assert_eq!(r.result, win(None));
    assert!(r.cycles.iter().all(|c| !c.alive));
    assert_eq!(step(&placed(at(10, 10, 1), at(11, 10, 3))).result, win(None));
    assert_eq!(step(&placed(at(0, 3, 3), at(COLS - 1, 20, 1))).result, win(None));
}

#[test]
fn scores_rounds_to_a_match() {
    let mut score = [0, 0];
    score = score_round(score, RoundResult { winner: Some(0) });
    score = score_round(score, RoundResult { winner: None });
    assert_eq!(score, [1, 0]);
    assert_eq!(match_winner(score), None);
    for _ in 1..WIN_ROUNDS {
        score = score_round(score, RoundResult { winner: Some(1) });
    }
    assert_eq!(match_winner(score), None);
    score = score_round(score, RoundResult { winner: Some(1) });
    assert_eq!(score, [1, WIN_ROUNDS]);
    assert_eq!(match_winner(score), Some(1));
}

#[test]
fn steer_sets_the_computers_move() {
    let mut r = placed(at(10, 10, 1), at(40, 20, 1));
    r = steer(&queue_turn(&r, 1, 0), 1, 2);
    assert_eq!(r.cycles[1].queue, vec![2]);
    assert!(steer(&r, 1, 3).cycles[1].queue.is_empty());
}

#[test]
fn never_drives_the_computer_into_a_wall_when_a_way_out_exists() {
    let mut rand = seeded(7);
    let mut checked = 0;
    for level in LEVELS {
        for _ in 0..150 {
            let x = 1 + (rand() * f64::from(COLS - 2)).floor() as i32;
            let y = 1 + (rand() * f64::from(ROWS - 2)).floor() as i32;
            let dir = (rand() * 4.0).floor() as u8;
            let mut r = placed(at(x, y, dir), at(0, 0, 1));
            let me = r.cycles[0].clone();
            let mut clutter = vec![];
            for _ in 0..700 {
                let x = (rand() * f64::from(COLS)).floor() as i32;
                let y = (rand() * f64::from(ROWS)).floor() as i32;
                if (x != me.x || y != me.y) && (x != 0 || y != 0) {
                    clutter.push((x, y));
                }
            }
            r = wall(&r, &clutter, 2);
            let safe: Vec<u8> = (0..4u8)
                .filter(|&d| {
                    let x = me.x + DX[d as usize];
                    let y = me.y + DY[d as usize];
                    d != (me.dir + 2) % 4 && x >= 0 && y >= 0 && x < COLS && y < ROWS && r.grid[cell_index(x, y) as usize] == 0
                })
                .collect();
            let d = bot_turn(&r, 0, level, &mut rand);
            if !safe.is_empty() {
                checked += 1;
                assert!(safe.contains(&d));
            }
        }
    }
    assert!(checked > 100);
}

#[test]
fn picks_the_open_side_over_a_dead_end() {
    let r = wall(&placed(at(10, 0, 1), at(50, 20, 3)), &[(12, 0), (11, 1)], 2);
    for level in [Level::Normal, Level::Hard] {
        assert_eq!(bot_turn(&r, 0, level, &mut seeded(1)), 2);
    }
}

// a whole round, bot against bot; players swap sides each round so neither start is favoured
pub(super) fn duel(a: Level, b: Level, rounds: u32, seed: u32) -> (u32, u32, u32) {
    let mut rand = seeded(seed);
    let (mut ta, mut tb, mut draws) = (0, 0, 0);
    for i in 0..rounds {
        let levels = if i % 2 == 1 { [b, a] } else { [a, b] };
        let mut r = new_round();
        let opening = (rand() * 12.0).floor() as u32;
        while r.result.is_none() && r.tick < (COLS * ROWS) as u32 {
            for p in 0..2 {
                if r.tick >= opening {
                    let d = bot_turn(&r, p, levels[p], &mut rand);
                    r = steer(&r, p, d);
                } else if rand() < 0.15 {
                    let d = (rand() * 4.0).floor() as u8;
                    r = steer(&r, p, d);
                }
            }
            r = step(&r);
        }
        match r.result.and_then(|x| x.winner) {
            None => draws += 1,
            Some(w) if (w == 0) == (i % 2 == 0) => ta += 1,
            Some(_) => tb += 1,
        }
    }
    (ta, tb, draws)
}

#[test]
fn plays_harder_the_higher_the_level() {
    let (a, b, _) = duel(Level::Hard, Level::Easy, 12, 3);
    assert!(a > b * 3);
    let (a, b, _) = duel(Level::Normal, Level::Easy, 16, 4);
    assert!(a > b * 3);
    let (a, b, _) = duel(Level::Hard, Level::Normal, 12, 5);
    assert!(a > b * 2);
}

#[test]
fn opens_from_its_names() {
    for t in ["trails", "Trail", " cycles ", "TRAILS"] {
        assert!(is_trails_command(t), "{t}");
    }
    for t in ["trails2", "trail s", "cycle", "2 trails", "trails+1", "sin(x)", "t"] {
        assert!(!is_trails_command(t), "{t}");
    }
}

#[test]
fn seeded_matches_mulberry32() {
    // the first draws of seeded(1) in JS
    let mut r = seeded(1);
    let first = r();
    assert!((first - 0.6270739405881613).abs() < 1e-15, "{first}");
}
