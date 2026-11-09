## Q Calc 3.0.2: matrices, the working, and a bigger graph

**If you already have Q Calc, you don't need to do anything.** It updates itself to 3.0.2 the usual way, and your settings, keybinds and history come with it.

### What's new

- **Matrices.** Type `matrix 2x2` (or `mat 2x2`) for a grid to fill in, or write one on the line. Q Calc handles determinants, inverses, eigenvalues, `Ax = b`, LU and other decompositions, subspaces, and questions asked in words like `find the eigenvalues of A`. Answers stay exact where they can, with fractions, square roots and unknowns inside the matrix. Row-operation steps come with them.
- **Show the working.** A line with steps, such as `d/dx x^2`, `∫ x e^x dx`, `3x + 2 = 11` or `5 ft to m`, says *show working · ⌃W* under its answer. Press ⌃W to see each step.
- **More calculus.** Antiderivatives with their steps, differential equations (`y' + 2y = e^x`, `y' = y; y(0) = 2`), and phrasings like `integral of x^2`. `x^2dx` works without a space.
- **Complex numbers** in `i` or `j`, in polar form, and with units.
- **Feet and inches.** `6'2" + 1'11"` adds the way you'd say it.
- **Money.** Sums like `$45 + $12.50`, conversions along the way, and the rate's day shown on hover.
- **Roots in answers.** `2^(1/2)` and `2^0.5` answer `√2`, and `8^(3/2)` answers `16√2`.
- **A bigger graph.**
  - Implicit curves and 3D surfaces.
  - Key points include where curves cross, and they're labelled and snap to the pointer.
  - Smooth dragging, zoom anywhere, a square grid, settings beside the plot, and a typeset export.
- **Formula sheets in one window.** Every sheet opens in one window, a page at a time, with back and forward.
- **Stacked fractions and powers (optional, off by default).** Turn it on in Settings › Answers to type maths as in a graphing calculator: `/` puts what you typed over a fraction bar and `^` raises a power. It's off unless you turn it on.

### Fixes

- A long answer shrinks as a whole and is never cut off. A root's number is never shown tiny or half-hidden.
- `ans * (10^5)/30` shows its typeset preview again.

### Downloads

| File | For |
| --- | --- |
| `Q-Calc-3.0.2-apple-silicon.zip` | Macs with Apple silicon (M1 and later) |
| `Q-Calc-3.0.2-intel.zip` | Intel Macs |
| `Q-Calc-3.0.2-x64-native-setup.exe` | most Windows PCs (Intel and AMD) |
| `Q-Calc-3.0.2-arm64-native-setup.exe` | Arm PCs (Snapdragon) |

On the Mac, unzip and drag **Q Calc** to Applications. It runs on macOS 12 and later.

### Known issues

- **The Mac app isn't notarized yet.** Opened by hand the first time, macOS may say it can't check it for malware: right-click **Q Calc** › **Open**, or allow it in System Settings › Privacy & Security. Automatic updates never show this.
- **The Windows installer isn't code-signed yet.** Windows SmartScreen may say *Windows protected your PC*: click **More info → Run anyway**. Automatic updates never show this.

Found a problem? Use the [feedback form](https://forms.gle/acunzzzwuMZUmMiF9) or [open an issue](https://github.com/maxconine/Q-Calc/issues).
