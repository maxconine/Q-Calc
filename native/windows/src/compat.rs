//! Every Windows 10 (1507, build 10240, onward) and Windows 11: the functions newer than 1507 are looked up at run
//! time, once, instead of imported, so the exe starts on any Windows 10 build (an import a build lacks stops the
//! process before `main` with "entry point not found"). What each falls back to:
//!
//! - `GetDpiForWindow` (1607): the DPI of the window's monitor (`GetDpiForMonitor`, 8.1), else the system DPI.
//! - `GetSystemMetricsForDpi` (1607): `GetSystemMetrics` scaled from the system DPI.
//! - `AdjustWindowRectExForDpi` (1607): `AdjustWindowRectEx`. Before 1703 the manifest's PerMonitorV2 isn't known,
//!   so the process is per-monitor v1 (`true/pm`) and Windows draws the title bar and borders at the system DPI:
//!   the unscaled frame is the right one there.
//! - `UiaRaiseNotificationEvent` (1709): skipped (a reader still hears the focused element's own changes).
//! - `SetProcessDpiAwarenessContext` (1703) isn't called at all: the embedded manifest declares the awareness.
//!
//! `build()` is the OS build (RtlGetVersion), for features that differ by build: DWM's dark border attribute (19
//! on 1809-1909, 20 from 20H1), the dark menus' uxtheme ordinals (1809+). DWM's rounded corners (Windows 11) are
//! asked for everywhere and ignored where unknown.
//!
//! **Testing the fallbacks here.** `QCALC_COMPAT_SIMULATE=<1507|1511|1607|1703|1709|1803|1809|1903|20H1|build>`
//! makes this module report an older build and the functions that build lacks as missing, so the fallbacks run on
//! a current machine (`tests/e2e/smoke.ps1` runs with it). It only takes features away, so a release build reads
//! it too.

use std::sync::OnceLock;

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    GetDC, GetDeviceCaps, LOGPIXELSX, MONITOR_DEFAULTTONEAREST, MonitorFromWindow, ReleaseDC,
};
use windows::Win32::System::LibraryLoader::{
    GetModuleHandleW, GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, GetSystemMetrics, SYSTEM_METRICS_INDEX, WINDOW_EX_STYLE, WINDOW_STYLE,
};
use windows::core::{BOOL, HRESULT, PCSTR, PCWSTR, s, w};

/// Windows 10 1507, the first build.
pub const RTM: u32 = 10240;
/// The builds the looked-up functions arrived in.
pub const B1607: u32 = 14393;
pub const B1703: u32 = 15063;
pub const B1709: u32 = 16299;
pub const B1809: u32 = 17763;
/// 20H1: DWMWA_USE_IMMERSIVE_DARK_MODE's documented value (20); 1809-1909 took 19
pub const B20H1: u32 = 19041;
/// Windows 11
pub const B21H2_11: u32 = 22000;

/// A release name (or a build number) to its build.
pub fn parse_build(name: &str) -> Option<u32> {
    let n = name.trim().to_ascii_uppercase();
    let b = match n.as_str() {
        "1507" | "RTM" => RTM,
        "1511" => 10586,
        "1607" => B1607,
        "1703" => B1703,
        "1709" => B1709,
        "1803" => 17134,
        "1809" => B1809,
        "1903" => 18362,
        "1909" => 18363,
        "2004" | "20H1" => B20H1,
        "11" => B21H2_11,
        _ => n.parse().ok().filter(|b| (RTM..1_000_000).contains(b))?,
    };
    Some(b)
}

/// The `DWMWA_USE_IMMERSIVE_DARK_MODE` attribute for a build: 20 from 20H1, 19 on 1809-1909, none before.
pub fn dark_mode_attribute(build: u32) -> Option<u32> {
    if build >= B20H1 {
        Some(20)
    } else if build >= B1809 {
        Some(19)
    } else {
        None
    }
}

/// The dark border attribute for this build, for `DwmSetWindowAttribute` (before 1809 there is none, and DWM
/// refuses the call harmlessly).
pub fn dark_attr() -> windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE {
    windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE(
        dark_mode_attribute(build()).unwrap_or(20) as i32
    )
}

type GetDpiForWindowFn = unsafe extern "system" fn(HWND) -> u32;
type GetSystemMetricsForDpiFn = unsafe extern "system" fn(i32, u32) -> i32;
type AdjustWindowRectExForDpiFn = unsafe extern "system" fn(*mut RECT, u32, BOOL, u32, u32) -> BOOL;
type UiaRaiseNotificationEventFn =
    unsafe extern "system" fn(*mut core::ffi::c_void, i32, i32, *const u16, *const u16) -> HRESULT;

struct Os {
    build: u32,
    simulated: bool,
    dpi_for_window: Option<GetDpiForWindowFn>,
    metrics_for_dpi: Option<GetSystemMetricsForDpiFn>,
    adjust_for_dpi: Option<AdjustWindowRectExForDpiFn>,
}

static OS: OnceLock<Os> = OnceLock::new();
static UIA: OnceLock<Option<UiaRaiseNotificationEventFn>> = OnceLock::new();

/// A function by name from an already loaded system module, if it has it.
fn lookup(module: PCWSTR, name: PCSTR) -> Option<unsafe extern "system" fn() -> isize> {
    // SAFETY: plain lookups; user32 and ntdll are loaded in every GUI process
    unsafe { GetProcAddress(GetModuleHandleW(module).ok()?, name) }
}

fn real_build() -> u32 {
    #[repr(C)]
    struct OsVersionInfoW {
        size: u32,
        major: u32,
        minor: u32,
        build: u32,
        platform: u32,
        csd: [u16; 128],
    }
    let Some(f) = lookup(w!("ntdll.dll"), s!("RtlGetVersion")) else {
        return RTM;
    };
    // SAFETY: RtlGetVersion(PRTL_OSVERSIONINFOW) -> NTSTATUS, into a sized local
    unsafe {
        let f: unsafe extern "system" fn(*mut OsVersionInfoW) -> i32 = std::mem::transmute(f);
        let mut v: OsVersionInfoW = std::mem::zeroed();
        v.size = std::mem::size_of::<OsVersionInfoW>() as u32;
        if f(&mut v) == 0 && v.major >= 10 {
            v.build
        } else {
            RTM
        }
    }
}

fn os() -> &'static Os {
    OS.get_or_init(|| {
        let sim = std::env::var("QCALC_COMPAT_SIMULATE")
            .ok()
            .and_then(|v| parse_build(&v));
        let build = sim.map_or_else(real_build, |s| s.min(real_build()));
        let has = |since: u32| build >= since;
        let user32 = w!("user32.dll");
        // SAFETY: each pointer is the named export, transmuted to its documented signature
        unsafe {
            Os {
                build,
                simulated: sim.is_some(),
                dpi_for_window: has(B1607)
                    .then(|| lookup(user32, s!("GetDpiForWindow")))
                    .flatten()
                    .map(|f| std::mem::transmute::<_, GetDpiForWindowFn>(f)),
                metrics_for_dpi: has(B1607)
                    .then(|| lookup(user32, s!("GetSystemMetricsForDpi")))
                    .flatten()
                    .map(|f| std::mem::transmute::<_, GetSystemMetricsForDpiFn>(f)),
                adjust_for_dpi: has(B1607)
                    .then(|| lookup(user32, s!("AdjustWindowRectExForDpi")))
                    .flatten()
                    .map(|f| std::mem::transmute::<_, AdjustWindowRectExForDpiFn>(f)),
            }
        }
    })
}

/// The OS build (an older one under `QCALC_COMPAT_SIMULATE`).
pub fn build() -> u32 {
    os().build
}

/// Whether `QCALC_COMPAT_SIMULATE` took effect (logged at launch).
pub fn simulated() -> bool {
    os().simulated
}

/// What launch logs: the build and which functions fell back.
pub fn describe() -> String {
    let o = os();
    let missing: Vec<&str> = [
        (o.dpi_for_window.is_none(), "GetDpiForWindow"),
        (o.metrics_for_dpi.is_none(), "GetSystemMetricsForDpi"),
        (o.adjust_for_dpi.is_none(), "AdjustWindowRectExForDpi"),
        (o.build < B1709, "UiaRaiseNotificationEvent (skipped)"),
        (o.build < B1809, "the dark border and menus (none)"),
    ]
    .into_iter()
    .filter_map(|(m, n)| m.then_some(n))
    .collect();
    format!(
        "build {}{}, {}",
        o.build,
        if o.simulated { " (simulated)" } else { "" },
        if missing.is_empty() {
            "nothing missing".to_string()
        } else {
            format!("without {}", missing.join(", "))
        }
    )
}

/// The screen's DPI the process started with (the DPI Windows scales unaware parts by).
pub fn system_dpi() -> u32 {
    // SAFETY: the screen DC, released at once
    unsafe {
        let dc = GetDC(None);
        let dpi = GetDeviceCaps(Some(dc), LOGPIXELSX);
        ReleaseDC(None, dc);
        u32::try_from(dpi).unwrap_or(96).max(96)
    }
}

/// `GetDpiForWindow`, or the DPI of the window's monitor.
pub fn dpi_for_window(hwnd: HWND) -> u32 {
    if let Some(f) = os().dpi_for_window {
        // SAFETY: the export, on a window handle (0 back for a bad one)
        return unsafe { f(hwnd) };
    }
    let (mut x, mut y) = (0u32, 0u32);
    // SAFETY: plain queries with live out values
    unsafe {
        let m = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut x, &mut y).is_ok() && x > 0 {
            return x;
        }
    }
    system_dpi()
}

/// `GetSystemMetricsForDpi`, or `GetSystemMetrics` scaled from the system DPI.
pub fn system_metrics_for_dpi(index: SYSTEM_METRICS_INDEX, dpi: u32) -> i32 {
    if let Some(f) = os().metrics_for_dpi {
        // SAFETY: the export, plain values
        return unsafe { f(index.0, dpi) };
    }
    // SAFETY: plain query
    let v = unsafe { GetSystemMetrics(index) };
    scale(v, dpi, system_dpi())
}

/// `v` at `from` DPI, at `to` DPI (rounded as Windows does).
pub fn scale(v: i32, to: u32, from: u32) -> i32 {
    let from = from.max(1) as i64;
    ((i64::from(v) * i64::from(to) + from / 2) / from) as i32
}

/// `AdjustWindowRectExForDpi`, or `AdjustWindowRectEx` (see the module notes).
pub fn adjust_window_rect_for_dpi(
    r: &mut RECT,
    style: WINDOW_STYLE,
    menu: bool,
    ex: WINDOW_EX_STYLE,
    dpi: u32,
) -> bool {
    if let Some(f) = os().adjust_for_dpi {
        // SAFETY: the export, on a live rect
        return unsafe { f(r, style.0, BOOL::from(menu), ex.0, dpi) }.as_bool();
    }
    // SAFETY: a live rect
    unsafe { AdjustWindowRectEx(r, style, menu, ex) }.is_ok()
}

/// `UiaRaiseNotificationEvent` (1709+), looked up the first time a reader is listening; None where missing.
pub fn uia_notification() -> Option<UiaRaiseNotificationEventFn> {
    *UIA.get_or_init(|| {
        if build() < B1709 {
            return None;
        }
        let name = w!("uiautomationcore.dll");
        // SAFETY: a system DLL by name from System32; the pointer is the named export with its signature
        unsafe {
            let module = GetModuleHandleW(name)
                .or_else(|_| LoadLibraryExW(name, None, LOAD_LIBRARY_SEARCH_SYSTEM32))
                .ok()?;
            GetProcAddress(module, s!("UiaRaiseNotificationEvent"))
                .map(|f| std::mem::transmute::<_, UiaRaiseNotificationEventFn>(f))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_names_are_builds() {
        assert_eq!(parse_build("1507"), Some(10240));
        assert_eq!(parse_build("1607"), Some(14393));
        assert_eq!(parse_build("20h1"), Some(19041));
        assert_eq!(parse_build("17763"), Some(17763));
        assert_eq!(parse_build(" 1809 "), Some(17763));
        // a number below Windows 10 isn't a Windows 10 build
        assert_eq!(parse_build("7601"), None);
        assert_eq!(parse_build("yes"), None);
        assert_eq!(parse_build(""), None);
    }

    #[test]
    fn the_dark_border_attribute_follows_the_build() {
        assert_eq!(dark_mode_attribute(RTM), None);
        assert_eq!(dark_mode_attribute(17134), None);
        assert_eq!(dark_mode_attribute(B1809), Some(19));
        assert_eq!(dark_mode_attribute(18363), Some(19));
        assert_eq!(dark_mode_attribute(B20H1), Some(20));
        assert_eq!(dark_mode_attribute(26200), Some(20));
    }

    #[test]
    fn metrics_scale_from_the_system_dpi() {
        assert_eq!(scale(16, 96, 96), 16);
        assert_eq!(scale(16, 144, 96), 24);
        assert_eq!(scale(16, 120, 96), 20);
        assert_eq!(scale(32, 96, 192), 16);
        assert_eq!(scale(5, 96, 0), 480);
    }

    #[test]
    fn this_machine_answers() {
        assert!(build() >= RTM);
        assert!(system_dpi() >= 96);
    }
}
