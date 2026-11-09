## Q Calc 3.0.2: matrices, the working, and a bigger graph

**If you already have Q Calc, you don't need to do anything.** It updates itself to 3.0.2 the usual way, and your settings, keybinds and history come with it.

### Windows: rebuilt as a native app

Q Calc on Windows is now a native Windows app, written in Rust on Win32, Direct2D and DirectWrite, like the native Mac app. There's no web view inside any more, so it's faster and much lighter. It's the same calculator, with the same shortcuts and the same answers, and it has everything below too.

**If you already have Q Calc on Windows, you don't need to do anything.** Within a day it updates itself in the background while the calculator is put away. Your settings, keybinds, history and saved rates come with it. It stays in the same folder, Start menu entry and **Installed apps**, and keeps starting when you sign in if it did before.

- **Faster and lighter.** Answers are drawn in about a millisecond after each key, and hidden in the tray it uses under 10 MB of memory.
- **Messages, clips and files.** Type `message <name>` to send someone a message, and `messages` to see your conversations. Clips and files are end-to-end encrypted.
- **Notes in the corner** for a message or file that arrives while the bar is hidden. They never take focus.
- **More PCs.** Every Windows 10 and Windows 11, on x64 and Arm. No WebView2 or Visual C++ runtime needed.

### What's new on Mac and Windows

- **Matrices.** Type `matrix 2x2` (or `mat 2x2`) for a grid to fill in, or write one on the line. Q Calc handles determinants, inverses, eigenvalues, `Ax = b`, LU and other decompositions, subspaces, and questions asked in words like `find the eigenvalues of A`. Answers stay exact where they can, with fractions, square roots and unknowns inside the matrix. Row-operation steps come with them.
- **Show the working.** A line with steps, such as `d/dx x^2`, `∫ x e^x dx`, `3x + 2 = 11` or `5 ft to m`, says *show working · ⌃W* under its answer. Press ⌃W (Ctrl+W on Windows) to see each step.
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
- A root or π inside a matrix works: `det([sqrt(3) 1; 0 2])` is `2√3`, and `[√3 √2]` has two entries.
- Values after a line can be split by commas as well as semicolons: `x^2 + y; x=5, y=2` is 27.
- In a formula sheet's filter, `tmax` finds `τ_max` and `sx` finds `σ_x`.

### Downloads

| File | For |
| --- | --- |
| `Q-Calc-3.0.2-apple-silicon.zip` | Macs with Apple silicon (M1 and later) |
| `Q-Calc-3.0.2-intel.zip` | Intel Macs |
| `Q-Calc-3.0.2-x64-native-setup.exe` | most Windows PCs (Intel and AMD) |
| `Q-Calc-3.0.2-arm64-native-setup.exe` | Arm PCs (Snapdragon) |

On the Mac, unzip and drag **Q Calc** to Applications. It runs on macOS 12 and later. On Windows, run the installer: it installs for your account only, with no admin prompt, and replaces an older Q Calc in place.

### Known issues

- **The Mac app isn't notarized yet.** Opened by hand the first time, macOS may say it can't check it for malware: right-click **Q Calc** › **Open**, or allow it in System Settings › Privacy & Security. Automatic updates never show this.
- **The Windows installer isn't code-signed yet.** Windows SmartScreen may say *Windows protected your PC*: click **More info → Run anyway**. Automatic updates never show this.
- **Smart App Control.** New Windows 11 PCs often have *Smart App Control* on, which quietly blocks apps that aren't code-signed yet, including Q Calc. Check Windows Security → App & browser control → Smart App Control. If Q Calc still doesn't start, look in `%LOCALAPPDATA%\com.maxconine.qcalc\qcalc.log` and send us the file.
- If **Alt + Space** opens something else, such as PowerToys Run, choose another key under Keybinds in settings.

Found a problem? Use the [feedback form](https://forms.gle/acunzzzwuMZUmMiF9) or [open an issue](https://github.com/maxconine/Q-Calc/issues).
