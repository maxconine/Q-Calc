//! The typed records and enums the UI sees (`native/QCalcCore/API.md`), and their conversions to the core's
//! TS-shaped types. Every field added after the freeze carries `#[uniffi(default)]`, so Swift code that builds
//! a record with its memberwise initializer keeps compiling.

use std::collections::HashMap;

use qcalc_core::types::{
    AngleMode, EvaluateOptions, LineKind, LineResult, Meas, SolveOutcome as CoreOutcome,
    TimeLetters as CoreTimeLetters, UserFunction, ValueKind,
};
use serde::{Deserialize, Serialize};

/// Errors the FFI can return. Evaluation itself never fails (an unreadable line is a blank answer).
#[derive(Debug, uniffi::Error)]
pub enum CoreError {
    /// JSON that isn't the shape asked for, or a `call_json` function the dispatch doesn't know.
    BadRequest { message: String },
    /// The job's `CancelToken` was cancelled.
    Cancelled,
    /// Reading or writing a file under the engine's data directory.
    Io { message: String },
    /// `call_json` on a TS function nothing ports yet.
    Unported { function: String },
    /// The engine panicked; it is still usable.
    Internal { message: String },
}

impl std::error::Error for CoreError {}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoreError::BadRequest { message } => write!(f, "bad request: {message}"),
            CoreError::Cancelled => write!(f, "cancelled"),
            CoreError::Io { message } => write!(f, "i/o: {message}"),
            CoreError::Unported { function } => write!(f, "not ported yet: {function}"),
            CoreError::Internal { message } => write!(f, "internal error: {message}"),
        }
    }
}

impl CoreError {
    pub(crate) fn bad(e: impl std::fmt::Display) -> Self {
        CoreError::BadRequest {
            message: e.to_string(),
        }
    }

    pub(crate) fn io(e: impl std::fmt::Display) -> Self {
        CoreError::Io {
            message: e.to_string(),
        }
    }
}

/// How the engine is set up.
#[derive(Clone, Debug, uniffi::Record)]
pub struct EngineConfig {
    #[uniffi(default)]
    pub is_windows: bool,
    #[uniffi(default = "en_US")]
    pub locale: String,
    /// Where `history.json` and `rates.json` live (`~/Library/Application Support/Q Calc`); none means no files.
    #[uniffi(default)]
    pub data_dir: Option<String>,
}

/// A user function, `f(x) = x^2`.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct UserFn {
    pub params: Vec<String>,
    pub body: String,
}

/// Sig figs and ± of a measured answer (`Meas` in the TS). Absent `sig` and `dp` mean exact.
#[derive(Clone, Debug, Default, PartialEq, uniffi::Record)]
pub struct MeasInfo {
    #[uniffi(default)]
    pub sig: Option<i32>,
    #[uniffi(default)]
    pub dp: Option<i32>,
    #[uniffi(default)]
    pub unc: Option<f64>,
    #[uniffi(default)]
    pub unc_digits: Option<String>,
}

impl From<&Meas> for MeasInfo {
    fn from(m: &Meas) -> Self {
        MeasInfo {
            sig: m.sig.map(|s| s as i32),
            dp: m.dp,
            unc: m.unc,
            unc_digits: m.unc_digits.clone(),
        }
    }
}

impl From<&MeasInfo> for Meas {
    fn from(m: &MeasInfo) -> Self {
        Meas {
            sig: m.sig.and_then(|s| u32::try_from(s).ok()),
            dp: m.dp,
            unc: m.unc,
            unc_digits: m.unc_digits.clone(),
        }
    }
}

/// Settings and, optionally, the sheet's context, as `EvaluateOptions` in the TS.
#[derive(Clone, Debug, uniffi::Record)]
pub struct EvalOptions {
    #[uniffi(default)]
    pub angle_radians: bool,
    #[uniffi(default)]
    pub fraction_mode: bool,
    #[uniffi(default = 6)]
    pub sig_figs: u32,
    #[uniffi(default)]
    pub sig_fig_mode: bool,
    #[uniffi(default = true)]
    pub rationalize: bool,
    #[uniffi(default)]
    pub space_thousands: bool,
    #[uniffi(default)]
    pub ans: Option<f64>,
    #[uniffi(default)]
    pub variables: HashMap<String, f64>,
    #[uniffi(default)]
    pub functions: HashMap<String, UserFn>,
    /// Unit-valued variables and `ans`, as text the unit reader reads back (`5 cm`).
    #[uniffi(default)]
    pub quantities: HashMap<String, String>,
    /// Display unit per dimension (`length` → `ft`).
    #[uniffi(default)]
    pub default_units: HashMap<String, String>,
    /// Sig figs and ± of variables and `ans`.
    #[uniffi(default)]
    pub measures: HashMap<String, MeasInfo>,
    /// For d/dt: letters moved to varying (true) or constant (false) by hand.
    #[uniffi(default)]
    pub time_varying: HashMap<String, bool>,
}

impl Default for EvalOptions {
    /// The settings a new user starts with (`defaultSettings()` in `src/lib/settings.ts`).
    fn default() -> Self {
        EvalOptions {
            angle_radians: false,
            fraction_mode: false,
            sig_figs: 6,
            sig_fig_mode: false,
            rationalize: true,
            space_thousands: false,
            ans: None,
            variables: HashMap::new(),
            functions: HashMap::new(),
            quantities: HashMap::new(),
            default_units: HashMap::new(),
            measures: HashMap::new(),
            time_varying: HashMap::new(),
        }
    }
}

impl From<&EvalOptions> for EvaluateOptions {
    // every field is set today; the update keeps this compiling when a package adds an option
    #[allow(clippy::needless_update)]
    fn from(o: &EvalOptions) -> Self {
        EvaluateOptions {
            angle_mode: Some(if o.angle_radians {
                AngleMode::Rad
            } else {
                AngleMode::Deg
            }),
            ans: o.ans,
            variables: o.variables.iter().map(|(k, v)| (k.clone(), *v)).collect(),
            functions: o
                .functions
                .iter()
                .map(|(k, f)| {
                    (
                        k.clone(),
                        UserFunction {
                            params: f.params.clone(),
                            body: f.body.clone(),
                        },
                    )
                })
                .collect(),
            fraction_mode: Some(o.fraction_mode),
            rationalize: Some(o.rationalize),
            sig_figs: Some(f64::from(o.sig_figs)),
            sig_fig_mode: Some(o.sig_fig_mode),
            measures: o
                .measures
                .iter()
                .map(|(k, m)| (k.clone(), Meas::from(m)))
                .collect(),
            quantities: o
                .quantities
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            default_units: o
                .default_units
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            space_thousands: Some(o.space_thousands),
            time_varying: o
                .time_varying
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect(),
            ..EvaluateOptions::default()
        }
    }
}

impl From<EvalOptions> for EvaluateOptions {
    fn from(o: EvalOptions) -> Self {
        EvaluateOptions::from(&o)
    }
}

/// The last answer as a chain continues from it (`ChainAnswer` in `src/lib/chain.ts`).
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChainAnswer {
    /// The answer as it inserts (`insertableHistoryAnswer`).
    pub plain: String,
    /// Whether it carries a unit, so `in ft` continues from it too.
    pub unit: bool,
}

/// What the history defines for the next line (`contextFromHistory`).
#[derive(Clone, Debug, Default, PartialEq, uniffi::Record)]
pub struct EvalContext {
    #[uniffi(default)]
    pub ans: Option<f64>,
    #[uniffi(default)]
    pub variables: HashMap<String, f64>,
    #[uniffi(default)]
    pub functions: HashMap<String, UserFn>,
    #[uniffi(default)]
    pub measures: HashMap<String, MeasInfo>,
    #[uniffi(default)]
    pub quantities: HashMap<String, String>,
    #[uniffi(default)]
    pub chain_answer: Option<ChainAnswer>,
    /// The last row's expression: `isolate x` alone works on it.
    #[uniffi(default)]
    pub last_expr: Option<String>,
}

/// Settings plus history context in one `EvalOptions`; the context wins where both say something.
#[uniffi::export]
pub fn merged_options(options: EvalOptions, context: EvalContext) -> EvalOptions {
    merge(&options, &context)
}

pub(crate) fn merge(options: &EvalOptions, context: &EvalContext) -> EvalOptions {
    let mut o = options.clone();
    if context.ans.is_some() {
        o.ans = context.ans;
    }
    o.variables.extend(context.variables.clone());
    o.functions.extend(context.functions.clone());
    o.measures.extend(context.measures.clone());
    o.quantities.extend(context.quantities.clone());
    o
}

/// One keystroke's input.
#[derive(Clone, Debug, uniffi::Record)]
pub struct LiveRequest {
    pub text: String,
    pub options: EvalOptions,
    pub context: EvalContext,
    /// Whether the text continues from the last answer: `Engine::chains`.
    #[uniffi(default)]
    pub chained: bool,
    /// The clock, ms since 1970, for time phrases and the rates' freshness.
    #[uniffi(default)]
    pub now_ms: i64,
    /// Answer natural language (`20% of 80`, `$20 in euros`).
    #[uniffi(default = true)]
    pub phrases: bool,
}

/// What a line was.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AnswerKind {
    Empty,
    Expression,
    Assignment,
    Function,
    Solve,
}

impl From<LineKind> for AnswerKind {
    fn from(k: LineKind) -> Self {
        match k {
            LineKind::Empty => AnswerKind::Empty,
            LineKind::Expression => AnswerKind::Expression,
            LineKind::Assignment => AnswerKind::Assignment,
            LineKind::Function => AnswerKind::Function,
            LineKind::Solve => AnswerKind::Solve,
        }
    }
}

/// A UTF-16 range of the input, as the TS engine counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

/// How a solve ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SolveOutcome {
    Roots,
    None,
    Contradiction,
    NoneFound,
    All,
}

/// A solve's unknown and roots.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SolveInfo {
    pub variable: String,
    pub roots: Vec<f64>,
    pub outcome: SolveOutcome,
    #[uniffi(default)]
    pub more: bool,
}

impl From<&qcalc_core::types::SolveInfo> for SolveInfo {
    fn from(s: &qcalc_core::types::SolveInfo) -> Self {
        SolveInfo {
            variable: s.variable.clone(),
            roots: s.roots.clone(),
            outcome: match s.outcome {
                CoreOutcome::Roots => SolveOutcome::Roots,
                CoreOutcome::None => SolveOutcome::None,
                CoreOutcome::Contradiction => SolveOutcome::Contradiction,
                CoreOutcome::NoneFound => SolveOutcome::NoneFound,
                CoreOutcome::All => SolveOutcome::All,
            },
            more: s.more == Some(true),
        }
    }
}

impl From<&SolveInfo> for qcalc_core::types::SolveInfo {
    fn from(s: &SolveInfo) -> Self {
        qcalc_core::types::SolveInfo {
            variable: s.variable.clone(),
            roots: s.roots.clone(),
            outcome: match s.outcome {
                SolveOutcome::Roots => CoreOutcome::Roots,
                SolveOutcome::None => CoreOutcome::None,
                SolveOutcome::Contradiction => CoreOutcome::Contradiction,
                SolveOutcome::NoneFound => CoreOutcome::NoneFound,
                SolveOutcome::All => CoreOutcome::All,
            },
            more: s.more.then_some(true),
        }
    }
}

/// A time derivative's letters: moving with time, held constant, and locked by hand.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TimeLetters {
    pub varying: Vec<String>,
    pub constant: Vec<String>,
    pub locked: Vec<String>,
}

impl From<&CoreTimeLetters> for TimeLetters {
    fn from(t: &CoreTimeLetters) -> Self {
        TimeLetters {
            varying: t.varying.clone(),
            constant: t.constant.clone(),
            locked: t.locked.clone(),
        }
    }
}

/// The live answer for one keystroke.
#[derive(Clone, Debug, uniffi::Record)]
pub struct LiveAnswer {
    /// What is copied and saved; empty when blank.
    pub display: String,
    /// What the bar shows: grouping commas and a real minus sign (`prettyAnswer`).
    pub pretty: String,
    pub exact: Option<String>,
    /// `π/4 ≈ 0.785398`, or the display.
    pub dual_label: String,
    pub number: Option<f64>,
    pub unit: Option<String>,
    pub kind: AnswerKind,
    /// Where a blank answer stops reading (the squiggle).
    pub blank: Option<Span>,
    /// The rest of the word being typed.
    pub completion: Option<String>,
    pub solve: Option<SolveInfo>,
    /// Hand to `Engine::closed_form`.
    pub closed_form_job: Option<String>,
    pub time_letters: Option<TimeLetters>,
    #[uniffi(default)]
    pub quantity: Option<String>,
    #[uniffi(default)]
    pub variable: Option<String>,
    #[uniffi(default)]
    pub meas: Option<MeasInfo>,
    /// The TS `Value` as JSON, for `FormSource`.
    #[uniffi(default)]
    pub value_json: Option<String>,
    /// What was evaluated (`ans*2` when chained).
    #[uniffi(default)]
    pub expr: String,
    /// The phrase layer answered.
    #[uniffi(default)]
    pub phrase: bool,
    /// `improper unit conversion`.
    #[uniffi(default)]
    pub improper: bool,
    /// The whole TS `LineResult` as JSON.
    pub json: String,
}

impl LiveAnswer {
    pub(crate) fn blank(expr: &str) -> Self {
        LiveAnswer {
            display: String::new(),
            pretty: String::new(),
            exact: None,
            dual_label: String::new(),
            number: None,
            unit: None,
            kind: AnswerKind::Expression,
            blank: None,
            completion: None,
            solve: None,
            closed_form_job: None,
            time_letters: None,
            quantity: None,
            variable: None,
            meas: None,
            value_json: None,
            expr: expr.to_string(),
            phrase: false,
            improper: false,
            json: serde_json::to_string(&LineResult::blank(expr)).unwrap_or_default(),
        }
    }
}

/// One answered line of a sheet.
#[derive(Clone, Debug, uniffi::Record)]
pub struct LineAnswer {
    pub raw: String,
    pub kind: AnswerKind,
    /// What the answer slot shows; empty when the line is blank.
    pub display: String,
    /// The closed form beside the decimal (`2sqrt(3)`).
    pub exact: Option<String>,
    /// The number, when the answer is one (with `unit` if it has one).
    pub number: Option<f64>,
    pub unit: Option<String>,
    /// The variable an assignment defined.
    pub variable: Option<String>,
    /// A unit answer as text the reader reads back, for chaining `ans`.
    pub quantity: Option<String>,
    /// The whole `LineResult` as the TS engine's JSON, for fields not typed here yet.
    pub json: String,
}

pub(crate) fn line_answer(r: &LineResult) -> LineAnswer {
    LineAnswer {
        raw: r.raw.clone(),
        kind: r.kind.into(),
        display: r.display.clone(),
        exact: r.exact.clone(),
        number: number_of(r),
        unit: r.value.as_ref().and_then(|v| v.unit.clone()),
        variable: r.variable.clone(),
        quantity: r.quantity.clone(),
        json: serde_json::to_string(r).unwrap_or_default(),
    }
}

pub(crate) fn number_of(r: &LineResult) -> Option<f64> {
    r.value
        .as_ref()
        .filter(|v| v.kind == ValueKind::Number)
        .map(|v| v.n)
}

/// What a saved history row was.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum HistoryKind {
    Definition,
    Function,
    System,
}

/// One row of the tape (`HistoryRow` in `src/lib/history.ts`).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct HistoryRow {
    pub id: String,
    pub expr: String,
    pub display: String,
    #[uniffi(default)]
    pub exact: Option<String>,
    #[uniffi(default)]
    pub n: Option<f64>,
    #[uniffi(default)]
    pub meas: Option<MeasInfo>,
    #[uniffi(default)]
    pub quantity: Option<String>,
    #[uniffi(default)]
    pub kind: Option<HistoryKind>,
    #[uniffi(default)]
    pub equations: Option<Vec<String>>,
    #[uniffi(default)]
    pub fn_name: Option<String>,
    #[uniffi(default)]
    pub fn_params: Option<Vec<String>>,
    #[uniffi(default)]
    pub fn_body: Option<String>,
    #[uniffi(default)]
    pub solve: Option<SolveInfo>,
    /// Commit time, ms since 1970.
    #[uniffi(default)]
    pub at: Option<i64>,
}

/// Which rows count for variables (`HistoryShow` in `src/lib/historyShow.ts`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, uniffi::Enum)]
pub enum HistoryShow {
    #[default]
    Recent,
    Always,
    Arrow,
}

/// Which side of a dual answer inserts (`AnswerForm` in `src/lib/answer.ts`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, uniffi::Enum)]
pub enum AnswerForm {
    #[default]
    Exact,
    Approx,
}

/// The settings `context_from_history` reads.
#[derive(Clone, Copy, Debug, uniffi::Record)]
pub struct HistoryView {
    #[uniffi(default)]
    pub show: HistoryShow,
    #[uniffi(default)]
    pub answer_form: AnswerForm,
    #[uniffi(default = 6)]
    pub sig_figs: u32,
    #[uniffi(default)]
    pub now_ms: i64,
}

/// Enter's outcome.
#[derive(Clone, Debug, uniffi::Record)]
pub struct CommitResult {
    pub saved: bool,
    /// The row saved, or the last row refreshed when this repeats it.
    pub row: Option<HistoryRow>,
    /// The history to keep (and save).
    pub history: Vec<HistoryRow>,
    /// What the bar holds now.
    pub input: String,
    /// When nothing was saved: why (`enterHint`).
    pub hint: Option<String>,
    /// The row as TS `HistoryRow` JSON.
    pub json: String,
}

/// Names `completion`, `blank_reason` and `spans` treat as known.
#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct CompletionNames {
    #[uniffi(default)]
    pub variables: Vec<String>,
    #[uniffi(default)]
    pub functions: Vec<String>,
    #[uniffi(default)]
    pub ans: bool,
    #[uniffi(default)]
    pub contacts: Vec<String>,
}

/// What a highlighted span of the input is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SpanKind {
    Unit,
    KnownWord,
    Radical,
    Bound,
}

/// A highlighted span of the input (UTF-16 offsets).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TextSpan {
    pub start: u32,
    pub end: u32,
    pub kind: SpanKind,
    #[uniffi(default)]
    pub name: Option<String>,
    /// The TS span as JSON (a bound's limit slots, for instance).
    #[uniffi(default)]
    pub json: String,
}

/// `answerForms` input: the answer as the slot shows it.
#[derive(Clone, Debug, uniffi::Record)]
pub struct FormSource {
    /// `LiveAnswer::value_json`.
    pub value_json: String,
    pub display: String,
    #[uniffi(default)]
    pub exact: Option<String>,
    #[uniffi(default)]
    pub meas: Option<MeasInfo>,
    #[uniffi(default = 6)]
    pub sig_figs: u32,
    #[uniffi(default)]
    pub sig_fig_mode: bool,
    #[uniffi(default = true)]
    pub rationalize: bool,
}

/// One form in the Tab cycle; `value_json` when it is a different quantity of units (26.5 lb as 423 oz).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct AnswerView {
    pub display: String,
    #[uniffi(default)]
    pub value_json: Option<String>,
}

/// How to draw a graph (`GraphOptions` in `src/engine/graph.ts`, minus the context, which comes from settings).
#[derive(Clone, Debug, uniffi::Record)]
pub struct GraphOptions {
    #[uniffi(default)]
    pub domain_lo: Option<f64>,
    #[uniffi(default)]
    pub domain_hi: Option<f64>,
    #[uniffi(default)]
    pub sample_count: Option<u32>,
    #[uniffi(default = true)]
    pub find_roots: bool,
    /// The plot's height over its width, so a contour plot's x and y share a scale.
    #[uniffi(default)]
    pub aspect: Option<f64>,
}

/// `graph f(x) = …`'s function, for the caller to register.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FunctionDef {
    pub name: String,
    pub params: Vec<String>,
    pub body: String,
}

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct GraphPoint {
    pub x: f64,
    /// None where the curve is undefined or breaks.
    pub y: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum CriticalKind {
    Min,
    Max,
    Critical,
}

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct CriticalPoint {
    pub x: f64,
    pub y: f64,
    pub kind: CriticalKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AngleUnit {
    Deg,
    Rad,
}

/// One level curve of a contour plot.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ContourLevel {
    pub level: f64,
    pub label: String,
    /// x1, y1, x2, y2 for each piece of the line.
    pub segments: Vec<f64>,
    pub at_x: Option<f64>,
    pub at_y: Option<f64>,
}

/// A plotted graph (`GraphResult`, without its readout functions: use `graph_y` and `graph_z`).
#[derive(Clone, Debug, uniffi::Record)]
pub struct GraphResult {
    pub label: String,
    pub expression: String,
    pub function_def: Option<FunctionDef>,
    pub points: Vec<GraphPoint>,
    pub domain_lo: f64,
    pub domain_hi: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub critical_points: Vec<CriticalPoint>,
    pub roots: Vec<f64>,
    pub angle_unit: Option<AngleUnit>,
    pub contours: Vec<ContourLevel>,
    /// A function of x and y, drawn as level curves.
    pub contour: bool,
    pub error: Option<String>,
    pub json: String,
}

/// Whether the currency rates are usable.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct RatesStatus {
    pub date: Option<String>,
    pub currencies: u32,
    /// Under 7 days old at the time asked, so conversions use them.
    pub fresh: bool,
    pub fetched_ms: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, uniffi::Record)]
pub struct Identity {
    pub show: String,
    pub copy: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub lead: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, uniffi::Record)]
pub struct IdentityGroup {
    pub title: String,
    pub items: Vec<Identity>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, uniffi::Record)]
pub struct IdentitySection {
    pub title: String,
    pub groups: Vec<IdentityGroup>,
    #[serde(default)]
    pub wide: bool,
}

/// A reference sheet (`IdentitySheet` in `src/lib/identities.ts`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, uniffi::Record)]
pub struct IdentitySheet {
    pub id: String,
    pub title: String,
    pub hint: String,
    pub sections: Vec<IdentitySection>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, uniffi::Enum)]
#[serde(rename_all = "lowercase")]
pub enum ElementCategory {
    Alkali,
    Alkaline,
    Transition,
    Post,
    Metalloid,
    Nonmetal,
    Halogen,
    Noble,
    Lanthanide,
    Actinide,
}

/// One element of the periodic table (`Element` in `src/lib/elements.ts`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, uniffi::Record)]
pub struct Element {
    pub n: u32,
    pub symbol: String,
    pub name: String,
    pub mass: String,
    pub period: u32,
    pub group: Option<u32>,
    pub category: ElementCategory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum HintTrigger {
    /// The answer sat still while typing.
    Pause,
    /// Enter was pressed.
    Commit,
    /// The bar just appeared.
    Open,
}

/// What `hint` picks from (`HintFacts` plus the seen mask and trigger of `pickHint`).
#[derive(Clone, Debug, uniffi::Record)]
pub struct HintFacts {
    pub seen: u32,
    pub on: HintTrigger,
    pub expr: String,
    #[uniffi(default)]
    pub answer: Option<String>,
    #[uniffi(default)]
    pub variable: Option<String>,
    #[uniffi(default)]
    pub unit: bool,
    #[uniffi(default)]
    pub chained: bool,
    #[uniffi(default)]
    pub angle_radians: bool,
    #[uniffi(default)]
    pub fraction_mode: bool,
    #[uniffi(default)]
    pub native: bool,
    #[uniffi(default)]
    pub opens: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, uniffi::Record)]
pub struct Hint {
    pub bit: u32,
    pub text: String,
}
