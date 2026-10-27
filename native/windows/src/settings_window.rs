//! The settings window (Ctrl+, in the bar, or the tray's Settings…): every setting the Mac's SettingsWindow.swift
//! and the Tauri app's settings page (src/components/SettingsPage.tsx) have, in six pages: General, Answers,
//! Units, Keybinds, Messages and Updates.
//!
//! **Drawn, not common controls.** A normal titled window drawn with Direct2D / DirectWrite through render.rs, in
//! the bar's colours, so it is right in light and dark (common controls have no dark mode for checkboxes, combo
//! boxes or their text) and needs no extra DLLs. The only native piece is a choice list, a popup menu
//! (`TrackPopupMenu`), made dark with the system's own menu theme (`dark_menus`).
//!
//! **Costs nothing until opened.** No class, window, render target, text format or layout exists before the first
//! `open`; everything lives in one `Win` (a thread-local `Option<Box<Win>>`) that is dropped when the window is
//! destroyed on close, giving back the render target, the formats, the layouts and the icons.
//!
//! **Live, both ways.** A change writes the store (`settings.json`, the Tauri shape, so both apps read it), hands
//! the settings to the bar's model (`BarModel::set_settings`, the live answer re-evaluated), and saves. A change
//! made in the bar (Ctrl+D...) or by the update loop comes back through `refresh`.
//!
//! **Keybinds.** A row per action (the core's `KEY_ACTIONS`); clicking a key records the next chord pressed.
//! Recording the show / hide shortcut pauses the global hotkey (as src-tauri/src/recorder.rs does) so Alt+Space and
//! Win chords reach the window; Alt+Space's window menu is held back while recording. A chord is checked by the
//! core's `chord_problem` (kept by Windows, by the text box, by another action); a show / hide chord is then tried
//! (`probe`: registered and let go), and a taken one says who probably holds it (hotkey.rs's guess) and offers a
//! free one. When only modifiers come through, another app took the key (`missed_key_text`).
//!
//! **Messages.** The page's rows come from the messaging feature through `set_messages_hook` (rows built from the
//! app, and a callback for a change); until it registers, the page says so.

use std::cell::{Cell, RefCell};

use qcalc_core::app::host_settings::{
    HostInfo, HotkeyVerdict, UpdateState, draft_choices, hotkey_note, spot_choices, update_line,
};
use qcalc_core::app::keybinds::{
    self as kb, KEY_ACTIONS, KeyAction, chord_label, chord_problem, default_keybind, keybind_for,
    missed_key_text,
};
use serde_json::{Value, json};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_WEIGHT_REGULAR, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_LEADING, IDWriteTextFormat, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows::Win32::Graphics::Gdi::{ClientToScreen, InvalidateRect, ValidateRect};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, HOT_KEY_MODIFIERS, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey, VK_CONTROL,
    VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CREATESTRUCTW, CW_USEDEFAULT, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
    DestroyIcon, DestroyMenu, DestroyWindow, GetClientRect, HICON, ICON_BIG, ICON_SMALL, IDC_ARROW,
    IDC_IBEAM, IsIconic, LoadCursorW, MF_CHECKED, MF_STRING, MINMAXINFO, RegisterClassExW,
    SW_RESTORE, SW_SHOW, SWP_NOACTIVATE, SWP_NOZORDER, SendMessageW, SetCursor,
    SetForegroundWindow, SetWindowPos, ShowWindow, TPM_RETURNCMD, TPM_TOPALIGN, TrackPopupMenu,
    WA_INACTIVE, WM_ACTIVATE, WM_CHAR, WM_CLOSE, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND,
    WM_GETMINMAXINFO, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT,
    WM_SETCURSOR, WM_SETICON, WM_SIZE, WM_SYSCHAR, WM_SYSCOMMAND, WM_SYSKEYDOWN, WM_SYSKEYUP,
    WNDCLASSEXW, WS_EX_APPWINDOW, WS_OVERLAPPEDWINDOW,
};
use windows::core::{BOOL, HSTRING, PCWSTR, w};

use crate::app::{App, Op, with_app};
use crate::input::Field;
use crate::render::{self, Canvas, Gfx, Rect, Surface, Theme, measure, with_alpha};
use crate::{analytics, autostart, hotkey, qlog, store, tray, update};

const CLASS: PCWSTR = w!("QCalcSettings");
/// The window's size in DIPs, and its smallest.
const SIZE: (f32, f32) = (760.0, 620.0);
const MIN: (f32, f32) = (620.0, 420.0);
const NAV_W: f32 = 196.0;
const PAD: f32 = 28.0;
const ROW_PAD: f32 = 14.0;
const GAP: f32 = 4.0;
/// the id RegisterHotKey uses while trying a chord (never the app's own HOTKEY_ID)
const PROBE_ID: i32 = 0x5150;

// ---- what a page holds ----

/// The pages, in the sidebar's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    General,
    Answers,
    Units,
    Keybinds,
    Messages,
    Updates,
}

impl Page {
    pub const ALL: [Page; 6] = [
        Page::General,
        Page::Answers,
        Page::Units,
        Page::Keybinds,
        Page::Messages,
        Page::Updates,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Page::General => "General",
            Page::Answers => "Answers",
            Page::Units => "Units",
            Page::Keybinds => "Keybinds",
            Page::Messages => "Messages",
            Page::Updates => "Software update",
        }
    }

    fn nav(self) -> &'static str {
        match self {
            Page::Updates => "Updates",
            p => p.title(),
        }
    }
}

/// A row's control. Choices are (id, label); a change reports the picked id.
#[derive(Clone, Debug, PartialEq)]
pub enum Control {
    None,
    Toggle(bool),
    /// a few choices side by side
    Seg {
        options: Vec<(String, String)>,
        value: String,
    },
    /// a list that opens
    Select {
        options: Vec<(String, String)>,
        value: String,
    },
    Button {
        label: String,
        enabled: bool,
    },
    /// a shortcut: its label, whether it is recording, whether it differs from the default
    Key {
        label: String,
        recording: bool,
        changed: bool,
    },
    /// one line of text, saved on Enter or when it loses the focus
    Text {
        value: String,
        placeholder: String,
    },
}

/// One thing on a page.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Heading(String),
    /// a paragraph (muted), or a warning (accent) when the flag is set
    Note(String, bool),
    Row {
        id: String,
        title: String,
        hint: String,
        control: Control,
    },
}

pub fn row(id: &str, title: &str, hint: &str, control: Control) -> Item {
    Item::Row {
        id: id.into(),
        title: title.into(),
        hint: hint.into(),
        control,
    }
}

fn options(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(i, l)| ((*i).to_string(), (*l).to_string()))
        .collect()
}

/// What the messaging feature supplies for the Messages page: its rows, and what to do with a change (the row's
/// id and the new value: a bool, a choice's id, text, or null for a button).
#[derive(Clone, Copy)]
pub struct MessagesHook {
    pub items: fn(&App) -> Vec<Item>,
    pub changed: fn(&mut App, &str, &Value),
}

thread_local! {
    static WIN: RefCell<Option<Box<Win>>> = const { RefCell::new(None) };
    static MESSAGES: Cell<Option<MessagesHook>> = const { Cell::new(None) };
    static WANT: Cell<Option<Page>> = const { Cell::new(None) };
    static CLASS_DONE: Cell<bool> = const { Cell::new(false) };
}

/// The messaging feature's rows (call once, from its own start).
pub fn set_messages_hook(hook: MessagesHook) {
    MESSAGES.with(|m| m.set(Some(hook)));
}

// ---- the rows, from the app ----

fn key_chord_title(action: &str, s: &store::Settings) -> String {
    KeyAction::from_id(action).map_or(String::new(), |a| {
        chord_label(&keybind_for(&binds(s), a, true), true)
    })
}

fn with_key(hint: &str, key: &str) -> String {
    if key.is_empty() {
        format!("{hint}.")
    } else {
        format!("{hint} · {key}")
    }
}

/// The store's keybinds as the core's.
fn binds(s: &store::Settings) -> kb::Keybinds {
    kb::sanitize_keybinds(&serde_json::to_value(&s.keybinds).unwrap_or(Value::Null))
}

fn installed() -> bool {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    autostart::is_installed(&exe, &autostart::Roots::from_env())
}

/// The show / hide chord's facts, in the shape the core's lines take.
fn host_info(app: &App, refused: &str) -> HostInfo {
    let hk = &app.hotkey;
    let failed = hk.gave_up;
    let refused_chord = hotkey::normalize(refused).unwrap_or_default();
    HostInfo {
        hotkey_chord: hk.active.clone().unwrap_or_else(|| hk.wanted.clone()),
        hotkey_failed: failed,
        hotkey_refused: if refused.is_empty() {
            String::new()
        } else {
            chord_label(&refused_chord, true)
        },
        hotkey_taken: if !refused_chord.is_empty() {
            refused_chord
        } else if failed {
            hk.wanted.clone()
        } else {
            String::new()
        },
        autostart: autostart::enabled(),
        spot_seconds: app.store.spot_seconds as f64,
        version: update::VERSION.into(),
        update: update::status(),
        auto_update: app.store.auto_update,
    }
}

fn now_s() -> f64 {
    crate::model::now_ms() as f64 / 1000.0
}

fn items(app: &App, win: &Win) -> Vec<Item> {
    let s = &app.store.settings;
    let mut out = Vec::new();
    match win.page {
        Page::General => {
            let login_hint = if installed() {
                "Start Q Calc in the tray when you sign in. Keeps the shortcut working after a restart."
            } else {
                "Start Q Calc in the tray when you sign in. Only an installed copy can."
            };
            out.push(row(
                "autostart",
                "Open at login",
                login_hint,
                Control::Toggle(autostart::enabled()),
            ));
            out.push(row(
                "theme",
                "Appearance",
                "Light and dark apply to Q Calc only. System matches Windows.",
                Control::Seg {
                    options: options(&[("system", "System"), ("light", "Light"), ("dark", "Dark")]),
                    value: s.theme.clone(),
                },
            ));
            out.push(row(
                "historyShow",
                "Show history",
                "Recent means the last 5 minutes.",
                Control::Select {
                    options: options(&[
                        ("recent", "Recent calculations"),
                        ("always", "Always"),
                        ("arrow", "Only on ↑"),
                    ]),
                    value: s.history_show.clone(),
                },
            ));
            out.push(row(
                "historyInsert",
                "Enter on a history row inserts",
                "Clicking an expression or an answer always inserts that side.",
                Control::Seg {
                    options: options(&[("expr", "Expression"), ("answer", "Answer")]),
                    value: s.history_insert.clone(),
                },
            ));
            let draft = draft_choices(s.draft_seconds.max(0) as u32);
            out.push(row(
                "draftSeconds",
                "Keep unfinished input",
                "How long a half-typed calculation waits for you after Q Calc hides.",
                Control::Select {
                    options: draft.iter().map(|(i, l)| (i.to_string(), l.clone())).collect(),
                    value: s.draft_seconds.to_string(),
                },
            ));
            let spot = spot_choices(app.store.spot_seconds.max(0) as u32);
            out.push(row(
                "spotSeconds",
                "Move the bar back after",
                "Once Q Calc has gone this long unopened, a bar you moved or widened opens in its usual spot and width.",
                Control::Select {
                    options: spot.iter().map(|(i, l)| (i.to_string(), l.clone())).collect(),
                    value: app.store.spot_seconds.to_string(),
                },
            ));
            out.push(row(
                "shareUsage",
                "Share anonymous usage",
                "Counts of which features get used, never what you type.",
                Control::Toggle(s.share_usage),
            ));
            out.push(row(
                "tutorial",
                "Tutorial",
                "Walk through the basics in the bar again, or type tutorial there.",
                Control::Button {
                    label: "Replay".into(),
                    enabled: true,
                },
            ));
        }
        Page::Answers => {
            let angle = key_chord_title("angle", s);
            out.push(row(
                "angleMode",
                "Angles",
                &if angle.is_empty() {
                    "The unit trig functions read.".to_string()
                } else {
                    format!("Switch with {angle}")
                },
                Control::Seg {
                    options: options(&[("deg", "Degrees"), ("rad", "Radians")]),
                    value: s.angle_mode.clone(),
                },
            ));
            out.push(row(
                "answerForm",
                "Answer form",
                "The form answers show in.",
                Control::Seg {
                    options: options(&[("exact", "Exact"), ("approx", "Approximate")]),
                    value: s.answer_form.clone(),
                },
            ));
            let flags: [(&str, &str, String, bool); 8] = [
                (
                    "fractionMode",
                    "Fractions",
                    with_key("Show answers as fractions", &key_chord_title("fraction", s)),
                    s.fraction_mode,
                ),
                (
                    "rationalize",
                    "Rationalize denominators",
                    "5/√41 becomes 5√41/41.".into(),
                    s.rationalize,
                ),
                (
                    "keepWords",
                    "Keep typed words as text",
                    "sqrt stays sqrt, not √.".into(),
                    s.keep_words,
                ),
                (
                    "spaceThousands",
                    "Spaces group thousands",
                    "1 000 000 is one million; off, a space between numbers multiplies (3 4 is 12).".into(),
                    s.space_thousands,
                ),
                (
                    "typstPreview",
                    "Typst preview",
                    "Show math like fractions, powers and integrals typeset under the bar.".into(),
                    s.typst_preview,
                ),
                (
                    "typstCopy",
                    "Copy Typst compatible",
                    "Equations copied from the bar paste as Typst math.".into(),
                    s.typst_copy,
                ),
                (
                    "copyUnitless",
                    "Copy answers without units",
                    "A copied answer is just the number: 15 m² copies as 15.".into(),
                    s.copy_unitless,
                ),
                ("", "", String::new(), false),
            ];
            for (id, title, hint, on) in flags.iter().filter(|f| !f.0.is_empty()) {
                out.push(row(id, title, hint, Control::Toggle(*on)));
            }
            out.push(Item::Heading("Significant figures".into()));
            out.push(row(
                "sigFigs",
                "Digits shown",
                "Answers round to this many significant figures.",
                Control::Select {
                    options: (2..=16).map(|n| (n.to_string(), n.to_string())).collect(),
                    value: s.sig_figs.to_string(),
                },
            ));
            out.push(row(
                "sigFigMode",
                "Propagate from input",
                &with_key("Match the precision you typed", &key_chord_title("sigFigs", s)),
                Control::Toggle(s.sig_fig_mode),
            ));
        }
        Page::Units => {
            out.push(row(
                "unitsReset",
                "Default units",
                "Quantities typed without “to …” convert into these units. Automatic keeps the built-in SI ↔ US pair.",
                Control::Button {
                    label: "Reset".into(),
                    enabled: !s.default_units.is_empty(),
                },
            ));
            for group in qcalc_ffi::unit_setting_groups() {
                out.push(Item::Heading(group.title.clone()));
                for item in group.items {
                    let mut opts = vec![(String::new(), "Automatic".to_string())];
                    opts.extend(item.units.iter().map(|u| (u.id.clone(), u.label.clone())));
                    out.push(row(
                        &format!("unit:{}", item.dim),
                        &item.label,
                        "",
                        Control::Select {
                            options: opts,
                            value: s.default_units.get(&item.dim).cloned().unwrap_or_default(),
                        },
                    ));
                }
            }
        }
        Page::Keybinds => {
            let hint = if !win.note.is_empty() {
                Item::Note(win.note.clone(), true)
            } else if win.recording.is_some() {
                Item::Note(
                    "Press the new keys. Esc cancels, Backspace leaves it without a key.".into(),
                    false,
                )
            } else {
                Item::Note("Click a shortcut, then press the keys you want.".into(), false)
            };
            out.push(hint);
            let core_binds = binds(s);
            for (action, label) in KEY_ACTIONS {
                let id = action.id();
                let chord = if action == KeyAction::Show {
                    app.hotkey.wanted.clone()
                } else {
                    keybind_for(&core_binds, action, true)
                };
                let recording = win.recording.as_ref().filter(|r| r.action == action);
                let text = match recording {
                    Some(r) if !r.held.is_empty() => kb::held_label(&r.held, true),
                    Some(_) => "Press keys…".into(),
                    None if chord.is_empty() => "None".into(),
                    None => chord_label(&chord, true),
                };
                let hint = if action == KeyAction::Show {
                    let note = hotkey_note(&host_info(app, &win.refused), win.verdict.as_ref());
                    if note.is_empty() {
                        "Opens Q Calc from anywhere.".into()
                    } else {
                        note
                    }
                } else {
                    String::new()
                };
                out.push(row(
                    &format!("key:{id}"),
                    label,
                    &hint,
                    Control::Key {
                        label: text,
                        recording: recording.is_some(),
                        changed: chord != default_keybind(action, true),
                    },
                ));
                if action == KeyAction::Show
                    && let Some(v) = &win.verdict
                    && !v.free
                    && !v.suggest.is_empty()
                {
                    out.push(row(
                        "keySuggest",
                        &format!("Use {} instead", chord_label(&v.suggest, true)),
                        "It's free right now.",
                        Control::Button {
                            label: "Use it".into(),
                            enabled: true,
                        },
                    ));
                }
            }
            let changed = !s.keybinds.is_empty() || app.hotkey.wanted != hotkey::STANDARD;
            out.push(row(
                "keysReset",
                "Reset all",
                "Every shortcut back to Q Calc's own.",
                Control::Button {
                    label: "Reset All".into(),
                    enabled: changed,
                },
            ));
        }
        Page::Messages => match MESSAGES.with(Cell::get) {
            Some(hook) => out.extend((hook.items)(app)),
            None => out.push(Item::Note(
                "Messages aren't in this version of the native app yet. The name others see you by on the relay will be set here."
                    .into(),
                false,
            )),
        },
        Page::Updates => {
            let st = update::status();
            let auto = app.store.auto_update;
            out.push(Item::Heading(format!("Q Calc {}", update::VERSION)));
            let busy = matches!(
                st.state,
                UpdateState::Checking | UpdateState::Downloading | UpdateState::Installing
            );
            let control = match st.state {
                UpdateState::Off => Control::None,
                UpdateState::Ready => Control::Button {
                    label: "Install now".into(),
                    enabled: true,
                },
                _ => Control::Button {
                    label: "Check now".into(),
                    enabled: !busy,
                },
            };
            out.push(row("update", "Status", &update_line(&st, auto, now_s()), control));
            if st.state != UpdateState::Off {
                out.push(row(
                    "autoUpdate",
                    "Update automatically",
                    "Check daily and install when Q Calc is put away and you're away. Off, only when you ask here.",
                    Control::Toggle(auto),
                ));
            }
        }
    }
    out
}

// ---- changes ----

/// The settings were changed here: to the bar, saved.
fn apply_settings(app: &mut App) {
    let next = app.store.settings.clone();
    let theme = next.theme != app.model.settings.theme;
    let share = next.share_usage != app.model.settings.share_usage;
    app.model.set_settings(next);
    if theme {
        app.theme = Theme::for_setting(&app.store.settings.theme);
        app.bar.set_dark(app.theme.dark);
    }
    if share {
        analytics::set_sharing(app, app.store.settings.share_usage);
    }
    app.save_store();
    if !app.model.field.text().trim().is_empty() {
        app.model.evaluate_live(&app.engine);
    }
}

fn patch(app: &mut App, v: Value) {
    if app.store.settings.merge(&v) {
        apply_settings(app);
    }
}

/// A row's control changed (`value`: a bool, a choice's id, text, or null for a button).
fn changed(app: &mut App, win: &mut Win, id: &str, value: Value) {
    if win.page == Page::Messages {
        if let Some(hook) = MESSAGES.with(Cell::get) {
            (hook.changed)(app, id, &value);
        }
        return;
    }
    let text = value.as_str().unwrap_or_default().to_string();
    let number = || text.parse::<f64>().ok();
    match id {
        "autostart" => {
            autostart::apply(false, Some(value.as_bool() == Some(true)));
        }
        "spotSeconds" => {
            if let Some(n) = number() {
                app.store.spot_seconds = store::spot_seconds(&json!(n));
                app.save_store();
            }
        }
        "draftSeconds" | "sigFigs" => {
            if let Some(n) = number() {
                patch(app, json!({ id: n }));
            }
        }
        "tutorial" => tray::show_with(app, "tutorial"),
        "unitsReset" => patch(app, json!({ "defaultUnits": {} })),
        "update" => {
            if update::status().state == UpdateState::Ready {
                update::ask_install();
            } else {
                update::ask_check();
            }
        }
        "autoUpdate" => {
            let on = value.as_bool() == Some(true);
            app.store.auto_update = on;
            update::set_auto(on);
            app.save_store();
        }
        "keysReset" => {
            win.stop_recording(app);
            win.note.clear();
            win.verdict = None;
            win.refused.clear();
            patch(app, json!({ "keybinds": {} }));
            if app.hotkey.wanted != hotkey::STANDARD {
                pick_show(app, win, hotkey::STANDARD);
            }
        }
        "keySuggest" => {
            if let Some(v) = win.verdict.clone() {
                pick_show(app, win, &v.suggest);
            }
        }
        _ if id.starts_with("unit:") => {
            let dim = &id["unit:".len()..];
            let mut units = app.store.settings.default_units.clone();
            if text.is_empty() {
                units.remove(dim);
            } else {
                units.insert(dim.to_string(), text);
            }
            patch(app, json!({ "defaultUnits": units }));
        }
        _ if id.starts_with("key:") => {
            if let Some(action) = KeyAction::from_id(&id["key:".len()..]) {
                win.toggle_recording(app, action);
            }
        }
        _ => patch(app, json!({ id: value })),
    }
    win.dirty = true;
}

/// A chord for one of the bar's actions: the core's rule that a default is stored as no override.
fn pick_key(app: &mut App, action: KeyAction, chord: &str) {
    let next = kb::set_keybind(&binds(&app.store.settings), action, chord, true);
    patch(app, json!({ "keybinds": kb::keybinds_json(&next) }));
}

/// The verdict on a chord for show / hide: free (or already Q Calc's), or taken with its likely owner and a free
/// chord to offer. src-tauri/src/hotkey.rs's `probe`.
pub fn probe(
    raw: &str,
    active: Option<&str>,
    mut bind: impl FnMut(&str) -> bool,
    owner: impl FnOnce(&str) -> Option<hotkey::Owner>,
) -> HotkeyVerdict {
    const SUGGESTIONS: [&str; 5] = [
        "ctrl+alt+space",
        "alt+q",
        "ctrl+alt+q",
        "ctrl+shift+space",
        "ctrl+alt+k",
    ];
    let chord = hotkey::normalize(raw).filter(|c| hotkey::win32_chord(c).is_some());
    let name = chord.clone().unwrap_or_else(|| raw.to_lowercase());
    let usable = chord.is_some();
    if chord
        .as_deref()
        .is_some_and(|c| active == Some(c) || bind(c))
    {
        return HotkeyVerdict {
            chord: name,
            usable,
            free: true,
            ..HotkeyVerdict::default()
        };
    }
    let suggest = SUGGESTIONS
        .iter()
        .find(|c| **c != name && active != Some(**c) && bind(c))
        .map_or(String::new(), |c| (*c).to_string());
    let owner = if usable { owner(&name) } else { None };
    HotkeyVerdict {
        chord: name,
        usable,
        free: false,
        owner: owner.as_ref().map_or(String::new(), |o| o.id.to_string()),
        sure: owner.is_some_and(|o| o.sure),
        suggest,
    }
}

/// Registers the chord on `hwnd` and lets go at once: whether another app holds it.
fn free(hwnd: HWND, chord: &str) -> bool {
    let Some((mods, vk)) = hotkey::win32_chord(chord) else {
        return false;
    };
    // SAFETY: our own window and id, unregistered straight away
    unsafe {
        if RegisterHotKey(
            Some(hwnd),
            PROBE_ID,
            HOT_KEY_MODIFIERS(mods) | MOD_NOREPEAT,
            vk,
        )
        .is_err()
        {
            return false;
        }
        let _ = UnregisterHotKey(Some(hwnd), PROBE_ID);
    }
    true
}

fn probe_now(app: &App, hwnd: HWND, chord: &str) -> HotkeyVerdict {
    probe(
        chord,
        app.hotkey.active.as_deref(),
        |c| free(hwnd, c),
        |c| hotkey::guess(c, &hotkey::running_launchers()),
    )
}

/// A new show / hide chord: tried first; kept only when it is Q Calc's.
fn pick_show(app: &mut App, win: &mut Win, chord: &str) {
    let v = probe_now(app, win.hwnd, chord);
    if !v.free {
        qlog!("settings: {chord} is taken ({})", v.owner);
        // the row says why (hotkey_note with this verdict) and the next row offers the free chord
        win.note.clear();
        win.refused = v.chord.clone();
        win.verdict = Some(v);
        app.hotkey.kick("settings");
        return;
    }
    let ok = app.hotkey.select(&v.chord);
    win.verdict = None;
    if ok {
        app.store.hotkey = app.hotkey.wanted.clone();
        app.save_store();
        win.refused.clear();
        win.note.clear();
    } else {
        win.refused = v.chord.clone();
        win.note = format!("{} is in use by another app", chord_label(&v.chord, true));
    }
}

// ---- the window ----

#[derive(Clone, Debug, PartialEq)]
struct Recording {
    action: KeyAction,
    /// the modifiers held now ("ctrl+alt"), and the most held since the last key
    held: String,
    peak: String,
}

struct Formats {
    title: IDWriteTextFormat,
    heading: IDWriteTextFormat,
    row: IDWriteTextFormat,
    hint: IDWriteTextFormat,
    nav: IDWriteTextFormat,
}

impl Formats {
    fn new(gfx: &Gfx) -> windows::core::Result<Formats> {
        let fam = gfx.fonts.text_family.as_str();
        let big = gfx.fonts.family.as_str();
        Ok(Formats {
            title: render::format(&gfx.dw, big, 24.0, DWRITE_FONT_WEIGHT_SEMI_BOLD)?,
            heading: render::format(&gfx.dw, fam, 14.0, DWRITE_FONT_WEIGHT_SEMI_BOLD)?,
            row: render::format(&gfx.dw, fam, 14.0, DWRITE_FONT_WEIGHT_REGULAR)?,
            hint: render::format(&gfx.dw, fam, 12.0, DWRITE_FONT_WEIGHT_REGULAR)?,
            nav: render::format(&gfx.dw, fam, 14.0, DWRITE_FONT_WEIGHT_REGULAR)?,
        })
    }
}

/// What a click or a key lands on.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    Nav(Page),
    /// an item's control; `part` is a choice's index (or 1 for a key's reset arrow)
    Control {
        item: usize,
        part: usize,
    },
}

/// One laid-out item (content coordinates, before scrolling).
struct Laid {
    card: Rect,
    title: Option<(IDWriteTextLayout, Rect)>,
    hint: Option<(IDWriteTextLayout, Rect)>,
    /// the control's pieces: rect, label, which part
    parts: Vec<(Rect, Option<IDWriteTextLayout>, usize)>,
}

struct Layout {
    width: f32,
    height: f32,
    items: Vec<Item>,
    laid: Vec<Laid>,
    page_title: Option<IDWriteTextLayout>,
    nav: Vec<(Page, Rect, Option<IDWriteTextLayout>)>,
    /// content height, for scrolling
    content: f32,
}

struct Win {
    hwnd: HWND,
    surface: Surface,
    fonts: Formats,
    page: Page,
    scroll: f32,
    /// a wheel notch's glide: from, to, started (history.rs's ease)
    glide: Option<(f32, f32, std::time::Instant)>,
    layout: Option<Layout>,
    dirty: bool,
    hover: Option<Target>,
    /// keyboard focus: an index into `focusables`
    focus: Option<usize>,
    recording: Option<Recording>,
    note: String,
    verdict: Option<HotkeyVerdict>,
    refused: String,
    /// the text control being edited: its item id and field
    editing: Option<(String, Field)>,
    icons: Vec<HICON>,
    dark: bool,
    /// work that sends messages back, done after the borrows end
    pending: Vec<Pending>,
}

enum Pending {
    Menu {
        item: usize,
        at: Rect,
        options: Vec<(String, String)>,
        value: String,
    },
    Destroy,
    Move(RECT),
}

fn color_of(theme: &Theme, dark: bool) -> (D2D1_COLOR_F, D2D1_COLOR_F) {
    // page, card
    if dark {
        (render::rgb(0x202022, 1.0), theme.bg)
    } else {
        (theme.bg_tape, theme.bg)
    }
}

fn wrap(gfx: &Gfx, text: &str, f: &IDWriteTextFormat, w: f32) -> Option<(IDWriteTextLayout, f32)> {
    let wide: Vec<u16> = text.encode_utf16().collect();
    // SAFETY: wide outlives the call; the format is live
    let l = unsafe { gfx.dw.CreateTextLayout(&wide, f, w.max(1.0), 10_000.0) }.ok()?;
    let (_, h) = measure(&l);
    Some((l, h))
}

fn one_line(
    gfx: &Gfx,
    text: &str,
    f: &IDWriteTextFormat,
    w: f32,
    h: f32,
) -> Option<IDWriteTextLayout> {
    gfx.layout_in(text, f, w, h, DWRITE_TEXT_ALIGNMENT_CENTER)
}

fn text_w(gfx: &Gfx, text: &str, f: &IDWriteTextFormat) -> f32 {
    gfx.layout(text, f, 0.0, 20.0)
        .map_or(0.0, |l| measure(&l).0)
}

impl Win {
    fn scale(&self) -> f32 {
        self.surface.scale()
    }

    fn client(&self) -> (f32, f32) {
        let mut r = RECT::default();
        // SAFETY: our window, a live out rect
        let _ = unsafe { GetClientRect(self.hwnd, &mut r) };
        let s = self.scale();
        ((r.right - r.left) as f32 / s, (r.bottom - r.top) as f32 / s)
    }

    fn invalidate(&self) {
        // SAFETY: our window; the paint comes later
        let _ = unsafe { InvalidateRect(Some(self.hwnd), None, false) };
    }

    /// Lays the page out again when something changed.
    fn ensure_layout(&mut self, app: &App) {
        let (w, h) = self.client();
        let stale = self
            .layout
            .as_ref()
            .is_none_or(|l| (l.width - w).abs() > 0.5 || (l.height - h).abs() > 0.5);
        if !self.dirty && !stale {
            return;
        }
        self.dirty = false;
        let items = items(app, self);
        let gfx = &app.gfx;
        let f = &self.fonts;
        let x0 = NAV_W + PAD;
        let cw = (w - x0 - PAD).max(200.0);
        let mut y = PAD;
        let page_title = gfx.layout(self.page.title(), &f.title, 0.0, 34.0);
        y += 34.0 + 16.0;
        let mut laid = Vec::new();
        for (i, item) in items.iter().enumerate() {
            match item {
                Item::Heading(text) => {
                    y += 14.0;
                    let t = wrap(gfx, text, &f.heading, cw);
                    let th = t.as_ref().map_or(18.0, |t| t.1);
                    laid.push(Laid {
                        card: Rect::new(x0, y, cw, th),
                        title: t.map(|(l, h)| (l, Rect::new(x0, y, cw, h))),
                        hint: None,
                        parts: Vec::new(),
                    });
                    y += th + 8.0;
                }
                Item::Note(text, _) => {
                    let t = wrap(gfx, text, &f.hint, cw);
                    let th = t.as_ref().map_or(16.0, |t| t.1);
                    laid.push(Laid {
                        card: Rect::new(x0, y, cw, th),
                        title: None,
                        hint: t.map(|(l, h)| (l, Rect::new(x0, y, cw, h))),
                        parts: Vec::new(),
                    });
                    y += th + 12.0;
                }
                Item::Row {
                    title,
                    hint,
                    control,
                    ..
                } => {
                    let (ctl_w, ctl_h) = self.control_size(gfx, control);
                    let tw = (cw - 2.0 * ROW_PAD - ctl_w - 16.0).max(120.0);
                    let t = wrap(gfx, title, &f.row, tw);
                    let hh = if hint.is_empty() {
                        None
                    } else {
                        wrap(gfx, hint, &f.hint, tw)
                    };
                    let text_h =
                        t.as_ref().map_or(18.0, |t| t.1) + hh.as_ref().map_or(0.0, |h| 2.0 + h.1);
                    let row_h = text_h.max(ctl_h) + 2.0 * ROW_PAD;
                    let card = Rect::new(x0, y, cw, row_h.max(56.0));
                    let ty = card.y + (card.h - text_h) / 2.0;
                    let title_rect =
                        Rect::new(x0 + ROW_PAD, ty, tw, t.as_ref().map_or(18.0, |t| t.1));
                    let hint_rect = Rect::new(
                        x0 + ROW_PAD,
                        title_rect.bottom() + 2.0,
                        tw,
                        hh.as_ref().map_or(0.0, |h| h.1),
                    );
                    let ctl = Rect::new(
                        card.right() - ROW_PAD - ctl_w,
                        card.y + (card.h - ctl_h) / 2.0,
                        ctl_w,
                        ctl_h,
                    );
                    let parts = self.control_parts(gfx, control, ctl);
                    laid.push(Laid {
                        card,
                        title: t.map(|(l, _)| (l, title_rect)),
                        hint: hh.map(|(l, _)| (l, hint_rect)),
                        parts,
                    });
                    let _ = i;
                    y += card.h + GAP;
                }
            }
        }
        let nav = Page::ALL
            .iter()
            .enumerate()
            .map(|(k, p)| {
                let r = Rect::new(12.0, PAD + 50.0 + k as f32 * 40.0, NAV_W - 24.0, 36.0);
                let l = gfx.layout_in(
                    p.nav(),
                    &f.nav,
                    r.w - 28.0,
                    r.h,
                    DWRITE_TEXT_ALIGNMENT_LEADING,
                );
                (*p, r, l)
            })
            .collect();
        self.layout = Some(Layout {
            width: w,
            height: h,
            items,
            laid,
            page_title,
            nav,
            content: y + PAD,
        });
        self.clamp_scroll();
    }

    fn control_size(&self, gfx: &Gfx, c: &Control) -> (f32, f32) {
        let f = &self.fonts;
        match c {
            Control::None => (0.0, 0.0),
            Control::Toggle(_) => (44.0, 22.0),
            Control::Seg { options, .. } => (
                options
                    .iter()
                    .map(|(_, l)| text_w(gfx, l, &f.hint) + 24.0)
                    .sum::<f32>()
                    + 4.0,
                30.0,
            ),
            // one width for every list, so a page's controls line up
            Control::Select { options, .. } => {
                let widest = options
                    .iter()
                    .map(|(_, l)| text_w(gfx, l, &f.hint))
                    .fold(0.0, f32::max);
                ((widest + 44.0).clamp(190.0, 300.0), 30.0)
            }
            Control::Button { label, .. } => ((text_w(gfx, label, &f.hint) + 32.0).max(88.0), 30.0),
            Control::Key { label, .. } => {
                ((text_w(gfx, label, &f.hint) + 28.0).max(150.0) + 34.0, 30.0)
            }
            Control::Text { .. } => (240.0, 30.0),
        }
    }

    fn control_parts(
        &self,
        gfx: &Gfx,
        c: &Control,
        r: Rect,
    ) -> Vec<(Rect, Option<IDWriteTextLayout>, usize)> {
        let f = &self.fonts;
        match c {
            Control::None => Vec::new(),
            Control::Toggle(_) => vec![(r, None, 0)],
            Control::Seg { options, .. } => {
                let mut x = r.x + 2.0;
                options
                    .iter()
                    .enumerate()
                    .map(|(k, (_, l))| {
                        let w = text_w(gfx, l, &f.hint) + 24.0;
                        let part = Rect::new(x, r.y + 2.0, w, r.h - 4.0);
                        x += w;
                        (part, one_line(gfx, l, &f.hint, w, r.h - 4.0), k)
                    })
                    .collect()
            }
            Control::Select { options, value } => {
                let label = options
                    .iter()
                    .find(|(i, _)| i == value)
                    .map_or(value.as_str(), |(_, l)| l.as_str());
                let l = gfx.layout_in(
                    label,
                    &f.hint,
                    r.w - 36.0,
                    r.h,
                    DWRITE_TEXT_ALIGNMENT_LEADING,
                );
                vec![(r, l, 0)]
            }
            Control::Button { label, .. } => vec![(r, one_line(gfx, label, &f.hint, r.w, r.h), 0)],
            Control::Key { label, changed, .. } => {
                let key = Rect::new(r.x + 34.0, r.y, r.w - 34.0, r.h);
                let mut out = vec![(key, one_line(gfx, label, &f.hint, key.w, key.h), 0)];
                if *changed {
                    let back = Rect::new(r.x, r.y, 28.0, r.h);
                    out.push((back, one_line(gfx, "↺", &f.row, back.w, back.h), 1));
                }
                out
            }
            Control::Text { value, placeholder } => {
                let shown = self
                    .editing
                    .as_ref()
                    .map_or(value.as_str(), |(_, field)| field.text());
                let text = if shown.is_empty() {
                    placeholder.as_str()
                } else {
                    shown
                };
                let l = gfx.layout_in(
                    text,
                    &f.hint,
                    r.w - 20.0,
                    r.h,
                    DWRITE_TEXT_ALIGNMENT_LEADING,
                );
                vec![(r, l, 0)]
            }
        }
    }

    fn clamp_scroll(&mut self) {
        let Some(l) = &self.layout else { return };
        let max = (l.content - l.height).max(0.0);
        self.scroll = self.scroll.clamp(0.0, max);
    }

    /// Every control part a click or Tab can land on, in order.
    fn focusables(&self) -> Vec<(Target, Rect)> {
        let Some(l) = &self.layout else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (i, laid) in l.laid.iter().enumerate() {
            let enabled = !matches!(
                &l.items[i],
                Item::Row {
                    control: Control::Button { enabled: false, .. },
                    ..
                }
            );
            if !enabled {
                continue;
            }
            for (r, _, part) in &laid.parts {
                out.push((
                    Target::Control {
                        item: i,
                        part: *part,
                    },
                    *r,
                ));
            }
        }
        out
    }

    fn hit(&self, x: f32, y: f32) -> Option<Target> {
        let l = self.layout.as_ref()?;
        if x < NAV_W {
            return l
                .nav
                .iter()
                .find(|(_, r, _)| r.contains(x, y))
                .map(|(p, _, _)| Target::Nav(*p));
        }
        let cy = y + self.scroll;
        self.focusables()
            .into_iter()
            .find(|(_, r)| r.contains(x, cy))
            .map(|(t, _)| t)
    }

    fn set_page(&mut self, app: &mut App, page: Page) {
        if self.page == page {
            return;
        }
        self.stop_recording(app);
        self.commit_text(app);
        self.page = page;
        self.scroll = 0.0;
        self.focus = None;
        self.note.clear();
        self.dirty = true;
        // a shortcut that couldn't be had: ask who holds it, so the row can say
        if page == Page::Keybinds && app.hotkey.gave_up && self.verdict.is_none() {
            let wanted = app.hotkey.wanted.clone();
            let v = probe_now(app, self.hwnd, &wanted);
            if !v.free {
                self.verdict = Some(v);
            }
        }
    }

    fn toggle_recording(&mut self, app: &mut App, action: KeyAction) {
        let again = self.recording.as_ref().is_some_and(|r| r.action == action);
        self.stop_recording(app);
        self.note.clear();
        if again {
            return;
        }
        if action == KeyAction::Show {
            // the old chord must not swallow the press; Alt+Space and Win chords reach this window meanwhile
            app.hotkey.unregister();
        }
        self.recording = Some(Recording {
            action,
            held: String::new(),
            peak: String::new(),
        });
        self.dirty = true;
    }

    fn stop_recording(&mut self, app: &mut App) {
        if let Some(r) = self.recording.take() {
            if r.action == KeyAction::Show {
                // the paused chord comes back
                app.hotkey.kick("settings recorder done");
            }
            self.dirty = true;
        }
    }

    /// Recording ends with a chord picked: no resume of the old chord first, the pick registers what stays.
    fn end_recording(&mut self) {
        self.recording = None;
        self.dirty = true;
    }

    /// A key while recording. True when it was taken.
    fn record_key(&mut self, app: &mut App, vk: u32, down: bool) -> bool {
        let Some(rec) = &mut self.recording else {
            return false;
        };
        let m = mods();
        let held = mods_text(m);
        let is_mod = matches!(vk, 0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0xA0..=0xA5);
        if is_mod || !down {
            if !down && held.is_empty() && !rec.peak.is_empty() && is_mod {
                // every modifier let go with no key between: another app took it
                let text = missed_key_text(&rec.peak);
                rec.peak.clear();
                rec.held.clear();
                self.note = text;
            } else {
                if held.len() > rec.peak.len() {
                    rec.peak.clone_from(&held);
                }
                rec.held = held;
            }
            self.dirty = true;
            return true;
        }
        let action = rec.action;
        rec.peak.clear();
        rec.held.clear();
        let plain = held.is_empty();
        if plain && vk == 0x1B {
            self.stop_recording(app);
            self.note.clear();
            return true;
        }
        if plain && (vk == 0x08 || vk == 0x2E) {
            if action == KeyAction::Show {
                self.note = "Show / hide needs a key".into();
            } else {
                self.stop_recording(app);
                self.note.clear();
                pick_key(app, action, "");
            }
            self.dirty = true;
            return true;
        }
        let Some(chord) = crate::keys::chord(
            crate::keys::Mods {
                ctrl: m.0,
                alt: m.1,
                shift: m.2,
                win: m.3,
            },
            vk,
        ) else {
            self.note = "Press a key".into();
            self.dirty = true;
            return true;
        };
        let chord = hotkey::normalize(&chord).unwrap_or(chord);
        let mut all = binds(&app.store.settings);
        all.insert(KeyAction::Show, app.hotkey.wanted.clone());
        if let Some(why) = chord_problem(&chord, action, &all, true) {
            self.note = why;
            self.dirty = true;
            return true;
        }
        self.end_recording();
        self.note.clear();
        if action == KeyAction::Show {
            pick_show(app, self, &chord);
        } else {
            pick_key(app, action, &chord);
        }
        self.dirty = true;
        true
    }

    /// The text being edited goes to its row.
    fn commit_text(&mut self, app: &mut App) {
        if let Some((id, field)) = self.editing.take() {
            let text = field.text().trim().to_string();
            changed(app, self, &id, Value::String(text));
        }
    }

    fn activate(&mut self, app: &mut App, t: Target) {
        match t {
            Target::Nav(p) => self.set_page(app, p),
            Target::Control { item, part } => {
                let Some(l) = &self.layout else { return };
                let Some(Item::Row { id, control, .. }) = l.items.get(item).cloned() else {
                    return;
                };
                let at = l.laid[item].parts.first().map_or(Rect::default(), |p| p.0);
                if !matches!(control, Control::Text { .. }) {
                    self.commit_text(app);
                }
                match control {
                    Control::Toggle(on) => changed(app, self, &id, Value::Bool(!on)),
                    Control::Seg { options, .. } => {
                        if let Some((v, _)) = options.get(part) {
                            changed(app, self, &id, Value::String(v.clone()));
                        }
                    }
                    Control::Select { options, value } => {
                        self.pending.push(Pending::Menu {
                            item,
                            at,
                            options,
                            value,
                        });
                    }
                    Control::Button { enabled: true, .. } => changed(app, self, &id, Value::Null),
                    Control::Key { .. } => {
                        if let Some(action) = KeyAction::from_id(&id["key:".len().min(id.len())..])
                        {
                            if part == 1 {
                                self.stop_recording(app);
                                self.note.clear();
                                let default = default_keybind(action, true);
                                if action == KeyAction::Show {
                                    pick_show(app, self, default);
                                } else {
                                    pick_key(app, action, default);
                                }
                            } else {
                                self.toggle_recording(app, action);
                            }
                        }
                    }
                    Control::Text { value, .. }
                        if self.editing.as_ref().is_none_or(|(e, _)| *e != id) =>
                    {
                        let mut field = Field::default();
                        field.reset(&value);
                        self.editing = Some((id, field));
                    }
                    _ => {}
                }
                self.dirty = true;
            }
        }
        self.invalidate();
    }

    fn scroll_by(&mut self, dy: f32) {
        self.glide = None;
        self.scroll += dy;
        self.clamp_scroll();
        self.invalidate();
    }

    /// The wheel: a notch glides 60 DIPs (on from where a glide under way is going); a touchpad's steps go as they
    /// come.
    fn wheel(&mut self, delta: f32) {
        let from = self.glide.map_or(self.scroll, |g| g.1);
        if delta.abs() < 120.0 || crate::history::reduced_motion() {
            self.scroll_by(-delta / 120.0 * 60.0);
            return;
        }
        let to = from - delta / 120.0 * 60.0;
        let max = self
            .layout
            .as_ref()
            .map_or(0.0, |l| (l.content - l.height).max(0.0));
        self.glide = Some((self.scroll, to.clamp(0.0, max), std::time::Instant::now()));
        self.glide_step();
    }

    /// One frame of the glide, the next on a one-shot timer; the last ends it.
    fn glide_step(&mut self) {
        let Some((from, to, at)) = self.glide else {
            return;
        };
        let t = at.elapsed().as_secs_f32() * 1000.0 / 140.0;
        self.scroll = crate::history::glide_at(from, to, t);
        self.clamp_scroll();
        self.invalidate();
        if t >= 1.0 {
            self.glide = None;
            return;
        }
        crate::later::free("settings-glide", 8, || {
            with_win(|w| w.glide_step());
        });
    }

    fn move_focus(&mut self, back: bool) {
        let all = self.focusables();
        if all.is_empty() {
            return;
        }
        let n = all.len();
        let next = match self.focus {
            None if back => n - 1,
            None => 0,
            Some(i) if back => (i + n - 1) % n,
            Some(i) => (i + 1) % n,
        };
        self.focus = Some(next);
        // keep it in view
        let r = all[next].1;
        if let Some(l) = &self.layout {
            if r.y - 12.0 < self.scroll {
                self.scroll = r.y - 12.0;
            } else if r.bottom() + 12.0 > self.scroll + l.height {
                self.scroll = r.bottom() + 12.0 - l.height;
            }
        }
        self.clamp_scroll();
        self.invalidate();
    }

    fn on_key(&mut self, app: &mut App, vk: u32, down: bool) -> bool {
        if self.record_key(app, vk, down) {
            self.invalidate();
            return true;
        }
        if !down {
            return false;
        }
        let m = mods();
        if let Some((_, field)) = &mut self.editing {
            match vk {
                0x0D => self.commit_text(app),
                0x1B => self.editing = None,
                0x25 => field.left(m.0, m.2),
                0x27 => field.right(m.0, m.2),
                0x24 => field.home(m.2),
                0x23 => field.end(m.2),
                0x08 => {
                    field.backspace(m.0);
                }
                0x2E => {
                    field.delete(m.0);
                }
                0x41 if m.0 => field.select_all(),
                0x56 if m.0 => {
                    if let Some(t) = crate::clip::get(self.hwnd) {
                        field.paste(&t);
                    }
                }
                0x09 => {
                    self.commit_text(app);
                    self.move_focus(m.2);
                }
                _ => return false,
            }
            self.dirty = true;
            self.invalidate();
            return true;
        }
        match vk {
            // Esc or Ctrl+W closes
            0x1B => self.pending.push(Pending::Destroy),
            0x57 if m.0 => self.pending.push(Pending::Destroy),
            0x09 if m.0 => {
                let i = Page::ALL.iter().position(|p| *p == self.page).unwrap_or(0);
                let n = Page::ALL.len();
                let next = if m.2 { (i + n - 1) % n } else { (i + 1) % n };
                self.set_page(app, Page::ALL[next]);
                self.invalidate();
            }
            0x09 => self.move_focus(m.2),
            0x20 | 0x0D => {
                if let Some(t) = self
                    .focus
                    .and_then(|i| self.focusables().get(i).map(|f| f.0))
                {
                    self.activate(app, t);
                }
            }
            0x21 => self.scroll_by(-300.0),
            0x22 => self.scroll_by(300.0),
            0x24 if !m.0 => self.scroll_by(-1e6),
            0x23 if !m.0 => self.scroll_by(1e6),
            0x26 => self.scroll_by(-40.0),
            0x28 => self.scroll_by(40.0),
            _ => return false,
        }
        true
    }

    fn paint(&mut self, app: &App) {
        self.ensure_layout(app);
        let theme = app.theme;
        let dark = theme.dark;
        let (page_bg, card_bg) = color_of(&theme, dark);
        let focus_rect = self
            .focus
            .and_then(|i| self.focusables().get(i).map(|f| f.1));
        let Some(l) = &self.layout else { return };
        let scroll = self.scroll;
        let hover = self.hover;
        let recording = self.recording.is_some();
        let editing = self.editing.as_ref().map(|(id, f)| (id.clone(), f.clone()));
        let fonts = &self.fonts;
        let page = self.page;
        let result = self.surface.draw(&app.gfx, &theme, |c: &mut Canvas| {
            c.clear(page_bg);
            // the sidebar
            let side = if dark {
                render::rgb(0x1b1b1d, 1.0)
            } else {
                render::rgb(0xebebef, 1.0)
            };
            c.fill(Rect::new(0.0, 0.0, NAV_W, c.height), side);
            if let Some(t) = c.gfx.layout("Settings", &fonts.heading, 0.0, 24.0) {
                c.text(&t, 26.0, PAD + 6.0, theme.muted);
            }
            for (p, r, label) in &l.nav {
                let on = *p == page;
                if on {
                    c.fill_round(*r, 6.0, theme.fill_selected);
                    c.fill_round(
                        Rect::new(r.x, r.y + 9.0, 3.0, r.h - 18.0),
                        1.5,
                        theme.accent,
                    );
                } else if hover == Some(Target::Nav(*p)) {
                    c.fill_round(*r, 6.0, theme.fill_hover);
                }
                if let Some(t) = label {
                    c.text(t, r.x + 16.0, r.y, theme.ink);
                }
            }
            // the page
            let view = Rect::new(NAV_W, 0.0, c.width - NAV_W, c.height);
            c.push_clip(view);
            if let Some(t) = &l.page_title {
                c.text(t, NAV_W + PAD, PAD - scroll, theme.ink);
            }
            for (i, laid) in l.laid.iter().enumerate() {
                let dy = -scroll;
                let card = Rect::new(laid.card.x, laid.card.y + dy, laid.card.w, laid.card.h);
                if card.bottom() < 0.0 || card.y > c.height {
                    continue;
                }
                match &l.items[i] {
                    Item::Heading(_) => {
                        if let Some((t, r)) = &laid.title {
                            c.text(t, r.x, r.y + dy, theme.ink);
                        }
                    }
                    Item::Note(_, warn) => {
                        if let Some((t, r)) = &laid.hint {
                            c.text(
                                t,
                                r.x,
                                r.y + dy,
                                if *warn { theme.accent } else { theme.muted },
                            );
                        }
                    }
                    Item::Row { id, control, .. } => {
                        c.fill_round(card, 8.0, theme.line);
                        c.fill_round(card.inset(1.0, 1.0), 7.0, card_bg);
                        if let Some((t, r)) = &laid.title {
                            c.text(t, r.x, r.y + dy, theme.ink);
                        }
                        if let Some((t, r)) = &laid.hint {
                            c.text(t, r.x, r.y + dy, theme.muted);
                        }
                        let editing_this =
                            editing.as_ref().filter(|(e, _)| e == id).map(|(_, f)| f);
                        draw_control(
                            c,
                            &theme,
                            control,
                            &laid.parts,
                            dy,
                            hover,
                            i,
                            recording,
                            editing_this,
                            fonts,
                        );
                    }
                }
            }
            if let Some(r) = focus_rect {
                let r = Rect::new(r.x - 3.0, r.y - 3.0 - scroll, r.w + 6.0, r.h + 6.0);
                outline(c, r, 8.0, 2.0, theme.accent, card_bg);
            }
            c.pop_clip();
            // the scrollbar, when the page is taller than the window
            if l.content > l.height {
                let track = l.height - 16.0;
                let thumb = (track * l.height / l.content).max(28.0);
                let at = 8.0 + (track - thumb) * scroll / (l.content - l.height);
                c.fill_round(
                    Rect::new(c.width - 8.0, at, 4.0, thumb),
                    2.0,
                    with_alpha(theme.muted, 0.5),
                );
            }
        });
        if let Err(e) = result {
            qlog!("settings: paint failed: {e}");
        }
    }
}

/// A rounded outline: `stroke` wide in `c`, its inside filled with `inside` (the canvas has no stroked shapes).
fn outline(cv: &Canvas, r: Rect, radius: f32, stroke: f32, c: D2D1_COLOR_F, inside: D2D1_COLOR_F) {
    // four bars around the rect keep the inside untouched
    cv.fill_round(Rect::new(r.x, r.y, r.w, stroke), stroke / 2.0, c);
    cv.fill_round(
        Rect::new(r.x, r.bottom() - stroke, r.w, stroke),
        stroke / 2.0,
        c,
    );
    cv.fill_round(Rect::new(r.x, r.y, stroke, r.h), stroke / 2.0, c);
    cv.fill_round(
        Rect::new(r.right() - stroke, r.y, stroke, r.h),
        stroke / 2.0,
        c,
    );
    let _ = (radius, inside);
}

/// A bordered box: border colour, then the fill inset by a pixel.
fn boxed(c: &Canvas, r: Rect, radius: f32, border: D2D1_COLOR_F, fill: D2D1_COLOR_F) {
    c.fill_round(r, radius, border);
    c.fill_round(r.inset(1.0, 1.0), (radius - 1.0).max(0.0), fill);
}

#[allow(clippy::too_many_arguments)]
fn draw_control(
    c: &Canvas,
    theme: &Theme,
    control: &Control,
    parts: &[(Rect, Option<IDWriteTextLayout>, usize)],
    dy: f32,
    hover: Option<Target>,
    item: usize,
    recording: bool,
    editing: Option<&Field>,
    fonts: &Formats,
) {
    let at = |r: &Rect| Rect::new(r.x, r.y + dy, r.w, r.h);
    let hovered = |part: usize| hover == Some(Target::Control { item, part });
    let field_bg = if theme.dark {
        render::rgb(0x3a3a3c, 1.0)
    } else {
        render::rgb(0xffffff, 1.0)
    };
    let border = if theme.dark {
        render::rgb(0xffffff, 0.14)
    } else {
        render::rgb(0x000000, 0.16)
    };
    match control {
        Control::None => {}
        Control::Toggle(on) => {
            let r = at(&parts[0].0);
            if *on {
                c.fill_round(
                    r,
                    r.h / 2.0,
                    if hovered(0) {
                        theme.accent_hover
                    } else {
                        theme.accent
                    },
                );
                let k = Rect::new(r.right() - r.h + 4.0, r.y + 4.0, r.h - 8.0, r.h - 8.0);
                c.fill_round(
                    k,
                    k.h / 2.0,
                    if theme.dark {
                        render::rgb(0x1d1d1f, 1.0)
                    } else {
                        render::rgb(0xffffff, 1.0)
                    },
                );
            } else {
                boxed(
                    c,
                    r,
                    r.h / 2.0,
                    theme.muted,
                    if hovered(0) {
                        theme.fill_hover
                    } else {
                        field_bg
                    },
                );
                let k = Rect::new(r.x + 5.0, r.y + 5.0, r.h - 10.0, r.h - 10.0);
                c.fill_round(k, k.h / 2.0, theme.muted);
            }
        }
        Control::Seg { options, value } => {
            if let (Some(first), Some(last)) = (parts.first(), parts.last()) {
                let whole = Rect::new(
                    first.0.x - 2.0,
                    first.0.y - 2.0 + dy,
                    last.0.right() - first.0.x + 4.0,
                    first.0.h + 4.0,
                );
                boxed(c, whole, 7.0, border, field_bg);
            }
            for (r, label, k) in parts {
                let r = at(r);
                let on = options.get(*k).is_some_and(|(i, _)| i == value);
                if on {
                    c.fill_round(
                        r,
                        5.0,
                        with_alpha(theme.accent, if theme.dark { 0.32 } else { 0.16 }),
                    );
                } else if hovered(*k) {
                    c.fill_round(r, 5.0, theme.fill_hover);
                }
                if let Some(t) = label {
                    c.text(t, r.x, r.y, if on { theme.ink } else { theme.muted });
                }
            }
        }
        Control::Select { .. } => {
            let (r, label, _) = &parts[0];
            let r = at(r);
            boxed(
                c,
                r,
                6.0,
                border,
                if hovered(0) {
                    theme.fill_selected
                } else {
                    field_bg
                },
            );
            if let Some(t) = label {
                c.text(t, r.x + 12.0, r.y, theme.ink);
            }
            if let Some(t) = c.gfx.layout("⌄", &fonts.row, 0.0, r.h - 6.0) {
                c.text(&t, r.right() - 22.0, r.y, theme.muted);
            }
        }
        Control::Button { enabled, .. } => {
            let (r, label, _) = &parts[0];
            let r = at(r);
            let fill = if !enabled {
                field_bg
            } else if hovered(0) {
                theme.fill_selected
            } else {
                field_bg
            };
            boxed(c, r, 6.0, border, fill);
            if let Some(t) = label {
                c.text(
                    t,
                    r.x,
                    r.y,
                    if *enabled {
                        theme.ink
                    } else {
                        with_alpha(theme.muted, 0.6)
                    },
                );
            }
        }
        Control::Key {
            recording: this, ..
        } => {
            for (r, label, k) in parts {
                let r = at(r);
                if *k == 1 {
                    if hovered(1) {
                        c.fill_round(r, 6.0, theme.fill_hover);
                    }
                    if let Some(t) = label {
                        c.text(t, r.x, r.y, theme.muted);
                    }
                    continue;
                }
                let edge = if *this { theme.accent } else { border };
                boxed(
                    c,
                    r,
                    6.0,
                    edge,
                    if hovered(0) && !recording {
                        theme.fill_selected
                    } else {
                        field_bg
                    },
                );
                if let Some(t) = label {
                    c.text(t, r.x, r.y, if *this { theme.accent } else { theme.ink });
                }
            }
        }
        Control::Text { .. } => {
            let (r, label, _) = &parts[0];
            let r = at(r);
            boxed(
                c,
                r,
                6.0,
                if editing.is_some() {
                    theme.accent
                } else {
                    border
                },
                field_bg,
            );
            if let Some(t) = label {
                let empty = editing.is_some_and(|f| f.text().is_empty());
                c.text(
                    t,
                    r.x + 10.0,
                    r.y,
                    if empty { theme.muted } else { theme.ink },
                );
                if let Some(f) = editing {
                    let x = render::caret_x(t, f.to_utf16(f.caret()));
                    c.fill(
                        Rect::new(r.x + 10.0 + x, r.y + 7.0, 1.5, r.h - 14.0),
                        theme.ink,
                    );
                }
            }
        }
    }
}

/// (ctrl, alt, shift, win) held now.
fn mods() -> (bool, bool, bool, bool) {
    // SAFETY: plain queries of this message's keyboard state
    let down = |vk: u16| unsafe { GetKeyState(i32::from(vk)) } < 0;
    (
        down(VK_CONTROL.0),
        down(VK_MENU.0),
        down(VK_SHIFT.0),
        down(VK_LWIN.0) || down(VK_RWIN.0),
    )
}

/// "ctrl+alt": the modifiers held, in the chord order.
fn mods_text(m: (bool, bool, bool, bool)) -> String {
    [(m.0, "ctrl"), (m.1, "alt"), (m.2, "shift"), (m.3, "cmd")]
        .iter()
        .filter(|(on, _)| *on)
        .map(|(_, n)| *n)
        .collect::<Vec<_>>()
        .join("+")
}

// ---- dark popup menus ----

/// Popup menus (the tray's, a choice list) in the dark or light system theme: uxtheme's SetPreferredAppMode
/// (ordinal 135) and FlushMenuThemes (136), which Explorer and Notepad use; absent on old builds, then a no-op.
pub fn dark_menus(dark: bool) {
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows::core::PCSTR;
    // SAFETY: uxtheme is a system DLL that stays loaded; the ordinals have these signatures since 1809
    unsafe {
        let Ok(ux) = LoadLibraryW(w!("uxtheme.dll")) else {
            return;
        };
        if let Some(set) = GetProcAddress(ux, PCSTR(135 as *const u8)) {
            let set: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(set);
            // 2 force dark, 3 force light
            set(if dark { 2 } else { 3 });
        }
        if let Some(flush) = GetProcAddress(ux, PCSTR(136 as *const u8)) {
            let flush: unsafe extern "system" fn() = std::mem::transmute(flush);
            flush();
        }
    }
}

// ---- entry points ----

/// Opens (or brings forward) the settings window, on the page it was on.
pub fn open(app: &mut App) {
    let page = WIN.with(|w| w.try_borrow().ok().and_then(|w| w.as_ref().map(|w| w.page)));
    open_page(app, page.unwrap_or(Page::General));
}

/// Opens the settings window on `page`.
pub fn open_page(app: &mut App, page: Page) {
    WANT.with(|w| w.set(Some(page)));
    if app.bar.visible {
        app.ops.push(Op::Hide);
    }
    app.ops.push(Op::Run(crate::bg::Later(show_now)));
}

/// Opens the page whose sidebar name is `name` ("updates"), else the one it was on (the bench pipe's `settings`).
pub fn open_named(app: &mut App, name: &str) {
    match Page::ALL
        .iter()
        .find(|p| p.nav().eq_ignore_ascii_case(name))
    {
        Some(p) => open_page(app, *p),
        None => open(app),
    }
}

/// The settings changed elsewhere (the bar's shortcuts, the theme, the update loop): shown again if open.
pub fn refresh(_app: &mut App) {
    WIN.with(|w| {
        if let Ok(mut w) = w.try_borrow_mut()
            && let Some(win) = w.as_mut()
        {
            win.dirty = true;
            win.invalidate();
        }
    });
}

/// Whether the window is open (tests, the bench).
pub fn is_open() -> bool {
    WIN.with(|w| w.try_borrow().is_ok_and(|w| w.is_some()))
}

/// Outside any borrow (Op::Run): create or raise the window.
fn show_now() {
    let page = WANT.with(Cell::take).unwrap_or(Page::General);
    let existing = WIN.with(|w| w.borrow().as_ref().map(|w| w.hwnd));
    if let Some(hwnd) = existing {
        with_app(|app| {
            WIN.with(|w| {
                if let Some(win) = w.borrow_mut().as_mut() {
                    win.set_page(app, page);
                    win.dirty = true;
                }
            })
        });
        // SAFETY: our window
        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(hwnd);
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
        return;
    }
    let t = std::time::Instant::now();
    let Some(hwnd) = create() else { return };
    let made = with_app(|app| {
        let fonts = Formats::new(&app.gfx).ok()?;
        // SAFETY: plain query on our window
        let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
        let dark = app.theme.dark;
        set_dark(hwnd, dark);
        let mut win = Box::new(Win {
            hwnd,
            surface: Surface::new_flip(hwnd, dpi),
            fonts,
            page: Page::General,
            scroll: 0.0,
            glide: None,
            layout: None,
            dirty: true,
            hover: None,
            focus: None,
            recording: None,
            note: String::new(),
            verdict: None,
            refused: String::new(),
            editing: None,
            icons: Vec::new(),
            dark,
            pending: Vec::new(),
        });
        win.set_page(app, page);
        let mut r = RECT::default();
        // SAFETY: our window
        let _ = unsafe { GetClientRect(hwnd, &mut r) };
        win.surface
            .resize((r.right - r.left) as u32, (r.bottom - r.top) as u32);
        analytics::track(app, "settings.open");
        Some((win, dpi))
    });
    let Some(Some((mut win, dpi))) = made else {
        // SAFETY: our window
        let _ = unsafe { DestroyWindow(hwnd) };
        return;
    };
    // the window's icons, then its place: centred on the pointer's monitor, sized for its DPI
    let scale = dpi as f32 / 96.0;
    for (kind, px) in [(ICON_BIG, 32.0 * scale), (ICON_SMALL, 16.0 * scale)] {
        if let Some(icon) = tray::icon(px as u32, false) {
            // SAFETY: our window; the icon is kept until the window goes
            unsafe {
                SendMessageW(
                    hwnd,
                    WM_SETICON,
                    Some(WPARAM(kind as usize)),
                    Some(LPARAM(icon.0 as isize)),
                );
            }
            win.icons.push(icon);
        }
    }
    let monitor = crate::window::cursor_monitor();
    let a = &monitor.area;
    let s = monitor.scale() as f32;
    let (w, h) = (
        (SIZE.0 * s) as i32,
        ((SIZE.1 * s) as i32).min(a.height as i32 - 40),
    );
    let x = a.x as i32 + (a.width as i32 - w) / 2;
    let y = a.y as i32 + (a.height as i32 - h) / 2;
    WIN.with(|cell| *cell.borrow_mut() = Some(win));
    // SAFETY: our window; these send messages back, which now find the window state
    unsafe {
        let _ = SetWindowPos(hwnd, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
    qlog!(
        "settings: opened in {:.1} ms",
        t.elapsed().as_secs_f64() * 1000.0
    );
}

fn create() -> Option<HWND> {
    // SAFETY: the class is registered once; the window is ours
    unsafe {
        let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).ok()?;
        if !CLASS_DONE.with(Cell::get) {
            let class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: CLASS,
                ..Default::default()
            };
            RegisterClassExW(&class);
            CLASS_DONE.with(|c| c.set(true));
        }
        CreateWindowExW(
            WS_EX_APPWINDOW,
            CLASS,
            w!("Q Calc Settings"),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .ok()
    }
}

fn set_dark(hwnd: HWND, dark: bool) {
    let v = BOOL::from(dark);
    // SAFETY: our window, a live value of the attribute's size
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&v as *const BOOL).cast(),
            std::mem::size_of::<BOOL>() as u32,
        )
    };
}

fn lo(v: isize) -> i32 {
    i32::from((v & 0xffff) as u16 as i16)
}

fn hi(v: isize) -> i32 {
    i32::from(((v >> 16) & 0xffff) as u16 as i16)
}

/// Runs `f` on the window's state, unless it is missing or busy (a message sent back while it is borrowed).
fn with_win<R>(f: impl FnOnce(&mut Win) -> R) -> Option<R> {
    WIN.with(|cell| {
        let mut slot = cell.try_borrow_mut().ok()?;
        slot.as_mut().map(|w| f(w))
    })
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_GETMINMAXINFO => {
            // SAFETY: plain query; lParam is the system's MINMAXINFO for this message
            unsafe {
                let s = GetDpiForWindow(hwnd).max(96) as f32 / 96.0;
                let info = &mut *(lp.0 as *mut MINMAXINFO);
                info.ptMinTrackSize = POINT {
                    x: (MIN.0 * s) as i32,
                    y: (MIN.1 * s) as i32,
                };
            }
            return LRESULT(0);
        }
        WM_DESTROY => {
            let gone = WIN.with(|w| w.try_borrow_mut().ok().and_then(|mut w| w.take()));
            if let Some(mut win) = gone {
                with_app(|app| {
                    win.stop_recording(app);
                    win.commit_text(app);
                });
                for icon in win.icons.drain(..) {
                    // SAFETY: icons we made
                    let _ = unsafe { DestroyIcon(icon) };
                }
                drop(win);
                qlog!("settings: closed, its resources released");
            }
            return LRESULT(0);
        }
        _ => {}
    }
    let _ = CREATESTRUCTW::default;
    let out = with_app(|app| {
        let r = with_win(|w| on_message(w, app, msg, wp, lp));
        app.after();
        r
    });
    crate::app::run_ops();
    run_pending();
    match out {
        Some(Some(Some(r))) => r,
        _ if msg == WM_PAINT => {
            // SAFETY: our window
            let _ = unsafe { ValidateRect(Some(hwnd), None) };
            LRESULT(0)
        }
        // SAFETY: the default for our own window
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

fn on_message(w: &mut Win, app: &mut App, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
    match msg {
        WM_PAINT => {
            w.paint(app);
            // SAFETY: our window
            let _ = unsafe { ValidateRect(Some(w.hwnd), None) };
        }
        WM_ERASEBKGND => return Some(LRESULT(1)),
        WM_SIZE => {
            w.surface
                .resize(lo(lp.0) as u32 & 0xffff, hi(lp.0) as u32 & 0xffff);
            w.invalidate();
        }
        WM_DPICHANGED => {
            w.surface.set_dpi((wp.0 & 0xffff) as u32);
            w.dirty = true;
            // SAFETY: lParam is the suggested rect for this message
            let r = unsafe { *(lp.0 as *const RECT) };
            w.pending.push(Pending::Move(r));
        }
        WM_ACTIVATE => {
            if (wp.0 & 0xffff) as u32 == WA_INACTIVE {
                w.stop_recording(app);
                w.commit_text(app);
            }
            // what changed while away ("checked 5 minutes ago", the autostart entry, a theme)
            w.dirty = true;
            if w.dark != app.theme.dark {
                w.dark = app.theme.dark;
                set_dark(w.hwnd, w.dark);
            }
            w.invalidate();
        }
        WM_CLOSE => w.pending.push(Pending::Destroy),
        WM_MOUSEMOVE => {
            let s = w.scale();
            let t = w.hit(lo(lp.0) as f32 / s, hi(lp.0) as f32 / s);
            if t != w.hover {
                w.hover = t;
                w.invalidate();
            }
        }
        WM_LBUTTONDOWN => {
            let s = w.scale();
            let (x, y) = (lo(lp.0) as f32 / s, hi(lp.0) as f32 / s);
            w.focus = None;
            match w.hit(x, y) {
                Some(t) => w.activate(app, t),
                None => {
                    w.commit_text(app);
                    if w.recording.is_some() {
                        w.stop_recording(app);
                        w.note.clear();
                    }
                    w.invalidate();
                }
            }
        }
        WM_MOUSEWHEEL => {
            let delta = hi(wp.0 as isize) as f32;
            w.wheel(delta);
        }
        WM_SETCURSOR => {
            if (lp.0 & 0xffff) as u32 != 1 {
                return None;
            }
            let over_text = matches!(w.hover, Some(Target::Control { item, .. })
                if w.layout.as_ref().is_some_and(|l| matches!(l.items.get(item), Some(Item::Row { control: Control::Text { .. }, .. }))));
            // SAFETY: system cursors
            unsafe {
                if let Ok(c) = LoadCursorW(None, if over_text { IDC_IBEAM } else { IDC_ARROW }) {
                    SetCursor(Some(c));
                }
            }
            return Some(LRESULT(1));
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if !w.on_key(app, wp.0 as u32, true) {
                return None;
            }
        }
        WM_KEYUP | WM_SYSKEYUP => {
            if !w.on_key(app, wp.0 as u32, false) {
                return None;
            }
        }
        WM_CHAR => {
            if let Some((_, field)) = &mut w.editing {
                let unit = wp.0 as u16;
                if unit >= 0x20 && unit != 0x7f {
                    field.char16(unit);
                    w.dirty = true;
                    w.invalidate();
                }
            }
        }
        // Alt+letter while recording isn't a menu key: no beep
        WM_SYSCHAR if w.recording.is_some() => {}
        // Alt+Space opens the window menu, unless it is the chord being recorded
        WM_SYSCOMMAND if w.recording.is_some() && (wp.0 & 0xfff0) == 0xF100 => {}
        _ => return None,
    }
    Some(LRESULT(0))
}

/// The work that sends messages back: choice lists, closing, a DPI move.
fn run_pending() {
    loop {
        let Some(Some((hwnd, job))) =
            with_win(|w| (!w.pending.is_empty()).then(|| (w.hwnd, w.pending.remove(0))))
        else {
            return;
        };
        match job {
            Pending::Destroy => {
                // SAFETY: our window; WM_DESTROY drops the state
                let _ = unsafe { DestroyWindow(hwnd) };
                return;
            }
            Pending::Move(r) => {
                // SAFETY: our window
                let _ = unsafe {
                    SetWindowPos(
                        hwnd,
                        None,
                        r.left,
                        r.top,
                        r.right - r.left,
                        r.bottom - r.top,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    )
                };
            }
            Pending::Menu {
                item,
                at,
                options,
                value,
            } => {
                let Some((scale, scroll, dark)) = with_win(|w| (w.scale(), w.scroll, w.dark))
                else {
                    return;
                };
                dark_menus(dark);
                let mut pt = POINT {
                    x: (at.x * scale) as i32,
                    y: ((at.bottom() - scroll + 2.0) * scale) as i32,
                };
                // SAFETY: the menu is destroyed below; the strings outlive the calls; our window owns it
                let picked = unsafe {
                    let _ = ClientToScreen(hwnd, &mut pt);
                    let Ok(menu) = CreatePopupMenu() else { return };
                    for (k, (id, label)) in options.iter().enumerate() {
                        let flags = if *id == value {
                            MF_STRING | MF_CHECKED
                        } else {
                            MF_STRING
                        };
                        let _ = AppendMenuW(menu, flags, k + 1, &HSTRING::from(label.as_str()));
                    }
                    let cmd = TrackPopupMenu(
                        menu,
                        TPM_RETURNCMD | TPM_TOPALIGN,
                        pt.x,
                        pt.y,
                        None,
                        hwnd,
                        None,
                    );
                    let _ = DestroyMenu(menu);
                    cmd.0 as usize
                };
                qlog!("settings: list for item {item} picked {picked}");
                if picked > 0
                    && let Some((id, _)) = options.get(picked - 1)
                {
                    let id = id.clone();
                    with_app(|app| {
                        with_win(|w| {
                            if let Some(Some(Item::Row { id: row_id, .. })) =
                                w.layout.as_ref().map(|l| l.items.get(item).cloned())
                            {
                                changed(app, w, &row_id, Value::String(id));
                                w.invalidate();
                            }
                        });
                        app.after();
                    });
                    crate::app::run_ops();
                }
            }
        }
    }
}

/// The window as the e2e suite sees it (the bench pipe's `settings-controls`): open or not, its page, and every
/// control part on screen in physical pixels with its row id and choice, so a test can click one for real.
pub fn debug() -> Value {
    WIN.with(|w| {
        let Ok(w) = w.try_borrow() else {
            return json!({ "open": true, "busy": true });
        };
        let Some(win) = w.as_ref() else {
            return json!({ "open": false });
        };
        let s = win.scale();
        let mut origin = POINT::default();
        // SAFETY: our window, a live out point
        let _ = unsafe { ClientToScreen(win.hwnd, &mut origin) };
        let (cw, ch) = win.client();
        let mut controls = Vec::new();
        if let Some(l) = &win.layout {
            for (i, laid) in l.laid.iter().enumerate() {
                let Some(Item::Row { id, control, .. }) = l.items.get(i) else {
                    continue;
                };
                let (options, value) = match control {
                    Control::Seg { options, value } | Control::Select { options, value } => {
                        (options.iter().map(|(o, _)| o.clone()).collect(), value.clone())
                    }
                    Control::Toggle(on) => (Vec::new(), on.to_string()),
                    _ => (Vec::new(), String::new()),
                };
                for (r, _, part) in &laid.parts {
                    let y = r.y - win.scroll;
                    let visible = y >= 0.0 && y + r.h <= ch && r.x + r.w <= cw;
                    controls.push(json!({
                        "id": id, "part": part, "option": options.get(*part), "value": value, "visible": visible,
                        "rect": [origin.x + (r.x * s) as i32, origin.y + (y * s) as i32, (r.w * s) as i32, (r.h * s) as i32],
                    }));
                }
            }
        }
        json!({ "open": true, "page": win.page.nav(), "dark": win.dark, "controls": controls })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use qcalc_core::app::host_settings::conflict_text;

    #[test]
    fn a_probe_names_the_owner_and_a_free_chord() {
        let held = ["alt+space", "ctrl+alt+space"];
        let free = |c: &str| !held.contains(&c);
        let powertoys = |_: &str| {
            Some(hotkey::Owner {
                id: "powertoys-run",
                sure: true,
            })
        };
        let v = probe("ALT+space", None, free, powertoys);
        assert_eq!(v.chord, "alt+space");
        assert!(v.usable && !v.free);
        assert_eq!((v.owner.as_str(), v.sure), ("powertoys-run", true));
        assert_eq!(v.suggest, "alt+q");
        assert_eq!(
            conflict_text(&v),
            "Alt+Space is used by PowerToys Run. Change it in PowerToys → PowerToys Run → Activation shortcut, or pick another here. Alt+Q is free."
        );
        let v = probe("ctrl+k", None, free, |_| {
            panic!("no owner for a free chord")
        });
        assert!(v.free && v.owner.is_empty() && v.suggest.is_empty());
        // Q Calc's own active chord isn't taken, and isn't offered
        assert!(probe("alt+space", Some("alt+space"), |_| false, |_| None).free);
        assert_eq!(
            probe(
                "ctrl+j",
                Some("alt+q"),
                |c| c == "alt+q" || c == "ctrl+alt+q",
                |_| None
            )
            .suggest,
            "ctrl+alt+q"
        );
        let v = probe("ctrl+", None, |_| true, |_| None);
        assert!(!v.usable && !v.free);
    }

    #[test]
    fn held_modifiers_read_in_chord_order() {
        assert_eq!(mods_text((true, true, false, false)), "ctrl+alt");
        assert_eq!(mods_text((false, false, true, true)), "shift+cmd");
        assert_eq!(mods_text((false, false, false, false)), "");
        assert_eq!(kb::held_label("ctrl+alt", true), "Ctrl+Alt+…");
        assert!(missed_key_text("alt").starts_with("Only Alt came through."));
    }

    #[test]
    fn hints_name_the_key() {
        assert_eq!(
            with_key("Show answers as fractions", "Ctrl+F"),
            "Show answers as fractions · Ctrl+F"
        );
        assert_eq!(
            with_key("Match the precision you typed", ""),
            "Match the precision you typed."
        );
        let mut s = store::Settings::default();
        assert_eq!(key_chord_title("fraction", &s), "Ctrl+F");
        s.merge(&json!({ "keybinds": { "fraction": "ctrl+alt+r", "angle": "" } }));
        assert_eq!(key_chord_title("fraction", &s), "Ctrl+Alt+R");
        assert_eq!(key_chord_title("angle", &s), "");
    }

    #[test]
    fn every_page_has_a_title_and_the_sidebar_order_is_fixed() {
        let names: Vec<&str> = Page::ALL.iter().map(|p| p.nav()).collect();
        assert_eq!(
            names,
            [
                "General", "Answers", "Units", "Keybinds", "Messages", "Updates"
            ]
        );
        assert_eq!(Page::Updates.title(), "Software update");
    }
}
