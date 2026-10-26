//! WP-APP-LOGIC's arms of the golden replay (`crate::golden::dispatch` asks here first): the `src/lib` modules
//! ported in `app/`, and blankReason's `{"$fn": {"kind": "evalNonBlank", "options": …}}` callback rebuilt as
//! `text => Boolean(evaluateSheet([text], options)[0]?.display)`. A test's own closure (`{"$fn": "anon"}`) stays
//! unreplayable.

use serde_json::{Value as Json, json};

use crate::app::answer::{self as ans, AnswerForm, HistoryInsert, HistorySide};
use crate::app::blank_reason::{EnterFacts, Names, blank_reason, enter_hint};
use crate::app::chain::{self, ChainAnswer};
use crate::app::completion::{CompletionNames, completion_for};
use crate::app::history as h;
use crate::app::history_show as hs;
use crate::app::identities as idt;
use crate::app::keybinds::{self as kb, KeyAction, KeyEventLike, Keybinds};
use crate::app::onboarding::{self as ob, Example, HintFacts, HintOn, Onboarding, Rotation};
use crate::app::settings::{self as st, NativeInfo, Settings};
use crate::app::undo::{self as un, Undo, UndoState};
use crate::format::DEFAULT_SIG_FIGS;
use crate::golden::{Outcome, decode};
use crate::types::{EvaluateOptions, LineResult};

fn of<T: serde::de::DeserializeOwned>(v: &Json) -> Option<T> {
    serde_json::from_value(v.clone()).ok()
}

fn to<T: serde::Serialize>(v: &T) -> Json {
    serde_json::to_value(v).unwrap_or(Json::Null)
}

fn opt<T: serde::Serialize>(v: Option<T>) -> Json {
    v.map_or(Json::Null, |v| to(&v))
}

fn form(v: &Json) -> AnswerForm {
    if v.as_str() == Some("exact") {
        AnswerForm::Exact
    } else {
        AnswerForm::Approx
    }
}

fn rows(v: &Json) -> Vec<Json> {
    v.as_array().cloned().unwrap_or_default()
}

/// The evaluator a recorded `evalNonBlank` tag stands for.
fn eval_non_blank(tag: &Json) -> Option<impl Fn(&str) -> bool> {
    let t = tag.get("$fn")?;
    if t.get("kind")?.as_str()? != "evalNonBlank" {
        return None;
    }
    let options: EvaluateOptions = of(&decode(t.get("options")?)).unwrap_or_default();
    Some(move |text: &str| {
        crate::evaluate_sheet(&[text.to_string()], &options)
            .first()
            .is_some_and(|r| !r.display.is_empty())
    })
}

fn keybinds(v: &Json) -> Keybinds {
    kb::keybinds_from_json(v)
}

fn action(v: &Json) -> Option<KeyAction> {
    v.as_str().and_then(KeyAction::from_id)
}

/// The replay of one call, or None when `module` isn't one of this package's.
pub fn dispatch(module: &str, name: &str, raw: &[Json]) -> Option<Outcome> {
    if !module.starts_with("lib/") {
        return None;
    }
    if module == "lib/blankReason" && name == "blankReason" {
        let Some(ok) = raw.get(1).and_then(eval_non_blank) else {
            return Some(Outcome::Unreplayable);
        };
        let text = raw.first().and_then(Json::as_str).unwrap_or("");
        let names: Names = raw.get(2).map(decode).and_then(|n| of(&n)).unwrap_or_default();
        return Some(Outcome::Answer(opt(blank_reason(text, &ok, &names))));
    }
    let args: Vec<Json> = raw.iter().map(decode).collect();
    let a = |i: usize| args.get(i).cloned().unwrap_or(Json::Null);
    let s = |i: usize| a(i).as_str().unwrap_or("").to_string();
    let n = |i: usize| a(i).as_f64();
    let b = |i: usize| a(i).as_bool().unwrap_or(false);
    let sig = |i: usize| n(i).unwrap_or(f64::from(DEFAULT_SIG_FIGS));
    let answer = match (module, name) {
        // ---- lib/answer ----
        ("lib/answer", "answerAmong") => json!(ans::answer_among(&s(0), b(1))),
        ("lib/answer", "hasDualAnswer") => json!(ans::row_has_dual_answer(&a(0))),
        ("lib/answer", "historyPickOn") => {
            let side: HistorySide = of(&a(0)).unwrap_or_default();
            let start = a(1).get("start").and_then(Json::as_f64).unwrap_or(0.0) as usize;
            json!(ans::history_pick_on(side, start))
        }
        ("lib/answer", "historyPicks") => to(&ans::history_picks(&a(0), form(&a(1)), sig(2))),
        ("lib/answer", "historySideOf") => to(&ans::history_side_of(n(0).unwrap_or(0.0) as usize)),
        ("lib/answer", "insertableAnswer") => json!(ans::insertable_answer(&s(0), n(1), sig(2))),
        ("lib/answer", "insertableHistoryAnswer") => {
            json!(ans::insertable_history_answer(&a(0), form(&a(1)), sig(2)))
        }
        ("lib/answer", "insertableHistoryReuse") => {
            let insert = if a(2).as_str() == Some("expr") {
                HistoryInsert::Expr
            } else {
                HistoryInsert::Answer
            };
            json!(ans::insertable_history_reuse(&a(0), form(&a(1)), insert, sig(3)))
        }
        ("lib/answer", "normalizeHistoryInsert") => to(&ans::normalize_history_insert(&a(0))),
        ("lib/answer", "prettyRoots") => json!(ans::pretty_roots(&s(0))),
        ("lib/answer", "solveRoots") => opt(ans::solve_roots(&a(0), sig(1))),
        ("lib/answer", "solveInsert") => opt(ans::solve_insert(&a(0), sig(1))),
        ("lib/answer", "unitlessAnswer") => json!(ans::unitless_answer(&s(0))),
        ("lib/answer", "visibleAnswer") => {
            let row = a(0);
            json!(ans::visible_answer(
                row.get("display").and_then(Json::as_str).unwrap_or(""),
                row.get("exact").and_then(Json::as_str),
                form(&a(1))
            ))
        }
        // ---- lib/answerForms ----
        // a non-finite value arrives as null (recorder encoding): no forms either way
        ("lib/answerForms", "answerForms") if !a(0)["value"]["n"].is_number() => json!([]),
        ("lib/answerForms", "answerForms") => match of(&a(0)) {
            Some(src) => to(&crate::app::answer_forms::answer_forms(&src)),
            None => return Some(Outcome::Threw("bad form source".into())),
        },
        // ---- lib/blankReason ----
        ("lib/blankReason", "enterHint") => match of::<EnterFacts>(&a(0)) {
            Some(f) => json!(enter_hint(&f)),
            None => return Some(Outcome::Threw("bad facts".into())),
        },
        // ---- lib/chain ----
        ("lib/chain", "chainsFromAnswer") => {
            let last: Option<ChainAnswer> = a(1).as_object().map(|o| ChainAnswer {
                plain: o.get("plain").and_then(Json::as_str).unwrap_or("").to_string(),
                unit: o.get("unit").and_then(Json::as_bool).unwrap_or(false),
            });
            json!(chain::chains_from_answer(&s(0), last.as_ref()))
        }
        ("lib/chain", "chainedExpr") => json!(chain::chained_expr(&s(0))),
        ("lib/chain", "chainedHistoryExpr") => json!(chain::chained_history_expr(&s(0), &s(1))),
        ("lib/chain", "ansWrittenOut") => json!(chain::ans_written_out(&s(0), &s(1))),
        // ---- lib/completion ----
        ("lib/completion", "completionFor") => {
            let names: CompletionNames = of(&a(1)).unwrap_or_default();
            json!(completion_for(&s(0), &names))
        }
        // ---- lib/history ----
        ("lib/history", "ellipsize") => json!(h::ellipsize(&s(0), n(1).unwrap_or(0.0) as i64)),
        ("lib/history", "isMatrixQuantity") => json!(h::is_matrix_quantity(a(0).as_str())),
        ("lib/history", "isComplexQuantity") => json!(h::is_complex_quantity(a(0).as_str())),
        ("lib/history", "isLiteralQuantity") => json!(h::is_literal_quantity(a(0).as_str())),
        ("lib/history", "slimHistoryRow") => h::slim_history_row(&a(0)).unwrap_or(Json::Null),
        ("lib/history", "normalizeHistoryRow") => match h::normalize_history_row(&a(0), &s(1)) {
            Ok(r) => r.unwrap_or(Json::Null),
            Err(h::Throws) => return Some(Outcome::Threw("TypeError".into())),
        },
        ("lib/history", "persistableHistory") => Json::Array(h::persistable_history(&rows(&a(0)))),
        ("lib/history", "historyVariables") => Json::Object(
            h::history_variables(&rows(&a(0)))
                .into_iter()
                .map(|(k, v)| (k, json!(v)))
                .collect(),
        ),
        ("lib/history", "historyMeasures") => Json::Object(h::history_measures(&rows(&a(0)))),
        ("lib/history", "historyQuantities") => Json::Object(h::history_quantities(&rows(&a(0)))),
        ("lib/history", "historyFunctions") => Json::Object(h::history_functions(&rows(&a(0)))),
        ("lib/history", "lastHistoryNumber") => opt(h::last_history_number(&rows(&a(0)))),
        // ---- lib/historyShow ----
        ("lib/historyShow", "normalizeHistoryShow") => to(&hs::normalize_history_show(&a(0))),
        ("lib/historyShow", "recentStart") => json!(hs::recent_start(&rows(&a(0)), n(1).unwrap_or(0.0))),
        ("lib/historyShow", "scopeStart") => json!(hs::scope_start(
            &rows(&a(0)),
            hs::normalize_history_show(&a(1)),
            n(2).unwrap_or(0.0)
        )),
        ("lib/historyShow", "nextRecentExpiry") => opt(hs::next_recent_expiry(&rows(&a(0)), n(1).unwrap_or(0.0))),
        // ---- lib/identities ----
        ("lib/identities", "identityLabel") => {
            let lead: Vec<String> = of(&a(1)).unwrap_or_default();
            json!(idt::identity_label(&s(0), &lead))
        }
        ("lib/identities", "identityRuns") => to(&idt::identity_runs(&s(0))),
        ("lib/identities", "identitySheetFor") => opt(idt::identity_sheet_for(&s(0))),
        ("lib/identities", "identityTargetFor") => opt(idt::identity_target_for(&s(0))),
        ("lib/identities", "isIdentityCommand") => json!(idt::is_identity_command(&s(0))),
        // the recorded tests ran with a host stub, which took the payload: true. Here the payload is built
        // (`native_identity_sheet`) and the Mac app is the host
        ("lib/identities", "openNativeIdentitySheet") => match of::<idt::IdentityTarget>(&a(0)) {
            Some(t) => json!(!idt::native_identity_sheet(&t).id.is_empty()),
            None => return Some(Outcome::Threw("bad target".into())),
        },
        // ---- lib/keybinds ----
        ("lib/keybinds", "actionForEvent") => {
            let e: KeyEventLike = of(&a(0)).unwrap_or_default();
            opt(kb::action_for_event(&e, &keybinds(&a(1)), b(2)).map(KeyAction::id))
        }
        ("lib/keybinds", "chordFromEvent") => {
            let e: KeyEventLike = of(&a(0)).unwrap_or_default();
            opt(kb::chord_from_event(&e))
        }
        ("lib/keybinds", "chordLabel") => json!(kb::chord_label(&s(0), b(1))),
        ("lib/keybinds", "chordProblem") => match action(&a(1)) {
            Some(act) => opt(kb::chord_problem(&s(0), act, &keybinds(&a(2)), b(3))),
            None => return Some(Outcome::Unported),
        },
        ("lib/keybinds", "defaultKeybind") => match action(&a(0)) {
            Some(act) => json!(kb::default_keybind(act, b(1))),
            None => return Some(Outcome::Unported),
        },
        ("lib/keybinds", "heldLabel") => json!(kb::held_label(&s(0), b(1))),
        ("lib/keybinds", "keybindFor") => match action(&a(1)) {
            Some(act) => json!(kb::keybind_for(&keybinds(&a(0)), act, b(2))),
            None => return Some(Outcome::Unported),
        },
        ("lib/keybinds", "keybindsEqual") => json!(kb::keybinds_equal(&keybinds(&a(0)), &keybinds(&a(1)))),
        ("lib/keybinds", "missedKeyText") => json!(kb::missed_key_text(&s(0))),
        ("lib/keybinds", "sanitizeKeybinds") => kb::keybinds_json(&kb::sanitize_keybinds(&a(0))),
        ("lib/keybinds", "setKeybind") => match action(&a(1)) {
            Some(act) => kb::keybinds_json(&kb::set_keybind(&keybinds(&a(0)), act, &s(2), b(3))),
            None => return Some(Outcome::Unported),
        },
        ("lib/keybinds", "windowsKeeps") => opt(kb::windows_keeps(&s(0))),
        // ---- lib/knownWords ----
        ("lib/knownWords", "knownWordSpans") => {
            let fns: Vec<String> = of(&a(1)).unwrap_or_default();
            to(&crate::app::known_words::known_word_spans(&s(0), &fns, b(2)))
        }
        // ---- lib/onboarding ----
        ("lib/onboarding", "advanceRotation") => to(&ob::advance_rotation(of(&a(0)).unwrap_or_default())),
        ("lib/onboarding", "dismissRotation") => to(&ob::dismiss_rotation(of(&a(0)).unwrap_or_default())),
        ("lib/onboarding", "startRotation") => to(&ob::start_rotation(b(0))),
        ("lib/onboarding", "rotationItem") => {
            let r: Rotation = of(&a(0)).unwrap_or_default();
            let items: Vec<Example> = of(&a(1)).unwrap_or_default();
            opt(ob::rotation_item(r, &items))
        }
        ("lib/onboarding", "afterHelpInput") => json!(ob::after_help_input(&s(0), &s(1))),
        ("lib/onboarding", "isHelpCommand") => json!(ob::is_help_command(&s(0))),
        ("lib/onboarding", "cheatSheet") => {
            let labels: Option<std::collections::BTreeMap<KeyAction, String>> = of(&a(2));
            let [keys, typed] = ob::cheat_sheet(a(0).as_str(), b(1), labels.as_ref());
            json!([keys, typed])
        }
        ("lib/onboarding", "emptyOnboarding") => to(&ob::empty_onboarding()),
        ("lib/onboarding", "exampleAnswer") => {
            let line: Option<LineResult> = of(&a(0));
            json!(ob::example_answer(line.as_ref()))
        }
        ("lib/onboarding", "exampleList") => to(&ob::example_list(a(0).as_str())),
        ("lib/onboarding", "examplesActive") => json!(ob::examples_active(&onboarding(&a(0)))),
        ("lib/onboarding", "mergeOnboarding") => to(&ob::merge_onboarding(&onboarding(&a(0)), &onboarding(&a(1)))),
        ("lib/onboarding", "pickHint") => {
            let facts: HintFacts = of(&a(1)).unwrap_or_default();
            let on: HintOn = of(&a(2)).unwrap_or_default();
            opt(ob::pick_hint(n(0).unwrap_or(0.0) as u32, &facts, on))
        }
        ("lib/onboarding", "recordCommit") => to(&ob::record_commit(&onboarding(&a(0)))),
        ("lib/onboarding", "recordOpen") => to(&ob::record_open(&onboarding(&a(0)))),
        ("lib/onboarding", "sanitizeOnboarding") => to(&ob::sanitize_onboarding(&a(0))),
        // ---- lib/paste ----
        ("lib/paste", "cleanPastedText") => json!(crate::app::paste::clean_pasted_text(&s(0))),
        // ---- lib/settings ----
        ("lib/settings", "defaultSettings") => st::default_settings().to_json(),
        ("lib/settings", "mergeSettings") => st::merge_settings(&a(0), &settings(&a(1))).to_json(),
        ("lib/settings", "settingsEqual") => json!(st::settings_equal(&settings(&a(0)), &settings(&a(1)))),
        ("lib/settings", "mergeNativeInfo") => {
            let base: NativeInfo = of(&a(1)).unwrap_or_default();
            to(&st::merge_native_info(&a(0), &base))
        }
        ("lib/settings", "toggleAngleMode") => st::toggle_angle_mode(&settings(&a(0))).to_json(),
        ("lib/settings", "toggleFractionMode") => st::toggle_fraction_mode(&settings(&a(0))).to_json(),
        ("lib/settings", "toggleSigFigMode") => st::toggle_sig_fig_mode(&settings(&a(0))).to_json(),
        // ---- lib/steady ----
        ("lib/steady", "isEditOf") => json!(crate::app::steady::is_edit_of(&s(0), &s(1))),
        // ---- lib/touches ----
        ("lib/touches", "lineCopyText") => {
            let line = a(1);
            json!(crate::app::touches::line_copy_text(
                &s(0),
                line.get("display").and_then(Json::as_str).unwrap_or(""),
                line.get("exact").and_then(Json::as_str),
                line.get("solve").map(|v| (
                    v.get("variable").and_then(Json::as_str).unwrap_or(""),
                    v.get("outcome").and_then(Json::as_str).unwrap_or("")
                )),
                form(&a(2))
            ))
        }
        // ---- lib/undo ----
        ("lib/undo", "undoStart") => to(&un::undo_start(&s(0))),
        ("lib/undo", "editKind") => opt(un::edit_kind(a(0).as_str())),
        ("lib/undo", "recordEdit") => {
            let next: UndoState = of(&a(1)).unwrap_or_default();
            to(&un::record_edit(&undo_of(&a(0)), next, of(&a(2)), n(3).unwrap_or(0.0)))
        }
        ("lib/undo", "breakRun") => to(&un::break_run(&undo_of(&a(0)))),
        ("lib/undo", "undo") => opt(un::undo(&undo_of(&a(0)))),
        ("lib/undo", "redo") => opt(un::redo(&undo_of(&a(0)))),
        _ => return None,
    };
    Some(Outcome::Answer(answer))
}

fn onboarding(v: &Json) -> Onboarding {
    of(v).unwrap_or_default()
}

fn settings(v: &Json) -> Settings {
    of(v).unwrap_or_default()
}

fn undo_of(v: &Json) -> Undo {
    of(v).unwrap_or_default()
}
