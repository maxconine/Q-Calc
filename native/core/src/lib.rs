//! Q Calc's engine, ported from the TS in `src/engine` and the engine-like parts of `src/lib`.
//!
//! Pure and portable: no UI, no platform code, no I/O except where a module says so (the rates fetcher).
//! See `native/ARCHITECTURE.md` for the layout and `native/PORTING_PLAN.md` for who owns which module.
//!
//! The entry point is [`evaluate_sheet`]: lines plus settings and context in, one [`LineResult`] per line out,
//! the same JSON the TS engine's `evaluateSheet` returns.

pub mod eval;
pub mod evaluate;
pub mod format;
pub mod golden;
pub mod jsnum;
pub mod jsre;
pub mod latex;
pub mod parens;
pub mod plain_math;
pub mod precise;
pub mod scientific;
pub mod simplify;
pub mod stubs;
// WP-SYMBOLIC
pub mod symbolic;
pub mod syntax;
pub mod types;
pub mod typst_input;
pub mod units;
pub mod work;
// ---- WP-UNITS ----
pub mod app;

// ---- WP-FUNCTIONS ----
pub mod angles;
pub mod dots;
pub mod letters;

// ---- WP-PHRASES ----
pub mod phrases;

pub use evaluate::{evaluate_line, evaluate_sheet};
pub use types::{EvaluateOptions, LineResult, Value};

/// The engine's version, for bug reports and the FFI handshake.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
