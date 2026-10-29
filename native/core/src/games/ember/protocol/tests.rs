// emberProtocol.test.ts's properties, over the shipped levels (the golden fixtures hold the exact text)

use super::*;
use crate::games::ember::engine::{init_state, stage_of, step};
use crate::games::ember::{LEVELS, compiled_at};

fn states(level: usize, ticks: usize) -> Vec<GameState> {
    let lv = compiled_at(level).unwrap();
    let mut s = init_state(lv, 0);
    let mut out = vec![];
    for t in 0..ticks {
        let bits = [
            if t % 50 < 30 {
                IN_RIGHT
            } else {
                IN_LEFT | IN_UP
            },
            if t % 70 < 40 {
                IN_RIGHT | IN_UP
            } else {
                IN_DOWN
            },
        ];
        s = step(lv, &s, bits);
        out.push(s.clone());
    }
    out
}

#[test]
fn reads_what_it_wrote() {
    for l in 0..LEVELS.len() {
        let lv = compiled_at(l).unwrap();
        for s in states(l, 400).iter().step_by(7) {
            let stage = stage_of(lv, s.stage);
            let k = slow_of(l, stage, s);
            let g = fast_of(stage, s);
            let msg = EmberMsg::State(Box::new(StateMsg {
                a: 1.0,
                f: 9.0,
                fi: 3.0,
                sl: Some((-2.0, 7.0)),
                r: 4.0,
                k: Some(k.clone()),
                g: g.clone(),
                e: s.events.clone(),
                q: None,
            }));
            let text = encode_ember(&msg);
            let Some(EmberMsg::State(back)) = decode_ember(&text) else {
                panic!("{text}")
            };
            assert_eq!(back.k.as_ref(), Some(&k));
            assert_eq!(back.g, g);
            let joined = join_state(lv, &k, &g).expect("joins");
            // a joined state packs to the same text again
            assert_eq!(fast_json(&fast_of(stage, &joined)), fast_json(&g));
            assert_eq!(slow_json(&slow_of(l, stage, &joined)), slow_json(&k));
            for e in &back.e {
                assert!(event_fits(lv, stage, e));
            }
        }
    }
}

#[test]
fn leaves_out_what_is_at_rest() {
    let lv = compiled_at(0).unwrap();
    let s = init_state(lv, 0);
    let stage = stage_of(lv, 0);
    assert_eq!(slow_json(&slow_of(0, stage, &s)), "{\"l\":0}");
    let g = fast_json(&fast_of(stage, &s));
    assert!(g.starts_with("{\"T\":0,\"pt\":0,\"p\":[["), "{g}");
    assert!(!g.contains("\"bt\""));
}

#[test]
fn refuses_a_state_that_does_not_fit_its_stage() {
    let lv = compiled_at(0).unwrap();
    let s = init_state(lv, 0);
    let stage = stage_of(lv, 0);
    let mut g = fast_of(stage, &s);
    g.mv = Some(vec![vec![0.5]]);
    assert!(join_state(lv, &slow_of(0, stage, &s), &g).is_none());
    let k = Slow {
        gm: Some("ffff".into()),
        ..slow_of(0, stage, &s)
    };
    assert!(join_state(lv, &k, &fast_of(stage, &s)).is_none());
}

#[test]
fn refuses_junk_and_survives_garble() {
    for t in [
        "",
        "[]",
        "null",
        "{}",
        "{\"t\":\"s\"}",
        "{\"t\":\"k\",\"a\":-1,\"f\":0,\"i\":[]}",
        "{\"t\":\"pick\",\"l\":999}",
        "{\"t\":\"note\",\"w\":\"dance\"}",
    ] {
        assert_eq!(decode_ember(t), None, "{t}");
    }
    assert_eq!(decode_ember(&"x".repeat(MAX_TEXT + 1)), None);
    let lv = compiled_at(1).unwrap();
    let s = &states(1, 200)[199];
    let stage = stage_of(lv, s.stage);
    let text = encode_ember(&EmberMsg::State(Box::new(StateMsg {
        a: 1.0,
        f: 2.0,
        fi: 0.0,
        sl: None,
        r: 1.0,
        k: Some(slow_of(1, stage, s)),
        g: fast_of(stage, s),
        e: s.events.clone(),
        q: None,
    })));
    let mut rand = crate::games::trails::seeded(5);
    let bytes: Vec<char> = text.chars().collect();
    for _ in 0..1000 {
        let mut b = bytes.clone();
        for _ in 0..3 {
            let i = (rand() * b.len() as f64) as usize;
            b[i] = ['0', '9', '-', ',', '[', ']', '"', 'e', '.'][(rand() * 9.0) as usize];
        }
        let garbled: String = b.into_iter().collect();
        if let Some(EmberMsg::State(m)) = decode_ember(&garbled)
            && let Some(k) = &m.k
            && let Some(l) = compiled_at(k.l as usize)
        {
            let _ = join_state(l, k, &m.g);
        }
    }
}

#[test]
fn small_messages_round_trip() {
    let all = [
        EmberMsg::Bye,
        EmberMsg::Sel { l: 2.0 },
        EmberMsg::Pick { l: 1.0 },
        EmberMsg::Go { l: 0.0 },
        EmberMsg::Retry,
        EmberMsg::Restart,
        EmberMsg::Next,
        EmberMsg::Menu,
        EmberMsg::Note {
            w: EmberNote::Restart,
        },
        EmberMsg::K {
            a: 3.0,
            f: 120.0,
            i: vec![(118, 2), (119, 6)],
            r: Some(4.0),
        },
        EmberMsg::K {
            a: 0.0,
            f: 0.0,
            i: vec![],
            r: None,
        },
    ];
    for m in all {
        assert_eq!(
            decode_ember(&encode_ember(&m)),
            Some(m.clone()),
            "{}",
            encode_ember(&m)
        );
    }
    assert_eq!(
        encode_ember(&EmberMsg::K {
            a: 3.0,
            f: 120.0,
            i: vec![(118, 2)],
            r: None
        }),
        r#"{"t":"k","a":3,"f":120,"i":[[118,2]]}"#
    );
    assert_eq!(
        encode_ember(&EmberMsg::Hi {
            v: 2.0,
            w: 1.0,
            g: true,
            ok: true
        }),
        r#"{"t":"hi","v":2,"w":1,"g":"ember","ok":true}"#
    );
    assert_eq!(
        decode_ember(r#"{"t":"hi","v":3,"w":9}"#),
        Some(EmberMsg::Hi {
            v: 3.0,
            w: 9.0,
            g: false,
            ok: false
        })
    );
    assert_eq!(
        decode_ember(r#"{"t":"tr","v":1}"#),
        Some(EmberMsg::Other {
            game: "trails".into()
        })
    );
    assert_eq!(
        decode_ember(r#"{"t":"k","a":1,"f":5,"i":[[4,1],[2,0]]}"#),
        Some(EmberMsg::K {
            a: 1.0,
            f: 5.0,
            i: vec![(2, 0), (4, 1)],
            r: None
        })
    );
}
