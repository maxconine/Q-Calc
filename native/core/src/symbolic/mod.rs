//! mathjs's symbolic layer (WP-SYMBOLIC): `simplify`, `derivative`, `rationalize` and `Node.toString()`, ported
//! from mathjs 15.2 (`function/algebra/*`, `expression/node/*`, `expression/operators.js`) over [`syntax::Node`],
//! the same tree the parser builds. The engine's readers (`calculus.ts`, `isolate.ts`, `system.ts`) call these
//! through the TS `math` object; their ports call them here.
//!
//! The tree mirrors mathjs's nodes: [`Node::Constant`] is a `ConstantNode` (only `value` matters; `text` is the
//! JS `String(value)` for nodes built here), [`Node::Symbol`] a `SymbolNode`, [`Node::Operator`] an
//! `OperatorNode` (`op` is its `fn`; the symbol is [`Op::symbol`]), [`Node::Function`] a `FunctionNode` and
//! [`Node::Parenthesis`] a `ParenthesisNode`.
//!
//! Every entry point keeps mathjs's quirks, including ones that look like bugs (see each module). Where mathjs
//! relies on object identity (derivative's product rule compares args with `===`), the port compares positions,
//! which is the same except for a tree that holds one node object twice as siblings; the golden recorder marks
//! such trees (`"same": true`) and none occurs in the fixtures.
//!
//! ```
//! use qcalc_core::symbolic::{self, PrintOptions};
//! let n = symbolic::parse("2x + 3x").unwrap();
//! let s = symbolic::simplify(&n).unwrap();
//! assert_eq!(symbolic::to_string(&s, &PrintOptions::default()), "5 * x");
//! let d = symbolic::derivative(&symbolic::parse("x^2").unwrap(), "x", true).unwrap();
//! assert_eq!(symbolic::to_string(&d, &PrintOptions::default()), "2 * x");
//! ```

pub mod constant;
pub mod derivative;
pub mod fraction;
pub mod json;
pub mod print;
pub mod rationalize;
pub mod simplify;

pub use crate::syntax::{Constant, Node, Op};
pub use constant::simplify_constant;
pub use derivative::derivative;
pub use derivative::derivative_text;
pub use print::{Implicit, Parenthesis, PrintOptions, format_number, to_string};
pub use rationalize::rationalize;
pub use simplify::{SimplifyOptions, simplify, simplify_core, simplify_text, simplify_with};

/// A symbolic operation failed, with mathjs's message where the TS would have thrown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymError(pub String);

impl std::fmt::Display for SymError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SymError {}

pub(crate) fn err<T>(message: impl Into<String>) -> Result<T, SymError> {
    Err(SymError(message.into()))
}

/// `math.parse(text)` in number mode.
pub fn parse(text: &str) -> Result<Node, SymError> {
    crate::syntax::parse(text).map_err(|e| SymError(e.message))
}

/// `new ConstantNode(value)`.
pub fn num(value: f64) -> Node {
    Node::Constant(Constant::Number {
        value,
        text: crate::jsnum::to_string(value),
    })
}

/// `new SymbolNode(name)`.
pub fn sym(name: &str) -> Node {
    Node::Symbol(name.to_string())
}

/// `new OperatorNode(op, fn, args)`: not implicit, not a percentage.
pub fn op(op: Op, args: Vec<Node>) -> Node {
    Node::Operator {
        op,
        args,
        implicit: false,
        percentage: false,
    }
}

/// `new OperatorNode(op, fn, args, implicit)`.
pub fn op_with(op: Op, args: Vec<Node>, implicit: bool) -> Node {
    Node::Operator {
        op,
        args,
        implicit,
        percentage: false,
    }
}

/// `new FunctionNode(name, args)`.
pub fn call(name: &str, args: Vec<Node>) -> Node {
    Node::Function {
        callee: Box::new(sym(name)),
        args,
    }
}

/// `new ParenthesisNode(content)`.
pub fn paren(content: Node) -> Node {
    Node::Parenthesis(Box::new(content))
}

/// A number constant's value (`ConstantNode.value` when it is a number).
pub fn number_value(n: &Node) -> Option<f64> {
    match n {
        Node::Constant(Constant::Number { value, .. }) => Some(*value),
        _ => None,
    }
}

/// `FunctionNode.name`: the callee's name, or `""` when the callee isn't a symbol.
pub fn function_name(n: &Node) -> &str {
    match n {
        Node::Function { callee, .. } => match callee.as_ref() {
            Node::Symbol(s) => s,
            _ => "",
        },
        _ => "",
    }
}

/// The `args` of an operator or function node (empty for any other node).
pub fn args(n: &Node) -> &[Node] {
    match n {
        Node::Operator { args, .. } | Node::Function { args, .. } => args,
        _ => &[],
    }
}

/// `node.getContent()`: through any parentheses.
pub fn content(n: &Node) -> &Node {
    match n {
        Node::Parenthesis(c) => content(c),
        _ => n,
    }
}

/// mathjs's `node.map(callback)`: a copy with `f` applied to each direct child.
pub fn map_children(n: &Node, f: &mut dyn FnMut(&Node) -> Node) -> Node {
    match n {
        Node::Operator {
            op,
            args,
            implicit,
            percentage,
        } => Node::Operator {
            op: *op,
            args: args.iter().map(&mut *f).collect(),
            implicit: *implicit,
            percentage: *percentage,
        },
        Node::Function { callee, args } => Node::Function {
            callee: Box::new(f(callee)),
            args: args.iter().map(&mut *f).collect(),
        },
        Node::Parenthesis(c) => Node::Parenthesis(Box::new(f(c))),
        Node::Array(items) => Node::Array(items.iter().map(&mut *f).collect()),
        Node::Range { start, end, step } => Node::Range {
            start: Box::new(f(start)),
            end: Box::new(f(end)),
            step: step.as_ref().map(|s| Box::new(f(s))),
        },
        Node::Conditional {
            condition,
            then,
            otherwise,
        } => Node::Conditional {
            condition: Box::new(f(condition)),
            then: Box::new(f(then)),
            otherwise: Box::new(f(otherwise)),
        },
        Node::Relational { ops, params } => Node::Relational {
            ops: ops.clone(),
            params: params.iter().map(&mut *f).collect(),
        },
        Node::Assignment {
            target,
            index,
            value,
        } => Node::Assignment {
            target: Box::new(f(target)),
            index: index.as_ref().map(|i| i.iter().map(&mut *f).collect()),
            value: Box::new(f(value)),
        },
        Node::FunctionAssignment { name, params, body } => Node::FunctionAssignment {
            name: name.clone(),
            params: params.clone(),
            body: Box::new(f(body)),
        },
        Node::Accessor { object, index, dot } => Node::Accessor {
            object: Box::new(f(object)),
            index: index.iter().map(&mut *f).collect(),
            dot: *dot,
        },
        Node::Block(items) => Node::Block(items.iter().map(|(n, v)| (f(n), *v)).collect()),
        Node::Constant(_) | Node::Symbol(_) => n.clone(),
    }
}

/// mathjs's `node.transform(callback)`: `f` returns `Some(replacement)` to replace a node (its children are then
/// not visited) or `None` to keep it and transform its children. The FunctionNode's callee is visited too, as in
/// mathjs (check [`is_callee`]-style context in `f` if a symbol callee must stay).
pub fn transform(n: &Node, f: &mut dyn FnMut(&Node) -> Option<Node>) -> Node {
    if let Some(r) = f(n) {
        return r;
    }
    map_children(n, &mut |c| transform(c, f))
}

/// mathjs's `node.traverse(callback)`: every node, parents first. The flag is true for a FunctionNode's callee
/// (`path === 'fn'`), which the engine skips when it collects variables.
pub fn traverse(n: &Node, f: &mut dyn FnMut(&Node, bool)) {
    fn walk(n: &Node, callee: bool, f: &mut dyn FnMut(&Node, bool)) {
        f(n, callee);
        match n {
            Node::Function { callee, args } => {
                walk(callee, true, f);
                for a in args {
                    walk(a, false, f);
                }
            }
            _ => {
                for k in children(n) {
                    walk(k, false, f);
                }
            }
        }
    }
    walk(n, false, f);
}

/// A node's direct children in mathjs's `forEach` order.
pub fn children(n: &Node) -> Vec<&Node> {
    match n {
        Node::Operator { args, .. } | Node::Array(args) => args.iter().collect(),
        Node::Function { callee, args } => std::iter::once(callee.as_ref()).chain(args).collect(),
        Node::Parenthesis(c) => vec![c],
        Node::Range { start, end, step } => {
            let mut v = vec![start.as_ref()];
            if let Some(s) = step {
                v.push(s);
            }
            v.push(end);
            v
        }
        Node::Conditional {
            condition,
            then,
            otherwise,
        } => vec![condition, then, otherwise],
        Node::Relational { params, .. } => params.iter().collect(),
        Node::Assignment {
            target,
            index,
            value,
        } => {
            let mut v = vec![target.as_ref()];
            if let Some(i) = index {
                v.extend(i);
            }
            v.push(value);
            v
        }
        Node::FunctionAssignment { body, .. } => vec![body],
        Node::Accessor { object, index, .. } => {
            std::iter::once(object.as_ref()).chain(index).collect()
        }
        Node::Block(items) => items.iter().map(|(n, _)| n).collect(),
        Node::Constant(_) | Node::Symbol(_) => Vec::new(),
    }
}

/// The variable names in a tree, in first-seen order, without function names.
pub fn symbols(n: &Node) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    traverse(n, &mut |m, callee| {
        if let Node::Symbol(s) = m
            && !callee
            && !out.contains(s)
        {
            out.push(s.clone());
        }
    });
    out
}

/// `removeParens`: the tree without parenthesis nodes.
pub fn remove_parens(n: &Node) -> Node {
    transform(n, &mut |m| match m {
        Node::Parenthesis(c) => Some(remove_parens(c)),
        _ => None,
    })
}
