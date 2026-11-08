# Mac changes to port and test on Windows

A running list, kept by whoever changes the Mac app. How to work through it: `PORT_RUNBOOK.md`. Newest additions go at the end of their section. When a line is done and tested on Windows, delete it.

**Before you start:** this work may not be committed or pushed yet. Check that `native/core/src/app/native_sheets.json` and `native/core/src/app/native_typst_cells.json` exist on your branch. If they don't, ask for the Mac work to be committed first.

**Mac-to-Windows key names:** ⌘ is Ctrl, ⌃ is Ctrl unless Windows already binds that chord, and ⌫ is Backspace.

**Spec:** `native/mac/PARITY.md` and the Mac sources named below. The rules are in the shared core, so don't copy them into Windows.

**Status of this list:** written from the Mac, then ported and run on Windows on branch `win-mac-port` (6 Nov 2026). New Mac changes go below as before.

## Checks to run first

```
cargo test -p qcalc-core
cargo test -p qcalc-ffi --features typst --test native_sheets_typst
cargo test -p qcalc-win
```

The `eval.blank` golden floor is now 1311, because two recordings of `∫ x dx` and one of `integral of x^2` expect a blank and they now answer. The `eval.units` floor is now 8988: two recordings want `2 ft + 3 in` as `2.25 ft` (it now answers `2 ft 3 in`), and one wants `1/2 in + 3/8 in` as `0.875 in` (now `7/8 in`). If Windows passes a different count, tell the Mac side rather than raising it.

On Windows (branch `win-mac-port`) all three pass, with the golden floors as set on the Mac. The `commands.rs` test now expects `idt summation` to open `identities: Summation` at section 0.

## 1. Comes from the core: just test it

Nothing left: every line was typed on Windows and matched. `chat max` needed the `chat` stem in `commands.rs`.

## 2. Edited on Windows but never run: test it

Nothing left.

## 3. Not on Windows yet: implement it

### Stacked fractions and powers, laid out like Desmos (`Bar/StackedLayout.swift`, `Bar/StackedText.swift`, `Bar/InputField.swift`, `Bar/BarView.swift`, `Bar/BarModel.swift`)
- **Sizes:** a fraction's slots are 0.82 of the line's size. A fraction inside a slot is 0.7 of that slot's size, as in TeX's script style. A power is 0.64 and raised 0.42 em (`BarTextView.slotSize`). Before, every slot was 0.6.
- **Gaps:** a numerator's baseline is the bar's axis + 2.5 + max(0.24 f, the depth of a stack inside it). A denominator's baseline is the axis − 3 − max(0.74 f, the height of a stack inside it), where f is the slot's size (`numOffset`, `denOffset`, `extent`). So a fraction inside a denominator never touches the bar above it. The bar overhangs each side by 0.2 f.
- **Room around signs:** with the setting on, `+ − = < > ≤ ≥ ± ≠` between two terms get 0.2 em each side, and `; ,` get 0.25 em after. A sign that starts a term (`-3`, `(−x`, `1e-5`) stays tight (`spaceOperators`). The caret beside a spaced sign is drawn in the middle of its room, never against the sign (`caretShift`), and it's placed after the spacing is applied.
- **No clipping:** the text container is 400 pt tall, not 40. TextKit cut a line's fragment down to the container and drew nothing past it, so a denominator was cut off. The line's view uses the composer's whole 72 pt (`stackRoom` 18). The baseline stays where a plain line's is, measured at a stack's hidden `/` (TextKit lowers it for a raised numerator, so the inset takes that back). A line too tall for that position moves up or down until all of it shows (`updateInset`).
- **No typeset preview** under the line while the setting is on: the line is already typeset (`refreshTypeset`).
- **History rows** draw their expression stacked the same way at 13 pt (`StackedText`), muted like the plain rows.
- **Test:**
  - Type `5x/32+x^3`, →, →, `;x=3`. It should look like Desmos: the slots nearly full size, a clear gap above and below the bar, and spaces around `+` and `=`. There should be no preview row, and the caret after `=` should sit in the gap.
  - Type `2^10+1/(x/3)+(1/2)/4;x=2` and `1/(x/3)`: nothing should be cut off at the top or the bottom.
  - Press Enter: the row in history shows the fraction stacked, in light and dark.

### Graph panel, newer Mac round (`Bar/GraphModel.swift`, `Bar/GraphPanel.swift`, `Bar/GraphSettings.swift`, core `graph.rs`)

- **Graph controls and fixes (this round):**
  - **Input:** the 2D plot now takes the mouse in the same AppKit layer as 3D (`PlotInput`), so a drag pans the view and never moves the window. Two fingers up/down (or a wheel) zoom any graph, about the pointer in 2D. Shift + two fingers pans. A pinch zooms and a double-click resets. In 3D, an up/down drag now tilts the other way (`dy` negated with `dx`).
  - **Zoom:** the head has zoom-out/zoom-in buttons (×1.25 / ×0.8). ⌘+ (or ⌘=) and ⌘− do the same while a graph shows (in the bar's key monitor in `OverlayController`).
  - **Camera button:** the plot is redrawn at 3× with each equation (a colour bar plus its text) above it. It is saved as `~/Desktop/Q Calc graph <date> at <time>.png` and copied to the clipboard as PNG. The hint says so.
  - **Settings inline:** the settings are now a block under the head, not a popover. A popover is a window of its own, and taking the keys hid the bar. The block uses the bar's look: monospaced rows (VIEW, X AXIS, Y/Z AXIS, SCALE) of hairline cells, in ink when on, with plain fields in hairline boxes.
  - **Editing keys:** the bar's window (`OverlayPanel.performKeyEquivalent`) gives ⌘A/C/X/V/Z to any text field other than the line. The bar has no Edit menu, so the graph's fields had none.
  - **Saving graphs:** ↵ on a `graph …` line saves it as a tape row. In the core, `LiveRequest.keep_graphs`, set by the FFI's `core_request` for every native app, so Windows gets it too. A click on the row brings the graph back. The web still saves only `graph f(x)=…`.
  - **Coefficients before z:** `20z = 2y + 12x` graphs as a surface. `native_intent`'s free-z test no longer refuses a z after a digit.
  - **Lines along an axis:** `x² = x` (the lines x = 0 and x = 1) no longer lists every sample of x = 0 as a y-intercept. `crossings` skips a zero whose neighbours are zero too.
  - **Samples:** a 2D curve gets 4001 samples, not 401, so a zoomed-out sin x is a dense band rather than a slow false wave. The count is odd so a sample lands on the window's middle. With 4096, x² had two level samples across its turn, and `find_critical_points` skipped it.
  - **Many points:** chips show the 12 points nearest the middle of the view, with "+N more". Expanded, they sit in a box that scrolls (156 pt high). Past 60 marks plus roots, no marks are drawn and the pointer doesn't snap.
  - **Every equation's points:** the chips list the added equations' points too, each with a small square in its colour.
  - **Square grid (as Desmos):** about 12 x ticks are asked for. For a fitted curve (no y window set, not log), y's step is the least 1/2/5 × 10ⁿ whose rows are as tall as x's columns are wide. The y window is centred on the curves (`squareYStep`), and y's minor lines split as x's do.
  - **Bounds to ∞ and endpoints (core, so Windows gets them by rebuilding):** `{3 < x < ∞}` (also `inf`, `infinity`, `-∞`) is accepted. `GraphRange` now records `lo_open`/`hi_open` (`<` is open, `≤` closed). `build_native_graph` adds new kinds `CriticalKind::Endpoint` (kept end) and `OpenEndpoint` (left-out end) at each finite x bound inside the window. An open end's height is the formula's value there, past the bounds. On Windows, `windows/src/graph.rs` names them "endpoint" / "open end". Draw a kept end as a filled square, an open one hollow.
  - **Home window:** in 2D, the home window is the union of every equation's home window, so a second equation bounded outside the first one's window still shows.
  - **Smooth dragging (`GraphModel.swift`):**
    - A resample no longer writes its x window back when it lands. Only a home view sets the window, so a result that arrives after the drag has moved on can't pull the view back.
    - The first drag or zoom on a fitted curve holds its y window (`holdY` sets `fixedY`). From then on x and y move together under the pointer, with `yMin`/`yMax` updated at once rather than after the resample. Reset clears it.
    - A held window keeps square cells: y's step is x's step at the same scale, when that is 1, 2, 2.5 or 5 × 10ⁿ.
  - **Home view shows the key points (core `native_home`, so Windows gets it by rebuilding):** `widen_to_features` looks 10 and 100 times wider than the page's home. It takes the roots (only ones the curve crosses, touches, or starts at), turns and the y-intercept. It stops when a wider window finds no more, or more than 12 (sin x keeps ±10). Then it widens the home to hold them all, with a third again around them; features already inside the window (not within 5% of its edges) leave it alone. `y = 2x + 34` opens on [−20, 4]. On the Mac, the fitted y range also reaches y = 0 when the curve comes within a third of its height of it.
  - **Numbers ride the axes (as Desmos):** x numbers sit under the line y = 0 and y numbers beside x = 0, on a small background patch. An axis off the view, or one that can't be drawn on a log scale, leaves its numbers at the edge it went off: the margins below and left, or just inside the top and right. The origin's 0 is written once. Log decades read 0.001 … 1000, then 10⁴, 10⁻⁵ (`decade`). Switching y between linear and log refits y (it clears `fixed_y`).
  - **Settings, cut down:** the panel keeps grid on/off, Cartesian/Polar, minor lines, numbers, the x and y (or z) ranges, their steps, linear/log and Zoom square. Each row has a line of explanation, in rounded cells inside a rounded card. Text size, invert colours, arrows, lock viewport, radians/degrees and hiding the axes are gone, and `GraphPrefs.load` resets them to their defaults.
  - **Export:** the camera opens an export block, not a menu (a menu or popover takes the keys, and the bar hides). It holds "Label key points" (on by default: each mark's `kind (x, y)` box, up to 16 that don't overlap), the axis names (moved here from the settings) and Save picture.
  - **Font:** the whole graph uses the system font, the plot with tabular digits (`monospacedDigitSystemFont`), not monospace.
  - **Snapping to roots:** a root within 9 pt of the pointer is snapped to, as critical points are. It is drawn picked out, labelled `root (x, 0)`.

Open questions for the Mac side:

- **Sheet filter:** the spec says `tmax` finds `τ_max`, but `τ` doesn't fold to `t` in the Mac's own matching, and Windows follows the Mac's code, so `tmax` finds nothing on either.

### Sheet data, for reference

All of it is generated by `native/scripts/gen_native_sheets.py`, so edit that and not the JSON. Named formulas are sheets with `names`, and their ids start `formula: `. Parts have ids like `continuum mechanics: Beams`.

## 4. How to test the sheets

- Try a small screen (the sheets were tried on a 4K screen at 150% only).
