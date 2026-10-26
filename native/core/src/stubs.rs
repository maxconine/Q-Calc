//! The evaluate pipeline's stages that later work packages port. Each function keeps the TS name, signature and
//! "not mine" answer, so `crate::evaluate` already calls everything in the TS order and a package only has to
//! replace bodies (moving a function into its own module and re-exporting it from here).
//!
//! Owners (see `native/PORTING_PLAN.md`): dots/letters/angles → WP-FUNCTIONS; chem → WP-CHEM; solve, isolate,
//! system, unit solve → WP-SOLVE; calculus → WP-CALCULUS; matrix → WP-MATRIX; sums → WP-SUMS; measure → WP-MEASURE.

use crate::types::{TimeLetters, Value};

// ---- WP-FUNCTIONS (dots.ts, letters.ts, angles.ts) ----

// ported: `crate::dots`, `crate::letters`, `crate::angles` (the TS exponent is a number; `angle_exponent` here
// keeps the stub's integer form, which is all `evaluate` compares)
pub use crate::angles::angle_exponent_int as angle_exponent;
pub use crate::angles::calls_worked_out;
pub use crate::dots::join_dots;
pub use crate::letters::split_letters;

// ---- WP-CHEM (chem.ts) ----

#[derive(Clone, Debug)]
pub struct GasConstant {
    pub n: f64,
    pub unit: String,
}

pub fn gas_constant(_text: &str) -> Option<GasConstant> {
    None
}

#[derive(Clone, Debug)]
pub struct MolarMass {
    pub mass: f64,
    pub display: String,
}

pub fn molar_mass_answer(_text: &str, _is_name: &dyn Fn(&str) -> bool) -> Option<MolarMass> {
    None
}

/// A formula or reaction's answer; Some("") is a chemistry line left blank on purpose.
pub fn chem_answer(_text: &str, _is_name: &dyn Fn(&str) -> bool) -> Option<String> {
    None
}

// ---- WP-SOLVE (solve.ts, isolate.ts, system.ts, unitSolve.ts) ----

// ported: `crate::solve`, `crate::isolate`, `crate::system` (the guess is a name → text list, as the TS record)
pub use crate::isolate::{NamedSolve, isolate_variable, parse_named_solve};
pub use crate::solve::{SolveContext, Solved, format_solve, is_equation, solve_equation};
pub use crate::system::{SystemCall, parse_system_call, solve_system_call};
// `solveWithUnitsOne` takes the TS ctx (solve context, stored names, default units, sig figs)
pub use crate::unit_solve::{
    UnitCtx, UnitSolved, clearly_units_blank, isolate_with_units, solve_with_units_one,
};

// ---- WP-MATRIX (matrix.ts) ----

// ported: `matrix_answer` now takes `MatrixOptions` (the TS passes functions and a scalar reader too)
pub use crate::matrix::{MatrixAnswer, MatrixOptions, looks_like_matrix, matrix_answer};

// ---- WP-SUMS (sums.ts) ----

/// `Σ` spellings as the one form `sumAnswer` reads.
pub fn normalize_sums(expr: &str) -> String {
    expr.to_string()
}

#[derive(Clone, Debug)]
pub struct SumAnswer {
    pub value: Value,
    pub exact: Option<String>,
}

pub fn sum_answer(_expr: &str) -> Option<SumAnswer> {
    None
}

// ---- WP-CALCULUS (calculus.ts, calcNumeric.ts, closedForm.ts) ----

#[derive(Clone, Debug)]
pub struct CalculusResult {
    pub value: Option<Value>,
    pub exact: Option<String>,
    /// The closed-form job the background search may upgrade (see ARCHITECTURE.md, "Long calculations").
    pub job: Option<serde_json::Value>,
    pub time: Option<TimeLetters>,
}

pub fn is_calculus_input(_text: &str) -> bool {
    false
}

pub fn evaluate_calculus(_expr: &str) -> Option<CalculusResult> {
    None
}

/// What folding the integrals beside other terms into their values did (`embedIntegrals`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Embedded {
    /// No integral to fold.
    Nothing,
    /// An integral that couldn't be worked out: the line is blank.
    Failed,
    /// The line with an integral replaced by its value.
    Rewritten(String),
}

pub fn embed_integrals(_expr: &str) -> Embedded {
    Embedded::Nothing
}

// ---- WP-MEASURE (measure.ts) ----

// ported: `measure` takes the TS's `MeasureContext` (ans, angle mode, variables, functions, measures), and
// `Measured.meas` is optional, as `meas?` is in the TS
pub use crate::measure::{MeasureContext, Measured, format_measured, has_plus_minus, measure};

// ---- WP-PHRASES: `mergeLiveAnswer` needs `isMolarMassInput` (chem.ts); WP-CHEM may move it into chem.rs ----

/// Whether a line asks for a molar mass in words: `molar mass H2O`, `H2O molar mass`. PORT: `isMolarMassInput`.
pub fn is_molar_mass_input(text: &str) -> bool {
    use crate::jsre::{self, Lazy};
    static ASK: Lazy = Lazy::new(|| {
        jsre::js(
            r"^(?:molar\s+mass|molecular\s+(?:weight|mass)|formula\s+(?:weight|mass)|chem)(?:\s+of)?\s+(\S+)$",
            "i",
        )
    });
    static AFTER: Lazy = Lazy::new(|| jsre::js(r"^(\S+)\s+molar\s+mass$", "i"));
    let t = crate::phrases::js_trim(text);
    jsre::test(&ASK, t) || jsre::test(&AFTER, t)
}
