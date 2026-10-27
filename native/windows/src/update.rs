//! Updates (stub). The Tauri app used tauri-plugin-updater (src-tauri/src/update.rs: check, download, install,
//! the marker that makes the relaunch quiet). The native app needs its own: fetch the release manifest, verify
//! the signature, run the NSIS installer with /S, relaunch with `--autostart`. Keep `store.auto_update`.
