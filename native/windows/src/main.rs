//! Q Calc for Windows, native. See native/windows/README.md for the module map and the rules.
#![cfg_attr(windows, windows_subsystem = "windows")]
// the foundation keeps API (lanes, fonts, theme colours, store helpers) for the panels later work adds
#![allow(dead_code)]

// pure logic: compiles (and is tested) everywhere
mod autostart;
mod hotkey;
mod keys;
mod log;
mod place;
mod store;
mod text_rules;

#[cfg(windows)]
mod analytics;
#[cfg(windows)]
mod answer;
#[cfg(windows)]
mod app;
#[cfg(windows)]
mod bench;
#[cfg(windows)]
mod cheats;
mod bg;
#[cfg(windows)]
mod clip;
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
mod history;
#[cfg(windows)]
mod input;
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
mod render;
#[cfg(windows)]
mod settings_window;
#[cfg(windows)]
mod sheets;
#[cfg(windows)]
mod systems;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
mod typst_row;
#[cfg(windows)]
mod update;
#[cfg(windows)]
mod window;

#[cfg(windows)]
fn main() {
    std::process::exit(app::run());
}

#[cfg(not(windows))]
fn main() {
    eprintln!("qcalc-win is the Windows app; on this OS it only builds its tests.");
}
