# Mac UI plan: from the web view to SwiftUI/AppKit

An inventory of everything the user sees today and the native counterpart for each item. The engine calls are
listed as their TS names; `ARCHITECTURE.md` §4 gives the `Engine` API they map to. Pure logic named here
(`answer.ts`, `chain.ts`…) moves into the Rust core (WP-APP-LOGIC), so Swift only draws and routes keys.

Ground rules for the UI agent:
- **No WKWebView** except in `GameWindowController` (§9), created when a game launches and released when it closes.
- The overlay is the existing `OverlayPanel`/`OverlayController` window machinery. Keep its AppKit code (placement,
  health checks, click-away, Esc layers) and replace only the web view inside it with an `NSHostingView` of a
  SwiftUI `BarView`.
- Evaluate on one serial background queue through the `Engine`, never on the main thread, and drop stale
  generations (`ARCHITECTURE.md` §5).
- Text that shows maths uses `prettyAnswer` (minus signs, digit grouping) from the core; copy and insert use the
  plain answer, which re-parses.

## 1. The bar window (`macos/Overlay.swift`, `OverlayHealth.swift`, `SheetPlace.swift`)

| Feature today | Native |
|---|---|
| Borderless non-activating `NSPanel` at the top of the active screen, sized by the page's `size` message | same `OverlayPanel`; height from SwiftUI `preferredContentSize`/`GeometryReader`, posted to `applySize` |
| Remembered position per screen (`overlayPositions`), width (`qcalc.overlayWidth`), reset after `placeResetMinutes` unopened | unchanged AppKit code |
| Drag anywhere that isn't a control (`drag`, `overControl` messages) | `NSView.mouseDownCanMoveWindow` on the SwiftUI background; controls opt out |
| Edge resize grips, width clamped per screen | unchanged (`trackResize`, `resizeBar`) |
| Click away, or focus loss, hides the bar; Esc closes layers first (`escapeLayer`, `__qcalcEscape`) | an `EscapeStack` in the view model (sheets, menus, completion, selection, then hide), consulted by the existing monitor |
| Health checks and recovery (`OverlayHealth`: window server visibility, painted check, rebuilds) | keep; "painted" becomes the first SwiftUI layout pass instead of the web `painted` message |
| `webViewWebContentProcessDidTerminate` recovery, the dev-server load and the bundled `quick.html` | deleted |
| SwiftUI fallback bar (`OverlayView` + `MathEval`) | deleted: the native bar *is* the bar; `MathEval.swift` goes |

## 2. The input line (`QuickInput.tsx`, `Bounds.tsx`, `Radical.tsx`, `TimeLetters.tsx`)

A custom `NSTextView` subclass (one line, no wrapping) inside `NSViewRepresentable`. `TextField` can't do
attributed spans, ghost text or a custom caret.

| Feature | Native |
|---|---|
| Typed words become symbols (`sqrt`→√, `pi`→π, `theta`→θ, `sum`→Σ, `inf`→∞…; `WORD_SYMBOLS`), unless "Keep words" | the core's replacement function (WP-APP-LOGIC) run in `textDidChange`, caret mapped |
| Inferred grey parens on both sides (`inferParens`) | drawn as attributed ghost text; ↵ / caret-at-end commits them (`autofillParens`) |
| Completion ghost (`completionFor`, `COMPLETION_FUNCTIONS`/`UNITS`, contacts after `message`), accepted with Tab or → | ghost attributed suffix; Tab/→ in `doCommand(by:)` |
| Bracket pairing for `[`/`]` and Backspace over a pair | `insertText`/`deleteBackward` overrides with the same rules |
| Spans: units, known words (`unitSpans`, `knownWordSpans`), the blank-reason squiggle (`blankReason`), the missing-unit suggestion (`missingUnit`) | `NSLayoutManager` temporary attributes from `engine.spans` / `blankReason` |
| Radicals drawn over their argument (`radicalSpans`) | custom drawing in `drawBackground(in:)` over the span's glyph rects |
| Σ/∫ bounds slots (`Bounds.tsx`, `boundsIn`, `afterTyping`, `snapOffset`) | small inline `NSTextField`s positioned at the bound glyphs, logic from `bounds.ts` in the core |
| Faint `ans` before the text while chaining (`chainsFromAnswer`) | leading attributed ghost |
| Example rotation in the empty field (onboarding `EXAMPLES`, fade) | placeholder view with a `.transition(.opacity)` |
| ↑/↓ walk history; ⌥↑/⌥↓ step a unit's SI prefix (`stepPrefix`); ←/→ pick a history row's side | key handling in the text view, routed to the view model |
| Undo/redo by typing runs (`undo.ts`: runs merge within `UNDO_RUN_MS`) | the core's undo model; `NSUndoManager` disabled on the field |
| ⌘A/⌘X/⌘V with paste cleaning (`cleanPastedText`), plain-text paste only | `paste(_:)` override through the core |
| Space-thousands setting, typed-space multiply | engine option only |
| d/dt letters panel (`TimeLetters`: which letters vary with time, toggles, `timeVarying`) | a row of toggle chips under the input |
| Draft kept across hides (`draftSeconds`, `shouldRestoreDraft`) | `UserDefaults` `qcalc.draft` |

## 3. The live answer (`LiveAnswer.tsx`, `useSteadyAnswer`, `useAnswerForms`, `useFitFont`)

| Feature | Native |
|---|---|
| Answer slot with pretty answer, dual `exact ≈ decimal` (`dualLabel`, answer form setting exact/approx) | SwiftUI `Text` with two tappable parts |
| Tab cycles answer forms (decimal, fraction, exact, scientific, other units: `answerForms`) | view model keeps the form list for the current answer key |
| Steady answer: the last real answer, dimmed, while the input is briefly invalid (`STEADY_MS`, `isEditOf`) | timer in the view model, logic from `steady.ts` |
| Fit font: shrinks long answers to fit | `minimumScaleFactor` + `lineLimit(1)` |
| Click copies (unitless if "copy unitless"), each solve root is its own click; drag out | `onTapGesture` → `NSPasteboard`; `.onDrag` |
| Copied flash | `.animation` on a `copied` flag |
| Closed-form upgrade arriving later (`useClosedForm` worker) | `Task` on `engine.closedForm`, cancelled when the input changes |
| Solve display: letter before roots, "no solution" messages, `MAX_SHOWN_ROOTS` | from `LineAnswer`/`SolveInfo` |
| Typst preview row (`TypstPreview`, `typstPreviewUseful`) | `Image(cgImage)` from `qcalc-typst`; text fallback |
| Base answers, complex answers, lists, matrices | just text from the core |

## 4. History tape (`HistoryTape.tsx`, `historyShow.ts`, `history.ts`, `useTapeWheel`)

| Feature | Native |
|---|---|
| Rows of expression = answer, newest at the bottom; shown per setting: recent (5 min), always, only on ↑ (`recentStart`, `scopeStart`, `nextRecentExpiry`) | `LazyVStack` in a `ScrollView`; visibility rules from the core |
| Selection with ↑/↓, side pick with ←/→, ↵ inserts expression or answer (`historyInsert`, `insertableHistoryAnswer`, `historyPicks`), ⌥↵ inserts the other half | view model state; core helpers |
| ⌘⌫ deletes the selected row, ⌘Z undoes it | view model + undo snapshot |
| Clear history (bound key, default ⌃C) | action |
| Matrix and complex rows, solve rows with roots, a system row reopens its panel (`onOpenSystem`) | row view variants |
| Wheel/trackpad scrolling of the tape (`useTapeWheel`) | native scrolling |
| Persistence | `history.json` via the core (`ARCHITECTURE.md` §8) |

## 5. Panels in the bar

| Feature | Native |
|---|---|
| Graph (`GraphPanel.tsx`): `graph …` commands, curves, implicit/contour plots, pan and zoom, axes and ticks (`graphTicks`), roots and critical points markers, angle-aware x axis, home view (`graphHome`) | SwiftUI `Canvas` drawing `GraphResult` points; `MagnifyGesture`/drag; re-sample through `engine.graph` on view change (background queue) |
| Systems (`SystemPanel.tsx`): `sys` command, one row per equation, solved live (`solveLive`) | a `Form` of equation fields; answers from the core |
| Cheat sheet (`help`/`?`: `cheatSheet`, `hostCheats`) | sheet view from core data |
| Games list (`games`), game hints | list view; launching opens §9 |
| Edge tools: deg/rad, fractions, sig-figs toggles, clear, settings (`EdgeTools.tsx`), with key-labelled tooltips (`useKeyLabels`) | toolbar buttons with `.help()` tooltips |
| Message notices (`MessageNotices.tsx`, `messageBanner.ts`): incoming message cards with a reply line, send status (`sendStatusText`) | SwiftUI cards fed by `Messages.swift`/`MessageInbox.swift` (already native) |
| Tour line and onboarding hints (`TourLine`, `tour.ts`, `onboarding.ts`: `pickHint`, `HINTS`, rotation, `tutorial`/`skip`) | a footer line from the core's `hint`; tour state in `qcalc.onboarding` |
| Greeting reply (`hi` → `GREETING_REPLY`) | answer text |
| `message <name> …` command and contacts | parse via core (`parseMessage`), send via `Messages.swift` |

## 6. Sheets and windows already native

| Feature | Today | Native |
|---|---|---|
| Periodic table (`periodic`, `PeriodicCard.tsx` in-page, `PeriodicWindow.swift` native) | the page asks the host (`openNativePeriodicTable`) | keep `PeriodicWindow.swift`; element data from the core (`ELEMENTS`) instead of its Swift copy |
| Identity sheets (`identities`, `IdentityCard.tsx`, `IdentityWindow.swift`) | host window | keep `IdentityWindow.swift`; sheets from the core (`IDENTITY_SHEET`, `identityRuns`) |
| Settings (`SettingsWindow.swift` SwiftUI: general, answers, units `UnitSettings.swift`, keybinds sheet, messages `MessagesSettingsView`, updates, login item) and the web `SettingsPage.tsx` with the same rows | both exist | keep the SwiftUI window; drop `SettingsPage.tsx`; push changes to the view model instead of `notifyWeb` |
| Menu bar item (`AppDelegate` menu: Show, Settings…, Tips…, Tutorial, Check for Updates…, Quit, hotkey warning) | native | unchanged |
| Global hotkey (`Keybinds.swift` `GlobalHotKey`, Carbon), hotkey nudges | native | unchanged |
| In-bar keybinds (`keybinds.ts`: copy answer ⌘C, copy line ⇧⌘C, deg/rad ⌃D, fractions ⌃F, sig figs ⌃S, clear ⌃C, settings ⌘,) | page | a `KeyRouter` in the panel's `keyDown`/`performKeyEquivalent`, chords parsed by the core (`actionForEvent`, `normalizeChord`) |
| Sparkle updates (`Updates.swift`), login item (`LoginItem.swift`), unitless copy (`UnitlessCopy.swift`), ECB rates (`ECBRates.swift`) | native | unchanged |
| First run, tips, tutorial entry points | `showFirstRun`, `showTips`, `showTutorial` → page | call into the view model |
| Usage analytics (`analytics.ts`: daily counts, payload, opt-out `shareUsage`) | page + host storage `qcalc.analytics` | port the counting to the core, send from Swift with `URLSession` |
| Theme (system/light/dark) | CSS + `applyWebAppearance` | `NSApp.appearance` / `.preferredColorScheme` |

## 7. Keyboard map (the bar)

| Keys | Action |
|---|---|
| ↵ | commit the line (or insert a selected history row; send a message; start the tutorial; open periodic/identity sheets for those commands) |
| ⌥↵ | insert the other half of the selected row |
| Esc | close the innermost layer, then hide |
| ↑ / ↓ | history selection (or show history in "only on ↑"); in a graph or system panel, move between rows |
| ← / → | pick a selected row's expression or answers; → accepts completion at the end |
| Tab / ⇧Tab | accept completion; otherwise cycle answer forms |
| ⌥↑ / ⌥↓ | step the answer's SI prefix |
| ⌘⌫ | delete the selected row; in a system panel, reset to calculating |
| ⌘Z / ⇧⌘Z | undo typing runs; undo a deleted row |
| ⌘C / ⇧⌘C / ⌃D / ⌃F / ⌃S / ⌃C / ⌘, | rebindable actions (`KEY_ACTIONS`, Mac defaults) |
| ⌘W | close an open sheet, else hide |

## 8. Behaviour to keep exactly

- The answer appears as you type, with no Enter needed, and blank lines stay blank rather than showing errors.
- Chaining: an operator typed first continues from `ans` (`chainedExpr`), shown as the faint `ans`.
- Recent-history window (5 minutes) and `RECENT_ROWS`.
- Copy unitless by default; copy as Typst when `typstCopy`.
- Degree mode by default, 6 sig figs for new users (`DEFAULT_SETTINGS_SIG_FIGS`), rationalize on.
- Every setting round-trips through the same `UserDefaults` keys, so updating keeps the user's choices.

## 9. Games and easter eggs (web, on demand)

`GameWindowController` owns the only `WKWebView`, built on launch of a game command and loaded with
`games.html#<kind>` (Ember, Pong, Connect 4, Trails, Ultimate, Crack, Equal, Numbers), with today's bridge
messages (`gameSave`, `gameFullscreen`, `escapeLayer`, `nickname`, `copy`). Fullscreen, letterbox and Esc layering
reuse `setGameWindow`/`dressForGame`. The 420/67/69 effects (`FourTwentySmoke`, `SixtySevenArms`,
`SixtyNineFold`) trigger on the live answer, load `games.html#smoke` / `#arms` / `#fold` in a transparent
click-through web view over the bar, and are torn down when the animation ends. Verify in Activity Monitor that
no "Safari Web Content" process exists until a game or effect starts and that it exits after.

## 10. Suggested build order for the UI agent

1. `BarView` in the existing panel: input field (plain), live answer through `evaluate`, history tape (in memory).
2. Engine view model on a background queue with generations, plus history persistence.
3. Input richness: symbols, ghost parens, completion, spans, squiggle, chain ghost, undo.
4. Answer forms, steady answer, copy and drag, closed-form upgrade.
5. Panels: graph, systems, cheat sheet, tour/hints, message cards.
6. Game window and effects; delete the web bar, `quick.html`, `MathEval.swift`.
7. Typst preview when `qcalc-typst` lands.
