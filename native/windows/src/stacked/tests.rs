//! The Mac's headless checks of the same rules (native/mac/Tests/stacked/main.swift, `scripts/test-stacked.sh`,
//! 120 of them, MathQuill's typing, SupSub, backspace, delete, updown and select cases among them), then Windows'
//! own: the places, the sizes and the gaps.

use super::*;

fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn s(v: &[u16]) -> String {
    String::from_utf16_lossy(v)
}

fn p(t: &str) -> Parsed {
    parse(&u(t), &[])
}

/// the stacks of `t`, outer first: `frac(num|den)` and `pow(exp)`, each slot's own text
fn stacks(t: &str) -> String {
    let v = u(t);
    p(t).stacks
        .iter()
        .map(|st| match st.num {
            Some(n) if st.fraction => format!(
                "frac({}|{})",
                s(&v[n.lo..n.hi]),
                s(&v[st.slot.lo..st.slot.hi])
            ),
            _ => format!("pow({})", s(&v[st.slot.lo..st.slot.hi])),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// the line as shown: hidden characters dropped
fn shown(t: &str) -> String {
    let x = p(t);
    let kept: Vec<u16> = u(t)
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !x.hidden.contains(i))
        .map(|(_, c)| c)
        .collect();
    s(&kept)
}

/// an edit as `text` with `|` at the caret
fn edit(e: Option<Edit>) -> String {
    match e {
        None => "nil".into(),
        Some(e) => format!("{}|{}", s(&e.text[..e.caret]), s(&e.text[e.caret..])),
    }
}

/// typing `keys` one at a time into an empty line, as the bar's keys do: `/`, `^` and `)` through the rules
fn typ(keys: &str) -> String {
    let (mut t, mut c): (Vec<u16>, usize) = (Vec::new(), 0);
    for k in keys.encode_utf16() {
        let put = |t: &mut Vec<u16>, c: &mut usize, k: u16| {
            t.insert(*c, k);
            *c += 1;
        };
        match k {
            0x2F => {
                let e = slash_key(&t, c, c);
                (t, c) = (e.text, e.caret);
            }
            // nothing to raise: the key does nothing
            0x5E => {
                if let Some(e) = power_key(&t, c, c) {
                    (t, c) = (e.text, e.caret);
                }
            }
            // the bar moves the caret on past a place it never stops (the field's snap)
            0x29 => {
                let x = parse(&t, &[]);
                match close_key(&x, c) {
                    Some(to) => c = snap(&x, to, c, t.len()),
                    None => put(&mut t, &mut c, k),
                }
            }
            _ => {
                let x = parse(&t, &[]);
                match break_out(&t, &x, c, k).or_else(|| term_after_stack(&t, &x, c, k)) {
                    Some(e) => (t, c) = (e.text, e.caret),
                    None => put(&mut t, &mut c, k),
                }
            }
        }
    }
    edit(Some(Edit { text: t, caret: c }))
}

/// `t` with `|` at the caret, as (text, caret)
fn at(t: &str) -> (Vec<u16>, usize) {
    let v = u(t);
    let c = v.iter().position(|&x| x == 0x7C).unwrap();
    let mut v = v;
    v.remove(c);
    (v, c)
}

fn bs(t: &str) -> String {
    let (v, c) = at(t);
    edit(backspace(&v, &parse(&v, &[]), c))
}

fn del(t: &str) -> String {
    let (v, c) = at(t);
    edit(forward_delete(&v, &parse(&v, &[]), c))
}

fn vert(t: &str, up: bool) -> String {
    let (v, c) = at(t);
    match vertical(&parse(&v, &[]), c, up) {
        Some(Vertical::To(i)) => edit(Some(Edit { text: v, caret: i })),
        Some(Vertical::Nearest(slot)) => format!("nearest({})", s(&v[slot.lo..slot.hi])),
        None => "nil".into(),
    }
}

fn edge(t: &str, c: usize, forward: bool) -> String {
    let v = u(t);
    let to = line_edge(&parse(&v, &[]), c, forward, v.len());
    edit(Some(Edit { text: v, caret: to }))
}

fn sel(t: &str, lo: usize, len: usize) -> String {
    let v = u(t);
    let (a, b) = expand(&parse(&v, &[]), lo, lo + len, v.len());
    s(&v[a..b])
}

fn tall(t: &str) -> String {
    p(t).tall
        .iter()
        .map(|b| format!("{}-{}", b.open, b.close))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn mac_checks_fractions() {
    assert_eq!(stacks("1/2"), "frac(1|2)", "a plain fraction");
    assert_eq!(
        stacks("12/(12^(2))"),
        "frac(12|12^(2)) pow(2)",
        "a bracketed denominator"
    );
    assert_eq!(
        shown("(1+x)/(2)"),
        "1+x2",
        "bracketed slots hide their brackets"
    );
    assert_eq!(
        stacks("1/(x/3)"),
        "frac(1|x/3) frac(x|3)",
        "a fraction in a denominator"
    );
    assert_eq!(
        stacks("(1/2)/4"),
        "frac(1/2|4) frac(1|2)",
        "a fraction in a numerator"
    );
    assert_eq!(stacks("1/2+3/4"), "frac(1|2) frac(3|4)", "two in a row");
    assert_eq!(
        stacks("2/3x"),
        "",
        "a term after the denominator takes it (2/3x is 2/(3x))"
    );
    assert_eq!(
        stacks("1/2^3"),
        "pow(3)",
        "a power after the denominator takes it"
    );
    assert_eq!(
        stacks("2x/3"),
        "",
        "a mixed number and letter isn't a numerator"
    );
    assert_eq!(
        stacks("sin(x)/2"),
        "frac(sin(x)|2)",
        "a function's call is a numerator"
    );
    assert_eq!(stacks("()/()"), "frac(|)", "an empty fraction");
    assert_eq!(stacks("1/"), "", "a slash with nothing after it");
    assert_eq!(stacks("0.5/2.25"), "frac(0.5|2.25)", "a decimal");
    assert_eq!(stacks("-1/2"), "frac(1|2)", "after a sign");
    assert_eq!(stacks("y=x/2"), "frac(x|2)", "in an equation");
}

#[test]
fn mac_checks_roots() {
    assert_eq!(
        stacks("√(12/(12^(2)))"),
        "frac(12|12^(2)) pow(2)",
        "a fraction under a root"
    );
    assert_eq!(
        stacks("sqrt(12/(12^(2)))"),
        "frac(12|12^(2)) pow(2)",
        "a fraction under sqrt("
    );
    assert_eq!(
        stacks("√2/3"),
        "",
        "a root over its number isn't a numerator (√2/3 is (√2)/3)"
    );
    assert_eq!(stacks("(√2)/3"), "frac(√2|3)", "a bracketed root is");
    assert_eq!(
        stacks("1/(√(2))"),
        "frac(1|√(2))",
        "a root in a denominator"
    );
    assert_eq!(stacks("2^(√(2))"), "pow(√(2))", "a root in a power");
    assert_eq!(stacks("√(2^(10))"), "pow(10)", "a power under a root");
    assert_eq!(
        stacks("√(x/2)+√((1/2)/4)"),
        "frac(x|2) frac(1/2|4) frac(1|2)",
        "two roots over fractions"
    );
    assert_eq!(stacks("∛(1/8)"), "frac(1|8)", "a cube root over a fraction");
}

#[test]
fn mac_checks_powers() {
    assert_eq!(stacks("2^10"), "pow(10)", "a number's power");
    assert_eq!(
        shown("2^(10)"),
        "210",
        "a bracketed power hides its brackets"
    );
    assert_eq!(stacks("x^2"), "pow(2)", "a letter's power");
    assert_eq!(stacks("e^x"), "pow(x)", "one letter as the power");
    assert_eq!(stacks("e^xy"), "", "a word isn't a power");
    assert_eq!(stacks("(1+x)^2"), "pow(2)", "a power of a group");
    assert_eq!(
        stacks("2^(3^(2))"),
        "pow(3^(2)) pow(2)",
        "a power in a power"
    );
}

#[test]
fn mac_checks_typing() {
    assert_eq!(typ("12/12^2"), "12/(12^(2|))", "12/12^2 typed");
    assert_eq!(typ("12/12^2))"), "12/(12^(2))|", "…and out of both");
    assert_eq!(
        typ("sqrt(12/12^2"),
        "sqrt(12/(12^(2|))",
        "sqrt(12/12^2) typed (the root's own bracket is the line's to close)"
    );
    assert_eq!(typ("sqrt(12/12^2)))"), "sqrt(12/(12^(2)))|", "…closed");
    assert_eq!(
        typ("√2/3"),
        "(√2)/(3|)",
        "a root's number goes over the bar whole"
    );
    assert_eq!(
        typ("√(2)/3"),
        "(√(2))/(3|)",
        "a bracketed root goes over whole"
    );
    assert_eq!(typ("/"), "(|)/()", "/ with nothing before it");
    assert_eq!(
        typ("/1)2"),
        "(1)/(2|)",
        ") from the numerator goes into the denominator"
    );
    assert_eq!(
        typ("1/2)x"),
        "1/(2)·x|",
        "a term after a fraction is multiplied by it"
    );
    assert_eq!(
        edit(power_key(&u("1/(2)"), 5, 5)),
        "(1/(2))^(|)",
        "^ after a fraction brackets it"
    );
    assert_eq!(
        edit(power_key(&u(""), 0, 0)),
        "nil",
        "^ with nothing before it"
    );
    assert_eq!(
        edit(Some(slash_key(&u("x+1"), 0, 3))),
        "(x+1)/(|)",
        "a selection goes over the bar"
    );
    assert_eq!(
        edit(power_key(&u("(1)/(2)"), 7, 7)),
        "((1)/(2))^(|)",
        "^ after a bracketed fraction brackets it"
    );
    assert_eq!(
        typ("1/2)^"),
        "(1/(2))^(|)",
        "^ typed after a fraction raises the whole fraction"
    );
    assert_eq!(
        typ("1/2)/"),
        "(1/(2))/(|)",
        "/ after a fraction makes a fraction of it"
    );
    assert_eq!(
        edit(power_key(&u("√(2)"), 4, 4)),
        "√(2)^(|)",
        "^ after a root raises the root"
    );
    assert_eq!(typ("1/x/3"), "1/(x/(3|))", "a nested fraction typed");
}

#[test]
fn mac_checks_backspace() {
    assert_eq!(
        edit(backspace(&u("1/(2)"), &p("1/(2)"), 5)),
        "1/(2|)",
        "⌫ after a fraction steps in"
    );
    assert_eq!(
        edit(backspace(&u("1/(2)"), &p("1/(2)"), 3)),
        "1|2",
        "⌫ at a denominator's start takes the fraction apart"
    );
    assert_eq!(
        edit(backspace(&u("(1)/(2)"), &p("(1)/(2)"), 1)),
        "|12",
        "⌫ at a numerator's start takes it apart"
    );
    assert_eq!(
        edit(backspace(&u("2^(10)"), &p("2^(10)"), 6)),
        "2^(10|)",
        "⌫ after a power steps in"
    );
    assert_eq!(
        edit(backspace(&u("12/(34)"), &p("12/(34)"), 5)),
        "nil",
        "⌫ in the middle is the line's"
    );
}

#[test]
fn mac_checks_mathquill() {
    assert_eq!(typ("1+2/"), "1+2/(|)", "1+/ takes only what's after the +");
    assert_eq!(typ("2x/3"), "(2x)/(3|)", "2x/ takes 2x");
    assert_eq!(typ("1 2/3"), "1 2/(3|)", "a space stops it (1 2/3)");
    assert_eq!(
        typ("sin(x)/2"),
        "sin(x)/(2|)",
        "a function's call goes over whole"
    );
    assert_eq!(typ("2^3)/4"), "(2^(3))/(4|)", "a power goes over whole");
    assert_eq!(
        typ("1/2)x/"),
        "(1/(2)·x)/(|)",
        "a fraction and the term after it go over whole (½x/)"
    );
    assert_eq!(typ("|x|/2"), "(|x|)/(2|)", "|x| goes over whole");
    assert_eq!(typ("x!/2"), "(x!)/(2|)", "x! goes over whole");
    assert_eq!(typ("5%/2"), "(5%)/(2|)", "a percentage goes over whole");
    assert_eq!(
        typ("√(1/2"),
        "√(1/(2|)",
        "/ inside a root takes only what's under it"
    );
    assert_eq!(typ("^2"), "2|", "^ at the start does nothing");
    assert_eq!(typ("x^^2"), "x^(2|)", "^^ raises once");
    assert_eq!(
        typ("x^2)^3"),
        "x^(23|)",
        "^ after a power goes back into it"
    );
    assert_eq!(
        edit(power_key(&u("x^2"), 3, 3)),
        "x^(2|)",
        "…an unbracketed one too"
    );
    assert_eq!(
        edit(power_key(&u("x2"), 1, 2)),
        "x^(2|)",
        "^ over a selection raises it"
    );
    assert_eq!(
        edit(power_key(&u("2"), 0, 1)),
        "nil",
        "^ over a selection with nothing before it does nothing"
    );
    assert_eq!(typ("x^2n"), "x^(2n|)", "x^2n stays in the power");
    assert_eq!(typ("x^2n+y"), "x^(2n)+y|", "x^2n+y: the + leaves the power");
    assert_eq!(
        typ("x^+2n"),
        "x^(+2n|)",
        "x^+2n: a sign in an empty power stays"
    );
    assert_eq!(typ("x^-2n"), "x^(-2n|)", "x^-2n stays");
    assert_eq!(typ("x^=2n"), "x^(=2n|)", "x^=2n stays");
    assert_eq!(
        edit(break_out(&u("x^(ab)"), &p("x^(ab)"), 4, u("+")[0])),
        "nil",
        "+ in the middle of a power stays"
    );
    assert_eq!(typ("2^3=8"), "2^(3)=8|", "= at a power's end leaves it");
    assert_eq!(
        typ("1/2+3"),
        "1/(2+3|)",
        "a fraction's slot is never left by +"
    );
    assert_eq!(bs("x^(nm)|"), "x^(nm|)", "⌫ after a power steps in");
    assert_eq!(bs("x^(|)"), "x|", "⌫ in an empty power takes it");
    assert_eq!(bs("x^()|"), "x|", "⌫ after an empty power takes it whole");
    assert_eq!(
        bs("1+()/()|"),
        "1+|",
        "⌫ after an empty fraction takes it whole"
    );
    assert_eq!(
        bs("1+1/(|)"),
        "1+1|",
        "⌫ in an empty denominator leaves the numerator"
    );
    assert_eq!(
        bs("1+1/(1/(|))"),
        "1+1/(1|)",
        "⌫ in an empty inner denominator"
    );
    assert_eq!(
        bs("1/(1/(2)+|2/(3))"),
        "nil",
        "⌫ at an inner fraction's start"
    );
    assert_eq!(
        bs("1/(1/(2)+2/(3)|)"),
        "1/(1/(2)+2/(3|))",
        "⌫ after a fraction in a denominator steps into it"
    );
    assert_eq!(
        bs("1/(2)·x|"),
        "1/(2)|",
        "⌫ takes a fraction's dot with the one term after it"
    );
    assert_eq!(
        del("(1|)/(2)"),
        "1|2",
        "⌦ at a numerator's end takes the fraction apart"
    );
    assert_eq!(del("1|/(2)"), "1|2", "⌦ at a plain numerator's end too");
    assert_eq!(
        del("(|)/(1/(2)+2/(3))+1"),
        "|1/(2)+2/(3)+1",
        "⌦ in an empty numerator"
    );
    assert_eq!(del("1/(2|)"), "12|", "⌦ at a denominator's end");
    assert_eq!(del("x^(2|)"), "x2|", "⌦ at a power's end");
    assert_eq!(del("x|^()"), "x|", "⌦ before an empty power takes it");
    assert_eq!(
        del("1+|()/()"),
        "1+|",
        "⌦ before an empty fraction takes it"
    );
    assert_eq!(
        del("x|^(2)"),
        "nil",
        "⌦ before a full power is the line's (it steps in)"
    );
    assert_eq!(
        vert("x^(nm)|", true),
        "x^(nm|)",
        "↑ after a power goes into it, at its end"
    );
    assert_eq!(
        vert("x^(nm|)", false),
        "x^(nm)|",
        "↓ from a power's end leaves it after"
    );
    assert_eq!(
        vert("x^(|nm)", false),
        "x|^(nm)",
        "↓ from a power's start leaves it before"
    );
    assert_eq!(
        vert("x^(n|m)", false),
        "x|^(nm)",
        "↓ from a power's middle leaves it before"
    );
    assert_eq!(
        vert("x|^(nm)", true),
        "x^(|nm)",
        "↑ before a power goes into its start"
    );
    assert_eq!(
        vert("(12)/(34)|", true),
        "(12|)/(34)",
        "↑ after a fraction goes to the numerator's end"
    );
    assert_eq!(
        vert("(12)/(34)|", false),
        "(12)/(34|)",
        "↓ after a fraction goes to the denominator's end"
    );
    assert_eq!(
        vert("|(12)/(34)", true),
        "(|12)/(34)",
        "↑ before a fraction goes to the numerator's start"
    );
    assert_eq!(
        vert("|(12)/(34)", false),
        "(12)/(|34)",
        "↓ before a fraction goes to the denominator's start"
    );
    assert_eq!(
        vert("(1|2)/(34)", false),
        "nearest(34)",
        "↓ in a numerator goes across"
    );
    assert_eq!(
        vert("(12)/(3|4)", true),
        "nearest(12)",
        "↑ in a denominator goes across"
    );
    assert_eq!(
        vert("(1|2)/(34)", true),
        "nil",
        "↑ in a numerator is the bar's"
    );
    assert_eq!(
        vert("1/(x+(a|)/(b))", true),
        "nearest(1)",
        "↑ in an inner numerator goes to the outer one"
    );
    assert_eq!(
        vert("1+|2", true),
        "nil",
        "↑ with no stack near is the bar's"
    );
    assert_eq!(
        edge("1/(23)", 5, false),
        "1/(|23)",
        "⌘← in a slot goes to its start"
    );
    assert_eq!(
        edge("1/(23)", 3, false),
        "|1/(23)",
        "…and again to the line's"
    );
    assert_eq!(
        edge("1/(23)+4", 3, true),
        "1/(23|)+4",
        "⌘→ in a slot goes to its end"
    );
    assert_eq!(sel("1+(23)/(45)", 3, 1), "2", "inside a slot stays");
    assert_eq!(
        sel("1+(23)/(45)", 4, 7),
        "(23)/(45)",
        "from a slot past the stack takes it whole"
    );
    assert_eq!(
        sel("1+(23)/(45)", 0, 4),
        "1+(23)/(45)",
        "from before the stack into it takes it whole"
    );
    assert_eq!(
        sel("x^(2)+1", 1, 2),
        "^(2)",
        "a power's base and exponent take the power"
    );
    assert_eq!(
        edit(root_key(&u("1/(2)"), 0, 5, 0x221A)),
        "√(1/(2)|)",
        "√ over a selection puts it under the root"
    );
    assert_eq!(
        tall("2(1/(2)+1)"),
        "1-9",
        "shown brackets around a fraction grow"
    );
    assert_eq!(
        p("√(1/(2))").tall.len().to_string(),
        "1",
        "a root's brackets grow too, under its tall sign"
    );
    assert_eq!(
        p("(1)/(2)").tall.len().to_string(),
        "0",
        "a fraction's own hidden brackets don't"
    );
    assert_eq!(
        p("[1/(2)]").tall.len().to_string(),
        "1",
        "square brackets do"
    );
}

// ---- Windows' own

#[test]
fn slots_of_a_fraction_and_a_power() {
    let x = p("(5x)/(32+x^(3))");
    assert_eq!(x.stacks.len(), 2);
    let f = x.stacks[0];
    assert!(f.fraction);
    assert_eq!((f.start, f.op, f.end), (0, 4, 15));
    assert_eq!(
        f.num,
        Some(Slot {
            lo: 1,
            hi: 3,
            open: Some(0),
            close: Some(3)
        })
    );
    assert_eq!(
        x.hidden.iter().copied().collect::<Vec<_>>(),
        vec![0, 3, 4, 5, 10, 11, 13, 14]
    );
    assert!(x.dead.contains(&4) && x.dead.contains(&5) && x.dead.contains(&11));
    assert_eq!(x.closers.iter().copied().collect::<Vec<_>>(), vec![13, 14]);
    // the innermost slot holds the caret
    assert_eq!(x.slot(12).map(|s| s.lo), Some(12));
    assert_eq!(x.slot(8).map(|s| s.lo), Some(6));
    assert_eq!(x.slot(0), None);
    let y = p("42/(12)+x^2");
    assert_eq!(y.stacks[0].num, Some(Slot::bare(0, 2)));
    assert_eq!(y.stacks[1].slot, Slot::bare(10, 11));
    assert!(!y.dead.contains(&2));
    // a ∫'s `^` is its own
    assert!(parse(&u("∫_0^1 x"), &[(0, 5)]).is_empty());
    assert_eq!(p("∫_0^1 x").stacks.len(), 1);
}

#[test]
fn the_keys_on_a_line_as_typed() {
    let e = |t: &str| {
        let v = u(t);
        edit(Some(slash_key(&v, v.len(), v.len())))
    };
    assert_eq!(e("5x"), "(5x)/(|)");
    assert_eq!(e("(1)/(2)"), "((1)/(2))/(|)");
    assert_eq!(e("x^(2)"), "(x^(2))/(|)");
    // ^ right after a power goes back into it; after a fraction it brackets the fraction
    let pw = |t: &str| {
        let v = u(t);
        edit(power_key(&v, v.len(), v.len()))
    };
    assert_eq!(pw("x^(2)"), "x^(2|)");
    assert_eq!(pw("1/(2)"), "(1/(2))^(|)");
    assert_eq!(pw("2+"), "nil");
    // ) steps out of a slot, a term after a stack gets its `·`, hidden and its place after it dead
    let x = p("(1)/(2)");
    assert_eq!(close_key(&x, 6), Some(7));
    assert_eq!(close_key(&x, 2), Some(4));
    assert_eq!(close_key(&x, 5), None);
    let y = p("(1)/(2)·x");
    assert!(y.dots.contains(&7) && y.hidden.contains(&7) && y.dead.contains(&8));
    assert_eq!(x.dead.iter().copied().collect::<Vec<_>>(), vec![3, 4]);
    assert_eq!(snap(&x, 3, 2, 7), 5);
    assert_eq!(snap(&x, 4, 5, 7), 2);
    // a sign at the end of a power leaves it, and the `·` after it goes
    let z = u("x^(2)·y");
    assert_eq!(edit(break_out(&z, &parse(&z, &[]), 4, 0x2B)), "x^(2)+|y");
}

#[test]
fn word_deletes_and_selections_keep_stacks_whole() {
    let t = "42/(12)*12/(23*12)·12";
    let v = u(t);
    let x = parse(&v, &[]);
    let n = v.len();
    assert_eq!(expand(&x, 1, 4, n), (0, 7));
    assert_eq!(expand(&x, 9, 12, n), (8, 19));
    let term = |i: usize| term_at(&v, i);
    assert_eq!(deletion(&x, 17, 0, n, term), Some((12, 17)));
    assert_eq!(deletion(&x, 12, 0, n, term), None);
    assert_eq!(deletion(&x, 21, 19, n, term), Some((18, 21)));
    assert_eq!(deletion(&x, 7, 0, n, term), Some((0, 7)));
    let y = p("42/(12)");
    assert_eq!(emptying(&y, 0, 2), Some(0));
    assert_eq!(emptying(&y, 4, 6), None);
    assert_eq!(emptying(&p("x^2"), 2, 3), Some(2));
}

#[test]
fn the_limit_the_caret_is_in_is_the_lower_first() {
    use qcalc_core::app::bounds::bounds_in;
    let b = bounds_in("Σ_0^5");
    assert_eq!(limit_slot(&b, 2), Some((0, false)));
    assert_eq!(limit_slot(&b, 4), Some((0, true)));
    assert_eq!(limit_slot(&b, 0), None);
    let b = bounds_in("Σ_1^2 x+Σ_3");
    assert_eq!(limit_slot(&b, 10), Some((1, false)));
}

#[test]
fn signs_between_terms_get_room_and_the_caret_its_middle() {
    let room = |t: &str| {
        let x = p(t);
        spacing(&x.text, &x.hidden, |_| 20.0)
    };
    // 1+2: room before and after the +, the caret either side of it 2 back
    let (r, sh) = room("1+2");
    assert_eq!(r, vec![(0, 4.0), (1, 4.0)]);
    assert_eq!(sh, vec![(1, 2.0), (2, 2.0)]);
    // a sign that starts a term stays tight: -3, (−x, 1e-5, and one at the end
    assert!(room("-3").0.is_empty());
    assert!(room("(−x").0.is_empty());
    assert!(room("1e-5").0.is_empty());
    assert!(room("2+").0.is_empty());
    // `;` gets room after it; a stack's hidden `)` before a sign takes the room before it
    assert_eq!(room("x;y").0, vec![(1, 5.0)]);
    let (r, _) = room("1/(2)+3");
    assert_eq!(r, vec![(4, 4.0), (5, 4.0)]);
}

#[test]
fn slot_sizes_are_texs() {
    // a fraction's slots in the line at 90%, one inside a slot and every power at 70%, nothing under half the line
    assert!((slot_size(24.0, true, 24.0, FRACTION_SCALE) - 21.6).abs() < 1e-4);
    assert!((slot_size(21.6, true, 24.0, FRACTION_SCALE) - 15.12).abs() < 1e-4);
    assert!((slot_size(24.0, false, 24.0, FRACTION_SCALE) - 16.8).abs() < 1e-4);
    assert!((slot_size(15.12, false, 24.0, FRACTION_SCALE) - 12.0).abs() < 1e-4);
    // a line too tall for the bar: 70% in the line
    assert!((slot_size(24.0, true, 24.0, 0.7) - 16.8).abs() < 1e-4);
    let x = p("(1)/((a)/(b))");
    let (size, offset, looks) = looks(&x, 13, 24.0, FRACTION_SCALE);
    assert_eq!(looks.len(), 4);
    assert!((size[1] - 21.6).abs() < 1e-4 && offset[1] > 0.0);
    assert!((size[6] - 15.12).abs() < 1e-4);
    assert!(offset[6] < offset[1] && offset[10] < offset[6]);
    assert_eq!(look_at(&x, &looks, 7).map(|l| l.lo), Some(6));
    assert_eq!(look_at(&x, &looks, 0), None);
}

#[test]
fn fraction_and_power_shifts_are_rule_15_and_18() {
    let s = 24.0;
    let axis = s * tex::AXIS;
    // a plain numerator: TeX's shift (0.394 em over the baseline), as nothing in it reaches lower
    assert!((num_offset(axis, s, 0.0, FRACTION_SCALE) - 0.394 * s).abs() < 1e-4);
    // one with a tail: the gap over the bar's top and the tail's depth
    let tail = 21.6 * tex::DEPTH;
    assert!((num_offset(axis, s, tail, FRACTION_SCALE) - (axis + 0.14 * s + tail)).abs() < 1e-4);
    // a plain denominator: the gap and its height, more than TeX's 0.345 em here
    let h = 21.6 * tex::HEIGHT;
    assert!((den_offset(axis, s, h, FRACTION_SCALE) - (axis - (0.14 * s + h))).abs() < 1e-4);
    // a denominator with a fraction in it stands clear of the bar above it
    let x = p("1/(x/3)");
    let (_, offset, looks) = looks(&x, 7, s, FRACTION_SCALE);
    let inner_num = looks.iter().find(|l| l.lo == 3).unwrap();
    assert!(offset[0] > 0.0 && inner_num.offset + 15.12 * tex::HEIGHT < axis);
    assert!(extent(&x, 3, 6, 21.6, s, FRACTION_SCALE).0 > 21.6 * tex::HEIGHT);
    // a power: 0.363 em up; over grown brackets no lower than their top less the drop
    let y = p("x^(2)");
    assert!((sup_shift(&y, &y.stacks[0], s, s, FRACTION_SCALE) - 0.363 * s).abs() < 1e-4);
    let z = p("(1/(2)+1)^(3)");
    let pw = z.stacks.iter().find(|st| !st.fraction).unwrap();
    let top = extent(&z, 0, 9, s, s, FRACTION_SCALE).0;
    assert!(sup_shift(&z, pw, s, s, FRACTION_SCALE) >= top - 0.25 * s - 1e-4);
}

#[test]
fn a_line_too_tall_for_the_bar_is_set_in_text_style() {
    assert!(fits(&p("(5x)/(32+x^(3))"), 24.0, 72.0));
    assert!(fits(&p("2^(10)+1/(x/(3))+(1/(2))/(4)"), 24.0, 72.0));
    // a fraction in a denominator under a root
    assert!(!fits(&p("√(1/(1+1/(1+1/(x))))"), 24.0, 72.0));
    assert!(rooted(&p("√(1/(2))")) && !rooted(&p("√2+1/(2)")));
    // grown brackets: as far either side of the axis as what they hold, and never under the size
    assert!((tex::bracket_half(10.0, 0.0, 24.0) - 12.0).abs() < 1e-4);
    assert!((tex::bracket_half(30.0, 20.0, 24.0) - 23.426).abs() < 1e-2);
}

#[test]
fn a_history_rows_fraction_fits_its_row() {
    // 4/(233*212^3) at 13: the power in the denominator, the whole fraction inside the 36 pt row
    let x = p("4/(233*212^(3))");
    let e = extent(&x, 0, x.text.len(), 13.0, 13.0, FRACTION_SCALE);
    assert!(e.0 + e.1 < 34.0, "{e:?}");
}
