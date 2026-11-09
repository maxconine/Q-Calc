//! "Stacked fractions and powers" (Settings › General, off by default; native only, StackedMath.swift): the line is
//! typed as a graphing calculator's is. `/` puts what's before the caret over a bar and the caret under it; `^` puts
//! the caret up in a small raised power. The text stays plain maths the engine reads, `(5x)/(32+x^(3))`: each slot
//! is a bracketed group whose brackets, with the `/` and `^`, are hidden, and stacked_layout.rs draws the slots
//! stacked. This is the text side, pure: what's stacked, where the caret may stop, the edits the keys make (as
//! MathQuill, Desmos's editor, makes them), and the sizes and gaps TeX gives each slot (StackedLayout.swift `TeX`).
//!
//! Offsets are UTF-16 (as the layout's and the Mac's are); the field converts at its edges.

use std::collections::BTreeSet;

/// One place for typing: the characters `lo..hi` between `open` and `close` (hidden brackets), or a bare run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub lo: usize,
    pub hi: usize,
    pub open: Option<usize>,
    pub close: Option<usize>,
}

impl Slot {
    const fn bare(lo: usize, hi: usize) -> Slot {
        Slot {
            lo,
            hi,
            open: None,
            close: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.hi == self.lo
    }

    pub fn len(&self) -> usize {
        self.hi - self.lo
    }

    /// The caret is in the slot from its first place to its last, both ends counted.
    pub fn holds(&self, caret: usize) -> bool {
        caret >= self.lo && caret <= self.hi
    }
}

/// A fraction (`num` over `slot`) or a power (`slot` raised; its base is left as typed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stack {
    pub fraction: bool,
    /// the `/` or `^`
    pub op: usize,
    /// the first character the stack takes (a hidden `(` or the numerator's first), and the one after its last
    pub start: usize,
    pub end: usize,
    pub num: Option<Slot>,
    pub slot: Slot,
}

impl Stack {
    pub fn slots(&self) -> impl Iterator<Item = Slot> + '_ {
        self.num.iter().copied().chain(std::iter::once(self.slot))
    }

    /// Where the stack begins as the keys see it: a fraction's first character, a power's `^`.
    fn head(&self) -> usize {
        if self.fraction { self.start } else { self.op }
    }
}

/// Shown brackets, `( )` or `[ ]`, around a stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pair {
    pub open: usize,
    pub close: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Parsed {
    /// outer stacks first (by start, the longer first)
    pub stacks: Vec<Stack>,
    /// characters drawn as nothing: the `/` and `^`, the slots' brackets, and the `·` put after a stack
    pub hidden: BTreeSet<usize>,
    /// a `·` between a stack and what's typed after it (the engine reads `(1)/(2)x` as 1/(2x))
    pub dots: BTreeSet<usize>,
    /// caret places that look the same as the one beside them, so the caret never stops there
    pub dead: BTreeSet<usize>,
    /// the hidden `)` that ends a stack's last slot: → or `)` there steps out, backspace after it steps in
    pub closers: BTreeSet<usize>,
    /// shown brackets with a stack inside: drawn as tall as what they hold (TeX's \left \right)
    pub tall: Vec<Pair>,
    /// the line's characters, for what depends on them (a slot's tails, a root's bar)
    pub text: Vec<u16>,
}

impl Parsed {
    pub fn is_empty(&self) -> bool {
        self.stacks.is_empty()
    }

    /// The innermost slot the caret is in.
    pub fn slot(&self, caret: usize) -> Option<Slot> {
        self.stacks
            .iter()
            .flat_map(Stack::slots)
            .filter(|s| s.holds(caret))
            .min_by_key(Slot::len)
    }
}

// ---- characters

const OPEN: u16 = 0x28;
const CLOSE: u16 = 0x29;
const SLASH: u16 = 0x2F;
const CARET: u16 = 0x5E;
const DOT: u16 = 0xB7;
const BANG: u16 = 0x21;
const PERCENT: u16 = 0x25;
const DEGREE: u16 = 0xB0;
const BAR: u16 = 0x7C;
const ROOT: u16 = 0x221A;
const CUBE_ROOT: u16 = 0x221B;

pub fn is_digit(c: u16) -> bool {
    (48..=57).contains(&c) || c == 46
}

pub fn is_letter(c: u16) -> bool {
    (65..=90).contains(&c) || (97..=122).contains(&c) || (0x391..=0x3C9).contains(&c) || c == 0x221E
}

pub fn is_word(c: u16) -> bool {
    is_digit(c) || is_letter(c)
}

/// What starts a term: typed right after a stack it multiplies it.
pub fn starts_term(c: u16) -> bool {
    is_word(c) || c == OPEN || c == ROOT || c == CUBE_ROOT
}

/// What may come before a stacked numerator: a fraction is only drawn stacked where the engine reads it so.
fn boundary(c: u16) -> bool {
    " +-−×·*(,=<>±÷:[{|".encode_utf16().any(|b| b == c)
}

/// What after a denominator would take it with it (`(a)/(b)c` is a/(bc), `(a)/(b)^2` is a/b²).
fn binds(c: u16) -> bool {
    starts_term(c) || c == CARET || c == BANG || c == PERCENT || c == 0x5B
}

/// Each bracket's partner: (close of an open, open of a close).
fn pairs(u: &[u16]) -> (Vec<Option<usize>>, Vec<Option<usize>>) {
    let mut close_of = vec![None; u.len()];
    let mut open_of = vec![None; u.len()];
    let mut stack = Vec::new();
    for (i, &c) in u.iter().enumerate() {
        if c == OPEN {
            stack.push(i);
        }
        if c == CLOSE
            && let Some(o) = stack.pop()
        {
            close_of[o] = Some(i);
            open_of[i] = Some(o);
        }
    }
    (close_of, open_of)
}

/// Each `(` with its `)`, as (open, close) pairs, inner and outer (the history's rows draw a root around one).
pub fn bracket_pairs(u: &[u16]) -> Vec<Option<usize>> {
    pairs(u).0
}

fn all(u: &[u16], a: usize, b: usize, test: fn(u16) -> bool) -> bool {
    a < b && u[a..b].iter().all(|&c| test(c))
}

fn plain(u: &[u16], a: usize, b: usize) -> bool {
    all(u, a, b, is_digit) || all(u, a, b, is_letter)
}

/// Whether `s` could have a stack at all (a cheap test before `parse`).
pub fn may_stack(s: &str) -> bool {
    s.contains('/') || s.contains('^')
}

// ---- what's stacked

/// The fractions and powers in `u`. `exclude`: the ∫ Σ Π limits (sign..end), whose `^` are theirs.
pub fn parse(u: &[u16], exclude: &[(usize, usize)]) -> Parsed {
    let n = u.len();
    let (close_of, open_of) = pairs(u);
    let mut p = Parsed::default();
    let boundary_ok = |j: usize| j == 0 || boundary(u[j - 1]);
    let excluded = |i: usize| exclude.iter().any(|&(a, b)| i >= a && i < b);

    for i in 0..n {
        if u[i] != SLASH || excluded(i) || i == 0 || i + 1 >= n {
            continue;
        }
        // the numerator: a bracketed group (its brackets hidden), a function's call, a number or a name
        let num = if u[i - 1] == CLOSE
            && let Some(o) = open_of[i - 1]
        {
            let mut j = o;
            while j > 0 && is_letter(u[j - 1]) {
                j -= 1;
            }
            if j < o {
                if !boundary_ok(j) {
                    continue;
                }
                Slot::bare(j, i)
            } else {
                if !boundary_ok(o) {
                    continue;
                }
                Slot {
                    lo: o + 1,
                    hi: i - 1,
                    open: Some(o),
                    close: Some(i - 1),
                }
            }
        } else {
            let mut j = i;
            while j > 0 && is_word(u[j - 1]) {
                j -= 1;
            }
            if !plain(u, j, i) || !boundary_ok(j) {
                continue;
            }
            Slot::bare(j, i)
        };
        // the denominator: a bracketed group, or a number or a name
        let (den, after) = if u[i + 1] == OPEN
            && let Some(c) = close_of[i + 1]
        {
            (
                Slot {
                    lo: i + 2,
                    hi: c,
                    open: Some(i + 1),
                    close: Some(c),
                },
                c + 1,
            )
        } else {
            let mut j = i + 1;
            while j < n && is_word(u[j]) {
                j += 1;
            }
            if !plain(u, i + 1, j) {
                continue;
            }
            (Slot::bare(i + 1, j), j)
        };
        if after < n && binds(u[after]) {
            continue;
        }
        p.stacks.push(Stack {
            fraction: true,
            op: i,
            start: num.open.unwrap_or(num.lo),
            end: after,
            num: Some(num),
            slot: den,
        });
        p.hidden.insert(i);
        for k in [num.open, num.close, den.open, den.close]
            .into_iter()
            .flatten()
        {
            p.hidden.insert(k);
        }
        if num.close.is_some() {
            p.dead.insert(i);
        }
        if den.open.is_some() {
            p.dead.insert(i + 1);
        }
        if let Some(c) = den.close {
            p.closers.insert(c);
        }
    }

    for i in 0..n {
        if u[i] != CARET || excluded(i) || i == 0 || i + 1 >= n {
            continue;
        }
        if !(is_word(u[i - 1]) || u[i - 1] == CLOSE || u[i - 1] == BANG) {
            continue;
        }
        let exp = if u[i + 1] == OPEN
            && let Some(c) = close_of[i + 1]
        {
            Slot {
                lo: i + 2,
                hi: c,
                open: Some(i + 1),
                close: Some(c),
            }
        } else {
            let mut j = i + 1;
            while j < n && is_digit(u[j]) {
                j += 1;
            }
            if j == i + 1 && is_letter(u[i + 1]) && (i + 2 >= n || !is_letter(u[i + 2])) {
                j = i + 2;
            }
            if j == i + 1 {
                continue;
            }
            Slot::bare(i + 1, j)
        };
        p.stacks.push(Stack {
            fraction: false,
            op: i,
            start: i,
            end: exp.close.map_or(exp.hi, |c| c + 1),
            num: None,
            slot: exp,
        });
        p.hidden.insert(i);
        if let (Some(o), Some(c)) = (exp.open, exp.close) {
            p.hidden.insert(o);
            p.hidden.insert(c);
            p.dead.insert(i + 1);
            p.closers.insert(c);
        }
    }

    // the `·` between a stack and the term typed after it is drawn as a small gap
    let ends: Vec<usize> = p
        .stacks
        .iter()
        .filter(|s| s.slot.close.is_some())
        .map(|s| s.end)
        .collect();
    for k in ends {
        if k + 1 < n && u[k] == DOT && starts_term(u[k + 1]) {
            p.dots.insert(k);
            p.hidden.insert(k);
            p.dead.insert(k + 1);
        }
    }
    p.stacks
        .sort_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)));
    p.text = u.to_vec();
    // shown brackets with a stack inside grow around it
    if !p.stacks.is_empty() {
        let mut shown: Vec<Pair> = close_of
            .iter()
            .enumerate()
            .filter_map(|(o, c)| c.map(|c| Pair { open: o, close: c }))
            .filter(|b| !p.hidden.contains(&b.open))
            .collect();
        let mut squares = Vec::new();
        for (i, &c) in u.iter().enumerate() {
            if c == 0x5B {
                squares.push(i);
            }
            if c == 0x5D
                && let Some(o) = squares.pop()
            {
                shown.push(Pair { open: o, close: i });
            }
        }
        shown.retain(|b| {
            p.stacks
                .iter()
                .any(|s| s.start > b.open && s.end <= b.close)
        });
        shown.sort_by_key(|b| b.open);
        p.tall = shown;
    }
    p
}

// ---- the keys

/// An edit a key makes: the whole line and the caret (UTF-16).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub text: Vec<u16>,
    pub caret: usize,
}

fn splice(u: &[u16], a: usize, b: usize, with: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(u.len() + with.len());
    out.extend_from_slice(&u[..a]);
    out.extend_from_slice(with);
    out.extend_from_slice(&u[b..]);
    out
}

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// `(` + u[a..b] + `)` + `tail`
fn bracketed(u: &[u16], a: usize, b: usize, tail: &str) -> Vec<u16> {
    let mut v = vec![OPEN];
    v.extend_from_slice(&u[a..b]);
    v.push(CLOSE);
    v.extend(tail.encode_utf16());
    v
}

/// Where the term that ends at `c` starts, as a graphing calculator takes it for `/` or `^`: a number, a name,
/// a function's call, a bracketed group, an absolute value, a root, `!` `%` `°`, and the powers and fractions
/// already made of them. MathQuill's `/` takes everything back to an operator, a comma or a space; so does this.
pub fn term_start(u: &[u16], c: usize) -> usize {
    let (_, open_of) = pairs(u);
    let mut dots: Option<BTreeSet<usize>> = None;
    let mut j = c;
    while j > 0 {
        let ch = u[j - 1];
        if ch == CLOSE
            && let Some(o) = open_of[j - 1]
        {
            j = o;
            continue;
        }
        if is_word(ch)
            || ch == ROOT
            || ch == CUBE_ROOT
            || ch == BANG
            || ch == PERCENT
            || ch == DEGREE
        {
            j -= 1;
            continue;
        }
        if j < c && ch == CARET && j >= 2 {
            j -= 1;
            continue;
        }
        // a fraction already made, its numerator bracketed or a plain number or name (`1/(2)` then ^ is (½)²)
        if j < c
            && ch == SLASH
            && j >= 2
            && (u[j - 2] == CLOSE || is_word(u[j - 2]))
            && u[j] == OPEN
        {
            j -= 1;
            continue;
        }
        // |x| is one term
        if ch == BAR
            && j >= 2
            && let Some(o) = (0..j - 1).rev().find(|&k| u[k] == BAR)
            && o + 2 < j
        {
            j = o;
            continue;
        }
        // the hidden `·` between a stack and the term typed after it: ½x is one term, as MathQuill takes it
        if j < c && ch == DOT {
            let d = dots.get_or_insert_with(|| parse(u, &[]).dots);
            if d.contains(&(j - 1)) {
                j -= 1;
                continue;
            }
        }
        break;
    }
    j
}

/// `/` typed: what's selected, or the term before the caret, goes over the bar, and the caret into the
/// denominator. With nothing before it the caret goes into an empty numerator.
pub fn slash_key(u: &[u16], start: usize, end: usize) -> Edit {
    if end > start {
        return Edit {
            text: splice(u, start, end, &bracketed(u, start, end, "/()")),
            caret: end + 4,
        };
    }
    let c = start;
    let j = term_start(u, c);
    if j == c {
        return Edit {
            text: splice(u, c, c, &w("()/()")),
            caret: c + 1,
        };
    }
    let (close_of, open_of) = pairs(u);
    let group = u[j] == OPEN && close_of[j] == Some(c - 1);
    // a function's call, `sin(x)`, is a numerator as it stands
    let call = u[c - 1] == CLOSE && open_of[c - 1].is_some_and(|o| all(u, j, o, is_letter));
    if group || ((plain(u, j, c) || call) && (j == 0 || boundary(u[j - 1]))) {
        return Edit {
            text: splice(u, c, c, &w("/()")),
            caret: c + 2,
        };
    }
    Edit {
        text: splice(u, j, c, &bracketed(u, j, c, "/()")),
        caret: c + 4,
    }
}

/// `^` typed: the caret goes up into an empty power. A fraction before it is bracketed first (½ then ^ is (½)³,
/// not 1/2³); right after a power it goes back into that power, at its end (MathQuill's weld: x², ^ is x^(2|),
/// never (x²)^). With a selection, the selection is the power. None with nothing to raise: the key does nothing
/// (MathQuill's supSubsRequireOperand, as Desmos sets it).
pub fn power_key(u: &[u16], start: usize, end: usize) -> Option<Edit> {
    if end <= start {
        return power_at(u, start);
    }
    let sel = &u[start..end];
    let rest = splice(u, start, end, &[]);
    let e = power_at(&rest, start)?;
    Some(Edit {
        text: splice(&e.text, e.caret, e.caret, sel),
        caret: e.caret + sel.len(),
    })
}

fn power_at(u: &[u16], c: usize) -> Option<Edit> {
    if c == 0 {
        return None;
    }
    let p = parse(u, &[]);
    if let Some(s) = p.stacks.iter().find(|s| !s.fraction && s.end == c) {
        if let Some(close) = s.slot.close {
            return Some(Edit {
                text: u.to_vec(),
                caret: close,
            });
        }
        return Some(Edit {
            text: splice(
                u,
                s.slot.lo,
                s.slot.hi,
                &bracketed(u, s.slot.lo, s.slot.hi, ""),
            ),
            caret: s.slot.hi + 1,
        });
    }
    let j = term_start(u, c);
    if j >= c {
        return None;
    }
    // the term ends with a stack (it isn't a bracketed group on its own): bracket it
    if p.stacks.iter().any(|s| s.end == c && s.start >= j) {
        return Some(Edit {
            text: splice(u, j, c, &bracketed(u, j, c, "^()")),
            caret: c + 4,
        });
    }
    Some(Edit {
        text: splice(u, c, c, &w("^()")),
        caret: c + 2,
    })
}

/// `√` or `∛` typed over a selection: the selection goes under the root, the caret at its end, inside.
pub fn root_key(u: &[u16], start: usize, end: usize, sign: u16) -> Option<Edit> {
    if end <= start {
        return None;
    }
    let mut with = vec![sign];
    with.extend(bracketed(u, start, end, ""));
    Some(Edit {
        text: splice(u, start, end, &with),
        caret: end + 2,
    })
}

/// What typed at the end of a power leaves it first (MathQuill's charsThatBreakOutOfSupSub, as Desmos sets it).
fn breaks_out(c: u16) -> bool {
    "+-−=<>≤≥≠±".encode_utf16().any(|b| b == c)
}

/// A sign typed at the end of a non-empty power: x², + is x²+, not x^(2+). Never from an empty power (x^-2) or
/// from the middle of one. A hidden `·` after the power goes: the sign keeps the terms apart now.
pub fn break_out(u: &[u16], p: &Parsed, c: usize, typed: u16) -> Option<Edit> {
    if !breaks_out(typed)
        || !p
            .stacks
            .iter()
            .any(|s| !s.fraction && s.slot.close == Some(c) && !s.slot.is_empty())
    {
        return None;
    }
    let after = if p.dots.contains(&(c + 1)) {
        c + 2
    } else {
        c + 1
    };
    Some(Edit {
        text: splice(u, c + 1, after, &[typed]),
        caret: c + 2,
    })
}

/// `)` typed right before a slot's hidden `)`: where the caret steps out to instead.
pub fn close_key(p: &Parsed, c: usize) -> Option<usize> {
    if p.closers.contains(&c) {
        return Some(c + 1);
    }
    // out of the numerator, into the denominator
    if p.stacks
        .iter()
        .any(|s| s.num.and_then(|n| n.close) == Some(c))
    {
        return Some(c + 2);
    }
    None
}

/// A term typed right after a stack: a `·` goes first, so the engine multiplies it by the whole stack.
pub fn term_after_stack(u: &[u16], p: &Parsed, c: usize, typed: u16) -> Option<Edit> {
    if c == 0 || !starts_term(typed) || !p.closers.contains(&(c - 1)) {
        return None;
    }
    Some(Edit {
        text: splice(u, c, c, &[DOT, typed]),
        caret: c + 2,
    })
}

/// A stack taken apart (MathQuill's unwrapGramp): its slots' text left in the line, the caret where it was in
/// `slot`, at its start or its end.
fn unwrap(u: &[u16], p: &Parsed, s: &Stack, slot: Slot, at_end: bool) -> Edit {
    let end = if p.dots.contains(&s.end) {
        s.end + 1
    } else {
        s.end
    };
    if let (true, Some(num)) = (s.fraction, s.num) {
        let mut joined = u[num.lo..num.hi].to_vec();
        joined.extend_from_slice(&u[s.slot.lo..s.slot.hi]);
        let (la, lb) = (num.len(), s.slot.len());
        let at = match (slot == num, at_end) {
            (true, true) => la,
            (true, false) => 0,
            (false, true) => la + lb,
            (false, false) => la,
        };
        return Edit {
            text: splice(u, s.start, end, &joined),
            caret: s.start + at,
        };
    }
    let e = &u[s.slot.lo..s.slot.hi];
    Edit {
        text: splice(u, s.op, end, e),
        caret: s.op + if at_end { e.len() } else { 0 },
    }
}

/// A stack with nothing in it goes whole (MathQuill deletes an empty command in one key).
fn remove(u: &[u16], p: &Parsed, s: &Stack) -> Edit {
    let from = s.head();
    let to = if p.dots.contains(&s.end) {
        s.end + 1
    } else {
        s.end
    };
    Edit {
        text: splice(u, from, to, &[]),
        caret: from,
    }
}

/// Backspace: after a stack it steps in (the last slot's end), or takes it whole if it's empty; at the start of
/// a slot it takes the stack apart, its slots' text left in the line. None leaves the key to the line.
pub fn backspace(u: &[u16], p: &Parsed, c: usize) -> Option<Edit> {
    if c == 0 {
        return None;
    }
    if p.closers.contains(&(c - 1)) {
        if let Some(s) = p.stacks.iter().find(|s| s.slot.close == Some(c - 1))
            && s.slots().all(|g| g.is_empty())
        {
            return Some(remove(u, p, s));
        }
        return Some(Edit {
            text: u.to_vec(),
            caret: c - 1,
        });
    }
    // the one term typed after a stack: its `·` goes with it
    if c >= 3
        && u[c - 2] == DOT
        && p.closers.contains(&(c - 3))
        && starts_term(u[c - 1])
        && (c == u.len() || !starts_term(u[c]))
    {
        return Some(Edit {
            text: splice(u, c - 2, c, &[]),
            caret: c - 2,
        });
    }
    for s in p.stacks.iter().rev() {
        if let Some(hit) = s.slots().find(|g| g.open.is_some() && g.lo == c) {
            return Some(unwrap(u, p, s, hit, false));
        }
    }
    None
}

/// Delete: before a stack with nothing in it, it takes it whole; at the end of a slot, the stack's hidden end
/// next, it takes the stack apart (as backspace does at a slot's start). None leaves the key to the line, which
/// steps over a stack's hidden characters into it.
pub fn forward_delete(u: &[u16], p: &Parsed, c: usize) -> Option<Edit> {
    if let Some(s) = p
        .stacks
        .iter()
        .find(|s| s.head() == c && s.slots().all(|g| g.is_empty()))
    {
        return Some(remove(u, p, s));
    }
    let mut best: Option<(&Stack, Slot)> = None;
    for s in &p.stacks {
        for slot in s.slots().filter(|g| g.hi == c) {
            let hidden_end =
                slot.close == Some(c) || (s.fraction && Some(slot) == s.num && s.op == c);
            if hidden_end && best.is_none_or(|b| slot.len() < b.1.len()) {
                best = Some((s, slot));
            }
        }
    }
    let (s, slot) = best?;
    Some(unwrap(u, p, s, slot, true))
}

/// Where ↑ or ↓ takes the caret: a place, or the nearest place across in a slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vertical {
    To(usize),
    Nearest(Slot),
}

/// As MathQuill moves: a stack right after the caret first (↑ into a fraction's numerator or a power's start, ↓
/// into the denominator), then one right before it (to the numerator's end, the power's end, the denominator's
/// end), then out of the slots the caret is in, innermost first (numerator ↔ denominator; ↓ out of a power, to
/// after it from its end and to before it from anywhere else). None leaves the key to the bar (the history).
pub fn vertical(p: &Parsed, c: usize, up: bool) -> Option<Vertical> {
    for s in p.stacks.iter().filter(|s| s.head() == c) {
        let to = match (s.fraction, up) {
            (true, true) => s.num.map(|n| n.lo),
            (true, false) => Some(s.slot.lo),
            (false, true) => Some(s.slot.lo),
            (false, false) => None,
        };
        if let Some(to) = to
            && to != c
        {
            return Some(Vertical::To(to));
        }
    }
    let mut before: Vec<&Stack> = p.stacks.iter().filter(|s| s.end == c).collect();
    before.sort_by_key(|s| std::cmp::Reverse(s.end - s.start));
    for s in before {
        let to = match (s.fraction, up) {
            (true, true) => s.num.map(|n| n.hi),
            (true, false) => Some(s.slot.hi),
            (false, true) => Some(s.slot.hi),
            (false, false) => None,
        };
        if let Some(to) = to
            && to != c
        {
            return Some(Vertical::To(to));
        }
    }
    let mut around: Vec<(&Stack, Slot)> = p
        .stacks
        .iter()
        .flat_map(|s| s.slots().filter(|g| g.holds(c)).map(move |g| (s, g)))
        .collect();
    around.sort_by_key(|a| a.1.len());
    for (s, slot) in around {
        if let (true, Some(num)) = (s.fraction, s.num) {
            if slot == num && !up {
                return Some(Vertical::Nearest(s.slot));
            }
            if slot == s.slot && up {
                return Some(Vertical::Nearest(num));
            }
        } else if !up {
            let to = if c == slot.hi { s.end } else { s.op };
            if to != c {
                return Some(Vertical::To(to));
            }
        }
    }
    None
}

/// Home / End (and Ctrl+← Ctrl+→ at a slot's edge): to the start or end of the slot the caret is in (MathQuill's
/// Home and End), and from there to the line's.
pub fn line_edge(p: &Parsed, c: usize, forward: bool, len: usize) -> usize {
    if let Some(slot) = p.slot(c) {
        let e = if forward { slot.hi } else { slot.lo };
        if e != c {
            return e;
        }
    }
    if forward { len } else { 0 }
}

/// The caret moved onto a place it never stops: on, the way it was going.
pub fn snap(p: &Parsed, caret: usize, old: usize, len: usize) -> usize {
    let mut c = caret;
    let forward = caret >= old;
    while p.dead.contains(&c) {
        if forward && c < len {
            c += 1;
        } else if !forward && c > 0 {
            c -= 1;
        } else {
            break;
        }
    }
    c
}

// ---- selections and deletions that would cut a stack

/// A selection `lo..hi` grown so it never takes part of a stack: one inside a single slot stays, one that reaches
/// past a slot takes the whole stack (and the hidden `·` after one it takes whole).
pub fn expand(p: &Parsed, lo: usize, hi: usize, len: usize) -> (usize, usize) {
    if hi <= lo {
        return (lo, hi);
    }
    let (mut lo, mut hi) = (lo, hi);
    let mut changed = true;
    while changed {
        changed = false;
        for s in &p.stacks {
            if !(lo < s.end && hi > s.start) || (lo <= s.start && hi >= s.end) {
                continue;
            }
            if s.slots().any(|g| lo >= g.lo && hi <= g.hi) {
                continue;
            }
            lo = lo.min(s.start);
            hi = hi.max(s.end);
            changed = true;
        }
    }
    if hi < len && p.dots.contains(&hi) && p.stacks.iter().any(|s| s.end == hi && s.start >= lo) {
        hi += 1;
    }
    (lo, hi)
}

/// What a word delete takes, from the caret to `to`: kept inside the slot the caret is in, and grown to whole
/// stacks outside one. None when the caret is at its slot's edge (backspace there takes the stack apart instead).
/// `term_at(i)`: whether a term starts at `i` (the hidden `·` before what's taken goes too when none is left).
pub fn deletion(
    p: &Parsed,
    caret: usize,
    to: usize,
    len: usize,
    term_at: impl Fn(usize) -> bool,
) -> Option<(usize, usize)> {
    let (mut lo, mut hi) = (caret.min(to), caret.max(to));
    if let Some(slot) = p.slot(caret) {
        lo = lo.max(slot.lo);
        hi = hi.min(slot.hi);
    }
    if hi <= lo {
        return None;
    }
    let (mut lo, hi) = expand(p, lo, hi, len);
    if lo > 0 && p.dots.contains(&(lo - 1)) && !term_at(hi) {
        lo -= 1;
    }
    Some((lo, hi))
}

/// A deletion of `lo..hi` that would take all of a slot with no hidden brackets (`42/(12)`'s 42, `x^2`'s 2): the
/// `/` or `^` would be left with nothing on that side, so the slot is left as an empty `()` instead. The slot's
/// first place, or None when `lo..hi` doesn't empty one.
pub fn emptying(p: &Parsed, lo: usize, hi: usize) -> Option<usize> {
    if hi <= lo {
        return None;
    }
    p.stacks
        .iter()
        .flat_map(Stack::slots)
        .find(|s| s.open.is_none() && s.lo == lo && s.hi == hi)
        .map(|s| s.lo)
}

/// The ∫ Σ Π limit a caret at `c` types into (bounds.ts `slotOf`: each sign's lower limit first, both ends
/// counted): which sign, and whether it's the upper limit.
pub fn limit_slot(limits: &[qcalc_core::app::bounds::Bound], c: usize) -> Option<(usize, bool)> {
    let c = c as i64;
    let inside = |s: qcalc_core::app::bounds::Slot| c >= s.start && c <= s.end;
    limits.iter().enumerate().find_map(|(i, b)| {
        if inside(b.lower) {
            Some((i, false))
        } else if b.upper.is_some_and(inside) {
            Some((i, true))
        } else {
            None
        }
    })
}

/// Whether a term starts at `i` (a digit, a letter, a bracket, a root): what keeps a hidden `·` needed.
pub fn term_at(u: &[u16], i: usize) -> bool {
    i < u.len() && starts_term(u[i])
}

// ---- room around signs

/// DIPs at character or caret places: (index, how much).
pub type Spans = Vec<(usize, f32)>;

/// `+ − = < > ≤ ≥ ± ≠` between two terms get room each side, `; ,` room after (spaceOperators), as a graphing
/// calculator spaces them. A sign that starts a term (`-3`, `(−x`, `1e-5`) stays tight. Returns the room added
/// after characters (index, DIPs) and, for each caret place beside a spaced sign, how far back the caret is drawn
/// so it stands in the middle of the room, never against the sign. `size(k)`: character k's size.
pub fn spacing(u: &[u16], hidden: &BTreeSet<usize>, size: impl Fn(usize) -> f32) -> (Spans, Spans) {
    let n = u.len();
    let (mut room, mut shift) = (Vec::new(), Vec::new());
    if n < 2 {
        return (room, shift);
    }
    let before = |k: usize| (0..k).rev().find(|j| !hidden.contains(j));
    for k in (0..n).filter(|k| !hidden.contains(k)) {
        let c = u[k];
        if "+-−=<>≤≥±≠".encode_utf16().any(|b| b == c) {
            let Some(b) = before(k).filter(|_| k + 1 < n) else {
                continue;
            };
            let p = u[b];
            // a sign that starts a term
            if !(is_word(p) || [CLOSE, 0x5D, BANG, PERCENT, 0xB2, 0xB3, 0x3C0].contains(&p)) {
                continue;
            }
            if (c == 0x2D || c == 0x2B) && (p == 0x65 || p == 0x45) && b > 0 && is_digit(u[b - 1]) {
                continue;
            }
            // a hidden `)` before the sign belongs to a slot that ends there: the room goes after it
            let gap = size(k) * 0.2;
            room.push((k - 1, gap));
            room.push((k, gap));
            shift.push((k, gap / 2.0));
            shift.push((k + 1, gap / 2.0));
        } else if (c == 0x3B || c == 0x2C) && k + 1 < n {
            room.push((k, size(k) * 0.25));
            shift.push((k + 1, size(k) * 0.125));
        }
    }
    (room, shift)
}

// ---- how a slot looks: TeX's sizes and gaps

/// TeX's maths constants (TeXbook Appendix G, under their OpenType MATH names; Latin Modern Math's values), in ems
/// of the size of the style they're used in. The bar is TeX's text style with display style's gaps: a line of its
/// own, set tight as a graphing calculator sets it.
pub mod tex {
    /// AxisHeight (σ22): a fraction's bar, a bracket's middle
    pub const AXIS: f32 = 0.25;
    /// FractionRuleThickness (ξ8, θ)
    pub const RULE: f32 = 0.04;
    /// FractionNumeratorShiftUp (σ9, num2)
    pub const NUM_UP: f32 = 0.394;
    /// FractionDenominatorShiftDown (σ12, denom2)
    pub const DEN_DOWN: f32 = 0.345;
    /// Fraction{Numerator,Denominator}DisplayStyleGapMin (3θ)
    pub const GAP: f32 = 0.12;
    /// Fraction{Numerator,Denominator}GapMin (θ): text style
    pub const GAP_TEXT: f32 = 0.04;
    /// SuperscriptShiftUp (σ14, sup2)
    pub const SUP_UP: f32 = 0.363;
    /// SuperscriptBottomMin (¼ σ5)
    pub const SUP_BOTTOM: f32 = 0.108;
    /// SuperscriptBaselineDropMax (σ18), for a base that isn't a glyph
    pub const SUP_DROP: f32 = 0.25;
    /// ScriptPercentScaleDown
    pub const SCRIPT: f32 = 0.7;
    /// ScriptScriptPercentScaleDown: nothing smaller
    pub const SCRIPT_SCRIPT: f32 = 0.5;
    /// \delimiterfactor 901
    pub const DELIM_FACTOR: f32 = 0.901;
    /// \delimitershortfall 5pt at 10pt
    pub const DELIM_SHORTFALL: f32 = 0.5;
    /// a box's height in the line's font: a digit's (with room for a capital's), and a tail's depth
    pub const HEIGHT: f32 = 0.74;
    pub const DEPTH: f32 = 0.21;
    /// Segoe UI's x-height and cap height
    pub const X_HEIGHT: f32 = 0.5;
    pub const CAP: f32 = 0.7;

    /// the gap a fraction's slot keeps off its bar: display style's at Desmos's 90%, text style's on a squeezed line
    pub fn gap(top: f32) -> f32 {
        if top < super::FRACTION_SCALE - 1e-3 {
            GAP_TEXT
        } else {
            GAP
        }
    }

    /// RadicalDisplayStyleVerticalGap (rule 11): θ + ¼ x-height between what's under a root and its bar
    pub fn radical_gap(size: f32) -> f32 {
        RULE * size + X_HEIGHT * size / 4.0
    }

    /// what reaches below the baseline
    pub fn tail(c: u16) -> bool {
        "gjpqy,;()[]|".encode_utf16().any(|t| t == c)
    }

    /// How tall shown brackets are drawn around something `height` over and `depth` under the baseline, at `size`
    /// (TeX's \left \right: as far each way from the axis as the larger, less the shortfall, and never under the
    /// factor of it): the half-height from the axis.
    pub fn bracket_half(height: f32, depth: f32, size: f32) -> f32 {
        let a = AXIS * size;
        let d = 2.0 * (height - a).max(depth + a);
        (d * DELIM_FACTOR).max(d - DELIM_SHORTFALL * size).max(size) / 2.0
    }
}

/// A fraction's slots in the line: MathQuill's (Desmos's) 90%, between TeX's display (100%) and text (70%) styles.
pub const FRACTION_SCALE: f32 = 0.9;
/// How far a fraction's bar reaches past the wider slot, in ems of the slot's size.
pub const BAR_OVERHANG: f32 = 0.2;

/// A root's stroke (RootSign.lineWidth).
pub fn root_line_width(size: f32) -> f32 {
    (size / 17.0).max(1.0)
}

/// How far a root's bar (its stroke's top edge) is over the baseline of a plain radicand (RootSign.top).
pub fn root_top(size: f32) -> f32 {
    size * tex::CAP + size * 0.2 + root_line_width(size) / 2.0
}

/// A slot's size: a fraction's in the line nearly the line's (`top`: 90%, or TeX's text style's 70% on a line too
/// tall for the bar), TeX's script size (70%) for one inside a slot and for a power, never under scriptscript
/// (50% of the line).
pub fn slot_size(parent: f32, fraction: bool, line: f32, top: f32) -> f32 {
    let scale = if fraction && parent >= line - 0.01 {
        top
    } else {
        tex::SCRIPT
    };
    (line * tex::SCRIPT_SCRIPT).max(parent * scale)
}

/// A numerator's baseline over the line's (rule 15): TeX's shift, or the gap over the bar's top and the
/// numerator's depth (its tails, or a stack in it), whichever is more.
pub fn num_offset(axis: f32, s: f32, depth: f32, top: f32) -> f32 {
    axis + ((tex::NUM_UP - tex::AXIS) * s).max((tex::RULE / 2.0 + tex::gap(top)) * s + depth)
}

/// A denominator's baseline under it: TeX's shift, or the gap under the bar and the denominator's height.
pub fn den_offset(axis: f32, s: f32, height: f32, top: f32) -> f32 {
    axis - ((tex::DEN_DOWN + tex::AXIS) * s).max((tex::RULE / 2.0 + tex::gap(top)) * s + height)
}

/// How far a power's baseline is over its base's (rule 18): TeX's shift, its bottom (a stack in it) clear of the
/// base's baseline, and over a base that isn't a glyph (brackets grown around a stack) no lower than the base's
/// top less the drop.
pub fn sup_shift(p: &Parsed, s: &Stack, parent: f32, line: f32, top: f32) -> f32 {
    let f = slot_size(parent, false, line, top);
    let mut u = (tex::SUP_UP * parent)
        .max(extent(p, s.slot.lo, s.slot.hi, f, line, top).1 + tex::SUP_BOTTOM * parent);
    if let Some(b) = p.tall.iter().find(|b| b.close + 1 == s.op) {
        let h = extent(p, b.open, b.close + 1, parent, line, top).0;
        u = u.max(h - tex::SUP_DROP * parent);
    }
    u
}

/// How far what's in `lo..hi` reaches above its baseline and below it, at `size`, the stacks, roots and grown
/// brackets in it taken in: (height, depth).
pub fn extent(p: &Parsed, lo: usize, hi: usize, size: f32, line: f32, top: f32) -> (f32, f32) {
    let mut height = size * tex::HEIGHT;
    let mut depth = 0.0f32;
    let len = hi.saturating_sub(lo);
    let inside: Vec<&Stack> = p
        .stacks
        .iter()
        .filter(|s| s.start >= lo && s.end <= hi && s.end - s.start < len + 2)
        .collect();
    // only the stacks directly in lo..hi: one inside another's slot is taken in by that one
    let direct: Vec<&Stack> = inside
        .iter()
        .copied()
        .filter(|s| {
            !inside
                .iter()
                .any(|o| o != s && o.slots().any(|g| g.lo <= s.start && s.end <= g.hi))
        })
        .collect();
    let in_slot = |k: usize| {
        direct
            .iter()
            .any(|s| s.slots().any(|g| k >= g.lo && k < g.hi))
    };
    if len > 0 && hi <= p.text.len() {
        // a tail (g, y, a bracket) in the range's own text, not a slot's
        if (lo..hi).any(|k| tex::tail(p.text[k]) && !p.hidden.contains(&k) && !in_slot(k)) {
            depth = size * tex::DEPTH;
        }
        // a root's bar over a plain radicand reaches higher than the glyphs
        if (lo..hi).any(|k| (p.text[k] == ROOT || p.text[k] == CUBE_ROOT) && !in_slot(k)) {
            height = height.max(root_top(size));
        }
    }
    for s in &direct {
        if let (true, Some(num)) = (s.fraction, s.num) {
            let f = slot_size(size, true, line, top);
            let axis = size * tex::AXIS;
            let n = extent(p, num.lo, num.hi, f, line, top);
            let d = extent(p, s.slot.lo, s.slot.hi, f, line, top);
            height = height.max(num_offset(axis, size, n.1, top) + n.0);
            depth = depth.max(-den_offset(axis, size, d.0, top) + d.1);
        } else {
            let f = slot_size(size, false, line, top);
            height = height.max(
                sup_shift(p, s, size, line, top) + extent(p, s.slot.lo, s.slot.hi, f, line, top).0,
            );
        }
    }
    for b in p
        .tall
        .iter()
        .filter(|b| b.open >= lo && b.close < hi && !in_slot(b.open))
    {
        let e = extent(p, b.open + 1, b.close, size, line, top);
        let half = tex::bracket_half(e.0, e.1, size);
        let a = size * tex::AXIS;
        height = height.max(a + half);
        depth = depth.max(half - a);
    }
    (height, depth)
}

/// Whether a root in the line takes a stack (its bar goes above the numerator, its foot under the denominator).
pub fn rooted(p: &Parsed) -> bool {
    if p.stacks.is_empty() || !p.text.iter().any(|&c| c == ROOT || c == CUBE_ROOT) {
        return false;
    }
    crate::input::radicand_ranges(&String::from_utf16_lossy(&p.text))
        .iter()
        .any(|&(a, b)| {
            p.stacks
                .iter()
                .any(|s| s.start >= a as usize && s.end <= b as usize)
        })
}

/// Where the line's baseline may go in a view `room` tall for all of a line this tall and deep to show
/// (lowest > highest: it doesn't fit). `size`: the line's.
pub fn baseline_range(
    (height, depth): (f32, f32),
    rooted: bool,
    size: f32,
    room: f32,
) -> (f32, f32) {
    let root_room = tex::radical_gap(size) + root_line_width(size) + 1.0;
    (
        height + if rooted { root_room } else { 2.0 },
        room - depth - if rooted { 3.0 } else { 2.0 },
    )
}

/// Whether the line's stacks fit a view `room` tall at Desmos's 90% fractions; a line that doesn't is set in TeX's
/// text style (70%, the slots θ off the bar).
pub fn fits(p: &Parsed, line: f32, room: f32) -> bool {
    let e = extent(p, 0, p.text.len(), line, line, FRACTION_SCALE);
    let (lowest, highest) = baseline_range(e, rooted(p), line, room);
    lowest <= highest
}

/// A slot's size and how far its baseline is raised (DIPs, negative is lowered), from the line's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub lo: usize,
    pub hi: usize,
    pub size: f32,
    pub offset: f32,
}

/// The size and raise of every character (`n` of them, the line at `full`), and each slot's look, by TeX's rules
/// (`top`: a fraction's size in the line). Outer stacks first, so an inner one is smaller again and moves with its
/// slot.
pub fn looks(p: &Parsed, n: usize, full: f32, top: f32) -> (Vec<f32>, Vec<f32>, Vec<Look>) {
    let mut size = vec![full; n];
    let mut offset = vec![0.0f32; n];
    let mut out = Vec::new();
    for s in &p.stacks {
        let (parent, base) = (size[s.op], offset[s.op]);
        let f = slot_size(parent, s.fraction, full, top);
        let mut put = |slot: Slot, o: f32| {
            out.push(Look {
                lo: slot.lo,
                hi: slot.hi,
                size: f,
                offset: o,
            });
            for k in slot.lo..slot.hi {
                size[k] = f;
                offset[k] = o;
            }
        };
        if let (true, Some(num)) = (s.fraction, s.num) {
            let axis = base + parent * tex::AXIS;
            // TeX's rule 15: a slot stands its gap off the bar, as far as its tails or the stacks in it reach
            let depth = extent(p, num.lo, num.hi, f, full, top).1;
            put(num, num_offset(axis, parent, depth, top));
            let height = extent(p, s.slot.lo, s.slot.hi, f, full, top).0;
            put(s.slot, den_offset(axis, parent, height, top));
        } else {
            put(s.slot, base + sup_shift(p, s, parent, full, top));
        }
    }
    (size, offset, out)
}

/// The look of the slot the caret is in (None outside every slot: the line's own).
pub fn look_at(p: &Parsed, looks: &[Look], caret: usize) -> Option<Look> {
    let s = p.slot(caret)?;
    looks.iter().copied().find(|l| l.lo == s.lo && l.hi == s.hi)
}

#[cfg(test)]
mod tests;
