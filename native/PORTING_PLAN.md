# Porting plan: the engine in parallel

The foundation (WP-0) is done. The core crate already runs every line through the full TS reader pipeline
(`core/src/evaluate.rs`); each reader that isn't ported yet is a stub in `core/src/stubs.rs` with the TS name and
signature. A work package (WP) replaces stubs with ports and turns its golden areas green.

## How every package works

1. **Own your files.** Create the Rust files listed for your WP. When you port a stub, move it out of
   `stubs.rs` into your module and leave a `pub use` in `stubs.rs`, so `evaluate.rs` needs no edit. Change a
   stub's signature only if the TS forces it, and say so in your commit.
2. **Shared files take one-line, append-only edits:** `core/src/lib.rs` (your `pub mod`), `core/src/golden.rs`
   (your `dispatch` arms, in a block headed with your WP name), `core/golden/baseline.json` (only through
   `GOLDEN_UPDATE_BASELINE=1`). `eval/ops.rs` is split per owner: complex arithmetic goes in `eval/complex.rs`
   (WP-FUNCTIONS) and list/matrix arithmetic in `eval/matrix.rs` (WP-MATRIX). Each is called from one
   marked line in `ops.rs`.
3. **Port faithfully.** Same reader order, same regexes (`jsre::js(pattern, flags)` takes the TS source verbatim),
   same JS number semantics (`jsnum`), the same mathjs grammar (`syntax`). When the TS is quirky, keep the quirk
   and comment it.
4. **Measure.** `cargo test -p qcalc-core --release --test golden -- --nocapture` (≈5 s) prints every area.
   `GOLDEN_AREA=<yours> GOLDEN_FAILS=30` lists your failures. Before committing, run
   `GOLDEN_UPDATE_BASELINE=1` so nobody can regress you, then `cargo fmt`, `cargo clippy --all-targets -- -D warnings`
   and `cargo test --workspace`.
5. **Don't touch the TS** except through the coordinator. If the spec looks wrong, record it in your report.

Blank answers count as passes (an unported reader leaves the line blank, which is right for the blank cases),
so judge progress against `baseline.json`, not against 0%.

## Packages

Fixture counts are from `core/golden/summary.json` (57,004 unique calls). TS paths are `src/engine/…` unless
they start with `lib/` (`src/lib/…`).

### WP-0 Foundation (done)
- **Rust:** `types.rs`, `jsnum.rs`, `jsre.rs`, `syntax/`, `eval/` (Val, ops, builtins: factorial, mod, abs,
  sqrt, cbrt, constants), `scientific/` (all `preprocessAscii`/`preprocessChecked` passes, `evalScientific`,
  `finish`, list/complex display), `precise.rs` (rational twin), `plain_math.rs`, `parens.rs`, `simplify.rs`,
  `format.rs`, `work.rs`, `evaluate.rs`, `golden.rs`, `ffi/`.
- **TS:** `evaluate.ts`, `format.ts`, `parens.ts`, `simplify.ts`, `work.ts`, `plainMath.ts` (minus LaTeX),
  `scientific.ts` (minus the function scope), `precise.ts` (rational part), the mathjs parser.
- **Areas:** `eval.arithmetic` 6909/6911, `engine.format` 9986/9986, `engine.parens` 746/746,
  `engine.simplify` 149/149, `engine.evaluate` 12/12, `eval.blank` 1317/1317, foundation functions of
  `engine.scientific` 1423/1423. The two arithmetic misses belong elsewhere: `(1+1e-15)^(1e15)` needs WP-PRECISE,
  and a 2001-term `1+1+…` is blank in the TS for a reason not yet traced (stack depth in mathjs?).

### Group A: needs only WP-0. All can run at once.

**WP-FUNCTIONS: the scientific function scope**
- Rust: `scientific/functions.rs` (the `TABLE`), `eval/complex.rs`, `angles.rs`, `letters.rs`, `dots.rs`.
- TS: `scientific.ts` `scalarScope`/`builtinScope`/`unitSafe` (trig in both angle modes with `sinCos` snapping,
  inverse and hyperbolic trig, logs, `round`, stats, gcd/lcm on fractions, nCr/nPr with the overflow fallback,
  random/randint (nondeterministic, so the fixtures skip them), `inclusiveRange`, zeta, powmod, `re/im/conj/arg`,
  `clamp`, `quartile`/`quantile`/`corr`), the mathjs functions the engine reaches outside its scope (`sum`,
  `hypot`, `nthRoot`…), complex arithmetic (mathjs Complex), `angles.ts`, `letters.ts`, `dots.ts`.
- Areas: `eval.functions` (4179), `engine.scientific` (4428; `evalScientific`, `preprocessChecked` and others),
  `engine.angles` (7), `engine.dots` (16).
- Note: give each function the JS `fn.length` as `max_args`. `unitSafe` throws on extra arguments.

**WP-PRECISE: truthful digits for everything**
- Rust: `precise.rs` (extend), `precise/real.rs` (new high-precision real: `astro-float` or `dashu-float`).
- TS: `precise.ts` `TWINS`, `kitFor`, `agrees`, `settle`, the 32/48 → 48/80 ladder, `callUser`; also give
  WP-CALCULUS the real type it needs for `closedForm.ts`.
- Areas: `engine.precise` (1356, `verdict`/`truthful`); digits in `eval.functions` and `eval.arithmetic`.
- Coordinate the list of twinned names with WP-FUNCTIONS (read-only for each other).

**WP-UNITS: the unit reader**
- Rust: `units.rs` (replace the stub), `units/table.rs`, `units/settings.rs`.
- TS: `units.ts` (2077 lines: the table, dimensions, prefixes, offset temperatures, `tryConvert`,
  `quantityText`, `unitAlternatives`, `stepPrefix`, `inLadderUnit`, `siQuantity`, `siValue`,
  `UNIT_SETTING_GROUPS`, `sanitizeDefaultUnits`), mathjs units met inside calls (`sin(90 deg)`), `lib/unitSpans.ts`,
  `lib/missingUnit.ts`, `lib/draft.ts` (one call).
- Areas: `eval.units` (8991), `engine.units` (1165), `lib.unitSpans` (23), `lib.missingUnit` (10), `lib.draft` (391).
- The Swift `UnitCatalog` in `macos/UnitSettings.swift` duplicates `UNIT_SETTING_GROUPS`; expose the core's
  version so the UI can drop the copy.

**WP-SYMBOLIC: mathjs's symbolic layer**
- Rust: `symbolic/` (`simplify.rs`, `derivative.rs`, `rationalize.rs`, `print.rs`).
- TS: the mathjs features `calculus.ts`, `isolate.ts` and `system.ts` call: `math.simplify` (3 sites),
  `math.derivative` (2), `math.rationalize` (1), node construction and **mathjs `Node.toString()`** (spacing,
  parentheses, implicit multiplication), which the solve and isolate displays print through.
- Areas: none of its own. It is measured through `eval.solve`, `eval.calculus` and `engine.isolate`. Build a small
  property test against mathjs output first (capture `simplify`/`derivative`/`toString` pairs by adding the mathjs
  module to the recorder).
- Highest risk: mathjs `simplify` is a rule engine with ordering subtleties. Port only the rules the engine's
  inputs reach, and check each against the fixtures.

**WP-MATRIX**
- Rust: `matrix.rs`, `eval/matrix.rs`.
- TS: `matrix.ts` (`looksLikeMatrix`, `matrixAnswer`, display, exact forms), mathjs matrix arithmetic, `det`,
  `inv`, `transpose`, list arithmetic (`[1,2,3] * 2`).
- Areas: `eval.matrix` (422), `engine.matrix` (2), `lib.history` matrix quantities.

**WP-MEASURE: uncertainty and sig figs**
- Rust: `measure.rs`.
- TS: `measure.ts` (`measure`, `literalMeas`, `typedDigits`, `sanitizeMeas`, `formatSig`, `formatUncertain`,
  `formatMeasured`).
- Areas: `eval.measure` (2100), `engine.measure` (23); ± lines in `eval.sheet`.

**WP-CHEM**
- Rust: `chem.rs`, `app/elements.rs`.
- TS: `chem.ts` (formulas, molar mass, balancing via null space, the gas constant), `lib/elements.ts`.
- Areas: `eval.chem` (257), `engine.chem` (67), `lib.elements` (482).

**WP-NOTATION: LaTeX and Typst in and out**
- Rust: `latex.rs`, `typst_input.rs`, `app/typst_math.rs`, `app/radical.rs`, `app/bounds.rs`.
- TS: `plainMath.ts` `latexToAscii` and helpers, `typstInput.ts`, `lib/typstMath.ts`, `lib/radical.ts`,
  `lib/bounds.ts`.
- Areas: `eval.latex` (222), `engine.typstInput` (9), `engine.plainMath` (10), `lib.typstMath` (251),
  `lib.radical` (1881), `lib.bounds` (309).

**WP-PHRASES: natural language, time and money**
- Rust: `phrases/` (`mod.rs`, `time.rs`, `money.rs`, `currency.rs`, `rates.rs`).
- TS: `phrases.ts`, `phraseTime.ts`, `phraseMoney.ts`, `currency.ts` (spec: `phrases.test.ts`,
  `phrases.soulver.test.ts`), the ECB parser and cache from `src-tauri/src/rates.rs`, `lib/phraseLive.ts`, and the
  still-relevant parts of `lib/nativeEval.ts` (`mergeLiveAnswer`'s rules for which answer wins).
- Areas: `engine.phrases` (365, replayed with each record's `now`), `engine.phraseTime` (13),
  `engine.currency` (14), `lib.nativeEval` (657).
- Time phrases need the recorded `now` and the recording machine's time zone. Pass `now` through a parameter,
  never the clock.

**WP-APP-LOGIC: the bar's pure logic from `src/lib`**
- Rust: `app/` (one file per TS module).
- TS: `lib/answer.ts`, `answerForms.ts`, `blankReason.ts`, `chain.ts`, `completion.ts`, `history.ts`,
  `historyShow.ts`, `settings.ts`, `keybinds.ts` (sanitising and chord parsing; key capture stays Swift),
  `onboarding.ts`, `tour.ts`, `identities.ts`, `knownWords.ts`, `paste.ts`, `steady.ts`, `touches.ts`, `undo.ts`,
  `greeting.ts`, `hostSettings.ts`, and the per-keystroke flow in `components/QuickCalc.tsx` (lines ≈540–700:
  chain, isolate-previous, phrase merge, squiggle) as `app/live.rs`.
- Areas: every `lib.*` area not named elsewhere (answer 708, answerForms 72, blankReason 80, chain 62,
  completion 292, history 81, historyShow 24, identities 241, keybinds 89, knownWords 14, onboarding 94, paste 35,
  settings 21, steady 18, touches 12, undo 45).
- `blankReason` takes a callback. The corpus tags it as `{"$fn": {"kind": "evalNonBlank", "options": …}}`; teach
  `golden::dispatch` to rebuild it. Fixtures recorded from tests' own closures stay unreplayable.

**WP-FFI-API: the surface the Mac UI calls**
- Rust: `ffi/src/` (split into `engine.rs`, `records.rs`, `cancel.rs`).
- Scope: the `Engine` object and records of `ARCHITECTURE.md` §4, the 32 MB evaluation thread, async with
  `CancelToken`, `set_rates`, history load/save helpers, a `QCalcCore` Swift package wrapping the generated file
  and the xcframework (`xcodebuild -create-xcframework`) for the UI agent.
- Measured by the Swift test (`scripts/build-mac.sh`), which it extends.
- Starts now against foundation calls, and wires each package's function in as it lands.

**WP-TYPST-RENDER**
- Rust: `native/typst/` (`qcalc-typst`).
- Scope: `ARCHITECTURE.md` §6. Compile `typstDocument` output, render to an RGBA pixmap, warm up, cache,
  embed the fonts, and expose it through a `qcalc-ffi` feature. Report the binary-size cost before turning it on.

### Group B: needs pieces of group A

**WP-SOLVE** (needs FUNCTIONS, SYMBOLIC; UNITS for `unitSolve`)
- Rust: `solve.rs`, `isolate.rs`, `system.rs`, `system_numeric.rs`, `unit_solve.rs`.
- TS: `solve.ts`, `isolate.ts`, `system.ts` (2005 lines), `systemNumeric.ts`, `unitSolve.ts`.
- Areas: `eval.solve` (468), `eval.system` (9), `engine.solve` (45), `engine.isolate` (50), `engine.system` (86).

**WP-CALCULUS** (needs FUNCTIONS, SYMBOLIC, PRECISE)
- Rust: `calculus/` (`mod.rs`, `numeric.rs`, `closed_form.rs`).
- TS: `calculus.ts` (1646 lines), `calcNumeric.ts` (Gauss–Kronrod, limits), `closedForm.ts` (PSLQ),
  `closedForm.worker.ts` becomes the async `closed_form` call, plus `lib/closedForm.ts`'s flow.
- Areas: `eval.calculus` (425), `engine.calculus` (89), `engine.calcNumeric` (20), `engine.closedForm` (41).
- The TS closed-form search is wall-clock limited in places. Make it budget-counted and say where its answers
  differ.

**WP-SUMS** (needs FUNCTIONS, PRECISE)
- Rust: `sums.rs`.
- TS: `sums.ts` (normalisation of Σ/∏ spellings, closed forms, Hurwitz zeta).
- Areas: `eval.sums` (307), `engine.sums` (6).

**WP-GRAPH** (needs FUNCTIONS; the fixtures cover graphs of units and calculus too)
- Rust: `graph.rs`.
- TS: `graph.ts` (intent parsing, compiled sampling, auto y-scale, roots, critical points, ticks, contour
  marching squares).
- Areas: `engine.graph` (2309).

### Group C: integration

**WP-SHEET** (after A and B)
- Rust: `evaluate.rs` (owned by whoever keeps the foundation), `stubs.rs` (deleted once empty).
- Areas: `eval.sheet` (4490). Sheets cross every reader; the remaining failures are the integration bugs.
- Also: the parity sweep. Find every area below 100%, decide with the coordinator whether each miss is a port
  bug or an accepted divergence, and list the accepted ones in `core/golden/DIVERGENCES.md`.

## Order

1. Now, in parallel: FUNCTIONS, PRECISE, UNITS, SYMBOLIC, MATRIX, MEASURE, CHEM, NOTATION, PHRASES, APP-LOGIC,
   FFI-API, TYPST-RENDER. The Mac UI agent can start at the same time against `evaluate` and the JSON escape
   hatch.
2. Start as soon as their inputs land: SOLVE (FUNCTIONS + SYMBOLIC), SUMS (FUNCTIONS + PRECISE), GRAPH
   (FUNCTIONS), CALCULUS (FUNCTIONS + SYMBOLIC + PRECISE).
3. Last: SHEET and the parity sweep, then the Mac app switches over (`ARCHITECTURE.md` §10).

FUNCTIONS and UNITS unblock the most fixtures (≈13k between them), so staff them first if agents are scarce.
