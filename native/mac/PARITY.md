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
| **Native-only addition:** Increase Contrast, Reduce Transparency | ✅ | the page has neither. With Increase Contrast `--muted` (3.3:1 on the light tape, under WCAG AA) becomes `#636366` / `#aeaeb2` (5.5:1 / 6.3:1) and the hairlines go from 8% to 30% and the picked row's fill from 8% to 16%; with either setting the card and tape are opaque instead of 90%. Switching it redraws the bar. The native games take the same muted and hairlines (`GameColors`). Pictured light and dark through `drive.sh contrast:on` (a dev-only stand-in, since the system setting isn't changed here) |

## 2. The input line

| Feature | Status | Notes |
|---|---|---|
| Typed words become symbols (`sqrt`→√ …), "Keep words" | ✅ | `Symbols.swift` (WORD_SYMBOLS, AT_ONCE, LONGER_WORDS, SHORTCUT_SYMBOLS); `dual` pair |
| Inferred grey parens (`inferParens`, `autofillParens`) | ✅ | engine `inferParens`, drawn muted before and after the text |
| Completion ghost, Tab / → accepts | ✅ | `LiveAnswer.completion`, else engine `completionFor` (`foo` → `foot`). **Native only:** with no completion, Tab (or → at the end) turns the grey inferred parens into typed ones where they are drawn, the caret kept by the same character and nothing on the line moving (`acceptParens`), and the grey parens go when the line is emptied (the page's do too; native left the last line's drawn) |
| `[` pairing, `]` step-over, backspace in `[]`, wrap a selection, `+-` → ± | ✅ | `Symbols.bracketKey/wrap`, `BarTextView` |
| Unit / known-word colouring, blank-reason squiggle, missing-unit offer | ✅ | `unitSpans`, `knownWordSpans` as temporary colours; `blankReason` after 700 ms idle and on a failed ↵ (`enterHint`); `missingUnit` drawn muted after the number, ↵ again takes it (`10 mA + 5 / 1 kohm`) |
| Hover label naming the word under the pointer (`.quick-unit-tip`) | ✅ | core `wordNames` (`lib/knownWords`, native only): units by their `unitSpans` name as the web does, and functions (`gcf` greatest common factor) and constants (`π` pi, `e`, `i`, `φ`, `∞`) too; a child window over the word, 350 ms on the first rest, at once after that and after an edit under the pointer, gone when the pointer leaves the line (`drive.sh point:x,y` / `point:off`) |
| Radicals drawn over their argument | ✅ | bar over the radicand in the field, tape and answer (`RadicalText`); a solve's `±2sqrt(2)` too (the ± had kept it as plain text in the answer and the tape). Pictured |
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
| Answer forms | ✅ | `Engine.answerForms` (bridged) with the same cycle built locally meanwhile. **Native only:** ⌃E shows the answer in exponent form (`3 × 10³`, core `exponent_form`, units kept) and back; ⌃⇧E cycles the other forms (Tab fills parens instead); both are rows in Settings › Keybinds (`exponent`, `forms`). The page cycles forms on Tab |
| Steady answer while briefly invalid | ✅ | |
| Selected part answered on its own (`selDisplay`, `.live.live-selection`) | ✅ | was missing: selecting `3*4` in `2 + 3*4` shows a muted `12` in the answer slot until the selection goes (evaluated alone, no history; not for the whole line, a graph, or a part that reads as its value), read out as the page's `aria-live` is; its own engine lane. Pictured |
| Fit font | ✅ | shrinks to half before truncating |
| Click copies (unitless setting), each half its own copy, drag out | ✅ | |
| "✓ copied" flash | ✅ | caption, accent-hover, 1.2 s |
| Closed-form upgrade | ✅ | `Engine.closedForm` with a CancelToken, cancelled by the next keystroke |
| Solve display: letter, roots each clickable, messages | ✅ | `x =` copies all, each root copies at full precision, `, …` past the shown ones; no-root outcomes as a message |
| Typeset preview row | ✅ | `qcalc-typst` paths drawn in `--ink` (`MathText.swift`), shown exactly when `typstShown` is (setting, no command panel, `typstPreviewUseful` of the chained line); the answer is the shown form (`typstAnswer`), a solve's letter after ⇒, nothing for a message; rejected mid-edit input keeps the last picture at 40% |
| Command messages in the answer slot (`periodic table`, `↵ play pong`, message hints) | ✅ | as `.live.message`; cut at the end like the page (`text-overflow: ellipsis`; it had been cut at the start, so a narrow bar showed `…essages · ↵ opens settings` and not why). **Native only:** hovering one, or the hint under the line, shows it whole |

## 4. History tape

| Feature | Status | Notes |
|---|---|---|
| Rows, newest at the bottom, recent / always / only on ↑ | ✅ | `history` pair; recent window 5 min, 5 rows |
| Selection, picks, ↵ inserts the pick, ⌥↵ the other half | ✅ | the tooltips are the page's titles (`Edit equations`, `Insert word at the cursor`, `Insert expression at the cursor`, `Insert exact value / approximation at the cursor` by the answer form); they said `Insert the expression` / `Insert the answer` for every row. **Native only:** an expression past 40 characters, cut at the row's end, is in its tooltip whole. Built; tooltips not pictured |
| ⌘⌫ deletes the row, ⌘Z restores it | ✅ | |
| Clear history (⌃C, edge tool) | ✅ | |
| Matrix / complex / solve rows; a system row reopens its panel | ✅ | engine displays; a `sys` row reopens its equations |
| Scrolling | ✅ | native |
| Persistence `history.json` | ✅ | `Engine.loadHistory/saveHistory` in `~/Library/Application Support/Q Calc Dev/` |

## 5. Panels in the bar

| Feature | Status | Notes |
|---|---|---|
| Graph: head, plot, grid, ticks, curve, pan, zoom, reset | ✅ | `graph` pair: same layout, colours and sizes |
| Graph: home view, roots and critical points, hover readout | ✅ | `graphHome`, `graphTicks`, `graphReadoutY`, `GraphResult` roots and critical points; graph pair matches. Fixed against GraphPanel.tsx: the critical-point chips wrap onto more rows (`flex-wrap`; an HStack had squeezed six of them into columns a character wide on a narrow bar), are buttons that copy x (`Copy x`), and show 8 figures ungrouped (`formatCoord`); no points says `None in this domain`; the ticks are the engine's labels as given (they had read `50,000` beside `−50000`). Pictured at 340 pt |
| Systems panel (`sys`) | ✅ | `sys N` opens N fields (`sysCommand`), solved live by `engine/system` `solveLive`, ↵ / ↑ ↓ between fields (arrows need macOS 14; Tab on 13), saved as a `system` row. **Native only:** the fields open as soon as N is typed (the page waits for ↵) with the caret in the first, a new N keeps the fields it still has, Tab / ⇧Tab go round them and → at the end of one types in its inferred parens (macOS 14+) |
| Cheat sheet (`?`, `help`) | ✅ | `help` pair, keys follow the keybinds. **Native only:** the things-to-type column also lists `messages`, `paste to max`, `send file to max` and `konami` before `?` (core `native_type_cheats`); a bar too narrow for both columns' labels (they ended in "…" at 340 pt) shows one column that scrolls past 12 rows (macOS 13+; 12 keeps two). Pictured light and dark at 680 and 340 pt |
| Games list | ✅ | `games` pair (↑ from the bottom, ↓ off the end) |
| Edge tools with key tooltips | ✅ | hover-only, ink pills after 450 ms |
| Message cards with reply, ask cards, 10 s auto-hide, held on hover/focus/draft | ✅ | `banner` pair; expiry verified at 11 s. The empty reply line's "Reply to max" is muted like the page's `::placeholder` (`mutedPrompt`): SwiftUI never passed the prompt's colour to AppKit, which used its own lighter placeholder grey (about 1.8:1), and `drive.sh snap` pictures drew it in ink (as the native pair shows); the same for the thread's reply line, the `sys` fields and 24 / countdown's entry, pictured light and dark. **Native only:** Tab on an empty line moves to the newest card's reply line (the line keeps Tab for completion and forms, so the keyboard had no way in), esc comes back; driven and pictured. With VoiceOver on, cards stay 30 s instead of 10 (not tried with VoiceOver running). A new line from them in their open thread is read out the same way, since it comes there instead of as a card |
| Tour line and onboarding hints | ✅ | open, pause (1.5 s) and commit hints (`pickHint`), the walkthrough’s line with `n/m` and skip, `tutorial` / `skip`, extras on later opens; skip sets `TOUR_OFF` (it asked for `tourBit("off")`, which is 0, so skipping did nothing). **Native only:** a bar too narrow for the step wraps it to a second line (at 340 pt it was cut to `try it: ⌃⌥⇧Space hides me, ⌃⌥…`); pictured. The hint under the line wraps the same way, up to two lines |
| Greeting reply | ✅ | `hello` (not `hi`) answers `hi 👋` in the answer slot; ↵ clears |
| `message <name> …` send | ✅ | `Messages.send` (added to the shared Messages.swift) |
| **Native-only addition:** message history | ✅ | not in the web app. `Messages/MessageHistory.swift`: every message the relay delivered (`send`) and every message shown from an accepted name (`deliver`) is kept per contact with its time and direction in `<support folder>/messages.json`, the newest 200 each, read and written on a background queue. Requests nobody answered and blocked names never reach it; an allowed request's message does. Hooks in the shared `Messages.swift` sit under `#if QCALC_NATIVE` (defined only by `native/mac/Package.swift`), so the web-view app is unchanged. Checked with two re-signed dev copies on a local relay (`wrangler dev`): both sides' files, the 200 cap and a bad name dropped from a seeded file |
| **Native-only addition:** `messages`, `messages <name>`, `messages with <name>`, `open messages with <name>` | ✅ | not in the web app. Parsed by the core (`parse_messages_command`, `messages_hint`, `messages_completion`, `thread_contacts`, `thread_empty_text` in `messaging.rs`, with tests; `lib/messaging` arms in `golden.rs`). Hints `↵ your conversations` / `↵ opens messages with max`; Tab finishes names from the history, then recent recipients, then accepted ones (`messages with ma` → `max2`). Bare `messages` lists conversations latest first (name, last line with `you:` for yours, time), ↑↓ and ↵ pick. Pictured in light and dark |
| **Native-only addition:** VoiceOver for messages, statuses and modes | 🟡 | a new card is read out (`max wants to message you`, `max shared their clipboard`, `max sent a.pdf`, `message from max: …`, core `card_announcement`), as are each new hint under the line and tutorial step (both `role="status"` on the page) and a reply's `sent` or why it failed, in a card or a thread, since none of them takes the VoiceOver cursor and cards go after 10 s. A card has an "Open messages with max" action (the click on its name or words); a thread's clip or file line keeps its own label and its Copy / Save / Take Back buttons instead of folding them into one line; after Allow or Block the line has the keys again. ⌃D, ⌃F and ⌃S (or the edge tools) say where they left the mode: `radians` / `degrees`, `fractions on`, `sig figs from input off` (core `mode_announcement`), since the tools showing it are hidden off hover. ↑ ↓ through the tape, the games list and the conversations list read out the row they land on (VoiceOver stays on the line meanwhile); the games list is a "Games" group with the picked row selected, as the page's `aria-label` and `aria-selected`. Built and run; not listened to with VoiceOver |
| **Native-only addition:** drops with no name yet | ✅ | a drop on a line holding a calculation keeps it and says `clear the line to send files · or drop them on a thread` (it had swapped the line for `send file to `, losing what was typed); a drop of more than 20 files (the relay starts 20 uploads an hour per address) sends none and says `that’s 21 files · up to 20 at a time, or zip them into one` (core `DROP_MAX`, `drop_count_text`), where it would have mostly failed one by one; several files dropped on an empty line all wait for the name (`send 2 files to <name>`, core `dropped_label`), not just the first; a Finder alias or a symlink sends the file it points to (checked headlessly with `URL(resolvingAliasFileAt:)`; it had sent the alias file itself); a folder (or an `.app`) says `a folder can’t be sent · zip it first` (core `file_error_text("folder")`) instead of `couldn’t read that file`, and folders in a mixed drop are left out; an earlier drop's word is cleared once the line asks for a name. A file or clip sent with no name set (a drop on a thread) says `set your name in settings to send files` (or `…to share your clipboard`), not `your name’s been taken`, and a copied folder through `paste to` gets the folder line too (core `share_status_text` `no-name`, `folder`, `empty-clipboard`). With no name, `paste to max` and `send file to max` say `set your name in settings to share your clipboard` / `…to send files · ↵ opens settings` (they borrowed `message`'s `…to send messages`; pictured). Driven with `drive.sh drop:` (dev only); the two-file hint is checked in the core's tests, since it shows only once a name is set. After `send file to max`'s picker (which takes the focus and hides the bar) the bar comes back, on the thread with them, where the file's line shows its progress; its statuses had gone to the hidden bar. Built only (it needs a name, and a picker on screen) |
| **Native-only addition:** thread view | ✅ | not in the web app. `Messages/MessageThreads.swift`, drawn like the cards (`--bg-tape`, hairline, gutter, 13 pt lines, 12 pt muted captions, 36 pt list rows, the cards' reply field). It shows the newest lines and scrolls back (260 pt tall at most). Its reply line sends through `Messages.send` and shows the cards' statuses (`sending…`, `sent`, `nobody’s called nobody9 · not sent`). Esc or ⌘W closes it before the bar; a new message from that person lands in the open thread, not as a card; a click on a message card's name or words opens its thread (the card's words are no longer selectable for that; the thread's are). An empty or unknown thread says `no messages with nobody9 yet`. Settings → Messages gets a History section with Clear Message History… (with a confirmation), and Remove / Unblock deletes that name's thread. The confirmation dialog's click wasn't driven, nor was scrolling back by wheel. An unsent reply is kept per name for the run when the thread closes (esc, ⌘W, ×, hiding the bar) and comes back with the caret after it; checked with esc and reopening. With no name set the empty reply line says `set your name to reply · ↵ opens settings` (core `thread_reply_prompt`) and ↵ opens Settings at Messages, as `message`, `paste to` and `send file to` now do too (they opened General); it had said so only after a reply was typed and sent. Pictured. Files dragged over a thread say where they'll go beside its reply line (`drop to send to max`, or why not, the core's `dropTarget`), as the bar's hint does for a drop on the bar; not dragged here (drive.sh can't drag), the words are the core's tested ones. The end-to-end key: with no name set the Keychain isn't touched (ⓘ in a thread had read it, and with a dev build beside Q Calc that is a Keychain password prompt while the bar hangs; seen, and the copy quit); builds other than Q Calc keep their own item (`<bundle id>.share-key`; Q Calc's is still `com.maxconine.qcalc.share-key`), so Q Calc Dev makes a fresh key once and its test contacts see it as changed. ⌘V with files copied (Finder's Copy) sends them as a drop would: in an open thread to that person, on a `message <name>` / `send file to <name>` line to that name (`pasteFiles`, in the panel's key monitor); on any other line ⌘V pastes text as before. `paste to <name>` sends every copied file, not just the first. ⌘V of text in a thread waits on the reply line as a clipboard chip (× or esc takes it off) and ↵ sends it as a clip, then any words typed with it. A 📎 before the reply line picks files for them. While a thread is open, or the line names someone (`message max2`, `send file to max2`, dropped files waiting), clicking another app doesn't hide the bar (`holdsOpen`), so files can be dragged in from Finder; esc, ⌘W and the shortcut still do. Files dropped on the menu bar's Q open the bar asking who they're for, or go to the open thread (`StatusDrop`). Receiving: the key is published each time the inbox connects (not only once a launch, which could leave the relay with a key this Mac no longer has), it's published again when a share doesn't open, and a clip or file that doesn't open (or isn't sealed) leaves a line in their thread, `… couldn’t be opened on this Mac · ask them to send it again`, where it had been dropped unseen; each is logged (`share in:`). Built, not driven |

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
| Tab completion and parens; ⌃E exponent form, ⌃⇧E other answer forms, both rebindable (native only; the page cycles forms on Tab / ⇧Tab) | ✅ |
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
| Games in an on-demand web view | ✅ | only ember & frost and trails now (another package is porting them); `parity/game-lifecycle.sh` still checks the web view comes and goes |
| 420 / 67 / 69 effects | 🟡 | native now (`Effects/EggOverlay.swift`): a click-through panel over the bar, Core Animation layers with the components' numbers and keyframes (22 puffs from the core, blurred 18 px; the two hands' weigh keyframes and cubic-béziers; the 6 and 9 spinning), reduced-motion variants; fires after the 200 ms settle through the core's `EggGate` (seen firing in the dev build: `effect smoke (native)`), torn down after `eggPlayMs`. The moving picture isn't captured by `drive.sh snap` (presentation layers), so not pixel-compared |
| **Native-only addition:** divide by zero | ✅ | not in the web app. `x/0` (a literal zero divisor, answer `undefined`/`∞`; core `divides_by_zero`) settles 200 ms, then a small black hole opens on the answer, a copy of it spirals in, the hole closes and the real answer fades back (1.7 s; the slot only hides it, `BarModel.answerVeiled`). Typing on ends it. Reduce Motion: a still hole beside the answer. Pictured mid-fall in light and dark via `drive.sh snap` (effect panel rendered from its presentation layers) |
| **Native-only addition:** Euler's identity | 🟡 | `e^(iπ)+1`, `1+e^(πi)`, `e^(iπ)`, `…=0`, `…=-1` (core `is_euler`; bare forms need answer 0/−1, or none, since the engine leaves `e^(i π)` blank) give a confetti burst (CAEmitterLayer, 0.1 s of births) and a pill under the bar, "the most beautiful equation" (2.4 s). Note pictured light and dark; confetti fires but an emitter can't be pictured from inside the app. Reduce Motion: the note only |
| **Native-only addition:** Pi Day | ✅ | March 14 (local): a small pie slice in the composer's left gutter; `pi`/`π` alone rolls π to 50 places out under the answer, as many groups as fit (2.3 s). Other days: neither (checked). Dev builds: `QCALC_PI_DAY=1` makes any day Pi Day. Reduce Motion: the digits fade in whole |
| **Native-only addition:** Konami code | 🟡 | ↑↑↓↓←→←→BA in the bar (`OverlayPanel.sendEvent` sees each key, the field still gets it; core `KonamiTracker`) toggles a green-phosphor CRT look for the run only (never saved): dark bar, a Core Image colour matrix + bloom on the content, 1 px scanlines every 3 px. Toggling on/off and the scanlines checked in light and dark; the CI tint itself isn't in `drive.sh snap` pictures (cacheDisplay skips content filters), only that 2 filters are attached. Typed out (or `konami`, listed in `?`), the answer slot says `↵ an old look` / `↵ back to the usual look` (core `konami_hint`) like a command; it showed nothing. Pictured both |
| **Native-only addition:** do a barrel roll | ✅ | the whole line `do a barrel roll` (any case, `!`/`.`) settles and a picture of the bar spins 360° once (1 s) while the bar hides; ↵ rolls at once if needed and clears the line without saving or a hint; no squiggle. Typing on ends it. Reduce Motion: skipped. Pictured mid-turn in dark |
| Ember & Frost | ✅ | native (`Games/Ember`): rules, levels, sessions, wire, hud logic and the sound score in Rust (`core/src/games/ember`); every shipped walkthrough (≈27 000 steps) and the engine tests' own ≈60 000 calls give the TS's state hash (`tests/games_golden.rs`); the wire text, camera, beams, score and solver match too. `emberDraw.ts`/`emberFx.ts` ported line for line onto `Canvas2D` (the canvas API over Core Graphics, drawn asynchronously by Core Animation each display frame); select grid with thumbnails, hud, banners, win card, light and dark pictured in the dev build; WebAudio's tones and filtered noise re-synthesised (`EmberSynth`). Online over the shell's `RelayTransport` (same messages as the web build) not played against a second machine here |
| Trails | ✅ | native (`Games/Trails`): round, bots, lockstep session and the arena's rounds/lines in Rust (bots move for move with the TS); the arena drawn as the page's canvas (glows as Core Graphics shadows); vs the computer pictured in the dev build; online not played against a second machine here |

## 10. Native games

Six games run without a web view: their rules, computers and online sessions are the TS ported one-to-one to
`native/core/src/games/` (with the TS tests ported beside them), drawn in SwiftUI (`Sources/Games/<Game>/`) inside the
native shell (`Sources/Games/Shell/`, see its README). Checked in the dev build against the web bundle
(`native/build/games-web/games.html`, the same CSS) side by side, dark theme; online against the web page over the real
relay.

| Feature | Status | Notes |
|---|---|---|
| Shell: the game in the bar's place, esc (full screen → lobby step → pause / "esc again to leave" → close), paused card, corner full-screen button, pause on focus loss | ✅ | as the page's game took the panel (composer hidden, the panel growing down from the composer's top): a borderless card at the bar's spot and width, at its level, as tall as the game (`GameShellView` measures it; the window follows), rounded with the bar's shadow. Esc from the lobby's first screen puts the bar back where it was; ⌘W and the hotkey take both away (a game that holds its link is hidden and comes back on the next show). It used to open a separate 900×620 titled window with the card in its top-left. `GameShellModel`, `escAction`/`pausedKey` from the core; esc, arm and leave checked in an online connect 4 |
| Full screen as the big window (⌃F, f, the buttons), boards growing with `--game-h` | ✅ | the card's window becomes a titled, centred, resizable one sized by `GameWindow.frame` from the card's shape (`setGameWindow` in Overlay.swift); esc, ⌃F, ⌘W or the red button go back to the bar's card. One view tree for both, so the game survives the switch (a branch per mode had rebuilt it from the lobby). Pong lobby and a match in and out of full screen, connect 4's lobby, light and dark, pictured in a re-signed dev copy |
| Lobby: start / vs computer / vs a friend screens, host open or locked, join by list or code, rename, ⌫ back, codes, notes | ✅ | the core's `lobby_*` (lobby.ts and its tests); room list, open host and join seen against the web page |
| Relay link: rooms socket, host/join, pings, seat kept through a dropped link, step away / come back, retries on wake | ✅ | the core's `RelayPeer` (peerOnline.ts and its tests) run over URLSessionWebSocketTask; native ↔ web connect 4 played both ways |
| Saves (`qcalc-equal`, `qcalc-numbers`, `qcalc-crack`, `qcalc-c4-ghost`, `qcalc-host-locked`) and the nickname | ✅ | the same AppSettings store and strings the web page mirrored, so saves carry over |
| Pong: court, paddles, serve, computer at three levels, online legs and clock, rematch | ✅ | the solo frame-for-frame digest of the TS test passes, so solo plays exactly as before; online simulation tests pass |
| Connect 4: board, held disc slide, fall, preview (G), win line, computer, online, rematch | ✅ | |
| Ultimate: chalk slate, marks, draw-in, open boards, trail, cursor, computer, online | ✅ | chalk paths equal the TS's to the character |
| Crack: dial, needle, dot, pop, shake, click, best level | ✅ | |
| Equal: daily (same answer as the web for each date) and practice, tiles, flip, shake, keypad marks, stats, share | ✅ | daily answers and practice deals match the TS |
| 24 / countdown: deals, parser, live line, faint parens, clock bar, tiles, stats, large numbers in the lobby | ✅ | solutions and deals match the TS for the same seed |
| Sound | ✅ | none of these six has any (only ember does) |
| VoiceOver labels | 🟡 | the page's `aria-label`s and live regions, which the port had dropped: the toasts (`role="status"`), connect 4's and ultimate's status line and equal's note (`aria-live="polite"`) are read out as they change; the room list is "rooms", each room reads host, game and "locked" / "open" (selected when picked), the code tiles read as "room code" and their digits, the rename box as "nickname" and its draft, the paused card as a "paused" modal; in the games, crack's dial reads `level 3, 5 to go`, connect 4's and ultimate's boards are "board", equal's grid "guesses" and its ⌫ ↵ keys "delete" / "enter", 24 / countdown's field "your expression", pong's court and trails' arena are named, and ember's hud reads its stage, checkpoint, keys carried and time as the page's does. Built and run; SwiftUI's tree isn't readable from inside the app without an assistive client, so not listened to |
| Reduce Motion | 🟡 | the page's `prefers-reduced-motion` rules, which the port had missed: no `.pong-wait` pulse, no `.lobby-caret` blink (it stays shown), no equal shake or flip, no crack shake, click or pop (`GameMotion.reduced`). Built; not seen with the setting on (it's a system setting this loop doesn't change) |

## Where the logic runs

Every rule above is the TS's own, ported to the Rust core and answered through `Engine` (typed calls) or
`Engine.callJson` (`AppLogic.call`, module and name as in `src/lib`). Nothing runs as script: JavaScriptCore and the
`applogic.js` bundle are gone. `blankReason` and `missingUnit` take their `ok` callback inside the engine
(`Engine.blankSpan`, `Engine.missingUnit`, each piece read by `live` with the line's context); `lib/tour`,
`lib/analytics` and the constants the bar reads are in the core's dispatch. The host keeps only what is the
platform's: the install id's random bytes and the clock's zone offset for `localDay`.

The only web code in the app is the games bundle (`games.html`, built alone by `games.vite.config.ts`), loaded in a
web view only while ember & frost, trails or an effect is up; the other six games are native (section 10). The bundle
goes once those are ported too.

## macOS 12 (Monterey)

The native app's floor is macOS 12.0, universal (arm64 + x86_64): `Package.swift` (both packages), `Info.plist`
`LSMinimumSystemVersion`, `build.sh` `MIN_MACOS` (the SwiftPM triple) and `scripts/build-mac.sh`
`MACOSX_DEPLOYMENT_TARGET` (cargo, so every object in `libqcalc_ffi.a` says `minos 12.0` or lower). Most of the app
is one code path that Monterey has. Where macOS 13 added the API the app uses, there is a fork: on 13 and later it is
the same API as before, so those Macs look and behave exactly as the rows above say; on 12 it is the closest
Monterey stand-in. Every fork:

| Where | 13+ (unchanged) | macOS 12 | What differs on 12 |
|---|---|---|---|
| Typeset preview row (`MathText.swift` `FitLayout`) | `Layout` | `FitFrame`: the same scale from the row's width, read through a `GeometryReader` preference | the width is measured a pass later, so after the bar is resized the preview settles a frame late |
| Shortcuts list `?` (`BarView.swift` `column`) | `Grid` | `KeyColumn`: rows of key and label, the key column as wide as its widest key (≥ 64 / 96 pt), measured by preference | same layout, a frame later |
| Game lobby buttons (`GameLobby.swift` `WrapRow`) | `FlowRow` (`Layout`) | `FlowRows`: buttons measured by preference, then centred rows by `FlowRow`'s rule | same rows, a frame later; buttons are rebuilt when they move between rows |
| Graph readout, Connect 4 and Ultimate hover (`Compat12.swift` `pointerHover`) | `onContinuousHover` | a tracking area in an AppKit view over the board/plot that never takes clicks | the graph readout doesn't follow the pointer while a drag (pan) is in progress |
| Connect 4 and Ultimate clicks (`spatialTap`) | `SpatialTapGesture` | `DragGesture(minimumDistance: 0)` that counts as a tap when it ends within 4 pt | a press that wanders up to 4 pt still counts |
| Edge tools' 0.02 em tracking, the system fields' display tracking (`viewTracking`) | `View.tracking` | none (12 has `tracking` on `Text` only) | the rad/deg/sf/clear labels and the equation fields use the font's own spacing (≈0.2 pt a glyph) |
| Settings rows (`macos/Compat.swift` `QLabeled`, shared with the web-view app) | `LabeledContent` | title, grey callout subtitle lines and the control trailing, in an `HStack` | close to the grouped form's row; spacing is SwiftUI's, not the form's |
| Settings forms (`groupedForm`) | `.formStyle(.grouped)` | Monterey's plain `Form` with 20 pt margins | no grouped cards behind the sections; section titles are plain headers |
| Settings tab size (`macos/SettingsWindow.swift`) | `sizingOptions = [.preferredContentSize]` | `preferredContentSize` set to the tab's fixed size | none (each tab's content is a fixed frame) |
| Open at login (`macos/LoginItem.swift`) | `SMAppService.mainApp` | a launch agent, `~/Library/LaunchAgents/<bundle id>.login.plist`, running `open -a <the app>` at login | it isn't listed under Login Items (Monterey shows no launch agents there); moving the app after turning it on needs it turned off and on again |

Forks that were already there and only take the older path on 12 too: the bar's `NSHostingView.sizingOptions = []`
(13; on 12 the hosting view keeps its default sizing constraints inside the autoresizing card), ↑ ↓ between system
equations (`onKeyPress`, 14; Tab moves between them on 12 and 13), the input's inline prediction (14) and Writing Tools
(15) switches.

Unverified: no macOS 12 machine or VM was available, so every 12 column above is built against the SDK with the
compiler's availability checks (no errors at `arm64/x86_64-apple-macosx12.0`) but not run. On this Mac (macOS 26)
the 13+ paths were spot-checked after the change: the live answer and typeset row, the graph, `?`, the periodic table,
settings (General) and Connect 4's lobby and board.
