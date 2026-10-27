# Integration follow-ups

Items packages handed back that no single package owns yet. Strike them through as they land.

- ~~**Time zone for phrases** (from WP-PHRASES): `Engine::phrase` goes through the generic dispatch with no zone, so everyone gets America/Los_Angeles. Add `EngineConfig.time_zone` and pass it as `"timeZone"`, or call `phrases::live::with_phrase_answer` directly.~~ Done: `EngineConfig.time_zone` (the Mac app passes `TimeZone.current.identifier`) reaches `answerPhrase` as `"timeZone"` through `live::LiveRequest.time_zone`; none (or an unknown name) keeps America/Los_Angeles.
- ~~**HTTP fetch for ECB rates** (from WP-PHRASES): `phrases::rates` takes a `Fetch` trait; a real client (`qcalc-net`) is still needed, or the Mac app keeps `ECBRates.swift` and calls `set_rates`.~~ Done the second way: the Mac app keeps `ECBRates.swift` and hands each fetch to `Engine.setRates` (`App.swift`). A Rust client is only needed for a port without one of its own.
- ~~**One `jsmath`** (from WP-SYMBOLIC/FUNCTIONS): `symbolic/jsmath.rs` (V8 tan) duplicates part of `scientific/functions/jsmath.rs`. Use the latter. `format.rs`, `units.rs` and `precise.rs` still call platform `log10`/`powf`/`cbrt`.~~ Done: `symbolic/jsmath.rs` is gone (its V8 `tan` checks moved to `scientific/functions/jsmath.rs`), and `format.rs`, `units.rs` and `precise.rs` call `jsmath::log10`/`pow`.
- ~~**`sanitize_meas` twice**: `app/history.rs` kept its own copy of `measure::sanitize_meas`.~~ Done: history calls `measure::sanitize_meas`.
- **TS bugs kept for parity, worth fixing in both** (from WP-PHRASES): `constructor`/`__proto__` hit `Object.prototype` in phrase and currency lookups; money amounts ≥ 1e21 are read back wrong from `toFixed`.

- Native only (2026-10-26): `iπ`, `i π`, `eπ` multiply instead of reading as calls `i(pi)`/`e (pi)`, and `exp` takes a complex argument, so `e^(iπ)+1` and `exp(i pi)+1` give 0. The TS app still answers blank / "undefined". Tests: `core/tests/native_fixes.rs`.
