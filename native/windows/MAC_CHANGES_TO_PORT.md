# Mac changes to port and test on Windows

A running list, kept by whoever changes the Mac app. Newest additions go at the end of their section. When a line is done and tested on Windows, delete it.

**Mac-to-Windows key names:** ⌘ is Ctrl, ⌃ is Ctrl unless Windows already binds that chord, and ⌫ is Backspace.

**Spec:** `native/mac/PARITY.md` and the Mac sources named below. The rules are in the shared core, so don't copy them into Windows.

**Status of this list:** written from the Mac; nothing here has been run on Windows.

## 3. Not on Windows yet: implement it

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
- **Pasted or linear text:** `3/4` is stacked only where the engine reads it the same way. The numerator is a number, a name, a call or a group after an operator or at the start. Nothing that binds tighter follows the denominator (`2x/4` and `(a)/(b)c` stay flat).
- **Test:**
  - Type `5x/32+x^3`, press → twice and type `x`. The typeset preview should read (5x/(32+x³))·x.
  - Type `1/2`, press ↑, type `0`, press ↓, type `5`. You should get 10/25.
  - Use Backspace in each slot, and check light and dark.
  - Type `42/12*12/23*1212` and `1/12345` at the start of the line: each numerator should be centred over its denominator, with the bar under both.

