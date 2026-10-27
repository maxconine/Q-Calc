//! `?` / `help`: the cheat sheet where the tape is (`HelpSheet` in BarView.swift, `CheatSheet` in HistoryTape.tsx).
//! Its rows are the core's `cheat_sheet` with the keys the user has now (keys.rs `Keybinds::cheat_sheet`, worded
//! for Windows); two columns 24 apart, rows of 28, the key column as wide as its widest key (at least 64 / 96).
//! Esc, ↑ or ↵ close it; typing past `?` keeps only what was typed after it (history.rs `text_changed`).

use std::cell::RefCell;

use windows::Win32::Graphics::DirectWrite::IDWriteTextLayout;

use crate::history::PAD_Y;
use crate::model::BarModel;
use crate::render::{Canvas, Gfx, Rect, measure};

pub const ROW: f32 = 28.0;
const KEY_MIN: [f32; 2] = [64.0, 96.0];

/// The sheet's rows for the model's keybinds and global shortcut.
pub fn rows(m: &BarModel) -> [Vec<(String, String)>; 2] {
    let hotkey = if m.tape.hotkey.is_empty() {
        String::new()
    } else {
        let k = qcalc_core::app::keybinds::normalize_chord(&m.tape.hotkey);
        qcalc_core::app::keybinds::chord_label(&k, true)
    };
    m.keybinds.cheat_sheet(&hotkey)
}

/// The sheet's height (0 when it isn't shown).
pub fn height(m: &BarModel) -> f32 {
    if !m.tape.help {
        return 0.0;
    }
    let [a, b] = rows(m);
    a.len().max(b.len()) as f32 * ROW + 16.0 + 2.0 * PAD_Y + 1.0
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
        if c.len() > 64 {
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
    let cols = rows(m);
    let col_w = (c.width - 16.0 - 24.0) / 2.0;
    for (k, col) in cols.iter().enumerate() {
        let x0 = 8.0 + k as f32 * (col_w + 24.0) + 8.0;
        let key_w = col
            .iter()
            .filter_map(|(key, _)| text(c.gfx, key).map(|l| l.1))
            .fold(KEY_MIN[k], f32::max);
        for (i, (key, label)) in col.iter().enumerate() {
            let y = PAD_Y + 8.0 + i as f32 * ROW;
            if let Some((l, _)) = text(c.gfx, key) {
                c.text(&l, x0, y, t.ink);
            }
            if let Some((l, _)) = text(c.gfx, label) {
                c.push_clip(Rect::new(x0 + key_w + 12.0, y, (col_w - key_w - 28.0).max(0.0), ROW));
                c.text(&l, x0 + key_w + 12.0, y, t.muted);
                c.pop_clip();
            }
        }
    }
}
