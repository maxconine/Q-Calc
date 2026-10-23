//! mathjs's recursive-descent parser (`expression/parse.js`), one function per precedence level, in its order.
//! The engine's text passes produce mathjs syntax, so every reading quirk the TS relies on lives here:
//! implicit multiplication binds tighter than `*` and `/` (`8 pi / 2 pi` is 1), "rule 2" makes `1/2x` `(1/2)x`,
//! `^` is right-associative and takes a unary on its right (`2^-3`), and `-2^2` is `-(2^2)`.

use super::ParseError;
use super::ast::{Constant, Node, Op};
use super::lexer::{Lexer, NAMED_DELIMITERS, TokenType};

/// Parses one expression the way `math.parse` does.
pub fn parse(expression: &str) -> Result<Node, ParseError> {
    let mut p = Parser { lx: Lexer::new(expression), depth: 0 };
    p.lx.next()?;
    let node = p.block()?;
    if !p.lx.token.is_empty() {
        let message = if p.lx.token_type == TokenType::Delimiter {
            format!("Unexpected operator {}", p.lx.token)
        } else {
            format!("Unexpected part \"{}\"", p.lx.token)
        };
        return Err(p.lx.error(&message));
    }
    Ok(node)
}

struct Parser {
    lx: Lexer,
    depth: usize,
}

/// Nesting past this is a syntax error: mathjs would hit the JS stack limit somewhere past it too, and a Rust
/// stack overflow aborts the process. Evaluation threads get a stack sized for it (see ARCHITECTURE.md).
pub const MAX_NESTING: usize = 512;

type Parsed = Result<Node, ParseError>;

fn is(lx: &Lexer, token: &str) -> bool {
    lx.token == token
}

impl Parser {
    fn tok(&self) -> &str {
        &self.lx.token
    }

    fn block(&mut self) -> Parsed {
        let mut node = None;
        let mut blocks = Vec::new();
        if !matches!(self.tok(), "" | "\n" | ";") {
            node = Some(self.assignment()?);
        }
        while matches!(self.tok(), "\n" | ";") {
            if blocks.is_empty()
                && let Some(n) = node.take()
            {
                let visible = self.tok() != ";";
                blocks.push((n, visible));
            }
            self.lx.next()?;
            if !matches!(self.tok(), "\n" | ";" | "") {
                let n = self.assignment()?;
                let visible = self.tok() != ";";
                blocks.push((n, visible));
            }
        }
        if !blocks.is_empty() {
            return Ok(Node::Block(blocks));
        }
        Ok(node.unwrap_or(Node::Constant(Constant::Undefined)))
    }

    fn assignment(&mut self) -> Parsed {
        self.depth += 1;
        let out = self.assignment_inner();
        self.depth -= 1;
        out
    }

    fn assignment_inner(&mut self) -> Parsed {
        if self.depth > MAX_NESTING {
            return Err(self.lx.error("Expression nested too deeply"));
        }
        let node = self.conditional()?;
        if self.tok() != "=" {
            return Ok(node);
        }
        match node {
            Node::Symbol(name) => {
                self.lx.next_skip_newline()?;
                let value = self.assignment()?;
                Ok(Node::Assignment { target: Box::new(Node::Symbol(name)), index: None, value: Box::new(value) })
            }
            Node::Accessor { object, index, .. } => {
                self.lx.next_skip_newline()?;
                let value = self.assignment()?;
                Ok(Node::Assignment { target: object, index: Some(index), value: Box::new(value) })
            }
            Node::Function { callee, args } => {
                let params: Option<Vec<String>> =
                    args.iter().map(|a| if let Node::Symbol(s) = a { Some(s.clone()) } else { None }).collect();
                match (*callee, params) {
                    (Node::Symbol(name), Some(params)) => {
                        self.lx.next_skip_newline()?;
                        let body = self.assignment()?;
                        Ok(Node::FunctionAssignment { name, params, body: Box::new(body) })
                    }
                    _ => Err(self.lx.error("Invalid left hand side of assignment operator =")),
                }
            }
            _ => Err(self.lx.error("Invalid left hand side of assignment operator =")),
        }
    }

    fn conditional(&mut self) -> Parsed {
        let mut node = self.logical_or()?;
        while self.tok() == "?" {
            let prev = self.lx.conditional_level;
            self.lx.conditional_level = Some(self.lx.nesting_level);
            self.lx.next_skip_newline()?;
            let then = self.assignment()?;
            if self.tok() != ":" {
                return Err(self.lx.error("False part of conditional expression expected"));
            }
            self.lx.conditional_level = None;
            self.lx.next_skip_newline()?;
            let otherwise = self.assignment()?;
            node = Node::Conditional { condition: Box::new(node), then: Box::new(then), otherwise: Box::new(otherwise) };
            self.lx.conditional_level = prev;
        }
        Ok(node)
    }

    /// A left-associative level with one operator token.
    fn left(&mut self, token: &str, op: Op, next: fn(&mut Self) -> Parsed) -> Parsed {
        let mut node = next(self)?;
        while is(&self.lx, token) {
            self.lx.next_skip_newline()?;
            let right = next(self)?;
            node = Node::op(op, vec![node, right]);
        }
        Ok(node)
    }

    fn logical_or(&mut self) -> Parsed {
        self.left("or", Op::Or, Self::logical_xor)
    }

    fn logical_xor(&mut self) -> Parsed {
        self.left("xor", Op::Xor, Self::logical_and)
    }

    fn logical_and(&mut self) -> Parsed {
        self.left("and", Op::And, Self::bitwise_or)
    }

    fn bitwise_or(&mut self) -> Parsed {
        self.left("|", Op::BitOr, Self::bitwise_xor)
    }

    fn bitwise_xor(&mut self) -> Parsed {
        self.left("^|", Op::BitXor, Self::bitwise_and)
    }

    fn bitwise_and(&mut self) -> Parsed {
        self.left("&", Op::BitAnd, Self::relational)
    }

    fn relational(&mut self) -> Parsed {
        let mut params = vec![self.shift()?];
        let mut ops = Vec::new();
        loop {
            let op = match self.tok() {
                "==" => Op::Equal,
                "!=" => Op::Unequal,
                "<" => Op::Smaller,
                ">" => Op::Larger,
                "<=" => Op::SmallerEq,
                ">=" => Op::LargerEq,
                _ => break,
            };
            ops.push(op);
            self.lx.next_skip_newline()?;
            params.push(self.shift()?);
        }
        Ok(match params.len() {
            1 => params.remove(0),
            2 => Node::op(ops[0], params),
            _ => Node::Relational { ops, params },
        })
    }

    fn shift(&mut self) -> Parsed {
        let mut node = self.conversion()?;
        loop {
            let op = match self.tok() {
                "<<" => Op::LeftShift,
                ">>" => Op::RightArithShift,
                ">>>" => Op::RightLogShift,
                _ => break,
            };
            self.lx.next_skip_newline()?;
            let right = self.conversion()?;
            node = Node::op(op, vec![node, right]);
        }
        Ok(node)
    }

    fn conversion(&mut self) -> Parsed {
        let mut node = self.range()?;
        while matches!(self.tok(), "to" | "in") {
            let name = self.tok().to_string();
            self.lx.next_skip_newline()?;
            if name == "in" && matches!(self.tok(), "]" | ")" | "}" | "," | ";" | "") {
                // end of expression: this is the unit `in` (inch)
                node = Node::Operator {
                    op: Op::Multiply,
                    args: vec![node, Node::Symbol("in".into())],
                    implicit: true,
                    percentage: false,
                };
            } else {
                let right = self.range()?;
                node = Node::op(Op::To, vec![node, right]);
            }
        }
        Ok(node)
    }

    fn range(&mut self) -> Parsed {
        let in_conditional = self.lx.conditional_level == Some(self.lx.nesting_level);
        let mut node = if self.tok() == ":" {
            if in_conditional {
                return Err(self.lx.error("The true-expression of a conditional operator may not be empty"));
            }
            number_node(1.0, "1")
        } else {
            self.add_subtract()?
        };
        if self.tok() == ":" && self.lx.conditional_level != Some(self.lx.nesting_level) {
            let mut params = vec![node];
            while self.tok() == ":" && params.len() < 3 {
                self.lx.next_skip_newline()?;
                if matches!(self.tok(), ")" | "]" | "," | "") {
                    params.push(Node::Symbol("end".into()));
                } else {
                    params.push(self.add_subtract()?);
                }
            }
            let mut it = params.into_iter();
            let start = Box::new(it.next().unwrap_or(Node::Constant(Constant::Undefined)));
            let second = it.next().unwrap_or(Node::Constant(Constant::Undefined));
            node = match it.next() {
                Some(end) => Node::Range { start, end: Box::new(end), step: Some(Box::new(second)) },
                None => Node::Range { start, end: Box::new(second), step: None },
            };
        }
        Ok(node)
    }

    fn add_subtract(&mut self) -> Parsed {
        let mut node = self.multiply_divide()?;
        loop {
            let op = match self.tok() {
                "+" => Op::Add,
                "-" => Op::Subtract,
                _ => break,
            };
            self.lx.next_skip_newline()?;
            let right = self.multiply_divide()?;
            let right = if matches!(right, Node::Operator { percentage: true, .. }) {
                // `100 + 5%` is 100 + 100 * 5%
                Node::op(Op::Multiply, vec![node.clone(), right])
            } else {
                right
            };
            node = Node::op(op, vec![node, right]);
        }
        Ok(node)
    }

    fn multiply_divide(&mut self) -> Parsed {
        let mut node = self.implicit_multiplication()?;
        loop {
            let op = match self.tok() {
                "*" => Op::Multiply,
                ".*" => Op::DotMultiply,
                "/" => Op::Divide,
                "./" => Op::DotDivide,
                "%" | "mod" => Op::Mod,
                _ => break,
            };
            self.lx.next_skip_newline()?;
            let right = self.implicit_multiplication()?;
            node = Node::op(op, vec![node, right]);
        }
        Ok(node)
    }

    fn implicit_multiplication(&mut self) -> Parsed {
        let mut node = self.rule2()?;
        let mut last_is_constant = node.is_constant();
        let mut last_blocks_number = blocks_number(&node);
        loop {
            let t = self.lx.token_type;
            let tok = self.tok();
            let in_after_constant = tok == "in"
                && (node.is_constant()
                    || matches!(&node, Node::Operator { op: Op::UnaryMinus, args, .. } if args.len() == 1 && args[0].is_constant()));
            let go = t == TokenType::Symbol
                || in_after_constant
                || (t == TokenType::Number && !last_is_constant && !last_blocks_number)
                || tok == "(";
            if !go {
                break;
            }
            let last = self.rule2()?;
            last_is_constant = last.is_constant();
            last_blocks_number = blocks_number(&last);
            node = Node::Operator { op: Op::Multiply, args: vec![node, last], implicit: true, percentage: false };
        }
        Ok(node)
    }

    fn rule2(&mut self) -> Parsed {
        let mut node = self.unary_percentage()?;
        let mut last_rule2 = node.is_rule2();
        while self.tok() == "/" && last_rule2 {
            let before_slash = self.lx.clone();
            self.lx.next_skip_newline()?;
            if self.lx.token_type != TokenType::Number {
                self.lx = before_slash;
                break;
            }
            let at_number = self.lx.clone();
            self.lx.next_skip_newline()?;
            if self.lx.token_type == TokenType::Symbol || matches!(self.tok(), "(" | "in") {
                // `number / number symbol`: the division binds first
                self.lx = at_number;
                let last = self.unary_percentage()?;
                last_rule2 = last.is_rule2();
                node = Node::op(Op::Divide, vec![node, last]);
            } else {
                self.lx = before_slash;
                break;
            }
        }
        Ok(node)
    }

    fn unary_percentage(&mut self) -> Parsed {
        let mut node = self.unary()?;
        if self.tok() == "%" {
            let before = self.lx.clone();
            self.lx.next_skip_newline()?;
            // a term after the % makes it a modulo; nothing parseable makes it a percentage
            let term = self.unary().is_ok();
            self.lx = before;
            if !term {
                self.lx.next_skip_newline()?;
                node = Node::Operator {
                    op: Op::Divide,
                    args: vec![node, number_node(100.0, "100")],
                    implicit: false,
                    percentage: true,
                };
            }
        }
        Ok(node)
    }

    fn unary(&mut self) -> Parsed {
        self.depth += 1;
        let out = self.unary_inner();
        self.depth -= 1;
        out
    }

    fn unary_inner(&mut self) -> Parsed {
        if self.depth > MAX_NESTING {
            return Err(self.lx.error("Expression nested too deeply"));
        }
        let op = match self.tok() {
            "-" => Op::UnaryMinus,
            "+" => Op::UnaryPlus,
            "~" => Op::BitNot,
            "not" => Op::Not,
            _ => return self.pow(),
        };
        self.lx.next_skip_newline()?;
        let arg = self.unary()?;
        Ok(Node::op(op, vec![arg]))
    }

    fn pow(&mut self) -> Parsed {
        let node = self.nullish()?;
        let op = match self.tok() {
            "^" => Op::Pow,
            ".^" => Op::DotPow,
            _ => return Ok(node),
        };
        self.lx.next_skip_newline()?;
        let right = self.unary()?;
        Ok(Node::op(op, vec![node, right]))
    }

    fn nullish(&mut self) -> Parsed {
        self.left("??", Op::Nullish, Self::left_hand_operators)
    }

    fn left_hand_operators(&mut self) -> Parsed {
        let mut node = self.symbol()?;
        loop {
            let op = match self.tok() {
                "!" => Op::Factorial,
                "'" => Op::CTranspose,
                _ => break,
            };
            self.lx.next()?;
            node = Node::op(op, vec![node]);
            node = self.accessors(node)?;
        }
        Ok(node)
    }

    fn symbol(&mut self) -> Parsed {
        let named = self.lx.token_type == TokenType::Delimiter && NAMED_DELIMITERS.contains(&self.tok());
        if self.lx.token_type != TokenType::Symbol && !named {
            return self.string();
        }
        let name = self.tok().to_string();
        self.lx.next()?;
        let node = match name.as_str() {
            "true" => Node::Constant(Constant::Bool(true)),
            "false" => Node::Constant(Constant::Bool(false)),
            "null" => Node::Constant(Constant::Null),
            "undefined" => Node::Constant(Constant::Undefined),
            "NaN" => number_node(f64::NAN, "NaN"),
            "Infinity" => number_node(f64::INFINITY, "Infinity"),
            _ => Node::Symbol(name),
        };
        self.accessors(node)
    }

    fn accessors(&mut self, mut node: Node) -> Parsed {
        loop {
            let mut optional = false;
            if self.tok() == "?." {
                optional = true;
                self.lx.next()?;
            }
            let has_next = matches!(self.tok(), "(" | "[" | ".");
            if !(optional || has_next) {
                return Ok(node);
            }
            if self.tok() == "(" {
                if !(optional || matches!(node, Node::Symbol(_) | Node::Accessor { .. })) {
                    // `(2+3)(4+5)`: implicit multiplication, left to that level
                    return Ok(node);
                }
                let args = self.call_args(")", "Parenthesis ) expected")?;
                node = Node::Function { callee: Box::new(node), args };
            } else if self.tok() == "[" {
                let index = self.call_args("]", "Parenthesis ] expected")?;
                node = Node::Accessor { object: Box::new(node), index, dot: false };
            } else {
                if !optional {
                    self.lx.next()?;
                }
                let named = self.lx.token_type == TokenType::Delimiter && NAMED_DELIMITERS.contains(&self.tok());
                if self.lx.token_type != TokenType::Symbol && !named {
                    let what = if optional { "optional chain" } else { "dot" };
                    return Err(self.lx.error(&format!("Property name expected after {what}")));
                }
                let prop = Node::Constant(Constant::Str(self.tok().to_string()));
                self.lx.next()?;
                node = Node::Accessor { object: Box::new(node), index: vec![prop], dot: true };
            }
        }
    }

    /// `( a, b )` or `[ a, b ]` after a callee, the opening bracket being the current token.
    fn call_args(&mut self, close: &str, missing: &str) -> Result<Vec<Node>, ParseError> {
        self.lx.nesting_level += 1;
        self.lx.next()?;
        let mut args = Vec::new();
        if self.tok() != close {
            args.push(self.assignment()?);
            while self.tok() == "," {
                self.lx.next()?;
                args.push(self.assignment()?);
            }
        }
        if self.tok() != close {
            return Err(self.lx.error(missing));
        }
        self.lx.nesting_level -= 1;
        self.lx.next()?;
        Ok(args)
    }

    fn string(&mut self) -> Parsed {
        let quote = match self.tok() {
            "\"" => '"',
            "'" => '\'',
            _ => return self.matrix(),
        };
        let s = self.lx.string_body(quote)?;
        self.accessors(Node::Constant(Constant::Str(s)))
    }

    fn matrix(&mut self) -> Parsed {
        if self.tok() != "[" {
            return self.object();
        }
        self.lx.nesting_level += 1;
        self.lx.next()?;
        let array = if self.tok() != "]" {
            let row = self.row()?;
            if self.tok() == ";" {
                let mut rows = vec![row];
                while self.tok() == ";" {
                    self.lx.next()?;
                    if self.tok() != "]" {
                        rows.push(self.row()?);
                    }
                }
                if self.tok() != "]" {
                    return Err(self.lx.error("End of matrix ] expected"));
                }
                self.lx.nesting_level -= 1;
                self.lx.next()?;
                let cols = row_len(&rows[0]);
                if let Some(bad) = rows.iter().skip(1).find(|r| row_len(r) != cols) {
                    return Err(self.lx.error(&format!("Column dimensions mismatch ({} !== {cols})", row_len(bad))));
                }
                Node::Array(rows)
            } else {
                if self.tok() != "]" {
                    return Err(self.lx.error("End of matrix ] expected"));
                }
                self.lx.nesting_level -= 1;
                self.lx.next()?;
                row
            }
        } else {
            self.lx.nesting_level -= 1;
            self.lx.next()?;
            Node::Array(Vec::new())
        };
        self.accessors(array)
    }

    fn row(&mut self) -> Parsed {
        let mut items = vec![self.assignment()?];
        while self.tok() == "," {
            self.lx.next()?;
            if !matches!(self.tok(), "]" | ";") {
                items.push(self.assignment()?);
            }
        }
        Ok(Node::Array(items))
    }

    fn object(&mut self) -> Parsed {
        if self.tok() == "{" {
            // the engine never writes objects; mathjs would build one, the engine would reject it
            return Err(self.lx.error("Objects are not supported"));
        }
        self.number()
    }

    fn number(&mut self) -> Parsed {
        if self.lx.token_type != TokenType::Number {
            return self.parentheses();
        }
        let text = self.tok().to_string();
        self.lx.next()?;
        let value = number_value(&text).ok_or_else(|| self.lx.error(&format!("Invalid number {text}")))?;
        Ok(number_node(value, &text))
    }

    fn parentheses(&mut self) -> Parsed {
        if self.tok() != "(" {
            return Err(if self.tok().is_empty() {
                self.lx.error("Unexpected end of expression")
            } else {
                self.lx.error("Value expected")
            });
        }
        self.lx.nesting_level += 1;
        self.lx.next()?;
        let inner = self.assignment()?;
        if self.tok() != ")" {
            return Err(self.lx.error("Parenthesis ) expected"));
        }
        self.lx.nesting_level -= 1;
        self.lx.next()?;
        self.accessors(Node::Parenthesis(Box::new(inner)))
    }
}

/// After a number, `(2+3)2` multiplies but `2 3` doesn't: mathjs only lets a number follow a non-constant that
/// isn't an operator (or is a factorial).
fn blocks_number(last: &Node) -> bool {
    matches!(last, Node::Operator { op, .. } if *op != Op::Factorial)
}

fn row_len(row: &Node) -> usize {
    match row {
        Node::Array(items) => items.len(),
        _ => 1,
    }
}

fn number_node(value: f64, text: &str) -> Node {
    Node::Constant(Constant::Number { value, text: text.to_string() })
}

/// A number token's value in mathjs `number` mode: decimal, or `0x`/`0b`/`0o` integers.
fn number_value(text: &str) -> Option<f64> {
    let lower = text.to_ascii_lowercase();
    for (prefix, radix) in [("0x", 16), ("0b", 2), ("0o", 8)] {
        if let Some(digits) = lower.strip_prefix(prefix) {
            if digits.contains('.') || digits.contains('i') {
                return None;
            }
            return u64::from_str_radix(digits, radix).ok().map(|v| v as f64);
        }
    }
    text.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(s: &str) -> String {
        parse(s).map(|n| n.to_string()).unwrap_or_else(|e| format!("error: {}", e.message))
    }

    #[test]
    fn precedence_and_associativity() {
        assert_eq!(shape("2+3*4"), "(2 + (3 * 4))");
        assert_eq!(shape("2^3^2"), "(2 ^ (3 ^ 2))");
        assert_eq!(shape("-2^2"), "(-(2 ^ 2))");
        assert_eq!(shape("2^-2^2"), "(2 ^ (-(2 ^ 2)))");
        assert_eq!(shape("10-4-3"), "((10 - 4) - 3)");
        assert_eq!(shape("3!^2"), "((3!) ^ 2)");
        assert_eq!(shape("-3!"), "(-(3!))");
    }

    #[test]
    fn implicit_multiplication() {
        assert_eq!(shape("8 pi / 2 pi"), "((8 * pi) / (2 * pi))");
        assert_eq!(shape("1/2x"), "((1 / 2) * x)");
        assert_eq!(shape("2(3)"), "(2 * 3)");
        assert_eq!(shape("(2)(3)"), "(2 * 3)");
        assert_eq!(shape("(1+2)3"), "((1 + 2) * 3)");
        assert_eq!(shape("sqrt(4)(1+2)"), "(sqrt(4) * (1 + 2))");
        assert!(shape("2 3").starts_with("error"));
    }

    #[test]
    fn percent_and_mod() {
        assert_eq!(shape("50%"), "(50 / 100)");
        assert_eq!(shape("17 % 5"), "(17 mod 5)");
        assert_eq!(shape("7 mod 3"), "(7 mod 3)");
        assert_eq!(shape("100 + 5%"), "(100 + (100 * (5 / 100)))");
    }

    #[test]
    fn numbers_and_errors() {
        assert_eq!(shape("0xff"), "0xff");
        assert_eq!(shape(".5"), ".5");
        assert!(shape("2+").starts_with("error: Unexpected end of expression"));
        assert!(shape("1e+").starts_with("error: Digit expected"));
        assert_eq!(shape("1e"), "(1 * e)");
        assert!(shape("(2").starts_with("error: Parenthesis ) expected"));
        assert_eq!(shape("f(x) = x^2"), "(f(x) = (x ^ 2))");
        assert_eq!(shape("[1, 2; 3, 4]"), "[[1, 2], [3, 4]]");
    }
}
