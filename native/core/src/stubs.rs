//! The evaluate pipeline's stages that later work packages port. Each function keeps the TS name, signature and
//! "not mine" answer, so `crate::evaluate` already calls everything in the TS order and a package only has to
//! replace bodies (moving a function into its own module and re-exporting it from here).
//!
//! Owners (see `native/PORTING_PLAN.md`): dots/letters/angles → WP-FUNCTIONS; chem → WP-CHEM; solve, isolate,
//! system, unit solve → WP-SOLVE; calculus → WP-CALCULUS; matrix → WP-MATRIX; sums → WP-SUMS; measure → WP-MEASURE.

use crate::types::Value;

// ---- WP-FUNCTIONS (dots.ts, letters.ts, angles.ts) ----

// ported: `crate::dots`, `crate::letters`, `crate::angles` (the TS exponent is a number; `angle_exponent` here
// keeps the stub's integer form, which is all `evaluate` compares)
pub use crate::angles::angle_exponent_int as angle_exponent;
pub use crate::angles::calls_worked_out;
pub use crate::dots::join_dots;
pub use crate::letters::split_letters;

// ---- WP-CHEM (chem.ts) ----

// ported: `crate::chem`
pub use crate::chem::{GasConstant, MolarMass, chem_answer, gas_constant, molar_mass_answer};

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

// ported: `crate::calculus`. `evaluate_calculus` and `embed_integrals` take the TS's context (variables,
// functions, angle mode, d/dt letters), so their signatures gained a `&CalcContext`.
pub use crate::calculus::{
    CalcContext, CalculusResult, Embedded, embed_integrals, evaluate_calculus, is_calculus_input,
};

// ---- WP-MEASURE (measure.ts) ----

// ported: `measure` takes the TS's `MeasureContext` (ans, angle mode, variables, functions, measures), and
// `Measured.meas` is optional, as `meas?` is in the TS
pub use crate::measure::{MeasureContext, Measured, format_measured, has_plus_minus, measure};

// ---- WP-PHRASES: `mergeLiveAnswer` needs `isMolarMassInput` (chem.ts), ported by WP-CHEM ----
pub use crate::chem::is_molar_mass_input;
