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
}

fn reader_on() -> bool {
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
    if !app.bar.visible || !reader_on() {
        return;
    }
    let now = name(app.model.field.text(), &app.model.answer_text());
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
