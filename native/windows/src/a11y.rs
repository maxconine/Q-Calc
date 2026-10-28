//! Screen readers (Narrator, NVDA, JAWS). The bar draws itself (Direct2D), so it describes itself to UI Automation
//! (uia.rs, `bar_tree`): the line as an edit with its value, the answer and the hint as live text, the tape's rows
//! in sight as list items. While a reader runs, each message's changes are raised as events (focus, value, live
//! region) and the news (`announce`: a mode toggled, a card) as a notification. Before any UIA client has asked
//! for the bar's provider, the older way stands: the bar's name is what it shows, "2+2 = 4", with a name-change
//! event. Nothing runs unless Windows says a reader is on (SPI_GETSCREENREADER, read at most every 2 s), so a
//! keystroke costs nothing extra otherwise.
//!
//! Not exposed: the caret and selection as a text pattern (a reader echoes typed keys itself).

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

/// After every message (line.rs's `after`): with UI Automation's provider handed out (uia.rs), the bar's parts'
/// events and the news as a notification; else the bar's name follows the line and the answer.
pub fn after(app: &App) {
    let news = NEWS.with(|n| std::mem::take(&mut *n.borrow_mut()));
    let hwnd = app.bar.hwnd;
    // Narrator listens to UI Automation without setting SPI_SETSCREENREADER
    if !app.bar.visible || !(reader_on() || crate::uia::heard(hwnd)) {
        return;
    }
    if crate::uia::provided(hwnd) {
        let tree = bar_tree(app);
        // a row picked from the tape is heard as the focus moves to it, not again as news
        let focused = tree
            .nodes
            .iter()
            .find(|n| n.focused)
            .map(|n| n.name.as_str());
        let news: Vec<&str> = news.split(" · ").filter(|s| Some(*s) != focused).collect();
        let news = news.join(" · ");
        UIA_NEWS.with(|n| {
            let mut n = n.borrow_mut();
            if !news.is_empty() {
                if !n.is_empty() {
                    n.push_str(" · ");
                }
                n.push_str(&news);
            }
        });
        let h = hwnd.0 as usize;
        // events can call back into the provider: outside the app's borrow
        crate::later::free("a11y-uia", 0, move || {
            let hwnd = windows::Win32::Foundation::HWND(h as *mut _);
            crate::uia::changed(hwnd, Some(tree));
            let news = UIA_NEWS.with(|n| std::mem::take(&mut *n.borrow_mut()));
            crate::uia::notify(hwnd, &news);
        });
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

thread_local! {
    /// news waiting for the next notification (uia.rs)
    static UIA_NEWS: RefCell<String> = const { RefCell::new(String::new()) };
}

/// The bar for UI Automation (uia.rs's source): the line as an edit with its value, the answer and the hint as live
/// text, the tape's rows in sight as list items, in screen pixels. None while the app is busy.
pub fn bar_source(_hwnd: windows::Win32::Foundation::HWND) -> Option<crate::uia::Tree> {
    crate::app::with_app(|app| bar_tree(app))
}

pub fn bar_tree(app: &App) -> crate::uia::Tree {
    use crate::uia::{Node, Role, Tree};
    let mut nodes = Vec::new();
    if app.bar.visible {
        let f = app.bar.frame();
        let s = f64::from(app.bar.scale());
        let l = &app.bar.layout;
        let at = |r: crate::render::Rect| {
            [
                f64::from(f.left) + f64::from(r.x) * s,
                f64::from(f.top) + f64::from(r.y) * s,
                f64::from(r.w) * s,
                f64::from(r.h) * s,
            ]
        };
        let m = &app.model;
        let row_focused = app.bar.focused && m.selected.is_some();
        nodes.push(Node {
            id: 1,
            role: Role::Edit,
            name: "Calculation".into(),
            value: Some(m.field.text().to_string()),
            help: "Type a calculation; Enter keeps it in the history, Up goes through it".into(),
            rect: at(l.field),
            focusable: true,
            focused: app.bar.focused && !row_focused,
            ..Node::default()
        });
        let answer = m.answer_text();
        if !answer.trim().is_empty() {
            nodes.push(Node {
                id: 2,
                name: answer.trim().to_string(),
                help: "The answer; Ctrl+C copies it".into(),
                rect: at(l.answer),
                live: true,
                ..Node::default()
            });
        }
        let status = status_of(app);
        if !status.trim().is_empty() {
            nodes.push(Node {
                id: 3,
                name: status,
                rect: at(l.tour.or(l.hint).unwrap_or_default()),
                live: true,
                ..Node::default()
            });
        }
        for i in l.tape.drawn.clone() {
            let Some(r) = m.rows.rows.get(i) else {
                continue;
            };
            let selected = m.selected == Some(i);
            nodes.push(Node {
                id: 100 + i as i32,
                role: Role::ListItem,
                name: name(&r.expr, &r.display),
                rect: at(crate::history::row_rect(l.width, &l.tape, i)),
                focusable: true,
                focused: selected && app.bar.focused,
                selected: Some(selected),
                ..Node::default()
            });
        }
    }
    Tree {
        name: "Q Calc".into(),
        nodes,
    }
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
