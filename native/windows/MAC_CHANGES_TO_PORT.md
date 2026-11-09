# Mac changes to port and test on Windows

A running list, kept by whoever changes the Mac app. How to work through it: `PORT_RUNBOOK.md`. Newest additions go at the end of their section. When a line is done and tested on Windows, delete it.

**Before you start:** this work may not be committed or pushed yet. Check that `native/core/src/app/native_sheets.json` and `native/core/src/app/native_typst_cells.json` exist on your branch. If they don't, ask for the Mac work to be committed first.

**Mac-to-Windows key names:** ⌘ is Ctrl, ⌃ is Ctrl unless Windows already binds that chord, and ⌫ is Backspace.

**Spec:** `native/mac/PARITY.md` and the Mac sources named below. The rules are in the shared core, so don't copy them into Windows.

**Status of this list:** written from the Mac, then ported and run on Windows on branch `win-mac-port` (6 Nov 2026). New Mac changes go below as before.

**Releases (3.0.2):** a `v*` tag now releases the Mac only, and Windows ships by a hand run of the Windows workflow. See the top of `RELEASE.md`.

**Linear algebra has its own list:** `PORT_linear_algebra.md` (matrices: the grid, `mat`, steps, words like `find eigenvalues of A`, cofactors, rotations, `A^(1/2)`). Branch `linear-algebra` is merged into `native` and `main`, so work through it as part of this list: run its checks, then its sections in order. When it's done, delete it.

## Checks to run first

```
cargo test -p qcalc-core
cargo test -p qcalc-ffi --features typst --test native_sheets_typst
cargo test -p qcalc-win
```

The `eval.blank` golden floor is now 1311, because two recordings of `∫ x dx` and one of `integral of x^2` expect a blank and they now answer. The `eval.units` floor is now 8988: two recordings want `2 ft + 3 in` as `2.25 ft` (it now answers `2 ft 3 in`), and one wants `1/2 in + 3/8 in` as `0.875 in` (now `7/8 in`). If Windows passes a different count, tell the Mac side rather than raising it.

On Windows (branch `win-mac-port`) all three pass, with the golden floors as set on the Mac. The `commands.rs` test now expects `idt summation` to open `identities: Summation` at section 0.

## 1. Comes from the core: just test it

- **`dx` touching the integrand** (core `calculus/mod.rs`): `∫_12^3 x^2dx` is −567 and `∫_0^1 xdx` is 0.5, as with a space. Before, a `dx` right after a digit or a letter wasn't taken as the variable, so a definite integral got no answer. A `d` that ends a longer word still isn't one. Tests: `integrals_with_a_touching_dx`, and `roots_and_fractions_as_the_stacked_bar_types_them` (in `app/live.rs`).

- **A halves power is a root** (core `simplify::half_power`, checked beside `wants_exact_form` in `evaluate.rs`): `2^(1/2)` (also the stacked bar's `2^(1/(2))`, `2^0.5`) answers `√2 ≈ 1.41421`, as `√2` does, and `8^(3/2)` answers `16√2 ≈ 22.6274`. A fraction needs its bracket: `2^3/2` is 2³ over 2. The web gives no exact form here, so the golden floors drop to `eval.arithmetic` 6889 and `eval.sheet` 4488 (the recordings of `2^0.5`, `16^(1/2)`, `∛2^-1.5` and the like want none). Tests: `closed_forms` (simplify.rs), `roots_and_fractions_as_the_stacked_bar_types_them`.

- **Faster graph intersections and graph sampling** (core `graph_native::native_intersections`, `scientific/compile.rs`, `eval/ops.rs`, `eval/mod.rs`). Nothing changes on screen: the results are bit-for-bit the same as before. Each curve is now sampled once instead of once per pair. Bisection stops when it has converged. A graph's sample skips building a `Value`. Two plain numbers skip the complex, matrix and units dispatch. A one-argument call (`sin x`) has no argument `Vec`. Intersections are 48% faster for 2 curves and 82% faster for 6 (`cargo run --release -p qcalc-core --example new_features_bench`). Tests: `curves_cross_at_exact_points`, plus the whole core suite.

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
- **Roots over stacks:** a root whose bracket holds a fraction or power draws its sign as tall as what it takes. The bar sits 3 pt above the numerator, the foot is under the denominator, and the line is measured at the stack's hidden op. The line moves to leave room for the bar above (`hideRootSigns`'s drawing, `updateInset`). History rows draw it too (`StackedText`'s `.root`, with `sqrt(` shown as `√(`).
- **`^` after a fraction** brackets the whole fraction (`1/2`, →, `^3` is `(½)³`, not `1/2³`). `termStart` now steps over a plain numerator before a stacked `/`, not only a bracketed one.
- **Answers shrink as one:** an answer too wide for its room tries 100%, 85% and 70%, then 50% truncated (`ViewThatFits`; macOS 12 keeps the scale factor). The scale factor on its own let each piece shrink alone, so `√3/6`'s 3 came out small or half cut. A root's run is never squeezed (`RadicalText`).
  - **Roots in every answer** (`Pretty.answer`, `TextRules.swift`): a solve's `(5 ± sqrt(33))/2` and an answer with a unit (`sqrt(2) m`) draw `√` too; `±`/`∓` count as maths, and any `sqrt(` left beside words becomes a root. Test: `√2/3`, `√3/6`, `2√3`, `solve x^2-5x-2=0`, `sqrt(2) m`: the radicand is the same size as the text beside it and whole, in the bar's answer and the history row, light and dark.
- **Faint text on a stacked line sits on its baseline:** the completion's rest (`sq` then grey `rt`), a grey `)`, an angle mark and the grey lead were drawn at the top of the line's box, and a stacked line's box is taller, so they rode up like a power. They now go where full-size text on the line's baseline would (`fullTextTop`). Test: `2/12`, →, `√3 sq`: the grey `rt` lines up with `sq`; the same after `2^10 + co`.
- **Tests:** `native/mac/scripts/test-stacked.sh` runs headless checks of `StackedMath` (what stacks, with roots, and what `/ ^ ) ⌫` do). Port them as unit tests on Windows' copy of the same rules.
  - Type `sqrt(12/12^2`: the root covers the whole fraction, nothing is cut off, and the answer's `√3` is full size.
  - Press Enter: the row shows the root around the stacked fraction.
- **Sizes and gaps from TeX** (TeXbook Appendix G, under the OpenType MATH constant names; `TeX` in `StackedLayout.swift`). These replace the Sizes, Gaps and Roots-over-stacks numbers above. All in ems of the parent size S (the size the stack sits at):
  - **Slot sizes** (`slotSize`): a fraction's slots in the line are 0.9 S (MathQuill's/Desmos's 90%). A fraction inside a slot, and every power, is 0.7 S (TeX script). Nothing goes under 0.5 of the line's size (TeX scriptscript).
  - **Fraction (rule 15)** (`numOffset`, `denOffset`): the axis is 0.25 S above the baseline, and the bar is 0.04 S thick (at least 1 pt). The numerator's baseline is max(0.394 S, 0.25 S + 0.02 S + 0.12 S + its depth) above the line's baseline. The denominator's baseline is max(0.345 S, its height − 0.25 S + 0.02 S + 0.12 S) below it. Height is 0.74 of the slot size, or the stacks inside. Depth is 0.21 of the slot size when the slot's own text has a tail (`g j p q y , ; ( ) [ ] |`), else 0, or the stacks inside.
  - **Power (rule 18)** (`supShift`): the exponent's baseline is max(0.363 S, its depth + 0.108 S) above the base's. When the base is grown brackets, it is also at least the base's height − 0.25 S.
  - **Roots (rule 11):** the bar sits θ + ¼ x-height (0.04 S + xHeight/4) over what's under it; the foot is θ under its depth. `updateInset` leaves that room plus the line width above.
  - **History rows** (`StackedText`) use the same sizes and raise (`slotSize`/`supShift` with `line:` = the row's size), the TeX gap less a line's own padding, the same rule thickness and the same root gap.
  - **A stacked history row is never cut off** (`51f90fa`, `BarView.swift`, `StackedText.swift`): a power is raised without taking height, a fraction inside a fraction is smaller, and a stacked row uses the row's full height. Before, `4/(233*212^3)` made the fraction taller than its row. Test: Enter on that line, then check the history row shows the whole fraction, in light and dark.
- **Brackets that grow (new):** shown `( )` and `[ ]` around a stack (including a root's own, and a function's call) are drawn as tall as they hold, like TeX's `\left \right`. Their glyphs are hidden. The half-height from the axis is max(0.901 δ, δ − 0.5 S, S)/2, where δ = 2·max(height − axis, depth + axis). Round ones are a crescent, square ones a bar with feet (`TallBracketShape`). Each gets up to 0.25 S more room on its inner side (`addKern`). The line's extent, a power over them, and the root around them all take them in. They are parsed into `StackedMath.Parsed.tall`, and the history draws them too (`.group`).
- **Keys as MathQuill (Desmos) has them** (`StackedMath.swift`, `InputField.swift`):
  - **`/`** takes everything back to an operator, comma or space, as before. It now also takes `|x|`, `%`, `°`, and a stack with the term typed after it (`½x/` puts ½x on top). A function's call `sin(x)` goes over as it is, unbracketed.
  - **`^`** with nothing to raise does nothing (supSubsRequireOperand), including `^^`. Right after a power it goes back into that power, at its end (`x²`, `^3` is `x^(23)`). With a selection, the selection becomes the power.
  - **`+ − = < > ≤ ≥ ≠ ±`** typed at the end of a non-empty power leave it first (`x^2n+y` is x^(2n)+y). A hidden `·` after the power goes. A sign in an empty power stays (`x^-2`).
  - **`√` / `∛` over a selection** puts it under the root, with the caret at its end inside.
  - **⌫** right after a stack with nothing in it takes it whole (`x^()`, `()/()`).
  - **⌦** before an empty stack takes it whole. At a slot's end, with the stack's hidden end next, it takes the stack apart (`(1|)/(2)` ⌦ is `1|2`).
  - **↑ ↓:** with a stack right after the caret, ↑ goes to the numerator's or power's start and ↓ to the denominator's start. With one right before it, they go to the numerator's, power's or denominator's end. In a power, ↓ leaves it: to after it from its end, else to before it. Numerator ↔ denominator goes to the nearest place across, as before. Otherwise ↑ ↓ are the bar's (`StackedMath.vertical`).
  - **⌘← ⌘→ / Home / End** (and with ⇧, selecting) go to the slot's start or end first, then to the line's (`lineEdge`).
- **Fixes before release:**
  - **A line too tall for the bar** (a fraction in a denominator under a root, `√(1/(1+1/x))`) uses TeX's text style: fractions in the line at 70% instead of 90%, and slots θ (0.04 em) off the bar instead of 3θ. `fits(p)` measures the line at 90% first (`BarTextView.fractionTop`, the `top:` argument of `slotSize`, `extent`, `supShift`, `numOffset`, `denOffset`; `TeX.gap(top)`).
  - **Nothing past the text container is cut off:** after the text view draws, the clip is reset to the view's bounds. It had been left at the text container, which starts about 12 pt down, so a root's bar over a tall line was cut. A line with stacks or a root is also always redrawn whole (`setNeedsDisplay` override): a part redrawn alone left roots cut and caret marks behind.
  - **A root inside a slot** (`(√2)/(3)`) is drawn at its slot's size and where the slot's glyphs are drawn, from the sign's start to the end of its last glyph (`stackCaretX` + the glyph's own width). Before, it was drawn at the line's size and its bar stopped halfway over the 2. `extent` makes room for a root's bar (`RootSign.top`).
  - **History rows:** a root over a plain radicand (`√2`) gets its bar (it was the bare glyph), and a root's group keeps its size (it showed `√…`).
  - **The blank-answer squiggle** on a stacked line goes under the lowest character it marks, not under the line's whole box (near the bottom edge).
- **Not ported, on purpose:** `)` in a slot still steps out of it (MathQuill would bracket the slot's text), because the plain line has a hidden `)` there. `√2/3` stays (√2)/3, because the plain line's root takes only `2`. `(a+b)/` hides the brackets. MathQuill's Ctrl-Home has no key of its own: a second ⌘← does it.
- **Tests:** `test-stacked.sh` has 120 checks now, with MathQuill's typing, SupSub, backspace, delete, updown, select and charsThatBreakOutOfSupSub cases.
  - Type `x^2+1/2`, ↑, `3`, ↓, `5`: the line is x²+13/25.
  - Type `2(1/(2)+1)^(3)` and `sqrt(1/2`: the brackets and the root grow around the fraction, and the 3 sits above the bracket's top. Check in light and dark. The caret before the `)` stays outside the bracket.
  - Type `√2/3`: the root's bar covers the whole 2 at the numerator's size. Type `sqrt(1/(1+1/x`: the fractions get smaller, and the root's bar and foot both show.

### Graph panel, newer Mac round

Ported on `win302-graph` (graph.rs, graph_view.rs, graph_settings.rs, graph_export.rs). Left as on Windows:

- **Pinch and two-finger gestures:** Windows takes a touchpad's pinch as Ctrl + wheel (it zooms about the pointer, as before); a two-finger swipe arrives as the wheel. Shift + wheel pans.
- **Hover tips** on the head's new buttons (zoom out, zoom in, camera) aren't drawn: the head's icons had none on Windows.

Open questions for the Mac side:

- **Sheet filter:** the spec says `tmax` finds `τ_max`, but `τ` doesn't fold to `t` in the Mac's own matching, and Windows follows the Mac's code, so `tmax` finds nothing on either.

### Sheet data, for reference

All of it is generated by `native/scripts/gen_native_sheets.py`, so edit that and not the JSON. Named formulas are sheets with `names`, and their ids start `formula: `. Parts have ids like `continuum mechanics: Beams`.

## 4. How to test the sheets

- Try a small screen (the sheets were tried on a 4K screen at 150% only).
