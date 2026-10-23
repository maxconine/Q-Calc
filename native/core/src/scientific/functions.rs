//! PORT(WP-FUNCTIONS): the rest of `scalarScope` in `src/engine/scientific.ts` (trig in both angle modes with
//! `sinCos` snapping, inverse trig, hyperbolics, logs, rounding, statistics, gcd/lcm, nCr/nPr, random, ranges,
//! zeta, powmod, complex parts) and the mathjs functions the engine reaches outside its scope (`sum`, `hypot`, …).
//!
//! Each entry's `max_args` is the JS function's `length` (0 for variadic), which `unitSafe` enforces.

use crate::eval::builtins::Builtin;

/// Looked up after the foundation's own table in `crate::eval::builtins`.
pub const TABLE: &[Builtin] = &[];
