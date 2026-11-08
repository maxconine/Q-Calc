# Mac changes to port and test on Windows

A running list, kept by whoever changes the Mac app. How to work through it: `PORT_RUNBOOK.md`. Newest additions go at the end of their section. When a line is done and tested on Windows, delete it.

**Before you start:** this work may not be committed or pushed yet. Check that `native/core/src/app/native_sheets.json` and `native/core/src/app/native_typst_cells.json` exist on your branch. If they don't, ask for the Mac work to be committed first.

**Mac-to-Windows key names:** ⌘ is Ctrl, ⌃ is Ctrl unless Windows already binds that chord, and ⌫ is Backspace.

**Spec:** `native/mac/PARITY.md` and the Mac sources named below. The rules are in the shared core, so don't copy them into Windows.

**Status of this list:** written from the Mac. `cargo check -p qcalc-win` passes there, which may not cover Windows-only code, and nothing here has been run on Windows.

## Checks to run first

```
cargo test -p qcalc-core
cargo test -p qcalc-ffi --features typst --test native_sheets_typst
cargo test -p qcalc-win
```

The `eval.blank` golden floor is now 1311, because two recordings of `∫ x dx` and one of `integral of x^2` expect a blank and they now answer. The `eval.units` floor is now 8988: two recordings want `2 ft + 3 in` as `2.25 ft` (it now answers `2 ft 3 in`), and one wants `1/2 in + 3/8 in` as `0.875 in` (now `7/8 in`). If Windows passes a different count, tell the Mac side rather than raising it.

Windows' own tests have not been run since these changes. `commands.rs` tests were edited to expect part sheets, such as `trig` opening `identities: Trig`. One test is known to need updating: the one in `commands.rs` that asserts `id == "identities"`, `section > 0` and a hint containing `summation`. That line now opens `identities: Summation`, at section 0.

## 1. Comes from the core: just test it

No Windows code should be needed. Type each line and check the result.

| Type | Expect |
| --- | --- |
| `(3+j4)/(1-j)` | complex answer, in `j`; the same with `i` gives `i` |
| `3+j4 in polar` | polar form |
| `(3+j4) kΩ` | complex answer with its unit |
| `1 kj`, `1 mj` | energy, not complex |
| `e^(j30)` | 30 radians, unless a unit is typed |
| `cos(π/4 rad)` in degree mode | uses radians, as typed |
| `chat max` | same as `messages max` (check `threads.rs` opens it, see the guard in `Messages/MessageThreads.swift`) |
| `x^2 + y; x=5; y=2` | `27`, with the preview x² + y = 27 (x = 5, y = 2); `x` isn't defined on the next line |
| `d/dx x^2; x=3` | `6` |
| `∫ sec(x)^2 dx` in radians | `tan(x) + C`; in degrees `(180/π) tan(x) + C` |
| `∫ sec(x)^2 dx; x=5` in radians | `-3.38052 + C` to your sig figs |
| `∫ e^(x^2) dx` | blank (no elementary answer) |
| `integral of x^2`, `the integral of x e^x dx`, `integrate sin(x)`, `antiderivative of 1/x`, `integrate t^2 with respect to t`, `∫ x e^x` | the same as the `∫ … dx` form (with no `dx`, the variable is x, else t or θ); ⌃W shows the working |
| `integral of x^2 from 0 to 1` | `0.3333…` (bounds still integrate numerically) |
| `formula` | autofill ghost `s`, and Tab takes it |
| `control ` | ghost `systems` |
| `ohms` | ghost ` law` |
| `graph y=-5x+7.5` | chips `x-intercept (1.5, 0)` and `y-intercept (0, 7.5)` |
| `graph y=3` | y-intercept only |
| `graph x^2-4` | no intercept chips |
| `6'2"`, `6′2″`, `6'2` | `6 ft 2 in` |
| `10''` | the same as `10"` |
| `6'2" + 1'11"`, `5 ft + 3 in` | `8 ft 1 in`, `5 ft 3 in` |
| `6'2" to in` | `74 in` (a conversion asked for wins) |
| `11.8379in + 14.6503in` | `26.4882 in` |
| `f'(2)`, `y''`, `30°15′` | unchanged: primes and arc minutes |
| `$5 + €3` | dollars, the euros converted (needs today's rates) |
| `$5 * 3 to eur`, `$20/hr * 40 hr to eur` | euros |
| `usd to eur` | `1 USD = 0.8909 EUR (Nov 5)`: the rate and its day. The day needs `date` in the rates JSON Windows hands `set_rates` (the Mac's `ECBRates.pageJSON` now includes it) |
| `5 3/8" + 2 7/16"` | `7 13/16 in` |
| `6'5 3/8"`, `(6'5 3/8") / 2` | `6 ft 5 3/8 in`, `3 ft 2 11/16 in` |
| `10" / 3 + 1/16"` | `≈ 3 3/8 in` (to the nearest 1/16, ≈ when rounded) |
| `5 3/8" to mm` | `136.525 mm` |
| `12.0 kg + 3.15 kg` in sig-fig mode | `15.2 kg`; `1.20 m + 35 cm` gives `1.55 m`; off, `15.15 kg` |
| `∫ x e^x dx`, `∫ ln(x) dx`, `∫ 2x cos(x^2) dx` | `x e^(x) - e^(x) + C`, `x ln(x) - x + C`, `sin(x^2) + C` |
| `y' = 2x` | `y = x^2 + C` |
| `dy/dx = y` | `y = C e^x` |
| `y'' + y = 0` | `y = C1 cos(x) + C2 sin(x)` (radians, in either angle mode) |
| `y' + 2y = e^x` | `y = e^x/3 + C e^(-2x)` |
| `y' = y; y(0)=2` | `y = 2e^x` |
| `y'' + y = 0; y(0)=1; y'(0)=0` | `y = cos(x)` |
| `y' = tan(x)`, `y' = x/y`, `y'' + y = 0; y(0)=1` | blank (not solved, never guessed) |

Also check the `?` sheet. `NATIVE_CHEATS` grew to 17 rows, and the Mac needed three columns to fit them. Make sure nothing is cut off in `cheats.rs`.

### Graphing (core `graph_native.rs`, FFI `graph` / `graph_home`)

`engine.graph` and `engine.graph_home` now return more. Check these numbers, even before the drawing is ported:

| Type | Expect from the engine |
| --- | --- |
| `graph y^{2}=10000x` | label `y^2 = 10000x`; home about `[−100000, 280000]`; one `vertical tangent (0, 0)` |
| `graph x^2+y^2=25` | `max (0, 5)`, `min (0, −5)`, `vertical tangent (±5, 0)` |
| `graph (x-3)^2+(y-50)^2=4` | `graph_home` gives `y_center` 50; pass it back as `GraphOptions::y_center` |
| `graph y - 3 = 2(x - 1)` | a curve, not a contour; label `y - 3 = 2(x - 1) → y = 2x + 1`; intercepts `(−0.5, 0)` and `(0, 1)` |
| `graph line through (1, 2) and (3, 5)`, `graph (1,2) slope 3`, `graph interpolate (0,0), (1,3), (2,1), (4,5)` | `point` critical points first; y readout interpolates |
| `graph z = x^2 - y^2` with `surface: true`, `aspect: 1` | `saddle (0, 0)` with `z: Some(0)`; `surface` is a 49 × 49 grid, z −100…100 |
| `graph x + y + z = 1` | a surface; `graph x^2+y^2+z^2=25` gives an error asking for `z = …` |

New FFI fields: `GraphOptions.y_center`, `GraphOptions.surface`, `GraphWindow.y_center`, `CriticalPoint.z`, `GraphResult.surface` (`SurfaceGrid`), and `CriticalKind::{Saddle, VerticalTangent, Point}`.

## 2. Edited on Windows but never run: test it

- **`ans` in the typeset row:** `ans * (10^5)/30` shows the number, not the word. `model.rs` (`ans_value`), `typst_row.rs`, `word_tip.rs`.
- **Typeset look:** `tan⁻¹(x)` has no brackets round `tan⁻¹`, and a root's bar sits clear of what's under it. `typst_row.rs` uses `native_typst_document`.
- **Graph chips:** a y-intercept's chip copies y, not x. `graph.rs`. The separate Roots list still repeats the x-intercept, so decide whether to hide it for a line. The Mac has no Roots list.
- **Sheet commands:** `commands.rs` uses `native_target_for`, with extra first-word stems and `part_could_start`. Check `beams`, `Ohm's law`, `newton's 2nd` and `F = ma` open a sheet, and that `ohm`, `newton`, `5 ohm` and `dt = 5` still calculate.
- **Sheet rows (`sheets.rs`):** a section's own lead widths, heading rows that aren't clickable, a click on a table cell copying that cell, rows on `formulas` opening their sheet, and the copied state lasting 0.25 s.
- **Graph FFI fields (`graph.rs`):** `kind_name` has arms for `saddle`, `vertical tangent` and `point`, and the three `GraphOptions` literals pass `y_center: None, surface: false, x_log: false`. This is only enough to compile. See section 3 for the behaviour.

## 3. Not on Windows yet: implement it

### Graph panel: implicit curves, 3D surfaces, key points (`Bar/GraphModel.swift`, `Bar/GraphPanel.swift`)
- **Draw `contours`:** the Mac panel never drew them before. An implicit curve (`contour && !contour_levels`) is one 2 pt accent line. A surface's level curves use opacity 0.35→1 from the lowest level up, with their labels on a plot-coloured box.
- **y centre:** keep `y_center` for anything but a curve. Take it from `graph_home` on open, send it with every sample, and let a vertical drag move it by `dy × (y_max − y_min)`. Reset restores it.
- **Readout for x-and-y plots:** crosshairs and `(x, y)`, plus `z = …` from `graph_readout_z` for a surface.
- **3D view, the default for a surface:** a `3D | contour` switch in the head. 3D samples with `aspect: 1` and `surface: true`, in a 652 × 300 plot. It draws the grid's quads back to front, viridis by height, lit by the normal, with a floor, the x/y/z values at the edges and the critical points ringed. Controls follow Onshape. A two-finger press-and-drag (a right-drag) orbits, and so does a plain drag. Orbit turns against the pointer side to side and against it up/down too: dragging down lowers the tilt, `pitch -= dy * 1.6` with dy the fraction of the plot height dragged down, clamped to 0.05…1.45 (flipped from the first port, where dragging down raised it). Two fingers moving up/down zoom, up zooming in whatever the natural-scrolling setting (`isDirectionInvertedFromDevice`). Shift + two-finger drag pans along the floor, following the fingers, and so do a shift/option drag and a middle-drag. Momentum is ignored. A mouse wheel or a pinch zooms. The right-click menu is suppressed over the plot. The 3D plot takes these events in its own native view (`PlotInput` in `GraphPanel.swift`), which never lets a press start a drag of the bar's window. Through SwiftUI gestures, a drag moved the whole bar instead of orbiting. See `Surface3D` in `GraphPanel.swift` for the projection. **Ticks and pointer readout (3D):** the floor has a faint grid at the x and y ticks, about 8 across using the 1/2/5 rule (`ticks`); x = 0 and y = 0 are a little stronger. Short tick marks stick out from the front x edge and the side y edge, and every other tick is labelled (all of them when there are 5 or fewer; `labelled`). The z edge has ticks about 6 across, each with a faint level line along the two back walls. Hovering the surface finds the nearest drawn quad under the pointer (`surfacePoint`, barycentric on its two triangles), takes x and y there and gets z exactly from `graphReadoutZ`. It draws a dot, a dashed drop to the floor and dashed guides across the floor to the x and y edges, plus an "(x, y, z)" pill at 4 significant figures. The controls hint shows only when the pointer is off the surface.
- **Key points easy to find:** marks are 6.5 pt. With the pointer within 9 pt of a mark, the plot shows a halo, dashed guides and a `kind (x, y)` pill, and the matching chip lights. Chips show `(x, y, z)` for a surface and copy z. The heading reads `Key points` for an implicit curve, `Points` when there are `point`s, and otherwise `Critical points`.
- **3D now looks like Desmos 3D (replaces the floor / viridis drawing above):**
  - The view is a cube, 360 tall. x and y are the window. z is `fixed_z`, or by default as wide as x and centred on 0 (`zWindow`), so the axes share one scale.
  - Each surface patch is clipped against the cube's top and bottom (`Surface3D.clip`, Sutherland–Hodgman), so a surface is cut off at the walls instead of squashed. Patches are shaded in their equation's colour, lambert-lit and stroked in their own colour to hide seams.
  - The 9 cube edges away from the nearest corner are drawn first. The z = 0 plane's grid (major/minor at the x and y ticks) and the axes through the origin (clamped to the walls, 1.75 pt) are cut into short lengths and depth-sorted with the patches. Front edges, arrowheads, axis names (`x`/`y` or the axis labels, `z`) and numbers go on top.
  - Hovering a patch reads `(x, y, z)` (`patchHit`: barycentric on the patch's fan, z from `graph_readout_z`), with dashed guides to the plane and the axes.
- **Several equations:**
  - `graph a; b; c` splits at top-level `;` (`splitGraphs`). The first is the graph. The rest are `GraphLayer`s marked `fromBar`.
  - The head's `+` adds a layer typed in the panel (`GraphEquations`: a colour dot, a text field and a remove button).
  - Every layer is sampled with the first one's options. In 2D a curve layer widens the fitted y range, and an implicit layer is sampled again over the final y window (aspect = y span / x span, y centre = its middle). In 3D, layer surfaces are drawn in the same cube.
  - Colours: the first is the accent, then red, blue, purple, orange, teal, grey (`layerRGB`). A layer's roots and critical points are rings in its colour. The readout dots every curve and reads the one nearest the pointer.
  - The critical-point chips list every equation's points, the bar's first (`allCritical` in `GraphPanel.swift`; on the 3D view only surface layers). With more than one equation each chip starts with a small square in its equation's colour. Hovering a chip or its mark highlights it as before.
- **Graph settings (head wrench → popover, `GraphSettings.swift`):**
  - Persisted in `GraphPrefs` (UserDefaults `graphPrefs`): text size (ticks 10 → 12.5 pt), reverse contrast (the plot drawn in the other appearance), grid on/off, cartesian or polar grid (circles a step apart, spokes every 15°), arrows (both ends of each axis), axis numbers, minor gridlines (a fifth of a step, a quarter of a 2), x/y axis shown, lock viewport (no pan or zoom; orbit still works), radians/degrees (overrides `angle_radians` for the graph).
  - Per graph, in `GraphState`: axis labels; `lo ≤ x ≤ hi` (sets the window); `lo ≤ y ≤ hi` (sets `fixed_y`, which pans and zooms with the view, and which Reset clears); in 3D, the second row sets `fixed_z`; steps; Zoom Square (y as many units per point as x); linear/log for x and y.
  - A log x axis sends `x_log: true` (new `GraphOptions` field), so the core samples evenly in log x. A log y axis is only a drawing transform; its auto y range comes from the positive y values of all curves. Ticks on a log axis are decades, with 2–9 as minor lines.
  - Braille mode and complex mode are not done.
- **Technical look (replaces the rounded/pill styling above for the graph panel):**
  - Monospaced type everywhere on the plot (ticks 9.5 pt, readouts 10.5 pt), the head's label, `x ∈ [lo, hi]`, the chips and the equation list.
  - The 2D plot sits in a square frame with a 0.75 pt edge, with tick marks out from the frame (4 pt major, 2 pt minor). Grids are 0.5 pt hairlines, and minor gridlines are on by default. Axes are solid at ink 62%. Curves are 1.6 pt.
  - Marks are 6 pt hollow squares. A picked mark is a filled square inside a 14 pt square.
  - Readouts are dashed crosshairs plus a square box with a hairline edge, reading `x  …` / `y  …` (/ `z  …`) on separate lines.
  - Chips are plain cells with a hairline edge, with no fill until lit. Equation rows are a 3 pt colour bar, the equation and a hairline under each. The section heading is uppercase mono.
  - 3D draws the surface with its mesh: each patch is filled and stroked in one call, the stroke being the fill × 0.62. The cube edges are hairlines, axis names are mono, and the hint is short mono text.
- **Graph performance:**
  - The plot is drawn in two passes. The base pass (graph, view, settings, scale) is cached as a bitmap. The pointer pass (crosshair, reading, picked mark) is a separate transparent layer over it, so moving the pointer never redraws the plot: 3D hover went from ~16 ms a move to under 1 ms.
  - The 3D base pass keeps its patches (`SceneCache`) for the pointer pass to hit-test.
  - Patch colours are made once per shade level (65 levels a surface), not twice a patch. A patch wholly above or below the cube is skipped before clipping, and the grid's corners are put in cube units once.

### Systems panel (`Bar/Systems.swift`)

- **Decimals:** with fraction form off, show `A = 7.5, B = 12.5`, not fractions. Call `lib/answer` `systemDecimals(display, sig_figs)` on the `solveLive` result unless it's a message. `systems.rs` calls `solveLive` directly at the moment. Toggling fractions with the panel open should re-solve.
- **Tab from the line:** Tab on the bar's line (`sys 2`) moves to the first equation.
- **Delete shortcut:** Ctrl+Backspace in an equation empties it wherever the caret is. In an empty equation it closes the system and clears the line. `systems.rs` already has a Ctrl+Backspace `reset`, so compare it with this.
- **Caret:** moving to an equation puts the caret at its end, not selecting the equation.

### Bar (`Bar/BarView.swift`, `Bar/BarModel.swift`)

- **Units on answers:** the preview shows the answer's unit. The copy block covers only the number, with the unit beside it. Hovering a unit in any answer shows its name, as in the expression (`unitName(forAnswer:)`).
- **Angle indicator:** the grey deg/rad appears only when no angle unit was typed.
- **Tip delay:** the bar's edge buttons (deg/rad, fraction, sig figs, clear) wait 1.5 s before their tip. Unit tips keep their normal delay.
- **Typed root sign:** drawn as one smooth stroke, with a gap under its bar like Desmos (`RootSign` and `RootStroke` in `Bar/RadicalText.swift`, `hideRootSigns()` in `Bar/InputField.swift`). Check how `line.rs` draws it.
- **Sheet autofill on a command line:** `formula` is itself a command, but still shows its completion. See the `.identities` case in `BarModel.evaluateLive`. Check `commands.rs` and `line.rs` do the same.

### Feet and inches with ⌥↑ / ⌥↓ (`Bar/BarModel.swift` `stepPrefix`)

- Before `stepPrefix`, call `engine/units` `stepFeetInches(value, shown, dir, sig_figs)`, where `shown` is the answer as shown now. It returns `{value, display}` or null. On a result, show that display and keep the value for the next step, as the prefix step does. On null, go on to `stepPrefix`.
- The steps run inches ↔ feet and inches ↔ feet: `6 ft 2 in` goes ⌥↑ to `6.16666666667 ft` and ⌥↓ to `74 in`. Windows steps in `line.rs` `step_prefix`.

### Currency rate on hover (`Bar/BarView.swift`, `BarModel.rateNote`)

- When the line converts between currencies (`100 usd to eur`), the main answer's tip reads `1 USD = 0.8909 EUR · ECB rate, Nov 5 · click to copy`. Get it from the bridge `lib/currency` `rateNote(text, rates JSON, now ms)`, which returns null for any other line. Remember the result for each line.

### Formula sheets (`macos/IdentityWindow.swift`, `Sheets/SheetData.swift`)

Windows opens all the sheets, but draws them in its old text style. To match the Mac:

- **Get the sheet from `identities::native_identity_sheet_by_id(id)`,** not `identity_sheet_by_id`. It fills in the Typst for the web's four sheets too. The web-parity data must stay as it is.
- **Maths in Typst, words in the app's font:**
  - Each string in `item.typst` is one cell (the lead cells, then the row). Compile it with `identities::sheet_math_document`.
  - A cell that is only a quoted string (`"Time"`) is words, so draw it in the app's normal font at 13 pt.
  - `item.note_typst` is a note that is maths, compiled as `#text(size: 10.5pt)[$…$]` inside `sheet_math_document`.
  - All other notes, and section, group and window titles, stay in the app's font.
  - Compile off the UI thread, the first time a sheet opens. `typst_row.rs` already draws Typst paths.
- **Row heights:** the tallest cell plus 10 pt, with 22 pt as the least. Cells are centred on the row's height.
- **Hover box:** on a typeset row, box only the text a click would copy. It is a rounded rectangle, radius 4, with a faint fill and a thin outline, and 3 pt of padding round the cell. Don't tint the whole row. It must fit the whole expression. `formulas` rows box the whole row.
- **Clicks:**
  - Copy on mouse down, not up, and show the box at once.
  - The copied box stays for 0.25 s, darker, with no fade or animation anywhere.
  - Over a label cell (`CT`, `Binomial`), box and copy the formula instead.
  - The copied text is `lead_copy[i]` for a lead cell when it isn't empty, otherwise `copy`.
- **Conversions:** `item.parts` draws the row as pieces in a line, 3 pt apart. A piece with a `copy` is boxed and copied alone (`2.54`, `cm`). A click anywhere else on the row boxes and copies the whole row.
- **Unit circle:**
  - `item.figure` is a complete Typst page of lines, and `item.spots` are its labels, each a document centred on (x, y) in page points.
  - A sheet with a figure shows only that section. The table is still there as the part `exact values`. Windows shows a blank row and the table until this is done.
  - No headings over the picture, centred, 680 pt square.
  - The window opens tall enough for all of it, up to the screen's height, even if that covers the bar.
  - Each label is hovered and copied alone, and each coordinate is separate.
- **Status text:** `click to open` on `formulas`, otherwise `click to copy`.
- **Colour:** draw in the theme's ink, in light and dark.
- **Filter:** a search field (app font, 13 pt, `Filter` placeholder, clear button) sits under the header; the rows start 60 pt down. It takes the keys whenever the sheet does, so typing filters live.
  - Each word of the filter (case and accents folded, `−` read as `-`) must appear in the row's text, note, copy, open line, lead copies or part copies, or in its group's or section's title (`torsion` keeps the Torsion section, `τ` its formulas). A word also matches a single word of the row with only its letters and digits kept (`tmax` finds `τ_max`).
  - A table's heading row (empty copy) stays when its group has a match. Groups and sections with none hide; the figure is never filtered.
  - No match anywhere: a quiet `no matches` line (12 pt, tertiary ink). The view scrolls back to the top on each change.
  - Esc clears the filter first; Esc on an empty filter closes the sheet.
  - Testing: the Mac dev build has `sheetfilter:<text>` (`drive.sh`), which sets the front sheet's filter as if typed, so it works while the app isn't in front. Give the Windows dev commands the same, if they drive sheets.
- **One window, a page at a time** (`IdentityWindowController` pages, `OverlayController.mainSheetWindow` / `sheetController`):
  - Every sheet the bar opens (`formulas`, `beams`, `ohm's law`…) goes in one sheets window, replacing the page. Its title, heading and height follow the page; the window keeps its top edge (and moves up if a taller page would run off the screen). The filter clears on each page.
  - A `formulas` row opens its sheet in the window it was clicked in. The same sheet again just scrolls to the part asked for.
  - Back and forward: ⌘[ and ⌘], two chevron buttons beside the close button (shown once there are two pages, greyed where there's nowhere to go), and a sideways two-finger swipe when the trackpad's "Swipe between pages" is on (`trackSwipeEvent`, the page following the fingers a little; a three-finger `swipe` too). The bar passes ⌘[ / ⌘] on to the front sheets window while it has the keys. Each page remembers where it was scrolled to. Windows: Alt+← / Alt+→ and the mouse's back/forward buttons as well.
  - A closed sheets window starts its pages afresh next time.
  - Right-click (or control-click) a row that opens a sheet: a menu with **Open in New Window**, which opens that sheet in a window of its own (cascaded, with its own pages).
  - Dev commands: `sheetback`, `sheetforward`, `sheetswipe:back|forward`, `sheetnew:<x>,<y>`, `sheetpages:<file>` (see `drive.sh`).

### Stacked fractions and powers (`Bar/StackedMath.swift`, `Bar/StackedLayout.swift`, `Bar/InputField.swift`)
- **Setting:** General → `Stacked fractions and powers`, key `qcalc.stackedMath`, **off by default**. When it's off, nothing changes.
- **The text stays plain maths:** `/` and `^` only change what's inserted, and the line keeps text the engine already reads, such as `(5x)/(32+x^(3))`.
  - Each slot is a bracketed group. Its brackets, the `/` and the `^` are drawn as nothing.
  - The numerator sits over a 1.2 pt ink bar on the math axis (0.25 em above the baseline), and the denominator sits under it. Both are 0.6× size and centred on the wider one.
  - A power is 0.6× size, raised 0.36 em. An empty slot shows the muted 0.62 × 0.8 em box the ∫ limits use.
- **`/`:** the term before the caret goes over the bar, and the caret goes into an empty denominator. A term is a number, a name, a function call, a bracketed group, a root, or a power or fraction made of them. A plain number or name stays as it is (`3/()`). Anything else is bracketed (`(5x)/()`). A selection becomes the numerator, and nothing before the caret gives `()/()`.
- **`^`:** inserts `^()` with the caret inside. A fraction or power right before it is bracketed first (`((1)/(2))^()`).
- **Typing after a stack:** a term typed right after a slot's hidden `)` gets a hidden `·` first (`(1)/(2)·x`), because the engine reads `(1)/(2)x` as 1/(2x). Backspace on that one term removes the `·` too.
- **Keys:**
  - `)` before a slot's hidden `)` steps out.
  - ↑ and ↓ move between the numerator and the denominator, to the place nearest across.
  - ← and → move one place in the text and skip the places that look the same as the one beside them (`StackedMath.dead`).
  - Backspace right after a stack steps into its last slot. At the start of a slot it takes the stack apart and leaves the slots' text in the line.
  - A click picks the nearest caret place, by height too.
- **Drawing on the Mac:** TextKit 1 never lets kern take a glyph's advance below zero. So the hidden `/` carries a `qcalc.stackPull` attribute, and `StackTypesetter` moves the pen back by that much. The caret inside a slot is drawn by the line, at the slot's height, and the system caret is hidden. `font` on the text view is the first character's font, which can be a hidden bracket, so the line uses `BarTextView.lineFont`.
- **A fraction that starts the line:** its numerator is centred by kerning the character before it, so a fraction at the very start (`42/12*12/23`, `1/12345`) had nothing to kern. The numerator sat at the bar's left end and the denominator, centred, stood off to the right of it. The line now starts that much further on instead: `qcalc.stackLead` on the first character, which `StackTypesetter` adds to where the line starts. The bar starts at the line's start.
- **↑ ↓ reach the fraction:** arrow keys carry the function-key flag, so AppKit offers them to `performKeyEquivalent` before `keyDown`. The bar's history ↑ ↓ took them there, and ↑ in a denominator picked a history row. `BarTextView.stackArrow` now runs first in both. On Windows, the fraction's ↑ ↓ should likewise come before the history's.
- **The caret, everywhere on a stacked line:** while the line has a stack, the line draws the caret at every place, at full height outside the slots. The system caret goes by the line's used width, which the pulled-back pen leaves wider than the text, so after `42/12 * 12/(23*12) 12` it stood far to the right of the last `12`. Every glyph also gets its own location from `StackTypesetter`, because TextKit works out where a glyph inside a run is from the font's advances. That left out the pulls and drew the caret at the start of the line inside the second fraction.
- **Room past the row:** the line's view reaches 12 pt above and below its 36 pt row (`BarTextView.stackRoom`, a negative vertical padding on `InputField`), with the line still centred. That way a fraction inside a denominator, like `1/(x/3)`, isn't cut off at the bottom. A click in that extra room still goes to the bar (dragging it) unless the line has a stack (`LineScroll.hitTest`).
- **The highlight fits the text:** `StackTypesetter.setLineFragmentRect` takes the pulls (and adds the lead) off the line's used width, so a selection's highlight and the line's end are where the text ends.
- **Selections never split a stack:** a selection inside one slot stays as it is. One that reaches past a slot grows to take the whole stack, plus the hidden `·` after it (`StackedMath.expand`, applied in the text view delegate's `willChangeSelection`). ⇧← ⇧→ move the end away from where the selection started one place at a time and skip the places the caret never stops. A drag from a click goes to the place nearest the pointer.
- **Deletes that keep stacks whole:**
  - ⌦ before a hidden `/ ^ ( ) ·` steps over it instead of deleting it.
  - ⌥⌫ ⌘⌫ ⌥⌦ ⌘⌦ inside a slot stop at the slot's edge. At the edge they act like Backspace or ⌦. Outside a slot they take whole stacks (`StackedMath.deletion`). The hidden `·` before what's taken goes with it when no term is left after it.
  - Emptying a slot that has no hidden brackets (the `42` of `42/(12)`, typed as `42/`) leaves an empty `()` with the caret in it (`StackedMath.emptying`). This applies to Backspace, ⌦, the word and line deletes, and cut. Without it, the `/` was left with nothing before it and the line showed `/(12)`.
- **Dev only:** the Mac dev build's `do:<selector>` drive command runs an editing action on the line (`do:moveLeft:`, `do:deleteWordBackward:`), which works when Q Calc isn't the frontmost app. Not needed on Windows.
- **Pasted or linear text:** `3/4` is stacked only where the engine reads it the same way. The numerator is a number, a name, a call or a group after an operator or at the start. Nothing that binds tighter follows the denominator (`2x/4` and `(a)/(b)c` stay flat).
- **Test:**
  - Type `5x/32+x^3`, press → twice and type `x`. The typeset preview should read (5x/(32+x³))·x.
  - Type `1/2`, press ↑, type `0`, press ↓, type `5`. You should get 10/25.
  - Use Backspace in each slot, and check light and dark.
  - Type `42/12*12/23*1212` and `1/12345` at the start of the line: each numerator should be centred over its denominator, with the bar under both.
  - Type `42/12`, →, `*12/23*12`, →, `12`. The caret should be right after the last `12`. Press ← through the whole line: the caret should be at the place it is in each time, and the right size.
  - Type `1/x/3` (a fraction in a denominator). Nothing should be cut off.
  - On the `42/12 …` line: Home, →, then ⇧→ three times. The first fraction should be selected whole and the highlight should fit it. Select All then Backspace should clear the line.
  - Home, then ⌦ twice. You should get an empty box over `12`, and typing `7` gives 7/12. Backspace twice after `42` should do the same.
  - With the caret in `23*12`, ⌘⌫ should take only what's before the caret in that denominator. ⌥⌫ on the last `12` should leave no stray `·`.

### The working, ⌃W (`Bar/Working.swift`, `Bar/BarView.swift`, `Bar/MathText.swift` `paths`)
- **Core:** `Engine::call_json("lib/steps", "steps", [text, options])` gives the steps for a line, or null. `options` is `EvaluateOptions` as JSON (`angleMode`, `sigFigs`, `fractionMode`, `spaceThousands`, `ans`, `variables`, `functions`). Each step is `{math, why, plain}`: Typst math, a short phrase, and plain text for copying. `lib/steps` `stepDocument` `[math]` gives the document a step compiles to (the preview's macros, 15 pt). The core already checks the last step against its own answer; the app also drops working whose last step's `plain` doesn't end in the shown `display`.
- **What has working:** `d/dx …` (first derivatives, also `at`), `∫ … dx` from the integral table, one equation in one unknown (linear or quadratic), unit conversions (`5 ft to m`), and differential equations. Nothing else, and never a chained line.
- **When:** ask after each live answer lands, on a worker thread of its own (never the keystroke path), keep only the newest line's answer. Typeset the steps only while the panel is up.
- **Hint:** with working, `show working · Ctrl+W` (11 pt muted, a button) sits at the composer's bottom right, left of the clear tool's corner (gutter + 44). `hide working` while the panel is up. Hidden while the hint line or a tutorial step shows.
- **Panel:** under the bar where the graph and systems panels go, same look: `--bg-tape`, a hairline on top, 12 / 16 / 16 padding. Head: `Working` 13 pt semibold, `click a step to copy it` and `⌃W hides` 12 pt muted. Rows: the number (12 pt muted), the typeset step, why at the right in 12 pt muted (up to 3 lines, 260 pt wide). Hover tints the row; a click copies `plain` with the copied flash. Scrolls past 320 pt.
- **Keys:** `working` action, default `ctrl+w` on both, rebindable, listed in Settings → Keybinds and the `?` keys column. Esc closes the panel before the cheat sheet. Hiding the bar closes it. With the panel up on a line without working nothing shows; it comes back on the next line that has working.

### The caret in ∫ Σ Π limits (`Bar/InputField.swift` `limitCaretRect`, `drawHoles`; `Bar/StackedLayout.swift` `stackCaretRect`)

- Native only, the way Wolfram does it. It works whether or not the stacked setting is on.
- In a limit, the line draws its own caret, sized to the limits' font (0.56 em) and standing on that limit's baseline: 0.36 em above the line for the upper, 0.2 em below it for the lower. The system caret is hidden. Take the line's baseline from the sign's glyph, not the limit's: the limit's glyph location already includes its raise or drop.
- In an empty limit, the caret stands in the middle of the limit's box, and that box is drawn a shade darker (muted at 32%, not 18%). Which limit the caret is in follows `slotOf` in `lib/bounds.ts`: the lower first, both ends counted.
- In the integrand, the caret is full size, as before.
- **Test:** type `∫`. A small caret should stand in the lower box, and that box should be darker. Type `0`: the small caret is after the 0, low. Type `..`: it moves into the upper box. Type `10`: it is after the 10, high. Type a space and `x^2`: the caret is full size. Then type `Σi=1..n`. Check light and dark.

### Sheet data, for reference

All of it is generated by `native/scripts/gen_native_sheets.py`, so edit that and not the JSON. Named formulas are sheets with `names`, and their ids start `formula: `. Parts have ids like `continuum mechanics: Beams`.

## 4. How to test the sheets

- Open every sheet from `formulas`, and also by typing its name.
- Type a part (`beams`, `torsion`, `trig`, `laplace pairs`, `pressure vessels`) and check only that part shows.
- Hover and click every kind of cell: a formula, a label, both sides of a transform, a conversion's number and unit, the unit circle's labels. Check the clipboard after each click.
- Try light and dark, and a small screen.
- Run the keystroke bench. `part_could_start` builds its word list on first use, so make sure ordinary typing doesn't get slower.
