# Q Calc for Mac, native

The Mac app rebuilt without a web view for the calculator: AppKit and SwiftUI over the Rust engine
(`native/core`, through the `QCalcCore` Swift package). It looks and behaves like the current app; `PARITY.md`
tracks every feature against it, with paired pictures in `parity-screenshots/`.

The dev build is **Q Calc Dev** (`com.maxconine.qcalc.native-dev`). It has its own settings, history
(`~/Library/Application Support/Q Calc Dev/`), logs and default shortcut (**⌃⌥⇧Space**), and never checks for
updates, so it runs beside an installed Q Calc without touching it.

## For testers: install a build you were sent

1. Unzip `Q-Calc-Dev-<version>-<commit>.zip` and drag **Q Calc Dev.app** into Applications (or anywhere).
2. The build is signed ad hoc, not notarized, so macOS blocks the first open. Either right-click the app,
   choose **Open**, then **Open** again; or run once in Terminal:
   `xattr -dr com.apple.quarantine "/Applications/Q Calc Dev.app"`.
3. Q Calc Dev lives in the menu bar (the Q icon). Press **⌃⌥⇧Space** to show the bar, or use the menu.
   Change the shortcut in Settings → Keybinds.
4. To send feedback, include `~/Library/Logs/Q Calc Dev/overlay.log` (shows, hides and launch timing).

To remove it: quit from the menu bar icon, delete the app, and optionally
`~/Library/Application Support/Q Calc Dev` and `defaults delete com.maxconine.qcalc.native-dev`.

## Building it

Prerequisites: Xcode 26 (Swift 6.3 toolchain), the Rust toolchain in `native/rust-toolchain.toml` (rustup installs
it on first use, with both Apple targets), and, for games, Node 20 with `npm install` run at the repo root.

```sh
zsh native/mac/build.sh            # native/mac/dist/Q Calc Dev.app, universal (arm64 + x86_64)
zsh native/mac/build.sh --run      # …and launch it (quits an earlier dev build only)
zsh native/mac/build.sh --zip      # …and native/mac/dist/Q-Calc-Dev-<version>-<commit>.zip for testers
zsh native/mac/build.sh --arch=arm64 --skip-rust   # quick iteration on one architecture
zsh native/mac/build.sh --release  # Q Calc's own id, name, shortcut and Sparkle key (replaces the web-view app)
```

What it does:

1. `native/scripts/build-mac.sh --no-test`: the Rust static library per architecture, the UniFFI Swift bindings
   and `native/QCalcCore/qcalc_ffiFFI.xcframework`.
2. `swift build` of `native/mac/Package.swift` once per architecture (`--triple`), lipo'd into one binary. The
   package depends on `../QCalcCore` and compiles `Sources/` plus `Sources/Shared/`, symlinks to the AppKit code the
   web-view app already has (`macos/AppSettings.swift`, `Keybinds`, `Messages`, `MessageInbox`, `ECBRates`,
   `SettingsWindow`, `PeriodicWindow`, `IdentityWindow`, `SheetPlace`, `LoginItem`, `Updates`, `UnitlessCopy`,
   `OverlayHealth`; `scripts/link-shared.sh` recreates them).
3. The bundle: `Info.plist`, Sparkle 2.10.0 (from `macos/vendor`, fetched and checksummed like `macos/build.sh`, or
   `QCALC_SPARKLE_DIR`), `Resources/sheets.json`, the icon, and the games bundle; then ad hoc signing.

## Games and the 420 / 67 / 69 effects

The calculator never makes a web view. Pong, connect 4, ultimate, crack, equal and 24 / countdown are native: their
rules are in the Rust core (`native/core/src/games/`) and they draw in SwiftUI inside the shell in
`Sources/Games/Shell/` (its README says how a game plugs in; `Sources/Games/NativeGames.swift` lists them). The games
not ported yet (`ember`, `trails`) open a window with one `WKWebView` (`Sources/Games/GameWindowController.swift`)
loading `Contents/Resources/web/games.html#<kind>`; closing the window releases it and the WebContent process exits
(`parity/game-lifecycle.sh` checks this). The effects load `games.html?x=…#smoke|arms|fold` in a click-through
panel over the bar and tear down after the animation.

`games.html` is a Vite entry (`games.html`, `src/games.tsx`) that mounts the existing `GameShell` and game panels
with today's bridge messages (`game`, `gameSave`, `nickname`, `copy`, `escapeLayer`, `gameFullscreen`, plus
`effectDone`). `build.sh` builds that entry alone (`games.vite.config.ts`, into `native/build/games-web`) when
`node_modules` exists, and copies it into the app: none of the calculator's pages, chunks or the typst.ts WASM.
Without it the app still builds, and games say the build has none.

## Development tools

- `scripts/drive.sh` sends commands to a running dev build (show, type:…, key:…, snap:…, theme:…); release builds
  don't listen. `snap:` writes a PNG of every window, drawn by the app itself, plus its view tree.
- `parity/run.sh` renders each parity scenario with the current app's own page (`parity/WebReference.swift` loads
  `dist/quick.html` with the web-view app's boot flags) and with the dev build, in light and dark, into
  `parity-screenshots/`.
- `parity/measure.sh` launch time, memory (`footprint`) and WebContent processes for any build.
- `scripts/export-sheets.sh` regenerates `Resources/sheets.json` (periodic table and identity sheets) from
  `src/lib/elements.ts` and `src/lib/identities.ts`, until the engine's `elements()` and `identitySheet()` answer.

## Layout

```
Sources/App       App.swift (menu bar, hotkey, launch), OverlayController (the bar's window), DevSnapshot, Util
Sources/Bar       BarModel (state and keys), BarView, InputField (NSTextView), GraphPanel, Theme (the CSS values),
                  History, Symbols (typed words to symbols), TextRules (pretty answers, chaining, commands),
                  RadicalText, MathText (the Typst hook)
Sources/Engine    EngineService (QCalcCore glue, graph fallback), EngineThread (lanes, newest wins)
Sources/Games     GameWindowController, EffectOverlay
Sources/Settings  UnitSettings (choices from the engine)
Sources/Sheets    SheetData
Sources/Shared    symlinks into macos/
```
