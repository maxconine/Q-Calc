//! Screen readers (Narrator, NVDA, JAWS): while one runs, the bar's accessible name is what it shows, "2+2 = 4",
//! kept up to date as the answer lands, with a name-change event so the reader speaks it. The bar draws itself
//! (Direct2D), so without this a reader heard only "Q Calc". Nothing runs unless Windows says a reader is on
//! (SPI_GETSCREENREADER, read at most every 2 s), so a keystroke costs nothing extra otherwise.
//!
//! Not a full UI Automation tree (the tape's rows, the caret and selection as text patterns): see README's gaps.

use std::cell::{Cell, RefCell};
use std::time::Instant;

use crate::app::App;

thread_local! {
    /// (reader on, when it was read)
    static READER: Cell<Option<(bool, Instant)>> = const { Cell::new(None) };
    static SPOKEN: RefCell<String> = const { RefCell::new(String::new()) };
    /// something said once, after the line's name (a new card, a hint, a mode toggled, a selection's answer)
    static NEWS: RefCell<String> = const { RefCell::new(String::new()) };
}

/// After Ctrl+D, Ctrl+F or Ctrl+S: where the mode is now (`radians`, `fractions on`…), since the edge tools that
/// show it are hidden off hover (core `mode_announcement`).
pub fn announce_mode(action: &str, on: bool) {
    announce(&qcalc_core::app::onboarding::mode_announcement(action, on));
}

thread_local! {
    /// the hint and tutorial step last read out, so each is heard once as it changes (the page's role="status")
    static STATUS: RefCell<String> = const { RefCell::new(String::new()) };
}

/// The line under the bar as a reader should hear it: the hint, else the tutorial's step; "" for none.
fn status_of(app: &App) -> String {
    match &app.model.hint {
        Some(h) if !h.is_empty() => h.clone(),
        _ => app
            .model
            .tour
            .line
            .as_ref()
            .map(|l| l.text.clone())
            .unwrap_or_default(),
    }
}

/// Has a reader hear `text` with the bar's next name (the Mac's `announce`, the page's aria-live regions). Costs a
/// string while no reader runs; `after` drops it then.
pub fn announce(text: &str) {
    if text.trim().is_empty() {
        return;
    }
    NEWS.with(|n| {
        let mut n = n.borrow_mut();
        if !n.is_empty() {
            n.push_str(" · ");
        }
        n.push_str(text.trim());
    });
}

pub fn reader_on() -> bool {
    // the e2e test's switch: behave as with a reader on, without starting one on the desktop
    if std::env::var_os("QCALC_READER").is_some() {
        return true;
    }
    if let Some((on, at)) = READER.with(Cell::get)
        && at.elapsed().as_secs() < 2
    {
        return on;
    }
    use windows::Win32::UI::WindowsAndMessaging::{
        SPI_GETSCREENREADER, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };
    let mut on = windows::core::BOOL(0);
    // SAFETY: a live out BOOL
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETSCREENREADER,
            0,
            Some((&mut on as *mut windows::core::BOOL).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    let on = ok.is_ok() && on.as_bool();
    READER.with(|r| r.set(Some((on, Instant::now()))));
    on
}

/// What a reader hears for the line and its answer.
pub fn name(text: &str, answer: &str) -> String {
    let text = text.trim();
    match (text.is_empty(), answer.trim().is_empty()) {
        (true, _) => "Q Calc".to_string(),
        (false, true) => text.to_string(),
        (false, false) => format!("{text} = {}", answer.trim()),
    }
}

/// After every message (line.rs's `after`): the bar's name follows the line and the answer.
pub fn after(app: &App) {
    let news = NEWS.with(|n| std::mem::take(&mut *n.borrow_mut()));
    if !app.bar.visible || !reader_on() {
        return;
    }
    let mut now = name(app.model.field.text(), &app.model.answer_text());
    let status = status_of(app);
    let fresh = STATUS.with(|s| {
        let fresh = *s.borrow() != status;
        if fresh {
            s.replace(status.clone());
        }
        fresh
    });
    let news = if fresh && !status.is_empty() && !news.contains(&status) {
        if news.is_empty() {
            status
        } else {
            format!("{news} · {status}")
        }
    } else {
        news
    };
    if !news.is_empty() {
        now = format!("{now}. {news}");
    }
    if SPOKEN.with(|s| *s.borrow() == now) {
        return;
    }
    SPOKEN.with(|s| s.replace(now.clone()));
    let hwnd = app.bar.hwnd.0 as usize;
    // SetWindowText sends WM_SETTEXT back: outside the app's borrow
    crate::later::free("a11y-name", 0, move || {
        use windows::Win32::UI::WindowsAndMessaging::SetWindowTextW;
        let hwnd = windows::Win32::Foundation::HWND(hwnd as *mut _);
        // SAFETY: our bar; a plain notification
        unsafe {
            let _ = SetWindowTextW(hwnd, &windows::core::HSTRING::from(now));
            // EVENT_OBJECT_NAMECHANGE, OBJID_WINDOW, CHILDID_SELF
            NotifyWinEvent(0x800C, hwnd, 0, 0);
        }
    });
}

// user32's NotifyWinEvent (in the windows crate only behind Win32_UI_Accessibility)
#[link(name = "user32")]
unsafe extern "system" {
    fn NotifyWinEvent(event: u32, hwnd: windows::Win32::Foundation::HWND, object: i32, child: i32);
}

#[cfg(test)]
mod tests {
    use super::name;

    #[test]
    fn what_a_reader_hears() {
        assert_eq!(name("", ""), "Q Calc");
        assert_eq!(name("2+2", "4"), "2+2 = 4");
        assert_eq!(name(" 2+ ", ""), "2+");
    }
}
