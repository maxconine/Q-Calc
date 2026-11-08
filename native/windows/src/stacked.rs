//! "Stacked fractions and powers" (Settings › General, off by default; native only, StackedMath.swift): the line is
//! typed as a graphing calculator's is. `/` puts what's before the caret over a bar and the caret under it; `^` puts
//! the caret up in a small raised power. The text stays plain maths the engine reads, `(5x)/(32+x^(3))`: each slot
//! is a bracketed group whose brackets, with the `/` and `^`, are hidden, and stacked_layout.rs draws the slots
//! stacked. This is the text side, pure: what's stacked, where the caret may stop, and the edits the keys make.
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
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
            .min_by_key(|s| s.hi - s.lo)
    }
}

// ---- characters

const OPEN: u16 = 0x28;
const CLOSE: u16 = 0x29;
const SLASH: u16 = 0x2F;
const CARET: u16 = 0x5E;
const DOT: u16 = 0xB7;
const BANG: u16 = 0x21;
const ROOT: u16 = 0x221A;
const CUBE_ROOT: u16 = 0x221B;

pub fn is_digit(c: u16) -> bool {
    (48..=57).contains(&c) || c == 46
}

pub fn is_letter(c: u16) -> bool {
    (65..=90).contains(&c) || (97..=122).contains(&c) || (0x391..=0x3C9).contains(&c) || c == 0x221E
}

fn is_word(c: u16) -> bool {
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
    starts_term(c) || c == CARET || c == BANG || c == 0x25 || c == 0x5B
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
/// a function's call, a bracketed group, a root, and the powers and fractions already made of them.
pub fn term_start(u: &[u16], c: usize) -> usize {
    let (_, open_of) = pairs(u);
    let fractions: Vec<usize> = if u.contains(&SLASH) {
        parse(u, &[])
            .stacks
            .iter()
            .filter(|s| s.fraction)
            .map(|s| s.op)
            .collect()
    } else {
        Vec::new()
    };
    let mut j = c;
    while j > 0 {
        let ch = u[j - 1];
        if ch == CLOSE
            && let Some(o) = open_of[j - 1]
        {
            j = o;
            continue;
        }
        if is_word(ch) || ch == ROOT || ch == CUBE_ROOT || ch == BANG {
            j -= 1;
            continue;
        }
        if j < c && ch == CARET && j >= 2 {
            j -= 1;
            continue;
        }
        // back over a stacked fraction's `/` (one with a bare numerator too, `1/(2)`)
        if j < c
            && ch == SLASH
            && j >= 2
            && (u[j - 2] == CLOSE && u[j] == OPEN || fractions.contains(&(j - 1)))
        {
            j -= 1;
            continue;
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
    let (close_of, _) = pairs(u);
    let group = u[j] == OPEN && close_of[j] == Some(c - 1);
    if group || (plain(u, j, c) && (j == 0 || boundary(u[j - 1]))) {
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

/// `^` typed: the caret goes up into an empty power. A fraction or a power before it is bracketed first
/// (½ then ^ is (½)³, not 1/2³). None with nothing before the caret to raise.
pub fn power_key(u: &[u16], c: usize) -> Option<Edit> {
    if c == 0 {
        return None;
    }
    let j = term_start(u, c);
    if j >= c {
        return None;
    }
    // the term ends with a stack (it isn't a bracketed group on its own): bracket it
    if parse(u, &[])
        .stacks
        .iter()
        .any(|s| s.end == c && s.start >= j)
    {
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

/// Backspace: after a stack it steps in (the last slot's end); at the start of a slot it takes the stack
/// apart, its slots' text left in the line. None leaves the key to the line.
pub fn backspace(u: &[u16], p: &Parsed, c: usize) -> Option<Edit> {
    if c == 0 {
        return None;
    }
    if p.closers.contains(&(c - 1)) {
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
        let Some(hit) = s.slots().find(|g| g.open.is_some() && g.lo == c) else {
            continue;
        };
        let end = if p.dots.contains(&s.end) {
            s.end + 1
        } else {
            s.end
        };
        if s.fraction
            && let Some(num) = s.num
        {
            let mut joined = u[num.lo..num.hi].to_vec();
            joined.extend_from_slice(&u[s.slot.lo..s.slot.hi]);
            let caret = s.start + if hit == num { 0 } else { num.hi - num.lo };
            return Some(Edit {
                text: splice(u, s.start, end, &joined),
                caret,
            });
        }
        return Some(Edit {
            text: splice(u, s.op, end, &u[s.slot.lo..s.slot.hi]),
            caret: s.op,
        });
    }
    None
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

/// ↑ from a denominator to its numerator, ↓ back: the place across nearest the caret's x (`x(i)`, the caret
/// place before character `i` as drawn).
pub fn vertical(p: &Parsed, caret: usize, up: bool, x: impl Fn(usize) -> f32) -> Option<usize> {
    let s = p
        .stacks
        .iter()
        .filter(|s| {
            s.fraction
                && if up {
                    s.slot.holds(caret)
                } else {
                    s.num.is_some_and(|n| n.holds(caret))
                }
        })
        .min_by_key(|s| s.end - s.start)?;
    let to = if up { s.num? } else { s.slot };
    let here = x(caret);
    (to.lo..=to.hi)
        .filter(|i| !p.dead.contains(i))
        .min_by(|&a, &b| (x(a) - here).abs().total_cmp(&(x(b) - here).abs()))
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

// ---- how a slot looks

/// A slot's size and how far its baseline is raised (DIPs, negative is lowered), from the line's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub lo: usize,
    pub hi: usize,
    pub size: f32,
    pub offset: f32,
}

/// The size and raise of every character (`n` of them, the line at `full`), and each slot's look: a fraction's
/// slots are 0.6 × its size, the numerator above the math axis (0.25 em) and the denominator under it; a power's is
/// 0.6 ×, raised 0.36 em. Outer stacks first, so an inner one is smaller again and moves with its slot.
pub fn looks(p: &Parsed, n: usize, full: f32) -> (Vec<f32>, Vec<f32>, Vec<Look>) {
    let mut size = vec![full; n];
    let mut offset = vec![0.0f32; n];
    let mut out = Vec::new();
    for s in &p.stacks {
        let (parent, base) = (size[s.op], offset[s.op]);
        let f = small(parent);
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
            let axis = base + parent * 0.25;
            put(num, axis + 1.5 + f * 0.08);
            put(s.slot, axis - 1.5 - f * 0.72);
        } else {
            put(s.slot, base + parent * 0.36);
        }
    }
    (size, offset, out)
}

/// A slot's size inside a stack at `parent`.
pub fn small(parent: f32) -> f32 {
    (parent * 0.6).max(8.0)
}

/// The look of the slot the caret is in (None outside every slot: the line's own).
pub fn look_at(p: &Parsed, looks: &[Look], caret: usize) -> Option<Look> {
    let s = p.slot(caret)?;
    looks.iter().copied().find(|l| l.lo == s.lo && l.hi == s.hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Vec<u16> {
        w(s)
    }

    fn s(v: &[u16]) -> String {
        String::from_utf16_lossy(v)
    }

    fn p(t: &str) -> Parsed {
        parse(&u(t), &[])
    }

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
            f.slot,
            Slot {
                lo: 6,
                hi: 14,
                open: Some(5),
                close: Some(14)
            }
        );
        let pow = x.stacks[1];
        assert!(!pow.fraction);
        assert_eq!(
            pow.slot,
            Slot {
                lo: 12,
                hi: 13,
                open: Some(11),
                close: Some(13)
            }
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
        // a plain numerator and a bare power
        let y = p("42/(12)+x^2");
        assert_eq!(y.stacks[0].num, Some(Slot::bare(0, 2)));
        assert_eq!(y.stacks[1].slot, Slot::bare(10, 11));
        assert!(!y.dead.contains(&2));
    }

    #[test]
    fn pasted_text_stacks_only_where_the_engine_reads_it_so() {
        assert_eq!(p("3/4").stacks.len(), 1);
        assert_eq!(p("1+sin(x)/2").stacks[0].num, Some(Slot::bare(2, 8)));
        // 2x/4 is (2x)/4 to no one: a mixed numerator stays flat, and so does what binds after the denominator
        assert!(p("2x/4").is_empty());
        assert!(p("(a)/(b)c").is_empty());
        assert!(p("(a)/(b)^2").stacks.iter().all(|s| !s.fraction));
        assert!(p("a)/b").is_empty());
        assert!(p("x/").is_empty() && p("/x").is_empty());
        // a ∫'s `^` is its own
        assert!(parse(&u("∫_0^1 x"), &[(0, 5)]).is_empty());
        assert_eq!(p("∫_0^1 x").stacks.len(), 1);
    }

    #[test]
    fn slash_puts_the_term_over_the_bar() {
        let e = |t: &str| {
            let v = u(t);
            let e = slash_key(&v, v.len(), v.len());
            (s(&e.text), e.caret)
        };
        assert_eq!(e("3"), ("3/()".into(), 3));
        assert_eq!(e("5x"), ("(5x)/()".into(), 6));
        assert_eq!(e("1+abc"), ("1+abc/()".into(), 7));
        assert_eq!(e("(1+2)"), ("(1+2)/()".into(), 7));
        assert_eq!(e("sin(x)"), ("(sin(x))/()".into(), 10));
        assert_eq!(e(""), ("()/()".into(), 1));
        assert_eq!(e("2+"), ("2+()/()".into(), 3));
        // a fraction's denominator, then / again: the whole fraction goes over the new bar
        assert_eq!(e("(1)/(2)"), ("((1)/(2))/()".into(), 11));
        assert_eq!(e("x^(2)"), ("(x^(2))/()".into(), 9));
        // a selection becomes the numerator
        let v = u("1+23");
        let x = slash_key(&v, 2, 4);
        assert_eq!((s(&x.text), x.caret), ("1+(23)/()".into(), 8));
    }

    #[test]
    fn caret_raises_a_power_and_brackets_a_stack_first() {
        let e = |t: &str| power_key(&u(t), t.encode_utf16().count()).map(|e| (s(&e.text), e.caret));
        assert_eq!(e("x"), Some(("x^()".into(), 3)));
        assert_eq!(e("(1)/(2)"), Some(("((1)/(2))^()".into(), 11)));
        assert_eq!(e("x^(2)"), Some(("(x^(2))^()".into(), 9)));
        assert_eq!(e("1/(2)"), Some(("(1/(2))^()".into(), 9)));
        assert_eq!(e(""), None);
        assert_eq!(e("2+"), None);
    }

    #[test]
    fn close_steps_out_and_a_term_after_a_stack_gets_a_dot() {
        let v = u("(1)/(2)");
        let x = parse(&v, &[]);
        assert_eq!(close_key(&x, 6), Some(7));
        assert_eq!(close_key(&x, 2), Some(4));
        assert_eq!(close_key(&x, 5), None);
        let e = term_after_stack(&v, &x, 7, u16::from(b'x')).unwrap();
        assert_eq!((s(&e.text), e.caret), ("(1)/(2)·x".into(), 9));
        assert!(term_after_stack(&v, &x, 7, u16::from(b'+')).is_none());
        // the `·` is hidden and its place after it dead
        let y = p("(1)/(2)·x");
        assert!(y.dots.contains(&7) && y.hidden.contains(&7) && y.dead.contains(&8));
    }

    #[test]
    fn backspace_steps_in_and_takes_a_stack_apart() {
        let b = |t: &str, c: usize| {
            let v = u(t);
            backspace(&v, &parse(&v, &[]), c).map(|e| (s(&e.text), e.caret))
        };
        // after a stack: into its last slot
        assert_eq!(b("(1)/(2)", 7), Some(("(1)/(2)".into(), 6)));
        // at the start of the denominator: apart, the caret between what was the top and the bottom
        assert_eq!(b("(1)/(2)", 5), Some(("12".into(), 1)));
        assert_eq!(b("(1)/(2)", 1), Some(("12".into(), 0)));
        assert_eq!(b("x^(23)", 3), Some(("x23".into(), 1)));
        // the one term after a stack takes its `·` with it
        assert_eq!(b("(1)/(2)·x", 9), Some(("(1)/(2)".into(), 7)));
        assert_eq!(b("(1)/(2)·xy", 10), None);
        // anywhere else the line's own backspace
        assert_eq!(b("(1)/(2)", 6), None);
        assert_eq!(b("12", 2), None);
    }

    #[test]
    fn dead_places_are_skipped_the_way_the_caret_goes() {
        let x = p("(1)/(2)");
        // before the `/` looks as after the numerator's `)`, and before the denominator's `(` as after it
        assert_eq!(x.dead.iter().copied().collect::<Vec<_>>(), vec![3, 4]);
        assert_eq!(snap(&x, 3, 2, 7), 5);
        assert_eq!(snap(&x, 4, 5, 7), 2);
        assert_eq!(snap(&x, 2, 1, 7), 2);
    }

    #[test]
    fn a_selection_never_takes_part_of_a_stack() {
        // 42/(12)*12/(23*12)·12
        let t = "42/(12)*12/(23*12)·12";
        let x = p(t);
        let n = t.encode_utf16().count();
        assert_eq!(expand(&x, 1, 2, n), (1, 2));
        assert_eq!(expand(&x, 1, 4, n), (0, 7));
        assert_eq!(expand(&x, 14, 16, n), (14, 16));
        // the second fraction taken whole takes its `·` too
        assert_eq!(expand(&x, 9, 12, n), (8, 19));
        assert_eq!(expand(&x, 0, n, n), (0, n));
        assert_eq!(expand(&x, 3, 3, n), (3, 3));
    }

    #[test]
    fn word_deletes_stop_at_a_slots_edge() {
        let t = "42/(12)*12/(23*12)·12";
        let v = u(t);
        let x = parse(&v, &[]);
        let n = v.len();
        let term = |i: usize| term_at(&v, i);
        // in 23*12, to the line's start: only what's before the caret in that denominator
        assert_eq!(deletion(&x, 17, 0, n, term), Some((12, 17)));
        // at the slot's edge: nothing (backspace takes it apart)
        assert_eq!(deletion(&x, 12, 0, n, term), None);
        // the last 12 and its `·`, with no term after it
        assert_eq!(deletion(&x, 21, 19, n, term), Some((18, 21)));
        // outside a slot: whole stacks
        assert_eq!(deletion(&x, 7, 0, n, term), Some((0, 7)));
    }

    #[test]
    fn emptying_a_bare_slot_leaves_a_box() {
        let x = p("42/(12)");
        assert_eq!(emptying(&x, 0, 2), Some(0));
        assert_eq!(emptying(&x, 0, 1), None);
        assert_eq!(emptying(&x, 4, 6), None);
        assert_eq!(emptying(&p("x^2"), 2, 3), Some(2));
    }

    #[test]
    fn up_and_down_go_to_the_place_nearest_across() {
        // (1)/(25): the numerator over the middle of the denominator
        let x = p("(1)/(25)");
        let xs = |i: usize| [0.0, 3.0, 7.0, 7.0, 0.0, 0.0, 5.0, 10.0, 10.0][i];
        assert_eq!(vertical(&x, 5, true, xs), Some(1));
        assert_eq!(vertical(&x, 7, true, xs), Some(2));
        assert_eq!(vertical(&x, 2, false, xs), Some(6));
        assert_eq!(vertical(&x, 2, true, xs), None);
        assert_eq!(vertical(&p("x^(2)"), 3, true, xs), None);
    }

    #[test]
    fn the_limit_the_caret_is_in_is_the_lower_first() {
        use qcalc_core::app::bounds::bounds_in;
        // Σ_0^5: the lower is 2..3, the upper 4..5
        let b = bounds_in("Σ_0^5");
        assert_eq!(limit_slot(&b, 2), Some((0, false)));
        assert_eq!(limit_slot(&b, 3), Some((0, false)));
        assert_eq!(limit_slot(&b, 4), Some((0, true)));
        assert_eq!(limit_slot(&b, 5), Some((0, true)));
        assert_eq!(limit_slot(&b, 0), None);
        // an empty lower holds the caret, and a second sign is found after the first
        let b = bounds_in("Σ_1^2 x+Σ_3");
        assert_eq!(limit_slot(&b, 10), Some((1, false)));
        let b = bounds_in("∫");
        assert_eq!(limit_slot(&b, 1), Some((0, false)));
    }

    #[test]
    fn slots_shrink_and_move_outer_first() {
        let x = p("(1)/((a)/(b))");
        let (size, offset, looks) = looks(&x, 13, 24.0);
        assert_eq!(looks.len(), 4);
        assert!((size[1] - 14.4).abs() < 1e-4 && offset[1] > 0.0);
        // the inner fraction's slots are smaller again, under the outer bar
        assert!((size[6] - 8.64).abs() < 1e-4);
        assert!(offset[6] < offset[1] && offset[10] < offset[6]);
        assert_eq!(look_at(&x, &looks, 7).map(|l| l.lo), Some(6));
        assert_eq!(look_at(&x, &looks, 0), None);
    }
}
