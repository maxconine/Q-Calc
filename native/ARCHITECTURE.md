# Q Calc native: architecture

Q Calc today is a Swift/AppKit shell (`macos/*.swift`) around a WKWebView running the React/TS app in `src/`
(Windows: the same web app in Tauri, `src-tauri/`). The native rewrite moves the engine into Rust and the UI into
SwiftUI/AppKit, so the calculator never starts a Safari WebContent process. Games stay web, in a web view that
exists only while a game is open.

This file is the map for the phases after this one: the engine's crate layout and API, the number model, threading,
maths rendering, the game web view, storage migration and the Mac build. `PORTING_PLAN.md` splits the engine port
into parallel work packages. `MAC_UI_PLAN.md` lists every UI feature and its SwiftUI/AppKit counterpart.

## 1. What the TS engine is

About 32k lines in `src/engine` (half of them tests) plus about 6k lines of engine-like logic in `src/lib`. Three
facts shape the port:

1. **It is a pipeline of text rewrites in front of mathjs.** A line passes through `evaluate.ts` (≈30 readers in a
   fixed order: system solve, isolate, named solve, function definitions, the gas constant, molar mass, chemistry,
   letter splitting, assignment, matrices, equations, Σ, calculus, ±, then `tryPlainMath`). `tryPlainMath` runs
   the unit reader (`units.ts`), then `scientific.ts`: about 40 regex passes (`preprocessAscii`) turn typed maths
   into mathjs syntax, and `math.evaluate` runs it in **`number` mode** (doubles) with the engine's own scope and
   overrides (`divide` by 0 is NaN, `pow` takes odd roots of negatives, `mod` is non-negative). Parity therefore
   means reproducing the regex passes exactly (with look-behind, so `fancy-regex`), mathjs's grammar exactly
   (implicit multiplication binds tighter than `/`; "rule 2" makes `1/2x` = `(1/2)x`), and JS number semantics
   exactly (`toPrecision`, `toFixed`, `String(n)` and `Math.round` all round half up on the exact binary value).
2. **Answers are doubles, with a precise twin.** `precise.ts` re-runs each answer in decimal.js at 32 and 48 digits
   (then 48/80) and replaces the double when both agree and each function agrees with the engine at its inputs.
   That is why `0.1+0.2` shows 0.3 at 16 figures. `closedForm.ts` (PSLQ in decimal.js, in a web worker) upgrades
   calculus answers to closed forms like `π²/6`.
3. **Work is budgeted by counting, never by time** (`work.ts`), so the same input always gets the same answer.
   The Rust port keeps that rule. A few TS tests are wall-clock sensitive (closed-form search, game bots) and flake
   under load, which shows why counting is the right rule.

### Module map and dependency order

Lowest layer first. Arrows mean "imports".

| Layer | TS modules | mathjs / decimal.js features | Rust home |
|---|---|---|---|
| 0 text | `parens`, `dots`, `typstInput`, `work`, `format` | none | `parens.rs`, `stubs::join_dots`, `typst_input.rs`, `work.rs`, `format.rs`, `jsnum.rs` |
| 1 parse/eval | `math` (mathjs `create(all, {number:'number'})`), `scientific` → format, math, work | `parse`, `compile`, `evaluate`, `typed` overrides of divide/pow/mod, `nthRoot`, `zeta`, `gamma`, `factorial`, `combinations`, `permutations`, `complex`, `isUnit`/units inside calls | `syntax/`, `eval/`, `scientific/` |
| 1 precise | `precise` → scientific, mathjs BigNumber parse | decimal.js `sin/cos/asin/atan/exp/ln/log/pow/sqrt/cbrt/sinh…`, `toDecimalPlaces`, `mod` | `precise.rs` |
| 2 units | `units` → format, measure | none (own table, 2k lines) | `units.rs` |
| 2 plain | `plainMath` → parens, typstInput, precise, scientific, units | none | `plain_math.rs`, `latex.rs` |
| 2 measure | `measure` → math, parens, scientific | `parse` (walks the tree) | `stubs::measure` → `measure.rs` |
| 2 simplify | `simplify` (closed forms beside decimals) | none | `simplify.rs` |
| 3 symbolic users | `calculus`, `calcNumeric`, `closedForm`, `solve`, `isolate`, `system`, `systemNumeric`, `sums`, `matrix`, `angles`, `letters`, `chem` | `simplify`, `derivative`, `rationalize`, node constructors and `toString`, matrices (`det`, `inv`, `multiply`, `transpose`), decimal.js (PSLQ) | one module each (see `PORTING_PLAN.md`) |
| 4 orchestration | `evaluate` → everything above; `unitSolve`; `graph` → evaluate | none | `evaluate.rs`; `graph.rs` |
| 5 phrases | `phrases`, `phraseTime`, `phraseMoney`, `currency` → evaluate, format | none | `phrases/` |
| 6 app logic (`src/lib`) | `answer`, `answerForms`, `blankReason`, `chain`, `completion`, `history`, `historyShow`, `draft`, `settings`, `keybinds`, `onboarding`, `tour`, `identities`, `elements`, `knownWords`, `missingUnit`, `unitSpans`, `radical`, `bounds`, `typstMath`, `paste`, `steady`, `touches`, `undo`, `greeting`, `hostSettings` | none | `app/` (one file each) |

**UI, not engine:** React components (`src/components`), `dom.ts`, `bridge.ts`, `platform.ts`, `storage.ts`,
`theme.ts` (applies CSS), `canvasFit.ts`, `typstRender.ts` (drives the WASM renderer), `analytics.ts` (the payload
logic is portable, the sending is UI), `messaging.ts`/`peer*.ts` (the relay transport; Mac already has
`Messages.swift`), `smokeRoom.ts`. **Games, which stay web:** `games.ts`, `ember*`, `pong*`, `connect4*`,
`ultimate*`, `trails*`, `crack`, `equal`, `numbers`, `lobby`, `hello`, `nickname`, `gameShell*`, `gameClock`,
`gameKeys`, `gameSaves`, `gameFullscreen`, `fourTwenty`, `sixtySeven`, `sixtyNine`, plus their panels.

**The existing native evaluators.** `macos/MathEval.swift` is a small recursive-descent parser for + − × ÷ ^ with
`fillParens`; it answers the SwiftUI fallback bar (`OverlayView`) when the web view is unavailable. SoulverCore
(`SoulverEval.swift`) answered natural-language phrases through the `soulver` message handler, merged by
`src/lib/nativeEval.ts` (`mergeLiveAnswer` lets the native answer win only for `looksLikeNaturalLanguage` inputs,
never Σ, calculus or molar mass). **That path is removed on main:** phrases are now answered in TS
(`phrases.ts`, `phraseTime.ts`, `phraseMoney.ts`, specified by `phrases.soulver.test.ts`), and ECB rates are fetched
natively by `macos/ECBRates.swift` into `~/Library/Application Support/Q Calc/rates.json` in the shape of
`src-tauri/src/rates.rs`. The native core is designed without SoulverCore: `phrases.ts` is the spec, and the rates
fetcher moves into Rust (§9). `MathEval.swift` goes away once the core is linked; the fallback bar calls the core.

## 2. Crate layout

```
native/
  Cargo.toml              workspace (edition 2024, pinned toolchain in rust-toolchain.toml)
  core/                   qcalc-core: the engine. Pure Rust, no I/O, no platform code, no unsafe.
    src/
      types.rs            LineResult, Value, Meas, EvaluateOptions… (serde camelCase = the TS JSON)
      jsnum.rs            JS number→text rules (String, toPrecision, toFixed, toExponential, Math.round)
      jsre.rs             fancy-regex helpers; `js()` compiles TS regex sources verbatim (ASCII \d \w \b)
      syntax/             mathjs tokenizer, parser and AST (Node mirrors MathNode)
      eval/               mathjs number-mode evaluation: Val, ops, builtins (the engine's scope)
      scientific/         preprocessAscii/preprocessChecked passes, evalScientific, finish; functions.rs (WP)
      precise.rs          the truthful-digits twin (exact rationals now; high-precision reals in WP-PRECISE)
      plain_math.rs       tryPlainMath, normalizeMathText (thousands, × 10^n, pasted spaces, parens)
      evaluate.rs         evaluateSheet/evaluateLine: the reader pipeline and sheet state
      format.rs simplify.rs parens.rs latex.rs typst_input.rs units.rs work.rs
      stubs.rs            every pipeline stage a later package owns, with the TS signature and a "not mine" body
      golden.rs           fixture replay dispatch (TS function name → Rust call)
    golden/               JSONL fixtures by area, summary.json, baseline.json (the ratchet)
    tests/golden.rs       the parity runner
  ffi/                    qcalc-ffi: UniFFI surface (staticlib for the app; cdylib only for bindgen)
  golden-capture/         vitest config + recorder that writes the fixtures from the TS engine
  swift-test/main.swift   links libqcalc_ffi.a through the generated bindings, both architectures
  scripts/build-mac.sh    per-arch static libs, universal lib, Swift bindings, Swift test
```

Planned additions: `typst/` (`qcalc-typst`, rendering maths, §6) and `net/` (`qcalc-net`, the rates fetch, §9),
both linked into the same static library through `qcalc-ffi` features, so `qcalc-core` stays pure and compiles
for any target (Windows, wasm for a web fallback, CI on Linux).

### FFI: UniFFI

Considered:

- **Hand-rolled C ABI returning JSON.** The smallest and most universal option: every language can call
  `char* qcalc_evaluate(const char*)`. But every type is untyped text on the Swift side (hand-written `Codable`
  mirrors of ~40 records that drift silently), memory ownership of returned strings is manual `unsafe` on both
  sides, errors are conventions, and async work or cancellation has to be built from scratch.
- **swift-bridge.** Typed, with good Swift ergonomics, but it targets Swift only. Windows would need a second
  binding layer later, and it is less mature for async and for objects shared across threads.
- **UniFFI (chosen).** It generates typed Swift records, enums, errors (as Swift `throws`) and objects
  (`Arc`-backed, thread-safe), and supports async functions bridged to Swift `async`. The same crate generates
  Kotlin and, through the maintained `uniffi-bindgen-cs`, C# for a WinUI front end, or Windows can call the Rust
  directly if its UI is Rust. Proc-macro mode keeps everything in Rust. No UDL file is needed.

The surface stays **coarse**, about one call per keystroke, and every typed record has a **JSON escape hatch**:
`evaluate_json` takes and returns the TS engine's exact shapes, and `LineAnswer.json` carries the full
`LineResult`. The UI can then use a field the typed records don't expose yet without an FFI change, and the
golden fixtures and the FFI speak the same JSON. The only `unsafe` code is UniFFI's generated scaffolding in
`qcalc-ffi`. `qcalc-core` has `unsafe_code = "forbid"`.

## 3. Number model

Parity first: **the value of an answer is an `f64`**, computed with mathjs-in-number-mode semantics, exactly as
the TS does. Exactness lives beside it, not instead of it:

| Concern | TS | Rust |
|---|---|---|
| plain value | JS double | `f64`; `eval::Val::Num` |
| truthful digits | decimal.js twin at 32/48 (then 48/80) digits, `verdict`/`truthful` | `precise.rs`: exact `BigRational` twin for + − × ÷, integer powers, factorial, mod, abs and perfect roots (done); WP-PRECISE adds a high-precision real backend (`astro-float` or `dashu-float`, pure Rust) for the transcendental twins, the two-precision ladder, `agrees` and `settle` |
| exact closed forms beside decimals | `simplify.ts` (pattern match on the double) | `simplify.rs` (done) |
| closed forms of calculus answers | `closedForm.ts` PSLQ over decimal.js | WP-CALCULUS, on WP-PRECISE's real type |
| fractions | `formatAsFraction` (search over denominators) | `format::format_as_fraction` (done) |
| uncertainty / sig figs | `Meas {sig, dp, unc, uncDigits}` walked by `measure.ts` | `types::Meas`; WP-MEASURE |
| units | own table: dimension vectors, SI factors, prefixes, offsets | WP-UNITS: `Quantity { si: f64, dim: [i8; N], unit_id }` |
| complex | mathjs Complex, shown only when an `i` was typed | `Val::Complex { re, im }` (finish/format done; arithmetic in WP-FUNCTIONS) |
| matrices / lists | mathjs arrays and matrices | `Val::List` (display done; ops in WP-MATRIX) |
| symbolic | mathjs `simplify`, `derivative`, `rationalize`, node `toString` | WP-SYMBOLIC over `syntax::Node` |

A separate arbitrary-precision decimal number type for every answer was rejected. It would change answers, and
parity with 21k tests is the requirement. The precise twin is where extra precision pays off, and that is the
same place the TS uses it.

## 4. The API the UI calls

The foundation exposes `core_version`, `default_eval_options`, `evaluate` and `evaluate_json`. WP-FFI-API grows
this into an `Engine` object. The target surface, in Rust (UniFFI) and as Swift sees it:

```rust
#[derive(uniffi::Object)]
pub struct Engine { /* rates, caches (callable names, closed forms), its evaluation thread */ }

#[uniffi::export]
impl Engine {
    #[uniffi::constructor]
    pub fn new(config: EngineConfig) -> Arc<Self>;                  // config: is_windows, locale, data_dir

    /// One keystroke: the sheet, the live answer as the bar shows it, and why a line is blank.
    pub fn live(&self, request: LiveRequest) -> LiveAnswer;
    /// Enter: the committed line as a history row, plus what the input becomes (chain, solve insert…).
    pub fn commit(&self, request: LiveRequest) -> CommitResult;
    pub fn evaluate(&self, lines: Vec<String>, options: EvalOptions) -> Vec<LineAnswer>;
    pub fn evaluate_json(&self, request: String) -> Result<String, CoreError>;

    pub fn answer_forms(&self, source: FormSource) -> Vec<AnswerView>;    // Tab cycle
    pub fn completion(&self, text: String, names: CompletionNames) -> Option<String>;
    pub fn blank_reason(&self, text: String, options: EvalOptions, names: CompletionNames) -> Option<Span>;
    pub fn spans(&self, text: String) -> Vec<TextSpan>;                     // units, known words, radicals, bounds
    pub fn graph(&self, input: String, options: GraphOptions) -> Option<GraphResult>;
    pub fn typst_source(&self, expr: String, answer: String, solved_for: String) -> Option<String>;

    /// The closed-form search the TS runs in a worker; cancelled by dropping the token or calling cancel().
    pub async fn closed_form(&self, job_json: String, cancel: Arc<CancelToken>) -> Option<String>;
    pub async fn live_async(&self, request: LiveRequest, cancel: Arc<CancelToken>) -> Result<LiveAnswer, CoreError>;

    pub fn set_rates(&self, rates_json: String) -> Result<(), CoreError>;  // rates.rs shape
    pub fn context_from_history(&self, rows_json: String) -> EvalContext;  // variables, measures, quantities, ans
    pub fn normalize_history(&self, rows_json: String) -> String;          // load/migrate/slim history rows
    pub fn identity_sheet(&self, id: String) -> Option<IdentitySheet>;
    pub fn elements(&self) -> Vec<Element>;
    pub fn hint(&self, facts: HintFacts) -> Option<String>;                // onboarding / tour text
}

#[derive(uniffi::Record)] pub struct LiveRequest { pub text: String, pub options: EvalOptions,
    pub context: EvalContext, pub chained: bool, pub now_ms: i64 }
#[derive(uniffi::Record)] pub struct LiveAnswer { pub display: String, pub pretty: String, pub exact: Option<String>,
    pub dual_label: String, pub number: Option<f64>, pub unit: Option<String>, pub kind: AnswerKind,
    pub blank: Option<Span>, pub completion: Option<String>, pub solve: Option<SolveInfo>,
    pub closed_form_job: Option<String>, pub time_letters: Option<TimeLetters>, pub json: String }
```

```swift
let engine = Engine(config: EngineConfig(isWindows: false, locale: "en_US", dataDir: supportURL.path))
let live = engine.live(request: LiveRequest(text: field.text, options: settings.evalOptions,
                                            context: history.context, chained: chain, nowMs: now))
answerView.text = live.pretty
if let job = live.closedFormJob {
    let token = CancelToken()
    upgrade = Task { if let exact = await engine.closedForm(jobJson: job, cancel: token) { … } }
}
```

The foundation types (`EvalOptions`, `LineAnswer`, `AnswerKind`, `UserFn`, `CoreError`) are already in
`ffi/src/lib.rs` and generated into Swift by `scripts/build-mac.sh`.

## 5. Threading and cancellation

- **Per-keystroke work is synchronous and fast.** In the Swift smoke test, 21 keystrokes take about 4 ms on
  either architecture. The UI calls `live` on one serial background queue, tags each request with a generation
  number, and drops results for stale generations. The main thread never evaluates.
- **Stack size.** Deep inputs recurse: the parser rejects nesting past `MAX_NESTING` (512), but parsing and
  evaluating near that depth still needs several MB of stack (the golden fixtures include such inputs, and the
  default 2 MB test thread overflowed on them). Dispatch worker threads have 512 KB stacks, so the `Engine` owns its own evaluation thread with a
  32 MB stack (`std::thread::Builder::stack_size`) and FFI calls hop to it. Swift never calls the engine on a
  dispatch thread directly. The golden runner uses a 256 MB thread for the same reason.
- **Long work** (closed-form PSLQ, big graph sampling, numeric system solves) runs as UniFFI `async` functions on
  the engine's worker pool, with a `CancelToken` object (`Arc<AtomicBool>`). Budgets stay counted (`work.rs`); the
  token is checked inside `work::spend`, so every budgeted loop is cancellable without timers. When the user types
  again, the Swift side cancels the previous `Task` and calls `token.cancel()`.
- **Determinism.** The same input always gives the same answer, as now. Nothing reads the clock except phrases,
  which take `now_ms` from the request.

## 6. Typeset maths

Today `typstMath.ts` turns the input into Typst source (`typstDocument`), and `typstRender.ts` compiles it with
typst.ts (the Typst compiler built to WASM) into SVG inside the web view.

Typst is written in Rust, so the native app uses it directly. A `qcalc-typst` crate depends on `typst`,
`typst-render` and the math fonts from `typst-assets` (New Computer Modern Math). It compiles the
`typstDocument` source in memory and lays the page out as vector paths in page points (`TypstRenderer.paths`;
an RGBA pixmap, `render`, is there too). The Mac app draws the paths in a SwiftUI `Canvas` in the text colour
(`Sources/Bar/MathText.swift`), so light and dark need no second compile and nothing is rasterised twice.
Typst's `comemo` caching makes keystroke-to-keystroke recompiles cheap. The renderer warms up on its own queue at
launch (the TS `warmupTypst`); the row shows exactly when the web app's does (`typstShown`: the setting, no
command panel, `typstPreviewUseful`), and mid-edit input Typst rejects keeps the last picture, dimmed.

Cost: typst adds about 27 MB per architecture to the stripped binary (measured by
`native/typst/scripts/measure.sh`; the web preview's typst.ts WASM it replaces was 29 MB) and heavy compile time.
It sits behind the `qcalc-ffi` feature `typst`, which `native/mac/build.sh` turns on
(`QCALC_FFI_FEATURES=typst` for `native/scripts/build-mac.sh`). The Typst *source* generation (`typstMath.ts`, `radical.ts`, `bounds.ts`) is engine logic in
both designs (WP-NOTATION), and the copy-as-Typst setting (`typstCopy`) uses it directly.

## 7. Games: an on-demand web view

The calculator never creates a `WKWebView`. A game command (`pong`, `ember`, `connect 4`, `trails`, `ultimate`,
`crack`, `equal`, `numbers`) or one of the 420/67/69 answers creates one `WKWebView` in a `GameWindowController`
that loads a games-only bundle: a new Vite entry `games.html` that mounts `GameShell` with the panel named in
the URL fragment (`games.html#pong`). Closing the game removes the web view and releases its configuration. The
WebContent process exits with it, and Activity Monitor shows no Safari helper while calculating.

- The web view keeps today's game bridge messages, moved out of `Overlay.swift` into the game controller:
  `game`, `gameSave` (saves stay in `UserDefaults` `qcalc.gameSaves`, injected on load as now),
  `gameFullscreen`, `escapeLayer`, `nickname`, `copy`, and the relay transport `peer*` (still JS WebSocket).
- Window behaviour (`setGameWindow`, `dressForGame`, fullscreen, letterbox, Esc layers) is the same AppKit code
  now in `OverlayController`.
- The 420/67/69 effects are short animations over the bar. They load `games.html#smoke` (or `#arms`, `#fold`)
  in a transparent, click-through web view that tears itself down when the animation ends. WebContent startup
  (≈150–300 ms) delays the first frame. If that shows, these three are small enough to redraw natively later.

## 8. State and migration

| State | Today (Mac) | Native |
|---|---|---|
| Settings | `UserDefaults` `qcalc.*` (`AppSettings` in `QCalcApp.swift`), pushed into the page | unchanged keys; Swift reads them and builds `EvalOptions`. Sanitising (`mergeSettings`, `sanitizeDefaultUnits`, `sanitizeKeybinds`, `clampSigFigs`) moves to the core so Windows shares it |
| History | page `localStorage` in a **non-persistent** `WKWebsiteDataStore`, so it doesn't survive a relaunch | `~/Library/Application Support/Q Calc/history.json`, an array of the TS `HistoryRow` JSON (`{"version":1,"rows":[…]}`), written through the core's `persistableHistory`/`slimHistoryRow` (same caps: `MAX_HISTORY`, `MAX_HISTORY_JSON`); read through `normalizeHistoryRow`, which already migrates legacy rows (`latex` field, old ids) |
| Draft | memory + `localStorage` | memory + `UserDefaults` `qcalc.draft` (`{expr, savedAt}`), restored by `shouldRestoreDraft` |
| Onboarding / tour | `UserDefaults` `qcalc.onboarding` | unchanged |
| Messaging | `UserDefaults` `qcalc.messaging.*`, `Messages.swift`, `MessageInbox.swift` | unchanged (already native) |
| Game saves, nickname | `UserDefaults` `qcalc.gameSaves`, `qcalc.nickname` | unchanged; injected into the game web view |
| Rates | `ECBRates.swift` → `Application Support/Q Calc/rates.json` | same file; read by the core (`set_rates`) and later fetched by `qcalc-net` |
| Bar place/width | `UserDefaults` `overlayPositions`, `qcalc.overlayWidth`, `qcalc.lastHidden` | unchanged |

Because the Mac's web storage was never persistent, the Mac has nothing to migrate out of `localStorage`.
Versioned JSON files with a `version` field leave room for later schema changes. Windows comes later: Tauri's
`store.rs` JSON stores are read once on first launch of the native build and rewritten in the same JSON shapes.

## 9. Currency rates

`src-tauri/src/rates.rs` is the reference: the ECB daily XML (`eurofxref-daily.xml`), strict parsing (one dated
cube, plain decimal rates, 3-letter codes), 7-day staleness, a JSON cache. WP-PHRASES ports the parsing,
staleness and conversion into `qcalc-core::phrases::rates` (pure: XML or cache text in, `CurrencyRates` out).
The fetch goes in `qcalc-net` (`ureq` with native TLS: Security.framework on Mac, SChannel on Windows, so no
OpenSSL), which `qcalc-ffi` exposes as `async fn refresh_rates(data_dir)`. Until then `ECBRates.swift` keeps
fetching and passes the JSON to `Engine::set_rates`.

## 10. Mac build changes (`macos/build.sh`)

- **New prerequisite:** the Rust toolchain from `native/rust-toolchain.toml` (rustup installs it on first
  `cargo` use) with both Apple targets. `build.sh` checks for `~/.cargo/bin/cargo` the way it checks for `swiftc`.
- **Before `swiftc`:** run `native/scripts/build-mac.sh --no-test`, which builds `libqcalc_ffi.a` per architecture
  (with `MACOSX_DEPLOYMENT_TARGET=12.0`, the native app's floor) and generates `qcalc_ffi.swift` plus its C header and
  module map.
- **`swiftc`:** add `native/build/swift/qcalc_ffi.swift` to the sources and
  `-Xcc -fmodule-map-file=native/build/swift/qcalc_ffiFFI.modulemap -I native/build/swift`, and link
  `-L native/build/<arch> -lqcalc_ffi`. Each per-arch app links its own thin library, matching the per-arch
  packaging. `MathEval.swift` and `math_test.swift` are deleted.
- **Web assets:** only the games entry (`games.html`), built on its own by `native/mac/games.vite.config.ts`.
  `Contents/Resources/web` holds only the game bundle; no calculator page, chunk or WASM ships, and no script
  runs outside a game's web view (the app logic that ran in JavaScriptCore is in the core now).
- **Frameworks:** SoulverCore is already gone. Sparkle is unchanged. WebKit stays linked (system framework, no
  cost until a game creates a view).
- **Signing:** unchanged. The engine is a static library inside the main binary, so there is no new bundle to sign.
  `strip -x` already drops local symbols. Thin LTO and `codegen-units = 1` keep the library small.
- **CI** installs rustup plus the two targets and runs `cargo fmt --check`, `cargo clippy -D warnings`,
  `cargo test` (including the golden runner) and `scripts/build-mac.sh` (which runs the Swift test on both
  architectures; x86_64 under Rosetta).

## 11. Golden parity

The TS engine is the spec, so its own answers become the Rust port's tests.

- **Capture** (`native/golden-capture/capture.sh`, about a minute): runs the app's vitest files (games excluded)
  plus the generated corpus in `golden-capture/corpus/*.golden.ts` under `golden-capture/vitest.config.ts`. Its
  setup file replaces each engine module (`src/engine/*`, the engine-like `src/lib/*`) with a copy whose
  exported functions record **top-level** calls: calls from tests, not the engine's calls to itself. Each call
  is recorded with its arguments, its result or its throw, and the inner engine functions that answered it. Each
  call runs twice; any answer that differs between the runs (random, the clock, caches) is marked `nondet`.
  `merge.mjs` deduplicates the calls, sorts `evaluateLine`/`evaluateSheet` into `eval.*` areas by which reader
  answered (arithmetic, functions, units, solve, calculus, matrix, chem, sums, measure, latex, sheet, blank), and
  files other calls by module (`engine.format`, `lib.answerForms`…). The raw calls stay in `golden/.raw`
  (gitignored), so `node native/golden-capture/merge.mjs` can re-sort them without recapturing. `npm test` is
  untouched: it doesn't load this config.
- **Replay** (`cargo test -p qcalc-core --release --test golden -- --nocapture`, about 5 s): `golden::dispatch`
  maps each TS function name to its Rust port; unported functions count as *unported*, not failures. Answers
  compare as JSON with displays exact and doubles within 4 ulps. The run prints a per-area table (also written to
  `golden/STATUS.md`). `GOLDEN_AREA=eval.units` filters by area, `GOLDEN_FAILS=20` prints failures, and
  `GOLDEN_UPDATE_BASELINE=1` raises the floors in `golden/baseline.json`. The test fails only if an area drops
  below its floor, so the port only ratchets forward.
- **Regenerate** after changing the TS engine or the corpus: `zsh native/golden-capture/capture.sh`, then rerun
  the replay and commit the changed fixtures with the change that caused them.
- Note that blank answers count: an area such as `eval.measure` "passes" its blank cases before anything is
  ported. Read an area's progress against its baseline, not as an absolute.
