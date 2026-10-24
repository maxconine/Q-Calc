//! The `Engine` object (ARCHITECTURE.md §4, `native/QCalcCore/API.md`). Every call hops to the engine's
//! 32 MB evaluation thread (`worker.rs`); the async calls go to the long-work pool with a `CancelToken`.
//!
//! The per-keystroke flow (`live`, `commit`, `chains`) follows `src/components/QuickCalc.tsx` (≈ lines 480–700
//! and `commit`) until WP-APP-LOGIC's `app/live.rs` takes it over. Calls into code other packages are porting go
//! through `bridge.rs` and answer as soon as those packages register their functions.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use qcalc_core::format::pretty_answer;
use qcalc_core::jsre::{self, Lazy, js};
use qcalc_core::simplify::dual_label;
use qcalc_core::types::{EvaluateOptions, LineKind};
use qcalc_core::units::is_improper_unit_conversion;
use serde_json::{Map, Value as Json, json};

use crate::bridge::{self, call};
use crate::cancel::CancelToken;
use crate::chain::{
    ans_written_out, chained_expr, chained_history_expr, chains_from_answer,
    insertable_history_answer, is_graph_command,
};
use crate::history::{self as hist, is_literal_quantity};
use crate::jsstr;
use crate::rates::{self, Rates};
use crate::records::*;
use crate::worker::Workers;

/// The engine: settings-independent state (rates, its threads), shared by every call.
#[derive(Debug, uniffi::Object)]
pub struct Engine {
    state: Arc<State>,
    workers: Workers,
}

#[derive(Debug)]
pub(crate) struct State {
    config: EngineConfig,
    rates: RwLock<Option<Rates>>,
}

// ---- small conversions ----

fn options_json(o: &EvalOptions) -> Json {
    serde_json::to_value(EvaluateOptions::from(o)).unwrap_or(Json::Null)
}

fn names_json(n: &CompletionNames) -> Json {
    json!({ "variables": n.variables, "functions": n.functions, "ans": n.ans, "contacts": n.contacts })
}

fn undef() -> Json {
    json!({ "$undef": true })
}

fn opt_string(v: Option<Json>) -> Option<String> {
    v.and_then(|v| v.as_str().map(str::to_string))
}

fn u32_of(v: &Json, key: &str) -> Option<u32> {
    v.get(key)?.as_f64().filter(|x| *x >= 0.0).map(|x| x as u32)
}

fn span_of(v: &Json) -> Option<Span> {
    Some(Span {
        start: u32_of(v, "start")?,
        end: u32_of(v, "end")?,
    })
}

fn graph_options_json(o: &GraphOptions, settings: &EvalOptions) -> Json {
    let core = EvaluateOptions::from(settings);
    let mut out = json!({
        "variables": core.variables,
        "functions": core.functions,
        "angleMode": core.angle(),
        "findRoots": o.find_roots,
    });
    if let Some(ans) = core.ans {
        out["ans"] = json!(ans);
    }
    if let (Some(lo), Some(hi)) = (o.domain_lo, o.domain_hi) {
        out["domain"] = json!([lo, hi]);
    }
    if let Some(n) = o.sample_count {
        out["sampleCount"] = json!(n);
    }
    if let Some(a) = o.aspect {
        out["aspect"] = json!(a);
    }
    out
}

fn function_def_of(v: Option<&Json>) -> Option<FunctionDef> {
    let v = v.filter(|v| v.is_object())?;
    Some(FunctionDef {
        name: v.get("name")?.as_str()?.to_string(),
        params: v
            .get("params")?
            .as_array()?
            .iter()
            .filter_map(Json::as_str)
            .map(str::to_string)
            .collect(),
        body: v.get("body")?.as_str()?.to_string(),
    })
}

fn graph_result(v: &Json) -> Option<GraphResult> {
    let intent = v.get("intent")?;
    let num = |x: Option<&Json>| x.and_then(Json::as_f64);
    let domain = v.get("domain").and_then(Json::as_array);
    let points = v
        .get("points")
        .and_then(Json::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    Some(GraphPoint {
                        x: num(p.get("x"))?,
                        y: num(p.get("y")),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let critical_points = v
        .get("criticalPoints")
        .and_then(Json::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    Some(CriticalPoint {
                        x: num(p.get("x"))?,
                        y: num(p.get("y"))?,
                        kind: match p.get("kind")?.as_str()? {
                            "min" => CriticalKind::Min,
                            "max" => CriticalKind::Max,
                            _ => CriticalKind::Critical,
                        },
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let contours: Vec<ContourLevel> = v
        .get("contours")
        .and_then(Json::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|c| {
                    let at = c.get("at").filter(|a| a.is_object());
                    Some(ContourLevel {
                        level: num(c.get("level"))?,
                        label: c.get("label")?.as_str()?.to_string(),
                        segments: c
                            .get("segments")?
                            .as_array()?
                            .iter()
                            .filter_map(Json::as_f64)
                            .collect(),
                        at_x: at.and_then(|a| num(a.get("x"))),
                        at_y: at.and_then(|a| num(a.get("y"))),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    // the readout functions (`y`, `z`) don't cross; graph_y and graph_z stand for them
    let mut json = v.clone();
    if let Some(o) = json.as_object_mut() {
        o.remove("y");
        o.remove("z");
    }
    Some(GraphResult {
        label: intent
            .get("label")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string(),
        expression: intent
            .get("expression")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string(),
        function_def: function_def_of(intent.get("functionDef")),
        points,
        domain_lo: domain.and_then(|d| num(d.first())).unwrap_or(-10.0),
        domain_hi: domain.and_then(|d| num(d.get(1))).unwrap_or(10.0),
        y_min: num(v.pointer("/yScale/min")).unwrap_or(f64::NAN),
        y_max: num(v.pointer("/yScale/max")).unwrap_or(f64::NAN),
        critical_points,
        roots: v
            .get("roots")
            .and_then(Json::as_array)
            .map(|a| a.iter().filter_map(|r| num(r.get("x"))).collect())
            .unwrap_or_default(),
        angle_unit: match v.get("angleUnit").and_then(Json::as_str) {
            Some("deg") => Some(AngleUnit::Deg),
            Some("rad") => Some(AngleUnit::Rad),
            _ => None,
        },
        contour: v.get("contours").is_some_and(Json::is_array)
            || intent.get("contour").is_some_and(|c| !c.is_null()),
        contours,
        error: v.get("error").and_then(Json::as_str).map(str::to_string),
        json: json.to_string(),
    })
}

/// `soulverAngleSafe` (`src/lib/nativeEval.ts`): a phrase can't be trusted with trig in degrees.
fn angle_safe(expr: &str, radians: bool) -> bool {
    static TRIG_CALL: Lazy = Lazy::new(|| {
        js(
            r"(?<![A-Za-z])(?:a|arc)?(?:sin|cos|tan|sec|csc|cot)(?![A-Za-z])",
            "i",
        )
    });
    radians || !jsre::test(&TRIG_CALL, expr)
}

/// `fnDefText` (`QuickCalc.tsx`).
fn fn_def_text(f: &FunctionDef) -> String {
    format!("{}({}) = {}", f.name, f.params.join(", "), f.body)
}

impl State {
    fn data_file(&self, name: &str) -> Option<PathBuf> {
        self.config
            .data_dir
            .as_ref()
            .filter(|d| !d.is_empty())
            .map(|d| PathBuf::from(d).join(name))
    }

    fn rates_table(&self, now_ms: i64) -> Json {
        self.rates
            .read()
            .ok()
            .and_then(|r| r.as_ref().map(|r| r.table(now_ms)))
            .unwrap_or(Json::Null)
    }

    // ---- bridged pieces ----

    fn looks_like_natural_language(&self, text: &str) -> bool {
        call("lib/nativeEval", "looksLikeNaturalLanguage", &[json!(text)])
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    fn isolate_previous(&self, text: &str, previous: Option<&str>) -> Option<String> {
        let prev = previous.map_or_else(undef, |p| json!(p));
        opt_string(call(
            "engine/isolate",
            "isolatePrevious",
            &[json!(text), prev],
        ))
    }

    /// `withPhraseAnswer` (`src/lib/phraseLive.ts`) with the bar's merge (`mergeLiveAnswer`, no native answer).
    fn phrase(
        &self,
        expr: &str,
        js_display: &str,
        sig_figs: u32,
        now_ms: i64,
    ) -> Option<(String, Option<f64>)> {
        if jsstr::trim(expr).is_empty() || qcalc_core::stubs::is_calculus_input(expr) {
            return None;
        }
        if !js_display.is_empty() && !self.looks_like_natural_language(expr) {
            return None;
        }
        let options = json!({
            "now": bridge::date(now_ms),
            "rates": self.rates_table(now_ms),
            "sigFigs": sig_figs,
        });
        let answer = call("engine/phrases", "answerPhrase", &[json!(expr), options])?;
        let display = answer.get("display")?.as_str()?.to_string();
        Some((display, answer.get("n").and_then(Json::as_f64)))
    }

    pub(crate) fn completion(&self, text: &str, names: &CompletionNames) -> Option<String> {
        opt_string(call(
            "lib/completion",
            "completionFor",
            &[json!(text), names_json(names)],
        ))
        .filter(|s| !s.is_empty())
    }

    fn blank_reason_with(
        &self,
        text: &str,
        options: &EvalOptions,
        names: &CompletionNames,
        chained: bool,
    ) -> Option<Span> {
        if jsstr::trim(text).is_empty() {
            return None;
        }
        let ok = bridge::eval_non_blank(&options_json(options), chained);
        let names = json!({ "variables": names.variables, "functions": names.functions });
        call("lib/blankReason", "blankReason", &[json!(text), ok, names])
            .as_ref()
            .and_then(span_of)
    }

    fn context_names(context: &EvalContext, functions: &[String]) -> CompletionNames {
        let mut fns: Vec<String> = context.functions.keys().cloned().collect();
        fns.extend(functions.iter().cloned());
        fns.sort();
        fns.dedup();
        let mut vars: Vec<String> = context.variables.keys().cloned().collect();
        vars.sort();
        CompletionNames {
            variables: vars,
            functions: fns,
            ans: context.ans.is_some(),
            contacts: Vec::new(),
        }
    }

    // ---- the bar ----

    pub(crate) fn chains(&self, text: &str, context: &EvalContext) -> bool {
        !is_graph_command(text)
            && self
                .isolate_previous(text, context.last_expr.as_deref())
                .is_none()
            && chains_from_answer(text, context.chain_answer.as_ref())
    }

    pub(crate) fn live(&self, req: &LiveRequest) -> LiveAnswer {
        let q = req.text.as_str();
        let graph = is_graph_command(q);
        let isolated = self.isolate_previous(q, req.context.last_expr.as_deref());
        let chained = req.chained && !graph && isolated.is_none();
        let expr = isolated.unwrap_or_else(|| {
            if chained {
                chained_expr(q)
            } else {
                q.to_string()
            }
        });
        let settings = merge(&req.options, &req.context);
        if graph {
            // the graph panel labels the curve; the answer slot stays empty (`graph(…)` draws it)
            return LiveAnswer::blank(&expr);
        }
        let r = qcalc_core::evaluate_line(&expr, &(&settings).into());
        let js_display = if jsstr::trim(q).is_empty() {
            String::new()
        } else {
            r.display.clone()
        };
        let js_n = if js_display.is_empty() {
            None
        } else {
            number_of(&r)
        };
        let phrase = (req.phrases && !chained && angle_safe(q, settings.angle_radians))
            .then(|| self.phrase(q, &js_display, settings.sig_figs, req.now_ms))
            .flatten();
        let (display, number, from_phrase) = match phrase {
            Some((d, n)) => (d, n, true),
            None => (js_display.clone(), js_n, false),
        };
        let exact = if js_display.is_empty() {
            None
        } else {
            r.exact.clone()
        };
        let from_js = !display.is_empty() && display == js_display;
        let value = r.value.as_ref().filter(|_| from_js);
        let blank = if display.is_empty()
            && !jsstr::trim(q).is_empty()
            && !self.looks_like_natural_language(q)
        {
            let names = Self::context_names(&req.context, &[]);
            self.blank_reason_with(q, &settings, &names, chained)
        } else {
            None
        };
        LiveAnswer {
            pretty: pretty_answer(&display),
            dual_label: dual_label(exact.as_deref(), &display),
            exact,
            number,
            unit: value.and_then(|v| v.unit.clone()),
            kind: if from_js {
                r.kind.into()
            } else {
                AnswerKind::Expression
            },
            blank,
            completion: self.completion(q, &Self::context_names(&req.context, &[])),
            solve: r
                .solve
                .as_ref()
                .filter(|_| from_js && r.kind == LineKind::Solve)
                .map(SolveInfo::from),
            closed_form_job: r
                .closed_form
                .as_ref()
                .filter(|_| from_js)
                .map(Json::to_string),
            time_letters: r.time.as_ref().filter(|_| from_js).map(TimeLetters::from),
            quantity: r.quantity.clone().filter(|_| from_js),
            variable: r
                .variable
                .clone()
                .filter(|_| from_js && r.kind == LineKind::Assignment),
            meas: r.meas.as_ref().filter(|_| from_js).map(MeasInfo::from),
            value_json: value.and_then(|v| serde_json::to_string(v).ok()),
            expr,
            phrase: from_phrase,
            improper: is_improper_unit_conversion(&display),
            json: serde_json::to_string(&r).unwrap_or_default(),
            display,
        }
    }

    /// Enter (`commit` in `QuickCalc.tsx`, minus analytics, onboarding and the `sys` editor).
    pub(crate) fn commit(&self, req: &LiveRequest, history: &[HistoryRow]) -> CommitResult {
        let rows: Vec<Json> = history.iter().map(hist::row_json).collect();
        let live = self.live(req);
        let q = req.text.as_str();
        let graph_q = is_graph_command(q);
        let isolated = self.isolate_previous(q, req.context.last_expr.as_deref());
        let chain = req.context.chain_answer.as_ref();
        let chained = req.chained && !graph_q && isolated.is_none();
        let tape = match chain {
            Some(c) if chained => chained_history_expr(q, &c.plain),
            _ => q.to_string(),
        };
        let written = isolated.unwrap_or_else(|| match chain {
            Some(c) => ans_written_out(&tape, &c.plain),
            None if chained => tape.clone(),
            None => String::new(),
        });
        let expr = if written.is_empty() {
            q.to_string()
        } else {
            written
        };
        let graph_fn = if graph_q {
            let functions = json!({ "functions": EvaluateOptions::from(&merge(&req.options, &req.context)).functions });
            call("engine/graph", "parseGraphIntent", &[json!(q), functions])
                .and_then(|i| function_def_of(i.get("functionDef")))
        } else {
            None
        };
        let fn_def = graph_fn.or_else(|| {
            qcalc_core::evaluate::parse_function_def(jsstr::trim(&expr))
                .map(|(name, params, body)| FunctionDef { name, params, body })
        });
        let is_graph = is_graph_command(&expr);
        let shown = if live.display.is_empty() {
            fn_def.as_ref().map(fn_def_text).unwrap_or_default()
        } else {
            live.display.clone()
        };
        let unchanged = |hint: Option<String>| CommitResult {
            saved: false,
            row: None,
            history: history.to_vec(),
            input: q.to_string(),
            hint,
            json: "null".into(),
        };
        if jsstr::trim(&expr).is_empty() || shown.is_empty() || is_improper_unit_conversion(&shown)
        {
            return unchanged(self.enter_hint(req, &live, !rows.is_empty()));
        }
        // inline `graph f(x)=…` registers the function; plain `graph expr` is not saved
        if is_graph && fn_def.is_none() {
            return CommitResult {
                saved: true,
                row: None,
                history: history.to_vec(),
                input: String::new(),
                hint: None,
                json: "null".into(),
            };
        }
        let mut row = Map::new();
        row.insert("id".into(), json!(hist::new_row_id()));
        match &fn_def {
            Some(f) => {
                let text = fn_def_text(f);
                row.insert(
                    "expr".into(),
                    json!(if is_graph { text.clone() } else { expr.clone() }),
                );
                row.insert("display".into(), json!(text));
                row.insert("kind".into(), json!("function"));
                row.insert("fnName".into(), json!(f.name));
                row.insert("fnParams".into(), json!(f.params));
                row.insert("fnBody".into(), json!(f.body));
            }
            None => {
                row.insert("expr".into(), json!(expr));
                row.insert("display".into(), json!(shown));
                if let Some(e) = live.exact.as_ref().filter(|e| **e != shown) {
                    row.insert("exact".into(), json!(e));
                }
                if let Some(n) = live.number.filter(|n| n.is_finite()) {
                    row.insert("n".into(), json!(n));
                }
                if let Some(m) = &live.meas {
                    row.insert("meas".into(), hist::meas_json(m));
                }
                if let Some(q) = &live.quantity {
                    row.insert("quantity".into(), json!(q));
                }
                if let Some(s) = &live.solve {
                    row.insert("solve".into(), hist::solve_json(s));
                }
            }
        }
        let Some(mut next) = hist::slim_history_row(&Json::Object(row)) else {
            return unchanged(None);
        };
        next["at"] = json!(req.now_ms);
        let last = rows.last();
        let same = last.is_some_and(|l| {
            l.get("expr") == next.get("expr")
                && l.get("display") == next.get("display")
                && next.get("kind").and_then(Json::as_str) != Some("system")
        });
        let (saved_row, kept) = if same {
            let mut refreshed = last.cloned().unwrap_or(Json::Null);
            refreshed["at"] = json!(req.now_ms);
            let mut kept = rows[..rows.len() - 1].to_vec();
            kept.push(refreshed.clone());
            (refreshed, kept)
        } else {
            let mut all = rows.clone();
            all.push(next.clone());
            (next, hist::persistable_history(&all))
        };
        CommitResult {
            saved: true,
            json: saved_row.to_string(),
            row: Some(hist::row_from_json(&saved_row)),
            history: kept.iter().map(hist::row_from_json).collect(),
            input: String::new(),
            hint: None,
        }
    }

    /// `blankEnterHint` (`QuickCalc.tsx`): why Enter saved nothing.
    fn enter_hint(
        &self,
        req: &LiveRequest,
        live: &LiveAnswer,
        has_history: bool,
    ) -> Option<String> {
        static BARE_GRAPH: Lazy = Lazy::new(|| js(r"^\s*graph\s*$", "i"));
        let q = req.text.as_str();
        let graph = is_graph_command(q);
        let settings = merge(&req.options, &req.context);
        let chained = req.chained && !graph;
        let span = if graph || jsstr::trim(q).is_empty() {
            None
        } else {
            self.blank_reason_with(
                q,
                &settings,
                &Self::context_names(&req.context, &[]),
                chained,
            )
        };
        let facts = json!({
            "expr": q,
            "span": span.map_or(Json::Null, |s| json!({ "start": s.start, "end": s.end })),
            "improper": is_improper_unit_conversion(&live.display),
            "naturalLanguage": self.looks_like_natural_language(q),
            "bareGraph": graph && jsre::test(&BARE_GRAPH, q),
            "history": has_history,
        });
        opt_string(call("lib/blankReason", "enterHint", &[facts]))
    }

    // ---- history ----

    pub(crate) fn context_from_history(
        &self,
        rows: &[HistoryRow],
        view: &HistoryView,
    ) -> EvalContext {
        let rows: Vec<Json> = rows.iter().map(hist::row_json).collect();
        let scoped = &rows[hist::scope_start(&rows, view.show, view.now_ms as f64)..];
        let functions = hist::history_functions(scoped)
            .into_iter()
            .filter_map(|(name, f)| {
                let params = f
                    .get("params")?
                    .as_array()?
                    .iter()
                    .filter_map(Json::as_str)
                    .map(str::to_string)
                    .collect();
                let body = f.get("body")?.as_str()?.to_string();
                Some((name, UserFn { params, body }))
            })
            .collect();
        let chain_answer = hist::last_answer_row(&rows).and_then(|last| {
            let plain = insertable_history_answer(
                last,
                view.answer_form == AnswerForm::Exact,
                f64::from(view.sig_figs),
            );
            let quantity = last.get("quantity").and_then(Json::as_str);
            let n_ok = last
                .get("n")
                .and_then(Json::as_f64)
                .is_some_and(f64::is_finite);
            (!plain.is_empty() && (n_ok || is_literal_quantity(quantity))).then(|| ChainAnswer {
                plain,
                unit: last.get("quantity").is_some_and(|q| !q.is_null())
                    && !is_literal_quantity(quantity),
            })
        });
        EvalContext {
            ans: hist::last_history_number(&rows),
            variables: hist::history_variables(scoped).into_iter().collect(),
            functions,
            measures: hist::history_measures(scoped)
                .into_iter()
                .filter_map(|(k, m)| Some((k, hist::meas_from_json(&m)?)))
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

    fn load_history(&self) -> Vec<HistoryRow> {
        let Some(path) = self.data_file("history.json") else {
            return Vec::new();
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return Vec::new();
        };
        hist::load_rows(&text)
            .iter()
            .map(hist::row_from_json)
            .collect()
    }

    fn save_history(&self, rows: &[HistoryRow]) -> Result<Vec<HistoryRow>, CoreError> {
        let kept = hist::persistable_history(&rows.iter().map(hist::row_json).collect::<Vec<_>>());
        let Some(path) = self.data_file("history.json") else {
            return Err(CoreError::io("the engine has no data directory"));
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(CoreError::io)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, hist::file_text(&kept)).map_err(CoreError::io)?;
        std::fs::rename(&tmp, &path).map_err(CoreError::io)?;
        Ok(kept.iter().map(hist::row_from_json).collect())
    }

    fn normalized_rows(rows_json: &str) -> Vec<Json> {
        // a bare array or the history file's envelope
        hist::load_rows(rows_json)
    }

    // ---- the rest ----

    fn evaluate(lines: &[String], options: &EvalOptions) -> Vec<LineAnswer> {
        qcalc_core::evaluate_sheet(lines, &options.into())
            .iter()
            .map(line_answer)
            .collect()
    }

    fn answer_forms(&self, source: &FormSource) -> Vec<AnswerView> {
        let Ok(value) = serde_json::from_str::<Json>(&source.value_json) else {
            return Vec::new();
        };
        let mut src = json!({
            "value": value,
            "display": source.display,
            "sigFigs": source.sig_figs,
            "sigFigMode": source.sig_fig_mode,
            "rationalize": source.rationalize,
        });
        if let Some(e) = &source.exact {
            src["exact"] = json!(e);
        }
        if let Some(m) = &source.meas {
            src["meas"] = hist::meas_json(m);
        }
        call("lib/answerForms", "answerForms", &[src])
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|f| {
                Some(AnswerView {
                    display: f.get("display")?.as_str()?.to_string(),
                    value_json: f
                        .get("value")
                        .filter(|v| v.is_object())
                        .map(Json::to_string),
                })
            })
            .collect()
    }

    fn spans(&self, text: &str, names: &CompletionNames) -> Vec<TextSpan> {
        let mut out = Vec::new();
        let mut add = |list: Option<Json>, kind: SpanKind, start_key: &str| {
            for s in list.and_then(|l| l.as_array().cloned()).unwrap_or_default() {
                let (Some(start), Some(end)) = (u32_of(&s, start_key), u32_of(&s, "end")) else {
                    continue;
                };
                out.push(TextSpan {
                    start,
                    end,
                    kind,
                    name: s.get("name").and_then(Json::as_str).map(str::to_string),
                    json: s.to_string(),
                });
            }
        };
        let t = json!(text);
        add(
            call(
                "lib/unitSpans",
                "unitSpans",
                &[t.clone(), json!(names.variables), json!(names.functions)],
            ),
            SpanKind::Unit,
            "start",
        );
        add(
            call(
                "lib/knownWords",
                "knownWordSpans",
                &[t.clone(), json!(names.functions), json!(names.ans)],
            ),
            SpanKind::KnownWord,
            "start",
        );
        add(
            call("lib/radical", "radicalSpans", std::slice::from_ref(&t)),
            SpanKind::Radical,
            "sign",
        );
        add(
            call("lib/bounds", "boundsIn", &[t]),
            SpanKind::Bound,
            "sign",
        );
        out.sort_by_key(|s| (s.start, s.end));
        out
    }

    fn graph(
        &self,
        input: &str,
        options: &GraphOptions,
        settings: &EvalOptions,
    ) -> Option<GraphResult> {
        call(
            "engine/graph",
            "buildGraph",
            &[json!(input), graph_options_json(options, settings)],
        )
        .as_ref()
        .and_then(graph_result)
    }
}

impl State {
    fn new(config: EngineConfig) -> State {
        State {
            config,
            rates: RwLock::new(None),
        }
    }

    fn set_rates(&self, text: &str) -> Result<bool, CoreError> {
        let parsed = rates::parse(text)?;
        let present = parsed.is_some();
        if let Ok(mut r) = self.rates.write() {
            *r = parsed;
        }
        Ok(present)
    }
}

impl Engine {
    /// Runs `f` on the evaluation thread; a panic gives `fallback`.
    fn on<R: Send + 'static>(
        &self,
        fallback: R,
        f: impl FnOnce(&State) -> R + Send + 'static,
    ) -> R {
        let state = self.state.clone();
        self.workers.run(move || f(&state)).unwrap_or(fallback)
    }

    fn on_result<R: Send + 'static>(
        &self,
        f: impl FnOnce(&State) -> Result<R, CoreError> + Send + 'static,
    ) -> Result<R, CoreError> {
        let state = self.state.clone();
        self.workers.run(move || f(&state))?
    }

    async fn long<R: Send + 'static>(
        &self,
        cancel: Arc<CancelToken>,
        f: impl FnOnce(&State) -> R + Send + 'static,
    ) -> Result<R, CoreError> {
        let state = self.state.clone();
        self.workers.spawn(cancel.flag(), move || f(&state)).await
    }
}

#[uniffi::export]
impl Engine {
    #[uniffi::constructor]
    pub fn new(config: EngineConfig) -> Arc<Self> {
        Arc::new(Engine {
            state: Arc::new(State::new(config)),
            workers: Workers::new(),
        })
    }

    // ---- the bar ----

    /// One keystroke: the live answer as the bar shows it, why it is blank, and the completion.
    pub fn live(&self, request: LiveRequest) -> LiveAnswer {
        let fallback = LiveAnswer::blank(&request.text);
        self.on(fallback, move |s| s.live(&request))
    }

    /// `live` on the long-work pool, cancellable.
    pub async fn live_async(
        &self,
        request: LiveRequest,
        cancel: Arc<CancelToken>,
    ) -> Result<LiveAnswer, CoreError> {
        self.long(cancel, move |s| s.live(&request)).await
    }

    /// Whether `text` continues from the last answer (`chainsFromAnswer`, never for `graph` or `isolate x`).
    pub fn chains(&self, text: String, context: EvalContext) -> bool {
        self.on(false, move |s| s.chains(&text, &context))
    }

    /// Enter: the history row it saves and the new history, or why nothing was saved.
    pub fn commit(&self, request: LiveRequest, history: Vec<HistoryRow>) -> CommitResult {
        let fallback = CommitResult {
            saved: false,
            row: None,
            history: history.clone(),
            input: request.text.clone(),
            hint: None,
            json: "null".into(),
        };
        self.on(fallback, move |s| s.commit(&request, &history))
    }

    // ---- sheets ----

    /// Every line of a sheet; each line sees what the lines above it defined.
    pub fn evaluate(&self, lines: Vec<String>, options: EvalOptions) -> Vec<LineAnswer> {
        self.on(Vec::new(), move |_| State::evaluate(&lines, &options))
    }

    /// `evaluate` on the long-work pool, cancellable (a pasted sheet, a long one).
    pub async fn evaluate_async(
        &self,
        lines: Vec<String>,
        options: EvalOptions,
        cancel: Arc<CancelToken>,
    ) -> Result<Vec<LineAnswer>, CoreError> {
        self.long(cancel, move |_| State::evaluate(&lines, &options))
            .await
    }

    /// `{"lines": [...], "options": {...}}` in the TS shapes in, `LineResult[]` JSON out.
    pub fn evaluate_json(&self, request: String) -> Result<String, CoreError> {
        self.on_result(move |_| crate::evaluate_json_text(&request))
    }

    // ---- pieces of the bar ----

    /// The other ways Tab shows the answer, in cycle order (`answerForms`).
    pub fn answer_forms(&self, source: FormSource) -> Vec<AnswerView> {
        self.on(Vec::new(), move |s| s.answer_forms(&source))
    }

    /// The rest of the word being typed (`completionFor`), or None.
    pub fn completion(&self, text: String, names: CompletionNames) -> Option<String> {
        self.on(None, move |s| s.completion(&text, &names))
    }

    /// Where a blank answer stops reading (`blankReason`), or None.
    pub fn blank_reason(
        &self,
        text: String,
        options: EvalOptions,
        names: CompletionNames,
    ) -> Option<Span> {
        self.on(None, move |s| {
            s.blank_reason_with(&text, &options, &names, false)
        })
    }

    /// Units, known words, radicals and bounds in the input, by start.
    pub fn spans(&self, text: String, names: CompletionNames) -> Vec<TextSpan> {
        self.on(Vec::new(), move |s| s.spans(&text, &names))
    }

    /// `buildGraph`.
    pub fn graph(
        &self,
        input: String,
        options: GraphOptions,
        settings: EvalOptions,
    ) -> Option<GraphResult> {
        self.on(None, move |s| s.graph(&input, &options, &settings))
    }

    /// `graph` on the long-work pool, cancellable.
    pub async fn graph_async(
        &self,
        input: String,
        options: GraphOptions,
        settings: EvalOptions,
        cancel: Arc<CancelToken>,
    ) -> Result<Option<GraphResult>, CoreError> {
        self.long(cancel, move |s| s.graph(&input, &options, &settings))
            .await
    }

    /// y at any x (`evaluateGraphY`), for the readout between samples.
    pub fn graph_y(
        &self,
        expression: String,
        x: f64,
        options: GraphOptions,
        settings: EvalOptions,
    ) -> Option<f64> {
        self.on(None, move |_| {
            call(
                "engine/graph",
                "evaluateGraphY",
                &[
                    json!(expression),
                    json!(x),
                    graph_options_json(&options, &settings),
                ],
            )
            .and_then(|v| v.as_f64())
        })
    }

    /// z at (x, y) for a contour plot's readout (`compileGraphZ(expression, options)(x, y)`).
    pub fn graph_z(
        &self,
        expression: String,
        x: f64,
        y: f64,
        options: GraphOptions,
        settings: EvalOptions,
    ) -> Option<f64> {
        self.on(None, move |_| {
            call(
                "engine/graph",
                "evaluateGraphZ",
                &[
                    json!(expression),
                    json!(x),
                    json!(y),
                    graph_options_json(&options, &settings),
                ],
            )
            .and_then(|v| v.as_f64())
        })
    }

    /// The Typst document for the preview (`typstDocument`).
    pub fn typst_source(&self, expr: String, answer: String, solved_for: String) -> Option<String> {
        self.on(None, move |_| {
            opt_string(call(
                "lib/typstMath",
                "typstDocument",
                &[json!(expr), json!(answer), json!(solved_for)],
            ))
        })
    }

    /// The closed-form search for `LiveAnswer::closed_form_job` (`identifyJob`); None when none is found.
    pub async fn closed_form(
        &self,
        job_json: String,
        cancel: Arc<CancelToken>,
    ) -> Result<Option<String>, CoreError> {
        let job: Json = serde_json::from_str(&job_json).map_err(CoreError::bad)?;
        self.long(cancel, move |_| {
            opt_string(call("engine/closedForm", "identifyJob", &[job]))
        })
        .await
    }

    // ---- currency rates ----

    /// The rates cache JSON (`{checked, fetched, date, rates}`) or a `{base, rates}` table.
    pub fn set_rates(&self, rates_json: String) -> Result<(), CoreError> {
        self.state.set_rates(&rates_json).map(|_| ())
    }

    /// Reads `<data_dir>/rates.json`; false when there is no file or no usable rate in it.
    pub fn reload_rates(&self) -> Result<bool, CoreError> {
        let Some(path) = self.state.data_file("rates.json") else {
            return Ok(false);
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => self.state.set_rates(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(CoreError::io(e)),
        }
    }

    pub fn rates_status(&self, now_ms: i64) -> RatesStatus {
        let rates = self.state.rates.read().ok();
        rates::status(rates.as_ref().and_then(|r| r.as_ref()), now_ms)
    }

    // ---- history ----

    /// `<data_dir>/history.json`, migrated and slimmed (`loadHistory`); empty when there is none.
    pub fn load_history(&self) -> Vec<HistoryRow> {
        self.on(Vec::new(), |s| s.load_history())
    }

    /// Writes what `persistable_history` keeps and returns it.
    pub fn save_history(&self, rows: Vec<HistoryRow>) -> Result<Vec<HistoryRow>, CoreError> {
        self.on_result(move |s| s.save_history(&rows))
    }

    /// `persistableHistory`: sticky definitions, slimmed rows, under the size cap.
    pub fn persistable_history(&self, rows: Vec<HistoryRow>) -> Vec<HistoryRow> {
        let fallback = rows.clone();
        self.on(fallback, move |_| {
            hist::persistable_history(&rows.iter().map(hist::row_json).collect::<Vec<_>>())
                .iter()
                .map(hist::row_from_json)
                .collect()
        })
    }

    /// TS `HistoryRow[]` JSON (legacy rows too, or the history file's envelope) to the slimmed rows' JSON.
    pub fn normalize_history(&self, rows_json: String) -> String {
        self.on("[]".into(), move |_| {
            Json::Array(State::normalized_rows(&rows_json)).to_string()
        })
    }

    /// `normalize_history`, typed.
    pub fn history_from_json(&self, rows_json: String) -> Vec<HistoryRow> {
        self.on(Vec::new(), move |_| {
            State::normalized_rows(&rows_json)
                .iter()
                .map(hist::row_from_json)
                .collect()
        })
    }

    /// Rows as TS `HistoryRow[]` JSON.
    pub fn history_to_json(&self, rows: Vec<HistoryRow>) -> String {
        Json::Array(rows.iter().map(hist::row_json).collect()).to_string()
    }

    /// What the history defines for the next line: variables, functions, measures, quantities, `ans`, and the
    /// answer a chain continues from.
    pub fn context_from_history(&self, rows: Vec<HistoryRow>, view: HistoryView) -> EvalContext {
        self.on(EvalContext::default(), move |s| {
            s.context_from_history(&rows, &view)
        })
    }

    // ---- reference ----

    /// The sheet a command opens (`identitySheetFor`): `identities`, `idt calculus`, `constants`…
    pub fn identity_sheet(&self, command: String) -> Option<IdentitySheet> {
        self.on(None, move |_| {
            call("lib/identities", "identitySheetFor", &[json!(command)])
                .and_then(|v| serde_json::from_value(v).ok())
        })
    }

    /// The periodic table (`ELEMENTS`).
    pub fn elements(&self) -> Vec<Element> {
        self.on(Vec::new(), |_| {
            call("lib/elements", "ELEMENTS", &[])
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default()
        })
    }

    /// The onboarding hint to show, if any (`pickHint`).
    pub fn hint(&self, facts: HintFacts) -> Option<Hint> {
        self.on(None, move |_| {
            let mut f = json!({
                "expr": facts.expr,
                "unit": facts.unit,
                "chained": facts.chained,
                "angleMode": if facts.angle_radians { "rad" } else { "deg" },
                "fractionMode": facts.fraction_mode,
                "native": facts.native,
                "opens": facts.opens,
            });
            if let Some(a) = &facts.answer {
                f["answer"] = json!(a);
            }
            if let Some(v) = &facts.variable {
                f["variable"] = json!(v);
            }
            let on = match facts.on {
                HintTrigger::Pause => "pause",
                HintTrigger::Commit => "commit",
                HintTrigger::Open => "open",
            };
            call(
                "lib/onboarding",
                "pickHint",
                &[json!(facts.seen), f, json!(on)],
            )
            .and_then(|v| serde_json::from_value(v).ok())
        })
    }

    // ---- escape hatch ----

    /// Any ported TS function by module and name (`lib/completion`, `completionFor`), its arguments a JSON array
    /// in the TS shapes; the answer as JSON.
    pub fn call_json(
        &self,
        module: String,
        function: String,
        args_json: String,
    ) -> Result<String, CoreError> {
        self.on_result(move |_| {
            let args: Json = serde_json::from_str(&args_json).map_err(CoreError::bad)?;
            let Json::Array(args) = args else {
                return Err(CoreError::bad("args_json must be a JSON array"));
            };
            bridge::call_checked(&module, &function, &args).map(|v| v.to_string())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Arc<Engine> {
        Engine::new(EngineConfig {
            is_windows: false,
            locale: "en_US".into(),
            data_dir: None,
        })
    }

    fn request(text: &str) -> LiveRequest {
        LiveRequest {
            text: text.into(),
            options: EvalOptions::default(),
            context: EvalContext::default(),
            chained: false,
            now_ms: 1_790_287_960_000,
            phrases: true,
        }
    }

    #[test]
    fn live_answers_and_blanks() {
        let e = engine();
        let a = e.live(request("1234567*2"));
        assert_eq!(a.display, "2469134");
        assert_eq!(a.pretty, "2,469,134");
        assert_eq!(a.number, Some(2_469_134.0));
        assert!(a.json.contains("\"display\":\"2469134\""));
        let s = e.live(request("√8"));
        assert_eq!(s.exact.as_deref(), Some("2sqrt(2)"));
        assert_eq!(s.dual_label, "2sqrt(2) ≈ 2.82843");
        assert_eq!(e.live(request("2+")).display, "");
        assert_eq!(e.live(request("")).display, "");
        // past the parser's limit: blank, and no stack overflow; 250 levels (the most it reads) overflow 512 KB
        let deep = format!("{}1{}", "(".repeat(600), ")".repeat(600));
        assert_eq!(e.live(request(&deep)).display, "");
        let ok = format!("{}1{}", "(".repeat(250), ")".repeat(250));
        assert_eq!(e.live(request(&ok)).display, "1");
    }

    #[test]
    fn chains_through_history() {
        let e = engine();
        let mut history = Vec::new();
        let first = e.commit(request("6*7"), history.clone());
        assert!(first.saved);
        history = first.history;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].display, "42");
        assert_eq!(history[0].at, Some(1_790_287_960_000));
        let view = HistoryView {
            show: HistoryShow::Always,
            answer_form: AnswerForm::Exact,
            sig_figs: 6,
            now_ms: 1_790_287_960_000,
        };
        let context = e.context_from_history(history.clone(), view);
        assert_eq!(context.ans, Some(42.0));
        assert!(e.chains("*2".into(), context.clone()));
        assert!(!e.chains("-2".into(), context.clone()));
        let mut req = request("*2");
        req.context = context;
        req.chained = true;
        let live = e.live(req.clone());
        assert_eq!((live.display.as_str(), live.expr.as_str()), ("84", "ans*2"));
        let second = e.commit(req, history.clone());
        assert_eq!(second.row.as_ref().map(|r| r.expr.as_str()), Some("42*2"));
        assert_eq!(second.history.len(), 2);
        // a blank line saves nothing
        let none = e.commit(request("2+"), second.history.clone());
        assert!(!none.saved);
        assert_eq!(none.history.len(), 2);
    }

    #[test]
    fn variables_and_functions_carry_over() {
        let e = engine();
        let mut history = Vec::new();
        for line in ["x = 5", "f(t) = t^2"] {
            history = e.commit(request(line), history).history;
        }
        let ctx = e.context_from_history(
            history.clone(),
            HistoryView {
                show: HistoryShow::Always,
                answer_form: AnswerForm::Exact,
                sig_figs: 6,
                now_ms: 0,
            },
        );
        assert_eq!(ctx.variables.get("x"), Some(&5.0));
        assert_eq!(ctx.functions.get("f").map(|f| f.body.as_str()), Some("t^2"));
        let mut req = request("f(x)+1");
        req.context = ctx;
        assert_eq!(e.live(req).display, "26");
        assert_eq!(history[1].kind, Some(HistoryKind::Function));
    }

    #[test]
    fn history_files_round_trip() {
        let dir = std::env::temp_dir().join(format!("qcalc-ffi-test-{}", hist::new_row_id()));
        let e = Engine::new(EngineConfig {
            is_windows: false,
            locale: "en_US".into(),
            data_dir: Some(dir.to_string_lossy().into_owned()),
        });
        assert!(e.load_history().is_empty());
        let rows = e.commit(request("2^10"), Vec::new()).history;
        let saved = e.save_history(rows.clone()).unwrap();
        assert_eq!(saved, rows);
        assert_eq!(e.load_history(), rows);
        let text = std::fs::read_to_string(dir.join("history.json")).unwrap();
        assert!(
            text.starts_with(r#"{"rows":["#) || text.contains(r#""version":1"#),
            "{text}"
        );
        assert!(!e.reload_rates().unwrap());
        std::fs::write(
            dir.join("rates.json"),
            r#"{"checked":1,"fetched":1790287960,"date":"2026-09-24","rates":{"USD":1.1}}"#,
        )
        .unwrap();
        assert!(e.reload_rates().unwrap());
        assert!(e.rates_status(1_790_287_960_000).fresh);
        assert!(!e.rates_status(1_790_287_960_000 + 8 * 86_400_000).fresh);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn escape_hatch_and_placeholders() {
        let e = engine();
        assert_eq!(
            e.call_json(
                "engine/format".into(),
                "formatNumber".into(),
                "[0.5, 6]".into()
            )
            .unwrap(),
            "\"0.5\""
        );
        assert!(matches!(
            e.call_json("lib/x".into(), "y".into(), "[]".into()),
            Err(CoreError::Unported { .. })
        ));
        assert!(matches!(
            e.call_json("lib/x".into(), "y".into(), "{}".into()),
            Err(CoreError::BadRequest { .. })
        ));
        assert!(e.set_rates("nope".into()).is_err());
        // placeholders answer blank until their packages land, and never fail
        let _ = e.completion("sq".into(), CompletionNames::default());
        let _ = e.spans("5 kg".into(), CompletionNames::default());
        let _ = e.elements();
    }
}
