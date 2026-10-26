# Parity: native app vs the current app

Every row of `native/MAC_UI_PLAN.md`, checked against the native dev build. Pictures of the same input in both apps,
light and dark, are in `parity-screenshots/<scenario>-<theme>-{current,native}.png` (`parity/run.sh`): `live`, `dual`,
`history`, `select`, `graph`, `banner`, `games`, `help`. The "current" pictures are the web-view app's own page
(`dist/quick.html` with its boot flags, `parity/WebReference.swift`), so they are pixel-true to what users see.

Status: **✅ verified** in the native build (driven by `scripts/drive.sh` and pictured, or read back from the app's
view tree), **🟡 ported** (the code is there and builds, not exercised end to end here), **⏳ waits** on an engine
package (the native side is wired and lights up when it lands), **❌ missing** (not built yet; listed, not dropped).

All look values come from `src/index.css` (`Sources/Bar/Theme.swift`): `--bg` at 90% with the tape at 90% `--bg-tape`
(no blur, as today), 12 pt corners, 72 pt composer with 8 × 16 padding, 24 pt input and answer at −0.021 em, 20 pt
dual halves around a 17 pt ≈, 36 pt tape rows (13 pt muted expression, 17 pt answer at −0.015 em, 5 visible),
edge tools 11 pt medium shown on hover, accent green `#1d7a4c` / `#63d392`, ink caret.

## 1. The bar window

| Feature | Status | Notes |
|---|---|---|
| Borderless non-activating panel, status-bar level, all spaces | ✅ | `OverlayController.makePanel` (Overlay.swift's) |
| Height from content, composer anchored (tape grows up, panels down) | ✅ | `BarSize` → `applySize`; pictures in `history`, `graph` |
| Remembered position per screen, width, reset after `placeResetMinutes` | 🟡 | same code and keys as Overlay.swift |
| Drag top/bottom strips, double-click to put back | 🟡 | same monitor |
| Edge resize grips (8 pt), width clamped | 🟡 | same `trackResize` |
| Click away / focus loss hides; Esc closes layers first | ✅ | Esc order: reply field, cards, sheets, selection, then hide |
| Health checks and recovery | ✅ | OverlayHealth verdicts; seen recovering while the display slept |
| Solid card, 90% opacity, 12 pt corners, system shadow | ✅ | all pairs |

## 2. The input line

| Feature | Status | Notes |
|---|---|---|
| Typed words become symbols (`sqrt`→√ …), "Keep words" | ✅ | `Symbols.swift` (WORD_SYMBOLS, AT_ONCE, LONGER_WORDS, SHORTCUT_SYMBOLS); `dual` pair |
| Inferred grey parens (`inferParens`, `autofillParens`) | ✅ | engine `inferParens`, drawn muted before and after the text |
| Completion ghost, Tab / → accepts | ✅ | `LiveAnswer.completion`, else engine `completionFor` (`foo` → `foot`) |
| `[` pairing, `]` step-over, backspace in `[]`, wrap a selection, `+-` → ± | ✅ | `Symbols.bracketKey/wrap`, `BarTextView` |
| Unit / known-word colouring, blank-reason squiggle, missing-unit offer | ✅ | `unitSpans`, `knownWordSpans` as temporary colours; `blankReason` after 700 ms idle and on a failed ↵ (`enterHint`); `missingUnit` drawn muted after the number, ↵ again takes it (`10 mA + 5 / 1 kohm`) |
| Radicals drawn over their argument | ✅ | bar over the radicand in the field, tape and answer (`RadicalText`) |
| Σ/∫ bounds slots | ✅ | `boundsIn`: limits drawn small beside the sign (upper over lower, hidden `_ ^ ..`, holes for empty ones); caret snaps (`snapOffset`); `afterTyping`, `boundKey` (→ Tab space ^ ↑ ↓ ↵ ⌫) and `wordToSign` |
| Faint `ans` while chaining | ✅ | `history` pair |
| Example rotation in the empty field | ✅ | `exampleList`, every 2.5 s with the fade, answers evaluated live, until the first keystroke (`Tour.swift`) |
| ↑/↓ history, ←/→ pick a side | ✅ | the web's pick model (`historyPicks`, side kept row to row); `select` pair |
| ⌥↑/⌥↓ unit prefix step | ✅ | engine `stepPrefix` + `formatValue` |
| Undo by typing runs | ✅ | the web’s `undo.ts` model (runs merge within 1 s, ⌘Z / ⇧⌘Z), NSTextView undo off |
| ⌘A/⌘X/⌘V, plain one-line paste | ✅ | `BarTextView.paste` |
| Space-thousands, typed-space multiply | ✅ | engine option |
| d/dt letters panel | ✅ | chips from `LiveAnswer.timeLetters`; a click moves a letter (`EvalOptions.timeVarying`) |
| Draft kept across hides (`qcalc.draft`) | ✅ | |
| Writing Tools / "Write with Siri" off on every field | ✅ | `writingToolsBehavior = .none`, `.noWritingTools()`, inline predictions off |

## 3. The live answer

| Feature | Status | Notes |
|---|---|---|
| Pretty answer, dual exact ≈ decimal, answer form setting | ✅ | `live`, `dual` pairs |
| Tab cycles forms | ✅ | `Engine.answerForms` (bridged) with the same cycle built locally meanwhile |
| Steady answer while briefly invalid | ✅ | |
| Fit font | ✅ | shrinks to half before truncating |
| Click copies (unitless setting), each half its own copy, drag out | ✅ | |
| "✓ copied" flash | ✅ | caption, accent-hover, 1.2 s |
| Closed-form upgrade | ✅ | `Engine.closedForm` with a CancelToken, cancelled by the next keystroke |
| Solve display: letter, roots each clickable, messages | ✅ | `x =` copies all, each root copies at full precision, `, …` past the shown ones; no-root outcomes as a message |
| Typeset preview row | ⏳ | hook `MathText.swift`; waits on `qcalc-typst` (the row stays hidden rather than showing a plain-text stand-in) |
| Command messages in the answer slot (`periodic table`, `↵ play pong`, message hints) | ✅ | as `.live.message` |

## 4. History tape

| Feature | Status | Notes |
|---|---|---|
| Rows, newest at the bottom, recent / always / only on ↑ | ✅ | `history` pair; recent window 5 min, 5 rows |
| Selection, picks, ↵ inserts the pick, ⌥↵ the other half | ✅ | |
| ⌘⌫ deletes the row, ⌘Z restores it | ✅ | |
| Clear history (⌃C, edge tool) | ✅ | |
| Matrix / complex / solve rows; a system row reopens its panel | ✅ | engine displays; a `sys` row reopens its equations |
| Scrolling | ✅ | native |
| Persistence `history.json` | ✅ | `Engine.loadHistory/saveHistory` in `~/Library/Application Support/Q Calc Dev/` |

## 5. Panels in the bar

| Feature | Status | Notes |
|---|---|---|
| Graph: head, plot, grid, ticks, curve, pan, zoom, reset | ✅ | `graph` pair: same layout, colours and sizes |
| Graph: home view, roots and critical points, hover readout | ✅ | `graphHome`, `graphTicks`, `graphReadoutY`, `GraphResult` roots and critical points; graph pair matches |
| Systems panel (`sys`) | ✅ | `sys N` opens N fields (`sysCommand`), solved live by `engine/system` `solveLive`, ↵ / ↑ ↓ between fields (arrows need macOS 14; Tab on 13), saved as a `system` row |
| Cheat sheet (`?`, `help`) | ✅ | `help` pair, keys follow the keybinds |
| Games list | ✅ | `games` pair (↑ from the bottom, ↓ off the end) |
| Edge tools with key tooltips | ✅ | hover-only, ink pills after 450 ms |
| Message cards with reply, ask cards, 10 s auto-hide, held on hover/focus/draft | ✅ | `banner` pair; expiry verified at 11 s |
| Tour line and onboarding hints | ✅ | open, pause (1.5 s) and commit hints (`pickHint`), the walkthrough’s line with `n/m` and skip, `tutorial` / `skip`, extras on later opens |
| Greeting reply | ✅ | `hello` (not `hi`) answers `hi 👋` in the answer slot; ↵ clears |
| `message <name> …` send | ✅ | `Messages.send` (added to the shared Messages.swift) |

## 6. Sheets and windows

| Feature | Status | Notes |
|---|---|---|
| Periodic table (formula, clear button, ↵ puts the mass in and starts over) | ✅ | shared `PeriodicWindow.swift`; data `sheets.json` |
| Identity / calculus / conversions / constants sheets | ✅ | shared `IdentityWindow.swift`; view tree read back (picture capture can't draw its labels) |
| ⌘W closes the front sheet, then the bar | ✅ | driven: ⌘W closed the identity sheet with the table still up |
| Settings: General, Units (choices from the engine), Messages, keybinds sheet | ✅ | shared SwiftUI; `Units` uses `unitSettingGroups()` |
| Menu bar item, hotkey, nudges | ✅ | shared code; dev hotkey ⌃⌥⇧Space |
| Sparkle (same appcast and key in `--release`; off in dev), open at login, unitless copy, ECB rates | ✅ | rates handed to `Engine.setRates` |
| First run, tips, tutorial entry points | ✅ | first run leads the examples with the shortcut; tutorial from settings and the menu starts the walkthrough |
| Usage analytics | ✅ | `analytics.ts` counting (`bump`, `calcKind`, `dueToSend`, `usagePayload`) at the web’s events; release builds send, dev builds only count |
| Theme system / light / dark | ✅ | all pairs in both |

## 7. Keyboard map

| Keys | Status |
|---|---|
| ↵ commit / insert pick / commands | ✅ |
| ⌥↵ other half | ✅ |
| Esc layers then hide | ✅ |
| ↑ / ↓ history, games list | ✅ |
| ← / → picks; → accepts completion at the end | ✅ |
| Tab / ⇧Tab completion, answer forms | ✅ |
| ⌥↑ / ⌥↓ prefix | ✅ |
| ⌘⌫ delete row | ✅ |
| ⌘Z / ⇧⌘Z | ✅ (typing runs, and a deleted row) |
| ⌘C ⇧⌘C ⌃D ⌃F ⌃S ⌃C ⌘, (rebindable) | ✅ |
| ⌘W sheet, else hide | ✅ |

## 8–9. Behaviour kept, games and effects

| Feature | Status | Notes |
|---|---|---|
| Answer as you type, blank lines blank | ✅ | |
| Chaining from `ans` | ✅ | `Engine.chains` |
| Copy unitless by default; copy as Typst | ✅ | `copiedEquation` on copies from the field when “Copy Typst compatible” is on |
| Degrees, 6 sig figs, rationalize by default; same UserDefaults keys | ✅ | |
| Games in an on-demand web view | ✅ | `parity/game-lifecycle.sh`: WebContent 1 → 2 with pong open → 1 after closing |
| 420 / 67 / 69 effects | 🟡 | click-through web view over the bar, torn down after 3.6 s; the effect pages reuse the components, not pixel-compared |

## Where the logic runs

Every rule above is the TS's own. The Rust engine answers what it has ported (through `Engine` and
`Engine.callJson`). A few `src/lib` functions it doesn't register yet (`bounds` `afterTyping`/`boundKey`/
`wordToSign`/`isHidden`/`snapOffset`, `blankReason`, `missingUnit`, `tour`, `analytics`, `typstMath`
`copiedEquation`) run as the TS itself in JavaScriptCore (`Resources/applogic.js`, built by `applogic/build.sh`): an
in-process interpreter, not a web view. `AppLogic.call` tries the engine first, so each moves to Rust with no change.

Still waiting: the typeset preview row needs a Typst renderer (`qcalc-typst`). The engine gives the Typst source
(`typstSource`) and `MathText.swift` is the hook; the row stays hidden until then rather than showing something
different from today's.
