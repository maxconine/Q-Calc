//! The settings window (stub). The Mac's spec: native/mac/Sources/Shared/SettingsWindow.swift (a symlink to
//! macos/SettingsWindow.swift); the Tauri app's page is src/settings.tsx with the host side in src-tauri/src/main.rs
//! (`settings_payload`, `host` commands: hotkey probe via hotkey.rs, keybinds, autostart via autostart::apply,
//! theme, units, sig figs, draft seconds, spot seconds, updates).
//!
//! To build here: its own top-level window (class `QCalcSettings`, a normal titled window, in the taskbar while
//! open), drawn with render.rs; changes go to `App::store`, then `App::save_store`, `App::model.set_settings`, and
//! `App::hotkey.select` for a new shortcut. While it is open the bar must not hide when it takes the foreground
//! (app.rs already ignores activations of our own windows).

use crate::app::App;
use crate::qlog;

/// Opens (or brings forward) the settings window. For now: says where the settings file is.
pub fn open(app: &mut App) {
    qlog!("settings: the native settings window isn't built yet");
    app.model.hint = Some(
        r"settings aren't in the native app yet · %APPDATA%\com.maxconine.qcalc\settings.json"
            .into(),
    );
    app.model.touch();
}
