//! mathjs `rationalize(expr)` (`function/algebra/rationalize.js`): a rational expression as one fraction of
//! expanded polynomials, through its own four rule sets, and in one variable written in canonical order
//! (`polyToCanonical`). Throws (as mathjs does) on a call that survives simplification, a non-integer
//! exponent, or an operator outside `+ - * / ^`.
//!
//! Quirk kept: `polynomial()` simplifies with `simplifyConstant` *before* it checks for calls, so a constant call
//! (`sqrt(2) * x`) is folded into a decimal instead of rejected.

use std::sync::OnceLock;

use super::print::{PrintOptions, to_string};
use super::simplify::{Rule, SimplifyOptions, Spec, build_rules, rule, simplify_with};
use super::{Node, Op, SymError, err, num, op, sym};

struct RuleSets {
    first: Vec<Rule>,
    distr_div: Vec<Rule>,
    suc_div: Vec<Rule>,
    first_again: Vec<Rule>,
    finals: Vec<Rule>,
}

const OLD_RULES: &[Spec] = &[
    Spec::Core,
    rule("n+n", "2*n"),
    rule("n+-n", "0"),
    Spec::Constant,
    rule("n*(n1^-1)", "n/n1"),
    rule("n*n1^-n2", "n/n1^n2"),
    rule("n1^-1", "1/n1"),
    rule("n*(n1/n2)", "(n*n1)/n2"),
    rule("1*n", "n"),
];

const RULES_FIRST: &[Spec] = &[
    rule("(-n1)/(-n2)", "n1/n2"),
    rule("(-n1)*(-n2)", "n1*n2"),
    rule("n1--n2", "n1+n2"),
    rule("n1-n2", "n1+(-n2)"),
    rule("(n1+n2)*n3", "(n1*n3 + n2*n3)"),
    rule("n1*(n2+n3)", "(n1*n2+n1*n3)"),
    rule("c1*n + c2*n", "(c1+c2)*n"),
    rule("c1*n + n", "(c1+1)*n"),
    rule("c1*n - c2*n", "(c1-c2)*n"),
    rule("c1*n - n", "(c1-1)*n"),
    rule("v/c", "(1/c)*v"),
    rule("v/-c", "-(1/c)*v"),
    rule("-v*-c", "c*v"),
    rule("-v*c", "-c*v"),
    rule("v*-c", "-c*v"),
    rule("v*c", "c*v"),
    rule("-(-n1*n2)", "(n1*n2)"),
    rule("-(n1*n2)", "(-n1*n2)"),
    rule("-(-n1+n2)", "(n1-n2)"),
    rule("-(n1+n2)", "(-n1-n2)"),
    rule("(n1^n2)^n3", "(n1^(n2*n3))"),
    rule("-(-n1/n2)", "(n1/n2)"),
    rule("-(n1/n2)", "(-n1/n2)"),
];

const RULES_DISTR_DIV: &[Spec] = &[
    rule("(n1/n2 + n3/n4)", "((n1*n4 + n3*n2)/(n2*n4))"),
    rule("(n1/n2 + n3)", "((n1 + n3*n2)/n2)"),
    rule("(n1 + n2/n3)", "((n1*n3 + n2)/n3)"),
];

const RULES_SUC_DIV: &[Spec] = &[
    rule("(n1/(n2/n3))", "((n1*n3)/n2)"),
    rule("(n1/n2/n3)", "(n1/(n2*n3))"),
];

const FINAL_RULES: &[Spec] = &[
    Spec::Core,
    rule("n*-n", "-n^2"),
    rule("n*n", "n^2"),
    Spec::Constant,
    rule("n*-n^n1", "-n^(n1+1)"),
    rule("n*n^n1", "n^(n1+1)"),
    rule("n^n1*-n^n2", "-n^(n1+n2)"),
    rule("n^n1*n^n2", "n^(n1+n2)"),
    rule("n^n1*-n", "-n^(n1+1)"),
    rule("n^n1*n", "n^(n1+1)"),
    rule("n^n1/-n", "-n^(n1-1)"),
    rule("n^n1/n", "n^(n1-1)"),
    rule("n/-n^n1", "-n^(1-n1)"),
    rule("n/n^n1", "n^(1-n1)"),
    rule("n^n1/-n^n2", "n^(n1-n2)"),
    rule("n^n1/n^n2", "n^(n1-n2)"),
    rule("n1+(-n2*n3)", "n1-n2*n3"),
    rule("v*(-c)", "-c*v"),
    rule("n1+-n2", "n1-n2"),
    rule("v*c", "c*v"),
    rule("(n1^n2)^n3", "(n1^(n2*n3))"),
];

fn rule_sets() -> &'static RuleSets {
    static SETS: OnceLock<RuleSets> = OnceLock::new();
    SETS.get_or_init(|| {
        let cat = |a: &[Spec], b: &[Spec], c: &[Spec]| -> Vec<Rule> {
            let all: Vec<Spec> = a.iter().chain(b).chain(c).copied().collect();
            build_rules(&all)
        };
        RuleSets {
            first: cat(OLD_RULES, RULES_FIRST, RULES_SUC_DIV),
            distr_div: build_rules(RULES_DISTR_DIV),
            suc_div: build_rules(RULES_SUC_DIV),
            first_again: cat(OLD_RULES, RULES_FIRST, &[]),
            finals: build_rules(FINAL_RULES),
        }
    })
}

const NO_EXACT: SimplifyOptions = SimplifyOptions {
    exact_fractions: Some(false),
    fractions_limit: None,
};
const EXACT: SimplifyOptions = SimplifyOptions {
    exact_fractions: Some(true),
    fractions_limit: None,
};

/// `isInteger(parseFloat(value))` for a constant node.
fn integer_constant(n: &Node) -> Option<f64> {
    let v = super::number_value(n)?;
    (v.is_finite() && v.round() == v).then_some(v)
}

/// `polynomial(expr, {}, true, firstRules)`: simplifies, then checks the result is a rational expression and
/// lists its variables.
fn polynomial(expr: &Node, rules: &[Rule]) -> Result<(Node, Vec<String>), SymError> {
    let node = simplify_with(expr, rules, &NO_EXACT)?;
    let mut vars = Vec::new();
    fn rec(n: &Node, vars: &mut Vec<String>) -> Result<(), SymError> {
        match n {
            Node::Function { .. } => err("There is an unsolved function call"),
            Node::Operator { op: o, args, .. } => {
                if *o == Op::Pow {
                    if !matches!(args[1], Node::Constant(_)) || integer_constant(&args[1]).is_none()
                    {
                        return err("There is a non-integer exponent");
                    }
                    rec(&args[0], vars)
                } else {
                    if !"+-*/".contains(o.symbol()) {
                        return err(format!(
                            "Operator {} invalid in polynomial expression",
                            o.symbol()
                        ));
                    }
                    args.iter().try_for_each(|a| rec(a, vars))
                }
            }
            Node::Symbol(s) => {
                if !vars.contains(s) {
                    vars.push(s.clone());
                }
                Ok(())
            }
            Node::Parenthesis(c) => rec(c, vars),
            Node::Constant(_) => Ok(()),
            _ => err("type is not allowed in polynomial expression"),
        }
    }
    rec(&node, &mut vars)?;
    Ok((node, vars))
}

/// `expandPower`: `(a+b)^3` → `(a+b) * (a+b)^2` → … so the distributive rules can reach it.
fn expand_power(n: &Node) -> Node {
    let mut node = n.clone();
    if let Node::Operator {
        op: Op::Pow, args, ..
    } = n
        && args.len() == 2
        && matches!(args[0], Node::Parenthesis(_) | Node::Operator { .. })
        && let Some(val) = super::number_value(&args[1])
        && val >= 2.0
        && crate::jsnum::is_integer(val)
    {
        node = if val > 2.0 {
            op(
                Op::Multiply,
                vec![
                    args[0].clone(),
                    op(Op::Pow, vec![args[0].clone(), num(val - 1.0)]),
                ],
            )
        } else {
            op(Op::Multiply, vec![args[0].clone(), args[0].clone()])
        };
    }
    match n {
        Node::Parenthesis(c) => Node::Parenthesis(Box::new(expand_power(c))),
        Node::Constant(_) | Node::Symbol(_) => node,
        _ => match &node {
            Node::Operator {
                op: o,
                args,
                implicit,
                percentage,
            } => Node::Operator {
                op: *o,
                args: args.iter().map(expand_power).collect(),
                implicit: *implicit,
                percentage: *percentage,
            },
            Node::Function { callee, args } => Node::Function {
                callee: callee.clone(),
                args: args.iter().map(expand_power).collect(),
            },
            other => other.clone(),
        },
    }
}

/// `math.rationalize(expr)`.
pub fn rationalize(expr: &Node) -> Result<Node, SymError> {
    let sets = rule_sets();
    let (mut expr, vars) = polynomial(expr, &sets.first)?;
    if !vars.is_empty() {
        expr = expand_power(&expr);
        expr = simplify_with(&expr, &sets.first, &NO_EXACT)?;
        let mut distr = true;
        let mut redo = false;
        let mut before: Option<String> = None;
        loop {
            let rules = if distr {
                &sets.distr_div
            } else {
                &sets.suc_div
            };
            expr = simplify_with(&expr, rules, &EXACT)?;
            distr = !distr;
            let s = to_string(&expr, &PrintOptions::default());
            if before.as_deref() == Some(s.as_str()) {
                break;
            }
            redo = true;
            before = Some(s);
        }
        if redo {
            expr = simplify_with(&expr, &sets.first_again, &NO_EXACT)?;
        }
        expr = simplify_with(&expr, &sets.finals, &NO_EXACT)?;
    }
    if let Node::Operator {
        op: Op::Divide,
        args,
        ..
    } = &mut expr
        && args.len() == 2
    {
        if vars.len() == 1 {
            args[0] = poly_to_canonical(&args[0])?;
            args[1] = poly_to_canonical(&args[1])?;
        }
    } else if vars.len() == 1 {
        expr = poly_to_canonical(&expr)?;
    }
    Ok(expr)
}

/// The walk state of `polyToCanonical` (`o`, `maxExpo`, `varname`, the coefficients).
struct Canon {
    cte: f64,
    oper: &'static str,
    fire: &'static str,
    no_fil: Option<usize>,
    max_expo: usize,
    varname: String,
    coefficients: Vec<Option<f64>>,
}

impl Canon {
    fn coef(&mut self, i: usize) -> &mut Option<f64> {
        if self.coefficients.len() <= i {
            self.coefficients.resize(i + 1, None);
        }
        &mut self.coefficients[i]
    }

    fn add_to(&mut self, i: usize, v: f64) {
        let c = self.coef(i);
        *c = Some(c.unwrap_or(f64::NAN) + v);
    }

    fn sign(&self) -> f64 {
        if self.oper == "+" { 1.0 } else { -1.0 }
    }

    fn rec(&mut self, node: &Node, parent: Option<&Node>) -> Result<(), SymError> {
        match node {
            Node::Function { .. } => err("There is an unsolved function call"),
            Node::Operator { op: o, args, .. } => {
                let symbol = o.symbol();
                if !"+-*^".contains(symbol) {
                    return err(format!("Operator {symbol} invalid"));
                }
                if let Some(Node::Operator { op: p, .. }) = parent {
                    let p = *p;
                    let in_sum = matches!(p, Op::Add | Op::Subtract);
                    if matches!(o, Op::UnaryMinus | Op::Pow) && !in_sum && p != Op::Multiply {
                        return err(format!("Invalid {symbol} placing"));
                    }
                    if matches!(o, Op::Subtract | Op::Add | Op::Multiply) && !in_sum {
                        return err(format!("Invalid {symbol} placing"));
                    }
                    if matches!(o, Op::Subtract | Op::Add | Op::UnaryMinus)
                        && self.no_fil != Some(0)
                    {
                        return err(format!("Invalid {symbol} placing"));
                    }
                }
                if symbol == "^" || symbol == "*" {
                    self.fire = if symbol == "^" { "^" } else { "*" };
                }
                for (i, a) in args.iter().enumerate() {
                    if *o == Op::UnaryMinus {
                        self.oper = "-";
                    }
                    if symbol == "+" || *o == Op::Subtract {
                        self.fire = "";
                        self.cte = 1.0;
                        self.oper = if i == 0 || symbol == "+" { "+" } else { "-" };
                    }
                    self.no_fil = Some(i);
                    self.rec(a, Some(node))?;
                }
                Ok(())
            }
            Node::Symbol(name) => {
                if *name != self.varname && !self.varname.is_empty() {
                    return err("There is more than one variable");
                }
                self.varname = name.clone();
                let Some(parent) = parent else {
                    *self.coef(1) = Some(1.0);
                    return Ok(());
                };
                let psym = match parent {
                    Node::Operator { op, .. } => op.symbol(),
                    _ => "",
                };
                if psym == "^" && self.no_fil != Some(0) {
                    return err("In power the variable should be the first parameter");
                }
                if psym == "*" && self.no_fil != Some(1) {
                    return err("In multiply the variable should be the second parameter");
                }
                if self.fire.is_empty() || self.fire == "*" {
                    if self.max_expo < 1 {
                        *self.coef(1) = Some(0.0);
                    }
                    let v = self.cte * self.sign();
                    self.add_to(1, v);
                    self.max_expo = self.max_expo.max(1);
                }
                Ok(())
            }
            Node::Constant(_) => {
                let valor = super::number_value(node).unwrap_or(f64::NAN);
                let Some(parent) = parent else {
                    *self.coef(0) = Some(valor);
                    return Ok(());
                };
                if matches!(parent, Node::Operator { op: Op::Pow, .. }) {
                    if self.no_fil != Some(1) {
                        return err("Constant cannot be powered");
                    }
                    if !crate::jsnum::is_integer(valor) || valor <= 0.0 {
                        return err("Non-integer exponent is not allowed");
                    }
                    let v = valor as usize;
                    for i in self.max_expo + 1..v {
                        *self.coef(i) = Some(0.0);
                    }
                    if v > self.max_expo {
                        *self.coef(v) = Some(0.0);
                    }
                    let add = self.cte * self.sign();
                    self.add_to(v, add);
                    self.max_expo = self.max_expo.max(v);
                    return Ok(());
                }
                self.cte = valor;
                if self.fire.is_empty() {
                    let add = self.cte * self.sign();
                    self.add_to(0, add);
                }
                Ok(())
            }
            _ => err("Type is not allowed"),
        }
    }
}

/// `polyToCanonical(node)`: a one-variable polynomial in descending powers (`3 x ^ 2 - x + 1`).
fn poly_to_canonical(node: &Node) -> Result<Node, SymError> {
    let mut c = Canon {
        cte: 1.0,
        oper: "+",
        fire: "",
        no_fil: None,
        max_expo: 0,
        varname: String::new(),
        coefficients: vec![Some(0.0)],
    };
    c.rec(node, None)?;
    let max = c.coefficients.len() - 1;
    let mut out: Option<Node> = None;
    for i in (0..=max).rev() {
        // a hole (never assigned) reads as undefined in JS; treat it as a zero coefficient
        let Some(k) = c.coefficients[i] else {
            continue;
        };
        if k == 0.0 {
            continue;
        }
        let first = out.is_none();
        let mut n1 = num(if first { k } else { k.abs() });
        let o = if k < 0.0 { Op::Subtract } else { Op::Add };
        if i > 0 {
            let mut n2 = sym(&c.varname);
            if i > 1 {
                n2 = op(Op::Pow, vec![n2, num(i as f64)]);
            }
            n1 = if k == -1.0 && first {
                op(Op::UnaryMinus, vec![n2])
            } else if k.abs() == 1.0 {
                n2
            } else {
                op(Op::Multiply, vec![n1, n2])
            };
        }
        out = Some(match out {
            None => n1,
            Some(prev) => op(o, vec![prev, n1]),
        });
    }
    Ok(out.unwrap_or_else(|| num(0.0)))
}
