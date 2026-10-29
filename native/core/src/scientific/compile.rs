//! `compileScientific`, `compileScientific2` and `parseScientific` (`src/engine/scientific.ts`): an expression in
//! one or two free variables, prepared once and evaluated at many points (sums, graphs).
//!
//! The TS evaluates the compiled code outside `evalScientific`, so its module-level `complexTyped` flag is off:
//! `sqrt(-1)` is NaN in a compiled term even when an `i` was typed. The scope here keeps that.

use super::{ScientificContext, finish, preprocess_checked, scope_for};
use crate::eval::{self, Scope};
use crate::syntax::{Node, parse};
use crate::types::Value;

/// The prepared text, its tree and the scope it evaluates in.
#[derive(Debug)]
pub struct Parsed {
    pub expr: String,
    pub node: Node,
    pub scope: Scope,
}

fn prepare(text: &str, ctx: &ScientificContext, vars: &[&str]) -> Option<Parsed> {
    let mut c = ctx.clone();
    for v in vars {
        c.variables.insert((*v).to_string(), 0.0);
    }
    let expr = preprocess_checked(text, &c)?;
    let node = parse(&expr).ok()?;
    let mut scope = scope_for(&expr, &c);
    scope.complex_typed = false;
    Some(Parsed { expr, node, scope })
}

/// `parseScientific`: the tree `compile_scientific` evaluates, with its scope, for walking its shape.
pub fn parse_scientific(text: &str, ctx: &ScientificContext, variable: &str) -> Option<Parsed> {
    prepare(text, ctx, &[variable])
}

/// A compiled expression in one or two variables.
#[derive(Debug)]
pub struct Compiled {
    p: Parsed,
    vars: Vec<String>,
    cost: u64,
}

impl Compiled {
    fn new(p: Parsed, vars: &[&str]) -> Self {
        let cost = 5 + p.expr.encode_utf16().count() as u64;
        Compiled {
            p,
            vars: vars.iter().map(|v| (*v).to_string()).collect(),
            cost,
        }
    }

    /// The expression as preprocessed.
    pub fn expr(&self) -> &str {
        &self.p.expr
    }

    fn run(&mut self, values: &[f64]) -> Option<Value> {
        for (name, v) in self.vars.iter().zip(values) {
            self.p.scope.variables.insert(name.clone(), *v);
        }
        crate::work::spend(self.cost);
        let v = eval::evaluate(&self.p.node, &self.p.scope).ok()?;
        finish(&v, &self.p.expr)
    }

    /// The value at `x` (one variable).
    pub fn at(&mut self, x: f64) -> Option<Value> {
        self.run(&[x])
    }

    /// The value at `(u, v)` (two variables).
    pub fn at2(&mut self, u: f64, v: f64) -> Option<Value> {
        self.run(&[u, v])
    }
}

/// `compileScientific`: None when the text doesn't prepare or parse.
pub fn compile_scientific(text: &str, ctx: &ScientificContext, variable: &str) -> Option<Compiled> {
    prepare(text, ctx, &[variable]).map(|p| Compiled::new(p, &[variable]))
}

/// `compileScientific2`, for a contour plot's f(x, y).
pub fn compile_scientific2(
    text: &str,
    ctx: &ScientificContext,
    a: &str,
    b: &str,
) -> Option<Compiled> {
    prepare(text, ctx, &[a, b]).map(|p| Compiled::new(p, &[a, b]))
}
