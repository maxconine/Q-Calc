//! Currency rates (stub). The engine reads `<config dir>\rates.json` (`Engine::reload_rates`, called at launch).
//! Fetching the ECB XML and writing that file (src-tauri/src/rates.rs: strict parsing, 7-day staleness) goes
//! here, on a background thread, then `Engine::reload_rates` on Lane::Misc.
