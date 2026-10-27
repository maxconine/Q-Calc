//! The bar: a borderless, topmost tool window (no taskbar button) with DWM's rounded corners and shadow, made
//! once at launch, hidden rather than destroyed, so showing it is a `SetWindowPos` and one paint.
//!
//! - `Bar::create`: the window (class `QCalcBar`, its wndproc is app.rs's `bar_proc`).
//! - `Bar::layout`: where everything goes for the model as it is (`BarLayout`, DIPs). The height follows the tape
//!   and the hint; the composer (the line and its answer) stays put on screen while the tape grows above it, as
//!   place.rs's `resized` keeps it.
//! - `Bar::paint`: one frame (tape, composer, hint) through render.rs.
//! - `Bar::placement`: where a showing opens: the monitor under the pointer, the spot remembered for it
//!   (store.positions, logical, expiring after `spotSeconds`), else centred 28% down the work area (place.rs).
//! - show / hide are app.rs's ops (`Op::Show`, `Op::Hide`), run outside the app's borrow because
//!   SetWindowPos and SetForegroundWindow send messages back synchronously. Showing cloaks the window, moves and
//!   shows it, paints, then uncloaks, so the old frame never flashes.
//!
//! Panels (graph.rs, systems.rs, typst_row.rs, messaging.rs) add their height to `BarLayout` below the
//! composer and paint into their rect; see README "adding a panel".

use std::ops::Range;

use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Dwm::{
    DWMWA_CLOAK, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFOEXW, MonitorFromPoint,
};
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_DBLCLKS, CreateWindowExW, GetCursorPos, IDC_IBEAM, LoadCursorW, RegisterClassExW,
    WINDOW_EX_STYLE, WNDCLASSEXW, WNDPROC, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{BOOL, Result, w};

use crate::input::FieldView;
use crate::model::BarModel;
use crate::place::{self, Area};
use crate::render::{Canvas, Gfx, Rect, Surface, Theme, size, with_alpha};
use crate::{answer, history};

/// Where things are, DIPs from the window's top-left.
#[derive(Clone, Debug, Default)]
pub struct BarLayout {
    pub width: f32,
    pub height: f32,
    /// the composer's top: what stays put on screen
    pub anchor: f32,
    pub tape_rows: Range<usize>,
    pub composer: Rect,
    pub field: Rect,
    pub answer: Rect,
    pub answer_text: String,
    pub hint: Option<Rect>,
    /// the `sys` rows' panel (systems.rs)
    pub systems: Option<Rect>,
    /// the graph panel (graph.rs)
    pub graph: Option<Rect>,
}

/// A monitor: its store key (`\\.\DISPLAY1`, as Tauri named it), work area (physical px) and scale.
#[derive(Clone, Debug)]
pub struct Monitor {
    pub key: String,
    pub area: Area,
    pub dpi: u32,
}

impl Monitor {
    pub fn scale(&self) -> f64 {
        f64::from(self.dpi) / 96.0
    }
}

pub fn monitor_at(pt: POINT) -> Monitor {
    // SAFETY: plain queries with live out structs
    unsafe {
        let hm: HMONITOR = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        let _ = GetMonitorInfoW(hm, &mut info.monitorInfo);
        let (mut dx, mut dy) = (96u32, 96u32);
        let _ = GetDpiForMonitor(hm, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        let len = info.szDevice.iter().position(|&c| c == 0).unwrap_or(0);
        let w = info.monitorInfo.rcWork;
        Monitor {
            key: String::from_utf16_lossy(&info.szDevice[..len]),
            area: Area {
                x: f64::from(w.left),
                y: f64::from(w.top),
                width: f64::from(w.right - w.left),
                height: f64::from(w.bottom - w.top),
            },
            dpi: dx.max(96),
        }
    }
}

pub fn cursor_monitor() -> Monitor {
    let mut pt = POINT::default();
    // SAFETY: a live out point
    let _ = unsafe { GetCursorPos(&mut pt) };
    monitor_at(pt)
}

/// The bar window and how it is drawn.
pub struct Bar {
    pub hwnd: HWND,
    pub surface: Surface,
    pub field_view: FieldView,
    pub layout: BarLayout,
    /// the bar's width in DIPs (store.width, or the standard 680)
    pub width: f32,
    pub visible: bool,
    pub focused: bool,
    /// physical px: the composer's top-left on screen while shown
    pub composer_at: (i32, i32),
    pub monitor: Option<Monitor>,
    /// a drag-select in the field is under way
    pub selecting: bool,
}

pub const CLASS: windows::core::PCWSTR = w!("QCalcBar");

impl Bar {
    pub fn create(proc_: WNDPROC) -> Result<Bar> {
        // SAFETY: registering a class and creating a window with static strings and our own wndproc
        let hwnd = unsafe {
            let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)?;
            let class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_DBLCLKS,
                lpfnWndProc: proc_,
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_IBEAM).unwrap_or_default(),
                lpszClassName: CLASS,
                ..Default::default()
            };
            RegisterClassExW(&class);
            CreateWindowExW(
                WINDOW_EX_STYLE(WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0),
                CLASS,
                w!("Q Calc"),
                WS_POPUP,
                0,
                0,
                680,
                72,
                None,
                None,
                Some(instance.into()),
                None,
            )?
        };
        // SAFETY: attributes on our own window, with pointers to live locals
        unsafe {
            let round = DWMWCP_ROUND;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                (&round as *const windows::Win32::Graphics::Dwm::DWM_WINDOW_CORNER_PREFERENCE)
                    .cast(),
                std::mem::size_of_val(&round) as u32,
            );
            // a 1 px frame inset gives a borderless window DWM's shadow
            let margins = MARGINS {
                cxLeftWidth: 0,
                cxRightWidth: 0,
                cyTopHeight: 0,
                cyBottomHeight: 1,
            };
            let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
        }
        // SAFETY: a plain query on our window
        let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) }.max(96);
        Ok(Bar {
            hwnd,
            surface: Surface::new(hwnd, dpi),
            field_view: FieldView::default(),
            layout: BarLayout::default(),
            width: place::MIN_WIDTH as f32,
            visible: false,
            focused: false,
            composer_at: (0, 0),
            monitor: None,
            selecting: false,
        })
    }

    /// DWM's dark border and shadow tint follow the theme.
    pub fn set_dark(&self, dark: bool) {
        let v = BOOL::from(dark);
        // SAFETY: as in create
        let _ = unsafe {
            DwmSetWindowAttribute(
                self.hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                (&v as *const BOOL).cast(),
                std::mem::size_of::<BOOL>() as u32,
            )
        };
    }

    pub fn cloak(&self, on: bool) {
        let v = BOOL::from(on);
        // SAFETY: as in create
        let _ = unsafe {
            DwmSetWindowAttribute(
                self.hwnd,
                DWMWA_CLOAK,
                (&v as *const BOOL).cast(),
                std::mem::size_of::<BOOL>() as u32,
            )
        };
    }

    pub fn scale(&self) -> f32 {
        self.surface.scale()
    }

    /// Lays the bar out for the model as it is.
    pub fn layout(gfx: &Gfx, model: &BarModel, width: f32, now: i64) -> BarLayout {
        let visible = model.visible(now);
        let tape_rows = history::shown(model, visible);
        let anchor = history::height(tape_rows.len());
        let composer = Rect::new(0.0, anchor, width, size::COMPOSER);
        let inner = width - 2.0 * size::GUTTER;
        let answer_text = model.answer_text();
        let aw = answer::wanted_width(gfx, &answer_text)
            .min(inner * 0.67 - 12.0)
            .max(0.0);
        let field_w = if aw > 0.0 { inner - aw - 12.0 } else { inner };
        let field = Rect::new(
            size::GUTTER,
            anchor + (size::COMPOSER - 36.0) / 2.0,
            field_w,
            36.0,
        );
        let answer = Rect::new(
            width - size::GUTTER - aw,
            anchor + (size::COMPOSER - 36.0) / 2.0,
            aw,
            36.0,
        );
        let mut height = anchor + size::COMPOSER;
        let hint = model.hint.as_ref().filter(|h| !h.is_empty()).map(|_| {
            // .composer-hint: 12 muted, tucked 12 up into the composer's bottom padding
            let r = Rect::new(size::GUTTER, height - 12.0, inner, 18.0);
            height += 18.0;
            r
        });
        let systems = model.systems.as_ref().map(|s| {
            let r = Rect::new(0.0, height, width, crate::systems::height(s));
            height += r.h;
            r
        });
        let graph = model.graph.as_ref().map(|g| {
            let r = Rect::new(0.0, height, width, crate::graph::height(gfx, g, width));
            height += r.h;
            r
        });
        BarLayout {
            width,
            height,
            anchor,
            tape_rows,
            composer,
            field,
            answer,
            answer_text,
            hint,
            systems,
            graph,
        }
    }

    /// One frame of the bar as laid out.
    pub fn paint(&mut self, gfx: &Gfx, theme: &Theme, model: &BarModel) -> Result<()> {
        let layout = self.layout.clone();
        let window_focused = self.focused || !self.visible;
        let focused = window_focused && !crate::systems::focused(model);
        let field_view = &mut self.field_view;
        self.surface.draw(gfx, theme, |c: &mut Canvas| {
            c.clear(theme.bg);
            history::paint(c, model, layout.tape_rows.clone());
            let placeholder = "Calculate";
            if model.chained && !model.field.text().is_empty() {
                // .quick-chain: the faint `ans` a chained line continues from
                if let Some(l) = c
                    .gfx
                    .layout("ans", &c.gfx.fonts.display, 0.0, layout.field.h)
                {
                    let (w, _) = crate::render::measure(&l);
                    c.text_plain(
                        &l,
                        layout.field.x,
                        layout.field.y,
                        with_alpha(theme.muted, 0.75),
                    );
                    let f = Rect::new(
                        layout.field.x + w + 5.0,
                        layout.field.y,
                        layout.field.w - w - 5.0,
                        layout.field.h,
                    );
                    field_view.paint(c, &model.field, f, focused, placeholder);
                } else {
                    field_view.paint(c, &model.field, layout.field, focused, placeholder);
                }
            } else {
                field_view.paint(c, &model.field, layout.field, focused, placeholder);
            }
            let stale = model.live_for != model.field.text();
            answer::paint(c, &layout.answer_text, layout.answer, stale);
            if let (Some(r), Some(hint)) = (layout.hint, model.hint.as_ref())
                && let Some(l) = c.gfx.layout_in(
                    hint,
                    &c.gfx.fonts.small,
                    r.w,
                    r.h,
                    windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT_LEADING,
                )
            {
                c.text(&l, r.x, r.y, theme.muted);
            }
            if let (Some(r), Some(s)) = (layout.systems, &model.systems) {
                crate::systems::paint(c, s, r, window_focused);
            }
            if let (Some(r), Some(g)) = (layout.graph, &model.graph) {
                crate::graph::paint(c, g, r);
            }
        })
    }

    /// Where a showing opens and how big the window is, physical px: (x, y, w, h).
    pub fn placement(
        &self,
        monitor: &Monitor,
        saved: Option<[f64; 2]>,
        width_dips: f32,
        height_dips: f32,
        anchor_dips: f32,
    ) -> (i32, i32, i32, i32) {
        let scale = monitor.scale();
        let w = (f64::from(width_dips) * scale).round();
        let h = (f64::from(height_dips) * scale).round();
        let anchor = f64::from(anchor_dips) * scale;
        // the composer's spot, then the window above it by the tape's height
        let (cx, cy) = place::show_origin(
            monitor.area,
            scale,
            w,
            f64::from(size::COMPOSER) * scale,
            saved,
        );
        let (x, y) = place::fit(monitor.area, cx, cy - anchor, w, h);
        (x as i32, y as i32, w as i32, h as i32)
    }

    /// The window's frame for a new height with the composer kept where it is on screen.
    pub fn reframe(&self, height_dips: f32, anchor_dips: f32) -> (i32, i32, i32, i32) {
        let scale = f64::from(self.scale());
        let w = (f64::from(self.width) * scale).round();
        let h = (f64::from(height_dips) * scale).round();
        let (cx, cy) = (f64::from(self.composer_at.0), f64::from(self.composer_at.1));
        let y = cy - f64::from(anchor_dips) * scale;
        let (x, y) = match &self.monitor {
            Some(m) => place::fit(m.area, cx, y, w, h),
            None => (cx, y),
        };
        (x as i32, y as i32, w as i32, h as i32)
    }

    /// The window's current frame.
    pub fn frame(&self) -> RECT {
        let mut r = RECT::default();
        // SAFETY: a live out rect
        let _ =
            unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowRect(self.hwnd, &mut r) };
        r
    }
}
