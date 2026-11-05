//! How the generic evaluator's answers are written: each entry both exactly (`1/7`, `2*sqrt(3)`, `k+1`) and to a few
//! digits, and answers in labeled parts (`P = […]`, `λ = 2: […]`) that the line shows joined.

use num_bigint::BigInt;
use num_traits::{One, Zero};

use super::exact_eval::{Cells, compact, rat_f64};
use crate::exact::Rat;
use crate::format::format_number;
use crate::jsnum;

/// A complex number as (re, im).
pub(super) type C64 = (f64, f64);

/// Matrix answers are wide; a few digits per entry keep a row on screen.
const ENTRY_FIGS: f64 = 6.0;

/// The float answer already shows this rational whole: a whole number, or a decimal that ends within the digits a
/// matrix entry gets (`0.00001`, `1.5`), where a fraction beside it would add nothing.
pub(super) fn shows_whole(r: &Rat) -> bool {
    if r.d.is_one() {
        return true;
    }
    let mut d = r.d.clone();
    for p in [2, 5] {
        let p = BigInt::from(p);
        while (&d % &p).is_zero() {
            d /= &p;
        }
    }
    if !d.is_one() {
        return false;
    }
    let s = jsnum::to_string(rat_f64(r));
    let mant = s.split('e').next().unwrap_or("");
    let digits = mant.chars().filter(char::is_ascii_digit).skip_while(|c| *c == '0');
    digits.count() <= 6
}

/// One entry of an answer.
#[derive(Clone, Debug)]
pub(super) struct Cell {
    /// Exact text, when the field is exact.
    pub exact: Option<String>,
    /// To a few digits (or the exact text, for a symbol).
    pub dec: String,
    pub z: C64,
    /// The exact text says more than `dec`.
    pub rich: bool,
}

/// One labeled piece of an answer: `P = [1 1; 0 1]`.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Part {
    /// Written before the value, with its own `=` or `:`.
    pub label: Option<String>,
    pub display: String,
    pub exact: Option<String>,
    /// What storing this part keeps, when it is one matrix or number.
    pub literal: Option<String>,
}

/// An answer in parts.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Answer {
    pub parts: Vec<Part>,
}

impl Answer {
    fn joined(&self, pick: impl Fn(&Part) -> &str) -> String {
        self.parts
            .iter()
            .map(|p| format!("{}{}", p.label.as_deref().unwrap_or(""), pick(p)))
            .collect::<Vec<_>>()
            .join(",  ")
    }

    pub(super) fn display(&self) -> String {
        self.joined(|p| &p.display)
    }

    /// The exact form, when some part has one.
    pub(super) fn exact(&self) -> Option<String> {
        self.parts
            .iter()
            .any(|p| p.exact.is_some())
            .then(|| self.joined(|p| p.exact.as_deref().unwrap_or(&p.display)))
    }
}

pub(super) struct Fmt {
    figs: f64,
}

impl Fmt {
    pub(super) fn new(sig_figs: f64) -> Self {
        // `Math.min` keeps a NaN
        let figs = if sig_figs.is_nan() {
            f64::NAN
        } else {
            sig_figs.min(ENTRY_FIGS)
        };
        Fmt { figs }
    }

    pub(super) fn num(&self, v: f64) -> String {
        format_number(super::tidy(v, 1.0), self.figs)
    }

    /// `1+2i`, `-3i`, `0.5`: written to sit in a spaced row.
    pub(super) fn complex(&self, z: C64) -> String {
        let (re, im) = (super::tidy(z.0, z.1.abs()), super::tidy(z.1, z.0.abs()));
        if im == 0.0 {
            return self.num(re);
        }
        let i = if (im.abs() - 1.0).abs() < 1e-12 {
            "i".to_string()
        } else {
            format!("{}i", self.num(im.abs()))
        };
        if re == 0.0 {
            return if im < 0.0 { format!("-{i}") } else { i };
        }
        format!("{}{}{i}", self.num(re), if im < 0.0 { "-" } else { "+" })
    }

    pub(super) fn cell<F: Cells>(&self, f: &F, c: &F::C) -> Cell {
        let v = f.approx(c);
        if f.is_float() {
            let v = v.unwrap_or(f64::NAN);
            return Cell {
                exact: None,
                dec: self.num(v),
                z: (v, 0.0),
                rich: false,
            };
        }
        let exact = f.show(c);
        match v {
            Some(v) => Cell {
                rich: f.as_rat(c).is_none_or(|r| !shows_whole(&r)),
                dec: self.num(v),
                exact: Some(exact),
                z: (v, 0.0),
            },
            // a symbol: the exact text is all there is
            None => Cell {
                dec: exact.clone(),
                exact: Some(exact),
                z: (f64::NAN, 0.0),
                rich: false,
            },
        }
    }

    pub(super) fn ccell(&self, z: C64) -> Cell {
        Cell {
            exact: None,
            dec: self.complex(z),
            z,
            rich: false,
        }
    }

    /// An entry known exactly by its text (an eigenvalue like `(5 ± sqrt(33))/2` written out).
    pub(super) fn text_cell(&self, exact: String, z: C64) -> Cell {
        Cell {
            exact: Some(exact),
            dec: self.complex(z),
            z,
            rich: true,
        }
    }

    pub(super) fn cells<F: Cells>(&self, f: &F, g: &[Vec<F::C>]) -> Vec<Vec<Cell>> {
        g.iter()
            .map(|r| r.iter().map(|c| self.cell(f, c)).collect())
            .collect()
    }
}

fn grid_text(g: &[Vec<Cell>], pick: impl Fn(&Cell) -> String) -> String {
    format!(
        "[{}]",
        g.iter()
            .map(|r| r.iter().map(|c| compact(&pick(c))).collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
            .join("; ")
    )
}

fn exact_of(c: &Cell) -> String {
    c.exact.clone().unwrap_or_else(|| c.dec.clone())
}

/// A matrix as one part.
pub(super) fn grid_part(label: Option<String>, g: &[Vec<Cell>]) -> Part {
    let display = grid_text(g, |c| c.dec.clone());
    let rich = g.iter().flatten().any(|c| c.rich);
    let exact = rich.then(|| grid_text(g, exact_of));
    let real = g.iter().flatten().all(|c| c.z.1 == 0.0);
    let literal = real.then(|| {
        format!(
            "[{}]",
            g.iter()
                .map(|r| format!(
                    "[{}]",
                    r.iter()
                        .map(|c| if c.z.0.is_nan() || rich { exact_of(c) } else { c.dec.clone() })
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
                .collect::<Vec<_>>()
                .join(", ")
        )
    });
    Part {
        label,
        display,
        exact: exact.filter(|e| *e != grid_text(g, |c| c.dec.clone())),
        literal,
    }
}

/// A column vector as a one-column grid.
pub(super) fn column(v: &[Cell]) -> Vec<Vec<Cell>> {
    v.iter().map(|c| vec![c.clone()]).collect()
}

/// Column vectors written one after another: `[1; 0], [0; 1]`.
pub(super) fn vectors_part(label: Option<String>, vs: &[Vec<Cell>]) -> Part {
    let parts: Vec<Part> = vs.iter().map(|v| grid_part(None, &column(v))).collect();
    let display = parts.iter().map(|p| p.display.clone()).collect::<Vec<_>>().join(", ");
    let exact = parts.iter().any(|p| p.exact.is_some()).then(|| {
        parts
            .iter()
            .map(|p| p.exact.clone().unwrap_or_else(|| p.display.clone()))
            .collect::<Vec<_>>()
            .join(", ")
    });
    Part {
        label,
        display,
        exact,
        literal: None,
    }
}

/// A number as one part.
pub(super) fn number_part(label: Option<String>, c: &Cell) -> Part {
    Part {
        label,
        display: c.dec.clone(),
        exact: c.rich.then(|| exact_of(c)).filter(|e| *e != c.dec),
        literal: None,
    }
}

pub(super) fn text_part(label: Option<String>, text: impl Into<String>) -> Part {
    Part {
        label,
        display: text.into(),
        exact: None,
        literal: None,
    }
}

/// A polynomial in λ from its coefficients (λ⁰ first), highest power first: `λ^2 - 5*λ - 2`.
pub(super) fn poly_part(label: Option<String>, coeffs: &[Cell], var: &str) -> Part {
    let write = |pick: &dyn Fn(&Cell) -> String| -> String {
        let mut out = String::new();
        for (k, c) in coeffs.iter().enumerate().rev() {
            let text = pick(c);
            if text == "0" {
                continue;
            }
            let power = match k {
                0 => String::new(),
                1 => var.to_string(),
                _ => format!("{var}^{k}"),
            };
            // a compound coefficient is bracketed; a plain one keeps its sign outside
            let compound = text.chars().skip(1).any(|ch| ch == '+' || ch == '-' || ch == ' ');
            let (neg, mag) = match text.strip_prefix('-') {
                Some(m) if !compound => (true, m.to_string()),
                _ => (false, if compound { format!("({text})") } else { text.clone() }),
            };
            let body = if power.is_empty() {
                mag
            } else if mag == "1" {
                power
            } else {
                format!("{mag}*{power}")
            };
            if out.is_empty() {
                out = if neg { format!("-{body}") } else { body };
            } else {
                out.push_str(if neg { " - " } else { " + " });
                out.push_str(&body);
            }
        }
        if out.is_empty() { "0".into() } else { out }
    };
    let display = write(&|c: &Cell| c.dec.clone());
    let rich = coeffs.iter().any(|c| c.rich);
    let exact = rich.then(|| write(&|c: &Cell| exact_of(c)));
    Part {
        label,
        display,
        exact,
        literal: None,
    }
}

/// `yes` or `no`, with why.
pub(super) fn verdict(yes: bool, why: Option<String>) -> Part {
    let word = if yes { "yes" } else { "no" };
    text_part(
        None,
        match why {
            Some(w) => format!("{word} ({w})"),
            None => word.to_string(),
        },
    )
}
