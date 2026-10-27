//! The line pipeline (`src/engine/evaluate.ts`): every line of a sheet through each reader in the TS order, with
//! variables, functions, measures, unit quantities and `ans` carried from line to line.
//!
//! Readers other packages port are called through `crate::stubs`, which answer "not mine" until then.

use std::collections::BTreeMap;

use crate::format::{DEFAULT_SIG_FIGS, format_as_fraction, format_value};
use crate::jsnum;
use crate::jsre::{self, Lazy};
use crate::latex::{latex_to_ascii, looks_like_latex};
use crate::plain_math::{PlainContext, try_plain_math, with_space_thousands};
use crate::scientific::{NAMES, ScientificContext, names_pattern, split_glued_functions};
use crate::simplify::{ExactFormOptions, exact_form, wants_exact_form};
use crate::stubs::{self, SolveContext, Solved};
use crate::types::{
    AngleMode, EvaluateOptions, LineKind, LineResult, Meas, UserFunction, Value, ValueKind,
};
use crate::typst_input::typst_to_ascii;
use crate::units::{is_improper_unit_conversion, quantity_text, reads_as_unit, try_convert};

fn js(p: &str, f: &str) -> jsre::Regex {
    jsre::js(p, f)
}

/// Capital E is free (Young's modulus, energy); lowercase e stays Euler's number.
fn is_reserved(name: &str) -> bool {
    name != "E"
        && (name.eq_ignore_ascii_case("e")
            || NAMES.split('|').any(|n| n.eq_ignore_ascii_case(name)))
}

/// `x = 5`: built-in names like `pi` can't be assigned.
pub fn parse_assignment(trimmed: &str) -> Option<(String, String)> {
    static RE: Lazy = Lazy::new(|| js(r"^([A-Za-z][A-Za-z0-9]*)\s*=\s*(.+)$", ""));
    let c = RE.captures(trimmed).ok()??;
    let variable = jsre::group(&c, 1);
    if is_reserved(variable) {
        return None;
    }
    Some((variable.to_string(), jsre::group(&c, 2).trim().to_string()))
}

/// `2+3=` is `2+3`; only one `=` goes, and never the end of `==`, `<=`, `>=` or `!=`.
pub fn strip_trailing_equals(trimmed: &str) -> String {
    static RE: Lazy = Lazy::new(|| js(r"^([\s\S]*[^=<>!\s])\s*=$", ""));
    match RE.captures(trimmed) {
        Ok(Some(c)) => jsre::group(&c, 1).to_string(),
        _ => trimmed.to_string(),
    }
}

/// `f(x, y) = …`: name, parameters and body.
pub fn parse_function_def(trimmed: &str) -> Option<(String, Vec<String>, String)> {
    static RE: Lazy = Lazy::new(|| {
        js(
            r"^([A-Za-z][A-Za-z0-9]*)\s*\(([\s\S]*)\)\s*=\s*([\s\S]+)$",
            "",
        )
    });
    static PARAM: Lazy = Lazy::new(|| js(r"^[A-Za-z][A-Za-z0-9]*$", ""));
    let c = RE.captures(trimmed).ok()??;
    let name = jsre::group(&c, 1);
    if is_reserved(name) {
        return None;
    }
    let param_text = jsre::group(&c, 2).trim();
    let params: Vec<String> = if param_text.is_empty() {
        Vec::new()
    } else {
        param_text
            .split(',')
            .map(|p| p.trim().to_string())
            .collect()
    };
    if params
        .iter()
        .any(|p| !jsre::test(&PARAM, p) || is_reserved(p))
    {
        return None;
    }
    let lower: std::collections::HashSet<String> =
        params.iter().map(|p| p.to_lowercase()).collect();
    if lower.len() != params.len() {
        return None;
    }
    let body = jsre::group(&c, 3).trim();
    if body.is_empty() {
        return None;
    }
    Some((name.to_string(), params, body.to_string()))
}

fn with_unit(text: &str, unit: Option<&str>) -> String {
    match unit {
        Some(u) if !u.is_empty() => format!("{text} {u}"),
        _ => text.to_string(),
    }
}

/// `d * 2` with `d = 5 cm` becomes `(5 cm) * 2`, so the unit reader sees the unit.
fn with_quantities(expr: &str, quantities: &BTreeMap<String, String>) -> String {
    if quantities.is_empty() {
        return expr.to_string();
    }
    let names: Vec<String> = quantities.keys().cloned().collect();
    let re = jsre::js_cached(
        &format!(
            r"(?<![A-Za-z_])(?:{})(?![A-Za-z0-9_])",
            names_pattern(&names)
        ),
        "",
    );
    jsre::replace_all(&re, expr, |c| {
        format!(
            "({})",
            quantities.get(jsre::group(c, 0)).map_or("", String::as_str)
        )
    })
    .into_owned()
}

/// `ans ft` when ans is already 5 m reads as ans in feet: a unit of the same kind converts.
fn ans_in_unit(expr: &str, ans_qty: Option<&str>, opts: &EvaluateOptions) -> String {
    static RE: Lazy = Lazy::new(|| {
        js(
            r"^ans\s+(?:(?:to|into|in)\s+(.+)|([A-Za-zµμ°Ω].*?)(?:\s+(?:to|into|in)\s+(.+))?)$",
            "i",
        )
    });
    let Some(q) = ans_qty else {
        return expr.to_string();
    };
    let Ok(Some(c)) = RE.captures(expr) else {
        return expr.to_string();
    };
    let kind = if c.get(1).is_some() {
        jsre::group(&c, 1)
    } else {
        jsre::group(&c, 2)
    };
    let target = if c.get(1).is_some() {
        jsre::group(&c, 1)
    } else if c.get(3).is_some() {
        jsre::group(&c, 3)
    } else {
        kind
    };
    for (left, out) in [
        (format!("({q})"), format!("ans to {target}")),
        (q.to_string(), format!("{q} to {target}")),
    ] {
        let converted = try_convert(
            &format!("{left} to {kind}"),
            &opts.default_units,
            &BTreeMap::new(),
            false,
            opts.angle(),
        );
        if converted.is_some_and(|v| v.kind == ValueKind::Number) {
            return out;
        }
    }
    expr.to_string()
}

fn show(value: &Value, fraction_mode: bool, sig_figs: f64) -> String {
    if value.kind == ValueKind::Text
        && let Some(t) = &value.text
        && !t.is_empty()
    {
        return t.clone();
    }
    if fraction_mode
        && value.kind == ValueKind::Number
        && let Some(f) = format_as_fraction(value.n, 10_000)
    {
        return with_unit(&f, value.unit.as_deref());
    }
    format_value(value, sig_figs)
}

/// Whether `expr` names a variable (or ans) that carries a ±.
fn uses_uncertain(expr: &str, measures: &BTreeMap<String, Meas>) -> bool {
    measures.iter().any(|(name, m)| {
        m.unc.is_some_and(|u| u != 0.0) && {
            let re = js(
                &format!(
                    r"(?<![A-Za-z0-9_]){}(?![A-Za-z0-9_])",
                    fancy_regex::escape(name)
                ),
                "",
            );
            jsre::test(&re, expr)
        }
    })
}

fn set_or_delete<T>(map: &mut BTreeMap<String, T>, key: &str, value: Option<T>) {
    match value {
        Some(v) => {
            map.insert(key.to_string(), v);
        }
        None => {
            map.remove(key);
        }
    }
}

#[derive(Clone, Copy)]
enum Base {
    Two,
    Eight,
    Ten,
    Sixteen,
}

impl Base {
    fn from_word(w: &str) -> Option<Base> {
        Some(match w.to_lowercase().as_str() {
            "hex" | "hexadecimal" => Base::Sixteen,
            "binary" | "bin" => Base::Two,
            "octal" | "oct" => Base::Eight,
            "decimal" => Base::Ten,
            _ => return None,
        })
    }
}

/// `<expr> in hex`, `<expr> to binary`, `hex(<expr>)`, `bin(…)`, `oct(…)`.
fn base_request(expr: &str) -> Option<(String, Base)> {
    static TAIL: Lazy = Lazy::new(|| {
        js(
            r"^([\s\S]*\S)\s+(?:in|to|as|into)\s+(hex|hexadecimal|binary|bin|octal|oct|decimal)\s*$",
            "i",
        )
    });
    static CALL: Lazy = Lazy::new(|| js(r"^\s*(hex|bin|oct)\s*\(([\s\S]*)\)\s*$", "i"));
    if let Ok(Some(c)) = TAIL.captures(expr) {
        return Some((
            jsre::group(&c, 1).to_string(),
            Base::from_word(jsre::group(&c, 2))?,
        ));
    }
    let c = CALL.captures(expr).ok()??;
    let inner = jsre::group(&c, 2);
    // the bracket that opens the call must be the one that closes the line
    let mut depth = 0i32;
    for ch in inner.chars() {
        if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth -= 1;
            if depth < 0 {
                return None;
            }
        }
    }
    if depth != 0 {
        return None;
    }
    Some((inner.to_string(), Base::from_word(jsre::group(&c, 1))?))
}

/// A whole number written in the base: 255 → 0xFF, 10 → 0b1010, -8 → -0o10.
fn in_base(n: f64, base: Base) -> Option<String> {
    let whole = jsnum::round(n);
    if !jsnum::is_safe_integer(whole) || (n - whole).abs() > 1e-9 * n.abs().max(1.0) {
        return None;
    }
    let v = whole.abs() as u64;
    let digits = match base {
        Base::Two => format!("0b{v:b}"),
        Base::Eight => format!("0o{v:o}"),
        Base::Ten => v.to_string(),
        Base::Sixteen => format!("0x{v:X}"),
    };
    Some(format!("{}{digits}", if whole < 0.0 { "-" } else { "" }))
}

/// Every line of a sheet, each seeing what the lines above it defined.
pub fn evaluate_sheet(lines: &[String], options: &EvaluateOptions) -> Vec<LineResult> {
    with_space_thousands(options.space_thousands.unwrap_or(false), || {
        Sheet::new(options).run(lines)
    })
}

/// One line on its own.
pub fn evaluate_line(text: &str, options: &EvaluateOptions) -> LineResult {
    evaluate_sheet(&[text.to_string()], options)
        .pop()
        .unwrap_or_else(|| LineResult::empty(text))
}

struct Sheet<'o> {
    options: &'o EvaluateOptions,
    angle_mode: AngleMode,
    fraction_mode: bool,
    sig_figs: f64,
    sig_fig_mode: bool,
    measures: BTreeMap<String, Meas>,
    variables: BTreeMap<String, f64>,
    quantities: BTreeMap<String, String>,
    functions: BTreeMap<String, UserFunction>,
    last_ans: Option<f64>,
}

impl<'o> Sheet<'o> {
    fn new(options: &'o EvaluateOptions) -> Self {
        Sheet {
            options,
            angle_mode: options.angle(),
            fraction_mode: options.fraction_mode.unwrap_or(false),
            sig_figs: options.sig_figs.unwrap_or(f64::from(DEFAULT_SIG_FIGS)),
            sig_fig_mode: options.sig_fig_mode.unwrap_or(false),
            measures: options.measures.clone(),
            variables: options.variables.clone(),
            quantities: options.quantities.clone(),
            functions: options.functions.clone(),
            last_ans: options.ans,
        }
    }

    fn known(&self, name: &str) -> bool {
        self.variables.contains_key(name)
            || self.quantities.contains_key(name)
            || self.measures.contains_key(name)
    }

    fn run(mut self, lines: &[String]) -> Vec<LineResult> {
        lines
            .iter()
            .map(|raw| crate::symbolic::simplify::with_line_budget(|| self.line(raw)))
            .collect()
    }

    fn sci(&self) -> ScientificContext {
        ScientificContext {
            ans: self.last_ans,
            angle_mode: self.angle_mode,
            variables: self.variables.clone(),
            functions: self.functions.clone(),
            depth: 0,
        }
    }

    // WP-MEASURE: the TS `ctx` that `measure` reads
    fn measure_ctx(&self) -> stubs::MeasureContext<'_> {
        stubs::MeasureContext {
            ans: self.last_ans,
            angle_mode: self.angle_mode,
            variables: Some(&self.variables),
            functions: Some(&self.functions),
            measures: Some(&self.measures),
        }
    }

    fn plain(&self, keep_units: bool) -> PlainContext {
        PlainContext {
            sci: self.sci(),
            default_units: self.options.default_units.clone(),
            keep_units,
        }
    }

    fn push_solved(&mut self, raw: &str, solved: &Solved) -> LineResult {
        let (display, exact) = stubs::format_solve(solved, self.sig_figs, self.fraction_mode);
        let info = &solved.info;
        let single = info.outcome == crate::types::SolveOutcome::Roots
            && info.roots.len() == 1
            && info.more != Some(true);
        // the root is never stored as the variable; `ans` carries it
        if single {
            self.last_ans = info.roots.first().copied();
            self.measures.remove("ans");
            self.quantities.remove("ans");
        }
        let value = if single {
            Value::number(info.roots[0])
        } else {
            Value::text(display.clone())
        };
        LineResult {
            raw: raw.into(),
            kind: LineKind::Solve,
            value: Some(value),
            display,
            exact,
            solve: Some(info.clone()),
            ..LineResult::default()
        }
    }

    fn line(&mut self, raw: &str) -> LineResult {
        static DOLLAR_LATEX: Lazy = Lazy::new(|| js(r"\$([^$]*\\[A-Za-z][^$]*)\$", ""));
        static ISOLATE: Lazy = Lazy::new(|| js(r"\bisolate\b", "i"));
        // `\dot\theta`, `dot(theta)` and `θ̇` all read as `thetadot` before anything else rewrites them;
        // LaTeX pasted inside a line loses its $ signs, money (`$10`) has no backslash
        let unwrapped =
            jsre::replace_all(&DOLLAR_LATEX, raw.trim(), |c| jsre::group(c, 1).to_string())
                .into_owned();
        let source = strip_trailing_equals(&typst_to_ascii(&stubs::join_dots(&unwrapped)));
        let known = |n: &str| self.known(n);
        let line = split_glued_functions(
            &if looks_like_latex(&source) {
                latex_to_ascii(&source)
            } else {
                source.clone()
            },
            &known,
        );
        // the input field turns a typed theta into θ, which is also a variable name; solve keeps θ as its unknown
        let trimmed_line = split_glued_functions(&line.replace('θ', "theta"), &known);
        if trimmed_line.is_empty() {
            return LineResult::empty(raw);
        }

        // `solve({eq1, eq2}, x = 1, y = 2)`
        if let Some(call) = stubs::parse_system_call(&line) {
            let answer = stubs::solve_system_call(
                &call.equations,
                &call.guess_text,
                self.angle_mode,
                &self.options.default_units,
            );
            return LineResult {
                raw: raw.into(),
                value: answer.clone().map(Value::text),
                display: answer.unwrap_or_default(),
                ..LineResult::blank(raw)
            };
        }
        // `isolate x in …` rearranges symbolically; stored values stay letters
        if jsre::test(&ISOLATE, &line) {
            return match stubs::isolate_variable(&line, None) {
                Some(d) => LineResult {
                    value: Some(Value::text(d.clone())),
                    display: d,
                    ..LineResult::blank(raw)
                },
                None => LineResult::blank(raw),
            };
        }
        // `solve x in …` or `… for x`
        if let Some(named) = (!stubs::has_plus_minus(&line))
            .then(|| stubs::parse_named_solve(&line))
            .flatten()
        {
            return self.named_solve(raw, &named);
        }

        if let Some((name, params, body)) = parse_function_def(&trimmed_line) {
            let known = |n: &str| params.iter().any(|p| p == n) || self.known(n);
            let body = stubs::split_letters(&body, &known, &params);
            self.functions.insert(
                name.clone(),
                UserFunction {
                    params: params.clone(),
                    body: body.clone(),
                },
            );
            return LineResult {
                raw: raw.into(),
                kind: LineKind::Function,
                display: format!("{name}({}) = {body}", params.join(", ")),
                fn_name: Some(name),
                fn_params: Some(params),
                fn_body: Some(body),
                ..LineResult::default()
            };
        }

        // the gas constant has no mol unit to carry, so `ans` after it is the bare number
        if let Some(gas) = stubs::gas_constant(&trimmed_line) {
            let value = Value {
                unit: Some(gas.unit),
                ..Value::number(gas.n)
            };
            self.last_ans = Some(gas.n);
            self.measures.remove("ans");
            self.quantities.remove("ans");
            let display = show(&value, false, self.sig_figs);
            return LineResult {
                raw: raw.into(),
                display,
                value: Some(value),
                ..LineResult::blank(raw)
            };
        }
        let is_name = |n: &str| {
            self.variables.contains_key(n)
                || self.quantities.contains_key(n)
                || self.functions.contains_key(n)
        };
        if let Some(molar) = stubs::molar_mass_answer(&trimmed_line, &is_name) {
            self.last_ans = Some(molar.mass);
            self.measures.remove("ans");
            self.quantities.remove("ans");
            return LineResult {
                raw: raw.into(),
                display: molar.display,
                value: Some(Value::number(molar.mass)),
                ..LineResult::blank(raw)
            };
        }
        if let Some(chem) = stubs::chem_answer(&trimmed_line, &is_name) {
            let value = (!chem.is_empty()).then(|| Value::text(chem.clone()));
            return LineResult {
                raw: raw.into(),
                display: chem,
                value,
                ..LineResult::blank(raw)
            };
        }

        let known = |n: &str| self.known(n);
        let typed = stubs::split_letters(&line, &known, &[]);
        let trimmed = stubs::split_letters(&trimmed_line, &known, &[]);
        let assign = parse_assignment(&trimmed);
        // `V = 12 V` stores 12 volts; as an equation it could only ever say V = 0
        let fn_names: Vec<String> = self.functions.keys().cloned().collect();
        let unit_self = assign.as_ref().is_some_and(|(variable, expr)| {
            reads_as_unit(variable)
                && !stubs::looks_like_matrix(&with_quantities(expr, &self.quantities), &fn_names)
                && try_convert(
                    expr,
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                    false,
                    self.angle_mode,
                )
                .is_some_and(|v| v.unit.is_some())
        });
        if unit_self && let Some((variable, _)) = &assign {
            self.variables.remove(variable);
            self.quantities.remove(variable);
            self.measures.remove(variable);
        }
        // WP-MATRIX: an entry like `sin(30)` is read by the calculator (`scalar` in the TS)
        let scalar_ctx = PlainContext {
            sci: self.sci(),
            default_units: Default::default(),
            keep_units: false,
        };
        let scalar = |text: &str| {
            try_plain_math(text, &scalar_ctx)
                .filter(|v| v.kind == ValueKind::Number && v.unit.is_none())
                .map(|v| v.n)
        };
        if !unit_self
            && let Some(m) = stubs::matrix_answer(
                &with_quantities(
                    assign.as_ref().map_or(&trimmed, |(_, e)| e),
                    &self.quantities,
                ),
                &stubs::MatrixOptions {
                    variables: &self.variables,
                    sig_figs: self.sig_figs,
                    fraction_mode: self.fraction_mode,
                    functions: &fn_names,
                    scalar: Some(&scalar),
                },
            )
        {
            return self.matrix_line(raw, m, assign.as_ref().map(|(v, _)| v.as_str()));
        }
        if !stubs::has_plus_minus(&trimmed) && !unit_self {
            let eq = with_quantities(&typed, &self.quantities);
            // `5 m = x` is an assignment written backwards, not a solve
            static BARE_SIDE: Lazy = Lazy::new(|| js(r"^\s*[A-Za-zθ][A-Za-z0-9_]*\s*$", ""));
            let bare_side = eq.split('=').any(|side| jsre::test(&BARE_SIDE, side));
            if stubs::is_equation(&eq)
                && !bare_side
                && let Some(r) = self.unit_solved(raw, &eq, None, None)
            {
                return r;
            }
            let ctx = SolveContext {
                ans: self.last_ans,
                angle_mode: self.angle_mode,
                variables: &self.variables,
                functions: &self.functions,
                rationalize: self.options.rationalize,
            };
            if let Some(solved) = stubs::solve_equation(&eq, &ctx) {
                // solve can't carry a ±, and a bare root would look exact
                if uses_uncertain(&typed, &self.measures) {
                    return LineResult::blank(raw);
                }
                return self.push_solved(raw, &solved);
            }
        }
        self.expression(raw, &trimmed, assign)
    }

    fn named_solve(&mut self, raw: &str, named: &stubs::NamedSolve) -> LineResult {
        let unit_eq = with_quantities(&named.eq, &self.quantities);
        if let Some(r) =
            self.unit_solved(raw, &unit_eq, Some(&named.variable), named.near.as_deref())
        {
            return r;
        }
        if let Some(formula) = stubs::isolate_with_units(&unit_eq, &named.variable, &self.variables)
        {
            return LineResult {
                value: Some(Value::text(formula.clone())),
                display: formula,
                ..LineResult::blank(raw)
            };
        }
        let names: Vec<String> = std::iter::once(named.variable.clone())
            .chain(self.variables.keys().cloned())
            .collect();
        if stubs::clearly_units_blank(&unit_eq, &names) {
            return LineResult::blank(raw);
        }
        let known = |n: &str| self.known(n);
        let typed_named = stubs::split_letters(&named.numeric, &known, &[]);
        if uses_uncertain(&typed_named, &self.measures) {
            return LineResult::blank(raw);
        }
        let eq = with_quantities(&typed_named, &self.quantities);
        let ctx = SolveContext {
            ans: self.last_ans,
            angle_mode: self.angle_mode,
            variables: &self.variables,
            functions: &self.functions,
            rationalize: self.options.rationalize,
        };
        let solved = stubs::solve_equation(&eq, &ctx);
        // a formula beats "no solution found", which only means the search came up empty
        if let Some(s) = &solved
            && s.info.outcome != crate::types::SolveOutcome::NoneFound
        {
            return self.push_solved(raw, s);
        }
        if let Some(d) = stubs::isolate_variable(&named.isolate, Some(&self.variables)) {
            return LineResult {
                value: Some(Value::text(d.clone())),
                display: d,
                ..LineResult::blank(raw)
            };
        }
        match solved {
            Some(s) => self.push_solved(raw, &s),
            None => LineResult::blank(raw),
        }
    }

    fn unit_solved(
        &mut self,
        raw: &str,
        eq: &str,
        variable: Option<&str>,
        near: Option<&str>,
    ) -> Option<LineResult> {
        // ---- WP-SOLVE: solveWithUnitsOne's ctx ----
        let names: Vec<String> = (self.variables.keys())
            .chain(self.quantities.keys())
            .chain(self.functions.keys())
            .cloned()
            .collect();
        let ctx = stubs::UnitCtx {
            solve: SolveContext {
                ans: self.last_ans,
                angle_mode: self.angle_mode,
                variables: &self.variables,
                functions: &self.functions,
                rationalize: self.options.rationalize,
            },
            variable,
            near,
            names: &names,
            defaults: &self.options.default_units,
            sig_figs: self.sig_figs,
        };
        let u = stubs::solve_with_units_one(eq, &ctx)?;
        let Some(info) = u.info else {
            return Some(LineResult {
                display: u.display,
                ..LineResult::blank(raw)
            });
        };
        if let Some(v) = &u.value {
            // `ans` is the quantity, like any unit answer
            self.measures.remove("ans");
            set_or_delete(
                &mut self.quantities,
                "ans",
                quantity_text(v, self.angle_mode),
            );
            self.last_ans = None;
        }
        let value = u.value.unwrap_or_else(|| Value::text(u.display.clone()));
        Some(LineResult {
            raw: raw.into(),
            kind: LineKind::Solve,
            value: Some(value),
            display: u.display,
            solve: Some(info),
            ..LineResult::default()
        })
    }

    fn matrix_line(
        &mut self,
        raw: &str,
        m: stubs::MatrixAnswer,
        assigned: Option<&str>,
    ) -> LineResult {
        let variable = if m.message {
            None
        } else {
            assigned.map(str::to_string)
        };
        if let Some(v) = &variable {
            // a matrix is stored as its literal, the way a unit answer is
            self.measures.remove(v);
            set_or_delete(&mut self.variables, v, m.n);
            set_or_delete(&mut self.quantities, v, m.literal.clone());
        }
        if let Some(n) = m.n {
            self.last_ans = Some(n);
            self.measures.remove("ans");
            self.quantities.remove("ans");
        } else if let Some(lit) = &m.literal {
            self.measures.remove("ans");
            self.quantities.insert("ans".into(), lit.clone());
        }
        LineResult {
            raw: raw.into(),
            kind: if variable.is_some() {
                LineKind::Assignment
            } else {
                LineKind::Expression
            },
            value: Some(m.n.map_or_else(|| Value::text(m.display.clone()), Value::number)),
            display: m.display,
            exact: m.exact,
            quantity: m.literal,
            variable,
            ..LineResult::default()
        }
    }

    fn expression(
        &mut self,
        raw: &str,
        trimmed: &str,
        assign: Option<(String, String)>,
    ) -> LineResult {
        let variable = assign.as_ref().map(|(v, _)| v.clone());
        let kind = if variable.is_some() {
            LineKind::Assignment
        } else {
            LineKind::Expression
        };
        let blank = |variable: Option<String>| LineResult {
            raw: raw.into(),
            kind,
            variable,
            ..LineResult::default()
        };
        let body = assign.as_ref().map_or(trimmed, |(_, e)| e.as_str());
        // ∓ is treated as ± until correlation is modelled
        let mut expr = stubs::normalize_sums(&with_quantities(
            &ans_in_unit(
                body,
                self.quantities.get("ans").map(String::as_str),
                self.options,
            ),
            &self.quantities,
        ))
        .replace('∓', "±");
        // `255 in hex`, `hex(255)`: worked out as usual, then written in that base
        let based = if variable.is_some() {
            None
        } else {
            base_request(&expr)
        };
        if let Some((left, _)) = &based {
            expr = left.clone();
        }

        let plus_minus = stubs::has_plus_minus(&expr);
        let mut value: Option<Value>;
        let mut calc: Option<stubs::CalculusResult> = None;
        let mut sum: Option<stubs::SumAnswer> = None;
        // WP-CALCULUS: calculus reads the sheet's context
        let calc_ctx = stubs::CalcContext {
            sci: self.sci(),
            time_varying: self.options.time_varying.clone(),
        };
        if !plus_minus {
            calc = stubs::evaluate_calculus(&expr, &calc_ctx);
            // a scalar or other term beside an integral is not a calculus line, so the integral is folded in
            for _ in 0..8 {
                if calc.as_ref().is_some_and(|c| c.value.is_some()) {
                    break;
                }
                match stubs::embed_integrals(&expr, &calc_ctx) {
                    stubs::Embedded::Failed => {
                        calc = Some(stubs::CalculusResult {
                            value: None,
                            exact: None,
                            job: None,
                            time: None,
                        });
                        break;
                    }
                    stubs::Embedded::Nothing => break,
                    stubs::Embedded::Rewritten(next) => {
                        expr = next;
                        calc = stubs::evaluate_calculus(&expr, &calc_ctx);
                    }
                }
            }
        }
        if calc.is_none() {
            // WP-SUMS: the TS passes `{ ...ctx, defaultUnits, rationalize }`
            sum = stubs::sum_answer(
                &expr,
                &stubs::SumContext {
                    sci: self.sci(),
                    default_units: self.options.default_units.clone(),
                    rationalize: self.options.rationalize,
                },
            );
        }
        let m = if plus_minus {
            stubs::measure(&expr, &self.measure_ctx())
        } else {
            None
        };
        if let Some(c) = &calc {
            value = c.value.clone();
        } else if let Some(m) = &m {
            value = Some(Value::number(m.v));
        } else if let Some(s) = &sum {
            value = s.value.clone(); // WP-SUMS
        } else {
            value = try_plain_math(&expr, &self.plain(variable.is_some()));
        }
        // a ± answer that lost its uncertainty would be a confidently wrong bare number
        if plus_minus
            && m.is_none()
            && value.as_ref().is_some_and(|v| {
                v.kind == ValueKind::Number && v.meas.as_ref().and_then(|x| x.unc).is_none()
            })
        {
            value = None;
        }
        // functions among typed units, which the unit reader can't call: each call stands in as its number
        let improper = value.as_ref().is_some_and(|v| {
            v.kind == ValueKind::Text
                && is_improper_unit_conversion(v.text.as_deref().unwrap_or(""))
        });
        if (value.is_none() || improper) && !plus_minus {
            let plain = self.plain(false);
            let call_value = |call: &str| {
                try_plain_math(call, &plain)
                    .filter(|v| v.kind == ValueKind::Number && v.unit.is_none())
                    .map(|v| v.n)
            };
            if let Some(worked) = stubs::calls_worked_out(&expr, self.angle_mode, &call_value)
                && let Some(second) = try_plain_math(&worked, &self.plain(variable.is_some()))
            {
                static HALF_UNIT: Lazy = Lazy::new(|| js(r"\^-?\d*\.\d", ""));
                // a root that leaves half a unit (sqrt(5 m) as m^0.5) is no answer
                if !(second.kind == ValueKind::Number
                    && jsre::test(&HALF_UNIT, second.unit.as_deref().unwrap_or("")))
                {
                    value = Some(second);
                }
            }
        }
        let Some(mut value) = value else {
            return blank(variable);
        };

        let want_meas = self.sig_fig_mode
            || plus_minus
            || variable.is_some()
            || self
                .measures
                .values()
                .any(|m| m.unc.is_some_and(|u| u != 0.0));
        let meas = if calc.is_some()
            || !want_meas
            || value.kind != ValueKind::Number
            || !value.n.is_finite()
        {
            None
        } else if value.unit.is_some() || value.meas.is_some() {
            value.meas.clone()
        } else {
            stubs::measure(&expr, &self.measure_ctx())
                .filter(|m| (m.v - value.n).abs() <= 1e-9 * value.n.abs().max(1.0))
                .and_then(|m| m.meas)
        };
        // a ± that went in and didn't come out would show a bare number as if exact
        if meas.as_ref().and_then(|m| m.unc).is_none_or(|u| u == 0.0)
            && uses_uncertain(body, &self.measures)
        {
            return blank(variable);
        }
        // an inverse trig answer is an angle, so it says which
        if calc.is_none()
            && sum.is_none()
            && value.kind == ValueKind::Number
            && value.unit.is_none()
            && value.n.is_finite()
            && stubs::angle_exponent(&expr) == Some(1)
        {
            let mode = if self.angle_mode == AngleMode::Rad {
                "rad"
            } else {
                "deg"
            };
            value.unit = Some(mode.into());
            value.unit_id = Some(mode.into());
        }
        let finite = value.kind == ValueKind::Number && value.n.is_finite();
        // '' (a unit the reader can't read back) makes later uses blank rather than unit-less;
        // a complex answer chains as its own text at full precision
        let quantity = if finite && value.unit.is_some() {
            Some(quantity_text(&value, self.angle_mode).unwrap_or_default())
        } else {
            value.complex.map(|z| {
                format!(
                    "{} {} {}i",
                    jsnum::to_string(z.re),
                    if z.im < 0.0 { '-' } else { '+' },
                    jsnum::to_string(z.im.abs())
                )
            })
        };
        if value.complex.is_some()
            && let Some(q) = &quantity
            && !q.is_empty()
        {
            self.measures.remove("ans");
            self.quantities.insert("ans".into(), q.clone());
            if let Some(v) = &variable {
                self.variables.remove(v);
                self.quantities.insert(v.clone(), q.clone());
                self.measures.remove(v);
            }
        }
        if finite {
            let plain_meas = if quantity.is_none() {
                meas.clone()
            } else {
                None
            };
            self.last_ans = Some(value.n);
            set_or_delete(&mut self.measures, "ans", plain_meas.clone());
            set_or_delete(&mut self.quantities, "ans", quantity.clone());
            if let Some(v) = &variable {
                set_or_delete(
                    &mut self.variables,
                    v,
                    quantity.is_none().then_some(value.n),
                );
                set_or_delete(&mut self.quantities, v, quantity.clone());
                set_or_delete(&mut self.measures, v, plain_meas);
            }
        }

        let measured_text = meas
            .as_ref()
            .and_then(|m| stubs::format_measured(value.n, m, self.sig_fig_mode));
        let mut display = match &measured_text {
            Some(t) => with_unit(t, value.unit.as_deref()),
            None => show(&value, self.fraction_mode, self.sig_figs),
        };
        if let Some((_, base)) = based {
            // only a whole number without a unit has a hex or binary form
            let written = (value.kind == ValueKind::Number && value.unit.is_none())
                .then(|| in_base(value.n, base))
                .flatten();
            match written {
                Some(w) => display = w,
                None => return LineResult::blank(raw),
            }
        }
        let measured_display = meas.as_ref().is_some_and(|m| {
            m.unc.is_some_and(|u| u != 0.0) || (self.sig_fig_mode && m.sig.is_some())
        });
        let form = (based.is_none()
            && calc.is_none()
            && !measured_display
            && finite
            && sum.is_none()
            && wants_exact_form(&expr))
        .then(|| {
            exact_form(
                value.n,
                ExactFormOptions {
                    rationalize: self.options.rationalize,
                    wide: false,
                },
            )
        })
        .flatten();
        let exact = calc
            .as_ref()
            .and_then(|c| c.exact.clone())
            .or_else(|| {
                (!measured_display)
                    .then(|| sum.as_ref().and_then(|s| s.exact.clone()))
                    .flatten()
            })
            .or_else(|| form.map(|f| with_unit(&f, value.unit.as_deref())));
        let (job, time) = calc.map_or((None, None), |c| (c.job, c.time));
        LineResult {
            raw: raw.into(),
            kind,
            value: Some(value),
            display,
            exact,
            meas,
            quantity,
            variable,
            closed_form: job,
            time,
            ..LineResult::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(s: &str) -> String {
        evaluate_line(s, &EvaluateOptions::default()).display
    }

    #[test]
    fn plain_lines() {
        assert_eq!(shown("2+3*4"), "14");
        assert_eq!(shown("2+3="), "5");
        assert_eq!(shown(""), "");
        assert_eq!(shown("1/3"), "0.333333333333");
        assert_eq!(shown("255 in hex"), "0xFF");
        assert_eq!(shown("hex(255)"), "0xFF");
        assert_eq!(shown("10 to binary"), "0b1010");
        assert_eq!(
            evaluate_line("√8", &EvaluateOptions::default())
                .exact
                .as_deref(),
            Some("2sqrt(2)")
        );
        let frac = EvaluateOptions {
            fraction_mode: Some(true),
            ..EvaluateOptions::default()
        };
        assert_eq!(evaluate_line("1/3+1/6", &frac).display, "1/2");
    }

    #[test]
    fn sheets_carry_variables_and_ans() {
        let out = evaluate_sheet(
            &["x = 5".into(), "x * 2".into(), "ans + 1".into()],
            &EvaluateOptions::default(),
        );
        let shown: Vec<&str> = out.iter().map(|r| r.display.as_str()).collect();
        assert_eq!(shown, ["5", "10", "11"]);
        assert_eq!(out[0].kind, LineKind::Assignment);
        let f = evaluate_sheet(
            &["f(x) = x^2 + 1".into(), "f(3)".into()],
            &EvaluateOptions::default(),
        );
        assert_eq!(f[1].display, "10");
    }

    #[test]
    fn reserved_names_cannot_be_assigned() {
        assert!(parse_assignment("pi = 3").is_none());
        assert!(parse_assignment("E = 5").is_some());
        assert_eq!(strip_trailing_equals("2 == "), "2 == ");
        assert_eq!(strip_trailing_equals("2+3 ="), "2+3");
    }
}
