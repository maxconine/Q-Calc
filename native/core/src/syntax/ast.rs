//! The expression tree, shaped like mathjs's nodes so ports of TS code that walks `MathNode`s translate directly.

use std::fmt;

/// Every operator mathjs parses, by its function name (`add`, `unaryMinus`, …).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    Add,
    Subtract,
    Multiply,
    DotMultiply,
    Divide,
    DotDivide,
    Mod,
    Pow,
    DotPow,
    UnaryMinus,
    UnaryPlus,
    BitNot,
    Not,
    Factorial,
    CTranspose,
    Or,
    Xor,
    And,
    BitOr,
    BitXor,
    BitAnd,
    Equal,
    Unequal,
    Smaller,
    Larger,
    SmallerEq,
    LargerEq,
    LeftShift,
    RightArithShift,
    RightLogShift,
    To,
    Nullish,
}

impl Op {
    /// The operator as written (`+`, `mod`, `!`).
    pub fn symbol(self) -> &'static str {
        match self {
            Op::Add | Op::UnaryPlus => "+",
            Op::Subtract | Op::UnaryMinus => "-",
            Op::Multiply => "*",
            Op::DotMultiply => ".*",
            Op::Divide => "/",
            Op::DotDivide => "./",
            Op::Mod => "mod",
            Op::Pow => "^",
            Op::DotPow => ".^",
            Op::BitNot => "~",
            Op::Not => "not",
            Op::Factorial => "!",
            Op::CTranspose => "'",
            Op::Or => "or",
            Op::Xor => "xor",
            Op::And => "and",
            Op::BitOr => "|",
            Op::BitXor => "^|",
            Op::BitAnd => "&",
            Op::Equal => "==",
            Op::Unequal => "!=",
            Op::Smaller => "<",
            Op::Larger => ">",
            Op::SmallerEq => "<=",
            Op::LargerEq => ">=",
            Op::LeftShift => "<<",
            Op::RightArithShift => ">>",
            Op::RightLogShift => ">>>",
            Op::To => "to",
            Op::Nullish => "??",
        }
    }

    /// The mathjs function name (`OperatorNode.fn`).
    pub fn name(self) -> &'static str {
        match self {
            Op::Add => "add",
            Op::Subtract => "subtract",
            Op::Multiply => "multiply",
            Op::DotMultiply => "dotMultiply",
            Op::Divide => "divide",
            Op::DotDivide => "dotDivide",
            Op::Mod => "mod",
            Op::Pow => "pow",
            Op::DotPow => "dotPow",
            Op::UnaryMinus => "unaryMinus",
            Op::UnaryPlus => "unaryPlus",
            Op::BitNot => "bitNot",
            Op::Not => "not",
            Op::Factorial => "factorial",
            Op::CTranspose => "ctranspose",
            Op::Or => "or",
            Op::Xor => "xor",
            Op::And => "and",
            Op::BitOr => "bitOr",
            Op::BitXor => "bitXor",
            Op::BitAnd => "bitAnd",
            Op::Equal => "equal",
            Op::Unequal => "unequal",
            Op::Smaller => "smaller",
            Op::Larger => "larger",
            Op::SmallerEq => "smallerEq",
            Op::LargerEq => "largerEq",
            Op::LeftShift => "leftShift",
            Op::RightArithShift => "rightArithShift",
            Op::RightLogShift => "rightLogShift",
            Op::To => "to",
            Op::Nullish => "nullish",
        }
    }
}

/// A literal.
#[derive(Clone, Debug, PartialEq)]
pub enum Constant {
    /// A number with the text it was written as, so exact arithmetic can read `0.1` as 1/10.
    Number {
        value: f64,
        text: String,
    },
    Str(String),
    Bool(bool),
    Null,
    Undefined,
}

/// One node of a parsed expression.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Constant(Constant),
    Symbol(String),
    Operator {
        op: Op,
        args: Vec<Node>,
        /// `2x`: a multiplication nobody typed.
        implicit: bool,
        /// `50%` read as `50 / 100`.
        percentage: bool,
    },
    /// A call; mathjs allows any callee, the engine only ever calls names.
    Function {
        callee: Box<Node>,
        args: Vec<Node>,
    },
    Parenthesis(Box<Node>),
    Array(Vec<Node>),
    Range {
        start: Box<Node>,
        end: Box<Node>,
        step: Option<Box<Node>>,
    },
    Conditional {
        condition: Box<Node>,
        then: Box<Node>,
        otherwise: Box<Node>,
    },
    /// `a < b <= c`.
    Relational {
        ops: Vec<Op>,
        params: Vec<Node>,
    },
    Assignment {
        target: Box<Node>,
        index: Option<Vec<Node>>,
        value: Box<Node>,
    },
    FunctionAssignment {
        name: String,
        params: Vec<String>,
        body: Box<Node>,
    },
    /// `a[1, 2]` or `a.b`.
    Accessor {
        object: Box<Node>,
        index: Vec<Node>,
        dot: bool,
    },
    Block(Vec<(Node, bool)>),
}

impl Node {
    pub(crate) fn op(op: Op, args: Vec<Node>) -> Node {
        Node::Operator {
            op,
            args,
            implicit: false,
            percentage: false,
        }
    }

    /// The name of a called symbol: `sqrt` in `sqrt(2)`.
    pub fn call_name(&self) -> Option<&str> {
        match self {
            Node::Function { callee, .. } => match callee.as_ref() {
                Node::Symbol(name) => Some(name),
                _ => None,
            },
            _ => None,
        }
    }

    pub(crate) fn is_constant(&self) -> bool {
        matches!(self, Node::Constant(_))
    }

    /// mathjs's `rule2Node`: a constant, or `-`/`+`/`~` applied to one.
    pub(crate) fn is_rule2(&self) -> bool {
        match self {
            Node::Constant(_) => true,
            Node::Operator {
                op: Op::UnaryMinus | Op::UnaryPlus | Op::BitNot,
                args,
                ..
            } => args.len() == 1 && args[0].is_constant(),
            _ => false,
        }
    }
}

impl fmt::Display for Node {
    /// A fully bracketed rendering, for tests and debugging (not mathjs's `toString`).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Node::Constant(Constant::Number { text, .. }) => write!(f, "{text}"),
            Node::Constant(Constant::Str(s)) => write!(f, "{s:?}"),
            Node::Constant(Constant::Bool(b)) => write!(f, "{b}"),
            Node::Constant(Constant::Null) => write!(f, "null"),
            Node::Constant(Constant::Undefined) => write!(f, "undefined"),
            Node::Symbol(s) => write!(f, "{s}"),
            Node::Operator { op, args, .. } => match args.as_slice() {
                [a] if matches!(op, Op::Factorial | Op::CTranspose) => {
                    write!(f, "({a}{})", op.symbol())
                }
                [a] => write!(f, "({}{a})", op.symbol()),
                [a, b] => write!(f, "({a} {} {b})", op.symbol()),
                _ => write!(f, "{}(…)", op.name()),
            },
            Node::Function { callee, args } => {
                write!(f, "{callee}(")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{a}")?;
                }
                write!(f, ")")
            }
            Node::Parenthesis(inner) => write!(f, "{inner}"),
            Node::Array(items) => {
                write!(f, "[")?;
                for (i, a) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{a}")?;
                }
                write!(f, "]")
            }
            Node::Range { start, end, step } => match step {
                Some(s) => write!(f, "({start}:{s}:{end})"),
                None => write!(f, "({start}:{end})"),
            },
            Node::Conditional {
                condition,
                then,
                otherwise,
            } => write!(f, "({condition} ? {then} : {otherwise})"),
            Node::Relational { params, .. } => write!(f, "relational({} terms)", params.len()),
            Node::Assignment { target, value, .. } => write!(f, "({target} = {value})"),
            Node::FunctionAssignment { name, params, body } => {
                write!(f, "({name}({}) = {body})", params.join(", "))
            }
            Node::Accessor { object, index, .. } => write!(f, "{object}[{} items]", index.len()),
            Node::Block(items) => write!(f, "block({} items)", items.len()),
        }
    }
}
