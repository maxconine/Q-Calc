use tauri::webview::{Color, ScrollBarStyle};
use tauri::{Theme, WebviewWindow};

// thin scrollbars that fade out, like the mac's; every webview on one data directory has to ask for the same style
#[cfg(windows)]
pub const SCROLL_BARS: ScrollBarStyle = ScrollBarStyle::FluentOverlay;
#[cfg(not(windows))]
pub const SCROLL_BARS: ScrollBarStyle = ScrollBarStyle::Default;

// the settings page's --page on windows, so the window doesn't flash white before it paints
pub fn settings_background(theme: Theme) -> Color {
    match theme {
        Theme::Dark => Color(0x20, 0x20, 0x20, 255),
        _ => Color(0xf3, 0xf3, 0xf3, 255),
    }
}

// dwm draws the overlay's edge the way windows 11 draws a flyout: rounded corners, the system's hairline border
// and a shadow. windows turns rounding off without a gpu (basic display adapter, most vms) and windows 10 never
// rounds, so the page draws a square bar with its own hairline and never depends on this. returns what dwm said,
// for the smoke test
#[cfg(windows)]
pub fn dress(window: &WebviewWindow) -> Vec<String> {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmExtendFrameIntoClientArea, DwmGetWindowAttribute, DwmIsCompositionEnabled, DwmSetWindowAttribute,
        DWMWA_BORDER_COLOR, DWMWA_COLOR_DEFAULT, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };
    use windows_sys::Win32::UI::Controls::MARGINS;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, GWL_EXSTYLE, GWL_STYLE};

    let Ok(hwnd) = window.hwnd() else { return vec!["no hwnd".into()] };
    let hwnd = hwnd.0;
    // any frame in the client area brings dwm's shadow back; one pixel along the bottom, because a taller one
    // would show the caption buttons dwm still keeps at the top right for a window with a system menu
    let glass = MARGINS { cxLeftWidth: 0, cxRightWidth: 0, cyTopHeight: 0, cyBottomHeight: 1 };
    let (round, border) = (DWMWCP_ROUND, DWMWA_COLOR_DEFAULT);
    let (mut composed, mut corner) = (0, -1);
    let (extend, set, get, composition) = unsafe {
        // back from `bare`; windows 10 has no border colour and says so, which is fine
        DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR as u32, (&border as *const u32).cast(), 4);
        (
            DwmExtendFrameIntoClientArea(hwnd, &glass),
            DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE as u32, (&round as *const i32).cast(), 4),
            DwmGetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE as u32, (&mut corner as *mut i32).cast(), 4),
            DwmIsCompositionEnabled(&mut composed),
        )
    };
    let (style, ex_style) = unsafe { (GetWindowLongPtrW(hwnd, GWL_STYLE), GetWindowLongPtrW(hwnd, GWL_EXSTYLE)) };
    vec![
        format!("dwm composition: {} (hr {:#010x})", composed != 0, composition),
        format!("extend frame: hr {extend:#010x}"),
        format!("corner preference round: hr {set:#010x}; reads back {corner} (hr {get:#010x})"),
        format!("style {style:#010x}, ex style {ex_style:#010x}"),
    ]
}

#[cfg(not(windows))]
pub fn dress(_: &WebviewWindow) -> Vec<String> {
    Vec::new()
}

// while the 420 smoke has the window grown around the bar, dwm's shadow, border and rounding would draw a box round
// the smoke; they go until `dress` brings them back
#[cfg(windows)]
pub fn bare(window: &WebviewWindow) {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE,
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
    };
    use windows_sys::Win32::UI::Controls::MARGINS;

    let Ok(hwnd) = window.hwnd() else { return };
    let hwnd = hwnd.0;
    let none = MARGINS { cxLeftWidth: 0, cxRightWidth: 0, cyTopHeight: 0, cyBottomHeight: 0 };
    let (square, border) = (DWMWCP_DONOTROUND, DWMWA_COLOR_NONE);
    unsafe {
        DwmExtendFrameIntoClientArea(hwnd, &none);
        DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE as u32, (&square as *const i32).cast(), 4);
        DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR as u32, (&border as *const u32).cast(), 4);
    }
}

#[cfg(not(windows))]
pub fn bare(_: &WebviewWindow) {}

// the web view's own background: 0,0,0,0 means it should let the window show through
#[cfg(windows)]
pub fn webview_background(window: &WebviewWindow, report: impl FnOnce(String) + Send + 'static) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{ICoreWebView2Controller2, COREWEBVIEW2_COLOR};
    use windows_core::Interface;
    let _ = window.with_webview(move |webview| unsafe {
        let mut c = COREWEBVIEW2_COLOR::default();
        let read = webview.controller().cast::<ICoreWebView2Controller2>().and_then(|c2| c2.DefaultBackgroundColor(&mut c));
        report(match read {
            Ok(()) => format!("webview2 default background: rgba({}, {}, {}, {})", c.R, c.G, c.B, c.A),
            Err(e) => format!("webview2 default background: unreadable ({e})"),
        });
    });
}

#[cfg(not(windows))]
pub fn webview_background(_: &WebviewWindow, _: impl FnOnce(String) + Send + 'static) {}
