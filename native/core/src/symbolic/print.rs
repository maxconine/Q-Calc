//! mathjs's `Node.toString(options)`: `OperatorNode._toString` with `calculateNecessaryParentheses`, the
//! precedence table of `expression/operators.js`, and `format(value)` for constants.
//!
//! Quirks kept from mathjs 15.2:
//! - `isAssociativeWith(a, b)` compares `a` with itself unless `parenthesis` is `'keep'` (it reads
//!   `nodeA.getContent()` for both sides), so `a - (b - c)` and `a / (b / c)` still get their parentheses only
//!   from the precedence rule.
//! - the multiply group's `associativeWith` names `'Operator:dotMultiply'` (sic), which never matches.
//! - an operator with three or more arguments that isn't `+` or `*` prints as `fn(args)` with the arguments in
//!   the default format, whatever the options.
//! - numbers print in mathjs's `auto` notation: exponential below 1e-3 and from 1e5 (`1e+5`, `1.5e-4`).

use super::{Constant, Node, Op, args, content, function_name};

/// `options.parenthesis`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Parenthesis {
    #[default]
    Keep,
    Auto,
    All,
}

/// `options.implicit`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Implicit {
    #[default]
    Hide,
    Show,
}

/// The options object passed to `toString`. `None` fields are absent from the object (mathjs then uses
/// `'keep'` and `'hide'`); [`PrintOptions::default`] is `toString()` with no argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PrintOptions {
    pub parenthesis: Option<Parenthesis>,
    pub implicit: Option<Implicit>,
}

impl PrintOptions {
    /// `{parenthesis: 'auto'}`, which the calculus reader prints with.
    pub fn auto() -> Self {
        PrintOptions {
            parenthesis: Some(Parenthesis::Auto),
            implicit: None,
        }
    }

    /// `{parenthesis: 'all'}`, simplify's loop key.
    pub fn all() -> Self {
        PrintOptions {
            parenthesis: Some(Parenthesis::All),
            implicit: None,
        }
    }
}

/// `node.toString(options)`.
pub fn to_string(n: &Node, o: &PrintOptions) -> String {
    match n {
        Node::Constant(c) => constant_text(c),
        Node::Symbol(s) => s.clone(),
        Node::Parenthesis(c) => {
            if matches!(o.parenthesis, None | Some(Parenthesis::Keep)) {
                format!("({})", to_string(c, o))
            } else {
                to_string(c, o)
            }
        }
        Node::Function { callee, args } => {
            let fname = match callee.as_ref() {
                f @ Node::FunctionAssignment { .. } => format!("({})", to_string(f, o)),
                f => to_string(f, o),
            };
            let a: Vec<String> = args.iter().map(|x| to_string(x, o)).collect();
            format!("{fname}({})", a.join(", "))
        }
        Node::Operator { .. } => operator_text(n, o),
        Node::Array(items) => {
            let a: Vec<String> = items.iter().map(|x| to_string(x, o)).collect();
            format!("[{}]", a.join(", "))
        }
        // nodes the symbolic layer never prints; a bracketed rendering
        other => other.to_string(),
    }
}

fn constant_text(c: &Constant) -> String {
    match c {
        Constant::Number { value, .. } => format_number(*value),
        Constant::Str(s) => stringify(s),
        Constant::Bool(b) => b.to_string(),
        Constant::Null => "null".into(),
        Constant::Undefined => "undefined".into(),
    }
}

/// mathjs's `stringify` (`utils/string.js`): double quotes, control characters escaped.
fn stringify(s: &str) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// mathjs `format(number)` with no precision: `auto` notation, exponential outside 1e-3 ≤ |x| < 1e5.
pub fn format_number(value: f64) -> String {
    if value == f64::INFINITY {
        return "Infinity".into();
    }
    if value == f64::NEG_INFINITY {
        return "-Infinity".into();
    }
    if value.is_nan() {
        return "NaN".into();
    }
    let (sign, coefficients, exponent) = split_number(value);
    let text = if !(-3..5).contains(&exponent) {
        // toExponential(value) with no precision
        let first = coefficients[0];
        let rest: String = coefficients[1..].iter().map(|d| char::from(b'0' + d)).collect();
        let dot = if rest.is_empty() {
            String::new()
        } else {
            format!(".{rest}")
        };
        let e = if exponent >= 0 {
            format!("+{exponent}")
        } else {
            exponent.to_string()
        };
        format!("{sign}{first}{dot}e{e}")
    } else {
        let mut c: Vec<u8> = coefficients.clone();
        let e = exponent;
        // append trailing zeros up to the decimal point
        let pad = e - c.len() as i64 + 1;
        if pad > 0 {
            c.extend(std::iter::repeat_n(0, pad as usize));
        }
        if e < 0 {
            let mut z = vec![0u8; (-e) as usize];
            z.extend(c);
            c = z;
        }
        let dot = if e > 0 { e as usize } else { 0 };
        let mut s: String = c.iter().map(|d| char::from(b'0' + d)).collect();
        if dot < c.len() - 1 {
            s.insert(dot + 1, '.');
        }
        format!("{sign}{s}")
    };
    strip_trailing_zeros(&text)
}

/// `.replace(/((\.\d*?)(0+))($|e)/, …)`: trailing zeros after the point go, and the point with them.
fn strip_trailing_zeros(s: &str) -> String {
    let Some(dot) = s.find('.') else {
        return s.to_string();
    };
    let after = &s[dot + 1..];
    let digits_end = after
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(after.len());
    let tail = &after[digits_end..];
    if !(tail.is_empty() || tail.starts_with('e')) {
        return s.to_string();
    }
    let digits = &after[..digits_end];
    let kept = digits.trim_end_matches('0');
    if kept.len() == digits.len() {
        return s.to_string();
    }
    let head = &s[..dot];
    if kept.is_empty() {
        format!("{head}{tail}")
    } else {
        format!("{head}.{kept}{tail}")
    }
}

/// mathjs `splitNumber`: sign, significant digits and the exponent of the first one.
fn split_number(value: f64) -> (&'static str, Vec<u8>, i64) {
    let text = crate::jsnum::to_string(value).to_lowercase();
    let (sign, body) = match text.strip_prefix('-') {
        Some(rest) => ("-", rest.to_string()),
        None => ("", text.clone()),
    };
    let (digits, exp_part) = match body.find('e') {
        Some(i) => (body[..i].to_string(), body[i + 1..].parse::<i64>().unwrap_or(0)),
        None => (body.clone(), 0),
    };
    let mut exponent = exp_part;
    match digits.find('.') {
        Some(dot) => exponent += dot as i64 - 1,
        None => exponent += digits.len() as i64 - 1,
    }
    let joined: String = digits.chars().filter(|c| *c != '.').collect();
    let trimmed = joined.trim_start_matches('0');
    exponent -= (joined.len() - trimmed.len()) as i64;
    let trimmed = trimmed.trim_end_matches('0');
    let mut coefficients: Vec<u8> = trimmed.bytes().map(|b| b - b'0').collect();
    if coefficients.is_empty() {
        coefficients.push(0);
        exponent += 1;
    }
    (sign, coefficients, exponent)
}

// ---- operators.js

/// `node.getIdentifier()`.
fn identifier(n: &Node) -> String {
    match n {
        Node::Operator { op, .. } => format!("OperatorNode:{}", op.name()),
        Node::Function { .. } => format!("FunctionNode:{}", function_name(n)),
        Node::Constant(_) => "ConstantNode".into(),
        Node::Symbol(_) => "SymbolNode".into(),
        Node::Parenthesis(_) => "ParenthesisNode".into(),
        Node::Array(_) => "ArrayNode".into(),
        Node::Range { .. } => "RangeNode".into(),
        Node::Conditional { .. } => "ConditionalNode".into(),
        Node::Relational { .. } => "RelationalNode".into(),
        Node::Assignment { .. } => "AssignmentNode".into(),
        Node::FunctionAssignment { .. } => "FunctionAssignmentNode".into(),
        Node::Accessor { .. } => "AccessorNode".into(),
        Node::Block(_) => "BlockNode".into(),
    }
}

/// The first group of `properties` holding the identifier.
fn group(id: &str) -> Option<usize> {
    let g = match id {
        "AssignmentNode" | "FunctionAssignmentNode" => 0,
        "ConditionalNode" => 1,
        "OperatorNode:or" => 2,
        "OperatorNode:xor" => 3,
        "OperatorNode:and" => 4,
        "OperatorNode:bitOr" => 5,
        "OperatorNode:bitXor" => 6,
        "OperatorNode:bitAnd" => 7,
        "OperatorNode:equal"
        | "OperatorNode:unequal"
        | "OperatorNode:smaller"
        | "OperatorNode:larger"
        | "OperatorNode:smallerEq"
        | "OperatorNode:largerEq"
        | "RelationalNode" => 8,
        "OperatorNode:leftShift" | "OperatorNode:rightArithShift" | "OperatorNode:rightLogShift" => 9,
        "OperatorNode:to" => 10,
        "RangeNode" => 11,
        "OperatorNode:add" | "OperatorNode:subtract" => 12,
        "OperatorNode:multiply"
        | "OperatorNode:divide"
        | "OperatorNode:dotMultiply"
        | "OperatorNode:dotDivide"
        | "OperatorNode:mod" => 13,
        "OperatorNode:unaryPlus"
        | "OperatorNode:unaryMinus"
        | "OperatorNode:bitNot"
        | "OperatorNode:not" => 15,
        "OperatorNode:pow" | "OperatorNode:dotPow" => 16,
        "OperatorNode:nullish" => 17,
        "OperatorNode:factorial" => 18,
        "OperatorNode:ctranspose" => 19,
        _ => return None,
    };
    Some(g)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Assoc {
    Left,
    Right,
}

/// `properties[group][id].associativity`.
fn associativity_of(id: &str) -> Option<Assoc> {
    match id {
        "AssignmentNode" | "FunctionAssignmentNode" | "ConditionalNode" | "RangeNode" => None,
        "OperatorNode:unaryPlus"
        | "OperatorNode:unaryMinus"
        | "OperatorNode:bitNot"
        | "OperatorNode:not"
        | "OperatorNode:pow"
        | "OperatorNode:dotPow" => Some(Assoc::Right),
        _ if group(id).is_some() => Some(Assoc::Left),
        _ => None,
    }
}

/// `properties[group][id].associativeWith`, when the property is defined.
fn associative_with(id: &str) -> Option<&'static [&'static str]> {
    match id {
        "OperatorNode:add" => Some(&["OperatorNode:add", "OperatorNode:subtract"]),
        "OperatorNode:multiply" => Some(&[
            "OperatorNode:multiply",
            "OperatorNode:divide",
            "Operator:dotMultiply",
            "Operator:dotDivide",
        ]),
        "OperatorNode:dotMultiply" => Some(&[
            "OperatorNode:multiply",
            "OperatorNode:divide",
            "OperatorNode:dotMultiply",
            "OperatorNode:doDivide",
        ]),
        "OperatorNode:unaryPlus"
        | "OperatorNode:unaryMinus"
        | "OperatorNode:bitNot"
        | "OperatorNode:not"
        | "OperatorNode:factorial"
        | "OperatorNode:ctranspose"
        | "AssignmentNode"
        | "FunctionAssignmentNode"
        | "ConditionalNode"
        | "RangeNode" => None,
        _ if group(id).is_some() => Some(&[]),
        _ => None,
    }
}

fn unwrap_paren(n: &Node, p: Option<Parenthesis>) -> &Node {
    if p == Some(Parenthesis::Auto) {
        content(n)
    } else {
        n
    }
}

/// mathjs `rule2Node`: a constant, or `-`/`+`/`~` applied to one.
pub(crate) fn rule2_node(n: &Node) -> bool {
    match n {
        Node::Constant(_) => true,
        Node::Operator { op, args, .. } => {
            args.len() == 1
                && matches!(args[0], Node::Constant(_))
                && matches!(op.symbol(), "-" | "+" | "~")
        }
        _ => false,
    }
}

/// `getPrecedence(node, parenthesis, implicit, parent)`; `implicit` None is `undefined`.
fn precedence(
    n: &Node,
    p: Parenthesis,
    implicit: Option<Implicit>,
    parent: Option<&Node>,
) -> Option<usize> {
    let node = if p != Parenthesis::Keep { content(n) } else { n };
    let id = identifier(node);
    let mut prec = group(&id);
    if let Node::Operator {
        op: Op::Multiply,
        args: a,
        implicit: true,
        ..
    } = node
        && implicit != Some(Implicit::Show)
    {
        let ps = Some(p);
        let left = unwrap_paren(&a[0], ps);
        let cond1 = matches!(left, Node::Constant(_))
            && parent.is_some_and(|par| {
                identifier(par) == "OperatorNode:divide"
                    && args(par)
                        .first()
                        .is_some_and(|x| rule2_node(unwrap_paren(x, ps)))
            });
        let cond2 = identifier(left) == "OperatorNode:divide"
            && rule2_node(unwrap_paren(&args(left)[0], ps))
            && matches!(args(left)[1], Node::Constant(_));
        if !cond1 && !cond2 {
            prec = prec.map(|x| x + 1);
        }
    }
    prec
}

/// `getAssociativity(node, parenthesis)`.
fn associativity(n: &Node, p: Parenthesis) -> Option<Assoc> {
    let node = if p != Parenthesis::Keep { content(n) } else { n };
    let index = precedence(node, p, None, None)?;
    let id = identifier(node);
    if index == 14 {
        // the implicit-multiplication group
        return Some(Assoc::Left);
    }
    associativity_of(&id)
}

/// `isAssociativeWith(nodeA, nodeB, parenthesis)`, with its `nodeA` twice outside `'keep'`.
fn is_associative_with(a: &Node, b: &Node, p: Parenthesis) -> Option<bool> {
    let (a, b) = if p != Parenthesis::Keep {
        (content(a), content(a))
    } else {
        (a, b)
    };
    let id_a = identifier(a);
    let id_b = identifier(b);
    precedence(a, p, None, None)?;
    associative_with(&id_a).map(|list| list.contains(&id_b.as_str()))
}

fn starts_with_constant(n: &Node, p: Parenthesis) -> bool {
    let mut cur = n;
    if p == Parenthesis::Auto {
        cur = content(cur);
    }
    match cur {
        Node::Constant(_) => true,
        Node::Operator { args, .. } => args.first().is_some_and(|a| starts_with_constant(a, p)),
        _ => false,
    }
}

fn calculate_parens(root: &Node, p: Parenthesis, implicit: Implicit, a: &[Node]) -> Vec<bool> {
    let prec = precedence(root, p, Some(implicit), None);
    let assoc = associativity(root, p);
    let root_id = identifier(root);
    let add_or_mul = root_id == "OperatorNode:add" || root_id == "OperatorNode:multiply";
    if p == Parenthesis::All || (a.len() > 2 && !add_or_mul) {
        return a
            .iter()
            .map(|x| {
                !matches!(
                    content(x),
                    Node::Array(_) | Node::Constant(_) | Node::Symbol(_) | Node::Parenthesis(_)
                )
            })
            .collect();
    }
    let mut result: Vec<bool> = match a.len() {
        0 => Vec::new(),
        1 => {
            let operand = precedence(&a[0], p, Some(implicit), Some(root));
            vec![match (operand, prec) {
                (None, _) => false,
                (Some(o), Some(r)) => o <= r,
                (Some(_), None) => false,
            }]
        }
        2 => {
            let lhs_prec = precedence(&a[0], p, Some(implicit), Some(root));
            let assoc_lhs = is_associative_with(root, &a[0], p);
            let lhs = match lhs_prec {
                None => false,
                Some(l) if Some(l) == prec && assoc == Some(Assoc::Right) && assoc_lhs != Some(true) => true,
                Some(l) => prec.is_some_and(|r| l < r),
            };
            let rhs_prec = precedence(&a[1], p, Some(implicit), Some(root));
            let assoc_rhs = is_associative_with(root, &a[1], p);
            let rhs = match rhs_prec {
                None => false,
                Some(r) if Some(r) == prec && assoc == Some(Assoc::Left) && assoc_rhs != Some(true) => true,
                Some(r) => prec.is_some_and(|x| r < x),
            };
            vec![lhs, rhs]
        }
        _ => {
            // only `+` and `*` get here
            a.iter()
                .map(|x| {
                    let arg_prec = precedence(x, p, Some(implicit), Some(root));
                    let assoc_arg = is_associative_with(root, x, p);
                    let arg_assoc = associativity(x, p);
                    match arg_prec {
                        None => false,
                        Some(ap) if Some(ap) == prec && assoc == arg_assoc && assoc_arg != Some(true) => true,
                        Some(ap) => prec.is_some_and(|r| ap < r),
                    }
                })
                .collect()
        }
    };
    if a.len() >= 2
        && root_id == "OperatorNode:multiply"
        && matches!(root, Node::Operator { implicit: true, .. })
        && p != Parenthesis::All
        && implicit == Implicit::Hide
    {
        for i in 1..result.len() {
            if starts_with_constant(&a[i], p)
                && !result[i - 1]
                && (p != Parenthesis::Keep || !matches!(a[i - 1], Node::Parenthesis(_)))
            {
                result[i] = true;
            }
        }
    }
    result
}

fn operator_text(n: &Node, o: &PrintOptions) -> String {
    let Node::Operator {
        op,
        args: a,
        implicit: imp,
        ..
    } = n
    else {
        unreachable!()
    };
    let p = o.parenthesis.unwrap_or_default();
    let implicit = o.implicit.unwrap_or_default();
    let id = identifier(n);
    let add_or_mul = id == "OperatorNode:add" || id == "OperatorNode:multiply";
    if a.is_empty() || (a.len() > 2 && !add_or_mul) {
        // fallback to a function call, arguments in the default format
        let parts: Vec<String> = a.iter().map(|x| to_string(x, &PrintOptions::default())).collect();
        return format!("{}({})", op.name(), parts.join(", "));
    }
    let parens = calculate_parens(n, p, implicit, a);
    let wrap = |s: String, on: bool| if on { format!("({s})") } else { s };
    let symbol = op.symbol();
    match a.len() {
        1 => {
            let assoc = associativity(n, p);
            let operand = wrap(to_string(&a[0], o), parens[0]);
            let named = symbol.chars().any(|c| c.is_ascii_alphabetic());
            let space = if named { " " } else { "" };
            match assoc {
                Some(Assoc::Right) => format!("{symbol}{space}{operand}"),
                Some(Assoc::Left) => format!("{operand}{space}{symbol}"),
                None => format!("{operand}{symbol}"),
            }
        }
        2 => {
            let lhs = wrap(to_string(&a[0], o), parens[0]);
            let rhs = wrap(to_string(&a[1], o), parens[1]);
            if *imp && *op == Op::Multiply && implicit == Implicit::Hide {
                format!("{lhs} {rhs}")
            } else {
                format!("{lhs} {symbol} {rhs}")
            }
        }
        _ => {
            let parts: Vec<String> = a
                .iter()
                .enumerate()
                .map(|(i, x)| wrap(to_string(x, o), parens[i]))
                .collect();
            if *imp && *op == Op::Multiply && implicit == Implicit::Hide {
                parts.join(" ")
            } else {
                parts.join(&format!(" {symbol} "))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_in_auto_notation() {
        let cases = [
            (1e5, "1e+5"),
            (12345.0, "12345"),
            (0.001, "0.001"),
            (0.0001, "1e-4"),
            (1e21, "1e+21"),
            (-0.0, "0"),
            (123456.7, "1.234567e+5"),
            (1.0 / 3.0, "0.3333333333333333"),
            (-2.5, "-2.5"),
            (100.0, "100"),
        ];
        for (v, want) in cases {
            assert_eq!(format_number(v), want, "{v}");
        }
    }
}
