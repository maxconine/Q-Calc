//! Keeps WARP's DLL loaded across hides. The bar's WARP device goes when it hides (app.rs `after_hide`), and with
//! the last device D3D10Warp.dll unloads; loading it again was most of the ~3 ms a showing spends making a device.
//! A reference of our own keeps the image mapped (shared, not private memory), so a showing only makes the device.

use std::sync::Once;

use windows::Win32::System::LibraryLoader::LoadLibraryW;
use windows::core::w;

/// After the first WARP device: one reference to its DLL, never released.
pub fn pin() {
    static PINNED: Once = Once::new();
    PINNED.call_once(|| {
        // SAFETY: loads a system DLL that is already loaded (the device just made uses it); the handle is kept
        let _ = unsafe { LoadLibraryW(w!("d3d10warp.dll")) };
    });
}
