# Q Calc

A Spotlight-style scientific calculator for Mac. Press **Control + Option + Space** to calculate without leaving the app you’re in.

Type `sin(90)`, `72 f`, or `$10 for lunch + 15% tip` — the answer updates as you type.

## Install on Apple silicon

You need an **Apple silicon Mac** (M1 or later) running **macOS 14** or later. Q Calc is built from this repository.

### 1. Xcode

Install [Xcode 26](https://developer.apple.com/xcode/) or later from the Mac App Store, open it once, then accept the license:

```bash
sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
sudo xcodebuild -license accept
```

Xcode 26 is required because the Mac app links [SoulverCore](https://github.com/soulverteam/SoulverCore), which ships a Swift 6.2 binary.

### 2. Node.js

Install Node.js 20 or later from [nodejs.org](https://nodejs.org) or Homebrew:

```bash
brew install node
```

### 3. Build and install

```bash
git clone https://github.com/maxconine/Instant-Calculator.git
cd Instant-Calculator
npm install
npm run mac:install
```

The first build downloads SoulverCore, compiles Q Calc, and copies **Q Calc.app** into `/Applications`. An older **Instant Solver.app** in Applications is removed.

To build without installing:

```bash
npm run mac
```

That writes `macos/dist/Q Calc.app`. If the clone is in **Documents** or **Desktop**, macOS may refuse to launch that copy because of folder security attributes — use `npm run mac:install` instead of opening the repo build directly.

### 4. First launch

```bash
open "/Applications/Q Calc.app"
```

The app is ad-hoc signed, not notarized. If macOS says it can’t be opened:

1. Control-click **Q Calc** in Applications and choose **Open**.
2. Or clear the quarantine flag, then open it:

```bash
xattr -cr "/Applications/Q Calc.app"
open "/Applications/Q Calc.app"
```

The Q Calc icon appears in the menu bar. Q Calc is a menu-bar app — it does not show in the Dock.

If **Control + Option + Space** does nothing, macOS is often using that shortcut for Input Sources. Turn the shortcut off in **System Settings → Keyboard → Input Sources → Edit**.

To start Q Calc at login, add it under **System Settings → General → Login Items & Extensions**.

## Using Q Calc

- **Control + Option + Space** shows the calculator. **Esc** or a click outside the window hides it. What you were typing is kept for a while (see **Keep unfinished** in the menu).
- Type as you would on a scientific calculator: `sin(90)`, `sqrt(2)`, `2^8`, `5!`. `pi` becomes π as you type.
- **⌃D** switches between degrees and radians. **⌃F** toggles fraction results.
- Answers show a closed form next to the decimal when both exist (`sqrt(3)/2 ≈ 0.866025`). Click either side to copy (or, in history, to insert). **⌘C** and `ans` follow **Answers → Exact** or **Approximate**.
- Unit conversions work too: `72 f`, `2 in to cm`.
- Natural-language math works in the Mac app via SoulverCore: `$10 for lunch + 15% tip`, `40 is what % of 90`, `3:45pm + 4 hr 10 min`.
- The answer updates as you type. Click it or press **⌘C** to copy. Type `ans` to insert it at the cursor.
- **Enter** saves the calculation to history. **Up arrow** or scroll the tape to see previous ones. Click a history answer — or either side of a dual answer — to insert it.

The menu bar icon can show the calculator without the hotkey. Significant figures, exact vs approximate answers, light or dark appearance, how long to keep unfinished input, and default units are set from the same menu.

## Development

The JavaScript engine can be tried in a browser. Natural-language phrases like `$10 for lunch + 15% tip` need the Mac app.

```bash
npm install
npm run dev
```

Open http://localhost:5173/

```bash
npm test
```

Runs the engine suite, including the scientific checklist in `src/engine/scientific.test.ts`.

## License

Q Calc is [MIT](LICENSE).

The Mac app embeds [SoulverCore](https://github.com/soulverteam/SoulverCore), a closed-source natural language math engine. SoulverCore may be used in personal and private projects. [Contact the authors](mailto:contact@soulver.app) before using it in a public or commercial project (they offer options, including a free license with attribution). `npm run mac` downloads the official xcframework at build time.
