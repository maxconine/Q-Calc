//! Exact arithmetic shared by the systems solver and the matrix engine: BigInt rationals, sums of square roots,
//! rational functions in named parameters, the `Field` they share, and Gauss–Jordan over any of them.
//! Moved out of `system.rs` unchanged (its quirks, such as cancelling only one-parameter fractions, are the TS's).

#![allow(clippy::needless_range_loop)] // index loops mirror the TS

use std::cell::{Cell, RefCell};

use num_bigint::BigInt;
use num_traits::{One, Signed, ToPrimitive, Zero};

use crate::jsre::{self, Lazy, js};
use crate::symbolic::{self, Node, PrintOptions};
thread_local! {
    static THROWN: Cell<bool> = const { Cell::new(false) };
}

pub(crate) fn throw() {
    THROWN.with(|t| t.set(true));
}

/// Runs `f` as a TS call that may throw: None when it did.
pub(crate) fn guarded<T>(f: impl FnOnce() -> Option<T>) -> Option<T> {
    let before = THROWN.with(|t| t.replace(false));
    let out = f();
    let thrown = THROWN.with(|t| t.replace(before));
    if thrown {
        THROWN.with(|t| t.set(true));
        return None;
    }
    out
}
// ---- ordered maps ----

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OMap<K, V> {
    pub(crate) e: Vec<(K, V)>,
}

impl<K: PartialEq + Clone, V: Clone> OMap<K, V> {
    pub(crate) fn new() -> Self {
        OMap { e: Vec::new() }
    }
    pub(crate) fn get(&self, k: &K) -> Option<&V> {
        self.e.iter().find(|(x, _)| x == k).map(|(_, v)| v)
    }
    pub(crate) fn set(&mut self, k: K, v: V) {
        match self.e.iter_mut().find(|(x, _)| *x == k) {
            Some(slot) => slot.1 = v,
            None => self.e.push((k, v)),
        }
    }
    pub(crate) fn delete(&mut self, k: &K) {
        self.e.retain(|(x, _)| x != k);
    }
    pub(crate) fn has(&self, k: &K) -> bool {
        self.get(k).is_some()
    }
    pub(crate) fn len(&self) -> usize {
        self.e.len()
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.e.is_empty()
    }
    pub(crate) fn keys(&self) -> impl Iterator<Item = &K> {
        self.e.iter().map(|(k, _)| k)
    }
    pub(crate) fn values(&self) -> impl Iterator<Item = &V> {
        self.e.iter().map(|(_, v)| v)
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = &(K, V)> {
        self.e.iter()
    }
}
// ---- rationals ----

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Rat {
    pub(crate) n: BigInt,
    pub(crate) d: BigInt,
}

pub(crate) fn bi(n: i64) -> BigInt {
    BigInt::from(n)
}

pub(crate) fn bgcd(a: &BigInt, b: &BigInt) -> BigInt {
    let mut a = a.abs();
    let mut b = b.abs();
    while !b.is_zero() {
        let r = &a % &b;
        a = b;
        b = r;
    }
    if a.is_zero() { BigInt::one() } else { a }
}

pub(crate) fn rat(n: BigInt, d: BigInt) -> Rat {
    let (mut n, mut d) = (n, d);
    if d.is_negative() {
        n = -n;
        d = -d;
    }
    if d.is_zero() {
        throw();
        return Rat {
            n: BigInt::zero(),
            d: BigInt::one(),
        };
    }
    let g = bgcd(&n, &d);
    Rat {
        n: n / &g,
        d: d / g,
    }
}

pub(crate) fn ri(n: i64) -> Rat {
    rat(bi(n), BigInt::one())
}

pub(crate) fn r0() -> Rat {
    ri(0)
}
pub(crate) fn r1() -> Rat {
    ri(1)
}

pub(crate) fn radd(a: &Rat, b: &Rat) -> Rat {
    rat(&a.n * &b.d + &b.n * &a.d, &a.d * &b.d)
}
pub(crate) fn rsub(a: &Rat, b: &Rat) -> Rat {
    rat(&a.n * &b.d - &b.n * &a.d, &a.d * &b.d)
}
pub(crate) fn rmul(a: &Rat, b: &Rat) -> Rat {
    rat(&a.n * &b.n, &a.d * &b.d)
}
pub(crate) fn rdiv(a: &Rat, b: &Rat) -> Rat {
    rat(&a.n * &b.d, &a.d * &b.n)
}
pub(crate) fn rneg(a: &Rat) -> Rat {
    rat(-&a.n, a.d.clone())
}
pub(crate) fn rzero(a: &Rat) -> bool {
    a.n.is_zero()
}
pub(crate) fn rpow(a: &Rat, k: i64) -> Rat {
    if k < 0 {
        return rpow(&rat(a.d.clone(), a.n.clone()), -k);
    }
    let mut n = BigInt::one();
    let mut d = BigInt::one();
    for _ in 0..k {
        n *= &a.n;
        d *= &a.d;
    }
    rat(n, d)
}
pub(crate) fn rstr(a: &Rat) -> String {
    if a.d.is_one() {
        a.n.to_string()
    } else {
        format!("{}/{}", a.n, a.d)
    }
}
// ---- node helpers ----

pub(crate) fn unwrap(n: &Node) -> &Node {
    symbolic::content(n)
}

pub(crate) fn fn_name(n: &Node) -> Option<&str> {
    n.call_name()
}

pub(crate) fn args_of(n: &Node) -> Vec<&Node> {
    symbolic::args(n).iter().map(unwrap).collect()
}

/// An operator's symbol (`OperatorNode.op`) and its arguments.
pub(crate) fn op_of(n: &Node) -> Option<(&'static str, Vec<&Node>)> {
    match n {
        Node::Operator { op, .. } => Some((op.symbol(), args_of(n))),
        _ => None,
    }
}

pub(crate) fn symbol_name(n: &Node) -> Option<&str> {
    match n {
        Node::Symbol(s) => Some(s),
        _ => None,
    }
}

pub(crate) fn int_of(n: &Node) -> Option<BigInt> {
    let v = symbolic::number_value(n)?;
    crate::jsnum::is_safe_integer(v).then(|| BigInt::from(v as i64))
}

pub(crate) fn const_rat(node: &Node) -> Option<Rat> {
    let node = unwrap(node);
    if let Some(n) = int_of(node) {
        return Some(rat(n, BigInt::one()));
    }
    let (op, args) = op_of(node)?;
    if op == "-" && args.len() == 1 {
        return const_rat(args[0]).map(|a| rneg(&a));
    }
    if op == "+" && args.len() == 1 {
        return const_rat(args[0]);
    }
    if args.len() != 2 {
        return None;
    }
    let a = const_rat(args[0])?;
    let b = const_rat(args[1])?;
    match op {
        "+" => Some(radd(&a, &b)),
        "-" => Some(rsub(&a, &b)),
        "*" => Some(rmul(&a, &b)),
        "/" => (!rzero(&b)).then(|| rdiv(&a, &b)),
        "^" => {
            if !b.d.is_one() || b.n.is_negative() || b.n > bi(12) {
                return None;
            }
            Some(rpow(&a, b.n.to_i64().unwrap_or(0)))
        }
        _ => None,
    }
}
pub(crate) fn factorize(n: &BigInt) -> Vec<(BigInt, u32)> {
    let mut n = n.abs();
    let mut out = Vec::new();
    let mut p = bi(2);
    while &p * &p <= n {
        let mut e = 0;
        while (&n % &p).is_zero() {
            n /= &p;
            e += 1;
        }
        if e > 0 {
            out.push((p.clone(), e));
        }
        p = if p == bi(2) { bi(3) } else { p + 2 };
    }
    if n > BigInt::one() {
        out.push((n, 1));
    }
    out
}

// ---- fields ----

pub(crate) trait Field {
    type C: Clone;
    fn zero(&self) -> Self::C;
    fn one(&self) -> Self::C;
    fn add(&self, a: &Self::C, b: &Self::C) -> Self::C;
    fn sub(&self, a: &Self::C, b: &Self::C) -> Self::C;
    fn mul(&self, a: &Self::C, b: &Self::C) -> Self::C;
    fn div(&self, a: &Self::C, b: &Self::C) -> Self::C;
    fn neg(&self, a: &Self::C) -> Self::C;
    fn is_zero(&self, a: &Self::C) -> bool;
    fn str(&self, a: &Self::C) -> String;
}

pub(crate) struct RatField;

impl Field for RatField {
    type C = Rat;
    fn zero(&self) -> Rat {
        r0()
    }
    fn one(&self) -> Rat {
        r1()
    }
    fn add(&self, a: &Rat, b: &Rat) -> Rat {
        radd(a, b)
    }
    fn sub(&self, a: &Rat, b: &Rat) -> Rat {
        rsub(a, b)
    }
    fn mul(&self, a: &Rat, b: &Rat) -> Rat {
        rmul(a, b)
    }
    fn div(&self, a: &Rat, b: &Rat) -> Rat {
        rdiv(a, b)
    }
    fn neg(&self, a: &Rat) -> Rat {
        rneg(a)
    }
    fn is_zero(&self, a: &Rat) -> bool {
        rzero(a)
    }
    fn str(&self, a: &Rat) -> String {
        rstr(a)
    }
}

pub(crate) const RAT: RatField = RatField;
// ---- radicals ----

pub(crate) struct Rad {
    pub(crate) c: Rat,
    pub(crate) rad: BigInt,
}

pub(crate) fn rad_norm(n: &BigInt, d: &BigInt) -> Rad {
    let m = n * d;
    let mut coeff = BigInt::one();
    let mut rad = BigInt::one();
    for (p, e) in factorize(&m) {
        for _ in 0..e / 2 {
            coeff *= &p;
        }
        if e % 2 == 1 {
            rad *= &p;
        }
    }
    Rad {
        c: rat(coeff, d.clone()),
        rad,
    }
}

pub(crate) fn sqrt_rat(a: &Rat) -> Option<Rad> {
    if a.n.is_negative() {
        return None;
    }
    Some(rad_norm(&a.n, &a.d))
}

pub(crate) struct RadField {
    pub(crate) primes: RefCell<Vec<BigInt>>,
}

impl RadField {
    pub(crate) fn new() -> Self {
        RadField {
            primes: RefCell::new(Vec::new()),
        }
    }
    pub(crate) fn n(&self) -> usize {
        1 << self.primes.borrow().len()
    }
    pub(crate) fn idx(&self, rad: &BigInt) -> usize {
        if rad.is_one() {
            return 0;
        }
        let mut mask = 0;
        for (p, _) in factorize(rad) {
            let mut primes = self.primes.borrow_mut();
            let i = match primes.iter().position(|x| *x == p) {
                Some(i) => i,
                None => {
                    primes.push(p);
                    primes.len() - 1
                }
            };
            mask |= 1 << i;
        }
        mask
    }
    pub(crate) fn fit(&self, a: &[Rat]) -> Vec<Rat> {
        let n = self.n();
        let mut out: Vec<Rat> = a.to_vec();
        if out.len() < n {
            out.resize(n, r0());
        }
        out
    }
    pub(crate) fn sqrt_of(&self, a: &Rat) -> Option<Vec<Rat>> {
        let s = sqrt_rat(a)?;
        let i = self.idx(&s.rad);
        let mut out = vec![r0(); self.n()];
        out[i] = s.c;
        Some(out)
    }
    pub(crate) fn rat_of(&self, a: &Rat) -> Vec<Rat> {
        let mut out = vec![r0(); self.n()];
        out[0] = a.clone();
        out
    }
}

impl Field for RadField {
    type C = Vec<Rat>;
    fn zero(&self) -> Vec<Rat> {
        vec![r0(); self.n()]
    }
    fn one(&self) -> Vec<Rat> {
        let mut v = self.zero();
        v[0] = r1();
        v
    }
    fn add(&self, a: &Vec<Rat>, b: &Vec<Rat>) -> Vec<Rat> {
        let (a, b) = (self.fit(a), self.fit(b));
        a.iter().zip(&b).map(|(x, y)| radd(x, y)).collect()
    }
    fn sub(&self, a: &Vec<Rat>, b: &Vec<Rat>) -> Vec<Rat> {
        self.add(a, &self.neg(b))
    }
    fn neg(&self, a: &Vec<Rat>) -> Vec<Rat> {
        self.fit(a).iter().map(rneg).collect()
    }
    fn is_zero(&self, a: &Vec<Rat>) -> bool {
        self.fit(a).iter().all(rzero)
    }
    fn mul(&self, a: &Vec<Rat>, b: &Vec<Rat>) -> Vec<Rat> {
        let n = self.n();
        let (a, b) = (self.fit(a), self.fit(b));
        let primes = self.primes.borrow().clone();
        let mut out = vec![r0(); n];
        for i in 0..n {
            if rzero(&a[i]) {
                continue;
            }
            for j in 0..n {
                if rzero(&b[j]) {
                    continue;
                }
                let mut mask = 0;
                let mut coeff = rmul(&a[i], &b[j]);
                for (bit, p) in primes.iter().enumerate() {
                    let (ba, bb) = ((i >> bit) & 1, (j >> bit) & 1);
                    if ba == 1 && bb == 1 {
                        coeff = rmul(&coeff, &rat(p.clone(), BigInt::one()));
                    } else if ba != bb {
                        mask |= 1 << bit;
                    }
                }
                out[mask] = radd(&out[mask], &coeff);
            }
        }
        out
    }
    fn div(&self, a: &Vec<Rat>, b: &Vec<Rat>) -> Vec<Rat> {
        let n = self.n();
        let (a, b) = (self.fit(a), self.fit(b));
        let mut m: Vec<Vec<Rat>> = vec![vec![r0(); n + 1]; n];
        for j in 0..n {
            let mut e = vec![r0(); n];
            e[j] = r1();
            let col = self.mul(&b, &e);
            for i in 0..n {
                m[i][j] = col[i].clone();
            }
        }
        for i in 0..n {
            m[i][n] = a[i].clone();
        }
        match eliminate(&m, &RAT) {
            Some((pivots, rows)) if pivots.len() == n => {
                let mut sol = vec![r0(); n];
                for (k, col) in pivots.iter().enumerate() {
                    sol[*col] = rows[k][n].clone();
                }
                sol
            }
            _ => {
                throw();
                self.zero()
            }
        }
    }
    fn str(&self, a: &Vec<Rat>) -> String {
        rad_str(&self.fit(a), &self.primes.borrow())
    }
}

pub(crate) fn join_signed(parts: &[(bool, String)]) -> String {
    if parts.is_empty() {
        return "0".into();
    }
    let mut out = String::new();
    for (i, (neg, body)) in parts.iter().enumerate() {
        if i == 0 {
            if *neg {
                out.push('-');
            }
        } else {
            out.push_str(if *neg { " - " } else { " + " });
        }
        out.push_str(body);
    }
    out
}

pub(crate) fn rad_str(coeffs: &[Rat], primes: &[BigInt]) -> String {
    let mut parts = Vec::new();
    for (mask, c) in coeffs.iter().enumerate() {
        if rzero(c) {
            continue;
        }
        let mut rad = BigInt::one();
        for (bit, p) in primes.iter().enumerate() {
            if (mask >> bit) & 1 == 1 {
                rad *= p;
            }
        }
        let unit = if rad.is_one() {
            String::new()
        } else {
            format!("sqrt({rad})")
        };
        let neg = c.n.is_negative();
        let mag = rat(c.n.abs(), c.d.clone());
        let body = if unit.is_empty() {
            rstr(&mag)
        } else if mag == r1() {
            unit
        } else {
            format!("{}*{unit}", rstr(&mag))
        };
        parts.push((neg, body));
    }
    join_signed(&parts)
}

// ---- polynomials in the parameters ----

pub(crate) type Mono = OMap<String, BigInt>;

pub(crate) fn mono_key(exps: &[i64]) -> String {
    let mut e = exps.to_vec();
    while e.last() == Some(&0) {
        e.pop();
    }
    e.iter().map(i64::to_string).collect::<Vec<_>>().join(",")
}

pub(crate) fn key_exps(key: &str) -> Vec<i64> {
    if key.is_empty() {
        Vec::new()
    } else {
        key.split(',').map(|x| x.parse().unwrap_or(0)).collect()
    }
}

pub(crate) fn pconst(n: BigInt) -> Mono {
    let mut m = OMap::new();
    if !n.is_zero() {
        m.set(String::new(), n);
    }
    m
}

pub(crate) fn padd(a: &Mono, b: &Mono) -> Mono {
    let mut o = a.clone();
    for (k, c) in b.iter() {
        let s = o.get(k).cloned().unwrap_or_default() + c;
        if s.is_zero() {
            o.delete(k);
        } else {
            o.set(k.clone(), s);
        }
    }
    o
}

pub(crate) fn pscale(a: &Mono, c: &BigInt) -> Mono {
    let mut o = OMap::new();
    if c.is_zero() {
        return o;
    }
    for (k, v) in a.iter() {
        o.set(k.clone(), v * c);
    }
    o
}

pub(crate) fn pdivc(a: &Mono, c: &BigInt) -> Mono {
    if c.is_one() || c.is_zero() {
        return a.clone();
    }
    let mut o = OMap::new();
    for (k, v) in a.iter() {
        o.set(k.clone(), v / c);
    }
    o
}

pub(crate) fn pmul(a: &Mono, b: &Mono) -> Mono {
    let mut o: Mono = OMap::new();
    for (ka, ca) in a.iter() {
        for (kb, cb) in b.iter() {
            let ea = key_exps(ka);
            let eb = key_exps(kb);
            let n = ea.len().max(eb.len());
            let exps: Vec<i64> = (0..n)
                .map(|i| ea.get(i).copied().unwrap_or(0) + eb.get(i).copied().unwrap_or(0))
                .collect();
            let key = mono_key(&exps);
            let s = o.get(&key).cloned().unwrap_or_default() + ca * cb;
            if s.is_zero() {
                o.delete(&key);
            } else {
                o.set(key, s);
            }
        }
    }
    o
}

pub(crate) fn pneg(a: &Mono) -> Mono {
    pscale(a, &bi(-1))
}

pub(crate) fn pcontent(a: &Mono) -> BigInt {
    let mut g = BigInt::zero();
    for c in a.values() {
        g = bgcd(&g, c);
    }
    if g.is_zero() { BigInt::one() } else { g }
}

#[derive(Clone, Debug)]
pub(crate) struct Rf {
    pub(crate) n: Mono,
    pub(crate) d: Mono,
}

pub(crate) fn first_value(m: &Mono) -> BigInt {
    m.values().next().cloned().unwrap_or_default()
}

pub(crate) fn rf_norm(n: &Mono, d: &Mono) -> Rf {
    if n.is_empty() {
        return Rf {
            n: OMap::new(),
            d: pconst(BigInt::one()),
        };
    }
    if d.is_empty() {
        throw();
        return Rf {
            n: OMap::new(),
            d: pconst(BigInt::one()),
        };
    }
    let cn = pcontent(n);
    let cd = pcontent(d);
    let mut nn = pdivc(n, &cn);
    let mut dd = pdivc(d, &cd);
    if first_value(&dd).is_negative() {
        nn = pneg(&nn);
        dd = pneg(&dd);
    }
    let g = bgcd(&cn, &cd);
    nn = pscale(&nn, &(&cn / &g));
    dd = pscale(&dd, &(&cd / &g));
    if first_value(&dd).is_negative() {
        nn = pneg(&nn);
        dd = pneg(&dd);
    }
    if dd.len() == 1 && dd.get(&String::new()).is_some_and(BigInt::is_one) {
        return Rf {
            n: nn,
            d: pconst(BigInt::one()),
        };
    }
    Rf { n: nn, d: dd }
}

pub(crate) fn deg_of(p: &Mono) -> i64 {
    if p.is_empty() {
        return -1;
    }
    p.keys()
        .map(|k| {
            if k.is_empty() {
                0
            } else {
                k.parse().unwrap_or(0)
            }
        })
        .max()
        .unwrap_or(-1)
}

pub(crate) fn coef(p: &Mono, e: i64) -> BigInt {
    let key = if e == 0 { String::new() } else { e.to_string() };
    p.get(&key).cloned().unwrap_or_default()
}

pub(crate) fn ekey(e: i64) -> String {
    if e == 0 { String::new() } else { e.to_string() }
}

pub(crate) fn trim(p: &Mono) -> Mono {
    let mut o = p.clone();
    o.e.retain(|(_, c)| !c.is_zero());
    o
}

pub(crate) fn poly_gcd1(a: &Mono, b: &Mono) -> Mono {
    let mut aa = trim(&pdivc(a, &pcontent(a)));
    let mut bb = trim(&pdivc(b, &pcontent(b)));
    if deg_of(&aa).max(-1) < deg_of(&bb) {
        std::mem::swap(&mut aa, &mut bb);
    }
    // the TS's deg is -1 for empty too
    let mut guard = 0;
    while !bb.is_empty() && guard < 10_000 {
        guard += 1;
        let db = deg_of(&bb);
        let lb = coef(&bb, db);
        let mut r = aa.clone();
        while !r.is_empty() && deg_of(&r) >= db {
            let dr = deg_of(&r);
            let lr = coef(&r, dr);
            let shift = dr - db;
            r = pscale(&r, &lb);
            for e in 0..=db {
                let c = coef(&bb, e);
                if c.is_zero() {
                    continue;
                }
                let key = ekey(e + shift);
                let s = r.get(&key).cloned().unwrap_or_default() - &lr * &c;
                if s.is_zero() {
                    r.delete(&key);
                } else {
                    r.set(key, s);
                }
            }
            r = trim(&pdivc(&r, &pcontent(&r)));
        }
        aa = bb;
        bb = r;
    }
    pdivc(&aa, &pcontent(&aa))
}

pub(crate) fn pdiv1(a: &Mono, b: &Mono) -> Mono {
    let mut r = a.clone();
    let db = deg_of(b);
    let lb = coef(b, db);
    let mut q: Mono = OMap::new();
    while !r.is_empty() && deg_of(&r) >= db {
        let dr = deg_of(&r);
        let lr = coef(&r, dr);
        if !(&lr % &lb).is_zero() {
            return a.clone();
        }
        let qc = &lr / &lb;
        let shift = dr - db;
        q.set(ekey(shift), qc.clone());
        for e in 0..=db {
            let c = coef(b, e);
            if c.is_zero() {
                continue;
            }
            let key = ekey(e + shift);
            let s = r.get(&key).cloned().unwrap_or_default() - &qc * &c;
            if s.is_zero() {
                r.delete(&key);
            } else {
                r.set(key, s);
            }
        }
    }
    if r.is_empty() { q } else { a.clone() }
}

pub(crate) fn rf_cancel(r: &Rf, params: &[String]) -> Rf {
    let mut out = rf_norm(&r.n, &r.d);
    if params.len() == 1 && !out.n.is_empty() && !(out.d.len() == 1 && out.d.has(&String::new())) {
        let g = poly_gcd1(&out.n, &out.d);
        if deg_of(&g) > 0 {
            out = rf_norm(&pdiv1(&out.n, &g), &pdiv1(&out.d, &g));
        }
    }
    out
}

pub(crate) fn rf_add(a: &Rf, b: &Rf) -> Rf {
    rf_norm(
        &padd(&pmul(&a.n, &b.d), &pmul(&b.n, &a.d)),
        &pmul(&a.d, &b.d),
    )
}
pub(crate) fn rf_sub(a: &Rf, b: &Rf) -> Rf {
    rf_add(
        a,
        &Rf {
            n: pneg(&b.n),
            d: b.d.clone(),
        },
    )
}
pub(crate) fn rf_mul(a: &Rf, b: &Rf) -> Rf {
    rf_norm(&pmul(&a.n, &b.n), &pmul(&a.d, &b.d))
}
pub(crate) fn rf_div(a: &Rf, b: &Rf) -> Rf {
    rf_norm(&pmul(&a.n, &b.d), &pmul(&a.d, &b.n))
}
pub(crate) fn rf_neg(a: &Rf) -> Rf {
    Rf {
        n: pneg(&a.n),
        d: a.d.clone(),
    }
}

pub(crate) fn poly_str(p: &Mono, params: &[String]) -> String {
    if p.is_empty() {
        return "0".into();
    }
    let mut keys: Vec<&String> = p.keys().collect();
    keys.sort_by(|a, b| {
        let da: i64 = key_exps(a).iter().sum();
        let db: i64 = key_exps(b).iter().sum();
        da.cmp(&db).then_with(|| a.as_bytes().cmp(b.as_bytes()))
    });
    let mut parts = Vec::new();
    for key in keys {
        let c = p.get(key).cloned().unwrap_or_default();
        let factors: Vec<String> = key_exps(key)
            .iter()
            .enumerate()
            .filter(|(_, e)| **e != 0)
            .map(|(i, e)| {
                let name = params.get(i).cloned().unwrap_or_default();
                if *e == 1 { name } else { format!("{name}^{e}") }
            })
            .collect();
        let mon = factors.join("*");
        let neg = c.is_negative();
        let mag = c.abs();
        let body = if mon.is_empty() {
            mag.to_string()
        } else if mag.is_one() {
            mon
        } else {
            format!("{mag}*{mon}")
        };
        parts.push((neg, body));
    }
    join_signed(&parts)
}

pub(crate) fn has_sign_after_first(s: &str) -> bool {
    s.chars().skip(1).any(|c| c == '+' || c == '-')
}

pub(crate) fn rf_str(a: &Rf, params: &[String]) -> String {
    let n = poly_str(&a.n, params);
    let d = poly_str(&a.d, params);
    if d == "1" {
        return n;
    }
    let num = if has_sign_after_first(&n) {
        format!("({n})")
    } else {
        n
    };
    let den = if has_sign_after_first(&d) || d.contains('*') {
        format!("({d})")
    } else {
        d
    };
    let raw = format!("{num}/{den}");
    static WS: Lazy = Lazy::new(|| js(r"\s+", ""));
    match symbolic::simplify_text(&raw) {
        Ok(n) => jsre::replace_all(
            &WS,
            &symbolic::to_string(&n, &PrintOptions::default()),
            |_| " ".into(),
        )
        .into_owned(),
        Err(_) => raw,
    }
}

pub(crate) struct RfField {
    pub(crate) params: Vec<String>,
}

impl Field for RfField {
    type C = Rf;
    fn zero(&self) -> Rf {
        Rf {
            n: OMap::new(),
            d: pconst(BigInt::one()),
        }
    }
    fn one(&self) -> Rf {
        Rf {
            n: pconst(BigInt::one()),
            d: pconst(BigInt::one()),
        }
    }
    fn add(&self, a: &Rf, b: &Rf) -> Rf {
        rf_cancel(&rf_add(a, b), &self.params)
    }
    fn sub(&self, a: &Rf, b: &Rf) -> Rf {
        rf_cancel(&rf_sub(a, b), &self.params)
    }
    fn mul(&self, a: &Rf, b: &Rf) -> Rf {
        rf_cancel(&rf_mul(a, b), &self.params)
    }
    fn div(&self, a: &Rf, b: &Rf) -> Rf {
        rf_cancel(&rf_div(a, b), &self.params)
    }
    fn neg(&self, a: &Rf) -> Rf {
        rf_neg(a)
    }
    fn is_zero(&self, a: &Rf) -> bool {
        a.n.is_empty()
    }
    fn str(&self, a: &Rf) -> String {
        rf_str(&rf_cancel(a, &self.params), &self.params)
    }
}
/// Gauss–Jordan: the pivot columns and reduced rows, or None for inconsistent.
pub(crate) type Reduced<C> = (Vec<usize>, Vec<Vec<C>>);

pub(crate) fn eliminate<F: Field>(rows: &[Vec<F::C>], f: &F) -> Option<Reduced<F::C>> {
    let mut a: Vec<Vec<F::C>> = rows.to_vec();
    let m = a.len();
    if m == 0 {
        return Some((Vec::new(), a));
    }
    let n = a[0].len() - 1;
    let mut pivots = Vec::new();
    let mut row = 0;
    let mut col = 0;
    while col < n && row < m {
        let Some(sel) = (row..m).find(|&i| !f.is_zero(&a[i][col])) else {
            col += 1;
            continue;
        };
        a.swap(row, sel);
        let piv = a[row][col].clone();
        for j in col..=n {
            a[row][j] = f.div(&a[row][j], &piv);
        }
        for i in 0..m {
            if i == row || f.is_zero(&a[i][col]) {
                continue;
            }
            let fac = a[i][col].clone();
            for j in col..=n {
                let t = f.mul(&fac, &a[row][j]);
                a[i][j] = f.sub(&a[i][j], &t);
            }
        }
        pivots.push(col);
        row += 1;
        col += 1;
    }
    for r in a.iter().skip(row) {
        if !f.is_zero(&r[n]) {
            return None;
        }
    }
    Some((pivots, a))
}
pub(crate) fn const_rad(node: &Node, field: &RadField) -> Option<Vec<Rat>> {
    let node = unwrap(node);
    if fn_name(node) == Some("sqrt") {
        let arg = *args_of(node).first()?;
        let r = const_rat(arg)?;
        return field.sqrt_of(&r);
    }
    if let Some(p) = const_rat(node) {
        return Some(field.rat_of(&p));
    }
    let (op, args) = op_of(node)?;
    if (op == "-" || op == "+") && args.len() == 1 {
        let a = const_rad(args[0], field)?;
        return Some(if op == "-" { field.neg(&a) } else { a });
    }
    if args.len() != 2 {
        return None;
    }
    let a = const_rad(args[0], field)?;
    let b = const_rad(args[1], field)?;
    match op {
        "+" => Some(field.add(&a, &b)),
        "-" => Some(field.sub(&a, &b)),
        "*" => Some(field.mul(&a, &b)),
        "/" => (!field.is_zero(&b)).then(|| field.div(&a, &b)),
        _ => None,
    }
}

pub(crate) fn const_rf(node: &Node, params: &[String]) -> Option<Rf> {
    let node = unwrap(node);
    if let Some(r) = const_rat(node) {
        return Some(Rf {
            n: pconst(r.n),
            d: pconst(r.d),
        });
    }
    if let Some(name) = symbol_name(node) {
        let i = params.iter().position(|p| p == name)?;
        let mut exps = vec![0; params.len()];
        exps[i] = 1;
        let mut n = OMap::new();
        n.set(mono_key(&exps), BigInt::one());
        return Some(Rf {
            n,
            d: pconst(BigInt::one()),
        });
    }
    let (op, args) = op_of(node)?;
    if (op == "-" || op == "+") && args.len() == 1 {
        let a = const_rf(args[0], params)?;
        return Some(if op == "-" { rf_neg(&a) } else { a });
    }
    if args.len() != 2 {
        return None;
    }
    if op == "^" {
        let base = const_rf(args[0], params)?;
        let e = const_rat(args[1])?;
        if !e.d.is_one() || e.n.is_negative() || e.n > bi(8) {
            return None;
        }
        let mut out = Rf {
            n: pconst(BigInt::one()),
            d: pconst(BigInt::one()),
        };
        for _ in 0..e.n.to_i64().unwrap_or(0) {
            out = rf_mul(&out, &base);
        }
        return Some(rf_norm(&out.n, &out.d));
    }
    let a = const_rf(args[0], params)?;
    let b = const_rf(args[1], params)?;
    match op {
        "+" => Some(rf_add(&a, &b)),
        "-" => Some(rf_sub(&a, &b)),
        "*" => Some(rf_mul(&a, &b)),
        "/" => (!b.n.is_empty()).then(|| rf_div(&a, &b)),
        _ => None,
    }
}
