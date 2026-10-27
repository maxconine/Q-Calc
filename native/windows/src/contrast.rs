//! Windows' contrast themes (Settings > Accessibility > Contrast themes): while one is on, the bar, settings and
//! the sheets draw in its system colours (window, text, grey text, highlight, hot-track) instead of Q Calc's own,
//! as a WebView did for the Tauri app with `forced-colors`. `apply` runs where render.rs makes the theme (launch,
//! the light / dark setting, Windows' colour changes).

use windows::Win32::Graphics::Gdi::{
    COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HOTLIGHT, COLOR_WINDOW, COLOR_WINDOWTEXT, GetSysColor,
};
use windows::Win32::UI::WindowsAndMessaging::{
    SPI_GETHIGHCONTRAST, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
};

use crate::render::{Color, Theme, rgb, with_alpha};

/// SPI_SETHIGHCONTRAST: WM_SETTINGCHANGE's wParam when a contrast theme goes on or off.
pub const SPI_SETHIGHCONTRAST: usize = 0x0043;

/// HIGHCONTRASTW (Win32_UI_Accessibility's, kept here so the crate needs no more of the windows features).
#[repr(C)]
struct HighContrast {
    size: u32,
    flags: u32,
    scheme: *mut u16,
}

const HCF_HIGHCONTRASTON: u32 = 1;

/// Whether a contrast theme is on.
pub fn active() -> bool {
    // a dev switch, to look at the system-colour theme without turning a contrast theme on for the whole desktop
    if std::env::var_os("QCALC_CONTRAST").is_some() {
        return true;
    }
    let mut hc = HighContrast {
        size: std::mem::size_of::<HighContrast>() as u32,
        flags: 0,
        scheme: std::ptr::null_mut(),
    };
    // SAFETY: a live struct of the size it says
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            hc.size,
            Some((&mut hc as *mut HighContrast).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    ok.is_ok() && hc.flags & HCF_HIGHCONTRASTON != 0
}

/// A COLORREF (0x00BBGGRR) as 0xRRGGBB.
pub fn rgb_of(colorref: u32) -> u32 {
    let (r, g, b) = (
        colorref & 0xff,
        (colorref >> 8) & 0xff,
        (colorref >> 16) & 0xff,
    );
    (r << 16) | (g << 8) | b
}

/// Relative luminance (0..1) of 0xRRGGBB, for which way DWM's frame should go.
pub fn luminance(hex: u32) -> f32 {
    let ch = |s: u32| {
        let c = s as f32 / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * ch(hex >> 16 & 0xff) + 0.7152 * ch(hex >> 8 & 0xff) + 0.0722 * ch(hex & 0xff)
}

/// The theme from the system's contrast colours (0xRRGGBB): window, text, grey text, highlight, hot-track.
pub fn theme_from(window: u32, text: u32, gray: u32, highlight: u32, hot: u32) -> Theme {
    let c = |h: u32| -> Color { rgb(h, 1.0) };
    let hl = c(highlight);
    Theme {
        dark: luminance(window) < 0.5,
        bg: c(window),
        bg_tape: c(window),
        ink: c(text),
        muted: c(gray),
        answer: c(text),
        answer_hover: c(hot),
        accent: hl,
        accent_hover: c(hot),
        line: c(text),
        fill_hover: with_alpha(hl, 0.3),
        fill_selected: with_alpha(hl, 0.5),
        selection: with_alpha(hl, 0.6),
    }
}

/// Q Calc's theme, or the contrast theme's colours while one is on.
pub fn apply(theme: Theme) -> Theme {
    if !active() {
        return theme;
    }
    // SAFETY: plain queries
    let sys = |i| rgb_of(unsafe { GetSysColor(i) });
    theme_from(
        sys(COLOR_WINDOW),
        sys(COLOR_WINDOWTEXT),
        sys(COLOR_GRAYTEXT),
        sys(COLOR_HIGHLIGHT),
        sys(COLOR_HOTLIGHT),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_colours() {
        assert_eq!(rgb_of(0x00ff_8040), 0x4080ff);
        // Night sky: black window, white text
        let night = theme_from(0x000000, 0xffffff, 0xa6a6a6, 0x8ee3f0, 0x75e9fc);
        assert!(night.dark);
        assert_eq!(night.bg.r, 0.0);
        assert_eq!(night.ink.r, 1.0);
        // Desert: light window, dark text
        let desert = theme_from(0xfffaef, 0x3d3d3d, 0x676767, 0x903909, 0x1c5e75);
        assert!(!desert.dark);
        assert!(luminance(0xffffff) > 0.99 && luminance(0) < 0.01);
    }
}
