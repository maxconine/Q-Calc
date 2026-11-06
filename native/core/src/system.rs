//! WP-SOLVE: `src/engine/system.ts`, systems of up to five equations solved exactly over the rationals (or
//! rationals with square roots, or rational functions of parameters), with exponential, reciprocal, square-root,
//! absolute-value, trig and one-quadratic forms, the numeric fallback of `systemNumeric.ts`, and units by
//! working in SI.
//!
//! JS `Map`s keep insertion order and several results depend on it, so maps here are ordered vectors. Where the
//! TS throws (a zero denominator, a singular radical) a thread-local flag is set instead and the public entry
//! points answer None, as the throw leaves nothing behind.

#![allow(clippy::needless_range_loop)] // index loops mirror the TS

use num_bigint::BigInt;
use num_traits::{One, Signed, ToPrimitive};
use serde_json::{Value as Json, json};

use crate::exact::*;
use crate::format::{format_number, format_value};
use crate::jsre::{self, Lazy, js};
use crate::symbolic::{self, Node};
use crate::system_numeric::{
    Guess, guess_number, no_overrides, plain_number, solve_numeric_system,
};
use crate::types::{AngleMode, DefaultUnits, Value};
use crate::units::{SiQuantity, reads_as_unit, si_quantity, si_value};






#[derive(Clone, Debug)]
struct Aff<C> {
    k: C,
    v: OMap<String, C>,
}

fn aff<F: Field>(f: &F, k: Option<F::C>) -> Aff<F::C> {
    Aff {
        k: k.unwrap_or_else(|| f.zero()),
        v: OMap::new(),
    }
}

fn is_const<F: Field>(a: &Aff<F::C>, f: &F) -> bool {
    a.v.values().all(|c| f.is_zero(c))
}

fn add_a<F: Field>(a: &Aff<F::C>, b: &Aff<F::C>, f: &F) -> Aff<F::C> {
    let mut v = a.v.clone();
    for (name, c) in b.v.iter() {
        let cur = v.get(name).cloned().unwrap_or_else(|| f.zero());
        v.set(name.clone(), f.add(&cur, c));
    }
    Aff {
        k: f.add(&a.k, &b.k),
        v,
    }
}

fn scale_a<F: Field>(a: &Aff<F::C>, c: &F::C, f: &F) -> Aff<F::C> {
    let mut v = OMap::new();
    for (name, k) in a.v.iter() {
        v.set(name.clone(), f.mul(k, c));
    }
    Aff {
        k: f.mul(&a.k, c),
        v,
    }
}

fn sub_a<F: Field>(a: &Aff<F::C>, b: &Aff<F::C>, f: &F) -> Aff<F::C> {
    add_a(a, &scale_a(b, &f.neg(&f.one()), f), f)
}


fn wrap(s: &str) -> String {
    static PLAIN: Lazy = Lazy::new(|| js(r"^[-\d/]+$", ""));
    static SQRT: Lazy = Lazy::new(|| js(r"^sqrt\(\d+\)$", ""));
    if jsre::test(&PLAIN, s) || jsre::test(&SQRT, s) {
        s.to_string()
    } else {
        format!("({s})")
    }
}

fn expr_str<F: Field>(k: &F::C, terms: &[(String, F::C)], f: &F) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !f.is_zero(k) {
        parts.push(f.str(k));
    }
    for (name, c) in terms {
        if f.is_zero(c) {
            continue;
        }
        let s = f.str(c);
        if s == "1" {
            parts.push(name.clone());
        } else if s == "-1" {
            parts.push(format!("-{name}"));
        } else if let Some(rest) = s.strip_prefix('-') {
            parts.push(format!("-{}*{name}", wrap(rest)));
        } else {
            parts.push(format!("{}*{name}", wrap(&s)));
        }
    }
    if parts.is_empty() {
        return "0".into();
    }
    let mut out = parts[0].clone();
    for p in &parts[1..] {
        match p.strip_prefix('-') {
            Some(rest) => out.push_str(&format!(" - {rest}")),
            None => out.push_str(&format!(" + {p}")),
        }
    }
    out
}

// ---- results ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Unique,
    Finite,
    Infinite,
    Inconsistent,
    Mismatch,
}

impl Status {
    fn name(self) -> &'static str {
        match self {
            Status::Unique => "unique",
            Status::Finite => "finite",
            Status::Infinite => "infinite",
            Status::Inconsistent => "inconsistent",
            Status::Mismatch => "mismatch",
        }
    }
}

/// A solved system (`SystemResult`, or `UnitResult`'s mismatch).
#[derive(Clone, Debug, PartialEq)]
pub struct SystemResult {
    pub status: Status,
    pub variables: Vec<String>,
    pub solutions: Option<Vec<Vec<String>>>,
    pub general: Option<Vec<String>>,
    pub free: Option<Vec<String>>,
    pub pinned: Option<String>,
    pub atoms: Option<Vec<(String, String)>>,
    pub angles: Option<Vec<(String, Vec<String>)>>,
    pub display: String,
}

impl SystemResult {
    fn new(status: Status, variables: &[String], display: String) -> Self {
        SystemResult {
            status,
            variables: variables.to_vec(),
            solutions: None,
            general: None,
            free: None,
            pinned: None,
            atoms: None,
            angles: None,
            display,
        }
    }
    fn inconsistent(variables: &[String]) -> Self {
        Self::new(Status::Inconsistent, variables, "no solution".into())
    }
    fn unique(variables: &[String], sol: Vec<String>) -> Self {
        let display = pairs(variables, &sol);
        let mut r = Self::new(Status::Unique, variables, display);
        r.solutions = Some(vec![sol]);
        r
    }

    pub fn to_json(&self) -> Json {
        let mut o = serde_json::Map::new();
        o.insert("status".into(), json!(self.status.name()));
        if self.status != Status::Mismatch {
            o.insert("variables".into(), json!(self.variables));
        }
        if let Some(s) = &self.solutions {
            o.insert("solutions".into(), json!(s));
        }
        if let Some(s) = &self.general {
            o.insert("general".into(), json!(s));
        }
        if let Some(s) = &self.free {
            o.insert("free".into(), json!(s));
        }
        if let Some(s) = &self.pinned {
            o.insert("pinned".into(), json!(s));
        }
        if let Some(a) = &self.atoms {
            o.insert(
                "atoms".into(),
                Json::Array(
                    a.iter()
                        .map(|(e, v)| json!({ "expr": e, "value": v }))
                        .collect(),
                ),
            );
        }
        if let Some(a) = &self.angles {
            let mut m = serde_json::Map::new();
            for (k, v) in a {
                m.insert(k.clone(), json!(v));
            }
            o.insert("angles".into(), Json::Object(m));
        }
        o.insert("display".into(), json!(self.display));
        Json::Object(o)
    }
}

fn pairs(variables: &[String], sol: &[String]) -> String {
    variables
        .iter()
        .zip(sol)
        .map(|(n, v)| format!("{n} = {v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

// ---- parsing ----

#[derive(Clone, Debug)]
struct Parsed {
    lhs: Node,
    rhs: Node,
    diff: Node,
}

// systemNumeric's scope adds these to mathjs's functions
const OWN_FNS: [&str; 4] = ["ln", "arcsin", "arccos", "arctan"];

// `typeof math[name] === 'function'` for the TS's mathjs instance (`create(all, { number: 'number' })`), its
// prototype chain included (`toString`, `valueOf`…), names that can start a call only
const MATHJS_FUNCTIONS: &str = "\
AccessorNode ArgumentsError ArrayNode AssignmentNode BigNumber BlockNode Chain Complex \
ConditionalNode ConstantNode DenseMatrix DimensionError FibonacciHeap Fraction \
FunctionAssignmentNode FunctionNode Help ImmutableDenseMatrix Index IndexError IndexNode Matrix Node \
ObjectNode OperatorNode ParenthesisNode Parser Range RangeNode RelationalNode ResultSet Spa \
SparseMatrix SymbolNode Unit abs acos acosh acot acoth acsc acsch add addScalar and apply arg asec \
asech asin asinh atan atan2 atanh bellNumbers bernoulli bigint bignumber bin bitAnd bitNot bitOr \
bitXor boolean catalan cbrt ceil chain clone column combinations combinationsWithRep compare \
compareNatural compareText compile complex composition concat config conj constructor corr cos cosh \
cot coth count create createUnit cross csc csch ctranspose cube cumsum deepEqual den derivative det \
diag diff distance divide divideScalar dot dotDivide dotMultiply dotPow eigs emit equal equalScalar \
equalText erf evaluate exp expm expm1 factorial factory fft filter fix flatten floor forEach format \
fraction freqz gamma gcd getMatrixDataType hasNumericValue hasOwnProperty help hex hypot identity \
ifft im import index intersect inv invmod isAccessorNode isArray isArrayNode isAssignmentNode \
isBigInt isBigNumber isBlockNode isBoolean isBounded isChain isCollection isComplex \
isConditionalNode isConstantNode isDate isDenseMatrix isFinite isFraction isFunction \
isFunctionAssignmentNode isFunctionNode isHelp isIndex isIndexNode isInteger isMap isMatrix isNaN \
isNegative isNode isNull isNumber isNumeric isObject isObjectNode isObjectWrappingMap isOperatorNode \
isParenthesisNode isPartitionedMap isPositive isPrime isPrototypeOf isRange isRangeNode isRegExp \
isRelationalNode isResultSet isSparseMatrix isString isSymbolNode isUndefined isUnit isZero \
kldivergence kron larger largerEq lcm leafCount leftShift lgamma log log10 log1p log2 lsolve \
lsolveAll lup lusolve lyap mad map mapSlices matrix matrixFromColumns matrixFromFunction \
matrixFromRows max mean median min mod mode multinomial multiply multiplyScalar norm not nthRoot \
nthRoots nullish num number numeric oct off on once ones or parse parser partitionSelect \
permutations pickRandom pinv polynomialRoot pow print prod propertyIsEnumerable qr quantileSeq \
random randomInt range rationalize re replacer reshape resize resolve reviver rightArithShift \
rightLogShift rotate rotationMatrix round row schur sec sech setCartesian setDifference setDistinct \
setIntersect setIsSubset setMultiplicity setPowerset setSize setSymDifference setUnion sign simplify \
simplifyConstant simplifyCore sin sinh size slu smaller smallerEq solveODE sort sparse splitUnit \
sqrt sqrtm square squeeze std stirlingS2 string subset subtract subtractScalar sum sylvester \
symbolicEqual tan tanh to toBest toLocaleString toString trace transpose typeOf typed unaryMinus \
unaryPlus unequal unit usolve usolveAll valueOf variance xgcd xor zeros zeta zpk2tf \
";

fn is_mathjs_function(name: &str) -> bool {
    MATHJS_FUNCTIONS.split_whitespace().any(|n| n == name)
}

fn prep_eq(text: &str) -> Option<Parsed> {
    static ABS: Lazy = Lazy::new(|| js(r"(?<![\d)\]!])\|([^|]+)\|", ""));
    static DEC: Lazy = Lazy::new(|| js(r"(\d+)\.(\d+)", ""));
    static JUXT: Lazy = Lazy::new(|| js(r"(\d)(?![eE][+-]?\d)(?=[A-Za-z(])", ""));
    static CLOSE: Lazy = Lazy::new(|| js(r"\)(?=[A-Za-z(])", ""));
    let s = text.trim();
    if s.is_empty() {
        return None;
    }
    let s = s.replace('θ', "theta").replace('φ', "phi");
    let s = jsre::replace_all(&ABS, &s, |c| format!("abs({})", jsre::group(c, 1))).into_owned();
    let s = jsre::replace_all(&DEC, &s, |c| {
        let (w, f) = (jsre::group(c, 1), jsre::group(c, 2));
        format!("({w}{f}/1{})", "0".repeat(f.len()))
    })
    .into_owned();
    let s = jsre::replace_all(&JUXT, &s, |c| format!("{}*", jsre::group(c, 1))).into_owned();
    let s = jsre::replace_all(&CLOSE, &s, |_| ")*".into()).into_owned();
    // `B_y(5)` from a statics sheet is B_y times 5: only a function name (mathjs's, or the solver's own ln and
    // arcsin…) is a call
    static CALL: Lazy = Lazy::new(|| js(r"(?<![\w.])([A-Za-z][A-Za-z0-9_]*)\s*\(", ""));
    let s = jsre::replace_all(&CALL, &s, |c| {
        let name = jsre::group(c, 1);
        if OWN_FNS.contains(&name) || is_mathjs_function(name) {
            jsre::group(c, 0).to_string()
        } else {
            format!("{name}*(")
        }
    })
    .into_owned();
    let parts: Vec<&str> = s.split('=').collect();
    if parts.len() != 2 {
        return None;
    }
    let (lhs, rhs) = (parts[0].trim(), parts[1].trim());
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }
    Some(Parsed {
        lhs: crate::syntax::parse(lhs).ok()?,
        rhs: crate::syntax::parse(rhs).ok()?,
        diff: crate::syntax::parse(&format!("({lhs})-({rhs})")).ok()?,
    })
}

fn symbols_of(node: &Node, into: &mut Vec<String>) {
    const RESERVED: [&str; 5] = ["pi", "i", "tau", "infinity", "inf"];
    symbolic::traverse(node, &mut |n, callee| {
        if let Node::Symbol(name) = n
            && !callee
            && !RESERVED.contains(&name.as_str())
            && !into.contains(name)
        {
            into.push(name.clone());
        }
    });
}

fn preferred(name: &str) -> bool {
    matches!(name, "x" | "y" | "z" | "w" | "theta" | "phi")
        || (name.len() == 1 && name.as_bytes()[0].is_ascii_uppercase())
        || name.bytes().any(|b| b.is_ascii_digit())
}

fn classify(names: &[String]) -> (Vec<String>, Vec<String>) {
    let pref: Vec<String> = names.iter().filter(|n| preferred(n)).cloned().collect();
    if !pref.is_empty() && pref.len() < names.len() {
        let params = names
            .iter()
            .filter(|n| !pref.contains(n))
            .cloned()
            .collect();
        return (pref, params);
    }
    (names.to_vec(), Vec::new())
}

fn walk_aff<F: Field>(
    node: &Node,
    f: &F,
    unknowns: &[String],
    cc: &dyn Fn(&Node) -> Option<F::C>,
) -> Option<Aff<F::C>> {
    let node = unwrap(node);
    if let Some(c0) = cc(node) {
        return Some(aff(f, Some(c0)));
    }
    if let Some(name) = symbol_name(node) {
        if !unknowns.iter().any(|u| u == name) {
            return None;
        }
        let mut a = aff(f, None);
        a.v.set(name.to_string(), f.one());
        return Some(a);
    }
    let (op, args) = op_of(node)?;
    if op == "-" && args.len() == 1 {
        let a = walk_aff(args[0], f, unknowns, cc)?;
        return Some(scale_a(&a, &f.neg(&f.one()), f));
    }
    if op == "+" && args.len() == 1 {
        return walk_aff(args[0], f, unknowns, cc);
    }
    if args.len() != 2 {
        return None;
    }
    match op {
        "+" | "-" => {
            let a = walk_aff(args[0], f, unknowns, cc)?;
            let b = walk_aff(args[1], f, unknowns, cc)?;
            Some(if op == "+" {
                add_a(&a, &b, f)
            } else {
                sub_a(&a, &b, f)
            })
        }
        "*" => {
            let a = walk_aff(args[0], f, unknowns, cc)?;
            let b = walk_aff(args[1], f, unknowns, cc)?;
            if is_const(&a, f) {
                return Some(scale_a(&b, &a.k, f));
            }
            if is_const(&b, f) {
                return Some(scale_a(&a, &b.k, f));
            }
            None
        }
        "/" => {
            let a = walk_aff(args[0], f, unknowns, cc)?;
            let b = walk_aff(args[1], f, unknowns, cc)?;
            if !is_const(&b, f) || f.is_zero(&b.k) {
                return None;
            }
            Some(scale_a(&a, &f.div(&f.one(), &b.k), f))
        }
        _ => None,
    }
}


fn finish<F: Field>(rows: &[Vec<F::C>], f: &F, variables: &[String]) -> SystemResult {
    let Some((pivots, rows)) = eliminate(rows, f) else {
        return SystemResult::inconsistent(variables);
    };
    let n = variables.len();
    let free_idx: Vec<usize> = (0..n).filter(|i| !pivots.contains(i)).collect();
    let free: Vec<String> = free_idx.iter().map(|i| variables[*i].clone()).collect();
    if free.is_empty() {
        let mut sol = vec!["0".to_string(); n];
        for (k, col) in pivots.iter().enumerate() {
            sol[*col] = f.str(&rows[k][n]);
        }
        return SystemResult::unique(variables, sol);
    }
    let mut exprs = vec!["0".to_string(); n];
    for (k, col) in pivots.iter().enumerate() {
        let row = &rows[k];
        let terms: Vec<(String, F::C)> = free_idx
            .iter()
            .map(|j| (variables[*j].clone(), f.neg(&row[*j])))
            .collect();
        exprs[*col] = expr_str(&row[n], &terms, f);
    }
    for j in &free_idx {
        exprs[*j] = variables[*j].clone();
    }
    let fixed: Vec<usize> = pivots
        .iter()
        .enumerate()
        .filter(|(k, _)| free_idx.iter().all(|j| f.is_zero(&rows[*k][*j])))
        .map(|(_, c)| *c)
        .collect();
    let pinned = variables
        .iter()
        .enumerate()
        .filter(|(i, _)| fixed.contains(i))
        .map(|(i, name)| format!("{name} = {}", exprs[i]))
        .collect::<Vec<_>>()
        .join(", ");
    let display = variables
        .iter()
        .enumerate()
        .filter(|(_, name)| !free.contains(name))
        .map(|(i, name)| format!("{name} = {}", exprs[i]))
        .collect::<Vec<_>>()
        .join(", ");
    let mut r = SystemResult::new(
        Status::Infinite,
        variables,
        if display.is_empty() {
            format!("free: {}", free.join(", "))
        } else {
            display
        },
    );
    r.general = Some(exprs);
    r.free = Some(free);
    r.pinned = (!pinned.is_empty()).then_some(pinned);
    r
}

fn matrix_from<F: Field>(
    diffs: &[&Node],
    variables: &[String],
    f: &F,
    cc: &dyn Fn(&Node) -> Option<F::C>,
) -> Option<Vec<Vec<F::C>>> {
    let mut rows = Vec::new();
    for diff in diffs {
        let a = walk_aff(diff, f, variables, cc)?;
        let mut row: Vec<F::C> = variables
            .iter()
            .map(|n| a.v.get(n).cloned().unwrap_or_else(|| f.zero()))
            .collect();
        row.push(f.neg(&a.k));
        rows.push(row);
    }
    Some(rows)
}

fn has_sqrt_const(node: &Node) -> bool {
    let mut yes = false;
    symbolic::traverse(node, &mut |n, _| {
        if fn_name(n) == Some("sqrt")
            && let Some(arg) = args_of(n).first()
            && const_rat(arg).is_some()
        {
            yes = true;
        }
    });
    yes
}

fn solve_linear(diffs: &[&Node], variables: &[String], params: &[String]) -> Option<SystemResult> {
    if !params.is_empty() {
        let f = RfField {
            params: params.to_vec(),
        };
        let rows = matrix_from(diffs, variables, &f, &|n| const_rf(n, params))?;
        return Some(finish(&rows, &f, variables));
    }
    if diffs.iter().any(|d| has_sqrt_const(d)) {
        let field = RadField::new();
        let rows = matrix_from(diffs, variables, &field, &|n| const_rad(n, &field))?;
        return Some(finish(&rows, &field, variables));
    }
    let rows = matrix_from(diffs, variables, &RAT, &const_rat)?;
    Some(finish(&rows, &RAT, variables))
}


fn parse_rat_str(s: &str) -> Option<Rat> {
    static RE: Lazy = Lazy::new(|| js(r"^(-?\d+)(?:\/(-?\d+))?$", ""));
    let c = RE.captures(s).ok().flatten()?;
    let n: BigInt = jsre::group(&c, 1).parse().ok()?;
    let d: BigInt = match c.get(2) {
        Some(m) => m.as_str().parse().ok()?,
        None => BigInt::one(),
    };
    Some(rat(n, d))
}

type AffRat = Aff<Rat>;

fn exp_side(node: &Node, unknowns: &[String]) -> Option<OMap<BigInt, AffRat>> {
    let mut acc: OMap<BigInt, AffRat> = OMap::new();
    fn add_exp(acc: &mut OMap<BigInt, AffRat>, prime: BigInt, e: &AffRat, sign: i32) {
        let cur = acc.get(&prime).cloned().unwrap_or_else(|| aff(&RAT, None));
        let next = if sign > 0 {
            add_a(&cur, e, &RAT)
        } else {
            sub_a(&cur, e, &RAT)
        };
        acc.set(prime, next);
    }
    fn take_int(acc: &mut OMap<BigInt, AffRat>, n: &BigInt, exp: &AffRat, sign: i32) -> bool {
        if n.is_negative() {
            return false;
        }
        if n.is_one() {
            return true;
        }
        for (p, e) in factorize(n) {
            add_exp(
                acc,
                p,
                &scale_a(exp, &rat(BigInt::from(e), BigInt::one()), &RAT),
                sign,
            );
        }
        true
    }
    fn walk(n: &Node, sign: i32, acc: &mut OMap<BigInt, AffRat>, unknowns: &[String]) -> bool {
        let n = unwrap(n);
        if let Some(c) = const_rat(n) {
            if c.n.is_negative() {
                return false;
            }
            let one = aff(&RAT, Some(r1()));
            return take_int(acc, &c.n, &one, sign) && take_int(acc, &c.d, &one, -sign);
        }
        let Some((op, args)) = op_of(n) else {
            return false;
        };
        if args.len() != 2 {
            return false;
        }
        match op {
            "*" => walk(args[0], sign, acc, unknowns) && walk(args[1], sign, acc, unknowns),
            "/" => walk(args[0], sign, acc, unknowns) && walk(args[1], -sign, acc, unknowns),
            "^" => {
                let base = const_rat(args[0]);
                let exp = walk_aff(args[1], &RAT, unknowns, &const_rat);
                let (Some(base), Some(exp)) = (base, exp) else {
                    return false;
                };
                if !base.n.is_positive() {
                    return false;
                }
                take_int(acc, &base.n, &exp, sign) && take_int(acc, &base.d, &exp, -sign)
            }
            _ => false,
        }
    }
    walk(node, 1, &mut acc, unknowns).then_some(acc)
}

fn try_exponential(parsed: &[Parsed], variables: &[String]) -> Option<SystemResult> {
    let mut sides = Vec::new();
    for p in parsed {
        let l = exp_side(&p.lhs, variables)?;
        let r = exp_side(&p.rhs, variables)?;
        sides.push((l, r));
    }
    let mut primes: Vec<BigInt> = Vec::new();
    for (l, r) in &sides {
        for p in l.keys().chain(r.keys()) {
            if !primes.contains(p) {
                primes.push(p.clone());
            }
        }
    }
    if primes.is_empty() {
        return None;
    }
    let mut rows = Vec::new();
    for (l, r) in &sides {
        for p in &primes {
            let lp = l.get(p).cloned().unwrap_or_else(|| aff(&RAT, None));
            let rp = r.get(p).cloned().unwrap_or_else(|| aff(&RAT, None));
            let d = sub_a(&lp, &rp, &RAT);
            let mut row: Vec<Rat> = variables
                .iter()
                .map(|n| d.v.get(n).cloned().unwrap_or_else(r0))
                .collect();
            row.push(rneg(&d.k));
            if row.iter().any(|c| !rzero(c)) {
                rows.push(row);
            }
        }
    }
    if rows.is_empty() {
        return None;
    }
    Some(finish(&rows, &RAT, variables))
}

fn walk_inv(node: &Node, unknowns: &[String]) -> Option<AffRat> {
    let node = unwrap(node);
    if let Some(c) = const_rat(node) {
        return Some(aff(&RAT, Some(c)));
    }
    let (op, args) = op_of(node)?;
    if (op == "-" || op == "+") && args.len() == 1 {
        let a = walk_inv(args[0], unknowns)?;
        return Some(if op == "-" {
            scale_a(&a, &ri(-1), &RAT)
        } else {
            a
        });
    }
    if args.len() != 2 {
        return None;
    }
    match op {
        "+" | "-" => {
            let a = walk_inv(args[0], unknowns)?;
            let b = walk_inv(args[1], unknowns)?;
            Some(if op == "+" {
                add_a(&a, &b, &RAT)
            } else {
                sub_a(&a, &b, &RAT)
            })
        }
        "*" => {
            let a = walk_inv(args[0], unknowns)?;
            let b = walk_inv(args[1], unknowns)?;
            if is_const(&a, &RAT) {
                return Some(scale_a(&b, &a.k, &RAT));
            }
            if is_const(&b, &RAT) {
                return Some(scale_a(&a, &b.k, &RAT));
            }
            None
        }
        "/" => {
            let b = unwrap(args[1]);
            if let Some(name) = symbol_name(b)
                && unknowns.iter().any(|u| u == name)
            {
                let coeff = const_rat(args[0])?;
                let mut a = aff(&RAT, None);
                a.v.set(name.to_string(), coeff);
                return Some(a);
            }
            let a = walk_inv(args[0], unknowns)?;
            let den = walk_inv(args[1], unknowns)?;
            if !is_const(&den, &RAT) || rzero(&den.k) {
                return None;
            }
            Some(scale_a(&a, &rdiv(&r1(), &den.k), &RAT))
        }
        _ => None,
    }
}

fn try_reciprocal(diffs: &[&Node], variables: &[String]) -> Option<SystemResult> {
    let mut rows = Vec::new();
    for diff in diffs {
        let a = walk_inv(diff, variables)?;
        let mut row: Vec<Rat> = variables
            .iter()
            .map(|n| a.v.get(n).cloned().unwrap_or_else(r0))
            .collect();
        row.push(rneg(&a.k));
        rows.push(row);
    }
    let solved = finish(&rows, &RAT, variables);
    if solved.status != Status::Unique || solved.solutions.is_none() {
        return (solved.status == Status::Inconsistent).then_some(solved);
    }
    let vals = solved.solutions.as_ref()?[0].clone();
    if vals.iter().any(|v| v == "0") {
        return None;
    }
    let inv: Vec<String> = vals
        .iter()
        .map(|v| parse_rat_str(v).map(|r| rstr(&rdiv(&r1(), &r))))
        .collect::<Option<_>>()?;
    Some(SystemResult::unique(variables, inv))
}

type Quad = OMap<String, Rat>;

fn qkey(vars: &[String]) -> String {
    let mut v = vars.to_vec();
    v.sort();
    v.join(",")
}

fn qadd(a: &Quad, b: &Quad) -> Quad {
    let mut o = a.clone();
    for (k, c) in b.iter() {
        let s = radd(&o.get(k).cloned().unwrap_or_else(r0), c);
        if rzero(&s) {
            o.delete(k);
        } else {
            o.set(k.clone(), s);
        }
    }
    o
}

fn qscale(a: &Quad, c: &Rat) -> Quad {
    let mut o = OMap::new();
    for (k, v) in a.iter() {
        let s = rmul(v, c);
        if !rzero(&s) {
            o.set(k.clone(), s);
        }
    }
    o
}

fn qmul(a: &Quad, b: &Quad) -> Option<Quad> {
    let mut o: Quad = OMap::new();
    for (ka, ca) in a.iter() {
        for (kb, cb) in b.iter() {
            let mut vars: Vec<String> = Vec::new();
            if !ka.is_empty() {
                vars.extend(ka.split(',').map(str::to_string));
            }
            if !kb.is_empty() {
                vars.extend(kb.split(',').map(str::to_string));
            }
            if vars.len() > 2 {
                return None;
            }
            let key = qkey(&vars);
            let s = radd(&o.get(&key).cloned().unwrap_or_else(r0), &rmul(ca, cb));
            if rzero(&s) {
                o.delete(&key);
            } else {
                o.set(key, s);
            }
        }
    }
    Some(o)
}

fn qdegree(q: &Quad) -> usize {
    q.keys()
        .map(|k| {
            if k.is_empty() {
                0
            } else {
                k.split(',').count()
            }
        })
        .max()
        .unwrap_or(0)
}

fn walk_quad(node: &Node, unknowns: &[String]) -> Option<Quad> {
    let node = unwrap(node);
    if let Some(c) = const_rat(node) {
        let mut q = OMap::new();
        if !rzero(&c) {
            q.set(String::new(), c);
        }
        return Some(q);
    }
    if let Some(name) = symbol_name(node) {
        if !unknowns.iter().any(|u| u == name) {
            return None;
        }
        let mut q = OMap::new();
        q.set(name.to_string(), r1());
        return Some(q);
    }
    let (op, args) = op_of(node)?;
    if (op == "-" || op == "+") && args.len() == 1 {
        let a = walk_quad(args[0], unknowns)?;
        return Some(if op == "-" { qscale(&a, &ri(-1)) } else { a });
    }
    if args.len() != 2 {
        return None;
    }
    match op {
        "+" | "-" => {
            let a = walk_quad(args[0], unknowns)?;
            let b = walk_quad(args[1], unknowns)?;
            Some(if op == "+" {
                qadd(&a, &b)
            } else {
                qadd(&a, &qscale(&b, &ri(-1)))
            })
        }
        "*" => {
            let a = walk_quad(args[0], unknowns)?;
            let b = walk_quad(args[1], unknowns)?;
            qmul(&a, &b)
        }
        "/" => {
            let a = walk_quad(args[0], unknowns)?;
            let den = const_rat(args[1])?;
            if rzero(&den) {
                return None;
            }
            Some(qscale(&a, &rdiv(&r1(), &den)))
        }
        "^" => {
            let e = const_rat(args[1])?;
            if e.n != bi(2) || !e.d.is_one() {
                return None;
            }
            let base = walk_quad(args[0], unknowns)?;
            qmul(&base, &base)
        }
        _ => None,
    }
}

fn aff_mul(a: &AffRat, b: &AffRat) -> Option<Quad> {
    let mut qa: Quad = OMap::new();
    qa.set(String::new(), a.k.clone());
    for (n, c) in a.v.iter() {
        qa.set(n.clone(), c.clone());
    }
    let mut qb: Quad = OMap::new();
    qb.set(String::new(), b.k.clone());
    for (n, c) in b.v.iter() {
        qb.set(n.clone(), c.clone());
    }
    qmul(&qa, &qb)
}

fn eval_aff(a: &AffRat, free: &str, t: &Rat) -> Rat {
    radd(
        &a.k,
        &rmul(&a.v.get(&free.to_string()).cloned().unwrap_or_else(r0), t),
    )
}

fn subst_quad(q: &Quad, expr: &OMap<String, AffRat>) -> Option<Quad> {
    let mut out: Quad = OMap::new();
    for (key, c) in q.iter() {
        if key.is_empty() {
            let mut k = OMap::new();
            k.set(String::new(), c.clone());
            out = qadd(&out, &k);
            continue;
        }
        let names: Vec<&str> = key.split(',').collect();
        let get = |n: &str| {
            expr.get(&n.to_string())
                .cloned()
                .unwrap_or_else(|| aff(&RAT, None))
        };
        let second = match names.get(1) {
            Some(n) => get(n),
            None => aff(&RAT, Some(r1())),
        };
        let piece = aff_mul(&get(names[0]), &second)?;
        out = qadd(&out, &qscale(&piece, c));
    }
    Some(out)
}

fn is_square(n: &BigInt) -> Option<BigInt> {
    if n.is_negative() {
        return None;
    }
    let r = n.sqrt();
    (&r * &r == *n).then_some(r)
}

fn quad_roots(a: &Rat, b: &Rat, c: &Rat) -> Option<Vec<Rat>> {
    if rzero(a) && rzero(b) {
        return None;
    }
    if rzero(a) {
        return Some(vec![rdiv(&rneg(c), b)]);
    }
    let disc = rsub(&rmul(b, b), &rmul(&ri(4), &rmul(a, c)));
    if disc.n.is_negative() {
        return None;
    }
    let num = is_square(&disc.n)?;
    let den = is_square(&disc.d)?;
    let s = rat(num, den);
    let two_a = rmul(&ri(2), a);
    let cands = [
        rdiv(&radd(&rneg(b), &s), &two_a),
        rdiv(&rsub(&rneg(b), &s), &two_a),
    ];
    let mut out: Vec<Rat> = Vec::new();
    for r in cands {
        if !out.contains(&r) {
            out.push(r);
        }
    }
    Some(out)
}

fn try_quadratic(diffs: &[&Node], variables: &[String]) -> Option<SystemResult> {
    let mut linear: Vec<&Node> = Vec::new();
    let mut quads: Vec<Quad> = Vec::new();
    for diff in diffs {
        let q = walk_quad(diff, variables)?;
        if qdegree(&q) <= 1 {
            linear.push(diff);
        } else {
            quads.push(q);
        }
    }
    if quads.is_empty() {
        return None;
    }
    let rows = matrix_from(&linear, variables, &RAT, &const_rat)?;
    let (pivots, lrows) = if rows.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        match eliminate(&rows, &RAT) {
            Some(x) => x,
            None => return Some(SystemResult::inconsistent(variables)),
        }
    };
    let free_idx: Vec<usize> = (0..variables.len())
        .filter(|i| !pivots.contains(i))
        .collect();
    if free_idx.len() != 1 {
        return None;
    }
    let fi = free_idx[0];
    let free_name = variables[fi].clone();
    let mut expr: OMap<String, AffRat> = OMap::new();
    for (col, name) in variables.iter().enumerate() {
        let Some(k_row) = pivots.iter().position(|p| *p == col) else {
            let mut a = aff(&RAT, None);
            a.v.set(free_name.clone(), r1());
            expr.set(name.clone(), a);
            continue;
        };
        let row = &lrows[k_row];
        let mut a = aff(&RAT, Some(row[variables.len()].clone()));
        let c = rneg(&row[fi]);
        if !rzero(&c) {
            a.v.set(free_name.clone(), c);
        }
        expr.set(name.clone(), a);
    }
    let mut poly: Quad = OMap::new();
    for q in &quads {
        let sub = subst_quad(q, &expr)?;
        poly = qadd(&poly, &sub);
    }
    let a = poly
        .get(&format!("{free_name},{free_name}"))
        .cloned()
        .unwrap_or_else(r0);
    let b = poly.get(&free_name).cloned().unwrap_or_else(r0);
    let c = poly.get(&String::new()).cloned().unwrap_or_else(r0);
    let Some(roots) = quad_roots(&a, &b, &c) else {
        return Some(if rzero(&a) && rzero(&b) && rzero(&c) {
            finish(&rows, &RAT, variables)
        } else {
            SystemResult::inconsistent(variables)
        });
    };
    let solutions: Vec<Vec<String>> = roots
        .iter()
        .map(|t| {
            variables
                .iter()
                .map(|n| {
                    rstr(&eval_aff(
                        expr.get(n).unwrap_or(&aff(&RAT, None)),
                        &free_name,
                        t,
                    ))
                })
                .collect()
        })
        .collect();
    let display = if solutions.len() == 1 {
        pairs(variables, &solutions[0])
    } else {
        solutions
            .iter()
            .map(|s| format!("({}) = ({})", variables.join(", "), s.join(", ")))
            .collect::<Vec<_>>()
            .join(" or ")
    };
    let mut r = SystemResult::new(
        if roots.len() == 1 {
            Status::Unique
        } else {
            Status::Finite
        },
        variables,
        display,
    );
    r.solutions = Some(solutions);
    Some(r)
}

#[derive(Clone)]
struct SqrtSlot {
    variable: String,
    scale: Rat,
    shift: Rat,
}

fn walk_sqrt(
    node: &Node,
    unknowns: &[String],
    slots: &mut OMap<String, SqrtSlot>,
) -> Option<AffRat> {
    let node = unwrap(node);
    if let Some(c) = const_rat(node) {
        return Some(aff(&RAT, Some(c)));
    }
    if fn_name(node) == Some("sqrt") {
        let arg = *args_of(node).first()?;
        let inside = walk_aff(arg, &RAT, unknowns, &const_rat)?;
        let vars: Vec<&(String, Rat)> = inside.v.iter().filter(|(_, c)| !rzero(c)).collect();
        if vars.len() != 1 {
            return None;
        }
        let (variable, scale) = vars[0].clone();
        let key = format!("sqrt({}*{variable}+{})", rstr(&scale), rstr(&inside.k));
        slots.set(
            key.clone(),
            SqrtSlot {
                variable,
                scale,
                shift: inside.k.clone(),
            },
        );
        let mut a = aff(&RAT, None);
        a.v.set(key, r1());
        return Some(a);
    }
    let (op, args) = op_of(node)?;
    if (op == "-" || op == "+") && args.len() == 1 {
        let a = walk_sqrt(args[0], unknowns, slots)?;
        return Some(if op == "-" {
            scale_a(&a, &ri(-1), &RAT)
        } else {
            a
        });
    }
    if args.len() != 2 {
        return None;
    }
    match op {
        "+" | "-" => {
            let a = walk_sqrt(args[0], unknowns, slots)?;
            let b = walk_sqrt(args[1], unknowns, slots)?;
            Some(if op == "+" {
                add_a(&a, &b, &RAT)
            } else {
                sub_a(&a, &b, &RAT)
            })
        }
        "*" => {
            let a = walk_sqrt(args[0], unknowns, slots)?;
            let b = walk_sqrt(args[1], unknowns, slots)?;
            if is_const(&a, &RAT) {
                return Some(scale_a(&b, &a.k, &RAT));
            }
            if is_const(&b, &RAT) {
                return Some(scale_a(&a, &b.k, &RAT));
            }
            None
        }
        "/" => {
            let a = walk_sqrt(args[0], unknowns, slots)?;
            let den = const_rat(args[1])?;
            if rzero(&den) {
                return None;
            }
            Some(scale_a(&a, &rdiv(&r1(), &den), &RAT))
        }
        _ => None,
    }
}

fn try_sqrt_linear(diffs: &[&Node], variables: &[String]) -> Option<SystemResult> {
    let mut slots: OMap<String, SqrtSlot> = OMap::new();
    let mut keys: Vec<String> = Vec::new();
    for diff in diffs {
        let a = walk_sqrt(diff, variables, &mut slots)?;
        for k in a.v.keys() {
            if !keys.contains(k) {
                keys.push(k.clone());
            }
        }
    }
    if slots.is_empty() {
        return None;
    }
    let mut rows = Vec::new();
    for diff in diffs {
        let a = walk_sqrt(diff, variables, &mut slots)?;
        let mut row: Vec<Rat> = keys
            .iter()
            .map(|k| a.v.get(k).cloned().unwrap_or_else(r0))
            .collect();
        row.push(rneg(&a.k));
        rows.push(row);
    }
    let solved = finish(&rows, &RAT, &keys);
    if solved.status != Status::Unique {
        return None;
    }
    let u: Vec<Rat> = solved.solutions.as_ref()?[0]
        .iter()
        .map(|s| parse_rat_str(s))
        .collect::<Option<_>>()?;
    if u.iter().any(|v| v.n.is_negative()) {
        return None;
    }
    let mut by_var: OMap<String, (Rat, Rat, Rat)> = OMap::new();
    for (i, key) in keys.iter().enumerate() {
        let slot = slots.get(key)?.clone();
        if by_var.has(&slot.variable) {
            continue;
        }
        by_var.set(slot.variable, (slot.scale, slot.shift, rmul(&u[i], &u[i])));
    }
    if by_var.len() != variables.len() {
        return None;
    }
    let sol: Vec<String> = variables
        .iter()
        .map(|n| {
            let (scale, shift, u2) = by_var.get(n).cloned()?;
            Some(rstr(&rdiv(&rsub(&u2, &shift), &scale)))
        })
        .collect::<Option<_>>()?;
    Some(SystemResult::unique(variables, sol))
}

fn try_abs(texts: &[String], variables: &[String]) -> Option<SystemResult> {
    if !texts.iter().any(|t| t.contains("abs(") || t.contains('|')) {
        return None;
    }
    let abs_vars: Vec<&String> = variables
        .iter()
        .filter(|n| {
            texts
                .iter()
                .any(|t| t.contains(&format!("abs({n})")) || t.contains(&format!("|{n}|")))
        })
        .collect();
    if abs_vars.is_empty() || abs_vars.len() > 4 {
        return None;
    }
    let mut sols: Vec<Vec<String>> = Vec::new();
    for mask in 0..(1usize << abs_vars.len()) {
        let signed: Vec<String> = texts
            .iter()
            .map(|t| {
                let mut s = t.clone();
                for (i, name) in abs_vars.iter().enumerate() {
                    let rep = if (mask >> i) & 1 == 1 {
                        format!("({name})")
                    } else {
                        format!("(-{name})")
                    };
                    s = s
                        .replace(&format!("abs({name})"), &rep)
                        .replace(&format!("|{name}|"), &rep);
                }
                s
            })
            .collect();
        let Some(parsed) = signed
            .iter()
            .map(|s| prep_eq(s))
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let diffs: Vec<&Node> = parsed.iter().map(|p| &p.diff).collect();
        let Some(solved) = solve_linear(&diffs, variables, &[]) else {
            continue;
        };
        if solved.status != Status::Unique {
            continue;
        }
        let Some(vals) = solved.solutions.as_ref().and_then(|s| {
            s[0].iter()
                .map(|v| parse_rat_str(v))
                .collect::<Option<Vec<_>>>()
        }) else {
            continue;
        };
        let ok = abs_vars.iter().enumerate().all(|(i, name)| {
            let v = &vals[variables.iter().position(|x| x == *name).unwrap_or(0)];
            if (mask >> i) & 1 == 1 {
                !v.n.is_negative()
            } else {
                !v.n.is_positive()
            }
        });
        if !ok {
            continue;
        }
        let strs: Vec<String> = vals.iter().map(rstr).collect();
        if !sols.contains(&strs) {
            sols.push(strs);
        }
    }
    if sols.is_empty() {
        return Some(SystemResult::inconsistent(variables));
    }
    let mut r = SystemResult::new(
        if sols.len() == 1 {
            Status::Unique
        } else {
            Status::Finite
        },
        variables,
        pairs(variables, &sols[0]),
    );
    r.solutions = Some(sols);
    Some(r)
}

fn angle_list(sin: bool, v: &Rat) -> Vec<String> {
    let key = rstr(v);
    let hit: Option<&[&str]> = if sin {
        match key.as_str() {
            "0" => Some(&["0", "pi"]),
            "1" => Some(&["pi/2"]),
            "-1" => Some(&["3*pi/2"]),
            "1/2" => Some(&["pi/6", "5*pi/6"]),
            "-1/2" => Some(&["7*pi/6", "11*pi/6"]),
            _ => None,
        }
    } else {
        match key.as_str() {
            "0" => Some(&["pi/2", "3*pi/2"]),
            "1" => Some(&["0"]),
            "-1" => Some(&["pi"]),
            "1/2" => Some(&["pi/3", "5*pi/3"]),
            "-1/2" => Some(&["2*pi/3", "4*pi/3"]),
            _ => None,
        }
    };
    if let Some(h) = hit {
        return h.iter().map(|s| s.to_string()).collect();
    }
    let unit = *v == r1() || *v == ri(-1);
    if !sin {
        let a = format!("acos({key})");
        return if unit {
            vec![a]
        } else {
            vec![a.clone(), format!("2*pi - {a}")]
        };
    }
    let a = format!("asin({key})");
    if *v == r1() {
        return vec!["pi/2".into()];
    }
    if *v == ri(-1) {
        return vec!["3*pi/2".into()];
    }
    vec![a.clone(), format!("pi - {a}")]
}

fn walk_trig(
    node: &Node,
    unknowns: &[String],
    atoms: &mut Vec<String>,
    kind: &mut OMap<String, (bool, String)>,
) -> Option<AffRat> {
    let node = unwrap(node);
    if let Some(c) = const_rat(node) {
        return Some(aff(&RAT, Some(c)));
    }
    let f = fn_name(node);
    if f == Some("sin") || f == Some("cos") {
        let f = f.unwrap_or_default().to_string();
        let arg = *args_of(node).first()?;
        let name = symbol_name(arg)?;
        if !unknowns.iter().any(|u| u == name) {
            return None;
        }
        let key = format!("{f}({name})");
        if !atoms.contains(&key) {
            atoms.push(key.clone());
        }
        kind.set(key.clone(), (f == "sin", name.to_string()));
        let mut a = aff(&RAT, None);
        a.v.set(key, r1());
        return Some(a);
    }
    let (op, args) = op_of(node)?;
    if (op == "-" || op == "+") && args.len() == 1 {
        let a = walk_trig(args[0], unknowns, atoms, kind)?;
        return Some(if op == "-" {
            scale_a(&a, &ri(-1), &RAT)
        } else {
            a
        });
    }
    if args.len() != 2 {
        return None;
    }
    match op {
        "+" | "-" => {
            let a = walk_trig(args[0], unknowns, atoms, kind)?;
            let b = walk_trig(args[1], unknowns, atoms, kind)?;
            Some(if op == "+" {
                add_a(&a, &b, &RAT)
            } else {
                sub_a(&a, &b, &RAT)
            })
        }
        "*" => {
            let a = walk_trig(args[0], unknowns, atoms, kind)?;
            let b = walk_trig(args[1], unknowns, atoms, kind)?;
            if is_const(&a, &RAT) {
                return Some(scale_a(&b, &a.k, &RAT));
            }
            if is_const(&b, &RAT) {
                return Some(scale_a(&a, &b.k, &RAT));
            }
            None
        }
        "/" => {
            let a = walk_trig(args[0], unknowns, atoms, kind)?;
            let den = const_rat(args[1])?;
            if rzero(&den) {
                return None;
            }
            Some(scale_a(&a, &rdiv(&r1(), &den), &RAT))
        }
        _ => None,
    }
}

fn try_trig(diffs: &[&Node], variables: &[String]) -> Option<SystemResult> {
    let mut atoms: Vec<String> = Vec::new();
    let mut kind: OMap<String, (bool, String)> = OMap::new();
    for diff in diffs {
        walk_trig(diff, variables, &mut atoms, &mut kind)?;
    }
    if atoms.is_empty() {
        return None;
    }
    let mut rows = Vec::new();
    for diff in diffs {
        let a = walk_trig(diff, variables, &mut atoms, &mut kind)?;
        let mut row: Vec<Rat> = atoms
            .iter()
            .map(|k| a.v.get(k).cloned().unwrap_or_else(r0))
            .collect();
        row.push(rneg(&a.k));
        rows.push(row);
    }
    let solved = finish(&rows, &RAT, &atoms);
    if solved.status != Status::Unique || solved.solutions.is_none() {
        return (solved.status == Status::Inconsistent).then_some(solved);
    }
    let values: Vec<Rat> = solved.solutions.as_ref()?[0]
        .iter()
        .map(|s| parse_rat_str(s))
        .collect::<Option<_>>()?;
    let mut angles: OMap<String, Vec<String>> = OMap::new();
    let mut shown: Vec<(String, String)> = Vec::new();
    for (i, atom) in atoms.iter().enumerate() {
        let (sin, name) = kind.get(atom)?.clone();
        let value = &values[i];
        if value.n.abs() > value.d {
            continue;
        }
        shown.push((atom.clone(), rstr(value)));
        angles.set(name, angle_list(sin, value));
    }
    if shown.len() != atoms.len() {
        return None;
    }
    let display = shown
        .iter()
        .map(|(e, v)| format!("{e} = {v}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut r = SystemResult::new(Status::Unique, variables, display);
    r.atoms = Some(shown);
    r.angles = Some(angles.e);
    Some(r)
}

// ---- the solver ----

/// `sys`, `sys3`, `sys 3`, `system 3`: Ok(count) or Err(hint).
pub fn sys_command(text: &str) -> Option<Result<u32, String>> {
    static RE: Lazy = Lazy::new(|| js(r"^sys(?:tem)?\s*(\d+)?\s*$", "i"));
    let c = RE.captures(text.trim()).ok().flatten()?;
    let Some(m) = c.get(1) else {
        return Some(Err("how many equations? sys2, sys3… up to sys5".into()));
    };
    let n: f64 = m.as_str().parse().unwrap_or(f64::NAN);
    if !(1.0..=5.0).contains(&n) || n.fract() != 0.0 {
        return Some(Err("up to 5 equations".into()));
    }
    Some(Ok(n as u32))
}

pub fn is_sys_command(text: &str) -> bool {
    static RE: Lazy = Lazy::new(|| js(r"^\s*sys(?:tem)?(?:\s|\d|$)", "i"));
    jsre::test(&RE, text) && sys_command(text).is_some()
}

/// `SystemAnswer`.
#[derive(Clone, Debug, PartialEq)]
pub struct SystemAnswer {
    pub display: String,
    pub message: bool,
}

/// Solve the equations that parse, including while later fields are empty or still being typed.
pub fn solve_live(
    lines: &[String],
    defaults: &DefaultUnits,
    mode: AngleMode,
) -> Option<SystemAnswer> {
    guarded(|| {
        let pending = lines.iter().any(|l| prep_eq(l).is_none());
        let ready: Vec<String> = lines
            .iter()
            .map(|l| l.trim().to_string())
            .filter(|l| prep_eq(l).is_some())
            .collect();
        if ready.is_empty() {
            return None;
        }
        let result = solve_with_units(&ready, defaults, mode, None)
            .or_else(|| solve_system(&ready, mode, None))?;
        if matches!(result.status, Status::Inconsistent | Status::Mismatch) {
            return Some(SystemAnswer {
                display: result.display,
                message: true,
            });
        }
        if result.status == Status::Infinite && pending {
            return result.pinned.map(|p| SystemAnswer {
                display: p,
                message: false,
            });
        }
        Some(SystemAnswer {
            display: result.display,
            message: false,
        })
    })
}

/// Solve up to five equations. None when the system isn't one this solver reads.
pub fn solve_system(
    equations: &[String],
    mode: AngleMode,
    guess: Option<&Guess>,
) -> Option<SystemResult> {
    guarded(|| solve_system_inner(equations, mode, guess))
}

fn solve_system_inner(
    equations: &[String],
    mode: AngleMode,
    guess: Option<&Guess>,
) -> Option<SystemResult> {
    static E_POW: Lazy = Lazy::new(|| js(r"(?<![A-Za-z0-9_])e\s*\^", ""));
    if equations.is_empty() || equations.len() > 5 {
        return None;
    }
    let ready: Vec<Parsed> = equations
        .iter()
        .map(|e| prep_eq(e))
        .collect::<Option<_>>()?;
    let mut names: Vec<String> = Vec::new();
    for p in &ready {
        symbols_of(&p.diff, &mut names);
    }
    if names.iter().any(|n| n == "e") && equations.iter().any(|eq| jsre::test(&E_POW, eq)) {
        names.retain(|n| n != "e");
    }
    let (unknowns, params) = if names.len() <= ready.len() {
        (names.clone(), Vec::new())
    } else {
        classify(&names)
    };
    if unknowns.is_empty() || unknowns.len() > 8 {
        return None;
    }
    let diffs: Vec<&Node> = ready.iter().map(|p| &p.diff).collect();
    let square = params.is_empty() && unknowns.len() == ready.len();
    if square
        && guess.is_some()
        && let Some(near) = numeric_result(&diffs, &unknowns, mode, guess)
    {
        return Some(near);
    }
    if params.is_empty() {
        if let Some(r) = try_exponential(&ready, &unknowns) {
            return Some(r);
        }
        if let Some(r) = try_reciprocal(&diffs, &unknowns) {
            return Some(r);
        }
        if let Some(r) = try_sqrt_linear(&diffs, &unknowns) {
            return Some(r);
        }
        if let Some(r) = try_abs(equations, &unknowns) {
            return Some(r);
        }
        if let Some(r) = try_trig(&diffs, &unknowns) {
            return Some(r);
        }
    }
    if let Some(r) = solve_linear(&diffs, &unknowns, &params) {
        return Some(r);
    }
    let quadratic = if params.is_empty() {
        try_quadratic(&diffs, &unknowns)
    } else {
        None
    };
    if let Some(q) = &quadratic
        && q.status != Status::Inconsistent
    {
        return quadratic;
    }
    if !square {
        return quadratic;
    }
    numeric_result(&diffs, &unknowns, mode, None)
        .or(quadratic)
        .or_else(|| {
            Some(SystemResult::new(
                Status::Inconsistent,
                &unknowns,
                "no solution found".into(),
            ))
        })
}

fn numeric_result(
    diffs: &[&Node],
    variables: &[String],
    mode: AngleMode,
    guess: Option<&Guess>,
) -> Option<SystemResult> {
    let owned: Vec<Node> = diffs.iter().map(|d| (*d).clone()).collect();
    let found = solve_numeric_system(&owned, variables, mode, guess)?;
    let solutions: Vec<Vec<String>> = found
        .solutions
        .iter()
        .map(|s| s.iter().map(|v| crate::jsnum::to_string(*v)).collect())
        .collect();
    let shown: Vec<Vec<String>> = found
        .solutions
        .iter()
        .map(|s| s.iter().map(|v| format_number(*v, 12.0)).collect())
        .collect();
    let more = if found.more { " or …" } else { "" };
    let display = if shown.len() == 1 {
        pairs(variables, &shown[0])
    } else {
        shown
            .iter()
            .map(|s| format!("({}) = ({})", variables.join(", "), s.join(", ")))
            .collect::<Vec<_>>()
            .join(" or ")
            + more
    };
    let mut r = SystemResult::new(
        if solutions.len() == 1 && !found.more {
            Status::Unique
        } else {
            Status::Finite
        },
        variables,
        display,
    );
    r.solutions = Some(solutions);
    Some(r)
}

// ---- units ----

const UNIT_WORD: &str = "[A-Za-zΩµμ°][A-Za-zΩµμ°]*";
const UNIT_POW: &str = r"(?:\s*(?:\^\s*-?\d+|[²³]))?";
const DIMS: usize = 7;

fn quantity_re() -> &'static jsre::Regex {
    static RE: crate::jsre::Lazy<jsre::Regex> = crate::jsre::Lazy::new(|| {
        js(
            &format!(
                r"(?<![\w.])((?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?)(\s*)({UNIT_WORD}{UNIT_POW}(?:\s*[*/·]\s*{UNIT_WORD}{UNIT_POW})*)"
            ),
            "",
        )
    });
    &RE
}

/// An exact rational for an SI float, kept within mathjs safe integers.
fn si_rat_text(x: f64) -> Option<String> {
    static RE: Lazy = Lazy::new(|| js(r"^(\d+)(?:\.(\d+))?(?:e([+-]\d+))?$", ""));
    let p = crate::jsnum::to_precision(x.abs(), 15);
    let c = RE.captures(p.as_str()).ok().flatten()?;
    let frac = jsre::group(&c, 2).trim_end_matches('0').to_string();
    let joined = format!("{}{frac}", jsre::group(&c, 1));
    let mut digits = {
        let t = joined.trim_start_matches('0');
        if t.is_empty() {
            "0".to_string()
        } else {
            t.to_string()
        }
    };
    let mut exp: i64 = c.get(3).map_or(0, |m| m.as_str().parse().unwrap_or(0)) - frac.len() as i64;
    while digits.len() > 1 && digits.ends_with('0') {
        digits.pop();
        exp += 1;
    }
    let pow = |mut k: i64| {
        let mut parts = Vec::new();
        while k > 0 {
            parts.push(format!("10^{}", k.min(12)));
            k -= 12;
        }
        parts.join("*")
    };
    let sign = if x < 0.0 { "-" } else { "" };
    Some(if exp == 0 {
        format!("({sign}{digits})")
    } else if exp > 0 {
        format!("({sign}{digits}*{})", pow(exp))
    } else {
        format!("({sign}{digits}/({}))", pow(-exp))
    })
}

/// `prepUnits`: each `number unit` as its SI value and as a marker.
#[derive(Clone, Debug)]
pub struct UnitPrep {
    pub plain: Vec<String>,
    pub marked: Vec<String>,
    pub quantities: Vec<SiQuantity>,
    pub texts: Vec<String>,
}

pub fn prep_units(lines: &[String], unknowns: &[String]) -> Option<UnitPrep> {
    static WORD: Lazy = Lazy::new(|| js(r"[A-Za-z_][A-Za-z0-9_]*", ""));
    let q = quantity_re();
    let mut bare: Vec<String> = unknowns.to_vec();
    for line in lines {
        let stripped = jsre::replace_all(q, line, |_| " ".into());
        for m in WORD.find_iter(&*stripped).flatten() {
            if !bare.iter().any(|b| b == m.as_str()) {
                bare.push(m.as_str().to_string());
            }
        }
    }
    let mut quantities = Vec::new();
    let mut texts = Vec::new();
    let mut plain = Vec::new();
    let mut marked = Vec::new();
    for line in lines {
        let mut p = String::new();
        let mut mk = String::new();
        let mut at = 0;
        for c in q.captures_iter(line.as_str()).flatten() {
            let Some(unit_text) = kept_unit(jsre::group(&c, 3), &bare) else {
                continue;
            };
            let Some(qty) = si_quantity(&format!("{} {unit_text}", jsre::group(&c, 1))) else {
                continue;
            };
            let Some(value) = si_rat_text(qty.si) else {
                continue;
            };
            let start = c.get(0).map_or(0, |m| m.start());
            let end = start + jsre::group(&c, 1).len() + jsre::group(&c, 2).len() + unit_text.len();
            let before = &line[at..start];
            p.push_str(&format!("{before}{value}"));
            mk.push_str(&format!("{before}(__q{})", quantities.len()));
            quantities.push(qty);
            texts.push(line[start..end].to_string());
            at = end;
        }
        plain.push(p + &line[at..]);
        marked.push(mk + &line[at..]);
    }
    (!quantities.is_empty()).then_some(UnitPrep {
        plain,
        marked,
        quantities,
        texts,
    })
}

fn kept_unit(text: &str, bare: &[String]) -> Option<String> {
    static RE: crate::jsre::Lazy<jsre::Regex> =
        crate::jsre::Lazy::new(|| js(&format!(r"(^|\s*[*/·]\s*)({UNIT_WORD})({UNIT_POW})"), ""));
    let mut parts: Vec<(String, bool)> = Vec::new();
    let mut pos = 0;
    while pos <= text.len() {
        let Ok(Some(c)) = RE.captures_from_pos(text, pos) else {
            break;
        };
        let Some(m) = c.get(0) else { break };
        if m.start() != pos {
            break;
        }
        let word = jsre::group(&c, 2);
        if !(reads_as_unit(word) || si_quantity(&format!("1 {word}")).is_some()) {
            break;
        }
        let plain_word: String = word
            .chars()
            .filter(|ch| !matches!(ch, '°' | 'µ' | 'μ' | 'Ω'))
            .collect();
        parts.push((m.as_str().to_string(), bare.contains(&plain_word)));
        if m.end() == pos {
            break;
        }
        pos = m.end();
    }
    while parts.last().is_some_and(|p| p.1) {
        parts.pop();
    }
    let s: String = parts.into_iter().map(|p| p.0).collect();
    (!s.is_empty()).then_some(s)
}

type Dim = Vec<f64>;

fn zero_dim() -> Dim {
    vec![0.0; DIMS]
}

fn dims_equal(a: &[f64], b: &[f64]) -> bool {
    (0..DIMS).all(|i| a.get(i).copied().unwrap_or(0.0) == b.get(i).copied().unwrap_or(0.0))
}

struct Clash;

struct Infer<'a> {
    quantities: &'a [SiQuantity],
    unknowns: &'a [String],
    assigned: OMap<String, Dim>,
    changed: bool,
    settle: bool,
}

impl Infer<'_> {
    fn known(&self, name: &str) -> Option<Dim> {
        if let Some(i) = name
            .strip_prefix("__q")
            .and_then(|k| k.parse::<usize>().ok())
        {
            return self.quantities.get(i).map(|q| q.dim.clone());
        }
        if let Some(d) = self.assigned.get(&name.to_string()) {
            return Some(d.clone());
        }
        if self.unknowns.iter().any(|u| u == name) {
            None
        } else {
            Some(zero_dim())
        }
    }

    fn dim_of(&mut self, node: &Node) -> Result<Option<Dim>, Clash> {
        let node = unwrap(node);
        if matches!(node, Node::Constant(_)) {
            return Ok(Some(zero_dim()));
        }
        if let Some(name) = symbol_name(node) {
            let d = self.known(name);
            return Ok(d.or_else(|| self.settle.then(zero_dim)));
        }
        if let Some(name) = fn_name(node) {
            let args = args_of(node);
            if name == "abs" && args.len() == 1 {
                return self.dim_of(args[0]);
            }
            if name == "sqrt" && args.len() == 1 {
                let Some(d) = self.dim_of(args[0])? else {
                    return Ok(None);
                };
                if d.iter().any(|x| x % 2.0 != 0.0) {
                    return Err(Clash);
                }
                return Ok(Some(d.iter().map(|x| x / 2.0).collect()));
            }
            for a in args {
                self.unify(a, &zero_dim())?;
            }
            return Ok(Some(zero_dim()));
        }
        let Some((op, args)) = op_of(node) else {
            return Err(Clash);
        };
        if args.len() == 1 {
            return self.dim_of(args[0]);
        }
        let (a, b) = (args[0], args[1]);
        match op {
            "+" | "-" => {
                let da = self.dim_of(a)?;
                let db = self.dim_of(b)?;
                if let (Some(x), Some(y)) = (&da, &db)
                    && !dims_equal(x, y)
                {
                    return Err(Clash);
                }
                let d = da.or(db);
                if let Some(d) = &d {
                    self.unify(a, d)?;
                    self.unify(b, d)?;
                }
                Ok(d)
            }
            "*" | "/" => {
                let da = self.dim_of(a)?;
                let db = self.dim_of(b)?;
                let (Some(da), Some(db)) = (da, db) else {
                    return Ok(None);
                };
                Ok(Some(
                    (0..DIMS)
                        .map(|i| {
                            if op == "*" {
                                da[i] + db[i]
                            } else {
                                da[i] - db[i]
                            }
                        })
                        .collect(),
                ))
            }
            "^" => {
                let e = const_rat(b);
                let da = self.dim_of(a)?;
                let Some(e) = e else {
                    self.unify(a, &zero_dim())?;
                    self.unify(b, &zero_dim())?;
                    return Ok(Some(zero_dim()));
                };
                let Some(da) = da else {
                    return Ok(None);
                };
                let (en, ed) = (big_f64(&e.n), big_f64(&e.d));
                let out: Dim = da.iter().map(|x| (x * en) / ed).collect();
                if out.iter().any(|x| x.fract() != 0.0 || !x.is_finite()) {
                    return Err(Clash);
                }
                Ok(Some(out))
            }
            _ => Err(Clash),
        }
    }

    fn unify(&mut self, node: &Node, d: &Dim) -> Result<(), Clash> {
        let node = unwrap(node);
        if matches!(node, Node::Constant(_)) {
            return if dims_equal(d, &zero_dim()) {
                Ok(())
            } else {
                Err(Clash)
            };
        }
        if let Some(name) = symbol_name(node) {
            if let Some(have) = self.known(name) {
                return if dims_equal(&have, d) {
                    Ok(())
                } else {
                    Err(Clash)
                };
            }
            self.assigned.set(name.to_string(), d.clone());
            self.changed = true;
            return Ok(());
        }
        if let Some(name) = fn_name(node) {
            let args = args_of(node);
            if name == "abs" && args.len() == 1 {
                self.unify(args[0], d)?;
            } else if name == "sqrt" && args.len() == 1 {
                self.unify(args[0], &d.iter().map(|x| x * 2.0).collect())?;
            } else if !dims_equal(d, &zero_dim()) {
                return Err(Clash);
            }
            return Ok(());
        }
        let Some((op, args)) = op_of(node) else {
            return Ok(());
        };
        if args.len() == 1 {
            return self.unify(args[0], d);
        }
        let (a, b) = (args[0], args[1]);
        match op {
            "+" | "-" => {
                self.unify(a, d)?;
                self.unify(b, d)
            }
            "*" | "/" => {
                let da = self.dim_of(a)?;
                let db = self.dim_of(b)?;
                let mul = op == "*";
                match (da, db) {
                    (Some(da), Some(db)) => {
                        let prod: Dim = (0..DIMS)
                            .map(|i| if mul { da[i] + db[i] } else { da[i] - db[i] })
                            .collect();
                        if !dims_equal(&prod, d) {
                            return Err(Clash);
                        }
                        Ok(())
                    }
                    (Some(da), None) => {
                        let t: Dim = (0..DIMS)
                            .map(|i| if mul { d[i] - da[i] } else { da[i] - d[i] })
                            .collect();
                        self.unify(b, &t)
                    }
                    (None, Some(db)) => {
                        let t: Dim = (0..DIMS)
                            .map(|i| if mul { d[i] - db[i] } else { d[i] + db[i] })
                            .collect();
                        self.unify(a, &t)
                    }
                    (None, None) => Ok(()),
                }
            }
            "^" => {
                let Some(e) = const_rat(b) else {
                    return Ok(());
                };
                if rzero(&e) {
                    return Ok(());
                }
                let (en, ed) = (big_f64(&e.n), big_f64(&e.d));
                let base: Dim = d.iter().map(|x| (x * ed) / en).collect();
                if base.iter().any(|x| x.fract() != 0.0 || !x.is_finite()) {
                    return Err(Clash);
                }
                self.unify(a, &base)
            }
            _ => Ok(()),
        }
    }

    fn pass(&mut self, eqs: &[Parsed]) -> Result<(), Clash> {
        for eq in eqs {
            let dl = self.dim_of(&eq.lhs)?;
            let dr = self.dim_of(&eq.rhs)?;
            if let (Some(l), Some(r)) = (&dl, &dr)
                && !dims_equal(l, r)
            {
                return Err(Clash);
            }
            if let Some(l) = &dl {
                self.unify(&eq.rhs, l)?;
            }
            if let Some(r) = &dr {
                self.unify(&eq.lhs, r)?;
            }
        }
        Ok(())
    }
}

fn big_f64(b: &BigInt) -> f64 {
    b.to_f64().unwrap_or(f64::NAN)
}

/// Each unknown's SI dimension; None when a sum or a side mixes units.
pub fn infer_dims(
    marked: &[String],
    quantities: &[SiQuantity],
    unknowns: &[String],
) -> Option<Vec<(String, Vec<f64>)>> {
    let eqs: Vec<Parsed> = marked.iter().map(|m| prep_eq(m)).collect::<Option<_>>()?;
    let mut inf = Infer {
        quantities,
        unknowns,
        assigned: OMap::new(),
        changed: false,
        settle: false,
    };
    let run = |inf: &mut Infer<'_>| -> Result<(), Clash> {
        for _ in 0..12 {
            inf.changed = false;
            inf.pass(&eqs)?;
            if !inf.changed {
                break;
            }
        }
        inf.settle = true;
        for name in unknowns {
            if !inf.assigned.has(name) {
                inf.assigned.set(name.clone(), zero_dim());
            }
        }
        inf.pass(&eqs)
    };
    run(&mut inf).ok()?;
    Some(inf.assigned.e)
}

fn show_si(exact: &str, d: &[f64], prep: &UnitPrep, defaults: &DefaultUnits) -> Option<String> {
    let node = crate::syntax::parse(exact).ok()?;
    let n = plain_number(&node, &|_| None, &no_overrides)?;
    let value = if dims_equal(d, &zero_dim()) {
        Value::number(n)
    } else {
        let unit = prep
            .quantities
            .iter()
            .find(|q| q.unit_id.is_some() && dims_equal(&q.dim, d))
            .and_then(|q| q.unit_id.clone());
        si_value(n, d, unit.as_deref(), defaults)?
    };
    (value.kind == crate::types::ValueKind::Number).then(|| format_value(&value, 12.0))
}

/// Solve with units by working in SI; None when no equation has a unit.
pub fn solve_with_units(
    equations: &[String],
    defaults: &DefaultUnits,
    mode: AngleMode,
    guess_text: Option<&[(String, String)]>,
) -> Option<SystemResult> {
    guarded(|| {
        let names: Vec<String> = guess_text
            .map(|g| g.iter().map(|(k, _)| k.clone()).collect())
            .unwrap_or_default();
        let prep = prep_units(equations, &names)?;
        let guess = match guess_text {
            Some(g) => Some(si_guesses(g)?),
            None => None,
        };
        let result = solve_system(&prep.plain, mode, guess.as_ref())?;
        if result.status == Status::Inconsistent {
            return Some(result);
        }
        if result.atoms.is_some() || result.angles.is_some() {
            return None;
        }
        let mismatch = SystemResult::new(Status::Mismatch, &[], "units don't match".into());
        let Some(dims) = infer_dims(&prep.marked, &prep.quantities, &result.variables) else {
            return Some(mismatch);
        };
        let dim = |name: &str| {
            dims.iter()
                .find(|(k, _)| k == name)
                .map(|(_, d)| d.clone())
                .unwrap_or_else(zero_dim)
        };
        let variables = result.variables.clone();
        if matches!(result.status, Status::Unique | Status::Finite) {
            let rows: Vec<Vec<String>> = result
                .solutions
                .as_ref()?
                .iter()
                .map(|sol| {
                    sol.iter()
                        .enumerate()
                        .map(|(i, exact)| show_si(exact, &dim(&variables[i]), &prep, defaults))
                        .collect::<Option<Vec<_>>>()
                })
                .collect::<Option<_>>()?;
            let display = if rows.len() == 1 {
                pairs(&variables, &rows[0])
            } else {
                rows.iter()
                    .map(|row| {
                        if variables.len() == 1 {
                            format!("{} = {}", variables[0], row[0])
                        } else {
                            format!("({}) = ({})", variables.join(", "), row.join(", "))
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" or ")
            };
            return Some(SystemResult { display, ..result });
        }
        let free = result.free.clone().unwrap_or_default();
        let mut pinned_parts: Vec<String> = Vec::new();
        for (i, name) in variables.iter().enumerate() {
            let Some(expr) = result.general.as_ref().and_then(|g| g.get(i)) else {
                continue;
            };
            if free.contains(name) {
                continue;
            }
            let mut syms = Vec::new();
            if let Ok(n) = crate::syntax::parse(expr) {
                symbols_of(&n, &mut syms);
            } else {
                throw();
                return None;
            }
            if syms.iter().any(|s| free.contains(s)) {
                continue;
            }
            if let Some(shown) = show_si(expr, &dim(name), &prep, defaults) {
                pinned_parts.push(format!("{name} = {shown}"));
            }
        }
        let pinned = pinned_parts.join(", ");
        let display = [pinned.clone(), format!("free: {}", free.join(", "))]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        Some(SystemResult {
            pinned: (!pinned.is_empty()).then_some(pinned),
            display,
            ..result
        })
    })
}

fn si_guesses(texts: &[(String, String)]) -> Option<Guess> {
    texts
        .iter()
        .map(|(name, text)| {
            let v = si_quantity(text)
                .map(|q| q.si)
                .or_else(|| guess_number(text))?;
            Some((name.clone(), v))
        })
        .collect()
}

/// `solve({eq1, eq2}, x = 1, y = 2)`.
pub fn solve_system_call_answer(
    equations: &[String],
    guess_text: &[(String, String)],
    mode: AngleMode,
    defaults: &DefaultUnits,
) -> Option<SystemAnswer> {
    guarded(|| {
        let texts = (!guess_text.is_empty()).then_some(guess_text);
        if let Some(w) = solve_with_units(equations, defaults, mode, texts) {
            return Some(SystemAnswer {
                message: matches!(w.status, Status::Inconsistent | Status::Mismatch),
                display: w.display,
            });
        }
        let guess = match texts {
            Some(t) => si_guesses(t),
            None => None,
        };
        let result = solve_system(equations, mode, guess.as_ref())?;
        Some(SystemAnswer {
            message: result.status == Status::Inconsistent,
            display: result.display,
        })
    })
}

/// The evaluate pipeline's form: the display.
pub fn solve_system_call(
    equations: &[String],
    guess_text: &[(String, String)],
    mode: AngleMode,
    defaults: &DefaultUnits,
) -> Option<String> {
    solve_system_call_answer(equations, guess_text, mode, defaults).map(|a| a.display)
}

/// `solve({…}, x = 1)`: the equations and the guess per name.
#[derive(Clone, Debug, PartialEq)]
pub struct SystemCall {
    pub equations: Vec<String>,
    pub guess_text: Vec<(String, String)>,
}

pub fn parse_system_call(text: &str) -> Option<SystemCall> {
    static HEAD: Lazy = Lazy::new(|| js(r"^solve\s*\(\s*\{", "i"));
    static GUESS: Lazy = Lazy::new(|| js(r"(?s)^([A-Za-z][A-Za-z0-9_]*|θ)\s*(?:=\s*(.+))?$", ""));
    let s = text.trim();
    let m = HEAD.find(s).ok().flatten()?;
    if !s.ends_with(')') {
        return None;
    }
    let close = matching_brace(s, m.end() - 1)?;
    let equations: Vec<String> = split_top_level(&s[m.end()..close], &[',', ';'])
        .into_iter()
        .map(|e| e.trim().to_string())
        .filter(|e| !e.is_empty())
        .collect();
    if equations.is_empty() || equations.len() > 5 || equations.iter().any(|e| !e.contains('=')) {
        return None;
    }
    let rest = s[close + 1..s.len() - 1].trim();
    let mut guess_text: Vec<(String, String)> = Vec::new();
    if !rest.is_empty() {
        let tail = rest.strip_prefix(',')?;
        for arg in split_top_level(tail, &[',']) {
            let arg = arg.trim();
            if arg.is_empty() {
                continue;
            }
            let c = GUESS.captures(arg).ok().flatten()?;
            if let Some(v) = c.get(2).filter(|v| !v.as_str().is_empty()) {
                let k = jsre::group(&c, 1).to_string();
                let v = v.as_str().trim().to_string();
                match guess_text.iter_mut().find(|(n, _)| *n == k) {
                    Some(slot) => slot.1 = v,
                    None => guess_text.push((k, v)),
                }
            }
        }
    }
    Some(SystemCall {
        equations,
        guess_text,
    })
}

/// A system typed on one line, the way a textbook writes it: `2x + y = 5, x - y = 1`, `…; …`, `solve 2x + y = 5,
/// x - y = 1`. Two to five equations, each with one `=`; a list of plain
/// assignments (`a = 1, b = 2`) is not a system (native only).
pub fn parse_equation_list(text: &str) -> Option<Vec<String>> {
    static HEAD: Lazy = Lazy::new(|| js(r"^solve\b\s*", "i"));
    static NAME: Lazy = Lazy::new(|| js(r"^\s*[A-Za-z_][A-Za-z0-9_]*\s*$", ""));
    let mut s = text.trim();
    if let Some(m) = HEAD.find(s).ok().flatten() {
        s = s[m.end()..].trim();
    }
    // `solve(…, …)` is the current app's call (blank, or an equation and a guess), and not changed here
    if s.starts_with('(') || s.contains('[') || s.contains('{') {
        return None;
    }
    let parts: Vec<String> = split_top_level(s, &[',', ';'])
        .into_iter()
        .map(|e| e.trim().to_string())
        .collect();
    let one_eq = |e: &str| {
        e.matches('=').count() == 1 && !e.contains("==") && !e.contains("<=") && !e.contains(">=") && !e.contains("!=")
    };
    if !(2..=5).contains(&parts.len()) || parts.iter().any(|e| e.is_empty() || !one_eq(e)) {
        return None;
    }
    let all_assignments = parts.iter().all(|e| e.split_once('=').is_some_and(|(l, _)| jsre::test(&NAME, l)));
    (!all_assignments).then_some(parts)
}

fn matching_brace(s: &str, open: usize) -> Option<usize> {
    let mut depth = 0;
    for (i, ch) in s.bytes().enumerate().skip(open) {
        if ch == b'{' {
            depth += 1;
        } else if ch == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

fn split_top_level<'a>(s: &'a str, sep: &[char]) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        if "([{".contains(ch) {
            depth += 1;
        } else if ")]}".contains(ch) {
            depth -= 1;
        } else if depth == 0 && sep.contains(&ch) {
            out.push(&s[start..i]);
            start = i + ch.len_utf8();
        }
    }
    out.push(&s[start..]);
    out
}

// ---- golden replay ----

fn strings(j: Option<&Json>) -> Vec<String> {
    j.and_then(Json::as_array)
        .map(|a| {
            a.iter()
                .map(|x| x.as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn mode_arg(j: Option<&Json>) -> AngleMode {
    j.map(crate::golden::decode)
        .and_then(|m| serde_json::from_value(m).ok())
        .unwrap_or_default()
}

fn answer_json(a: Option<SystemAnswer>) -> Json {
    a.map_or(Json::Null, |a| {
        let mut o = json!({ "display": a.display });
        if a.message {
            o["message"] = json!(true);
        }
        o
    })
}

/// Lines pasted from a worksheet as a system (`pastedEquations`): 2 to 5 lines (or `;`-separated), each an
/// equation or, like a moment sum, an expression taken as `= 0`. None when the paste isn't a system, so it goes in
/// the field as plain text.
pub fn pasted_equations(text: &str) -> Option<Vec<String>> {
    static DIGIT: Lazy = Lazy::new(|| js(r"\d", ""));
    static OP: Lazy = Lazy::new(|| js(r"[-+*/^]", ""));
    static LETTER: Lazy = Lazy::new(|| js(r"[A-Za-z]", ""));
    let lines: Vec<String> = text
        .replace("\r\n", "\n")
        .split(['\r', '\n', ';'])
        .map(|l| crate::app::js::trim(l).to_string())
        .filter(|l| !l.is_empty())
        .collect();
    if !(2..=5).contains(&lines.len()) || !lines.iter().any(|l| l.contains('=')) {
        return None;
    }
    // a line with no = has to look like a sum, not words (mathjs would read `hello there` as hello*there)
    if lines
        .iter()
        .any(|l| !l.contains('=') && !(jsre::test(&DIGIT, l) && jsre::test(&OP, l)))
    {
        return None;
    }
    let equations: Vec<String> = lines
        .into_iter()
        .map(|l| {
            if l.contains('=') {
                l
            } else {
                format!("{l} = 0")
            }
        })
        .collect();
    equations
        .iter()
        .all(|eq| prep_eq(eq).is_some() && jsre::test(&LETTER, eq))
        .then_some(equations)
}

pub fn golden_dispatch(name: &str, args: &[Json]) -> Option<Json> {
    let st = |i: usize| crate::golden::str_arg(args, i).unwrap_or_default();
    let defaults = |i: usize| -> DefaultUnits {
        args.get(i)
            .map(crate::golden::decode)
            .and_then(|d| serde_json::from_value(d).ok())
            .unwrap_or_default()
    };
    Some(match name {
        "sysCommand" => match sys_command(&st(0)) {
            None => Json::Null,
            Some(Ok(n)) => json!({ "count": n }),
            Some(Err(h)) => json!({ "hint": h }),
        },
        "isSysCommand" => json!(is_sys_command(&st(0))),
        "pastedEquations" => pasted_equations(&st(0)).map_or(Json::Null, |l| json!(l)),
        "parseSystemCall" => parse_system_call(&st(0)).map_or(Json::Null, |c| {
            let mut g = serde_json::Map::new();
            for (k, v) in c.guess_text {
                g.insert(k, json!(v));
            }
            json!({ "equations": c.equations, "guessText": g })
        }),
        "solveLive" => answer_json(solve_live(
            &strings(args.first()),
            &defaults(1),
            mode_arg(args.get(2)),
        )),
        "solveSystem" => {
            let o = args.get(1).map(crate::golden::decode).unwrap_or(Json::Null);
            let mode = mode_arg(o.get("angleMode"));
            let guess: Option<Guess> = o.get("guess").and_then(Json::as_object).map(|g| {
                g.iter()
                    .map(|(k, v)| (k.clone(), v.as_f64().unwrap_or(f64::NAN)))
                    .collect()
            });
            solve_system(&strings(args.first()), mode, guess.as_ref())
                .map_or(Json::Null, |r| r.to_json())
        }
        "solveSystemCall" => {
            let g: Vec<(String, String)> = args
                .get(1)
                .and_then(Json::as_object)
                .map(|o| {
                    o.iter()
                        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let o = args.get(2).map(crate::golden::decode).unwrap_or(Json::Null);
            let d: DefaultUnits = o
                .get("defaults")
                .and_then(|d| serde_json::from_value(d.clone()).ok())
                .unwrap_or_default();
            answer_json(solve_system_call_answer(
                &strings(args.first()),
                &g,
                mode_arg(o.get("angleMode")),
                &d,
            ))
        }
        _ => return None,
    })
}

#[cfg(test)]
mod worksheet_tests {
    use super::*;

    fn live(lines: &[&str]) -> String {
        let lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        answer_json(solve_live(&lines, &DefaultUnits::default(), AngleMode::Deg))["display"]
            .as_str()
            .unwrap_or("")
            .to_string()
    }

    #[test]
    fn a_name_before_parentheses_multiplies_unless_mathjs_knows_it() {
        assert_eq!(
            live(&["20(7.5)-B_y(5)+40(2) = 0", "B_y+D_y = 60"]),
            "B_y = 46, D_y = 14"
        );
        assert_eq!(live(&["x(2) = 6", "y = x + 1"]), "x = 3, y = 4");
        assert_eq!(live(&["sqrt(x) = 3", "y = x"]), "x = 9, y = 9");
        assert_eq!(live(&["ln(x) = 0", "y = x + 1"]), "x = 1, y = 2");
    }

    #[test]
    fn a_pasted_system_one_equation_per_line() {
        let v = |a: &[&str]| Some(a.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(
            pasted_equations("20(7.5)-B_y(5)+40(2)\nB_y+D_y = 60"),
            v(&["20(7.5)-B_y(5)+40(2) = 0", "B_y+D_y = 60"])
        );
        assert_eq!(
            pasted_equations("x + y = 3\r\n\r\nx - y = 1\n"),
            v(&["x + y = 3", "x - y = 1"])
        );
        assert_eq!(
            pasted_equations("x + y = 3; x - y = 1"),
            v(&["x + y = 3", "x - y = 1"])
        );
    }

    #[test]
    fn anything_else_is_plain_text() {
        assert_eq!(pasted_equations("x + y = 3"), None);
        assert_eq!(pasted_equations("2 + 3\n4 + 5"), None);
        assert_eq!(
            pasted_equations("a = 1\nb = 2\nc = 3\nd = 4\ne = 5\nf = 6"),
            None
        );
        assert_eq!(pasted_equations("hello there\nx = 2"), None);
    }
}
