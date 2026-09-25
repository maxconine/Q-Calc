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

// dwm draws the overlay's edge the way windows 11 draws a flyout: rounded corners, the system's hairline border,
// a shadow and acrylic behind the page's translucent bar. older systems refuse what they lack: windows 10 keeps
// square corners and the shadow, and acrylic needs windows 11 22h2; without it the bar sits on the plain window
#[cfg(windows)]
pub fn dress(window: &WebviewWindow) {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };
    use windows_sys::Win32::UI::Controls::MARGINS;

    let Ok(hwnd) = window.hwnd() else { return };
    let hwnd = hwnd.0;
    let set = |attribute: i32, value: i32| unsafe {
        DwmSetWindowAttribute(hwnd, attribute as u32, (&value as *const i32).cast(), 4);
    };
    // any frame in the client area brings dwm's shadow back; one pixel along the bottom, because a taller one
    // would show the caption buttons dwm still keeps at the top right for a window with a system menu
    let glass = MARGINS { cxLeftWidth: 0, cxRightWidth: 0, cyTopHeight: 0, cyBottomHeight: 1 };
    unsafe {
        DwmExtendFrameIntoClientArea(hwnd, &glass);
    }
    set(DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND);
    set(DWMWA_SYSTEMBACKDROP_TYPE, DWMSBT_TRANSIENTWINDOW);
}

#[cfg(not(windows))]
pub fn dress(_: &WebviewWindow) {}
