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
use crate::app::messaging as msg;
use crate::app::onboarding::{self as ob, Example, HintFacts, HintOn, Onboarding, Rotation};
use crate::app::settings::{self as st, NativeInfo, Settings};
use crate::app::tour as tr;
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

/// A `Messaging` as the TS passes it (lists in their own order), or sanitized when it isn't one.
fn messaging(v: &Json) -> msg::Messaging {
    of(v).unwrap_or_else(|| msg::sanitize_messaging(v))
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
    // the bar's squiggle evaluates a chained line through `ans` (`chained: true`, from the FFI bridge)
    let chained = t.get("chained") == Some(&Json::Bool(true));
    Some(move |text: &str| {
        let text = if chained {
            chain::chained_expr(text)
        } else {
            text.to_string()
        };
        crate::evaluate_sheet(&[text], &options)
            .first()
            .is_some_and(|r| !r.display.is_empty())
    })
}

/// A tour bitmask (`done`), as JS numbers come.
fn bits(v: &Json) -> u32 {
    v.as_f64().map_or(0, |n| n as u32)
}

/// A recorded `evalNonBlank` tag as an evaluator that gives the display of a piece that reads, else None.
fn eval_line(tag: &Json) -> Option<impl Fn(&str) -> Option<String>> {
    let t = tag.get("$fn")?;
    if t.get("kind")?.as_str()? != "evalNonBlank" {
        return None;
    }
    let options: EvaluateOptions = of(&decode(t.get("options")?)).unwrap_or_default();
    let chained = t.get("chained") == Some(&Json::Bool(true));
    Some(move |text: &str| {
        let text = if chained {
            chain::chained_expr(text)
        } else {
            text.to_string()
        };
        crate::evaluate_sheet(&[text], &options)
            .into_iter()
            .next()
            .map(|r| r.display)
            .filter(|d| !d.is_empty())
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
        let names: Names = raw
            .get(2)
            .map(decode)
            .and_then(|n| of(&n))
            .unwrap_or_default();
        return Some(Outcome::Answer(opt(blank_reason(text, &ok, &names))));
    }
    if module == "lib/missingUnit" && name == "missingUnit" {
        // the bar's offer: a piece is fine when it reads and isn't an improper unit conversion
        let Some(reads) = raw.get(1).and_then(eval_line) else {
            return Some(Outcome::Unreplayable);
        };
        let text = raw.first().and_then(Json::as_str).unwrap_or("");
        let ok = |piece: &str| {
            reads(piece).is_some_and(|d| !crate::units::is_improper_unit_conversion(&d))
        };
        return Some(Outcome::Answer(opt(
            crate::app::missing_unit::missing_unit(text, &ok),
        )));
    }
    if module == "lib/analytics" {
        return Some(
            crate::app::analytics::golden_dispatch(name, raw)
                .map_or(Outcome::Unported, Outcome::Answer),
        );
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
            json!(ans::insertable_history_reuse(
                &a(0),
                form(&a(1)),
                insert,
                sig(3)
            ))
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
                plain: o
                    .get("plain")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string(),
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
        // ---- lib/messaging (the pure parts; the relay calls stay in the shells) ----
        ("lib/messaging", "sanitizeMessaging") => to(&msg::sanitize_messaging(&a(0))),
        ("lib/messaging", "withRecent") => to(&msg::with_recent(&messaging(&a(0)), &s(1))),
        ("lib/messaging", "messageContacts") => json!(msg::message_contacts(&messaging(&a(0)))),
        ("lib/messaging", "isMessageCommand") => json!(msg::is_message_command(&s(0))),
        ("lib/messaging", "parseMessage") => opt(msg::parse_message(&s(0))),
        ("lib/messaging", "messageNameSpan") => {
            opt(msg::message_name_span(&s(0))
                .map(|(start, end)| json!({"start": start, "end": end})))
        }
        ("lib/messaging", "messageHint") => {
            let line: msg::MessageLine = of(&a(0)).unwrap_or_default();
            json!(msg::message_hint(
                &line,
                &messaging(&a(1)),
                b(2),
                a(3).as_bool()
            ))
        }
        // native only (no TS counterpart): the conversation commands and the message threads
        ("lib/messaging", "parseMessagesCommand") => opt(msg::parse_messages_command(&s(0))),
        ("lib/messaging", "messagesHint") => match msg::parse_messages_command(&s(0)) {
            Some(cmd) => json!(msg::messages_hint(&cmd)),
            None => Json::Null,
        },
        ("lib/messaging", "threadContacts") => {
            let history: Vec<String> = of(&a(0)).unwrap_or_default();
            json!(msg::thread_contacts(&history, &messaging(&a(1))))
        }
        ("lib/messaging", "messagesCompletion") => {
            let contacts: Vec<String> = of(&a(1)).unwrap_or_default();
            json!(msg::messages_completion(&s(0), &contacts))
        }
        ("lib/messaging", "threadEmptyText") => json!(msg::thread_empty_text(&s(0))),
        ("lib/messaging", "cardAnnouncement") => json!(msg::card_announcement(&s(0), &s(1), &s(2))),
        // native only: clips and files
        ("lib/messaging", "parseShareCommand") => opt(msg::parse_share_command(&s(0))),
        ("lib/messaging", "shareHint") => match msg::parse_share_command(&s(0)) {
            Some(cmd) => json!(msg::share_hint(
                &cmd,
                &messaging(&a(1)),
                b(2),
                a(3).as_bool(),
                &s(4)
            )),
            None => Json::Null,
        },
        ("lib/messaging", "shareCompletion") => {
            let contacts: Vec<String> = of(&a(1)).unwrap_or_default();
            json!(msg::share_completion(&s(0), &contacts))
        }
        ("lib/messaging", "shareStatusText") => {
            json!(msg::share_status_text(&s(0), &s(1), &s(2), &s(3)))
        }
        ("lib/messaging", "fileStateText") => json!(msg::file_state_text(
            &s(0),
            &s(1),
            n(2).map(|p| p.max(0.0) as u32)
        )),
        ("lib/messaging", "fileErrorText") => json!(msg::file_error_text(&s(0), &s(1))),
        ("lib/messaging", "fileSizeText") => {
            json!(msg::file_size_text(n(0).unwrap_or(0.0).max(0.0) as u64))
        }
        ("lib/messaging", "encryptedLength") => {
            json!(msg::encrypted_len(n(0).unwrap_or(0.0).max(0.0) as u64))
        }
        ("lib/messaging", "cleanFilename") => json!(msg::clean_filename(&s(0))),
        ("lib/messaging", "droppedLabel") => {
            json!(msg::dropped_label(&s(0), a(1).as_u64().unwrap_or(1) as u32))
        }
        ("lib/messaging", "numberedName") => json!(msg::numbered_name(
            &s(0),
            n(1).unwrap_or(1.0).max(1.0) as u32
        )),
        ("lib/messaging", "cleanClip") => opt(msg::clean_clip(&a(0))),
        ("lib/messaging", "clipPreview") => json!(msg::clip_preview(&s(0))),
        // native only: end-to-end keys for clips and files
        ("lib/messaging", "cleanSealedOffer") => opt(msg::clean_sealed_offer(&a(0))),
        ("lib/messaging", "cleanSealedClip") => opt(msg::clean_sealed_clip(&a(0))),
        ("lib/messaging", "cleanPublicKey") => opt(msg::clean_public_key(&a(0))),
        ("lib/messaging", "safetyCode") => opt(msg::safety_code(&s(0), &s(1))),
        ("lib/messaging", "shareContext") => {
            opt(msg::share_context(&s(0), &s(1), &s(2), &s(3), &s(4), &s(5)))
        }
        ("lib/messaging", "sealFileFacts") => opt(msg::seal_file_facts(
            &s(0),
            &s(1),
            n(2).unwrap_or(-1.0).max(0.0) as u64,
            &s(3),
            &s(4),
        )),
        ("lib/messaging", "openFileFacts") => opt(msg::open_file_facts(
            &s(0),
            n(1).unwrap_or(0.0).max(0.0) as u64,
        )),
        ("lib/messaging", "pinState") => json!(msg::pin_state(&a(0), &s(1), &s(2))),
        ("lib/messaging", "privacyNotice") => json!(msg::privacy_notice()),
        ("lib/messaging", "privacySettingsText") => json!(msg::privacy_settings_text()),
        ("lib/messaging", "lockTooltip") => json!(msg::lock_tooltip(&s(0))),
        ("lib/messaging", "keyChangedText") => json!(msg::key_changed_text(&s(0))),
        ("lib/messaging", "keyChangedDetail") => json!(msg::key_changed_detail(&s(0))),
        ("lib/messaging", "safetyCodeText") => json!(msg::safety_code_text(&s(0), &s(1))),
        ("lib/messaging", "safetyCodeDetail") => json!(msg::safety_code_detail(&s(0))),
        ("lib/messaging", "dropTarget") => json!(msg::drop_target(
            &s(0),
            &messaging(&a(1)),
            b(2),
            a(3).as_bool(),
            n(4).unwrap_or(1.0).max(0.0) as u32
        )),
        ("lib/messaging", "autoAccept") => json!(msg::auto_accept(
            &messaging(&a(0)),
            &s(1),
            n(2).unwrap_or(0.0).max(0.0) as u64,
            n(3).unwrap_or(0.0).max(0.0) as u64
        )),
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
        ("lib/historyShow", "recentStart") => {
            json!(hs::recent_start(&rows(&a(0)), n(1).unwrap_or(0.0)))
        }
        ("lib/historyShow", "scopeStart") => json!(hs::scope_start(
            &rows(&a(0)),
            hs::normalize_history_show(&a(1)),
            n(2).unwrap_or(0.0)
        )),
        ("lib/historyShow", "nextRecentExpiry") => {
            opt(hs::next_recent_expiry(&rows(&a(0)), n(1).unwrap_or(0.0)))
        }
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
        ("lib/keybinds", "keybindsEqual") => {
            json!(kb::keybinds_equal(&keybinds(&a(0)), &keybinds(&a(1))))
        }
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
            to(&crate::app::known_words::known_word_spans(
                &s(0),
                &fns,
                b(2),
            ))
        }
        // ---- lib/onboarding ----
        ("lib/onboarding", "advanceRotation") => {
            to(&ob::advance_rotation(of(&a(0)).unwrap_or_default()))
        }
        ("lib/onboarding", "dismissRotation") => {
            to(&ob::dismiss_rotation(of(&a(0)).unwrap_or_default()))
        }
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
        ("lib/onboarding", "nativeCheats") => json!(ob::native_type_cheats()),
        ("lib/onboarding", "modeAnnouncement") => json!(ob::mode_announcement(&s(0), b(1))),
        ("lib/onboarding", "emptyOnboarding") => to(&ob::empty_onboarding()),
        ("lib/onboarding", "exampleAnswer") => {
            let line: Option<LineResult> = of(&a(0));
            json!(ob::example_answer(line.as_ref()))
        }
        ("lib/onboarding", "exampleList") => to(&ob::example_list(a(0).as_str())),
        ("lib/onboarding", "examplesActive") => json!(ob::examples_active(&onboarding(&a(0)))),
        ("lib/onboarding", "mergeOnboarding") => to(&ob::merge_onboarding(
            &onboarding(&a(0)),
            &onboarding(&a(1)),
        )),
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
        ("lib/settings", "settingsEqual") => {
            json!(st::settings_equal(&settings(&a(0)), &settings(&a(1))))
        }
        ("lib/settings", "mergeNativeInfo") => {
            let base: NativeInfo = of(&a(1)).unwrap_or_default();
            to(&st::merge_native_info(&a(0), &base))
        }
        ("lib/settings", "toggleAngleMode") => st::toggle_angle_mode(&settings(&a(0))).to_json(),
        ("lib/settings", "toggleFractionMode") => {
            st::toggle_fraction_mode(&settings(&a(0))).to_json()
        }
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
            to(&un::record_edit(
                &undo_of(&a(0)),
                next,
                of(&a(2)),
                n(3).unwrap_or(0.0),
            ))
        }
        ("lib/undo", "breakRun") => to(&un::break_run(&undo_of(&a(0)))),
        ("lib/undo", "undo") => opt(un::undo(&undo_of(&a(0)))),
        ("lib/undo", "redo") => opt(un::redo(&undo_of(&a(0)))),
        // ---- lib/tour ----
        ("lib/tour", "TOUR_ON") => json!(tr::TOUR_ON),
        ("lib/tour", "TOUR_OFF") => json!(tr::TOUR_OFF),
        ("lib/tour", "TOUR_OPENS") => json!(tr::TOUR_OPENS),
        ("lib/tour", "TOUR_ALL") => json!(tr::TOUR_ALL),
        ("lib/tour", "TOUR_DONE_HINT") => json!(tr::TOUR_DONE_HINT),
        ("lib/tour", "TUTORIAL_HINT") => json!(tr::TUTORIAL_HINT),
        ("lib/tour", "tourSteps") => to(&tr::tour_steps(&s(0))),
        ("lib/tour", "TOUR_EXTRAS") => to(&tr::tour_extras()),
        ("lib/tour", "tourBit") => json!(of::<tr::TourId>(&a(0)).map_or(0, tr::tour_bit)),
        ("lib/tour", "enrolTour") => to(&tr::enrol_tour(&onboarding(&a(0)), b(1))),
        ("lib/tour", "tourRunning") => json!(tr::tour_running(&onboarding(&a(0)))),
        ("lib/tour", "tourStep") => opt(tr::tour_step(bits(&a(0)), &s(1))),
        ("lib/tour", "tourProgress") => to(&tr::tour_progress(bits(&a(0)), &s(1))),
        ("lib/tour", "tourExtra") => opt(tr::tour_extra(bits(&a(0)), &s(1))),
        ("lib/tour", "tourUsed") => json!(tr::tour_used(&of(&a(0)).unwrap_or_default())),
        ("lib/tour", "tourLineFits") => match of::<tr::TourStep>(&a(0)) {
            Some(step) => json!(tr::tour_line_fits(&step, &s(1), b(2))),
            None => Json::Bool(false),
        },
        ("lib/tour", "isTutorialCommand") => json!(tr::is_tutorial_command(&s(0))),
        ("lib/tour", "isSkipCommand") => json!(tr::is_skip_command(&s(0))),
        // ---- constants the hosts read ----
        ("lib/onboarding", "EXAMPLE_MS") => json!(ob::EXAMPLE_MS),
        ("lib/onboarding", "HINT_PAUSE_MS") => json!(ob::HINT_PAUSE_MS),
        ("lib/undo", "UNDO_RUN_MS") => json!(un::UNDO_RUN_MS),
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
