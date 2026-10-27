//! The bar's WARP device, made ahead on a thread of its own, and WARP's DLL kept loaded across hides.
//!
//! The bar draws through a flip swap chain on a WARP device (render.rs), which goes when the bar hides (app.rs
//! `after_hide`; ~2.5 MB). Making one costs ~11 ms the first time in a process and ~3 ms after, on the UI thread at
//! the first paint, which is most of a launch's time to its first frame and a third of a showing. So a launch and a
//! showing start making it at once on another thread (`prefetch`), while the UI thread makes its windows and
//! lays the bar out, and the first paint takes it (`take`). D3D11CreateDevice and the DXGI lookups may run on any
//! thread; the device is single-threaded, so only one thread at a time uses it: the prefetch thread until it hands
//! it over, then the UI thread.
//!
//! With the last device D3D10Warp.dll unloads, and loading it again was ~1 ms of a showing: a reference of our own
//! (`pin`) keeps the image mapped (shared, not private memory).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Once};
use std::thread::JoinHandle;

use windows::Win32::Foundation::{E_FAIL, HMODULE};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_WARP;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_PREVENT_INTERNAL_THREADING_OPTIMIZATIONS,
    D3D11_CREATE_DEVICE_SINGLETHREADED, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
};
use windows::Win32::Graphics::Dxgi::{IDXGIDevice, IDXGIFactory2};
use windows::Win32::System::LibraryLoader::LoadLibraryW;
use windows::core::{Interface, Result, w};

/// A device and its DXGI factory, on their way from the prefetch thread to the UI thread.
struct Made(ID3D11Device, IDXGIFactory2);
// SAFETY: the device is used by one thread at a time: the prefetch thread makes it and lets go of it before the UI
// thread joins it (`take`); DXGI factories are free-threaded
unsafe impl Send for Made {}

/// The prefetch under way (or done and not taken).
static PENDING: Mutex<Option<JoinHandle<Option<Made>>>> = Mutex::new(None);
/// The UI thread holds a device (made or taken), so a prefetch would only duplicate it.
static HELD: AtomicBool = AtomicBool::new(false);

/// A WARP device for the bar, as render.rs makes it.
fn create() -> Result<Made> {
    let mut d3d = None;
    // SAFETY: plain device creation into a live out pointer; the casts and lookups are on live objects
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_WARP,
            HMODULE::default(),
            // Direct2D needs BGRA; only the UI thread draws
            D3D11_CREATE_DEVICE_BGRA_SUPPORT
                | D3D11_CREATE_DEVICE_SINGLETHREADED
                | D3D11_CREATE_DEVICE_PREVENT_INTERNAL_THREADING_OPTIMIZATIONS,
            None,
            D3D11_SDK_VERSION,
            Some(&mut d3d),
            None,
            None,
        )?;
        let d3d: ID3D11Device = d3d.ok_or_else(|| windows::core::Error::from(E_FAIL))?;
        let dxgi: IDXGIDevice = d3d.cast()?;
        let factory: IDXGIFactory2 = dxgi.GetAdapter()?.GetParent()?;
        Ok(Made(d3d, factory))
    }
}

/// Starts making the bar's device on a thread of its own, unless the UI thread holds one or one is on its way.
/// On the UI thread: at launch and as a hidden bar starts to show.
pub fn prefetch() {
    if HELD.load(Ordering::Relaxed) {
        return;
    }
    let Ok(mut pending) = PENDING.lock() else {
        return;
    };
    if pending.is_some() {
        return;
    }
    *pending = std::thread::Builder::new()
        .name("qcalc-device".into())
        .spawn(|| create().ok())
        .ok();
}

/// The prefetched device and its factory, waiting for it if it isn't made yet; None when there's none (render.rs
/// then makes one itself). The UI thread holds it from here on.
pub fn take() -> Option<(ID3D11Device, IDXGIFactory2)> {
    let handle = PENDING.lock().ok()?.take()?;
    let made = handle.join().ok().flatten()?;
    held();
    Some((made.0, made.1))
}

/// The UI thread made or took a device.
pub fn held() {
    HELD.store(true, Ordering::Relaxed);
    pin();
}

/// The UI thread let its device go (a hide, a lost device): a prefetch may make the next; one made and never taken
/// goes too.
pub fn dropped() {
    HELD.store(false, Ordering::Relaxed);
    let handle = PENDING.lock().ok().and_then(|mut p| p.take());
    if let Some(h) = handle {
        drop(h.join());
    }
}

/// After the first WARP device: one reference to its DLL, never released.
fn pin() {
    static PINNED: Once = Once::new();
    PINNED.call_once(|| {
        // SAFETY: loads a system DLL that is already loaded (the device just made uses it); the handle is kept
        let _ = unsafe { LoadLibraryW(w!("d3d10warp.dll")) };
    });
}
