//! The clipboard, as Unicode text (and a received file, as files: `set_files`).

use std::os::windows::ffi::OsStrExt;

use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;

/// Opens the clipboard, retrying briefly: another app may hold it for a moment.
fn open(owner: HWND) -> bool {
    for _ in 0..10 {
        // SAFETY: closed by the caller
        if unsafe { OpenClipboard(Some(owner)) }.is_ok() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    false
}

pub fn get(owner: HWND) -> Option<String> {
    if !open(owner) {
        return None;
    }
    // SAFETY: the clipboard is open; the global is locked only while read, then the clipboard is closed
    let out = unsafe {
        GetClipboardData(u32::from(CF_UNICODETEXT.0))
            .ok()
            .and_then(|h| {
                let g = HGLOBAL(h.0);
                let p = GlobalLock(g) as *const u16;
                if p.is_null() {
                    return None;
                }
                let mut len = 0;
                while *p.add(len) != 0 {
                    len += 1;
                }
                let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
                let _ = GlobalUnlock(g);
                Some(s)
            })
    };
    // SAFETY: opened above
    let _ = unsafe { CloseClipboard() };
    out
}

/// Files on the clipboard as Explorer's Copy puts them (CF_HDROP, and the shell's "Preferred DropEffect" saying
/// copy), so Ctrl+V pastes them in Explorer or any app that takes files.
pub fn set_files(owner: HWND, paths: &[std::path::PathBuf]) -> bool {
    use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
    use windows::Win32::UI::Shell::DROPFILES;
    const CF_HDROP: u32 = 15;
    if paths.is_empty() || !open(owner) {
        return false;
    }
    // the paths, each ending in a nul, then one more
    let mut wide: Vec<u16> = Vec::new();
    for p in paths {
        wide.extend(p.as_os_str().encode_wide());
        wide.push(0);
    }
    wide.push(0);
    let head = std::mem::size_of::<DROPFILES>();
    // SAFETY: the clipboard is open; each global is filled while locked and handed to the clipboard, which owns it
    let ok = unsafe {
        let _ = EmptyClipboard();
        let drop = GlobalAlloc(GMEM_MOVEABLE, head + wide.len() * 2)
            .ok()
            .and_then(|g| {
                let p = GlobalLock(g) as *mut u8;
                if p.is_null() {
                    return None;
                }
                let files = DROPFILES {
                    pFiles: head as u32,
                    fWide: true.into(),
                    ..Default::default()
                };
                std::ptr::copy_nonoverlapping((&files as *const DROPFILES).cast::<u8>(), p, head);
                std::ptr::copy_nonoverlapping(wide.as_ptr(), p.add(head).cast::<u16>(), wide.len());
                let _ = GlobalUnlock(g);
                SetClipboardData(CF_HDROP, Some(HANDLE(g.0))).ok()
            });
        // DROPEFFECT_COPY: a paste copies, never moves the saved file away
        let effect = RegisterClipboardFormatW(windows::core::w!("Preferred DropEffect"));
        if drop.is_some()
            && effect != 0
            && let Ok(g) = GlobalAlloc(GMEM_MOVEABLE, 4)
        {
            let p = GlobalLock(g) as *mut u32;
            if !p.is_null() {
                *p = 1;
                let _ = GlobalUnlock(g);
                let _ = SetClipboardData(effect, Some(HANDLE(g.0)));
            }
        }
        drop.is_some()
    };
    // SAFETY: opened above
    let _ = unsafe { CloseClipboard() };
    ok
}

pub fn set(owner: HWND, text: &str) -> bool {
    if !open(owner) {
        return false;
    }
    let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
    // SAFETY: the clipboard is open; the global is filled while locked and handed to the clipboard, which owns it
    let ok = unsafe {
        let _ = EmptyClipboard();
        match GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) {
            Ok(g) => {
                let p = GlobalLock(g) as *mut u16;
                if p.is_null() {
                    false
                } else {
                    std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
                    let _ = GlobalUnlock(g);
                    SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(HANDLE(g.0))).is_ok()
                }
            }
            Err(_) => false,
        }
    };
    // SAFETY: opened above
    let _ = unsafe { CloseClipboard() };
    ok
}
