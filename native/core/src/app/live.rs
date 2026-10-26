//! The bar, one keystroke at a time: the per-keystroke flow of `src/components/QuickCalc.tsx` (chain, isolate the
//! previous row, the commands that answer with a message, phrase merge, ⌥↑ prefix steps, the Tab form, the squiggle)
//! and its ↵ (`commit`), without React, timers or storage.
//!
//! # Contract
//!
//! [`live`] is the single call the UI makes per keystroke. It takes a [`LiveRequest`]: the text in the bar, the
//! engine options (settings *and* the sheet's context: `ans`, variables, functions, measures, quantities, as
//! [`context_from_history`] builds them from the tape), the bar-only settings, and the UI state the TS kept in React
//! (the ⌥↑ unit, the Tab form index, open `sys` fields, the closed form a background search found). It returns a
//! [`LiveAnswer`]: everything the bar shows for that text, already decided:
//!
//! - `slot` is the text of the answer slot (an answer, or a command's line such as `periodic table`), with
//!   `message` saying it is a message, not an answer (so it isn't copied, inserted or typeset);
//! - `display`/`exact`/`shown` are the answer, its exact form, and the side `answer_form` reads; `steady` the label
//!   the dimmed slot keeps mid-edit; `sides`/`roots`/`roots_of` what clicking each part inserts;
//! - `squiggle` is where a blank answer stops reading. The TS waits 700 ms of idle typing before showing it; the UI
//!   keeps that delay, the core just computes it (None whenever the slot isn't blank);
//! - `completion` is the faint rest of the word being typed;
//! - `command` names a line that is a command (graph, sys, periodic, identities, a game, `games`, `tutorial`, `?`,
//!   `hello`, `message …`), and `tape_expr` is what ↵ saves (the chain written out, an isolate's rewrite);
//! - `line` is the engine's full [`LineResult`] (`closedForm` job, time letters, value) for anything else.
//!
//! Spans are UTF-16 indices (NSString's). Nothing here reads the clock: `now_ms` comes in the request. The call is
//! pure and synchronous; the UI runs it off the main thread and drops stale generations (ARCHITECTURE.md §5).
//!
//! [`commit`] is ↵ on a line that isn't a command: the new tape (slimmed, deduplicated, capped as
//! `persistableHistory` does), the saved row, or why nothing was saved (`enterHint`). Onboarding and the tour stay
//! the caller's: the result carries the [`HintFacts`] and the tour bits the commit counts toward.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json, json};

use crate::app::answer::{
    AnswerForm, insertable_answer, insertable_history_answer, solve_roots, visible_answer,
};
use crate::app::answer_forms::{FormSource, answer_forms};
use crate::app::blank_reason::{EnterFacts, Names, Span, blank_reason, enter_hint};
use crate::app::chain::{ChainAnswer, ans_written_out, chained_expr, chained_history_expr, chains_from_answer};
use crate::app::completion::{CompletionNames, completion_for};
use crate::app::games::{GAMES_HINT, PERIODIC_HINT, game_command, game_hint, is_games_command, is_periodic_command};
use crate::app::greeting::{GREETING_REPLY, is_greeting};
use crate::app::history as hist;
use crate::app::history_show::{HistoryShow, scope_start};
use crate::app::identities::identity_target_for;
use crate::app::js as jstr;
use crate::app::messaging::{Messaging, is_message_command, message_hint, parse_message};
use crate::app::needs::{
    FunctionDef, SysCommand, answer_phrase, in_ladder_unit, is_graph_command, is_molar_mass_input,
    is_sys_command, isolate_previous, looks_like_natural_language, parse_graph_intent, solve_live,
    sys_command, typst_answer, typst_preview_useful,
};
use crate::app::onboarding::{HintFacts, is_help_command};
use crate::app::tour::{TUTORIAL_HINT, TourFacts, is_tutorial_command, tour_used};
use crate::evaluate::parse_function_def;
use crate::format::{DEFAULT_SIG_FIGS, format_value};
use crate::jsre::{self, Lazy, js};
use crate::simplify::dual_label;
use crate::types::{
    EvaluateOptions, LineKind, LineResult, Meas, SolveInfo, SolveOutcome, UserFunction, Value, ValueKind,
};
use crate::units::is_improper_unit_conversion;

/// One keystroke's worth of input.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LiveRequest {
    /// The bar's text.
    pub text: String,
    /// Settings and the sheet's context, as the engine reads them.
    pub options: EvaluateOptions,
    /// Which side of a dual answer the bar reads (settings `answerForm`).
    pub answer_form: AnswerForm,
    /// The last answer as a chain continues from it ([`context_from_history`]); None when nothing chains.
    pub chain_answer: Option<ChainAnswer>,
    /// The last row's expression, for `isolate x` / `solve for x` alone.
    pub last_expr: Option<String>,
    /// Whether the phrase layer answers natural language (the TS `nativeUsable && !hasSoulver()`); true in the app.
    pub phrases: bool,
    /// Whether the typeset preview is on (settings `typstPreview`).
    pub typst_preview: bool,
    /// The unit ⌥↑/⌥↓ picked for the live answer (`prefixUnit`).
    pub prefix_unit: Option<String>,
    /// Tab: 0 shows the answer as it is, k shows the k-th of [`answer_forms`].
    pub tab: usize,
    /// The `sys N` equation fields, once ↵ opened them.
    pub sys_lines: Option<Vec<String>>,
    /// A closed form the background search verified for this line's `closedForm` job.
    pub closed_form: Option<String>,
    /// Messaging state and whether a relay is configured, for `message …` lines.
    pub messaging: Messaging,
    pub relay: bool,
    /// Accepted contacts, which `message <name>` completes.
    pub contacts: Vec<String>,
    /// Milliseconds since the epoch, for phrases (`now`).
    pub now_ms: f64,
    /// Currency rates for phrases, as the TS `CurrencyRates` table (null without any).
    pub rates: Json,
}

/// A line that answers with a message or opens something, rather than calculating.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LiveCommand {
    Graph,
    Sys,
    Periodic,
    Greeting,
    Message,
    /// `identities …`: the sheet id and the section to open at.
    Identity { sheet: String, section: usize },
    Game { game: crate::app::games::GameKind },
    Games,
    Tutorial,
    Help,
}

/// What clicking each side of a dual answer inserts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sides {
    pub exact: String,
    pub approx: String,
}

/// Everything the bar shows for one text.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveAnswer {
    /// The answer slot's text: the answer, or a command's line.
    pub slot: String,
    /// The slot holds a message, not an answer.
    pub message: bool,
    /// The answer (after phrases, ⌥↑ and Tab), "" when blank.
    pub display: String,
    pub exact: Option<String>,
    /// The side `answer_form` reads: what is copied.
    pub shown: String,
    pub number: Option<f64>,
    pub unit: Option<String>,
    /// The value ⌥↑/⌥↓ steps and Tab re-expresses (the stepped one when stepped).
    pub value: Option<Value>,
    pub meas: Option<Meas>,
    pub quantity: Option<String>,
    pub solve: Option<SolveInfo>,
    /// A solve's variable when it found roots (`x = …` before the answer).
    pub roots_of: Option<String>,
    pub roots: Option<Vec<String>>,
    pub sides: Sides,
    /// What the dimmed slot keeps mid-edit; None for commands (no steadying), "" for a blank.
    pub steady: Option<String>,
    pub squiggle: Option<Span>,
    pub completion: String,
    pub command: Option<LiveCommand>,
    /// The line evaluated (the chain through `ans`, an isolate's rewrite).
    pub expr: String,
    /// What ↵ saves when it differs from the text ("" otherwise).
    pub tape_expr: String,
    pub chained: bool,
    /// The answer came from the phrase layer, not the engine.
    pub phrase: bool,
    pub improper: bool,
    /// `graph f(x) = …` defines this function.
    pub function_def: Option<FunctionDef>,
    /// The typeset preview shows, for `typst_expr`, beside `typst_answer`.
    pub typst_shown: bool,
    pub typst_expr: String,
    pub typst_answer: String,
    /// What hint rules see of this answer (`liveRef.facts`).
    pub facts: HintFacts,
    /// The engine's own answer for the line, when it ran.
    pub line: Option<LineResult>,
}

/// `soulverAngleSafe` (lib/nativeEval.ts): in degree mode a phrase never answers anything with trig in it.
fn angle_safe(expr: &str, options: &EvaluateOptions) -> bool {
    static TRIG: Lazy = Lazy::new(|| js(r"(?<![A-Za-z])(?:a|arc)?(?:sin|cos|tan|sec|csc|cot)(?![A-Za-z])", "i"));
    options.angle() == crate::types::AngleMode::Rad || !jsre::test(&TRIG, expr)
}

/// `mergeLiveAnswer` with no native answer (the Mac no longer has SoulverCore): the engine's, or blank.
fn merge_live_answer(expr: &str, js_display: &str, js_n: Option<f64>) -> (String, Option<f64>) {
    static SUM_LINE: Lazy = Lazy::new(|| js(r"[Σ∑Π∏]|^\s*\\?(?:sum|prod|product)(?![A-Za-z0-9])", "i"));
    if jstr::trim(expr).is_empty() {
        return (String::new(), None);
    }
    if jsre::test(&SUM_LINE, expr) {
        return (js_display.into(), js_n.filter(|_| !js_display.is_empty()));
    }
    if crate::stubs::is_calculus_input(expr) || is_molar_mass_input(expr) {
        return (js_display.into(), js_n);
    }
    if js_display.is_empty() {
        (String::new(), None)
    } else {
        (js_display.into(), js_n)
    }
}

/// `withPhraseAnswer` (lib/phraseLive.ts): a phrase beats the engine on natural language and otherwise only fills
/// a blank. Returns the phrase's answer when it wins.
fn phrase_answer(req: &LiveRequest, js_display: &str, enabled: bool, sig_figs: f64) -> Option<(String, Option<f64>)> {
    let expr = req.text.as_str();
    if !enabled || jstr::trim(expr).is_empty() || crate::stubs::is_calculus_input(expr) {
        return None;
    }
    if !js_display.is_empty() && !looks_like_natural_language(expr) {
        return None;
    }
    answer_phrase(expr, req.now_ms, sig_figs, &req.rates).map(|p| (p.display, p.n))
}

/// The squiggle's and ↵'s evaluator: whether a piece of text answers (through `ans` when the line chains).
pub fn evaluates(options: &EvaluateOptions, chained: bool) -> impl Fn(&str) -> bool + '_ {
    move |text: &str| {
        let text = if chained { chained_expr(text) } else { text.to_string() };
        crate::evaluate_sheet(&[text], options)
            .first()
            .is_some_and(|r| !r.display.is_empty())
    }
}

fn names_of(options: &EvaluateOptions) -> Names {
    Names {
        variables: options.variables.keys().cloned().collect(),
        functions: options.functions.keys().cloned().collect(),
    }
}

/// One keystroke: everything the bar shows for `req.text`. See the module docs for the contract.
pub fn live(req: &LiveRequest) -> LiveAnswer {
    let q = req.text.as_str();
    let o = &req.options;
    let sig_figs = o.sig_figs.unwrap_or(f64::from(DEFAULT_SIG_FIGS));
    let graph_cmd = is_graph_command(q);
    let sys_cmd = is_sys_command(q);
    let greeting = is_greeting(q);
    let message_cmd = is_message_command(q);
    // `periodic`, `hello` and `message` answer with a message, not a calculation
    let periodic_cmd = is_periodic_command(q) || greeting || message_cmd;
    let identity = identity_target_for(q);
    let pong = game_command(q);
    let tutorial = is_tutorial_command(q);
    let help = is_help_command(q);
    let games = is_games_command(q);
    let intent = if graph_cmd { parse_graph_intent(q, &o.functions) } else { None };
    let function_def = intent.as_ref().and_then(|i| i.function_def.clone());
    let mut options = o.clone();
    if let Some(d) = &function_def {
        options.functions.insert(
            d.name.clone(),
            UserFunction {
                params: d.params.clone(),
                body: d.body.clone(),
            },
        );
    }

    // `isolate x` or `solve for x` alone works on the equation in the row before
    let isolated = isolate_previous(q, req.last_expr.as_deref());
    let chained = !graph_cmd && isolated.is_none() && chains_from_answer(q, req.chain_answer.as_ref());
    let plain = req.chain_answer.as_ref().map(|c| c.plain.as_str());
    let tape = match plain {
        Some(p) if chained => chained_history_expr(q, p),
        _ => q.to_string(),
    };
    let tape_expr = isolated.clone().unwrap_or_else(|| match plain {
        Some(p) => ans_written_out(&tape, p),
        None if chained => tape.clone(),
        None => String::new(),
    });
    let typst_expr = if chained { chained_expr(q) } else { q.to_string() };
    let a_command = graph_cmd || sys_cmd || periodic_cmd || identity.is_some() || pong.is_some() || tutorial;
    let expr = isolated.clone().unwrap_or_else(|| if chained { chained_expr(q) } else { q.to_string() });
    let line = (!a_command).then(|| {
        crate::evaluate_sheet(std::slice::from_ref(&expr), &options)
            .pop()
            .unwrap_or_else(|| LineResult::blank(&expr))
    });

    // the `sys` fields
    let sys_parsed = if sys_cmd { sys_command(q) } else { None };
    let sys_answer = match (&req.sys_lines, &sys_parsed) {
        (Some(lines), Some(SysCommand::Count(n))) if *n == lines.len() => {
            solve_live(lines, &o.default_units, o.angle())
        }
        _ => None,
    };
    let sys_hint = match &sys_parsed {
        Some(SysCommand::Hint(h)) if req.sys_lines.is_none() => Some(h.clone()),
        _ => None,
    };
    let sys_shown = sys_answer
        .as_ref()
        .map(|a| a.display.clone())
        .or_else(|| sys_hint.clone())
        .unwrap_or_default();
    let sys_message = sys_cmd && (sys_answer.as_ref().is_some_and(|a| a.message) || sys_hint.is_some());

    let js_display = if graph_cmd {
        intent
            .as_ref()
            .filter(|i| !i.label.is_empty())
            .map_or_else(String::new, |i| format!("graph {}", i.label))
    } else if !jstr::trim(q).is_empty() {
        line.as_ref().map_or_else(String::new, |l| l.display.clone())
    } else {
        String::new()
    };
    let line_value = line.as_ref().and_then(|l| l.value.as_ref());
    let js_n = if graph_cmd || sys_cmd {
        None
    } else {
        line_value.filter(|v| v.kind == ValueKind::Number).map(|v| v.n)
    };
    let native_usable = !a_command && !chained && angle_safe(q, o);
    let (mut merged_display, mut merged_n) = merge_live_answer(q, &js_display, js_n);
    let mut phrase = false;
    if let Some((d, n)) = phrase_answer(req, &js_display, native_usable && req.phrases, sig_figs) {
        (merged_display, merged_n) = (d, n);
        phrase = true;
    }
    // ⌥↑/⌥↓ re-expresses the engine's answer on its SI prefix ladder; what's shown is what's copied and saved
    let js_value = line_value.filter(|_| !graph_cmd && !js_display.is_empty() && merged_display == js_display);
    let stepped = match (&req.prefix_unit, js_value) {
        (Some(u), Some(v)) => in_ladder_unit(v, u),
        _ => None,
    };
    let base_display = stepped
        .as_ref()
        .map_or_else(|| merged_display.clone(), |s| format_value(s, sig_figs));
    let base_exact = if graph_cmd || stepped.is_some() || jstr::trim(q).is_empty() || js_display.is_empty() {
        None
    } else {
        line.as_ref().and_then(|l| l.exact.clone()).or_else(|| req.closed_form.clone())
    };
    // Tab re-expresses the answer; the form shown is what's copied and saved
    let form_value = stepped.clone().or_else(|| js_value.cloned());
    let tab_form = match (&form_value, req.tab) {
        (Some(v), k) if k > 0 && !base_display.is_empty() => {
            let forms = answer_forms(&FormSource {
                value: v.clone(),
                display: base_display.clone(),
                exact: base_exact.clone(),
                meas: line.as_ref().and_then(|l| l.meas.clone()),
                sig_figs,
                sig_fig_mode: o.sig_fig_mode,
                rationalize: o.rationalize,
            });
            (!forms.is_empty()).then(|| forms[(k - 1) % forms.len()].clone())
        }
        _ => None,
    };
    let display = if sys_cmd {
        if sys_message { String::new() } else { sys_shown.clone() }
    } else {
        tab_form.as_ref().map_or_else(|| base_display.clone(), |f| f.display.clone())
    };
    let number = if sys_cmd {
        None
    } else if let Some(v) = tab_form.as_ref().and_then(|f| f.value.as_ref()) {
        Some(v.n)
    } else if let Some(s) = &stepped {
        Some(s.n)
    } else {
        merged_n
    };
    let exact = if sys_cmd {
        if sys_message { None } else { sys_answer.as_ref().and_then(|a| a.exact.clone()) }
    } else if tab_form.is_some() {
        None
    } else {
        base_exact.clone()
    };
    let shown = if sys_cmd {
        display.clone()
    } else if graph_cmd {
        String::new()
    } else {
        visible_answer(&display, exact.as_deref(), req.answer_form).to_string()
    };
    let from_js = !display.is_empty() && display == js_display;
    let solve = line
        .as_ref()
        .filter(|l| from_js && l.kind == LineKind::Solve)
        .and_then(|l| l.solve.clone());
    let roots_of = solve
        .as_ref()
        .filter(|s| s.outcome == SolveOutcome::Roots)
        .map(|s| s.variable.clone());
    let improper = is_improper_unit_conversion(&display);
    let steady = if a_command || help || games {
        None
    } else if !shown.is_empty() && !improper {
        let label = dual_label(exact.as_deref(), &display);
        Some(match &roots_of {
            Some(v) => format!("{v} = {label}"),
            None => label,
        })
    } else {
        Some(String::new())
    };
    let stepped_or_js = stepped.as_ref().or(js_value);
    let facts = HintFacts {
        expr: q.to_string(),
        answer: Some(display.clone()),
        variable: line
            .as_ref()
            .filter(|l| from_js && l.kind == LineKind::Assignment)
            .and_then(|l| l.variable.clone()),
        unit: Some(stepped_or_js.is_some_and(|v| v.unit.as_deref().is_some_and(|u| !u.is_empty()))),
        chained: Some(chained),
        angle_mode: Some(o.angle()),
        fraction_mode: Some(o.fraction_mode == Some(true)),
        native: None,
        opens: None,
    };
    let squiggle = if display.is_empty()
        && !jstr::trim(q).is_empty()
        && !a_command
        && !help
        && !games
        && !looks_like_natural_language(q)
    {
        blank_reason(q, &evaluates(&options, chained), &names_of(&options))
    } else {
        None
    };
    let identity_slot = identity.as_ref().map(|t| t.hint.clone());
    let slot = if sys_cmd {
        sys_shown.clone()
    } else if greeting {
        GREETING_REPLY.into()
    } else if message_cmd {
        parse_message(q).map_or_else(String::new, |l| message_hint(&l, &req.messaging, req.relay))
    } else if periodic_cmd {
        PERIODIC_HINT.into()
    } else if let Some(h) = &identity_slot {
        h.clone()
    } else if tutorial {
        TUTORIAL_HINT.into()
    } else if games {
        GAMES_HINT.into()
    } else if let Some(g) = pong {
        game_hint(g).into()
    } else if graph_cmd {
        String::new()
    } else {
        display.clone()
    };
    let message = sys_message
        || periodic_cmd
        || games
        || identity.is_some()
        || tutorial
        || pong.is_some()
        || solve.as_ref().is_some_and(|s| s.outcome != SolveOutcome::Roots);
    let sides = Sides {
        exact: match &exact {
            Some(e) if !e.is_empty() => {
                if solve.is_some() { e.clone() } else { insertable_answer(e, None, f64::from(DEFAULT_SIG_FIGS)) }
            }
            _ => String::new(),
        },
        approx: if solve.is_some() {
            display.clone()
        } else if improper {
            String::new()
        } else {
            insertable_answer(&display, number, sig_figs)
        },
    };
    let roots = solve.as_ref().and_then(|s| {
        solve_roots(&json!({ "solve": serde_json::to_value(s).unwrap_or(Json::Null) }), sig_figs)
    });
    let typst_shown = req.typst_preview && !a_command && !help && !games && typst_preview_useful(&typst_expr);
    let typst_ans = if graph_cmd
        || periodic_cmd
        || identity.is_some()
        || pong.is_some()
        || tutorial
        || sys_message
        || (solve.is_some() && roots_of.is_none())
    {
        String::new()
    } else {
        typst_answer(exact.as_deref(), &display)
    };
    let completion = completion_for(
        q,
        &CompletionNames {
            variables: options.variables.keys().cloned().collect(),
            functions: options.functions.keys().cloned().collect(),
            ans: o.ans.is_some(),
            contacts: req.contacts.clone(),
        },
    );
    let command = if graph_cmd {
        Some(LiveCommand::Graph)
    } else if sys_cmd {
        Some(LiveCommand::Sys)
    } else if greeting {
        Some(LiveCommand::Greeting)
    } else if message_cmd {
        Some(LiveCommand::Message)
    } else if periodic_cmd {
        Some(LiveCommand::Periodic)
    } else if let Some(t) = &identity {
        Some(LiveCommand::Identity {
            sheet: t.sheet.id.clone(),
            section: t.section,
        })
    } else if let Some(g) = pong {
        Some(LiveCommand::Game { game: g })
    } else if tutorial {
        Some(LiveCommand::Tutorial)
    } else if games {
        Some(LiveCommand::Games)
    } else if help {
        Some(LiveCommand::Help)
    } else {
        None
    };
    LiveAnswer {
        slot,
        message,
        unit: stepped_or_js.and_then(|v| v.unit.clone()),
        value: form_value,
        meas: line.as_ref().filter(|_| from_js).and_then(|l| l.meas.clone()),
        quantity: line.as_ref().filter(|_| js_value.is_some()).and_then(|l| l.quantity.clone()),
        solve,
        roots_of,
        roots,
        sides,
        steady,
        squiggle,
        completion,
        command,
        expr,
        tape_expr,
        chained,
        phrase,
        improper,
        function_def,
        typst_shown,
        typst_expr,
        typst_answer: typst_ans,
        facts,
        line,
        display,
        exact,
        shown,
        number,
    }
}

/// The context the tape gives the next line, as `QuickCalc.tsx` derives it: `ans` from the last answer row,
/// variables, functions, measures and quantities from the rows in scope (`scopeStart`), the chain answer and the
/// last row's expression. `rows` are the TS `HistoryRow` JSON.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveContext {
    pub ans: Option<f64>,
    pub variables: std::collections::BTreeMap<String, f64>,
    pub functions: std::collections::BTreeMap<String, UserFunction>,
    pub measures: std::collections::BTreeMap<String, Meas>,
    pub quantities: std::collections::BTreeMap<String, String>,
    pub chain_answer: Option<ChainAnswer>,
    pub last_expr: Option<String>,
}

impl LiveContext {
    /// `options` with this context in it.
    pub fn apply(&self, options: &EvaluateOptions) -> EvaluateOptions {
        EvaluateOptions {
            ans: self.ans,
            variables: self.variables.clone(),
            functions: self.functions.clone(),
            measures: self.measures.clone(),
            quantities: self.quantities.clone(),
            ..options.clone()
        }
    }
}

pub fn context_from_history(
    rows: &[Json],
    show: HistoryShow,
    now_ms: f64,
    answer_form: AnswerForm,
    sig_figs: f64,
) -> LiveContext {
    let scoped = &rows[scope_start(rows, show, now_ms)..];
    let functions = hist::history_functions(scoped)
        .into_iter()
        .filter_map(|(name, f)| Some((name, serde_json::from_value(f).ok()?)))
        .collect();
    let chain_answer = hist::last_answer_row(rows).and_then(|last| {
        let plain = insertable_history_answer(last, answer_form, sig_figs);
        let quantity = last.get("quantity").and_then(Json::as_str);
        let n_ok = last.get("n").and_then(Json::as_f64).is_some_and(f64::is_finite);
        // a chain needs the last answer row to be the one `ans` means
        (!plain.is_empty() && (n_ok || hist::is_literal_quantity(quantity))).then(|| ChainAnswer {
            plain,
            unit: last.get("quantity").is_some_and(|q| !q.is_null()) && !hist::is_literal_quantity(quantity),
        })
    });
    LiveContext {
        ans: hist::last_history_number(rows),
        variables: hist::history_variables(scoped).into_iter().collect(),
        functions,
        measures: hist::history_measures(scoped)
            .into_iter()
            .filter_map(|(k, m)| Some((k, serde_json::from_value(m).ok()?)))
            .collect(),
        quantities: hist::history_quantities(scoped)
            .into_iter()
            .filter_map(|(k, q)| Some((k, q.as_str()?.to_string())))
            .collect(),
        chain_answer,
        last_expr: rows
            .last()
            .and_then(|r| r.get("expr"))
            .and_then(Json::as_str)
            .map(str::to_string),
    }
}

/// What ↵ did.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub saved: bool,
    /// The row saved (or refreshed, when it repeats the last one).
    pub row: Option<Json>,
    /// The tape after it, as stored.
    pub history: Vec<Json>,
    /// Why nothing was saved (`enterHint`).
    pub hint: Option<String>,
    /// What commit hints see (`pickHint(…, 'commit')`).
    pub facts: HintFacts,
    /// The tour steps and extras this commit counts toward (`tourUsed`).
    pub tour_bits: u32,
}

fn fn_def_text(f: &FunctionDef) -> String {
    format!("{}({}) = {}", f.name, f.params.join(", "), f.body)
}

/// ↵ on a line (the TS `commit`, minus analytics and the caller's onboarding): the row it saves and the tape after
/// it, or the hint saying why nothing was saved. `row_id` names the new row (`newRowId`; the caller owns the clock
/// and randomness), and `req.now_ms` is its `at`.
pub fn commit(req: &LiveRequest, rows: &[Json], row_id: &str) -> Commit {
    static BARE_GRAPH: Lazy = Lazy::new(|| js(r"^\s*graph\s*$", "i"));
    let live = live(req);
    let q = req.text.as_str();
    let sys_now = req.sys_lines.as_ref().filter(|_| is_sys_command(q));
    let expr = if sys_now.is_some() {
        "sys".to_string()
    } else if live.tape_expr.is_empty() {
        q.to_string()
    } else {
        live.tape_expr.clone()
    };
    let fn_def = live.function_def.clone().or_else(|| {
        parse_function_def(jstr::trim(&expr)).map(|(name, params, body)| FunctionDef { name, params, body })
    });
    let is_graph = is_graph_command(&expr);
    let written = sys_now
        .map(|lines| {
            lines
                .iter()
                .map(|l| jstr::trim(l))
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join("; ")
        })
        .unwrap_or_default();
    let shown = if sys_now.is_some() {
        let s = jstr::trim(&live.slot).to_string();
        if s.is_empty() { written } else { s }
    } else if !live.display.is_empty() {
        live.display.clone()
    } else {
        fn_def.as_ref().map(fn_def_text).unwrap_or_default()
    };
    let facts = HintFacts {
        expr: expr.clone(),
        ..live.facts.clone()
    };
    let unsaved = |hint: Option<String>| Commit {
        saved: false,
        row: None,
        history: rows.to_vec(),
        hint,
        facts: facts.clone(),
        tour_bits: 0,
    };
    if jstr::trim(&expr).is_empty() || shown.is_empty() || is_improper_unit_conversion(&shown) {
        let graph = is_graph_command(q);
        let span = if graph || jstr::trim(q).is_empty() {
            None
        } else {
            blank_reason(q, &evaluates(&req.options, live.chained), &names_of(&req.options))
        };
        return unsaved(Some(enter_hint(&EnterFacts {
            expr: q.to_string(),
            span,
            improper: Some(is_improper_unit_conversion(&live.display)),
            natural_language: Some(looks_like_natural_language(q)),
            bare_graph: Some(graph && jsre::test(&BARE_GRAPH, q)),
            history: Some(!rows.is_empty()),
        })));
    }
    let tour_bits = tour_used(&TourFacts {
        expr: expr.clone(),
        unit: live.facts.unit == Some(true) || live.quantity.is_some(),
        chained: live.chained,
        solve: live.solve.is_some(),
        graph: is_graph,
    });
    // inline `graph f(x)=…` registers the function; plain `graph expr` is not saved
    if is_graph && fn_def.is_none() {
        return Commit {
            saved: true,
            row: None,
            history: rows.to_vec(),
            hint: None,
            facts,
            tour_bits,
        };
    }
    let mut row = Map::new();
    row.insert("id".into(), json!(row_id));
    match &fn_def {
        Some(f) => {
            let text = fn_def_text(f);
            row.insert("expr".into(), json!(if is_graph { text.clone() } else { expr.clone() }));
            row.insert("display".into(), json!(text));
            row.insert("kind".into(), json!("function"));
            row.insert("fnName".into(), json!(f.name));
            row.insert("fnParams".into(), json!(f.params));
            row.insert("fnBody".into(), json!(f.body));
        }
        None => {
            row.insert("expr".into(), json!(expr));
            row.insert("display".into(), json!(shown));
            if let Some(e) = live.exact.as_ref().filter(|e| **e != shown && !e.is_empty()) {
                row.insert("exact".into(), json!(e));
            }
            if let Some(n) = live.number.filter(|n| n.is_finite()) {
                row.insert("n".into(), json!(n));
            }
            if let Some(m) = &live.meas {
                row.insert("meas".into(), serde_json::to_value(m).unwrap_or(Json::Null));
            }
            if let Some(q) = &live.quantity {
                row.insert("quantity".into(), json!(q));
            }
            if let Some(s) = &live.solve {
                row.insert("solve".into(), serde_json::to_value(s).unwrap_or(Json::Null));
            }
            if let Some(lines) = sys_now {
                row.insert("kind".into(), json!("system"));
                row.insert("equations".into(), json!(lines.iter().take(5).collect::<Vec<_>>()));
            }
        }
    }
    let Some(mut next) = hist::slim_history_row(&Json::Object(row)) else {
        return unsaved(None);
    };
    next["at"] = json!(req.now_ms);
    let last = rows.last();
    let system = next.get("kind").and_then(Json::as_str) == Some("system");
    let same_system = system && last.and_then(|l| l.get("equations")) == next.get("equations");
    let same = last.is_some_and(|l| {
        l.get("expr") == next.get("expr") && l.get("display") == next.get("display") && (!system || same_system)
    });
    let (saved_row, history) = if same {
        let mut refreshed = last.cloned().unwrap_or(Json::Null);
        refreshed["at"] = json!(req.now_ms);
        let mut kept = rows[..rows.len() - 1].to_vec();
        kept.push(refreshed.clone());
        (refreshed, kept)
    } else {
        let mut all = rows.to_vec();
        all.push(next.clone());
        (next, hist::persistable_history(&all))
    };
    Commit {
        saved: true,
        row: Some(saved_row),
        history,
        hint: None,
        facts,
        tour_bits,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(text: &str) -> LiveRequest {
        LiveRequest {
            text: text.into(),
            phrases: true,
            options: EvaluateOptions {
                sig_figs: Some(6.0),
                ..EvaluateOptions::default()
            },
            ..LiveRequest::default()
        }
    }

    #[test]
    fn answers_chains_and_commands() {
        let a = live(&req("2+3"));
        assert_eq!((a.slot.as_str(), a.shown.as_str(), a.message), ("5", "5", false));
        let mut chain = req("*2");
        chain.chain_answer = Some(ChainAnswer { plain: "-3".into(), unit: false });
        chain.options.ans = Some(-3.0);
        let c = live(&chain);
        assert!(c.chained);
        assert_eq!((c.display.as_str(), c.tape_expr.as_str()), ("-6", "(-3)*2"));
        let p = live(&req("periodic"));
        assert_eq!((p.slot.as_str(), p.message), ("periodic table", true));
        assert_eq!(live(&req("hello")).slot, GREETING_REPLY);
        let s = live(&req("2+*3"));
        assert_eq!(s.squiggle, Some(Span { start: 2, end: 3 }));
    }

    #[test]
    fn commits_rows() {
        let done = commit(&req("2+3"), &[], "a");
        assert!(done.saved);
        assert_eq!(done.history.len(), 1);
        assert_eq!(done.history[0]["display"], "5");
        let again = commit(&req("2+3"), &done.history, "b");
        assert_eq!(again.history.len(), 1);
        assert_eq!(again.history[0]["id"], "a");
        let blank = commit(&req("2+"), &[], "c");
        assert!(!blank.saved);
        assert_eq!(blank.hint.as_deref(), Some("unfinished: something goes after the +"));
        let ctx = context_from_history(&done.history, HistoryShow::Always, 0.0, AnswerForm::Exact, 6.0);
        assert_eq!(ctx.ans, Some(5.0));
        assert_eq!(ctx.chain_answer.map(|c| c.plain), Some("5".into()));
    }
}
