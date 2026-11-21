//! The easter eggs (EggOverlay.swift, BarModel.swift's `checkEffects` / `konamiKey`, PARITY.md's egg rows):
//!
//! - 420 smoke, 67 hands, 69 digits: the answer rests on the number for 200 ms (so typing 4200 passes through 420
//!   without firing), then the core's `EggGate` fires it once, and again only after the answer was something else.
//! - native-only, through the core's `LineEggGate` on the same settle: the black hole for `x/0`, Euler's confetti
//!   and note for `e^(iπ)+1`, π's digits for `pi` on March 14, and `do a barrel roll`. One of them wins over a number
//!   egg. The black hole, the digits and the roll belong to their line: typing on ends them.
//! - Pi Day: a small slice of pie in the composer's left gutter, all March 14 (`QCALC_PI_DAY=1` makes any day one).
//! - the Konami code (↑↑↓↓←→←→BA in the bar, the core's `KonamiTracker`) toggles a green-phosphor CRT look for the
//!   run: the bar's colours through the Mac's colour matrix, on the dark bar, under scanlines.
//!
//! Cost on the keystroke path: `after` compares the answer with the last one seen (two short strings) and, when it
//! changed, notes it and arms one 200 ms timer if none is armed; the gates run when that fires, never per key. Each
//! effect draws on its own thread in its own window (overlay.rs) and takes everything with it when it ends.
//! The bar itself only changes for the CRT look (`crt_theme`, `paint_bar`, called from `Bar::paint`) and the roll
//! (the bar is cloaked while a picture of it turns; it keeps the keys).

mod anim;
mod overlay;
mod shapes;

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use qcalc_ffi::{
    EggGate, EggKind, KonamiGate, KonamiKey, LineEggGate, LineEggKind, egg_play_ms, egg_settle_ms,
    is_barrel_roll, is_pi_day, line_egg_play_ms, pi_digits, smoke_puffs,
};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_CLOSED,
    D2D1_FIGURE_END_OPEN,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_SMALL, D2D1_CAP_STYLE_FLAT, D2D1_CAP_STYLE_ROUND,
    D2D1_DASH_STYLE_SOLID, D2D1_LINE_JOIN_ROUND, D2D1_STROKE_STYLE_PROPERTIES,
    D2D1_SWEEP_DIRECTION_CLOCKWISE, ID2D1PathGeometry, ID2D1StrokeStyle,
};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleDC, CreateDIBSection,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, KillTimer, SPI_GETCLIENTAREAANIMATION, SW_HIDE,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SetTimer, ShowWindowAsync, SystemParametersInfoW,
};
use windows::core::BOOL;
use windows_numerics::{Matrix3x2, Vector2};

use crate::app::{App, with_app};
use crate::model::BarModel;
use crate::render::{Canvas, Rect, Theme, rgb};
use crate::window::BarLayout;
use overlay::{Kind, Shared, Spec};

/// the settle timer's id on the host window (its own TIMERPROC, so the host's wndproc never sees it)
const TIMER_SETTLE: usize = 0x4547;

/// An answer as it arrived, waiting to settle.
struct Answered {
    text: String,
    display: String,
    number: Option<f64>,
    /// a plain number (no unit): what 420 / 67 / 69 read
    plain: Option<f64>,
}

/// An effect on screen.
struct Playing {
    shared: Arc<Shared>,
    kind: Kind,
}

/// The eggs' state, on `BarModel` (as the Mac keeps its gates in BarModel).
#[derive(Default)]
pub struct Eggs {
    gate: EggGate,
    lines: LineEggGate,
    konami: KonamiGate,
    /// the Konami code's CRT look, for this run only (never saved)
    pub crt: bool,
    /// the answer last seen: (the line it answers, its display)
    seen: Option<(String, String)>,
    answered: Option<Answered>,
    answered_at: Option<Instant>,
    armed: bool,
    /// the line a black hole, π's digits or the roll is playing for
    effect_line: Option<String>,
    playing: Option<Playing>,
    /// the Konami code under way: whether the tape was open at its first ↑, and the line as its B came
    konami_start: Option<(bool, String)>,
    /// the code finished: that, and the line as its A went down (its `a` comes in the WM_CHAR after)
    konami_end: Option<(bool, String, String)>,
    /// the code's leftovers to take back out, on the next turn (`konami_tidy`)
    konami_tidy: Option<(bool, String)>,
}

/// The Konami code's B and A go on into the line (the field still gets every key), so a finished code left `ba`
/// in it (EggRules.swift `konamiLeftover`). `before` is the line as the B came, `after` the line once the A landed:
/// when `after` is `before` with `ba` (any case) put in at one spot, the line to put back and its caret (bytes);
/// none when anything else changed.
pub fn konami_leftover(before: &str, after: &str) -> Option<(String, usize)> {
    let (b, a) = (before.as_bytes(), after.as_bytes());
    if a.len() != b.len() + 2 {
        return None;
    }
    let p = b.iter().zip(a).take_while(|(x, y)| x == y).count();
    let typed = &a[p..p + 2];
    (typed.eq_ignore_ascii_case(b"ba") && a[p + 2..] == b[p..]).then(|| (before.to_string(), p))
}

impl std::fmt::Debug for Eggs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Eggs")
            .field("crt", &self.crt)
            .finish_non_exhaustive()
    }
}

/// After a message changed the model (app.rs `App::after`): end an effect whose line was typed on, and note a new
/// answer for the settle.
pub fn after(app: &mut App) {
    let host = app.host;
    let bar = app.bar.hwnd;
    let m = &mut app.model;
    // the code's A has landed in the line: its `ba` comes back out on the next turn
    if let Some((open, before, at_a)) = m.eggs.konami_end.take() {
        if m.field.text() == at_a {
            m.eggs.konami_end = Some((open, before, at_a));
        } else {
            m.eggs.konami_tidy = Some((open, before));
            app.ops
                .push(crate::app::Op::Run(crate::bg::Later(konami_tidy)));
        }
    }
    let m = &mut app.model;
    if let Some(line) = &m.eggs.effect_line
        && m.field.text() != line
    {
        m.eggs.effect_line = None;
        stop(&mut m.eggs, bar);
    }
    let display = m.live.as_ref().map_or("", |a| a.display.as_str());
    if m.eggs
        .seen
        .as_ref()
        .is_some_and(|(t, d)| *t == m.live_for && d == display)
    {
        return;
    }
    let (number, plain) = m.live.as_ref().map_or((None, None), |a| {
        (a.number, if a.unit.is_none() { a.number } else { None })
    });
    m.eggs.seen = Some((m.live_for.clone(), display.to_string()));
    m.eggs.answered = Some(Answered {
        text: m.live_for.clone(),
        display: display.to_string(),
        number,
        plain,
    });
    m.eggs.answered_at = Some(Instant::now());
    if !m.eggs.armed {
        m.eggs.armed = true;
        arm(host, egg_settle_ms() as u32);
    }
}

fn arm(host: HWND, ms: u32) {
    // SAFETY: a one-shot timer on our host, handled by our own TIMERPROC (which kills it)
    unsafe { SetTimer(Some(host), TIMER_SETTLE, ms.max(1), Some(settle_proc)) };
}

unsafe extern "system" fn settle_proc(hwnd: HWND, _msg: u32, id: usize, _time: u32) {
    // SAFETY: our own timer
    let _ = unsafe { KillTimer(Some(hwnd), id) };
    with_app(settle);
}

/// The settle timer fired: if the answer has rested 200 ms, run the gates.
fn settle(app: &mut App) {
    let eggs = &mut app.model.eggs;
    eggs.armed = false;
    let wait = Duration::from_secs_f64(egg_settle_ms() / 1000.0);
    if let Some(at) = eggs.answered_at {
        let rested = at.elapsed();
        if rested + Duration::from_millis(1) < wait {
            eggs.armed = true;
            arm(app.host, (wait - rested).as_millis() as u32 + 1);
            return;
        }
    }
    let Some(a) = eggs.answered.take() else {
        return;
    };
    let egg = eggs.gate.settle(a.plain);
    let line = eggs
        .lines
        .settle(a.text.clone(), a.display.clone(), a.number, pi_day());
    let kind = match (line, egg) {
        (Some(l), _) => {
            eggs.effect_line = (l != LineEggKind::Euler).then(|| a.text.clone());
            line_kind(l)
        }
        (None, Some(e)) => match e {
            EggKind::Smoke => Kind::Smoke,
            EggKind::Arms => Kind::Arms,
            EggKind::Fold => Kind::Fold,
        },
        (None, None) => return,
    };
    play(app, kind);
}

fn line_kind(l: LineEggKind) -> Kind {
    match l {
        LineEggKind::Void => Kind::Void,
        LineEggKind::Euler => Kind::Euler,
        LineEggKind::PiDigits => Kind::PiDigits,
        LineEggKind::BarrelRoll => Kind::Roll,
    }
}

/// Windows' "Animation effects" (Settings > Accessibility > Visual effects) off reads as Reduce Motion.
fn reduce_motion() -> bool {
    let mut on = BOOL(1);
    // SAFETY: a live out BOOL
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut on as *mut BOOL).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    ok.is_ok() && !on.as_bool()
}

/// March 14 by the local calendar (`QCALC_PI_DAY=1` makes any day Pi Day, to try it).
pub fn pi_day() -> bool {
    if std::env::var_os("QCALC_PI_DAY").is_some_and(|v| v == "1") {
        return true;
    }
    // SAFETY: plain query
    let now = unsafe { GetLocalTime() };
    is_pi_day(u32::from(now.wMonth), u32::from(now.wDay))
}

/// The bar's colours under the CRT look: the dark bar through the phosphor matrix.
pub fn crt_theme(model: &BarModel) -> Option<Theme> {
    if !model.eggs.crt {
        return None;
    }
    let mut t = Theme::new(true);
    let p = |c: &mut crate::render::Color| {
        let (r, g, b) = anim::phosphor(c.r, c.g, c.b);
        (c.r, c.g, c.b) = (r, g, b);
    };
    for c in [
        &mut t.bg,
        &mut t.bg_tape,
        &mut t.ink,
        &mut t.muted,
        &mut t.answer,
        &mut t.answer_hover,
        &mut t.accent,
        &mut t.accent_hover,
        &mut t.line,
        &mut t.fill_hover,
        &mut t.fill_selected,
        &mut t.selection,
    ] {
        p(c);
    }
    Some(t)
}

thread_local! {
    /// Pi Day's slice: (wedge, crust) geometries and the crust's round caps, made the first time they're drawn
    static PIE: std::cell::OnceCell<Option<(ID2D1PathGeometry, ID2D1PathGeometry, ID2D1StrokeStyle)>> =
        const { std::cell::OnceCell::new() };
}

fn pie(c: &Canvas) -> Option<(ID2D1PathGeometry, ID2D1PathGeometry, ID2D1StrokeStyle)> {
    let f = &c.gfx.d2d;
    let pt = |x: f32, y: f32| Vector2 { X: x, Y: y };
    let arc = |r: f32, deg: f32| {
        pt(
            1.0 + r * deg.to_radians().cos(),
            8.5 + r * deg.to_radians().sin(),
        )
    };
    let seg = |r: f32| D2D1_ARC_SEGMENT {
        point: arc(r, 0.0),
        size: windows::Win32::Graphics::Direct2D::Common::D2D_SIZE_F {
            width: r,
            height: r,
        },
        rotationAngle: 0.0,
        sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
        arcSize: D2D1_ARC_SIZE_SMALL,
    };
    // SAFETY: building geometries we own; each sink is closed before use
    unsafe {
        let wedge = f.CreatePathGeometry().ok()?;
        let s = wedge.Open().ok()?;
        s.BeginFigure(pt(1.0, 8.5), D2D1_FIGURE_BEGIN_FILLED);
        s.AddLine(arc(8.5, -60.0));
        s.AddArc(&seg(8.5));
        s.EndFigure(D2D1_FIGURE_END_CLOSED);
        s.Close().ok()?;
        let crust = f.CreatePathGeometry().ok()?;
        let s = crust.Open().ok()?;
        s.BeginFigure(arc(7.8, -60.0), D2D1_FIGURE_BEGIN_HOLLOW);
        s.AddArc(&seg(7.8));
        s.EndFigure(D2D1_FIGURE_END_OPEN);
        s.Close().ok()?;
        let props = D2D1_STROKE_STYLE_PROPERTIES {
            startCap: D2D1_CAP_STYLE_ROUND,
            endCap: D2D1_CAP_STYLE_ROUND,
            dashCap: D2D1_CAP_STYLE_FLAT,
            lineJoin: D2D1_LINE_JOIN_ROUND,
            miterLimit: 10.0,
            dashStyle: D2D1_DASH_STYLE_SOLID,
            dashOffset: 0.0,
        };
        let round = f.CreateStrokeStyle(&props, None).ok()?;
        Some((wedge, crust, round))
    }
}

/// Over the bar, last in `Bar::paint`: Pi Day's slice, and the CRT look's scanlines.
pub fn paint_bar(c: &mut Canvas, model: &BarModel, layout: &BarLayout, scale: f32) {
    if pi_day() {
        PIE.with(|cell| {
            if let Some((wedge, crust, round)) = cell.get_or_init(|| pie(c)) {
                let (rt, brush) = c.raw();
                // the slice is 10 × 9, 3 in from the left, centred on the composer and 1 up
                let (x, y) = (
                    3.0,
                    layout.composer.y + (layout.composer.h - 9.0) / 2.0 - 1.0,
                );
                let mut before = Matrix3x2::default();
                // SAFETY: inside the bar's draw; the transform is put back
                unsafe {
                    rt.GetTransform(&mut before);
                    let mut m = before;
                    m.M31 += x;
                    m.M32 += y;
                    rt.SetTransform(&m);
                    brush.SetColor(&crate::render::Color {
                        r: 0.93,
                        g: 0.72,
                        b: 0.42,
                        a: 0.75,
                    });
                    rt.FillGeometry(wedge, brush, None);
                    brush.SetColor(&crate::render::Color {
                        r: 0.72,
                        g: 0.45,
                        b: 0.2,
                        a: 0.75,
                    });
                    rt.DrawGeometry(crust, brush, 1.4, Some(round));
                    rt.SetTransform(&before);
                }
            }
        });
    }
    if model.eggs.crt {
        // 1 px lines every 3, on whole pixels
        let line = rgb(0, 0.28);
        let px = scale.max(1.0);
        let mut k = 0.0f32;
        loop {
            let y = (k * 3.0 * px).round() / px;
            if y >= c.height {
                break;
            }
            c.fill(Rect::new(0.0, y, c.width, px.round() / px), line);
            k += 1.0;
        }
    }
}

/// A key down in the bar, before the model has it (app.rs, WM_KEYDOWN): the Konami code watches every key and
/// never eats one; ↵ on `do a barrel roll` rolls now if it hasn't yet, and clears the line without saving.
/// Returns true when it took the key.
pub fn key_down(app: &mut App, vk: u32, lp: isize) -> bool {
    // SAFETY: plain queries of the keyboard state for this message
    let down = |k: u16| unsafe { GetKeyState(i32::from(k)) } < 0;
    let chord = down(VK_CONTROL.0) || down(VK_MENU.0) || down(VK_LWIN.0) || down(VK_RWIN.0);
    // a held key's repeats aren't new presses
    let repeat = lp & (1 << 30) != 0;
    if !repeat {
        let key = match vk {
            0x26 => KonamiKey::Up,
            0x28 => KonamiKey::Down,
            0x25 => KonamiKey::Left,
            0x27 => KonamiKey::Right,
            0x42 if !chord => KonamiKey::B,
            0x41 if !chord => KonamiKey::A,
            _ => KonamiKey::Other,
        };
        let text = app.model.field.text().to_string();
        let eggs = &mut app.model.eggs;
        let before = eggs.konami.progress();
        if before == 0 && key == KonamiKey::Up {
            eggs.konami_start = Some((app.model.tape_open, text.clone()));
        }
        if key == KonamiKey::B
            && before == 8
            && let Some(start) = &mut eggs.konami_start
        {
            start.1.clone_from(&text);
        }
        if app.model.eggs.konami.push(key) {
            let eggs = &mut app.model.eggs;
            if let Some((open, at_b)) = eggs.konami_start.take() {
                eggs.konami_end = Some((open, at_b, text));
            }
            eggs.crt = !eggs.crt;
            app.bar.set_dark(eggs.crt || app.theme.dark);
            app.model.touch();
            crate::qlog!(
                "konami: crt {}",
                if app.model.eggs.crt { "on" } else { "off" }
            );
        }
    }
    if vk == 0x0D
        && !chord
        && !down(VK_SHIFT.0)
        && app.model.selected.is_none()
        && is_barrel_roll(app.model.field.text().to_string())
    {
        let eggs = &mut app.model.eggs;
        let rolling = eggs
            .playing
            .as_ref()
            .is_some_and(|p| p.kind == Kind::Roll && !p.shared.done.load(Ordering::SeqCst));
        if !rolling {
            let text = app.model.field.text().to_string();
            if eggs.lines.settle(text, String::new(), None, pi_day())
                == Some(LineEggKind::BarrelRoll)
            {
                play(app, Kind::Roll);
            }
        }
        app.model.eggs.effect_line = None;
        app.model.field.reset("");
        app.text_changed();
        return true;
    }
    false
}

/// After the Konami code's A landed (an `Op::Run`, outside the message's borrow): the line as it was at the B when
/// only the code's `ba` went in since, caret where it was, and a tape that was closed at the first ↑ closed again.
fn konami_tidy() {
    crate::app::poke(|app| {
        let Some((open, before)) = app.model.eggs.konami_tidy.take() else {
            return;
        };
        if let Some((text, caret)) = konami_leftover(&before, app.model.field.text()) {
            app.model.field.set_text_at(&text, caret);
            app.model.field.break_run();
            app.text_changed();
        }
        if !open && app.model.selected.is_none() && app.model.tape_open {
            app.model.tape_open = false;
        }
        app.model.touch();
    });
}

/// The bar hid (app.rs `after_hide`): an effect over it goes with it. The barrel roll holds the bar cloaked under
/// its spinning picture, which the hotkey's hide left turning on screen.
pub fn will_hide(app: &mut App) {
    let bar = app.bar.hwnd;
    let eggs = &mut app.model.eggs;
    eggs.effect_line = None;
    eggs.konami_start = None;
    stop(eggs, bar);
}

/// Ends the effect on screen, if any.
fn stop(eggs: &mut Eggs, bar: HWND) {
    let Some(p) = eggs.playing.take() else {
        return;
    };
    p.shared.cancel.store(true, Ordering::SeqCst);
    let hwnd = p.shared.hwnd.load(Ordering::SeqCst);
    if hwnd != 0 && !p.shared.done.load(Ordering::SeqCst) {
        // SAFETY: posted, never sent: the effect's thread hides it now and tears down on its next frame
        unsafe {
            let _ = ShowWindowAsync(HWND(hwnd as *mut _), SW_HIDE);
        }
    }
    if p.kind == Kind::Roll {
        overlay::cloak(bar.0 as usize, false);
    }
}

/// The bar's pixels as it is now (for the barrel roll's picture): BGRA rows and the size.
fn capture(bar: HWND) -> Option<(Vec<u8>, u32, u32)> {
    let mut r = Default::default();
    // SAFETY: plain GDI on our own bar's DC; everything made here is released here
    unsafe {
        GetClientRect(bar, &mut r).ok()?;
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        if w <= 0 || h <= 0 {
            return None;
        }
        let dc = GetDC(Some(bar));
        let mem = CreateCompatibleDC(Some(dc));
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let out = match CreateDIBSection(Some(mem), &info, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(dib) => {
                let old = SelectObject(mem, dib.into());
                let ok = BitBlt(mem, 0, 0, w, h, Some(dc), 0, 0, SRCCOPY).is_ok();
                let px = ok.then(|| {
                    std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize).to_vec()
                });
                SelectObject(mem, old);
                let _ = DeleteObject(dib.into());
                px.map(|px| (px, w as u32, h as u32))
            }
            Err(_) => None,
        };
        let _ = DeleteDC(mem);
        ReleaseDC(Some(bar), dc);
        out
    }
}

/// Starts an effect over the bar (it must be up).
fn play(app: &mut App, kind: Kind) {
    if !app.bar.visible {
        return;
    }
    let bar = app.bar.hwnd;
    stop(&mut app.model.eggs, bar);
    let reduce = reduce_motion();
    // the barrel roll is all motion: Reduce Motion skips it
    if kind == Kind::Roll && reduce {
        return;
    }
    let l = &app.bar.layout;
    if matches!(kind, Kind::Void | Kind::PiDigits)
        && (l.answer.w <= 0.0 || l.answer_text.is_empty())
    {
        return;
    }
    let picture = if kind == Kind::Roll {
        match capture(bar) {
            Some(p) => Some(p),
            None => return,
        }
    } else {
        None
    };
    let theme = crt_theme(&app.model).unwrap_or(app.theme);
    let scale = app.bar.scale();
    let (pw, ph) = app.bar.surface.px;
    let play_ms = match kind {
        Kind::Smoke => egg_play_ms(EggKind::Smoke),
        Kind::Arms => egg_play_ms(EggKind::Arms),
        Kind::Fold => egg_play_ms(EggKind::Fold),
        Kind::Void => line_egg_play_ms(LineEggKind::Void),
        Kind::Euler => line_egg_play_ms(LineEggKind::Euler),
        Kind::PiDigits => line_egg_play_ms(LineEggKind::PiDigits),
        Kind::Roll => line_egg_play_ms(LineEggKind::BarrelRoll),
    };
    let spec = Spec {
        kind,
        play_ms,
        reduce,
        bar: bar.0 as usize,
        dpi: app.bar.surface.dpi,
        bar_w: pw as f32 / scale,
        bar_h: ph as f32 / scale,
        composer: l.composer,
        answer: l.answer,
        answer_text: l.answer_text.clone(),
        bg: theme.bg,
        muted: theme.muted,
        answer_color: theme.answer,
        family: app.gfx.fonts.family.clone(),
        text_family: app.gfx.fonts.text_family.clone(),
        puffs: if kind == Kind::Smoke {
            smoke_puffs()
        } else {
            Vec::new()
        },
        pi: if kind == Kind::PiDigits {
            pi_digits()
        } else {
            String::new()
        },
        picture,
    };
    if let Some(shared) = overlay::spawn(spec) {
        app.model.eggs.playing = Some(Playing { shared, kind });
        crate::qlog!(
            "effect {} (native){}",
            kind.name(),
            if reduce { ", reduced motion" } else { "" }
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // test-eggs.sh's leftover checks
    #[test]
    fn the_konami_codes_ba_comes_back_out() {
        assert_eq!(konami_leftover("2+2", "2+2ba"), Some(("2+2".into(), 3)));
        assert_eq!(konami_leftover("2+2", "2+2BA"), Some(("2+2".into(), 3)));
        // typed mid-line, the caret where the B went in
        assert_eq!(konami_leftover("2+2", "2+ba2"), Some(("2+2".into(), 2)));
        assert_eq!(konami_leftover("", "ba"), Some((String::new(), 0)));
        assert_eq!(konami_leftover("π", "πba"), Some(("π".into(), 2)));
        // anything else changed: left alone
        assert_eq!(konami_leftover("2+2", "2+2ab"), None);
        assert_eq!(konami_leftover("2+2", "2+3ba"), None);
        assert_eq!(konami_leftover("2+2", "2+2b"), None);
        assert_eq!(konami_leftover("2+2", "2+2bax"), None);
    }

    #[test]
    fn crt_theme_is_green_on_black() {
        let mut m = BarModel::new(crate::store::Settings::default());
        assert!(crt_theme(&m).is_none());
        m.eggs.crt = true;
        let t = crt_theme(&m).unwrap();
        assert!(t.ink.g > t.ink.r && t.ink.g > t.ink.b);
        assert!(t.bg.g < 0.2 && t.bg.r < 0.1);
    }
}
