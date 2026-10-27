//! The notification-area icon: Show Q Calc / Settings… / Quit Q Calc, a click shows the bar, and the balloon
//! notices (the hotkey held after retries ran out).
//!
//! The icon is src-tauri/icons/icon.ico, built into the binary: a black glyph, made white for a dark taskbar
//! (`SystemUsesLightTheme`), redone when the taskbar's colour changes (WM_SETTINGCHANGE "ImmersiveColorSet") and
//! re-added when Explorer restarts (the registered "TaskbarCreated" message; app.rs calls `add` again).

use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
    DeleteDC, DeleteObject, GetDIBits, HBITMAP, HGDIOBJ,
};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NIM_SETVERSION, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconFromResourceEx, CreateIconIndirect, CreatePopupMenu, DestroyIcon,
    DestroyMenu, GetCursorPos, GetIconInfo, HICON, ICONINFO, LR_DEFAULTCOLOR, MF_SEPARATOR,
    MF_STRING, SM_CXSMICON, SetForegroundWindow, TPM_BOTTOMALIGN, TPM_NONOTIFY, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, TrackPopupMenu,
};
use windows::core::HSTRING;

pub const ICO: &[u8] = include_bytes!("../../../src-tauri/icons/icon.ico");
const UID: u32 = 1;

/// Menu commands.
pub const CMD_SHOW: usize = 1;
pub const CMD_SETTINGS: usize = 2;
pub const CMD_QUIT: usize = 3;

/// The PNG of the ICO entry nearest `px` (at least it, if any is).
pub fn ico_entry(ico: &[u8], px: u32) -> Option<&[u8]> {
    let n = u16::from_le_bytes([*ico.get(4)?, *ico.get(5)?]) as usize;
    let mut best: Option<(u32, &[u8])> = None;
    for i in 0..n {
        let e = ico.get(6 + 16 * i..22 + 16 * i)?;
        let w = if e[0] == 0 { 256 } else { u32::from(e[0]) };
        let size = u32::from_le_bytes([e[8], e[9], e[10], e[11]]) as usize;
        let off = u32::from_le_bytes([e[12], e[13], e[14], e[15]]) as usize;
        let data = ico.get(off..off + size)?;
        let better = match best {
            None => true,
            // the smallest that is at least px, else the largest
            Some((b, _)) => (w >= px && (b < px || w < b)) || (b < px && w > b),
        };
        if better {
            best = Some((w, data));
        }
    }
    best.map(|(_, d)| d)
}

/// The app icon at `px`, white when `white`.
pub fn icon(px: u32, white: bool) -> Option<HICON> {
    let data = ico_entry(ICO, px)?;
    // SAFETY: data is a PNG icon image the call copies
    let icon = unsafe {
        CreateIconFromResourceEx(
            data,
            true,
            0x0003_0000,
            px as i32,
            px as i32,
            LR_DEFAULTCOLOR,
        )
    }
    .ok()?;
    if !white {
        return Some(icon);
    }
    Some(whiten(icon).unwrap_or(icon))
}

/// The same glyph in white, its edges' alpha kept.
fn whiten(icon: HICON) -> Option<HICON> {
    // SAFETY: the GDI objects made here are deleted before returning; buffers are sized from the header
    unsafe {
        let mut info = ICONINFO::default();
        GetIconInfo(icon, &mut info).ok()?;
        let dc = CreateCompatibleDC(None);
        let mut bmi = BITMAPINFO::default();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        // first call: the size
        GetDIBits(dc, info.hbmColor, 0, 0, None, &mut bmi, DIB_RGB_COLORS);
        let (w, h) = (bmi.bmiHeader.biWidth, bmi.bmiHeader.biHeight.abs());
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB.0;
        bmi.bmiHeader.biHeight = -h;
        let mut px = vec![0u8; (w * h * 4) as usize];
        let got = GetDIBits(
            dc,
            info.hbmColor,
            0,
            h as u32,
            Some(px.as_mut_ptr().cast()),
            &mut bmi,
            DIB_RGB_COLORS,
        );
        let mut out = None;
        if got == h {
            for p in px.as_chunks_mut::<4>().0 {
                p[0] = 255 - p[0];
                p[1] = 255 - p[1];
                p[2] = 255 - p[2];
            }
            let mut bits = std::ptr::null_mut();
            if let Ok(color) = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
                && !bits.is_null()
            {
                std::ptr::copy_nonoverlapping(px.as_ptr(), bits.cast::<u8>(), px.len());
                let made = ICONINFO {
                    fIcon: true.into(),
                    hbmColor: color,
                    hbmMask: info.hbmMask,
                    ..Default::default()
                };
                out = CreateIconIndirect(&made).ok();
                let _ = DeleteObject(HGDIOBJ(color.0));
            }
        }
        let _ = DeleteDC(dc);
        let del = |b: HBITMAP| {
            if !b.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(b.0));
            }
        };
        del(info.hbmColor);
        del(info.hbmMask);
        if out.is_some() {
            let _ = DestroyIcon(icon);
        }
        out
    }
}

/// Whether the taskbar is dark (`SystemUsesLightTheme` 0, or missing on old Windows 10).
pub fn taskbar_dark() -> bool {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
    let mut light: u32 = 1;
    let mut size = 4u32;
    // SAFETY: live out pointers of the right size
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            &HSTRING::from("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut light as *mut u32).cast()),
            Some(&mut size),
        )
    };
    status.is_err() || light == 0
}

fn copy_into(dst: &mut [u16], s: &str) {
    let wide: Vec<u16> = s.encode_utf16().take(dst.len() - 1).collect();
    dst[..wide.len()].copy_from_slice(&wide);
    dst[wide.len()] = 0;
}

pub struct Tray {
    hwnd: HWND,
    msg: u32,
    icon: Option<HICON>,
    pub added: bool,
}

impl Tray {
    pub fn new(hwnd: HWND, msg: u32) -> Tray {
        Tray {
            hwnd,
            msg,
            icon: None,
            added: false,
        }
    }

    fn data(&self) -> NOTIFYICONDATAW {
        NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: UID,
            ..Default::default()
        }
    }

    fn make_icon(&mut self) -> HICON {
        // SAFETY: plain query
        let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(self.hwnd) }.max(96);
        // SAFETY: plain query
        let px = unsafe { windows::Win32::UI::HiDpi::GetSystemMetricsForDpi(SM_CXSMICON, dpi) }
            .max(16) as u32;
        if let Some(old) = self.icon.take() {
            // SAFETY: an icon we made
            let _ = unsafe { DestroyIcon(old) };
        }
        let icon = icon(px, taskbar_dark()).unwrap_or_default();
        self.icon = Some(icon);
        icon
    }

    /// Adds the icon (at launch, and again after Explorer restarts).
    pub fn add(&mut self) -> bool {
        let mut d = self.data();
        d.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        d.uCallbackMessage = self.msg;
        d.hIcon = self.make_icon();
        copy_into(&mut d.szTip, "Q Calc");
        // SAFETY: d is a live, sized struct
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &d);
            self.added = Shell_NotifyIconW(NIM_ADD, &d).as_bool();
            d.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            let _ = Shell_NotifyIconW(NIM_SETVERSION, &d);
        }
        self.added
    }

    /// The taskbar changed colour.
    pub fn refresh_icon(&mut self) {
        let mut d = self.data();
        d.uFlags = NIF_ICON;
        d.hIcon = self.make_icon();
        // SAFETY: as above
        let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &d) };
    }

    /// A balloon (a toast on Windows 10/11).
    pub fn notify(&self, title: &str, text: &str) {
        let mut d = self.data();
        d.uFlags = NIF_INFO;
        d.dwInfoFlags = NIIF_INFO;
        copy_into(&mut d.szInfoTitle, title);
        copy_into(&mut d.szInfo, text);
        // SAFETY: as above
        let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &d) };
    }

    pub fn remove(&mut self) {
        let d = self.data();
        // SAFETY: as above
        let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &d) };
        self.added = false;
    }

    /// The menu at the pointer; the command picked, if any. Modal: call it outside the app's borrow.
    pub fn menu(hwnd: HWND) -> Option<usize> {
        // SAFETY: the menu is destroyed below; strings are static
        unsafe {
            let menu = CreatePopupMenu().ok()?;
            let _ = AppendMenuW(menu, MF_STRING, CMD_SHOW, windows::core::w!("Show Q Calc"));
            let _ = AppendMenuW(
                menu,
                MF_STRING,
                CMD_SETTINGS,
                windows::core::w!("Settings…"),
            );
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
            let _ = AppendMenuW(menu, MF_STRING, CMD_QUIT, windows::core::w!("Quit Q Calc"));
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            // the menu closes when clicked away only if its owner is in front
            let _ = SetForegroundWindow(hwnd);
            let cmd = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
                pt.x,
                pt.y,
                None,
                hwnd,
                None,
            );
            let _ = DestroyMenu(menu);
            (cmd.0 > 0).then_some(cmd.0 as usize)
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        if self.added {
            self.remove();
        }
        if let Some(icon) = self.icon.take() {
            // SAFETY: an icon we made
            let _ = unsafe { DestroyIcon(icon) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ico_entry_nearest_the_size() {
        let png = |d: &[u8]| d.starts_with(b"\x89PNG");
        for px in [16, 20, 24, 32, 40, 48, 64, 300] {
            assert!(ico_entry(ICO, px).is_some_and(png), "{px}");
        }
        // 20 px takes the 24 entry, not the 16
        assert_eq!(
            ico_entry(ICO, 20).map(<[u8]>::len),
            ico_entry(ICO, 24).map(<[u8]>::len)
        );
    }
}
