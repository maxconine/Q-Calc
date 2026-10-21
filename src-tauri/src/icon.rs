// the icon is a black glyph on clear, which a dark title bar or a dark taskbar all but hides; there it goes white
use tauri::image::Image;
use tauri::{AppHandle, Manager, Theme, WebviewWindow};

// the same glyph in white, its edges' alpha kept
pub fn white(icon: &Image<'_>) -> Image<'static> {
    let mut px = icon.rgba().to_vec();
    for p in px.chunks_exact_mut(4) {
        p[0] = 255 - p[0];
        p[1] = 255 - p[1];
        p[2] = 255 - p[2];
    }
    Image::new_owned(px, icon.width(), icon.height())
}

// a window's title bar icon, for the theme it wears
pub fn dress(window: &WebviewWindow) {
    let Some(icon) = window.app_handle().default_window_icon() else { return };
    let dark = window.theme().is_ok_and(|t| t == Theme::Dark);
    let _ = window.set_icon(if dark { white(icon) } else { icon.clone() });
}

pub const TRAY: &str = "main";

// the tray's icon, for the taskbar windows draws: dark by default on windows 10, a choice on 11
pub fn tray(app: &AppHandle) -> Option<Image<'static>> {
    let icon = app.default_window_icon()?;
    Some(if taskbar_dark() { white(icon) } else { icon.clone().to_owned() })
}

// the taskbar can change colour while q calc runs, so the tray looks again now and then
pub fn watch_taskbar(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let mut dark = taskbar_dark();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
            let now = taskbar_dark();
            if now != dark {
                dark = now;
                if let (Some(tray), Some(icon)) = (app.tray_by_id(TRAY), tray(&app)) {
                    let _ = tray.set_icon(Some(icon));
                }
            }
        }
    });
}

#[cfg(windows)]
fn taskbar_dark() -> bool {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (key, value) = (wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"), wide("SystemUsesLightTheme"));
    let mut light: u32 = 1;
    let mut size = std::mem::size_of::<u32>() as u32;
    let read = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), RRF_RT_REG_DWORD, std::ptr::null_mut(), (&mut light as *mut u32).cast(), &mut size)
    };
    // no setting (older windows) is windows 10's dark taskbar
    if read != 0 {
        return true;
    }
    light == 0
}

#[cfg(not(windows))]
fn taskbar_dark() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_goes_white_and_the_clear_stays_clear() {
        let icon = Image::new(&[0, 0, 0, 255, 0, 0, 0, 0, 30, 30, 30, 128], 3, 1);
        assert_eq!(white(&icon).rgba(), &[255, 255, 255, 255, 255, 255, 255, 0, 225, 225, 225, 128]);
    }
}
