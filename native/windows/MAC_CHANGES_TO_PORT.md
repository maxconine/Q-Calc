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

## 2. Edited on Windows but never run: test it

- **`ans` in the typeset row:** `ans * (10^5)/30` shows the number, not the word. `model.rs` (`ans_value`), `typst_row.rs`, `word_tip.rs`.
- **Typeset look:** `tan⁻¹(x)` has no brackets round `tan⁻¹`, and a root's bar sits clear of what's under it. `typst_row.rs` uses `native_typst_document`.

## 3. Not on Windows yet: implement it

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

- Try a small screen (the sheets were tried on a 4K screen at 150% only).
