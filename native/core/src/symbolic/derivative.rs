//! mathjs `derivative(expr, variable, {simplify})` (`function/algebra/derivative.js`): the chain, product,
//! quotient and power rules exactly as mathjs writes them, unsimplified unless asked.
//!
//! Quirks kept: other symbols are constants (partial derivatives); `d/dx c^f` is `c^f * (log(c) * f')` with the
//! original node reused; the product rule writes `f' * g + f * g'` with every factor in place (n-ary products
//! too); `0^f` and `1^f` differentiate to 0; a constant factor is pulled out of a product first.
//! mathjs's product rule tells the differentiated factor by object identity; this port tells it by position
//! (the same unless one node object sits in a product twice).

use super::simplify::{is_one_node, is_zero_node};
use super::{Node, Op, SymError, call, err, function_name, num, op, simplify};

/// `math.derivative(expr, variable, {simplify})`.
pub fn derivative(expr: &Node, variable: &str, simplify_result: bool) -> Result<Node, SymError> {
    let d = Deriv { var: variable };
    let res = d.derive(expr)?;
    if simplify_result {
        simplify(&res)
    } else {
        Ok(res)
    }
}

/// `derivative(expr, 'x')` with the variable as text (`parseIdentifier`).
pub fn derivative_text(
    expr: &Node,
    variable: &str,
    simplify_result: bool,
) -> Result<Node, SymError> {
    match super::parse(variable)? {
        Node::Symbol(name) => derivative(expr, &name, simplify_result),
        _ => err(format!(
            "Invalid variable. Cannot parse {variable:?} into a variable in function derivative"
        )),
    }
}

struct Deriv<'a> {
    var: &'a str,
}

fn pow(a: Node, b: Node) -> Node {
    op(Op::Pow, vec![a, b])
}

fn mul(a: Node, b: Node) -> Node {
    op(Op::Multiply, vec![a, b])
}

fn sub(a: Node, b: Node) -> Node {
    op(Op::Subtract, vec![a, b])
}

fn add(a: Node, b: Node) -> Node {
    op(Op::Add, vec![a, b])
}

fn div(a: Node, b: Node) -> Node {
    op(Op::Divide, vec![a, b])
}

fn neg(a: Node) -> Node {
    op(Op::UnaryMinus, vec![a])
}

impl Deriv<'_> {
    /// `_isConst(node, varName)`.
    fn is_const(&self, n: &Node) -> Result<bool, SymError> {
        Ok(match n {
            Node::Constant(_) => true,
            Node::Symbol(s) => s != self.var,
            Node::Parenthesis(c) => self.is_const(c)?,
            Node::FunctionAssignment { params, body, .. } => {
                !params.iter().any(|p| p == self.var) || self.is_const(body)?
            }
            Node::Function { args, .. } | Node::Operator { args, .. } => {
                for a in args {
                    if !self.is_const(a)? {
                        return Ok(false);
                    }
                }
                true
            }
            _ => return err("Unexpected type of argument in function _isConst"),
        })
    }

    fn derive(&self, n: &Node) -> Result<Node, SymError> {
        match n {
            Node::Constant(_) => Ok(num(0.0)),
            Node::Symbol(_) => Ok(num(if self.is_const(n)? { 0.0 } else { 1.0 })),
            Node::Parenthesis(c) => Ok(Node::Parenthesis(Box::new(self.derive(c)?))),
            Node::FunctionAssignment { body, .. } => {
                if self.is_const(n)? {
                    Ok(num(0.0))
                } else {
                    self.derive(body)
                }
            }
            Node::Function { args, .. } => self.function(n, args),
            Node::Operator { op: o, args, .. } => self.operator(n, *o, args),
            _ => err("Unexpected type of argument in function _derivative"),
        }
    }

    fn function(&self, node: &Node, args: &[Node]) -> Result<Node, SymError> {
        if self.is_const(node)? {
            return Ok(num(0.0));
        }
        let name = function_name(node);
        let arg0 = args.first().cloned().unwrap_or(num(0.0));
        let sq = |a: &Node| pow(a.clone(), num(2.0));
        let mut div_out = false;
        let mut negative = false;
        let func: Node = match name {
            "cbrt" => {
                div_out = true;
                mul(num(3.0), pow(arg0.clone(), div(num(2.0), num(3.0))))
            }
            "sqrt" | "nthRoot" if args.len() == 1 => {
                div_out = true;
                mul(num(2.0), call("sqrt", vec![arg0.clone()]))
            }
            "sqrt" | "nthRoot" if args.len() == 2 => {
                let e = div(num(1.0), args[1].clone());
                return self.derive(&pow(arg0, e));
            }
            "log10" | "log" => {
                let arg1 = (name == "log10").then(|| num(10.0));
                if arg1.is_none() && args.len() == 1 {
                    div_out = true;
                    arg0.clone()
                } else if (args.len() == 1 && arg1.is_some())
                    || (args.len() == 2 && self.is_const(&args[1])?)
                {
                    div_out = true;
                    let base = arg1.unwrap_or_else(|| args[1].clone());
                    mul(arg0.clone(), call("log", vec![base]))
                } else if args.len() == 2 {
                    return self.derive(&div(
                        call("log", vec![arg0]),
                        call("log", vec![args[1].clone()]),
                    ));
                } else {
                    return unsupported(name);
                }
            }
            "pow" if args.len() == 2 => return self.derive(&pow(arg0, args[1].clone())),
            "exp" => call("exp", vec![arg0.clone()]),
            "sin" => call("cos", vec![arg0.clone()]),
            "cos" => neg(call("sin", vec![arg0.clone()])),
            "tan" => pow(call("sec", vec![arg0.clone()]), num(2.0)),
            "sec" => mul(node.clone(), call("tan", vec![arg0.clone()])),
            "csc" => {
                negative = true;
                mul(node.clone(), call("cot", vec![arg0.clone()]))
            }
            "cot" => {
                negative = true;
                pow(call("csc", vec![arg0.clone()]), num(2.0))
            }
            "asin" => {
                div_out = true;
                call("sqrt", vec![sub(num(1.0), sq(&arg0))])
            }
            "acos" => {
                div_out = true;
                negative = true;
                call("sqrt", vec![sub(num(1.0), sq(&arg0))])
            }
            "atan" => {
                div_out = true;
                add(sq(&arg0), num(1.0))
            }
            "asec" => {
                div_out = true;
                mul(
                    call("abs", vec![arg0.clone()]),
                    call("sqrt", vec![sub(sq(&arg0), num(1.0))]),
                )
            }
            "acsc" => {
                div_out = true;
                negative = true;
                mul(
                    call("abs", vec![arg0.clone()]),
                    call("sqrt", vec![sub(sq(&arg0), num(1.0))]),
                )
            }
            "acot" => {
                div_out = true;
                negative = true;
                add(sq(&arg0), num(1.0))
            }
            "sinh" => call("cosh", vec![arg0.clone()]),
            "cosh" => call("sinh", vec![arg0.clone()]),
            "tanh" => pow(call("sech", vec![arg0.clone()]), num(2.0)),
            "sech" => {
                negative = true;
                mul(node.clone(), call("tanh", vec![arg0.clone()]))
            }
            "csch" => {
                negative = true;
                mul(node.clone(), call("coth", vec![arg0.clone()]))
            }
            "coth" => {
                negative = true;
                pow(call("csch", vec![arg0.clone()]), num(2.0))
            }
            "asinh" => {
                div_out = true;
                call("sqrt", vec![add(sq(&arg0), num(1.0))])
            }
            "acosh" => {
                div_out = true;
                call("sqrt", vec![sub(sq(&arg0), num(1.0))])
            }
            "atanh" => {
                div_out = true;
                sub(num(1.0), sq(&arg0))
            }
            "asech" => {
                div_out = true;
                negative = true;
                mul(arg0.clone(), call("sqrt", vec![sub(num(1.0), sq(&arg0))]))
            }
            "acsch" => {
                div_out = true;
                negative = true;
                mul(
                    call("abs", vec![arg0.clone()]),
                    call("sqrt", vec![add(sq(&arg0), num(1.0))]),
                )
            }
            "acoth" => {
                div_out = true;
                negative = true;
                sub(num(1.0), sq(&arg0))
            }
            "abs" => div(call("abs", vec![arg0.clone()]), arg0.clone()),
            _ => return unsupported(name),
        };
        let mut chain = self.derive(&arg0)?;
        if negative {
            chain = neg(chain);
        }
        Ok(op(
            if div_out { Op::Divide } else { Op::Multiply },
            vec![chain, func],
        ))
    }

    fn operator(&self, node: &Node, o: Op, args: &[Node]) -> Result<Node, SymError> {
        if self.is_const(node)? {
            return Ok(num(0.0));
        }
        let symbol = o.symbol();
        if symbol == "+" {
            let d = args
                .iter()
                .map(|a| self.derive(a))
                .collect::<Result<_, _>>()?;
            return Ok(op(o, d));
        }
        if symbol == "-" {
            if args.len() == 1 {
                return Ok(op(o, vec![self.derive(&args[0])?]));
            }
            if args.len() == 2 {
                return Ok(op(o, vec![self.derive(&args[0])?, self.derive(&args[1])?]));
            }
        }
        if symbol == "*" {
            let mut constant = Vec::new();
            let mut varying = Vec::new();
            for a in args {
                if self.is_const(a)? {
                    constant.push(a.clone());
                } else {
                    varying.push(a.clone());
                }
            }
            if !constant.is_empty() {
                let rest = if varying.len() == 1 {
                    varying.pop().expect("one")
                } else {
                    op(Op::Multiply, varying)
                };
                constant.push(self.derive(&rest)?);
                return Ok(op(Op::Multiply, constant));
            }
            let mut terms = Vec::with_capacity(args.len());
            for i in 0..args.len() {
                let mut factors = Vec::with_capacity(args.len());
                for (j, a) in args.iter().enumerate() {
                    factors.push(if i == j { self.derive(a)? } else { a.clone() });
                }
                terms.push(op(Op::Multiply, factors));
            }
            return Ok(op(Op::Add, terms));
        }
        if symbol == "/" && args.len() == 2 {
            let (a0, a1) = (&args[0], &args[1]);
            if self.is_const(a1)? {
                return Ok(div(self.derive(a0)?, a1.clone()));
            }
            if self.is_const(a0)? {
                return Ok(mul(
                    neg(a0.clone()),
                    div(self.derive(a1)?, pow(a1.clone(), num(2.0))),
                ));
            }
            return Ok(div(
                sub(
                    mul(self.derive(a0)?, a1.clone()),
                    mul(a0.clone(), self.derive(a1)?),
                ),
                pow(a1.clone(), num(2.0)),
            ));
        }
        if symbol == "^" && args.len() == 2 {
            let (base, exp) = (&args[0], &args[1]);
            if self.is_const(base)? {
                if matches!(base, Node::Constant(_)) && (is_zero_node(base) || is_one_node(base)) {
                    return Ok(num(0.0));
                }
                return Ok(mul(
                    node.clone(),
                    mul(call("log", vec![base.clone()]), self.derive(exp)?),
                ));
            }
            if self.is_const(exp)? {
                if matches!(exp, Node::Constant(_)) {
                    if is_zero_node(exp) {
                        return Ok(num(0.0));
                    }
                    if is_one_node(exp) {
                        return self.derive(base);
                    }
                }
                let lowered = pow(base.clone(), sub(exp.clone(), num(1.0)));
                return Ok(mul(exp.clone(), mul(self.derive(base)?, lowered)));
            }
            return Ok(mul(
                pow(base.clone(), exp.clone()),
                add(
                    mul(self.derive(base)?, div(exp.clone(), base.clone())),
                    mul(self.derive(exp)?, call("log", vec![base.clone()])),
                ),
            ));
        }
        err(format!(
            "Cannot process operator \"{symbol}\" in derivative: the operator is not supported, undefined, or the number of arguments passed to it are not supported"
        ))
    }
}

fn unsupported<T>(name: &str) -> Result<T, SymError> {
    err(format!(
        "Cannot process function \"{name}\" in derivative: the function is not supported, undefined, or the number of arguments passed to it are not supported"
    ))
}
