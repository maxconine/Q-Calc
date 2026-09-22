# Q Calc

A Spotlight-style scientific calculator for Mac. Press **Control + Option + Space** to calculate without leaving the app you’re in.

Type `sin(90)`, `72 f`, or `$10 for lunch + 15% tip` — the answer updates as you type.

## Install on Apple silicon

You need [Homebrew](https://brew.sh), an **Apple silicon Mac** (M1 or later), and **macOS 14** or later.

```bash
brew tap maxconine/qcalc https://github.com/maxconine/Instant-Calculator
brew trust --tap maxconine/qcalc
brew install --cask q-calc
xattr -cr "/Applications/Q Calc.app"
```

Or run the install script, which also turns on daily upgrades of Q Calc:

```bash
/bin/zsh -c "$(curl -fsSL https://raw.githubusercontent.com/maxconine/Instant-Calculator/main/macos/install.sh)"
```

The app is ad-hoc signed, not notarized. Homebrew 7 also requires trusting this tap before the cask will load. If macOS still refuses to open it:

1. Control-click **Q Calc** in Applications and choose **Open**.
2. Or clear the quarantine flag, then open it:

```bash
xattr -cr "/Applications/Q Calc.app"
open "/Applications/Q Calc.app"
```

The Q Calc icon appears in the menu bar. Q Calc is a menu-bar app — it does not show in the Dock.

If **Control + Option + Space** does nothing, macOS is often using that shortcut for Input Sources. Turn the shortcut off in **System Settings → Keyboard → Input Sources → Edit**.

To start Q Calc at login, add it under **System Settings → General → Login Items & Extensions**.

### Updates

New versions are published as GitHub Releases and picked up by Homebrew. On a Mac that already has Q Calc:

```bash
brew update
brew upgrade --cask q-calc
```

For field Macs that should take updates on their own, enable Homebrew autoupdate so only Q Calc is upgraded:

```bash
brew tap domt4/autoupdate
brew autoupdate start --upgrade --immediate --only=q-calc
```

That checks once a day (and at login) and installs the latest cask when you ship a release.

## Using Q Calc

- **Control + Option + Space** shows the calculator. **Esc** or a click outside the window hides it. What you were typing is kept for a while (see **Keep unfinished** in the menu).
- Type as you would on a scientific calculator: `sin(90)`, `arcsin(0.5)`, `sin^-1(0.5)`, `sqrt(2)`, `2^8`, `5!`. `pi` becomes π as you type.
- **⌃D** switches between degrees and radians. **⌃F** toggles fraction results. **Answers → Rationalize denominators** controls whether exact answers like `5/sqrt(41)` are rewritten as `5sqrt(41)/41`.
- Answers show a closed form next to the decimal when both exist (`sqrt(3)/2 ≈ 0.866025`). Click either side to copy (or, in history, to insert). **⌘C** and `ans` follow **Answers → Exact** or **Approximate**.
- Unit conversions work too: `72 f`, `2 in to cm`.
- Natural-language math works in the Mac app via SoulverCore: `$10 for lunch + 15% tip`, `40 is what % of 90`, `3:45pm + 4 hr 10 min`.
<!-- Apple Dictionary — uncomment to restore:
- Type a word such as `ingenious` to see its Apple Dictionary definition. Phrases work too: `define New York`, `definition of apple`, `what does pi mean`. Click the definition or press **⌘C** to copy it. **Enter** saves the word to history.
-->
- The answer updates as you type. Click it or press **⌘C** to copy. Type `ans` to insert it at the cursor.
- **Enter** saves the calculation to history. **Up arrow** or scroll the tape to see previous ones. Click a previous expression to insert it at the typing cursor. Click a previous answer — or either side of a dual answer — to insert that value (`cos(` then a previous `31` becomes `cos(31)`). Press **Enter** on a highlighted row to reuse it; **History** in the menu (or the History setting in the browser) chooses whether that brings in the original expression or just the answer.

The menu bar icon can show the calculator without the hotkey. Significant figures, exact vs approximate answers, whether to rationalize denominators, whether history inserts the expression or the answer, light or dark appearance, how long to keep unfinished input, and default units are set from the same menu.

## Development

The JavaScript engine can be tried in a browser. Natural-language math needs the Mac app.

```bash
npm install
npm run dev
```

Open http://localhost:5173/

```bash
npm test
```

Runs the engine suite, including the scientific checklist in `src/engine/scientific.test.ts`.

### Build the Mac app from source

Install [Xcode 26](https://developer.apple.com/xcode/) or later from the Mac App Store, open it once, then accept the license:

```bash
sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
sudo xcodebuild -license accept
```

Xcode 26 is required because the Mac app links [SoulverCore](https://github.com/soulverteam/SoulverCore), which ships a Swift 6.2 binary. Node.js 20 or later is also required (`brew install node`).

```bash
npm install
npm run mac:install
```

That compiles Q Calc and copies **Q Calc.app** into `/Applications`. An older **Instant Solver.app** in Applications is removed.

```bash
npm run mac          # macos/dist/Q Calc.app
npm run mac:package  # macos/dist/Q-Calc-<version>.zip for Homebrew
```

If the clone is in **Documents** or **Desktop**, macOS may refuse to launch the repo build directly — use `npm run mac:install` or the Homebrew cask.

### Ship an update to field Macs

Before anyone can `brew install`, publish the first zip by tagging the current version (`v2.0.0`) or by running **Release** from the Actions tab.

For later versions:

1. Bump `"version"` in `package.json`.
2. Commit, then tag and push that version:

```bash
git tag v2.0.1
git push origin main v2.0.1
```

The **Release** GitHub Action builds the zip, publishes it on GitHub Releases, and updates `Casks/q-calc.rb`. Field Macs that used Homebrew (and autoupdate, if enabled) install that build on the next upgrade. You can also run **Release** from the Actions tab without pushing a tag.

## License

Q Calc is [MIT](LICENSE).

The Mac app embeds [SoulverCore](https://github.com/soulverteam/SoulverCore), a closed-source natural language math engine. SoulverCore may be used in personal and private projects. [Contact the authors](mailto:contact@soulver.app) before using it in a public or commercial project (they offer options, including a free license with attribution). `npm run mac` downloads the official xcframework at build time.
