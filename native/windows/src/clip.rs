//! The clipboard, as Unicode text.

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
