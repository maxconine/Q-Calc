//! mathjs `simplify` (`function/algebra/simplify.js`, `simplify/util.js`, `simplify/wildcards.js`) and
//! `simplifyCore`; `simplifyConstant` is in [`super::constant`].
//!
//! The rule engine is ported whole, and so is mathjs's default rule list in its order, so any input simplifies
//! the way mathjs simplifies it. The parts that make results depend on more than the rules:
//! - before every rule the tree is flattened (`a+(b+c)` → `+(a,b,c)`) and after it unflattened to a left-heavy
//!   binary tree, so the tree's shape between rules is mathjs's;
//! - an associative rule also gets an "expanded" form with an extra placeholder (`n+n` matches `x+1+x`), and
//!   n-ary nodes are matched through every binary split (`getSplits`), first match winning;
//! - constant matches use mathjs `equal` (relative tolerance 1e-12, absolute 1e-15);
//! - the loop ends when the `{parenthesis: 'all'}` text repeats.
//!
//! Quirks kept: `flatten` merges children by operator *symbol*, so an `add` swallows a `unaryPlus` child; the rule
//! `n-(n1+n2) -> n-n1-n2` assumes the context `addition` (sic), which no context defines, so it never applies;
//! `simplifyCore` rebuilds a three-argument `add(a, b, c)` call as `add(a, add(c, b))` (its `[last, seclast]`).

use std::collections::HashSet;
use std::sync::OnceLock;

use super::constant::simplify_constant;
use super::print::{PrintOptions, to_string};
use super::{Constant, Node, Op, SymError, err, function_name, parse, remove_parens, sym};

/// `options` of `simplify` that the engine and `rationalize` pass.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct SimplifyOptions {
    /// `exactFractions` (default true): fold constants into exact fractions.
    pub exact_fractions: Option<bool>,
    /// `fractionsLimit` (mathjs's default is no limit, despite its documentation saying 10000).
    pub fractions_limit: Option<f64>,
}

// ---- contexts (simplify/util.js)

/// An operator property: `trivial`, `total`, `commutative`, `associative`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prop {
    Trivial,
    Total,
    Commutative,
    Associative,
}

/// A context: overrides of the default operator properties (only rules' `imposeContext` makes one here).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ctx(Vec<(&'static str, Prop, bool)>);

impl Ctx {
    fn lookup(&self, name: &str, prop: Prop) -> bool {
        self.0
            .iter()
            .find(|(n, p, _)| *n == name && *p == prop)
            .map_or_else(|| default_property(name, prop), |(_, _, v)| *v)
    }

    /// `mergeContext(primary, secondary)` with `self` as primary.
    fn merged(&self, secondary: &Ctx) -> Ctx {
        let mut out = self.0.clone();
        for e in &secondary.0 {
            if !out.iter().any(|(n, p, _)| *n == e.0 && *p == e.1) {
                out.push(*e);
            }
        }
        Ctx(out)
    }
}

fn default_property(name: &str, prop: Prop) -> bool {
    use Prop::*;
    match name {
        "add" | "unaryPlus" | "multiply" => true,
        "subtract" | "divide" => matches!(prop, Total),
        "paren" => !matches!(prop, Associative),
        _ => matches!(prop, Total),
    }
}

/// The name `hasProperty` looks a node up by.
fn property_name(n: &Node) -> &str {
    match n {
        Node::Operator { op, .. } => op.name(),
        Node::Function { .. } => function_name(n),
        Node::Parenthesis(_) => "paren",
        _ => "defaultF",
    }
}

pub(crate) fn has_property(n: &Node, prop: Prop, ctx: &Ctx) -> bool {
    ctx.lookup(property_name(n), prop)
}

pub(crate) fn is_commutative(n: &Node, ctx: &Ctx) -> bool {
    has_property(n, Prop::Commutative, ctx)
}

pub(crate) fn is_associative(n: &Node, ctx: &Ctx) -> bool {
    has_property(n, Prop::Associative, ctx)
}

/// `createMakeNodeFunction(node)`: same operator (and `implicit`), or the same function name.
pub(crate) fn make_node(template: &Node, args: Vec<Node>) -> Node {
    match template {
        Node::Operator { op, implicit, .. } => Node::Operator {
            op: *op,
            args,
            implicit: *implicit,
            percentage: false,
        },
        _ => Node::Function {
            callee: Box::new(sym(function_name(template))),
            args,
        },
    }
}

fn node_args(n: &Node) -> Option<&Vec<Node>> {
    match n {
        Node::Operator { args, .. } | Node::Function { args, .. } => Some(args),
        _ => None,
    }
}

fn with_args(n: &Node, new_args: Vec<Node>) -> Node {
    match n {
        Node::Operator {
            op,
            implicit,
            percentage,
            ..
        } => Node::Operator {
            op: *op,
            args: new_args,
            implicit: *implicit,
            percentage: *percentage,
        },
        Node::Function { callee, .. } => Node::Function {
            callee: callee.clone(),
            args: new_args,
        },
        other => other.clone(),
    }
}

/// `allChildren`: the arguments as if flattened; same-symbol operator children are merged.
pub(crate) fn all_children(n: &Node, ctx: &Ctx) -> Vec<Node> {
    let Some(a) = node_args(n) else {
        return Vec::new();
    };
    if !is_associative(n, ctx) {
        return a.clone();
    }
    let Node::Operator { op, .. } = n else {
        // a FunctionNode has no `op`, so no child shares it
        return a.clone();
    };
    let symbol = op.symbol();
    let mut out = Vec::new();
    fn find(n: &Node, symbol: &str, out: &mut Vec<Node>) {
        for child in node_args(n).into_iter().flatten() {
            match child {
                Node::Operator { op, .. } if op.symbol() == symbol => find(child, symbol, out),
                _ => out.push(child.clone()),
            }
        }
    }
    find(n, symbol, &mut out);
    out
}

/// `flatten(node)`: associative operators become n-ary, all the way down (not inside parentheses).
pub(crate) fn flatten(n: &Node, ctx: &Ctx) -> Node {
    match node_args(n) {
        Some(a) if !a.is_empty() => {
            let kids = all_children(n, ctx);
            with_args(n, kids.iter().map(|k| flatten(k, ctx)).collect())
        }
        _ => n.clone(),
    }
}

/// `unflattenl`: n-ary associative operators back to left-heavy binary trees.
pub(crate) fn unflatten_l(n: &Node, ctx: &Ctx) -> Node {
    let Some(a) = node_args(n).filter(|a| !a.is_empty()) else {
        return n.clone();
    };
    let mut kids: Vec<Node> = a.iter().map(|k| unflatten_l(k, ctx)).collect();
    if kids.len() > 2 && is_associative(n, ctx) {
        let mut it = kids.into_iter();
        let mut cur = it.next().expect("args");
        let rest: Vec<Node> = it.collect();
        let last_index = rest.len() - 1;
        for (i, next) in rest.into_iter().enumerate() {
            if i == last_index {
                kids = vec![cur, next];
                return with_args(n, kids);
            }
            cur = make_node(n, vec![cur, next]);
        }
        unreachable!()
    }
    with_args(n, kids)
}

/// `unflattenr`: n-ary associative operators to right-heavy binary trees.
pub(crate) fn unflatten_r(n: &Node, ctx: &Ctx) -> Node {
    let Some(a) = node_args(n).filter(|a| !a.is_empty()) else {
        return n.clone();
    };
    let mut kids: Vec<Node> = a.iter().map(|k| unflatten_r(k, ctx)).collect();
    if kids.len() > 2 && is_associative(n, ctx) {
        let mut cur = kids.pop().expect("args");
        while kids.len() > 1 {
            let prev = kids.pop().expect("args");
            cur = make_node(n, vec![prev, cur]);
        }
        let first = kids.pop().expect("args");
        return with_args(n, vec![first, cur]);
    }
    with_args(n, kids)
}

// ---- mathjs `equal` / `isZero` on constant values

/// mathjs `nearlyEqual(a, b, relTol 1e-12, absTol 1e-15)`.
pub(crate) fn nearly_equal(a: f64, b: f64) -> bool {
    if a.is_nan() || b.is_nan() {
        return false;
    }
    if !a.is_finite() || !b.is_finite() {
        return a == b;
    }
    if a == b {
        return true;
    }
    (a - b).abs() <= (1e-12 * a.abs().max(b.abs())).max(1e-15)
}

/// A constant's value as mathjs's typed conversions read it for `equal` and `isZero`.
fn constant_number(c: &Constant) -> Option<f64> {
    match c {
        Constant::Number { value, .. } => Some(*value),
        Constant::Str(s) => Some(crate::jsnum::parse(s.trim()).unwrap_or(f64::NAN)),
        Constant::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Constant::Null | Constant::Undefined => None,
    }
}

/// `equal(a.value, b.value)` for two constants.
pub(crate) fn values_equal(a: &Constant, b: &Constant) -> bool {
    match (a, b) {
        (Constant::Str(x), Constant::Str(y)) => x == y,
        _ => match (constant_number(a), constant_number(b)) {
            (Some(x), Some(y)) => nearly_equal(x, y),
            _ => false,
        },
    }
}

/// `isZero(node.value)` for a constant node (false for anything else).
pub(crate) fn is_zero_node(n: &Node) -> bool {
    match n {
        Node::Constant(c) => constant_number(c) == Some(0.0),
        _ => false,
    }
}

/// `equal(node.value, 1)` for a constant node.
pub(crate) fn is_one_node(n: &Node) -> bool {
    match n {
        Node::Constant(c) => constant_number(c).is_some_and(|v| nearly_equal(v, 1.0)),
        _ => false,
    }
}

// ---- rules

/// One entry of a rule list: a function rule, or a pattern rule after `_canonicalizeRule`.
#[derive(Clone, Debug)]
pub enum Rule {
    Core,
    Constant,
    Pattern(Box<PatternRule>),
}

#[derive(Clone, Debug)]
pub struct PatternRule {
    pub l: Node,
    pub r: Node,
    pub repeat: bool,
    /// The rule's `assuming` held in the (default) context it was built for.
    pub enabled: bool,
    pub impose: Ctx,
    pub expanded: Option<(Node, Node)>,
    pub expanded_nc: Option<((Node, Node), (Node, Node))>,
}

/// A rule as written in mathjs's source.
#[derive(Clone, Copy, Debug)]
pub enum Spec {
    Core,
    Constant,
    Rule {
        l: &'static str,
        r: &'static str,
        repeat: bool,
        assuming: &'static [(&'static str, Prop, bool)],
        impose: &'static [(&'static str, Prop, bool)],
    },
}

pub(crate) const fn rule(l: &'static str, r: &'static str) -> Spec {
    Spec::Rule {
        l,
        r,
        repeat: false,
        assuming: &[],
        impose: &[],
    }
}

const fn assume(
    l: &'static str,
    r: &'static str,
    assuming: &'static [(&'static str, Prop, bool)],
) -> Spec {
    Spec::Rule {
        l,
        r,
        repeat: false,
        assuming,
        impose: &[],
    }
}

use Prop::{Associative as A, Commutative as C, Total as T};

/// `simplify.rules`, in order.
const DEFAULT_RULES: &[Spec] = &[
    Spec::Core,
    rule("log(e)", "1"),
    assume("n-n1", "n+-n1", &[("subtract", T, true)]),
    assume("n-n", "0", &[("subtract", T, false)]),
    assume(
        "-(cl*v)",
        "v * (-cl)",
        &[("multiply", C, true), ("subtract", T, true)],
    ),
    assume(
        "-(cl*v)",
        "(-cl) * v",
        &[("multiply", C, false), ("subtract", T, true)],
    ),
    assume(
        "-(v*cl)",
        "v * (-cl)",
        &[("multiply", C, false), ("subtract", T, true)],
    ),
    rule("-(n1/n2)", "-n1/n2"),
    rule("-v", "v * (-1)"),
    Spec::Rule {
        l: "(n1 + n2)*(-1)",
        r: "n1*(-1) + n2*(-1)",
        repeat: true,
        assuming: &[],
        impose: &[],
    },
    rule("n/n1^n2", "n*n1^-n2"),
    rule("n/n1", "n*n1^-1"),
    assume("(n1*n2)^n3", "n1^n3 * n2^n3", &[("multiply", C, true)]),
    assume(
        "(n1*n2)^(-1)",
        "n2^(-1) * n1^(-1)",
        &[("multiply", C, false)],
    ),
    assume("(n ^ n1) ^ n2", "n ^ (n1 * n2)", &[("divide", T, true)]),
    rule(" vd   * ( vd   * n1 + n2)", "vd^2       * n1 +  vd   * n2"),
    assume(
        " vd   * (vd^n4 * n1 + n2)   ",
        "  vd^(1+n4)  * n1 +  vd   * n2",
        &[("divide", T, true)],
    ),
    assume(
        "vd^n3 * ( vd   * n1 + n2)   ",
        "  vd^(n3+1)  * n1 + vd^n3 * n2",
        &[("divide", T, true)],
    ),
    assume(
        "vd^n3 * (vd^n4 * n1 + n2)   ",
        "  vd^(n3+n4) * n1 + vd^n3 * n2",
        &[("divide", T, true)],
    ),
    rule("n*n", "n^2"),
    assume("n * n^n1 ", " n^(n1+1)", &[("divide", T, true)]),
    assume("n^n1 * n^n2 ", " n^(n1+n2)", &[("divide", T, true)]),
    Spec::Constant,
    assume("n+n ", " 2*n", &[("add", T, true)]),
    rule("n+-n", "0"),
    rule("vd*n + vd", "vd*(n+1)"),
    rule("n3*n1 + n3*n2", "n3*(n1+n2)"),
    rule("n3^(-n4)*n1 +   n3  * n2", "n3^(-n4)*(n1 + n3^(n4+1) *n2)"),
    rule("n3^(-n4)*n1 + n3^n5 * n2", "n3^(-n4)*(n1 + n3^(n4+n5)*n2)"),
    assume("n*vd + vd ", " (n+1)*vd", &[("multiply", C, false)]),
    assume("vd + n*vd ", " (1+n)*vd", &[("multiply", C, false)]),
    assume("n1*n3 + n2*n3 ", " (n1+n2)*n3", &[("multiply", C, false)]),
    assume(
        "n^n1 * n ",
        " n^(n1+1)",
        &[("divide", T, true), ("multiply", C, false)],
    ),
    assume(
        "n1*n3^(-n4) + n2 * n3    ",
        " (n1 + n2*n3^(n4 +  1))*n3^(-n4)",
        &[("multiply", C, false)],
    ),
    assume(
        "n1*n3^(-n4) + n2 * n3^n5 ",
        " (n1 + n2*n3^(n4 + n5))*n3^(-n4)",
        &[("multiply", C, false)],
    ),
    rule("n*cd + cd", "(n+1)*cd"),
    assume("cd*n + cd ", " cd*(n+1)", &[("multiply", C, false)]),
    assume("cd + cd*n ", " cd*(1+n)", &[("multiply", C, false)]),
    Spec::Constant,
    assume("(-n)*n1 ", " -(n*n1)", &[("subtract", T, true)]),
    assume(
        "n1*(-n) ",
        " -(n1*n)",
        &[("subtract", T, true), ("multiply", C, false)],
    ),
    Spec::Rule {
        l: "ce+ve ",
        r: " ve+ce",
        repeat: false,
        assuming: &[("add", C, true)],
        impose: &[("add", C, false)],
    },
    Spec::Rule {
        l: "vd*cd ",
        r: " cd*vd",
        repeat: false,
        assuming: &[("multiply", C, true)],
        impose: &[("multiply", C, false)],
    },
    rule("n+-n1", "n-n1"),
    rule("n+-(n1)", "n-(n1)"),
    assume("n*(n1^-1) ", " n/n1", &[("multiply", C, true)]),
    assume("n*n1^-n2 ", " n/n1^n2", &[("multiply", C, true)]),
    assume("n^-1 ", " 1/n", &[("multiply", C, true)]),
    rule("n^1", "n"),
    assume("n*(n1/n2) ", " (n*n1)/n2", &[("multiply", A, true)]),
    assume(
        "n-(n1+n2) ",
        " n-n1-n2",
        &[("addition", A, true), ("addition", C, true)],
    ),
    Spec::Rule {
        l: "1*n",
        r: "n",
        repeat: false,
        assuming: &[],
        impose: &[("multiply", C, true)],
    },
    assume("n1/(n2/n3) ", " (n1*n3)/n2", &[("multiply", A, true)]),
    rule("n1/(-n2)", "-n1/n2"),
];

/// `_canonicalizeRule` + `_buildRules` for a list (context: the default one).
pub fn build_rules(specs: &[Spec]) -> Vec<Rule> {
    let ctx = Ctx::default();
    let mut counter = 0usize;
    let mut placeholder = || {
        let s = sym(&format!("_p{counter}"));
        counter += 1;
        s
    };
    specs
        .iter()
        .map(|s| match *s {
            Spec::Core => Rule::Core,
            Spec::Constant => Rule::Constant,
            Spec::Rule {
                l,
                r,
                repeat,
                assuming,
                impose,
            } => {
                let mut l = remove_parens(&parse(l).expect("rule parses"));
                let r = remove_parens(&parse(r).expect("rule parses"));
                let enabled = assuming
                    .iter()
                    .all(|(name, prop, want)| ctx.lookup(name, *prop) == *want);
                let mut expanded = None;
                let mut expanded_nc = None;
                if is_associative(&l, &ctx) {
                    let non_commutative = !is_commutative(&l, &ctx);
                    let left = non_commutative.then(&mut placeholder);
                    let p = placeholder();
                    // flatten + unflattenr on `makeNode([l, p])` also rewrites, in place, every subtree of `l`
                    // that becomes one of its arguments; `l` keeps those rewrites
                    l = rewrite_frontier(&l, &ctx);
                    let el = unflatten_r(
                        &flatten(&make_node(&l, vec![l.clone(), p.clone()]), &ctx),
                        &ctx,
                    );
                    let er = make_node(&l, vec![r.clone(), p]);
                    if let Some(lp) = left {
                        let nc1 = (
                            make_node(&l, vec![lp.clone(), l.clone()]),
                            make_node(&l, vec![lp.clone(), r.clone()]),
                        );
                        let nc2 = (
                            make_node(&l, vec![lp.clone(), el.clone()]),
                            make_node(&l, vec![lp, er.clone()]),
                        );
                        expanded_nc = Some((nc1, nc2));
                    }
                    expanded = Some((el, er));
                }
                Rule::Pattern(Box::new(PatternRule {
                    l,
                    r,
                    repeat,
                    enabled,
                    impose: Ctx(impose.to_vec()),
                    expanded,
                    expanded_nc,
                }))
            }
        })
        .collect()
}

/// The side effect of `flatten(makeNode([l, p]))` + `unflattenr` on `l`: each subtree that ends up as a direct
/// argument of the expanded node (the frontier below `l`'s chain of its own operator) is flattened and then
/// unflattened to the right.
fn rewrite_frontier(l: &Node, ctx: &Ctx) -> Node {
    let Node::Operator { op, .. } = l else {
        return l.clone();
    };
    let symbol = op.symbol();
    fn walk(n: &Node, symbol: &str, ctx: &Ctx) -> Node {
        let kids: Vec<Node> = node_args(n)
            .into_iter()
            .flatten()
            .map(|c| match c {
                Node::Operator { op, .. } if op.symbol() == symbol => walk(c, symbol, ctx),
                _ => unflatten_r(&flatten(c, ctx), ctx),
            })
            .collect();
        with_args(n, kids)
    }
    walk(l, symbol, ctx)
}

/// The default rule list, built once.
pub fn default_rules() -> &'static [Rule] {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(|| build_rules(DEFAULT_RULES))
}

// ---- matching

/// Placeholder bindings in insertion order (a JS object's key order).
type Match = Vec<(String, Node)>;

fn get<'a>(m: &'a Match, k: &str) -> Option<&'a Node> {
    m.iter().find(|(n, _)| n == k).map(|(_, v)| v)
}

fn set(m: &mut Match, k: &str, v: Node) {
    if let Some(e) = m.iter_mut().find(|(n, _)| n == k) {
        e.1 = v;
    } else {
        m.push((k.to_string(), v));
    }
}

/// `_exactMatch(p, q)`.
fn exact_match(p: &Node, q: &Node) -> bool {
    match (p, q) {
        (Node::Constant(a), Node::Constant(b)) => values_equal(a, b),
        (Node::Symbol(a), Node::Symbol(b)) => a == b,
        (Node::Operator { op: a, args: x, .. }, Node::Operator { op: b, args: y, .. }) => {
            a == b && x.len() == y.len() && x.iter().zip(y).all(|(i, j)| exact_match(i, j))
        }
        (Node::Function { args: x, .. }, Node::Function { args: y, .. }) => {
            function_name(p) == function_name(q)
                && x.len() == y.len()
                && x.iter().zip(y).all(|(i, j)| exact_match(i, j))
        }
        _ => false,
    }
}

/// `mergeMatch(match1, match2)`.
fn merge_match(a: &Match, b: &Match) -> Option<Match> {
    let mut out: Match = Vec::new();
    for (k, v) in a {
        out.push((k.clone(), v.clone()));
        if let Some(w) = get(b, k)
            && !exact_match(v, w)
        {
            return None;
        }
    }
    for (k, v) in b {
        set(&mut out, k, v.clone());
    }
    Some(out)
}

/// A key equal for two matches exactly when their JSON is (mathjs dedupes with `JSON.stringify`).
fn match_key(m: &Match) -> String {
    let mut s = String::new();
    for (k, v) in m {
        s.push_str(k);
        s.push('=');
        node_key(v, &mut s);
        s.push(';');
    }
    s
}

fn node_key(n: &Node, s: &mut String) {
    use std::fmt::Write as _;
    match n {
        Node::Constant(Constant::Number { value, .. }) => {
            let _ = write!(s, "C{}", crate::jsnum::to_string(*value));
        }
        Node::Constant(c) => {
            let _ = write!(s, "C{c:?}");
        }
        Node::Symbol(x) => {
            let _ = write!(s, "S{x}");
        }
        Node::Operator {
            op,
            args,
            implicit,
            percentage,
        } => {
            let _ = write!(s, "O{}{}{}(", op.name(), *implicit as u8, *percentage as u8);
            for a in args {
                node_key(a, s);
                s.push(',');
            }
            s.push(')');
        }
        Node::Function { callee, args } => {
            s.push('F');
            node_key(callee, s);
            s.push('(');
            for a in args {
                node_key(a, s);
                s.push(',');
            }
            s.push(')');
        }
        Node::Parenthesis(c) => {
            s.push('P');
            node_key(c, s);
        }
        other => {
            let _ = write!(s, "X{other}");
        }
    }
}

fn combine(l1: &[Match], l2: &[Match]) -> Vec<Match> {
    let mut out = Vec::new();
    for a in l1 {
        for b in l2 {
            if let Some(m) = merge_match(a, b) {
                out.push(m);
            }
        }
    }
    out
}

/// `mergeChildMatches`: the product of the children's matches, deduplicated.
fn merge_child_matches(lists: Vec<Vec<Match>>) -> Vec<Match> {
    let mut it = lists.into_iter();
    let Some(first) = it.next() else {
        return Vec::new();
    };
    let sets = it.fold(first, |acc, l| combine(&acc, &l));
    let mut seen = HashSet::new();
    sets.into_iter()
        .filter(|m| seen.insert(match_key(m)))
        .collect()
}

/// `getSplits`: the binary ways to read an n-ary node.
fn get_splits(n: &Node, ctx: &Ctx) -> Vec<Node> {
    let a = node_args(n).expect("operator");
    let mut out = Vec::new();
    if is_commutative(n, ctx) {
        for i in 0..a.len() {
            let mut rest = a.clone();
            rest.remove(i);
            let right = if rest.len() == 1 {
                rest.pop().expect("one")
            } else {
                make_node(n, rest)
            };
            out.push(make_node(n, vec![a[i].clone(), right]));
        }
    } else {
        for i in 1..a.len() {
            let left = if i > 1 {
                make_node(n, a[..i].to_vec())
            } else {
                a[0].clone()
            };
            let rest = a[i..].to_vec();
            let right = if rest.len() == 1 {
                rest[0].clone()
            } else {
                make_node(n, rest)
            };
            out.push(make_node(n, vec![left, right]));
        }
    }
    out
}

const SUPPORTED_CONSTANTS: &[&str] = &[
    "true", "false", "e", "i", "Infinity", "LN2", "LN10", "LOG2E", "LOG10E", "NaN", "phi", "pi",
    "SQRT1_2", "SQRT2", "tau",
];

/// `isNumericNode`: a constant, or a unary operator on one.
pub(crate) fn is_numeric_node(n: &Node) -> bool {
    match n {
        Node::Constant(_) => true,
        Node::Operator { args, .. } => args.len() == 1 && matches!(args[0], Node::Constant(_)),
        _ => false,
    }
}

/// `isConstantExpression`.
pub(crate) fn is_constant_expression(n: &Node) -> bool {
    match n {
        Node::Constant(_) => true,
        Node::Operator { args, .. } | Node::Function { args, .. } => {
            args.iter().all(is_constant_expression)
        }
        Node::Parenthesis(c) => is_constant_expression(c),
        _ => false,
    }
}

/// The `name` property `_ruleMatch` compares built-in constants by.
fn js_name(n: &Node) -> Option<&str> {
    match n {
        Node::Symbol(s) => Some(s),
        Node::Function { .. } => Some(function_name(n)),
        _ => None,
    }
}

/// `_ruleMatch(rule, node, context, isSplit)`.
fn rule_match(rule: &Node, node: &Node, ctx: &Ctx, is_split: bool) -> Result<Vec<Match>, SymError> {
    let mut res: Vec<Match> = vec![Vec::new()];
    let pair = match (rule, node) {
        (
            Node::Operator {
                op: a, args: ra, ..
            },
            Node::Operator {
                op: b, args: na, ..
            },
        ) => {
            if a != b {
                return Ok(Vec::new());
            }
            Some((ra, na))
        }
        (Node::Function { args: ra, .. }, Node::Function { args: na, .. }) => {
            if function_name(rule) != function_name(node) {
                return Ok(Vec::new());
            }
            Some((ra, na))
        }
        _ => None,
    };
    match (rule, node) {
        _ if pair.is_some() => {
            let (ra, na) = pair.expect("checked");
            if (na.len() == 1 && ra.len() == 1)
                || (!is_associative(node, ctx) && na.len() == ra.len())
                || is_split
            {
                let mut child_matches = Vec::new();
                for (rchild, nchild) in ra.iter().zip(na.iter()) {
                    let m = rule_match(rchild, nchild, ctx, false)?;
                    if m.is_empty() {
                        break;
                    }
                    child_matches.push(m);
                }
                if child_matches.len() != ra.len() {
                    if !is_commutative(node, ctx) || ra.len() == 1 {
                        return Ok(Vec::new());
                    }
                    if ra.len() > 2 {
                        return err(
                            "permuting >2 commutative non-associative rule arguments not yet implemented",
                        );
                    }
                    let left = rule_match(&ra[0], &na[1], ctx, false)?;
                    if left.is_empty() {
                        return Ok(Vec::new());
                    }
                    let right = rule_match(&ra[1], &na[0], ctx, false)?;
                    if right.is_empty() {
                        return Ok(Vec::new());
                    }
                    child_matches = vec![left, right];
                }
                res = merge_child_matches(child_matches);
            } else if na.len() >= 2 && ra.len() == 2 {
                let mut out = Vec::new();
                for split in get_splits(node, ctx) {
                    out.extend(rule_match(rule, &split, ctx, true)?);
                }
                return Ok(out);
            } else if ra.len() > 2 {
                return err("Unexpected non-binary associative function");
            } else {
                return Ok(Vec::new());
            }
        }
        (Node::Symbol(name), _) => {
            if name.is_empty() {
                return err("Symbol in rule has 0 length...!?");
            }
            if SUPPORTED_CONSTANTS.contains(&name.as_str()) {
                if js_name(node) != Some(name.as_str()) {
                    return Ok(Vec::new());
                }
            } else {
                let b = name.as_bytes();
                let kind = if b.len() > 1 && b[1].is_ascii_lowercase() {
                    &name[..2]
                } else {
                    &name[..1]
                };
                let ok = match kind {
                    "n" | "_p" => true,
                    "c" | "cl" => matches!(node, Node::Constant(_)),
                    "v" => !matches!(node, Node::Constant(_)),
                    "vl" => matches!(node, Node::Symbol(_)),
                    "cd" => is_numeric_node(node),
                    "vd" => !is_numeric_node(node),
                    "ce" => is_constant_expression(node),
                    "ve" => !is_constant_expression(node),
                    _ => return err(format!("Invalid symbol in rule: {name}")),
                };
                if !ok {
                    return Ok(Vec::new());
                }
                res[0].push((name.clone(), node.clone()));
            }
        }
        (Node::Constant(rc), _) => match node {
            Node::Constant(nc) if values_equal(rc, nc) => {}
            _ => return Ok(Vec::new()),
        },
        _ => return Ok(Vec::new()),
    }
    Ok(res)
}

/// The right-hand side with placeholders filled (`repl.clone()` + `transform`).
fn substitute(repl: &Node, m: &Match, implicit: bool) -> Node {
    let mut res = repl.clone();
    if implicit && let Node::Operator { implicit: i, .. } = &mut res {
        *i = true;
    }
    super::transform(&res, &mut |n| match n {
        Node::Symbol(s) => get(m, s).cloned(),
        _ => None,
    })
}

/// Applies `rule` to each item; `None` when no item changed.
fn apply_all(items: &[Node], rule: &PatternRule, ctx: &Ctx) -> Result<Option<Vec<Node>>, SymError> {
    let mut out: Option<Vec<Node>> = None;
    for (i, a) in items.iter().enumerate() {
        if let Some(n) = apply_rule(a, rule, ctx)? {
            out.get_or_insert_with(|| items[..i].to_vec()).push(n);
        } else if let Some(v) = out.as_mut() {
            v.push(a.clone());
        }
    }
    Ok(out)
}

/// `applyRule(node, rule, context)`: `None` when mathjs would return the same node (`res === node`).
fn apply_rule(node: &Node, rule: &PatternRule, ctx: &Ctx) -> Result<Option<Node>, SymError> {
    if !rule.enabled {
        return Ok(None);
    }
    let merged = rule.impose.merged(ctx);
    let rebuilt: Option<Node> = match node {
        Node::Operator { args, .. } | Node::Function { args, .. } => {
            apply_all(args, rule, ctx)?.map(|a| with_args(node, a))
        }
        Node::Parenthesis(c) => apply_rule(c, rule, ctx)?.map(|n| Node::Parenthesis(Box::new(n))),
        Node::Array(items) => apply_all(items, rule, ctx)?.map(Node::Array),
        _ => None,
    };
    let res: &Node = rebuilt.as_ref().unwrap_or(node);
    let mut repl = &rule.r;
    let mut matches = rule_match(&rule.l, res, &merged, false)?.into_iter().next();
    if matches.is_none()
        && let Some((el, er)) = &rule.expanded
    {
        repl = er;
        matches = rule_match(el, res, &merged, false)?.into_iter().next();
    }
    if matches.is_none()
        && let Some(((l1, r1), (l2, r2))) = &rule.expanded_nc
    {
        repl = r1;
        matches = rule_match(l1, res, &merged, false)?.into_iter().next();
        if matches.is_none() {
            repl = r2;
            matches = rule_match(l2, res, &merged, false)?.into_iter().next();
        }
    }
    let out = match matches {
        Some(m) => {
            let implicit = matches!(res, Node::Operator { implicit: true, .. });
            Some(substitute(repl, &m, implicit))
        }
        None => rebuilt,
    };
    match out {
        Some(n) if rule.repeat => Ok(Some(apply_rule(&n, rule, ctx)?.unwrap_or(n))),
        other => Ok(other),
    }
}

// ---- resolve and the loop

/// `resolve(expr, emptyScope)`: a copy; operators lose `isPercentage`, calls are rebuilt by name.
fn resolve(n: &Node) -> Node {
    match n {
        Node::Operator {
            op, args, implicit, ..
        } => Node::Operator {
            op: *op,
            args: args.iter().map(resolve).collect(),
            implicit: *implicit,
            percentage: false,
        },
        Node::Parenthesis(c) => Node::Parenthesis(Box::new(resolve(c))),
        Node::Function { args, .. } => Node::Function {
            callee: Box::new(sym(function_name(n))),
            args: args.iter().map(resolve).collect(),
        },
        Node::Constant(_) | Node::Symbol(_) => n.clone(),
        other => super::map_children(other, &mut resolve),
    }
}

/// Passes over the rule list before giving up on an input that never settles (mathjs would loop forever).
const MAX_PASSES: usize = 10_000;

/// `math.simplify(expr)` with mathjs's default rules.
pub fn simplify(expr: &Node) -> Result<Node, SymError> {
    simplify_with(expr, default_rules(), &SimplifyOptions::default())
}

/// `math.simplify(text)`: parses, then simplifies.
pub fn simplify_text(text: &str) -> Result<Node, SymError> {
    simplify(&parse(text)?)
}

/// `math.simplify(expr, rules, {}, options)`.
pub fn simplify_with(
    expr: &Node,
    rules: &[Rule],
    options: &SimplifyOptions,
) -> Result<Node, SymError> {
    let ctx = Ctx::default();
    let mut res = remove_parens(&resolve(expr));
    let mut visited = HashSet::new();
    let mut text = to_string(&res, &PrintOptions::all());
    let mut passes = 0;
    while visited.insert(text) {
        passes += 1;
        if passes > MAX_PASSES {
            return err("simplify did not settle");
        }
        for rule in rules {
            res = match rule {
                Rule::Core => simplify_core(&res),
                Rule::Constant => simplify_constant(&res, options)?,
                Rule::Pattern(p) => {
                    let flat = flatten(&res, &ctx);
                    apply_rule(&flat, p, &ctx)?.unwrap_or(flat)
                }
            };
            res = unflatten_l(&res, &ctx);
        }
        text = to_string(&res, &PrintOptions::all());
    }
    Ok(res)
}

// ---- simplifyCore

/// Every operator mathjs knows, for `getOperator(fnName)`.
const ALL_OPS: &[Op] = &[
    Op::Or,
    Op::Xor,
    Op::And,
    Op::BitOr,
    Op::BitXor,
    Op::BitAnd,
    Op::Equal,
    Op::Unequal,
    Op::Smaller,
    Op::Larger,
    Op::SmallerEq,
    Op::LargerEq,
    Op::LeftShift,
    Op::RightArithShift,
    Op::RightLogShift,
    Op::To,
    Op::Add,
    Op::Subtract,
    Op::Multiply,
    Op::Divide,
    Op::DotMultiply,
    Op::DotDivide,
    Op::Mod,
    Op::UnaryPlus,
    Op::UnaryMinus,
    Op::BitNot,
    Op::Not,
    Op::Pow,
    Op::DotPow,
    Op::Nullish,
    Op::Factorial,
    Op::CTranspose,
];

/// `getOperator(fn)`: the operator a function name stands for (`add` → `+`).
pub fn operator_for(name: &str) -> Option<Op> {
    ALL_OPS.iter().copied().find(|o| o.name() == name)
}

fn is_always_boolean(n: &Node) -> bool {
    matches!(
        n,
        Node::Operator {
            op: Op::And | Op::Not | Op::Or,
            ..
        }
    )
}

fn truthy(c: &Constant) -> bool {
    match c {
        Constant::Number { value, .. } => *value != 0.0 && !value.is_nan(),
        Constant::Str(s) => !s.is_empty(),
        Constant::Bool(b) => *b,
        Constant::Null | Constant::Undefined => false,
    }
}

fn boolean(b: bool) -> Node {
    Node::Constant(Constant::Bool(b))
}

/// mathjs `simplifyCore(node)`: one pass of the cheap identities (`x+0`, `1*x`, `x^1`, `--x`, …), calls with
/// operator names become operators, and a constant moves to the left of a product.
pub fn simplify_core(node: &Node) -> Node {
    let ctx = Ctx::default();
    if has_property(node, Prop::Trivial, &ctx) {
        if let Node::Function { args, .. } = node
            && args.len() == 1
        {
            return simplify_core(&args[0]);
        }
        let kids = super::children(node);
        if kids.len() == 1 {
            return simplify_core(kids[0]);
        }
    }
    let mut node = node.clone();
    if let Node::Function { args, .. } = &node {
        let name = function_name(&node).to_string();
        match operator_for(&name) {
            Some(op) => {
                let mut args = args.clone();
                if args.len() > 2 && has_property(&node, Prop::Associative, &ctx) {
                    while args.len() > 2 {
                        let last = args.pop().expect("arg");
                        let seclast = args.pop().expect("arg");
                        args.push(super::op(op, vec![last, seclast]));
                    }
                }
                node = super::op(op, args);
            }
            None => {
                let Node::Function { callee, args } = &node else {
                    unreachable!()
                };
                return Node::Function {
                    callee: Box::new(simplify_core(callee)),
                    args: args.iter().map(simplify_core).collect(),
                };
            }
        }
    }
    let Node::Operator {
        op, args, implicit, ..
    } = &node
    else {
        return match &node {
            Node::Array(items) => Node::Array(items.iter().map(simplify_core).collect()),
            Node::Accessor { .. } => super::map_children(&node, &mut simplify_core),
            _ => node,
        };
    };
    let (mut op, mut args, implicit) = (*op, args.clone(), *implicit);
    if args.len() == 1 {
        let a0 = simplify_core(&args[0]);
        if op == Op::BitNot
            && let Node::Operator {
                op: Op::BitNot,
                args: inner,
                ..
            } = &a0
            && inner.len() == 1
        {
            return inner[0].clone();
        }
        if op == Op::Not
            && let Node::Operator {
                op: Op::Not,
                args: inner,
                ..
            } = &a0
            && inner.len() == 1
            && is_always_boolean(&inner[0])
        {
            return inner[0].clone();
        }
        let mut finish = true;
        if op.symbol() == "-"
            && let Node::Operator {
                op: inner_op,
                args: inner,
                ..
            } = &a0
        {
            if inner.len() == 2 && *inner_op == Op::Subtract {
                op = Op::Subtract;
                args = vec![inner[1].clone(), inner[0].clone()];
                finish = false;
            }
            if inner.len() == 1 && inner_op.symbol() == "-" {
                return inner[0].clone();
            }
        }
        if finish {
            return super::op(op, vec![a0]);
        }
    }
    if args.len() == 2 {
        let a = simplify_core(&args[0]);
        let mut a1 = simplify_core(&args[1]);
        if op.symbol() == "+" {
            if is_zero_node(&a) {
                return a1;
            }
            if is_zero_node(&a1) {
                return a;
            }
            if let Node::Operator {
                op: inner_op,
                args: inner,
                ..
            } = &a1
                && inner.len() == 1
                && inner_op.symbol() == "-"
            {
                a1 = inner[0].clone();
                op = Op::Subtract;
            }
        }
        if op.symbol() == "-" {
            if let Node::Operator {
                op: inner_op,
                args: inner,
                ..
            } = &a1
                && inner.len() == 1
                && inner_op.symbol() == "-"
            {
                return simplify_core(&super::op(Op::Add, vec![a, inner[0].clone()]));
            }
            if is_zero_node(&a) {
                return simplify_core(&super::op(Op::UnaryMinus, vec![a1]));
            }
            if is_zero_node(&a1) {
                return a;
            }
            return super::op(op, vec![a, a1]);
        }
        if op.symbol() == "*" {
            if matches!(a, Node::Constant(_)) {
                if is_zero_node(&a) {
                    return super::num(0.0);
                } else if is_one_node(&a) {
                    return a1;
                }
            }
            if matches!(a1, Node::Constant(_)) {
                if is_zero_node(&a1) {
                    return super::num(0.0);
                } else if is_one_node(&a1) {
                    return a;
                }
                if ctx.lookup(op.name(), Prop::Commutative) {
                    return super::op_with(op, vec![a1, a], implicit);
                }
            }
            return super::op_with(op, vec![a, a1], implicit);
        }
        if op.symbol() == "/" {
            if is_zero_node(&a) {
                return super::num(0.0);
            }
            if is_one_node(&a1) {
                return a;
            }
            return super::op(op, vec![a, a1]);
        }
        if op.symbol() == "^" && matches!(a1, Node::Constant(_)) {
            if is_zero_node(&a1) {
                return super::num(1.0);
            } else if is_one_node(&a1) {
                return a;
            }
        }
        if op == Op::And {
            if let Node::Constant(c) = &a {
                if truthy(c) {
                    if is_always_boolean(&a1) {
                        return a1;
                    }
                    if let Node::Constant(c1) = &a1 {
                        return boolean(truthy(c1));
                    }
                } else {
                    return boolean(false);
                }
            }
            if let Node::Constant(c1) = &a1 {
                if truthy(c1) {
                    if is_always_boolean(&a) {
                        return a;
                    }
                } else {
                    return boolean(false);
                }
            }
        }
        if op == Op::Or {
            if let Node::Constant(c) = &a {
                if truthy(c) {
                    return boolean(true);
                } else if is_always_boolean(&a1) {
                    return a1;
                }
            }
            if let Node::Constant(c1) = &a1 {
                if truthy(c1) {
                    return boolean(true);
                } else if is_always_boolean(&a) {
                    return a;
                }
            }
        }
        return super::op(op, vec![a, a1]);
    }
    super::op(op, args.iter().map(simplify_core).collect())
}
