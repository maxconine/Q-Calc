# Mac changes to port and test on Windows

A running list, kept by whoever changes the Mac app. How to work through it: `PORT_RUNBOOK.md`. Newest additions go at the end of their section. When a line is done and tested on Windows, delete it.

**Before you start:** this work may not be committed or pushed yet. Check that `native/core/src/app/native_sheets.json` and `native/core/src/app/native_typst_cells.json` exist on your branch. If they don't, ask for the Mac work to be committed first.

**Mac-to-Windows key names:** ⌘ is Ctrl, ⌃ is Ctrl unless Windows already binds that chord, and ⌫ is Backspace.

**Spec:** `native/mac/PARITY.md` and the Mac sources named below. The rules are in the shared core, so don't copy them into Windows.

**Status of this list:** written from the Mac, then ported and run on Windows on branch `win-mac-port` (6 Nov 2026), and again for 3.0.2 on branch `win-port-302` (8 Nov 2026), with the linear algebra list folded in and done. New Mac changes go below as before.

**Releases (3.0.2):** a `v*` tag now releases the Mac only, and Windows ships by a hand run of the Windows workflow. See the top of `RELEASE.md`.

## Checks to run first

```
cargo test -p qcalc-core
cargo test -p qcalc-ffi --features typst --test native_sheets_typst
cargo test -p qcalc-win
```

The `eval.blank` golden floor is now 1311, because two recordings of `∫ x dx` and one of `integral of x^2` expect a blank and they now answer. The `eval.units` floor is now 8988: two recordings want `2 ft + 3 in` as `2.25 ft` (it now answers `2 ft 3 in`), and one wants `1/2 in + 3/8 in` as `0.875 in` (now `7/8 in`). Since 11 Nov 2026 it is 8756: 232 recordings of a conversion to a compound unit (`… to ft * lbf`, `… to kg/liter`) want the answer's `quantity` empty, so `ans` was lost after them; it now keeps the quantity. If Windows passes a different count, tell the Mac side rather than raising it.

On Windows (branch `win-port-302`) all three pass, with the golden floors as set on the Mac (the matrix floor `eval.matrix` 420 too). The `commands.rs` test now expects `idt summation` to open `identities: Summation` at section 0.

## 1. Comes from the core: just test it

- **History file: notes and complex solves kept, an unreadable file kept aside** (ffi `history.rs` `kind_of` + `is_empty_history`, ffi `engine.rs` `load_history`, core `app/history.rs` `sanitize_solve`): a note row lost its `note` kind on the next ↵ and on every relaunch (it came back as a plain row with an empty answer, cut to 240 characters); an `x^2 = -1` row (outcome `complex`) lost its solve when saved; a history.json that reads as no rows (damaged, a `null` row, a newer shape) is copied to `history.unread.json` before the next save overwrites it. Windows loads through the same `Engine::load_history`, so: save a note, ↵ another line, restart, the note is still a note; `x^2 = -1` then restart draws `x = ±i` full size. Test: `ffi/tests/history_migration.rs`.

The rest is done. (`dx` touching the integrand, halves powers as roots and the faster intersections were checked in the app on 8 Nov 2026, with the linear algebra tables.)

- **Imaginary roots: the row's copy button** (core `solve.rs` `ComplexRoots`; 21 Nov 2026 the answers, the live `x = ±i`, the saved `complex` outcome and the row after a relaunch were checked on Windows): left, check the row's copy button copies `±i` after `x^2=-1` ↵ (with the tape's copy work).

## 2. Edited on Windows but never run: test it

## 3. Not on Windows yet: implement it

### Games: slots, VoiceOver (20 Nov 2026)

Left on Windows: the slots reels are a button with the line as its value (games/slots.rs `parts`), but uia.rs has no Invoke pattern, so a reader can't press it; space or ↵ spins. Results are raised as UIA notifications, not yet heard with Narrator itself (the tree was read with a UIA client).

### Already ported

The stacked fractions round (TeX sizes, Desmos keys, grown brackets, stacked history rows, answers shrinking as one) and the newer graph round were ported for 3.0.2 (branch `win-port-302`, 8 Nov 2026). What stays different on Windows:

- **Stacked: a `(` typed inside a slot** doesn't bring its `)`, so `2^10+1/(x/3)+(1/2)/4` typed key by key nests differently from the Mac; typing `)` to step out gives the intended layout.
- **Stacked: ↓ pressed within a few ms of typing**, before the next paint, can land on the wrong side of a slot (the places come from the last paint).

- **Graph: pinch and two-finger gestures:** Windows takes a touchpad's pinch as Ctrl + wheel (it zooms about the pointer, as before); a two-finger swipe arrives as the wheel. Shift + wheel pans.

### QA round, 20 Nov 2026: not on Windows yet

Not ported: these landed under Already ported when this file was rebuilt. Lines marked core or "Core only" need only testing as in section 1; Mac lines need the same on Windows.

- **Message cards** (Mac `mac/Sources/Messages/CardRules.swift`, `Messages/ShareViews.swift`; QA item 25): on Windows the rules are in (card_rules.rs `kept`: a card being answered stays past three; a failure note goes when the reply is edited; a delivered reply counts for Tab; a thread line's `Copied` restarts its 1.2 s). Left: the cards have no UI Automation nodes on Windows (a reader hears only the announcement, which already says `max shared their clipboard` / `max sent a.pdf`), so the VoiceOver labels and actions have nothing to port to until the cards get them.
- **Graph settings fields have names** (Mac `Bar/GraphSettings.swift`; QA item 20): in graph settings the range fields read `x from` / `x to` / `x step`, the axis names `x axis name` / `y axis name` (`z` in 3D), the extra equations `Equation 2`… and their buttons `Remove equation 2`…. Windows: the graph panel and its settings card have no UI Automation parts yet (a11y.rs exposes the line, answer, hint and tape rows), so these names need that first. The live answer read out 0.7 s after typing stops (`equals 1,024`) is done on Windows (a11y.rs `speak_later`, a polite UIA notification).
- **Settings window: the keybinds page in one view** (Mac `macos/SettingsWindow.swift`; QA item 18): Windows' Keybinds page lists all 13 rows but scrolls in the 620-DIP settings window (rows 9-13 are below the fold at first). Every row is reachable, each key button reads `<action>: <key>` to UI Automation, Ctrl+C/V/X/A/Z/Y work in the Messages name field and an unusable key says "That key can’t be used. Try a letter, number or F-key" (done); left: whether the page should fit without scrolling (tighter rows or a taller window on that page).
- **VoiceOver names on tonight's controls** (Mac `mac/Sources/Bar/BarView.swift` `SolveRoots`/`RootButton`/`AnswerButton`/`AnswerCell`/`RowCopyButton`, `Bar/GraphPanel.swift` head; dev `axtree:<file>` in `mac/Sources/App/DevSnapshot.swift` (the bar's accessibility tree read in-app, no permission); test `mac/scripts/test-voiceover.sh`, a re-signed copy; QA item 110): the live roots' `x =` lead read as "x =" (now `Copy all the solutions`, as the tape's), a dual's halves both read `Answer …` (now `Exact value …` / `Approximation …`, bar and tape), a tape root `Answer 1` (now `Solution 1`), the copy button only `Copy` (now `Copy answer` / `Copy note`, `Copied` after), the graph's `reset` its word (now `Reset view`); an answer's unit was read again after its label, the live roots' commas read, and the graph's export/settings icons now say when open (selected). Windows: the same UI Automation names on those controls. Windows status: a gap. The bar's UI Automation tree (a11y.rs `bar_tree`) has the line, the answer as one text, the hint and the tape rows (`expr = answer`); the live roots, a dual's halves, a copy button and the graph head aren't separate parts there, so their names need parts of their own first.
- **Typeset drawing: the tape's ∫/Σ/Π limits** (Mac `mac/Sources/Bar/RadicalText.swift` `bounds` run; QA item 92; the preview's bare fractions and raised digits are checked on Windows): the tape showed `∫_0^1 x^2 dx` and `Σ_(n=1)^10 n^2` as typed, now the sign with its limits small beside it (upper over lower) as the field draws them. Windows: draw the tape row's ∫/Σ/Π limits the same way (`history.rs`; the empty bar's examples have no such line).
- **Live drive leftovers: the tape giving way under the 640 pt cap, a polar curve's x ticks** (QA item 38; the `$sqrt(2$` preview is checked on Windows; Mac `mac/Sources/Bar/BarView.swift` `fitTape`/`TapeHeightKey` with `Theme.barMaxHeight` (640, also `OverlayController.overlayMaxHeight`): a tall panel or card under the line (matrix steps, then a message card) pushed the bar past its cap and the window cut off the top tape row and the card's reply field; the tape now gives up whole rows (one kept) and scrolls, keeping the newest or picked row in sight, and grows back when the card goes; `Bar/GraphModel.swift`: a polar curve (`r = θ`, `r = 1 + cos(θ)`, a contour whose label starts `r =`) gets plain x ticks, not π's, since its x is a distance). Windows has no such height cap on the bar yet (`window.rs` `Bar::layout`). To test on Windows: four tape rows open, `inv([1 2; 3 4])`, Ctrl+R, then a message card (the reply field whole, the tape shorter); `graph r = θ` (x ticks −60, −40…, `graph sin(x)` keeps π's).
- **Settings audit: the settings window's controls** (QA item 53; core test `core/tests/settings_effects.rs` passes on Windows): on 21 Nov 2026 the stored settings were checked through to the answer on Windows by writing `settings.json` (`sin(30)` in rad, `asin(0.5)` in rad, `1 000 + 1` with spaces on, fractions, 10 digits, `2 m + 3 ft` with length in mm, sig figs). Left: try each choice from the settings window itself and check it lands in the same keys.
- **Spoken ∫/Σ limits on tape rows** (core `graph_native.rs` `Spiral`, `graph.rs` `build_native_graph`; Mac `mac/Sources/Bar/TapeRules.swift` `spokenLimits`; QA item 97; the core half, `20 °C + 5 K` is 25 °C (77 °F with the default units) and `\frac{1}{2} + \sqrt{x}` squiggles only `x`, checked on Windows 21 Nov 2026): `graph r = θ` / `r = e^(0.1θ)` draw over three turns from 0 as an implicit curve whose segments are the traced spiral; VoiceOver reads a tape row's `∫_0^1` as "integral from 0 to 1" ("sum from n = 1 to 10", "product from …"). Windows: the spiral draws in the graph panel (checked 21 Nov 2026); left: give the tape row's UIA name the same spoken limits (port `spokenLimits`).

- **Formula sheets window: search, Related links, keys** (QA item 17): ported (`sheet_match.rs`, `sheets.rs` `book_key`); left on Windows: the explanation's text can't be selected, so Ctrl+C there copies nothing (Ctrl+C/X/V/A/Z work in the filter), and the filter has no Ctrl+Shift+Z redo.
- **Leaks and long runs** (Mac `macos/IdentityWindow.swift`, `macos/PeriodicWindow.swift` `animationBehavior = .none`; dev `threads:<file>` in `mac/Sources/App/DevSnapshot.swift`; check `mac/scripts/test-leaks.sh`; QA item 51): driven on a re-signed copy, show/hide 300×, 1,000 committed lines, 50 graphs (2D and 3D), 50 games and 50 sheet/periodic opens. Memory levels off (28 MB → 74 MB by 500 lines, flat to 1,000; the first graph adds ~60 MB once), no timer wakeups while hidden, no WebContent left after games. Each sheet or periodic-table open and close left a thread behind for good, parked in AppKit's threaded window zoom animation (`_NSWindowTransformAnimation`); 64 filled the user-interactive dispatch pool. Those two windows now open and close without the zoom. On Windows the sheets and periodic table are done (50 opens and closes of each, threads and handles flat, branch of 21 Nov 2026); the rest of the long run (show/hide, 1,000 lines, graphs, games) is not driven here.

Open questions for the Mac side:

- **Matrix grid gaps left on Windows:** Ctrl+G and Ctrl+R are fixed chords (the core's keybinds have no matrix actions; a user binding on the same chord wins); the grid doesn't scroll sideways (8 columns need about 590 px); no UI Automation for the cells yet. (A click in a cell now puts the caret under the pointer.)

### Sheet data, for reference

All of it is generated by `native/scripts/gen_native_sheets.py`, so edit that and not the JSON. Named formulas are sheets with `names`, and their ids start `formula: `. Parts have ids like `continuum mechanics: Beams`.

## 4. How to test the sheets

- Try a small screen (the sheets were tried on a 4K screen at 150% only).
- **Dynamics body pictures drawn as in Mitiguy's book** (data only: `scripts/gen_native_sheets.py` `Pic.axes`, `Pic.cm`, new `Pic.body`, every `pic_*`; user request 20 Nov 2026): the unit vectors are bold `hat(bold(b))_x/y/z` arrows with larger heads, b̂_z coming toward the reader; the mass centre is the textbook's symbol (a circle, top-right and bottom-left quarters filled) labelled `B_cm` under it; each body has its name, an upright B in a circle, with an arrow to its outline; the hoop is drawn as a thin double ring. Uses `circle(...)` with `align` inside `place`, still opaque black = ink. Test: open `dynamics` (Inertia sections) in light and dark; `ffi/tests/native_sheets_typst.rs` (`--features typst`) passes.
- **Dynamics body pictures, second pass** (data only: `scripts/gen_native_sheets.py` `Pic` and every `pic_*`; user request 20 Nov 2026): every body redrawn from the book's pp. 301–305. b̂_z on flat bodies is the book's heavy counter-clockwise curl round the mass-centre symbol, its big head at the left pointing down; radius lines run from the centre through the symbol and out its far side; leader arrows to Ⓑ stop clear of the outline (`body(ow=…)`); dimensions carry the book's ticks and fractions (3h/4, 2r/π, 3r/8, a/3); the box, triangle and tetrahedron use the book's view and hatching. Pictures still use only opaque black (ink) and see-through greys. Test: `ffi/tests/native_sheets_typst.rs` (`--features typst`); look at `dynamics` (Inertia sections) in light and dark.
- **Signals, transforms and linear algebra explanations audited** (data only: `core/src/app/explanations/signals.json`, `transforms.json`, `analysis.json`; 20 Nov 2026): a wrong number fixed (CT time-shift example said −0.6915j, it is −0.6909j); the integration-property example now states the coefficients; rows shared by two tables (δ(t) ↔ 1 for Fourier and Laplace, δ[n] ↔ 1 for DTFT and z, 1 ↔ 2πδ(ω) for CT and DT, the vector-angle row for linear algebra and machine learning) now explain both uses; conditions (ROCs, a ≥ 0, invertibility, square A) added where they were empty; 16 new Try lines. The sheets themselves are unchanged. Test: `cargo test --test explanations` in `core`.
- **Euler, complex numbers, Taylor, vector calculus, precalculus, differential equations and numerical methods explanations audited** (data only: `core/src/app/explanations/calculus.json`, `signals.json`, `applied-math.json`, `science.json`; search tags in `scripts/gen_native_sheets.py`; critic round 1, 20 Nov 2026): the tan x series Try line computed tan(0.1) for a tan(0.3) example (now `0.3 + 0.3^3/3` = 0.309); the tangent-line Try now gives the estimate 2.025, not √4.1; thin entries rewritten whole (Maclaurin, Lagrange remainder, line integral, flux integral with a worked parametrised surface, curl-free fields); x = r cos θ, y = r sin θ and atan2 now read for points as well as phasors (precalculus shares them); half-life and doubling time now general, not only pharmacology (biology shares them); missing variables and conditions added; 30 new Try lines. Tags: de Moivre spelled `demoivre`, irrotational, gradiant, gaussian elimination (LU), over/under/critically damped on the ODE root table. 64 queries appended to `mac/Tests/sheetsearch/queries.tsv`. Test: `cargo test --test explanations` in `core`; `bash mac/scripts/test-sheet-search.sh`.
- **Geometry, statistics, finance, computer science and machine learning explanations audited** (data only: `core/src/app/explanations/analysis.json`, `applied-math.json`, `transforms.json`; notes and search tags in `scripts/gen_native_sheets.py`; critic round 1, 20 Nov 2026): about 70 thin entries made whole (what it is and when to use it, conditions, every variable with its unit, related formulas) and given Try lines that match their answers; Heron's stable form spelled out; the z-score entry also covers ML standardization; a wrong rounding fixed (hash load-factor probes 3.73 → 3.74); the growing-perpetuity note now says r > g; new search tags for CAPM, loan balance, depreciation, profit margin and Pythagoras misspellings; 64 queries added to `mac/Tests/sheetsearch/queries.tsv`. Nothing to port beyond regenerating the sheets.
- **Physics, circuits, heat transfer, continuum mechanics, chemistry, biology and astronomy explanations audited** (data only: `core/src/app/explanations/applied-science.json`, `physics.json`, `circuits-1.json`, `science.json`; critic round 1, 20 Nov 2026): 87 thin entries rewritten whole (what it is and when to use it, conditions, every variable with its SI unit, related formulas, a worked question whose answer names the variable): every heat-transfer, astronomy, chemistry, biology and fluids row that had only a one-line example, plus projectile range, collisions, SHM, closed pipes, thermal expansion, charge radius in B, Norton resistance, source transformation, rms, series RLC (now an off-resonance example). Missing units on P = (1/2)V_{m}I_{m}cos θ filled. 86 new Try lines, each checked against its stated answer; no wrong formulas or notes found. 8 queries appended to `mac/Tests/sheetsearch/queries.tsv`. Test: `cargo test --test explanations` in `core`; `bash mac/scripts/test-sheet-search.sh`.
- **Fourier, transform tables, Laplace, z transform and linear algebra made findable** (data only: `scripts/gen_native_sheets.py`, explanation in `core/src/app/explanations/analysis.json`; critic round 2 findability, 20 Nov 2026): every Laplace pair now shows its region of convergence as its note (`Re(s) > −a`, `Re(s) > |a|`, `all s`; group retitled "For t ≥ 0, with the region of convergence") and is tagged ROC like the z pairs; a new linear algebra group "Gaussian elimination" with the row `R_{i} ← R_{i} − (a_{ik}/a_{kk}) R_{k}` (explained, Try line gives y = 3); a `ROW_TAGS` table keyed by lead ‖ show (so a shared frequency side like `1` is tagged on one table only) adds ~60 search words (signum, dirac comb, IFFT, first/second shifting theorem, IVT/FVT, left sided, rref, kramer, symetric…); the z pairs' ROC tag now appends rather than overwrites. 429 queries appended to `mac/Tests/sheetsearch/queries.tsv`, all in the top 3. The unit circle is pictured, so its rows aren't searched. Windows: reads the regenerated JSON; check the Laplace pairs' new note column isn't clipped.
- **Precalculus, differential equations, numerical methods, Taylor, vector calculus, Euler and complex numbers made findable** (data only: `scripts/gen_native_sheets.py`; critic round 2 findability, 20 Nov 2026): a tag block adds formulas typed without spaces (`y=mx+b`, `b^2-4ac`, `1/(1-x)`, `ln(1+x)`, `sqrt(1+x)`, `e^(i theta)`, `e^(i pi)`, `cos(ix)`), other names (rational exponent, power iteration, rate of convergence, argand diagram, cis, sum of angles, amplitude phase form, hyperbolic sine/cosine, common ratio, repeated root, exact ODE) to their rows; the RK4 stage notes now read `stage 1, slope at the start` and so on (no `RK4:` prefix), so `rk4` finds the RK4 step first. 235 queries appended to `mac/Tests/sheetsearch/queries.tsv`, all in the top 3 (220 first). Windows: reads the regenerated JSON; nothing else to port.
- **Geometry, statistics, finance, computer science, machine learning and the formulas page made findable** (data only: `scripts/gen_native_sheets.py`, `core/src/app/native_sheets.json` regenerated; critic round 2 findability, 20 Nov 2026): one tag block adds spelled-out symbols and other names (`pi r squared`, `cuboid`, `sem`, `pert`, `prt`, `mergesort`, `tikhonov`, `gini index`, `self-attention`, `r-squared`), the six distribution table rows now carry their own names as tags (a table row has no note, so `binomial` lost to the binomial theorem), and the formulas page's rows for these sheets carry `stats`, `algorithms`, `ml`, `neural networks`, `money`, `shapes`. 476 queries appended to `mac/Tests/sheetsearch/queries.tsv`, all in the top 3. Nothing to port in code: the Windows build reads the same JSON.

- Sheet critic round 2, findability (physics, circuits, heat transfer, continuum mechanics, chemistry, biology, astronomy): ~190 more
  search tags in `scripts/gen_native_sheets.py` (one `TAGS` block, keyed by `show`): formulas typed without spaces (`f=ma`, `mgh`,
  `e=mc^2`, `v=ir`, `qvb`, `1/ha`), symbols spelled in parts (`delta g`, `delta h`, `sigma 1`, `theta p`), misspellings
  (`vonmises`, `vanderwaals`, `demorgans`, `poissons`) and course phrases (`kinematic equations`, `bode plot`, `sfee`, `rtt`,
  `ice table`, `lewis structure`). Astronomy's Pogson row's note is now "magnitude scale (Pogson)". Data only: regenerate
  native_sheets.json; nothing to port in code. 1196 queries appended to `mac/Tests/sheetsearch/queries.tsv`, all in the top 3.

- Critic round 3, explanation depth (precalculus, differential equations, numerical methods, taylor series, vector
  calculus, euler, complex numbers): about 110 explanations in `core/src/app/explanations/` (applied-math, calculus,
  signals, identities) now say why the formula holds and name a common slip; about 45 thinnest ones rewritten with new
  related rows and realistic examples, and Try lines added or fixed (`cos(cos(cos(1 rad) rad) rad)` so degree mode
  doesn't change it). Data only: nothing to port in code.

- Sheet explanations, round 3 (geometry, statistics, finance, computer science, machine learning): 30 entries deepened in `core/src/app/explanations/{analysis,transforms,calculus}.json` (why each holds, common mistakes such as rate per period vs per year, P(none) vs adding chances, log_{b} a vs log_{a} b, precision vs recall; more related rows). New Try lines for the master theorem form, case 3, ReLU and the midpoint. Generator TAGS: the complement rule gains "at least one". Shared core data, so Windows picks it up from the core; nothing to port by hand beyond the regenerated `native_sheets.json`.

- Sheet explanations, round 3 (fourier, CT and DT transform tables, laplace, z transform, unit circle, linear algebra):
  86 entries in `core/src/app/explanations/{signals,transforms,calculus,analysis}.json` now say why the row holds and
  name a common mistake (shift direction, e^{+jωt_{0}} for a delay, dropping the πX(0)δ(ω), 1/N vs 2π/N, F(as) for
  F(s/a), (z − 1) vs (1 − z^{−1}), swapping 1/2 and √3/2, b A^{−1}); the linearity, conjugation, accumulation,
  repeated-pole, damped-sine and inverse rows rewritten with related rows and realistic examples; 30 new Try lines.
  Data only.
- **Data only (explanations, critic round 3: physics, circuits, heat transfer, continuum mechanics, chemistry,
  biology, astronomy):** about 190 explanations deepened in `core/src/app/explanations/*.json` (why each formula
  holds, the common mistake such as kPa vs Pa, °C vs K, peak vs rms, half-thickness vs thickness, C_min vs C_max,
  and more related rows); a few examples made realistic, with new Try lines checked in the bar. No code change; the
  shared core picks it up.
- **Data only (sheets, critic round 4 exam walkthrough: geometry, statistics, finance, computer science, machine
  learning, the formulas index):** 30 standard rows added, each explained (applied-math.json): chord, annulus,
  rhombus/kite, frustum, exterior angle, similar-figure scaling, inscribed angle; IQR, one-proportion z test,
  two-sample t test; harmonic series, chaining cost, MIPS, cache sets and tag bits, stop-and-wait utilization (and
  Shannon capacity reused on CS); naive Bayes, SVM margin, k-means, PCA explained variance, conv output size,
  bias-variance, the log-loss gradient (and entropy reused under decision trees); annuity due, IRR, payback period,
  portfolio return and variance, current yield, declining balance. Groups renamed/split so titles match what a
  student looks for (Circles and ellipses; Rectangles, triangles and polygons; The normal curve section; Judging a
  project; Portfolios and bonds; Caches; Clustering, PCA and decision trees; Regularization alone). Finance sections
  are wide now. New TAGS and `formula: …` sheets (IRR, annuity due, empirical rule, hypothesis test, confidence
  interval, k-means, naive Bayes, conv output size, …); index hints widened. ~115 queries appended. Regenerated
  `native_sheets.json`; no code change.

- Sheet critic round 4, exam walkthrough (precalculus, differential equations, numerical methods, taylor series,
  vector calculus, euler, complex numbers), in `scripts/gen_native_sheets.py` only. New explained rows: precalculus
  gets a Trigonometry section (SOH CAH TOA, sinusoid graph, laws of sines and cosines, …), compound interest, rational
  asymptotes and absolute value inequalities; differential equations gets a separable-first ordering (logistic,
  mixing tank), a spring-mass-damper group, reduction of order, a Laplace transform method section (ℒ{y′}, ℒ{y′′},
  step, impulse) and phase-portrait eigenvalues; numerical methods gets bisection iteration count, Newton for systems,
  divided differences, Simpson 3/8, 2-point Gauss, Simpson error, Gauss-Seidel; taylor gets Taylor polynomial, an
  Error bounds group (Taylor's inequality, alternating series estimate) and root, p-series and divergence tests;
  vector calculus gets tangent plane, Lagrange multipliers, scalar line integral, flux through a graph, surface area,
  Green's area, spherical coordinates, Jacobian; euler gets other cos 2A forms, sum to product, cosh² − sinh²
  (harmonic addition split into its own group); complex numbers puts Rectangular and polar first and adds Euler's
  formula and powers of j. Several sections made wide so rows fit. 36 explanations added (analysis, applied-math,
  calculus, signals), new TAGS, hints widened, ~145 queries appended. Regenerated `native_sheets.json`; no code change.

- Sheet critic round 5 (geometry, statistics, finance, computer science, machine learning): the computer science
  sheet's bandwidth-delay product row is now `BDP = R RTT` (was `B = R RTT`, which clashed with B for bandwidth in
  Shannon capacity and block size in caches); its explanation key (transforms.json), related links (applied-math.json)
  and TAGS moved with it. Regenerated `native_sheets.json`; no code change.
- Sheet critic round 5 (precalculus, differential equations, numerical methods, taylor, vector calculus, euler,
  complex numbers): "solve ode laplace" now leads with the differential equations sheet's ℒ{y′} and ℒ{y′′} rows (the
  method), then the Laplace table's derivative row, which still leads with that table open; the c_{c} = 2√(k m) row is
  tagged "critically damped". Notes tidied: compound interest "compounded n times a year", the 2 × 2 eigenvalue group
  names T = trace and D = det, backward Euler and the central second difference state their error order, κ(A) reads
  "condition number". The absolute-value inequality explanation's Try line (it gave only one end) is gone.
  Regenerated `native_sheets.json`; no code change.
- Sheet critic round 4 (exam walkthrough: fourier, CT and DT transform tables, laplace, z transform, unit circle,
  linear algebra). New rows, each explained: Fourier gets a "Common series and LTI output" section (square, pulse
  train, triangle, sawtooth, impulse train, b_k = a_k H(jkω_o)); the CT table gets an "LTI systems" group (H(jω) from
  an ODE, sinusoidal steady state), damped cosine/sine pairs, modulation and impulse sampling; the DT table gets
  "LTI systems and sampling" (H(e^{jω}), ω = ΩT), the causal rectangular window, modulation, upsampling, and DFT
  samples the DTFT; Laplace gets "Inverse by partial fractions" (expansion, cover-up, repeated pole, completing the
  square), g(t)u(t − a), H(s) and the periodic-function transform; the z table gets "ROC and inversion" (right, left,
  two-sided ROCs, stability, partial fractions, H(z)), u[n] − u[n − N] and the one-sided delay. The unit circle gets
  1 + tan² = sec², "Symmetry and reference angles" and inverse-function ranges, and its section is now wide; its
  identities.json rows (sin² + cos², csc, sec, cot, tan) were rewritten with quadrant-sign examples. Linear algebra
  is reordered (Matrices, Eigenvalues, Solving and least squares, Vectors last) and gains the adjugate inverse,
  eigenvectors, the 3 × 3 characteristic polynomial, triangular eigenvalues, A^k, e^{At}, SVD, the consistency test,
  the least-squares solution, the projection matrix, Gram–Schmidt and QR. Search tags in a new R4_TAGS block; queries
  appended. Regenerated `native_sheets.json`; no code change.

- Sheets, round 5 polish (fourier, ct/dt tables, laplace, z, unit circle, linear algebra): the Fourier sheet's one-row
  "Through an LTI system" group is gone; b_k = a_k H(jkω_o) joins "Analysis: the coefficients a_k" with lead LTI (its
  explanation key is now `LTI ‖ b_{k} = a_{k} H(jkω_{o})`) and the section is "Common series". The z table's one-row
  "Systems" group takes the |p_i| < 1 stability row; the section is "ROC, inversion and systems". The unit circle's
  "All, Sin, Tan, Cos" hint moves to the reference-angle group's title. Linear algebra runs Vectors, Matrices, Solving
  and least squares, Eigenvalues. analysis.json: Gaussian elimination's about trimmed, rank test's A is m × n.
  Queries appended. Regenerated `native_sheets.json`; no code change.

- Sheets, round 4 exam walkthrough (physics, circuits, heat transfer, continuum mechanics, chemistry, biology,
  astronomy): 34 new explained rows. Physics: projectile x and y components (group now "Projectiles"), banked curve,
  resistors in series and parallel, RC charging (group "Current and DC circuits"). Circuits: series RLC characteristic
  equation and over-, critically and underdamped step responses in a new "RLC natural and step response" group; the
  old group is "Second-order step: overshoot and timing"; op-amp notes say "amplifier gain". Heat transfer: corrected
  fin length, infinite fin, fin rate from efficiency, fin effectiveness, a "Heat generation" group (wall, wire),
  natural convection Nu = C Ra^n, pipe outlet temperature, LMTD end differences (counterflow, parallel), the F
  correction, parallel-flow and c = 0 effectiveness, and a wide "Exchange between gray surfaces" group. Continuum: the
  principal-values/Mohr group comes first and gains Mohr's radius R, "As tensors" moves after Yield, Tensor analysis
  moves after Beams; combined loading σ = P/A ± Mc/I and the composite centroid ȳ. Chemistry: mole ratio
  (group "Moles and stoichiometry"), titration. Biology: t_d ≈ 70/r, so "rule of 70" finds biology. Astronomy: orbital
  energy, synodic period, M_bol from luminosity. Tags in a new block before `native=[...]`; queries appended.
  Regenerated `native_sheets.json`; no code change.

- Sheets, round 5 polish (physics, circuits, heat transfer, continuum mechanics, chemistry, biology, astronomy):
  notes only, no rows added or moved. Physics v = √(T/μ) reads "wave on a string: T tension, μ mass per length";
  chemistry P_i = x_i P_tot "partial pressure from mole fraction", pOH gains "basicity"; heat transfer Grashof
  "Grashof: buoyancy over viscosity, natural (free) convection", Rayleigh "Rayleigh: sets the natural convection
  regime"; circuits V_rms = V_peak/√2 "sine, V_peak = V_m". Tags: the five ε-NTU rows take "ε-NTU epsilon-NTU" (and
  the ε rows "effectiveness-NTU method"), large parallel plates take "radiation shield", in a new block before
  `native=[...]`. Queries appended. Regenerated `native_sheets.json`; no code change.
- **A bare e beside units is Euler's number; round-off imaginary parts aren't shown** (core: `units.rs` parse_primary, `complex_line.rs` rectangular; test `core/tests/bar_bugs_e.rs`): next to a unit, a lowercase `e` that starts a term was read as the electron charge, so `20 V * e^(-1)` was blank and `100 uF * 9 V * (1 - e^(-2)) to mC` said improper unit conversion; it's now e = 2.718… as `pi` is (after an amount, `5 e`, it's still the charge; capital `E` is unchanged). `e^(jπ) + 1` showed its 1.2e-16 imaginary part as `j0`; a part that formats as 0 is dropped. Shared core, so Windows gets it by rebuilding.
