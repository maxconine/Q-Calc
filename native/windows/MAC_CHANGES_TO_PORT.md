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

- **A root or π in a matrix entry** (core `matrix.rs`: `roots`, `spaced`, `SPACED_ROW`): `det([√3 1; 0 2])` is 3.46410161514 (exact `2sqrt(3)`), `[√3 √2]` is two entries, `[1 -√2]` is `[1 -1.41421]`, `[π 1]` and `[2π 1; 0 1]` work. Your report; fixed on the Mac. Test: `roots_and_pi_in_entries` in `tests/linear_algebra_typed.rs`, then type `det([sqrt(3) 1; 0 2])` in the bar.

- **Named constants in words** (core `named_constants.rs`, called from `Sheet::line` in `evaluate.rs`): the constants page of Mitiguy's appendix answer when typed, `Earth's axial tilt` is 23.45 deg, `mass of earth / mass of moon` is 81.3924, `gravity on mars` is 3.7 m/s². Test: `named_constants_answer_in_words` in `tests/requested_features.rs`, then type `Earth's axial tilt` in the bar. A second round added physics constants, each planet's mass, year, day and escape velocity, materials and fun facts; test `more_named_constants_answer_in_words`, then type `young's modulus of steel`.

- **`ans` after a conversion to a compound unit** (core `units.rs` `quantity_text_now`): `160lbs*3ft to lb*in` then `ans/2` is 2880 lb·in; it used to say no answer, because `lb*in` read back as `lb·in` and the answer kept no quantity. Test: `ans_after_a_conversion_to_a_compound_unit` in `tests/requested_features.rs`, then type those two lines in the bar.

- **Preview keeps a single raised digit** (core `typst_math.rs`): with exponent answers on (⌃E), `5*10^7*((0.2/2)^2-(0.075)^2)` previewed `= 2.1875 × 10`, its ⁵ dropped because the math font has ¹ ² ³ but not ⁰ or ⁴–⁹; it's now written `^(5)`. Test: `preview_keeps_a_single_raised_digit`, then look at that line's preview.

- **√ of a whole name** (core `scientific/preprocess.rs` `ROOT_LETTER`): `1/√ans` after `1*10^-7` is 3162.28, and `√pi` answers; √ used to root only a single letter, so `√ans` read as the unknown word `sqrtans`. Test: `root_of_a_whole_name`, then type `1*10^-7` and `1/√ans` in the bar.

- **A date plus several spans** (core `phrases/time.rs` `span_chain`): `today + 6 days + 10 days`, `today - 3 days + 1 day`, `today - (6 days + 10 days)` and `9:00 + 1 hr - 15 min` answer; a date or clock used to take only one span. Test: `a_date_plus_several_spans`, then type `today + 6 days + 10 days` in the bar.

- **A date answer chains by spans** (core `app/chain.rs` `is_date_answer`, `app/live.rs`, `phrases/time.rs`): after `today + 6 days`, typing `+ 1 week` or `- 2 days` goes on from that date, and dates as answers show them (`Wed 11 Nov 2026 + 1 week`) read back; a bare `-2` or `+ 3` is still a number. Test: `a_date_answer_chains_by_spans`, then do it in the bar and check the tape row reads `Tue 17 Nov 2026 + 1 week`.

- **Graph: crossing implicit curves, holes, y-intercepts** (core `graph_native.rs` `native_intersections` and `fill_holes`; `graph.rs` `GraphResult::xy_fn`): a curve now also crosses an implicit curve, at the roots of F(x, f(x)). `sin^2(x)/x^2; x=1` gives intersection (1, 0.000304586), and `x^2+y^2=25; y=x+1` gives x = −4 and 3. A removable hole is filled with its limit: in degrees sin²x/x² now lists max (0, 0.000304617), where its top was missing before. Every curve lists its y-intercept unless a key point or root is already at x = 0 (the page lists one for lines only). Test: `holes_intercepts_and_implicit_crossings` in `tests/graph_more.rs`, then type `graph sin^2(x)/x^2; x=1` in degrees.

- **Smart quotes as feet and inches** (core `feet_inches.rs`: `marks`): after a digit, `”` and `“` read as `"` and `’` as `'`, so `1.0621" + 0.328125”` and `6’2”` answer (macOS smart quotes typed them). Test: `feet_and_inches_marks` in `tests/native_fixes.rs`, then type `0.328125”` in the bar.

- **A minus before a bracket of units** (core `units.rs` `Parser::parse_primary`): `-(2 m) * 3` is -6 m, `-(1 V / 10 kΩ) * 33 kΩ` is -3.3 V and `-(2 m)^2` is -4 m²; the unit reader took a sign only as part of a number, so these were blank. Test: `a_minus_before_a_bracket_of_units` in `tests/native_fixes.rs`, then type `-(2 m) * 3` in the bar.

- **`√ans` answers** (core `scientific/preprocess.rs`, `ROOT_NAME`): a root sign takes a whole `ans`, `pi` or `tau`, where it read `sqrtans` and left the line blank. After 4, `√ans` is 2, `3√ans` is 6 and `√ans+1` is 3. Test: `a_root_sign_takes_a_whole_ans_pi_or_tau` in `tests/native_fixes.rs`, then type `4`, ↵, `√ans` in the bar.

The rest is done. (`dx` touching the integrand, halves powers as roots and the faster intersections were checked in the app on 8 Nov 2026, with the linear algebra tables.)
- **Imaginary roots instead of "no real solution"** (core `solve.rs`: `ComplexRoots`, `complex_pairs`, `format_complex`; native only, user request 18 Nov 2026): a polynomial equation with no real root answers with its complex roots. `x^(2)=-1` is `±i`, `x^2=-4` is `±2i`, `x^2+2x+5=0` is `-1 ± 2i`, `x^2+x+1=0` is `-0.5 ± 0.866025403784i` (exact `(-1 ± sqrt(3)i)/2`), `x^4=-1` lists both pairs. Real roots are unchanged. The solve has a new outcome, `SolveOutcome::Complex` (`"complex"` in saved history), so the answer is drawn full size with its `x =` like real roots, not as a muted message; `ans` isn't set from it. Already done on Windows, not yet built or run there: `answer.rs` and `history.rs` (message checks and `x =` labels) and `typst_row.rs` (`solved_for`). The steps now say the roots are imaginary or complex. Golden floors: `eval.solve` is now 449 and `eval.sheet` 4487, since 20 recordings want `no real solution`. Test: `tests/complex_roots.rs`, then type `x^2=-1` in the bar and check the row's copy button copies `±i`.

- **The dynamics sheet covers Mitiguy's appendix, with a picture of every body** (`native/scripts/gen_native_sheets.py`, `dynamics`; data only, user request 20 Nov 2026): the sheet now has every equation of the appendix (pp. 301–310): vectors (dot and cross product identities, triple products, derivatives), rotation matrices, angular velocity, velocity and acceleration of points on a body, rolling and gears, centre of mass, inertia dyadics and shift theorems, momentum, kinetic energy, forces and moments, power and work, equations of motion (Newton, Euler 2D and 3D, Lagrange), a road-map section in words, and the p. 306 constants. The 24 bodies of pp. 301–305 are each a group whose first row starts with a Typst picture of the body and its axes (`Pic` in the script: drawn in opaque black, which is the ink colour, with a see-through grey `rgb(128, 128, 128, 70)` fill), so `typst_row.rs`'s `color()` must draw a non-ink paint with its alpha. Rows are taller than usual (up to about 110 pt) and the widest non-wide row is 308 pt. Mitiguy's notation (`^{N}v⃗^{Q}`, `I⃡^{B∕O}`, `θ̇`) is handled by `accents()` in the script; nothing else changed in any other sheet. Every new row is explained in `core/src/app/explanations/dynamics-1.json`, `-2` and `-3` (added to `EXPLANATION_FILES`). Test: open `dynamics` (or `dynamics: Inertia of cylinders and cones`) in light and dark, check the pictures draw with grey shading and the rows aren't clipped.

- **Convert to feet and inches** (core `feet_inches.rs`: `split_target`, `answer_split`, called from `evaluate.rs` `line`; `app/unit_spans.rs` colours the target's inches; native only, QA 20 Nov 2026): a length converted `to ft in`, `in ft and in`, `in feet and inches`, `to ft, in`, `to ft-in`, `to feet & inches` or `to '"` answers split, `1.8 m to ft in` is `5 ft 10.8661417323 in` (fractions of an inch when the line typed one: `50 3/8" to ft in` is `4 ft 2 3/8 in`); its value is in inches, so ⌥↑/⌥↓ step it to feet or inches as before. `to ft-in` was a nonsense `ft − in` quotient before. The `in`/`inches` of such a target is coloured as a unit. Test: `tests/units_feet_inches.rs`, then type `1.8 m to ft in` in the bar, check `in` is coloured and ⌥↑ gives `5.90551181102 ft`.

## 2. Edited on Windows but never run: test it

- **Sheet filter: greek spelled in latin** (`sheet_page.rs`: `latin`, used in `matches`; the Mac's `IdentityWindow.swift` does the same with ICU's Greek-Latin). `tmax` finds `τ_max`, `sx` finds `σ_x`, and a typed `τ` still finds only τ. Edited from the Mac, where `qcalc-win` doesn't build (`stacked.rs` calls the Windows-only `crate::input`); the logic was tested on its own. Test: `cargo test -p qcalc-win a_filter_word`, then type `tmax` in the continuum mechanics sheet's filter.

- **`ans` drawn as a chip in the preview** (`typst_row.rs`: `request` uses the core's `chain::ans_chipped` in place of `ans_written_out`, and shows the row for a line that uses `ans` even with stacked fractions on; the chip is `anschip` in `typst/src/macros.typ`). Mac: `MathText.swift` and `BarModel.refreshTypeset`. The preview draws `ans` as the number it stands for in a blue box with a small "ans" pill on its edge, as Desmos does, and every line that uses `ans`, typed or chained, gets the row. Edited from the Mac, where `qcalc-win` doesn't build. Test: `cargo test -p qcalc-typst --test render an_ans_chip`, then type `3247*123`, ↵, `sqrt(ans(12))`, in light and dark, with stacked fractions on and off.

## 3. Not on Windows yet: implement it

### Tape: copy button and notes (17 Nov 2026)

The user asked for both. Core part is already in (`HistoryKind::Note`, see below); the UI is Mac only so far.

- **Copy button on each row** (`mac/Sources/Bar/BarView.swift`: `RowCopyButton`, placed at the end of `HistoryRowView`): a small copy icon (`doc.on.doc`, 11 pt, muted, ink on hover with a faint fill) at the right end of every tape row. It always takes its 22 pt so the row doesn't shift, and is seen only while the pointer is on the row or the row is selected (↑). A click copies the answer as Copy Answer does (`copyRow(row, answer: true)`, so unitless copy applies), flashes the bar's `✓ copied`, turns the icon into a green check for 1.2 s, and puts focus back on the line. It never inserts into the line. Tooltip "Copy".
- **↵ on a line with no answer saves it as a note** (`mac/Sources/Bar/BarModel.swift`: `commit`'s unsaved branch → `saveNote`): instead of the "nothing to save" hint, the line is kept as typed as a row with `kind: note`, `display: ""`, and the hint says `saved as a note`. Not when a unit is being offered (that ↵ still takes the unit), not for a `graph…` line, and not when the bar hides with unfinished input. A note row shows its text across the whole row (no answer column) in the expression colour; a click inserts it (tip "Insert note at the cursor"); ← → have nothing to pick; the copy button and the context menu's single "Copy Note" copy the text.
- **Core** (`core/src/app/history.rs`, `ffi/src/history.rs`, `ffi/src/records.rs`): `note` is a row kind, so it is never `ans`, a variable, a function or a sticky definition, and it keeps up to 1000 characters (`MAX_HISTORY_NOTE`) where an expression keeps 240. Test: `a_note_row_keeps_its_text_and_counts_for_nothing` in `tests/requested_features.rs`. Windows matches on `HistoryKind` with `==` only, so it still builds; a note row there today would show an empty answer.

- **Keys on a selected row** (`mac/Sources/Bar/BarModel.swift`: `key`, `copySelectedRow`; `InputField.swift` `keyDown` hands ⌫ / ⌦ to the bar before its own pair-deleting): with a row picked by ↑, ⌘C (the `copyAnswer` binding, Ctrl+C's Windows equivalent is your call since Ctrl+C clears history there) copies what ← → picked, the expression or that answer (a note's text), unitless as Copy Answer; ⌘↓ (Ctrl+↓) drops the selection and goes straight back to the line; a plain ⌫ or ⌦ deletes the row, as ⌘⌫ did, and ⌘Z brings it back. With no row selected all three keys do what they did.

- **Hover shows the pick box** (`mac/Sources/Bar/BarView.swift`: `AnswerCell` and the row's expression button): the pointer on a row's answer, or its expression, draws the same ring and tint (`PickedMark`, 4 pt side padding) that ← → draw on a selected row's pick.

To test on Windows: type `call Sam at 3pm` ↵ (a note row, hint `saved as a note`), `2^10` ↵, then hover each row and click its copy icon; the clipboard holds `call Sam at 3pm` and `1024`, and the line stays empty.

### Sheets: a click explains the equation (17 Nov 2026)

The user asked for it: a click on an equation row in any reference sheet should also open an explanation of it. The data is in the core; the popup is Mac only so far.

- **Core** (`core/src/app/identities.rs`: `Explanation`, `Example`, `EXPLANATION_FILES`, `explain_key`; data in `core/src/app/explanations/*.json`, format in its `README.md`): every equation row of every sheet (not conversions, constants or the formulas page) now carries `explain`, found by the row's lead cells and `show` joined by ` ‖ `. It has a title, what it's for, assumptions, variables (symbol, meaning, SI unit), related formulas, a worked example (problem, steps, answer) and an optional `try` bar line that works the example out. Text raises `^{…}` and lowers `_{…}` as `show` does. Tests: `core/tests/explanations.rs` (every key is a row, every row is explained, every Try line answers in the bar). Windows only needed `explain: None` added to the test helper in `sheet_page.rs`.
- **Mac UI** (`macos/IdentityWindow.swift`, linked as `mac/Sources/Shared/IdentityWindow.swift`: `ExplanationView`, `showExplanation`; `mac/Sources/Sheets/SheetData.swift` passes `explain` through; `mac/Sources/App/OverlayController.swift` sets `onTry`): the click still copies as before, and then a popover opens beside the row (on the side of the screen with more room), 420 pt wide, scrolling past 520 pt. It shows the title (15 pt semibold), the row's equation typeset as in the sheet, the about text, then "Assumptions" (bullets), "Variables" (symbol, a tab, meaning, then the unit muted), "Related" (formula, then when it's used, muted) and "Example" (problem, numbered steps, a bold "Answer" line). With a `try` line, a footer shows it in 11 pt monospaced, muted, and a "Try it in the bar" button puts that line in the bar (replacing what was there) and shows the bar. The text is selectable. A click outside, another row, a page change or Esc closes it (Esc closes it before clearing the filter). The sheet's status hint reads "click to copy and explain" on a sheet with explanations.

To test on Windows: type `torsion formula` ↵, click `τ = Tρ/J`; the clipboard holds `τ = Tρ/J` and the explanation shows J for solid and hollow shafts and a 50 mm shaft example. Its Try button puts `1.5 kN m * 25 mm / (pi (25 mm)^4 / 2) to MPa` in the bar, which answers 61.1155 MPa.

### Slots: a slot machine game (18 Nov 2026)

The user asked for it: type `slots` and play the gambling game, for free chips, no money, and press space while it spins to stop each reel. Rules and reel motion in the core, panel Mac only so far. Until Windows has a panel, `slots` is in the games list there (core `GAMES`) but opening it logs `games: no game called slots`.

- **Core** (`core/src/games/slots.rs`; `app/games.rs` adds `GameKind::Slots`, typed as `slots`, `slot`, `slot machine` or `one armed bandit`, hint `↵ play slots`; `games/saves.rs` adds the `qcalc-slots` save key; FFI `slots_*` in `ffi/src/games_play.rs`): three reels on one line over a 32-stop strip (`STRIP`: 7 ×1, π ×2, ∞ ×3, ∑ ×4, √ ×5, e ×4, blank ×13). Three of a kind pays 500/200/100/60/30/20 × the bet (7 π ∞ ∑ √ e), two e's ×6, one e ×1 (bet back); unstopped it pays back about 95% (`return_to_player`). Bet 1–10, 100 chips to start, kept with the best as JSON in `qcalc-slots`; at 0 chips a spin hands out 100 more. **Motion is in the core** so a press can stop a reel where it is: `position(state, reel, now)` winds up over 220 ms, rolls at 30 stops a second, and on reaching its stop runs 0.25 stop past and springs back over 300 ms (ease-out cubic), the bounce of the reference video. Unstopped, reel i lands on a randomly picked stop the first time it comes round after 700 / 950 / 1200 ms (and 180 ms after the reel to its left). `press` (space while spinning, not in the wind-up) stops the leftmost reel still rolling on the next stop to reach the line, so timing decides it. `tick` pays 150 ms after the last reel lands. FFI: `slots_spin`, `slots_press`, `slots_tick`, `slots_path(state, reel, start, step, count)` for a whole animation's positions in one call, `slots_settled_at`.
- **Mac UI** (`mac/Sources/Games/Slots/SlotsPanel.swift`, opened from `NativeGames.swift`; bar command in `TextRules.swift` `gameList`): **no lobby** (since 19 Nov 2026: the user wanted `slots` to start at once): `SlotsPanel` is the machine itself, so the shell stays in lobby mode and one esc (or the header's `esc to leave` button, `shell.escape()`) closes the game back to the bar, with no pause card and no "esc again". The machine is drawn as **Core Animation layers, not a redrawn canvas** (a canvas redrawn per frame was far too slow): three rounded reel windows on a tape-coloured body, each holding the strip drawn once as a tall image, sharp and smeared (9 faint copies, for speed). A spin or a press samples `slots_path` at 240 a second from now to the settle and hands each reel discrete keyframes for its position and the sharp/blurred crossfade (by speed), so nothing runs per frame in the app and nothing at all while idle. The rows above and below the line are shaded (a gradient of the background to its own clear, not to clear black, which bands), a rolling reel is dimmed to 55% and lifts over 180 ms as it lands. Symbol colours: 7 red, π the accent green, e gold, the rest ink. A win lights the pay line (accent, gold for 50× or more, which pulses 4 times), the chips count up (0.35–1.4 s) in a timeline that runs only then, and the result (`+N`, `jackpot +N`, `no win`, `+100 free chips`) fades in. The layout (cleaned up 19 Nov 2026): a header line `slots  N chips` with `esc to leave` at the right; the machine; one row under it, as wide as the machine, with `bet − N +` on the left and on the right the result, or before one a muted hint (`space or click to spin`, `space stops a reel`, `space for 100 free chips` at 0); and one muted caption line with the pay table (`7 7 7 ×500   π π π ×200 …`). No best score and no long key note any more. A 30 ms timer asks the core to pay while spinning. Keys: space, ↵ or a click spins and then stops each reel; ← ↓ − lower and → ↑ + raise the bet, 1–9 set it and 0 is 10. Reduce Motion: each reel stands until it lands, no blur, pulse or count-up.

To test on Windows: type `slots` ↵ (the machine, no lobby), press space to spin and three more times quickly; the reels stop left to right, each on what was passing the line, with a small bounce, and the chips change by the pay table. Close mid-spin and reopen: the chips are what the spin paid. One esc goes back to the bar.

### Sheets: Related links, search everywhere, clicks with the bar up (20 Nov 2026)

The user asked for all three. Mac files: `macos/IdentityWindow.swift` (shared with the web-view app), `macos/PeriodicWindow.swift`, `native/mac/Sources/Sheets/SheetData.swift` (`allSheets`), `native/mac/Sources/App/OverlayController.swift`, `App.swift` (sets `IdentityWindowController.allSheets`).

- **Related at the bottom, and clickable** (`ExplanationView`): an explanation now reads title, equation, about, assumptions, variables, example, then Related last. Each Related formula is a link in the accent colour. If some row in any sheet is that formula (spaces, brackets, `^ _ { }`, minus and times signs ignored; either side of `=` first), its explanation replaces this one in the same popover, with a `‹ Back` button bottom left. About 45% of the Related formulas have one. Otherwise the click closes the popover and searches the sheets for the formula's note (`effective annual yield`), or for the formula itself when it has no note.
- **Search reads more, and searches every sheet** (`layout`, `searchText`, `matchCount`, `unitNames`): a row also matches on its explanation's title, its Related formulas and notes, its variables' meanings and units, and each unit's name (`Ω` adds `ohm ohms resistance resistor`, `N` adds `newton force`; the full table is `unitNames`). Under the open sheet's matches come every other sheet's, after a quiet `in other sheets` line, each under `<sheet> · <section>`. That's 60 rows at most, never a picture, and never a row already listed (the one-formula sheets such as `ohm's law` repeat the big sheet's rows). When no row has every word, rows with all but one show, under `no row has every word · closest matches`. The list of sheets comes from the new core call `lib/identities` `nativeSheetIds` (core `identities.rs` `native_sheet_ids`, test `every_sheet_can_be_searched`).
- **Clicking a sheet while the bar is up** (`OverlayController.isSheetWindow`, `PeriodicPanel.wantsKeys`): a click in a row's explanation popover no longer counts as a click elsewhere, so it no longer hides the bar (this broke Try it in the bar and the new links). A click in the sheet's search field now takes the keys, so you can type there with the bar still up; the bar's focus-loss check runs a turn later and keeps the bar when the keys went to a sheet. A click on a row still leaves the keys and caret in the bar.

To test on Windows: open `finance`, search `ohm` (Physics and Electrical Engineering rows, the Ω constants), `force`, and `effective anual yield` (closest matches: compound interest); click compound interest, then a Related formula (`A = Pe^{rt}` opens its own explanation, Back returns); with the bar up, click into the sheet's search and type.

### Input line: undo steps for paste, cut and brackets (20 Nov 2026)

Mac file: `native/mac/Sources/Bar/InputField.swift` (`pastePlain`, `cut`, `insertText`, `replace`); headless test `native/mac/scripts/test-input.sh` (`Tests/input/main.swift`: words to symbols, `[` pairing, undo runs).

- **A paste is its own undo step** (`insertFromPaste`, kind none): it went through the field's own typing path and was recorded as typing, so ⌘Z within a second took the paste and what was typed before it together. A cut likewise (`deleteByCut`). Check Windows' input does the same.
- **A typed `[` (pairing, stepping over `]`, wrapping a selection) joins the typing run**: it had been its own step and ended the run.
- **Whole-line swaps record the caret where it lands**: the caret was set after the change was recorded, so undo/redo back to such a step (a `[` pair, a stacked `/`, a bounds key) put it at the end of the line.

To test on Windows: type `12`, paste `+34` at once, ⌘Z (Ctrl+Z) leaves `12`; type `v=[1 2` then Ctrl+Z, Ctrl+Shift+Z, and the caret comes back inside the brackets.

### Already ported

The stacked fractions round (TeX sizes, Desmos keys, grown brackets, stacked history rows, answers shrinking as one) and the newer graph round were ported for 3.0.2 (branch `win-port-302`, 8 Nov 2026). What stays different on Windows:

- **Stacked: a `(` typed inside a slot** doesn't bring its `)`, so `2^10+1/(x/3)+(1/2)/4` typed key by key nests differently from the Mac; typing `)` to step out gives the intended layout.
- **Stacked: ↓ pressed within a few ms of typing**, before the next paint, can land on the wrong side of a slot (the places come from the last paint).

- **Graph: pinch and two-finger gestures:** Windows takes a touchpad's pinch as Ctrl + wheel (it zooms about the pointer, as before); a two-finger swipe arrives as the wheel. Shift + wheel pans.
- **Graph: hover tips** on the head's new buttons (zoom out, zoom in, camera) aren't drawn: the head's icons had none on Windows.

Open questions for the Mac side:

- **Matrix grid gaps left on Windows:** Ctrl+G and Ctrl+R are fixed chords (the core's keybinds have no matrix actions; a user binding on the same chord wins); a click in a cell puts the caret at the end (double-click selects it) rather than under the pointer; the grid doesn't scroll sideways (8 columns need about 590 px); no UI Automation for the cells yet.


### Sheet data, for reference

All of it is generated by `native/scripts/gen_native_sheets.py`, so edit that and not the JSON. Named formulas are sheets with `names`, and their ids start `formula: `. Parts have ids like `continuum mechanics: Beams`.

## 4. How to test the sheets

- Try a small screen (the sheets were tried on a 4K screen at 150% only).
