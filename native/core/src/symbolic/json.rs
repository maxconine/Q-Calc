//! The golden recorder's tree format (`native/golden-capture/symbolic.ts`) to and from [`Node`]:
//! `{"C": value}`, `{"S": name}`, `{"O": fn, "op": op, "a": [...], "i"?: true, "p"?: true}`,
//! `{"F": name | tree, "a": [...]}`, `{"P": tree}`.

use serde_json::{Map, Value as Json, json};

use super::simplify::operator_for;
use super::{Constant, Node, num};

/// A recorded tree as a node; `Err` for nodes the recorder couldn't write (`{"X": …}`).
pub fn from_tree(j: &Json) -> Result<Node, String> {
    let o = j.as_object().ok_or("tree: not an object")?;
    if let Some(v) = o.get("C") {
        return Ok(match v {
            Json::Number(n) => num(n.as_f64().unwrap_or(f64::NAN)),
            Json::String(s) => Node::Constant(Constant::Str(s.clone())),
            Json::Bool(b) => Node::Constant(Constant::Bool(*b)),
            Json::Null => Node::Constant(Constant::Null),
            Json::Object(m) if m.contains_key("$f") => num(match m["$f"].as_str() {
                Some("Infinity") => f64::INFINITY,
                Some("-Infinity") => f64::NEG_INFINITY,
                Some("-0") => -0.0,
                _ => f64::NAN,
            }),
            Json::Object(m) if m.contains_key("$undef") => Node::Constant(Constant::Undefined),
            _ => return Err("tree: constant".into()),
        });
    }
    if let Some(Json::String(s)) = o.get("S") {
        return Ok(Node::Symbol(s.clone()));
    }
    if let Some(c) = o.get("P") {
        return Ok(Node::Parenthesis(Box::new(from_tree(c)?)));
    }
    let args = || -> Result<Vec<Node>, String> {
        o.get("a")
            .and_then(Json::as_array)
            .ok_or("tree: args")?
            .iter()
            .map(from_tree)
            .collect()
    };
    if let Some(Json::String(f)) = o.get("O") {
        let op = operator_for(f).ok_or_else(|| format!("tree: operator {f}"))?;
        if o.get("op")
            .and_then(Json::as_str)
            .is_some_and(|s| s != op.symbol())
        {
            return Err(format!("tree: operator {f} written as {}", o["op"]));
        }
        return Ok(Node::Operator {
            op,
            args: args()?,
            implicit: o.get("i").and_then(Json::as_bool).unwrap_or(false),
            percentage: o.get("p").and_then(Json::as_bool).unwrap_or(false),
        });
    }
    if let Some(f) = o.get("F") {
        let callee = match f {
            Json::String(s) => Node::Symbol(s.clone()),
            other => from_tree(other)?,
        };
        return Ok(Node::Function {
            callee: Box::new(callee),
            args: args()?,
        });
    }
    Err(format!("tree: unreplayable {j}"))
}

fn number(v: f64) -> Json {
    if v.is_finite() { json!(v) } else { Json::Null }
}

/// A node in the recorder's tree format (non-finite constants as `null`, which the golden compare accepts for
/// `{"$f": …}`).
pub fn to_tree(n: &Node) -> Json {
    match n {
        Node::Constant(c) => json!({ "C": match c {
            Constant::Number { value, .. } => number(*value),
            Constant::Str(s) => json!(s),
            Constant::Bool(b) => json!(b),
            Constant::Null | Constant::Undefined => Json::Null,
        }}),
        Node::Symbol(s) => json!({ "S": s }),
        Node::Parenthesis(c) => json!({ "P": to_tree(c) }),
        Node::Operator {
            op,
            args,
            implicit,
            percentage,
        } => {
            let mut m = Map::new();
            m.insert("O".into(), json!(op.name()));
            m.insert("op".into(), json!(op.symbol()));
            m.insert("a".into(), Json::Array(args.iter().map(to_tree).collect()));
            if *implicit {
                m.insert("i".into(), json!(true));
            }
            if *percentage {
                m.insert("p".into(), json!(true));
            }
            Json::Object(m)
        }
        Node::Function { callee, args } => {
            let f = match callee.as_ref() {
                Node::Symbol(s) => json!(s),
                other => to_tree(other),
            };
            json!({ "F": f, "a": args.iter().map(to_tree).collect::<Vec<_>>() })
        }
        other => {
            json!({ "X": super::print::type_name(other), "s": super::to_string(other, &super::PrintOptions::default()) })
        }
    }
}
