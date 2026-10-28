//! The pointer over the tape and the live answer: what is under it (`Spot`), the hand cursor over what a click
//! inserts or copies, and the tooltip after a 450 ms rest (BarView.swift's `.onHover` / `.help`, HistoryTape.tsx's
//! and LiveAnswer.tsx's `title`s). The row and cell under the pointer are drawn by history.rs and answer.rs
//! (`tape_cell`, `answer_item`).
//!
//! The tooltip is the system's (comctl32 v6, a tracking tooltip), so it can stand outside the bar when the bar is
//! one composer tall, follows the light / dark theme (`DarkMode_Explorer`), and is made at the first tip. Nothing
//! here runs on a keystroke but `key_down`, which only clears; no timer runs unless the pointer rests on a cell.

use std::cell::Cell;

use crate::app::App;

/// What the pointer is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spot {
    /// a tape row, and the cell in it (None: the row's background)
    Row(usize, Option<usize>),
    /// a piece of the live answer that copies
    Answer(usize),
    /// a command's words in the answer slot, cut at the end: the whole text on hover
    Slot,
}

thread_local! {
    static SPOT: Cell<Option<Spot>> = const { Cell::new(None) };
}

const TIP_MS: u32 = 450;

/// The tape row and cell under the pointer, for history.rs's paint.
pub fn tape_cell() -> Option<(usize, Option<usize>)> {
    match SPOT.with(Cell::get) {
        Some(Spot::Row(r, c)) => Some((r, c)),
        _ => None,
    }
}

/// The answer piece under the pointer, for answer.rs's paint.
pub fn answer_item() -> Option<usize> {
    match SPOT.with(Cell::get) {
        Some(Spot::Answer(i)) => Some(i),
        _ => None,
    }
}

/// Whether a spot is something a click does something with (the hand cursor).
fn clickable(app: &App, spot: Spot) -> bool {
    match spot {
        Spot::Answer(_) => true,
        Spot::Slot => false,
        Spot::Row(r, Some(k)) => tip_of(app, Spot::Row(r, Some(k))).is_some(),
        Spot::Row(_, None) => false,
    }
}
fn spot_at(app: &App, x: f32, y: f32) -> Option<Spot> {
    if app.bar.selecting || !app.bar.visible {
        return None;
    }
    let l = &app.bar.layout;
    if l.answer.contains(x, y)
        && crate::commands::slot_cut(&app.gfx, &app.model, l.answer).is_some()
    {
        return Some(Spot::Slot);
    }
    if l.answer.contains(x, y) || l.composer.contains(x, y) {
        return crate::answer::item_at(&l.live, x, y).map(Spot::Answer);
    }
    crate::history::cell_at(&app.gfx, &app.model, l.width, &l.tape, x, y)
        .map(|(row, cell, _)| Spot::Row(row, cell))
}

/// The tooltip and the box it belongs to (window DIPs).
fn tip_of(app: &App, spot: Spot) -> Option<(String, crate::render::Rect)> {
    match spot {
        Spot::Answer(i) => {
            let item = app.bar.layout.live.items.get(i)?;
            crate::answer::tip(&app.bar.layout.live, i).map(|t| (t.to_string(), item.rect))
        }
        Spot::Row(r, Some(k)) => crate::history::tip_at(app, r, k),
        Spot::Row(_, None) => None,
        Spot::Slot => {
            let r = app.bar.layout.answer;
            crate::commands::slot_cut(&app.gfx, &app.model, r).map(|t| (t, r))
        }
    }
}

/// The pointer moved over the bar (None: it left). Called from commands.rs's `pointer`.
pub fn pointer(app: &mut App, at: Option<(f32, f32)>) {
    let spot = at.and_then(|(x, y)| spot_at(app, x, y));
    if spot == SPOT.with(Cell::get) {
        return;
    }
    SPOT.with(|s| s.set(spot));
    app.model.touch();
    tip::hide();
    crate::later::cancel("hover-tip");
    if let Some(spot) = spot
        && tip_of(app, spot).is_some()
    {
        crate::later::after("hover-tip", TIP_MS, move |app| {
            if SPOT.with(Cell::get) != Some(spot) || !app.bar.visible {
                return;
            }
            if let Some((text, r)) = tip_of(app, spot) {
                let f = app.bar.frame();
                let s = app.bar.scale();
                let mut pt = windows::Win32::Foundation::POINT::default();
                // SAFETY: a live out point
                let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt) };
                // under the cell, at the pointer's x, as a system tooltip sits
                let left = f.left + (r.x * s).round() as i32;
                let x = pt.x.clamp(left, left + (r.w * s).round() as i32);
                let y = f.top + ((r.bottom() + 4.0) * s).round() as i32;
                let dark = app.theme.dark;
                let owner = app.bar.hwnd.0 as usize;
                crate::later::free("hover-tip-show", 0, move || {
                    tip::show(owner, &text, x, y, dark)
                });
            }
        });
    }
}

/// WM_SETCURSOR over the bar's client area: the hand over a cell or a piece a click takes.
pub fn cursor(app: &App, x: f32, y: f32) -> Option<windows::core::PCWSTR> {
    let spot = spot_at(app, x, y)?;
    clickable(app, spot).then_some(windows::Win32::UI::WindowsAndMessaging::IDC_HAND)
}

/// A key or a click: the tip goes, and the highlight waits for the pointer to move again (the rows may move).
pub fn key_down() {
    if SPOT.with(|s| s.take()).is_some() || tip::shown() {
        crate::later::cancel("hover-tip");
        tip::hide();
    }
}

thread_local! {
    /// the field's run of clicks: when the last one came, where (window DIPs), and how many
    static CLICKS: Cell<(i64, f32, f32, u32)> = const { Cell::new((0, 0.0, 0.0, 0)) };
}

/// How many clicks a click at `now` (ms) and (x, y) makes, given the run so far: within the double-click time and
/// box of the last it adds one, else it starts a new run.
pub fn click_count(
    last: (i64, f32, f32, u32),
    now: i64,
    x: f32,
    y: f32,
    ms: i64,
    slop: f32,
) -> u32 {
    let (at, lx, ly, n) = last;
    if n > 0 && now - at <= ms && (x - lx).abs() <= slop && (y - ly).abs() <= slop {
        n + 1
    } else {
        1
    }
}

/// A click in the line: the third in a row selects all of it (as a browser's input did in the Tauri app, and the
/// Mac's field). True when it took the click.
pub fn triple_click(app: &mut App, x: f32, y: f32) -> bool {
    if !app.bar.layout.field.contains(x, y) {
        CLICKS.with(|c| c.set((0, 0.0, 0.0, 0)));
        return false;
    }
    use windows::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime;
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXDOUBLECLK};
    let now = crate::model::now_ms();
    // SAFETY: plain queries
    let (ms, slop) = unsafe { (GetDoubleClickTime(), GetSystemMetrics(SM_CXDOUBLECLK)) };
    let slop = slop.max(4) as f32 / app.bar.scale();
    let n = click_count(CLICKS.with(Cell::get), now, x, y, i64::from(ms), slop);
    CLICKS.with(|c| c.set((now, x, y, n)));
    if n == 3 {
        app.model.field.select_all();
        app.model.touch();
        return true;
    }
    false
}

/// The bar hid.
pub fn will_hide() {
    SPOT.with(|s| s.set(None));
    crate::later::cancel("hover-tip");
    tip::hide();
}
mod tip {
    //! The one tracking tooltip, made at the first tip and kept (a few KB).
    use std::cell::Cell;

    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::Controls::{
        SetWindowTheme, TOOLTIPS_CLASSW, TTF_ABSOLUTE, TTF_TRACK, TTM_ADDTOOLW, TTM_SETMAXTIPWIDTH,
        TTM_TRACKACTIVATE, TTM_TRACKPOSITION, TTM_UPDATETIPTEXTW, TTS_ALWAYSTIP, TTS_NOPREFIX,
        TTTOOLINFOW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, SendMessageW, WINDOW_STYLE, WS_EX_TOPMOST, WS_POPUP,
    };
    use windows::core::{HSTRING, PCWSTR, PWSTR, w};

    thread_local! {
        static TIP: Cell<usize> = const { Cell::new(0) };
        static OWNER: Cell<usize> = const { Cell::new(0) };
        static UP: Cell<bool> = const { Cell::new(false) };
    }

    pub fn shown() -> bool {
        UP.with(Cell::get)
    }

    fn info(tip_owner: HWND, text: Option<&HSTRING>) -> TTTOOLINFOW {
        TTTOOLINFOW {
            cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
            uFlags: TTF_TRACK | TTF_ABSOLUTE,
            hwnd: tip_owner,
            uId: 1,
            lpszText: text.map_or(PWSTR::null(), |t| PWSTR(t.as_ptr() as *mut u16)),
            ..Default::default()
        }
    }

    fn make(owner: HWND) -> Option<HWND> {
        // SAFETY: a system tooltip window owned by our bar
        let tip = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST,
                TOOLTIPS_CLASSW,
                PCWSTR::null(),
                WS_POPUP | WINDOW_STYLE(TTS_NOPREFIX | TTS_ALWAYSTIP),
                0,
                0,
                0,
                0,
                Some(owner),
                None,
                None,
                None,
            )
            .ok()?
        };
        let empty = HSTRING::from("");
        let ti = info(owner, Some(&empty));
        // SAFETY: messages to our tooltip with live structs
        unsafe {
            SendMessageW(
                tip,
                TTM_ADDTOOLW,
                Some(WPARAM(0)),
                Some(LPARAM(&ti as *const _ as isize)),
            );
            SendMessageW(tip, TTM_SETMAXTIPWIDTH, Some(WPARAM(0)), Some(LPARAM(400)));
        }
        Some(tip)
    }

    /// Shows `text` with its top-left at screen (x, y). Runs outside the app's borrow (later::free).
    pub fn show(owner: usize, text: &str, x: i32, y: i32, dark: bool) {
        let owner = HWND(owner as *mut _);
        let mut tip = HWND(TIP.with(Cell::get) as *mut _);
        if tip.is_invalid() || OWNER.with(Cell::get) != owner.0 as usize {
            let Some(t) = make(owner) else { return };
            tip = t;
            TIP.with(|c| c.set(t.0 as usize));
            OWNER.with(|c| c.set(owner.0 as usize));
        }
        let text = HSTRING::from(text);
        let ti = info(owner, Some(&text));
        let pos = ((y as u32 & 0xffff) << 16) | (x as u32 & 0xffff);
        // SAFETY: messages to our tooltip with live structs
        unsafe {
            let _ = SetWindowTheme(
                tip,
                if dark {
                    w!("DarkMode_Explorer")
                } else {
                    w!("Explorer")
                },
                PCWSTR::null(),
            );
            SendMessageW(
                tip,
                TTM_UPDATETIPTEXTW,
                Some(WPARAM(0)),
                Some(LPARAM(&ti as *const _ as isize)),
            );
            SendMessageW(
                tip,
                TTM_TRACKPOSITION,
                Some(WPARAM(0)),
                Some(LPARAM(pos as isize)),
            );
            SendMessageW(
                tip,
                TTM_TRACKACTIVATE,
                Some(WPARAM(1)),
                Some(LPARAM(&ti as *const _ as isize)),
            );
        }
        UP.with(|u| u.set(true));
    }

    /// Hides the tip if it is up. Safe inside the app's borrow: the tooltip's own window takes the message.
    pub fn hide() {
        if !UP.with(|u| u.replace(false)) {
            return;
        }
        let tip = HWND(TIP.with(Cell::get) as *mut _);
        let owner = HWND(OWNER.with(Cell::get) as *mut _);
        if tip.is_invalid() {
            return;
        }
        let ti = info(owner, None);
        // SAFETY: as in show
        unsafe {
            SendMessageW(
                tip,
                TTM_TRACKACTIVATE,
                Some(WPARAM(0)),
                Some(LPARAM(&ti as *const _ as isize)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::answer::piece_tip;

    #[test]
    fn answer_tips() {
        // one answer
        assert_eq!(
            piece_tip(&[(true, false, "")], 0),
            Some("Copy to clipboard · Ctrl+C also copies")
        );
        // exact ≈ approx
        let dual = [(true, false, ""), (false, true, "≈"), (true, false, "")];
        assert_eq!(piece_tip(&dual, 0), Some("Copy exact value"));
        assert_eq!(piece_tip(&dual, 1), None);
        assert_eq!(piece_tip(&dual, 2), Some("Copy approximation"));
        // x = -2, 2
        let roots = [
            (true, true, ""),
            (true, false, ""),
            (false, false, ""),
            (true, false, ""),
        ];
        assert_eq!(piece_tip(&roots, 0), Some("Copy all the solutions"));
        assert_eq!(piece_tip(&roots, 1), Some("Copy this solution"));
        assert_eq!(piece_tip(&roots, 2), None);
        // a message copies nothing
        assert_eq!(piece_tip(&[(false, true, "")], 0), None);
    }

    #[test]
    fn clicks_in_a_row() {
        use super::click_count;
        let first = click_count((0, 0.0, 0.0, 0), 1000, 50.0, 20.0, 500, 3.0);
        assert_eq!(first, 1);
        let second = click_count((1000, 50.0, 20.0, 1), 1200, 51.0, 20.0, 500, 3.0);
        assert_eq!(second, 2);
        assert_eq!(
            click_count((1200, 51.0, 20.0, 2), 1500, 51.0, 21.0, 500, 3.0),
            3
        );
        // too late, or too far, starts again
        assert_eq!(
            click_count((1200, 51.0, 20.0, 2), 1800, 51.0, 20.0, 500, 3.0),
            1
        );
        assert_eq!(
            click_count((1200, 51.0, 20.0, 2), 1300, 80.0, 20.0, 500, 3.0),
            1
        );
    }

    #[test]
    fn no_steady_answer_while_a_graph_is_typed() {
        use crate::answer::graphing;
        assert!(graphing("graph "));
        assert!(graphing("  Plot sin(x)"));
        assert!(!graphing("graph"));
        assert!(!graphing("grams"));
        assert!(!graphing("2 gr"));
    }
}
