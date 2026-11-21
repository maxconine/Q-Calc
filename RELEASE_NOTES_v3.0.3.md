## Q Calc 3.0.3: formula sheets that explain themselves, and hundreds of fixes

**If you already have Q Calc on a Mac, you don't need to do anything.** It updates itself to 3.0.3 the usual way, and your settings, keybinds and history come with it.

This is a Mac release. Windows stays on 3.0.2 for now.

### Formula sheets

- **Click an equation to have it explained.** Each explanation says what the equation is for, names every variable with its unit, lists when it holds, and works an example you can try.
- **Search that keeps up.** The filter answers as you type and searches every sheet. It finds rows by name, acronym, a symbol spelled out (`omega`, `zeta`), common misspellings, half-typed words and the phrase a course would use.
- **Dynamics.** The dynamics sheet covers Mitiguy's appendix: vectors, rotation matrices, angular velocity and acceleration, inertia, and a picture of every body drawn like the textbook's.
- **Every other sheet reviewed.** More rows, clearer notes, and Related links you can reach from the keys.

### New

- **Constants in words.** `mass of the earth`, `planck constant`, `Earth's axial tilt`, planets, materials and more.
- **Dates and times.** `now + 2 hours`, `now in tokyo`, `tomorrow 3pm in tokyo`, `days until 2027-01-01`, week numbers and unix time.
- **Feet and inches out.** `1.8 m to ft in` is `5 ft 10.866 in`.
- **Complex roots.** `x^2 = -4` answers `±2i` instead of "no real solution".
- **Notes and copy buttons in the tape.** ↵ on a line with no answer saves it as a note, and each row has its own copy button.
- **Calculate with Q Calc.** A Services item that swaps a selection in any app for its answer.
- **LaTeX and Typst input.** `\frac12`, `\log_{2} 8` and `\frac{d}{dx}` evaluate.
- **3D implicit surfaces** like `x^2+y^2+z^2=1`, and pinch-zoom about the pointer.
- **Slots**, a slot machine for free chips. Type `slots`.

### Better answers

- **Units.** Prefixed squares and cubes (`1 km2 to m2`), tesla and siemens, powers shown as superscripts (m², m/s²), `(12 V)^2` in V², and temperatures like `20 °C + 5 K`.
- **Circuits.** AC phasors with units, power factor, kVAR.
- **Measurements.** Uncertainties carry through variables, units and exponents.
- **Chemistry.** Grams ↔ moles of a formula, and half-reactions with electrons balance.
- **Calculus.** Integrals to ∞ that keep returning to zero, like `sin(x)/x`, now answer.
- **Exact forms.** `1/(1+√2)` answers `√2 − 1`.

### Fixes

- Undo treats a paste, a completion or an insert as its own step, in the line, equation fields and matrix cells.
- An unfinished line, such as `d/dx` or `5 m to`, says what goes next instead of showing a squiggle.
- VoiceOver reads the live answer, the tape, the graph settings and the games.
- Lighter when hidden: no timers run, and memory is freed after a while.
- Faster typing on long lines, and no stalls on absurd inputs.
- `e` beside units is Euler's number: `20 kg * (1 - 1/e)` answers.
- `e^(jπ) + 1` answers `0`, not `j0`.

### Downloads

| File | For |
| --- | --- |
| `Q-Calc-3.0.3-apple-silicon.zip` | Macs with Apple silicon (M1 and later) |
| `Q-Calc-3.0.3-intel.zip` | Intel Macs |

Unzip and drag **Q Calc** to Applications. It runs on macOS 12 and later.

### Known issues

- **The Mac app isn't notarized yet.** The first time you open it yourself, macOS may say it can't check it for malware. Right-click **Q Calc** › **Open**, or allow it in System Settings › Privacy & Security. Automatic updates never show this.

Found a problem? Use the [feedback form](https://forms.gle/acunzzzwuMZUmMiF9) or [open an issue](https://github.com/maxconine/Q-Calc/issues).
