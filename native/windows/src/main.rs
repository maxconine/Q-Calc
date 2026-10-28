//! Q Calc for Windows, native. See native/windows/README.md for the module map and the rules.
#![cfg_attr(windows, windows_subsystem = "windows")]
// the foundation keeps API (lanes, fonts, theme colours, store helpers) for the panels later work adds
#![allow(dead_code)]

// pure logic: compiles (and is tested) everywhere
#[cfg(windows)]
mod a11y;
mod autostart;
mod commands;
mod edge;
mod exponent;
mod hotkey;
mod keys;
mod log;
mod perf;
mod place;
mod store;
mod text_rules;
mod tour;

#[cfg(windows)]
mod analytics;
#[cfg(windows)]
mod answer;
#[cfg(windows)]
mod app;
#[cfg(windows)]
mod bench;
mod bg;
#[cfg(windows)]
mod cheats;
#[cfg(windows)]
mod clip;
#[cfg(windows)]
mod clip_image;
#[cfg(windows)]
mod contrast;
#[cfg(windows)]
mod crash;
#[cfg(windows)]
mod drag;
#[cfg(windows)]
mod effects;
#[cfg(windows)]
mod engine;
#[cfg(windows)]
mod games;
#[cfg(windows)]
mod graph;
#[cfg(windows)]
mod heap;
#[cfg(windows)]
mod history;
#[cfg(windows)]
mod hover;
#[cfg(windows)]
mod input;
#[cfg(windows)]
mod later;
#[cfg(windows)]
mod line;
#[cfg(windows)]
mod messaging;
#[cfg(windows)]
mod migrate;
#[cfg(windows)]
mod model;
#[cfg(windows)]
mod net;
#[cfg(windows)]
mod rates;
#[cfg(windows)]
mod relay;
#[cfg(windows)]
mod render;
#[cfg(windows)]
mod selection;
#[cfg(windows)]
mod settings_window;
#[cfg(windows)]
mod share;
#[cfg(windows)]
mod share_crypt;
#[cfg(windows)]
mod share_debug;
#[cfg(windows)]
mod share_drop;
#[cfg(windows)]
mod share_keys;
#[cfg(windows)]
mod sheets;
#[cfg(windows)]
mod systems;
#[cfg(windows)]
mod threads;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
mod typst_row;
#[cfg(windows)]
mod uia;
#[cfg(windows)]
mod update;
#[cfg(windows)]
mod warp;
#[cfg(windows)]
mod window;
#[cfg(windows)]
mod word_tip;
#[cfg(windows)]
mod wrap;

#[cfg(feature = "perf")]
#[global_allocator]
static ALLOC: perf::Counting = perf::Counting;
// the engine's freeable regexes in a heap of their own (heap.rs)
#[cfg(all(windows, not(feature = "perf")))]
#[global_allocator]
static ALLOC: heap::Routed = heap::Routed;

#[cfg(windows)]
fn main() {
    // `--game <kind>`: a game in a process of its own (games/process.rs), nothing of the bar
    if let Some(code) = games::child_main() {
        std::process::exit(code);
    }
    std::process::exit(app::run());
}

#[cfg(not(windows))]
fn main() {
    eprintln!("qcalc-win is the Windows app; on this OS it only builds its tests.");
}
