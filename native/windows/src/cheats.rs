//! `?` / `help`: the cheat sheet where the tape is (`HelpSheet` in BarView.swift, `CheatSheet` in HistoryTape.tsx).
//! Its keys are the core's `cheat_sheet` with the keys the user has now (keys.rs `Keybinds::cheat_sheet`, worded
//! for Windows) and the native app's own (exponent form, answer forms, complete); its things to type are the
//! core's `native_type_cheats`, the web's with the native app's before `?`.
//!
//! Native only, as the Mac lays it out: three columns when the bar has room (the keys without the show / hide
//! chord; that chord, whose long name would widen the keys' column, heading the first half of the things to type;
//! the second half), rows of 28, a key column as wide as its widest key (at least 64 / 96 / 96). A narrower bar
//! (the Mac scrolls one column there; here Windows' longer key names, `Ctrl+Shift+Backspace`, need about 860 DIPs
//! for three) shares everything between two columns, so nothing is cut off.
//! Esc, ↑ or ↵ close it; typing past `?` keeps only what was typed after it (history.rs `text_changed`).

use std::cell::RefCell;

use windows::Win32::Graphics::DirectWrite::IDWriteTextLayout;

use crate::history::PAD_Y;
use crate::model::BarModel;
use crate::render::{Canvas, Gfx, Rect, measure};

pub const ROW: f32 = 28.0;

type Row = (String, String);

/// The sheet's two lists for the model's keybinds and global shortcut: [keys, things to type].
pub fn rows(m: &BarModel) -> [Vec<Row>; 2] {
    let hotkey = if m.tape.hotkey.is_empty() {
        String::new()
    } else {
        let k = qcalc_core::app::keybinds::normalize_chord(&m.tape.hotkey);
        qcalc_core::app::keybinds::chord_label(&k, true)
    };
    let [mut keys, _] = m.keybinds.cheat_sheet(&hotkey);
    // the native actions, after sig figs (BarView.swift's left column)
    let native = |id: &str| {
        let chord = crate::exponent::native_chord(&m.settings.keybinds, id);
        qcalc_core::app::keybinds::chord_label(&chord, true)
    };
    let mut own: Vec<Row> = [
        ("exponent", "exponent form"),
        ("forms", "answer forms"),
        ("working", "show working"),
    ]
    .iter()
    .map(|(id, label)| (native(id), label.to_string()))
    .filter(|(k, _)| !k.is_empty())
    .collect();
    own.push(("Tab".into(), "complete".into()));
    let at = keys
        .iter()
        .position(|(_, l)| l == "sig figs from input")
        .map_or(keys.len(), |i| i + 1);
    keys.splice(at..at, own);
    // the page collapses the core's double spaces (`GCF  LCM`), so this does too
    let typed = qcalc_core::app::onboarding::native_type_cheats()
        .into_iter()
        .map(|(a, b)| (a.replace("  ", " "), crate::keys::host_keys(&b)))
        .collect();
    [keys, typed]
}

/// Three columns: the keys without the show / hide chord; that chord heading the first half of the things to type;
/// the second half. Each with its key column's least width.
fn three(m: &BarModel) -> Vec<(Vec<Row>, f32)> {
    let [keys, typed] = rows(m);
    let (head, keys) = if !m.tape.hotkey.is_empty() && !keys.is_empty() {
        (keys[..1].to_vec(), keys[1..].to_vec())
    } else {
        (Vec::new(), keys)
    };
    let half = typed.len().div_ceil(2);
    let mut second = head;
    second.extend_from_slice(&typed[..half]);
    vec![(keys, 64.0), (second, 96.0), (typed[half..].to_vec(), 96.0)]
}

/// Everything shared between two columns.
fn two(m: &BarModel) -> Vec<(Vec<Row>, f32)> {
    let [keys, typed] = rows(m);
    let all: Vec<Row> = keys.into_iter().chain(typed).collect();
    let half = all.len().div_ceil(2);
    vec![(all[..half].to_vec(), 96.0), (all[half..].to_vec(), 96.0)]
}

/// A column's key width and the width it needs.
struct Fit {
    key: f32,
    need: f32,
}

fn fits(gfx: &Gfx, cols: &[(Vec<Row>, f32)]) -> Vec<Fit> {
    let widest = |col: &[Row], pick: fn(&Row) -> &str, least: f32| {
        col.iter()
            .filter_map(|r| text(gfx, pick(r)).map(|l| l.1))
            .fold(least, f32::max)
    };
    cols.iter()
        .map(|(col, least)| {
            let key = widest(col, |r| &r.0, *least);
            Fit {
                key,
                need: key + 12.0 + widest(col, |r| &r.1, 0.0) + 8.0,
            }
        })
        .collect()
}

fn gap(n: usize) -> f32 {
    if n > 2 { 12.0 } else { 24.0 }
}

/// The columns at this width: three when every label fits whole (the Mac's), else two.
fn columns(gfx: &Gfx, m: &BarModel, width: f32) -> (Vec<(Vec<Row>, f32)>, Vec<Fit>) {
    let cols = three(m);
    let f = fits(gfx, &cols);
    let need: f32 = f.iter().map(|f| f.need).sum::<f32>() + 2.0 * gap(3) + 16.0;
    if need <= width {
        return (cols, f);
    }
    let cols = two(m);
    let f = fits(gfx, &cols);
    (cols, f)
}

/// The sheet's height at this width (0 when it isn't shown).
pub fn height(gfx: &Gfx, m: &BarModel, width: f32) -> f32 {
    if !m.tape.help {
        return 0.0;
    }
    let tallest = columns(gfx, m, width)
        .0
        .iter()
        .map(|c| c.0.len())
        .max()
        .unwrap_or(0);
    tallest as f32 * ROW + 16.0 + 2.0 * PAD_Y + 1.0
}

thread_local! {
    static LAYOUTS: RefCell<Vec<(String, IDWriteTextLayout, f32)>> = const { RefCell::new(Vec::new()) };
}

fn text(gfx: &Gfx, s: &str) -> Option<(IDWriteTextLayout, f32)> {
    if let Some(hit) = LAYOUTS.with(|c| {
        c.borrow()
            .iter()
            .find(|e| e.0 == s)
            .map(|e| (e.1.clone(), e.2))
    }) {
        return Some(hit);
    }
    let l = gfx.layout(s, &gfx.fonts.body, 0.0, ROW)?;
    let w = measure(&l).0;
    LAYOUTS.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 96 {
            c.clear();
        }
        c.push((s.to_string(), l.clone(), w));
    });
    Some((l, w))
}

/// Drops the cached layouts (the hidden trim).
pub fn trim() {
    LAYOUTS.with(|c| c.borrow_mut().clear());
}

pub fn paint(c: &Canvas, m: &BarModel, h: f32) {
    if !m.tape.help || h <= 0.0 {
        return;
    }
    let t = c.theme;
    c.fill(Rect::new(0.0, 0.0, c.width, h), t.bg_tape);
    c.fill(Rect::new(0.0, h - 1.0, c.width, 1.0), t.line);
    let (cols, fit) = columns(c.gfx, m, c.width);
    let n = cols.len() as f32;
    let gap = gap(cols.len());
    // each column as wide as its keys and labels need, the room left over shared out (or, short of room, each
    // gives up the same share of what it needs)
    let room = c.width - 16.0 - gap * (n - 1.0);
    let total: f32 = fit.iter().map(|f| f.need).sum();
    let widths: Vec<f32> = fit
        .iter()
        .map(|f| {
            if total <= room {
                f.need + (room - total) / n
            } else {
                f.need * room / total
            }
        })
        .collect();
    let mut x0 = 16.0;
    for (k, (col, _)) in cols.iter().enumerate() {
        let (key_w, col_w) = (fit[k].key, widths[k]);
        if k > 0 {
            x0 += widths[k - 1] + gap;
        }
        for (i, (key, label)) in col.iter().enumerate() {
            let y = PAD_Y + 8.0 + i as f32 * ROW;
            if let Some((l, _)) = text(c.gfx, key) {
                c.text(&l, x0, y, t.ink);
            }
            if let Some((l, _)) = text(c.gfx, label) {
                c.push_clip(Rect::new(
                    x0 + key_w + 12.0,
                    y,
                    (col_w - key_w - 12.0).max(0.0),
                    ROW,
                ));
                c.text(&l, x0 + key_w + 12.0, y, t.muted);
                c.pop_clip();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_things_to_type_are_the_native_list() {
        let typed = qcalc_core::app::onboarding::native_type_cheats();
        assert!(typed.iter().any(|(a, _)| a == "chat max"));
        assert_eq!(typed.last().map(|r| r.0.as_str()), Some("?"));
        // ⌃W is worded for Windows
        let working = typed.iter().find(|(_, b)| b.contains("working")).unwrap();
        assert_eq!(
            crate::keys::host_keys(&working.1),
            "Ctrl+W shows the working"
        );
    }
}
