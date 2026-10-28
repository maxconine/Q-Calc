//! Files dragged over the bar (the Mac's BarView `onDrop(isTargeted:)` and the thread's `dropHint`): while they're
//! over it, the hint under the bar says where a drop would send them (`drop to send to max`) or why it can't (no
//! name set, a calculation on the line, too many, a folder), and over an open thread its reply line says so for
//! that person. The drop itself goes where the hint said (share.rs `drop_files`, or the thread's person).
//!
//! WM_DROPFILES has no "over" moment, so the bar registers an OLE drop target (IDropTarget, a static object with a
//! hand-made vtable: the `windows` crate's `implement` isn't in this build). It's registered only once messaging is
//! in use (a name set, or a message / share line typed), so nobody else loads OLE; before that WM_DROPFILES still
//! takes drops, without the hint.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::path::PathBuf;

use qcalc_core::app::messaging as core;
use windows::Win32::Foundation::{HWND, POINT, POINTL};
use windows::Win32::System::Com::{DVASPECT_CONTENT, FORMATETC, IDataObject, TYMED_HGLOBAL};
use windows::Win32::System::Ole::{
    DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE, IDropTarget, OleInitialize, RegisterDragDrop,
    ReleaseStgMedium,
};
use windows::core::{GUID, HRESULT, IUnknown, Interface};

use crate::app::App;

thread_local! {
    static REGISTERED: Cell<bool> = const { Cell::new(false) };
    /// the drag under way: its files, and the hint the bar had before it
    static DRAG: RefCell<Option<Drag>> = const { RefCell::new(None) };
}

struct Drag {
    paths: Vec<PathBuf>,
    folders_only: bool,
    before: Option<String>,
}

/// `ensure`, soon, from code without the app at hand (a message or share line's hint).
pub fn ensure_soon() {
    if !REGISTERED.with(Cell::get) && !crate::later::armed("share-drop") {
        crate::later::after("share-drop", 1, |app| ensure(app));
    }
}

/// Registers the bar as a drop target, once (UI thread), when messaging is in use.
pub fn ensure(app: &App) {
    if REGISTERED.with(Cell::get) {
        return;
    }
    REGISTERED.with(|r| r.set(true));
    let hwnd = app.bar.hwnd;
    // SAFETY: OLE on the UI thread (an STA, never initialised otherwise); the target is a static that lives for the
    // process, and Revoke happens with the window
    unsafe {
        if OleInitialize(None).is_err() {
            crate::qlog!("share drop: no OLE");
            return;
        }
        let target = IDropTarget::from_raw(std::ptr::addr_of!(TARGET) as *mut c_void);
        let ok = RegisterDragDrop(hwnd, &target).is_ok();
        std::mem::forget(target);
        crate::qlog!("share drop: registered {ok}");
    }
}

#[repr(C)]
struct Vtbl {
    query: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    enter: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        u32,
        POINTL,
        *mut DROPEFFECT,
    ) -> HRESULT,
    over: unsafe extern "system" fn(*mut c_void, u32, POINTL, *mut DROPEFFECT) -> HRESULT,
    leave: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    drop: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        u32,
        POINTL,
        *mut DROPEFFECT,
    ) -> HRESULT,
}

#[repr(C)]
struct Target {
    vtbl: &'static Vtbl,
}

static VTBL: Vtbl = Vtbl {
    query,
    add_ref,
    release,
    enter,
    over,
    leave,
    drop: dropped,
};
static TARGET: Target = Target { vtbl: &VTBL };

const S_OK: HRESULT = HRESULT(0);
const E_NOINTERFACE: HRESULT = HRESULT(0x8000_4002_u32 as i32);

unsafe extern "system" fn query(
    this: *mut c_void,
    iid: *const GUID,
    out: *mut *mut c_void,
) -> HRESULT {
    // SAFETY: COM hands live pointers
    unsafe {
        if out.is_null() || iid.is_null() {
            return E_NOINTERFACE;
        }
        if *iid == IUnknown::IID || *iid == IDropTarget::IID {
            *out = this;
            S_OK
        } else {
            *out = std::ptr::null_mut();
            E_NOINTERFACE
        }
    }
}

// a static object: counting does nothing
unsafe extern "system" fn add_ref(_: *mut c_void) -> u32 {
    1
}
unsafe extern "system" fn release(_: *mut c_void) -> u32 {
    1
}

/// The files in a drag's data (CF_HDROP).
fn paths_of(data: *mut c_void) -> Vec<PathBuf> {
    const CF_HDROP: u16 = 15;
    // SAFETY: OLE's live data object for the call; the medium is released once read
    unsafe {
        let Some(obj) = IDataObject::from_raw_borrowed(&data) else {
            return Vec::new();
        };
        let fmt = FORMATETC {
            cfFormat: CF_HDROP,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        };
        let Ok(mut medium) = obj.GetData(&fmt) else {
            return Vec::new();
        };
        let h = medium.u.hGlobal;
        let out = crate::share::drop_paths(windows::Win32::UI::Shell::HDROP(h.0));
        ReleaseStgMedium(&mut medium);
        out
    }
}

fn to_bar(hwnd: HWND, scale: f32, pt: POINTL) -> (f32, f32) {
    let mut p = POINT { x: pt.x, y: pt.y };
    // SAFETY: our window, a live point
    let _ = unsafe { windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut p) };
    (p.x as f32 / scale, p.y as f32 / scale)
}

/// What the hint says for the drag at (x, y): the thread's words over an open thread, else the bar's (None: none).
fn update(app: &mut App, pt: POINTL) -> bool {
    let Some((count, folders_only)) = DRAG.with(|d| {
        d.borrow()
            .as_ref()
            .map(|d| (d.paths.len() as u32, d.folders_only))
    }) else {
        return false;
    };
    if count == 0 {
        return false;
    }
    let (x, y) = to_bar(app.bar.hwnd, app.bar.scale(), pt);
    let folder = folders_only.then(|| core::file_error_text("folder", ""));
    if crate::threads::over_thread(app, x, y)
        && let Some(name) = crate::threads::open_name()
    {
        let hint = folder.unwrap_or_else(|| crate::threads::drop_hint_for(&name, count));
        if crate::threads::set_drop_hint(Some(hint)) {
            app.model.touch();
        }
        restore_bar_hint(app);
        return true;
    }
    if crate::threads::set_drop_hint(None) {
        app.model.touch();
    }
    let hint = folder.or_else(|| {
        let h = bar_hint(app, count);
        (!h.is_empty()).then_some(h)
    });
    match hint {
        Some(h) => {
            DRAG.with(|d| {
                if let Some(d) = d.borrow_mut().as_mut()
                    && d.before.is_none()
                {
                    d.before = Some(app.model.hint.clone().unwrap_or_default());
                }
            });
            if app.model.hint.as_deref() != Some(h.as_str()) {
                app.model.hint = Some(h);
                app.model.touch();
            }
        }
        None => restore_bar_hint(app),
    }
    true
}

/// The bar's word for a drop with the line as it is (the core's dropTarget, with the relay's word on the name).
pub fn bar_hint(app: &App, count: u32) -> String {
    let (me, key) = crate::messaging::credentials();
    let m = core::Messaging {
        name: me,
        key,
        ..core::Messaging::default()
    };
    let line = app.model.field.text();
    let t = core::drop_target(line, &m, true, None, count);
    if !t.name.is_empty() && crate::messaging::found_name(&t.name) == Some(false) {
        return core::drop_target(line, &m, true, Some(false), count).hint;
    }
    t.hint
}

fn restore_bar_hint(app: &mut App) {
    let before = DRAG.with(|d| d.borrow_mut().as_mut().and_then(|d| d.before.take()));
    if let Some(b) = before {
        app.model.hint = (!b.is_empty()).then_some(b);
        app.model.touch();
    }
}

fn end(app: &mut App) {
    restore_bar_hint(app);
    if crate::threads::set_drop_hint(None) {
        app.model.touch();
    }
    DRAG.with(|d| d.borrow_mut().take());
}

unsafe extern "system" fn enter(
    _: *mut c_void,
    data: *mut c_void,
    _: u32,
    pt: POINTL,
    effect: *mut DROPEFFECT,
) -> HRESULT {
    let paths = paths_of(data);
    let folders_only = !paths.is_empty() && paths.iter().all(|p| p.is_dir());
    let any = !paths.is_empty();
    DRAG.with(|d| {
        *d.borrow_mut() = Some(Drag {
            paths,
            folders_only,
            before: None,
        })
    });
    crate::app::poke(|app| {
        update(app, pt);
    });
    // SAFETY: OLE's out pointer
    unsafe {
        if !effect.is_null() {
            *effect = if any {
                DROPEFFECT_COPY
            } else {
                DROPEFFECT_NONE
            };
        }
    }
    S_OK
}

unsafe extern "system" fn over(
    _: *mut c_void,
    _: u32,
    pt: POINTL,
    effect: *mut DROPEFFECT,
) -> HRESULT {
    let any = DRAG.with(|d| d.borrow().as_ref().is_some_and(|d| !d.paths.is_empty()));
    crate::app::poke(|app| {
        update(app, pt);
    });
    // SAFETY: OLE's out pointer
    unsafe {
        if !effect.is_null() {
            *effect = if any {
                DROPEFFECT_COPY
            } else {
                DROPEFFECT_NONE
            };
        }
    }
    S_OK
}

unsafe extern "system" fn leave(_: *mut c_void) -> HRESULT {
    crate::app::poke(end);
    S_OK
}

unsafe extern "system" fn dropped(
    _: *mut c_void,
    data: *mut c_void,
    _: u32,
    pt: POINTL,
    effect: *mut DROPEFFECT,
) -> HRESULT {
    let paths = paths_of(data);
    crate::app::poke(move |app| {
        let (x, y) = to_bar(app.bar.hwnd, app.bar.scale(), pt);
        let thread = crate::threads::over_thread(app, x, y)
            .then(crate::threads::open_name)
            .flatten();
        end(app);
        match thread {
            Some(name) => crate::share::send_files(app, paths, &name),
            None => {
                crate::share::drop_files(app, paths);
            }
        }
    });
    // SAFETY: OLE's out pointer
    unsafe {
        if !effect.is_null() {
            *effect = DROPEFFECT_COPY;
        }
    }
    S_OK
}

/// The e2e suite: a drag of `paths` over bar point (x, y) DIPs, as OLE would call it (enter, over), its hint back;
/// `drop` finishes it, else it leaves.
pub fn simulate(
    app: &mut App,
    paths: Vec<PathBuf>,
    x: f32,
    y: f32,
    drop: bool,
) -> serde_json::Value {
    let folders_only = !paths.is_empty() && paths.iter().all(|p| p.is_dir());
    DRAG.with(|d| {
        *d.borrow_mut() = Some(Drag {
            paths: paths.clone(),
            folders_only,
            before: None,
        })
    });
    let s = app.bar.scale();
    let mut p = POINT {
        x: (x * s) as i32,
        y: (y * s) as i32,
    };
    // SAFETY: our window, a live point
    let _ = unsafe { windows::Win32::Graphics::Gdi::ClientToScreen(app.bar.hwnd, &mut p) };
    update(app, POINTL { x: p.x, y: p.y });
    let out = serde_json::json!({ "bar": app.model.hint, "thread": crate::threads::drop_hint() });
    let thread = crate::threads::over_thread(app, x, y)
        .then(crate::threads::open_name)
        .flatten();
    end(app);
    if drop {
        match thread {
            Some(name) => crate::share::send_files(app, paths, &name),
            None => {
                crate::share::drop_files(app, paths);
            }
        }
    }
    out
}
