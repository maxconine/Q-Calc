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
- **Sheet autofill on a command line:** `formula` is itself a command, but still shows its completion. See the `.identities` case in `BarModel.evaluateLive`. Check `commands.rs` and `line.rs` do the same.

### Feet and inches with ⌥↑ / ⌥↓ (`Bar/BarModel.swift` `stepPrefix`)

- Before `stepPrefix`, call `engine/units` `stepFeetInches(value, shown, dir, sig_figs)`, where `shown` is the answer as shown now. It returns `{value, display}` or null. On a result, show that display and keep the value for the next step, as the prefix step does. On null, go on to `stepPrefix`.
- The steps run inches ↔ feet and inches ↔ feet: `6 ft 2 in` goes ⌥↑ to `6.16666666667 ft` and ⌥↓ to `74 in`. Windows steps in `line.rs` `step_prefix`.

### Currency rate on hover (`Bar/BarView.swift`, `BarModel.rateNote`)

- When the line converts between currencies (`100 usd to eur`), the main answer's tip reads `1 USD = 0.8909 EUR · ECB rate, Nov 5 · click to copy`. Get it from the bridge `lib/currency` `rateNote(text, rates JSON, now ms)`, which returns null for any other line. Remember the result for each line.


### Sheet data, for reference

All of it is generated by `native/scripts/gen_native_sheets.py`, so edit that and not the JSON. Named formulas are sheets with `names`, and their ids start `formula: `. Parts have ids like `continuum mechanics: Beams`.

## 4. How to test the sheets

- Try a small screen (the sheets were tried on a 4K screen at 150% only).
