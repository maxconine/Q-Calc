# Integration follow-ups

Items packages handed back that no single package owns yet. Strike them through as they land.

- **Time zone for phrases** (from WP-PHRASES): `Engine::phrase` goes through the generic dispatch with no zone, so everyone gets America/Los_Angeles. Add `EngineConfig.time_zone` and pass it as `"timeZone"`, or call `phrases::live::with_phrase_answer` directly.
- **HTTP fetch for ECB rates** (from WP-PHRASES): `phrases::rates` takes a `Fetch` trait; a real client (`qcalc-net`) is still needed, or the Mac app keeps `ECBRates.swift` and calls `set_rates`.
- **One `jsmath`** (from WP-SYMBOLIC/FUNCTIONS): `symbolic/jsmath.rs` (V8 tan) duplicates part of `scientific/functions/jsmath.rs`. Use the latter. `format.rs`, `units.rs` and `precise.rs` still call platform `log10`/`powf`/`cbrt`.
- **TS bugs kept for parity, worth fixing in both** (from WP-PHRASES): `constructor`/`__proto__` hit `Object.prototype` in phrase and currency lookups; money amounts ≥ 1e21 are read back wrong from `toFixed`.
