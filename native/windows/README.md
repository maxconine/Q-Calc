# qcalc-win: Q Calc for Windows, native

Pure Rust: Win32 + Direct2D + DirectWrite (the `windows` crate) around the engine (`qcalc-ffi`'s `Engine`, called
directly as a Rust crate, feature `typst` for the typeset preview). No WebView2, no Tauri, no web code. The
feature spec is the Mac native app (`native/mac/Sources`); the Windows knowledge (placement, store shapes,
hotkey chords, the log) comes from `src-tauri/src`.

```
cargo build --release -p qcalc-win        # native/target/release/qcalc.exe
cargo test -p qcalc-win
cargo clippy -p qcalc-win
```

On a non-Windows host the crate builds a stub `main` and the pure modules' tests, so the Mac's workspace build
is unaffected.

## Targets (design for them in everything you add)

| what | target | how it's measured |
|---|---|---|
| keystroke → top answer painted | < 0.8 ms median | the bench (`--bench`, pipe), in-process, WM_CHAR post → EndDraw |
| idle memory, bar hidden after use | < 12.5 MB private working set | Task Manager "Memory"; `scripts/bench.ps1 -Mem` |
| warm launch → ready | < 75 ms | the `ready ... since process start` log line |

Measured on this machine (4K at 150%, release build): keystroke median 1.03 ms, p90 1.54, p99 2.24 (460
keystrokes over 10 inputs); warm launch to the first frame 41-49 ms (median 43); hidden after use 14.7 MB private
working set, 0.4 MB after the trim (private bytes 17.7 MB); 0 ms CPU over 5 s idle, shown or hidden.

With the sheets, messaging and the tour merged (same machine, shared with other builds, A/B against the build
before): keystroke median 0.85 ms for a returning user (0.80 before); during the walkthrough's first opens 1.2 ms
(0.82 before), all of it the first keystroke of a line, when the tour's line under the bar goes and the window
shrinks; hidden after use 17.4-18.5 MB private working set (16.8 before), 0.4-2 MB after the trim.

After the performance pass (PERFORMANCE.md, `scripts/perf.ps1`): keystroke median 0.51 ms (p90 1.18, p99 1.55;
golden inputs 0.53 / 1.20 / 1.70), never shown 4.2 MB private working set / 6.3 MB private bytes, hidden after use
13.7 / 15.5 MB (the engine's compiled regexes are most of it), warm launch 46 ms. After the second pass:
keystroke median 0.47 ms (p90 0.62, p99 1.28; golden 0.47 / 0.56 / 1.73), hidden after use 6.4 / 8.1 MB (7.5 /
9.2 MB after the golden inputs too), warm launch 34 ms, a showing after a hide 9 ms.

Rules that keep them: never evaluate on the UI thread; never block the UI thread on the engine; repaint only
when the model changed; no timer while idle (one-shot timers only, an animation's timer stops with it); make
COM objects and text layouts once and cache them (render.rs `Fonts`, input.rs `FieldView`'s layout cache).

## Module map (who owns what)

Each later feature lives in its own file; touch app.rs, model.rs and window.rs only at the hooks named here.

| file | owns |
|---|---|
| `main.rs` | module list, `windows_subsystem`, the non-Windows stub |
| `app.rs` | entry (`run`), single instance (named mutex `Local\com.maxconine.qcalc.native`; a second launch sends its args by WM_COPYDATA and exits), the host window, message routing, `App` (all state), `Op`s, show / hide / reframe, the hidden-trim, the bench driver |
| `window.rs` | the bar window (`Bar`): creation, DWM corners / shadow / cloak, `BarLayout` (`Bar::layout`), `Bar::paint`, placement (`placement`, `reframe`), monitors |
| `place.rs` | placement maths, ported unchanged from src-tauri (tested) |
| `render.rs` | D2D / DWrite factories (`Gfx`), `Fonts`, `Theme` (light / dark colours), `Surface` (the HWND render target, DPI), `Canvas` (fill, rounded rect, line, text, clip), text hit-testing helpers |
| `engine.rs` | the engine thread (32 MB stack) and the Typst thread, lanes, generations, `Done` closures, the history-context cache, the warm-up after a showing (in steps behind keystrokes) and freeing the compiled regexes at a hide (`release`) |
| `heap.rs` | the global allocator: the process heap, but slabs of our own for the regexes the engine, UI and Typst threads free at a hide (native/lazy, PERFORMANCE.md) |
| `warp.rs` | the bar's WARP device made ahead on a thread of its own at launch and as a hidden bar shows; WARP's DLL kept loaded |
| `model.rs` | `BarModel` (BarModel.swift port): the line, live answer, chaining, commit, tape selection, in-bar actions |
| `input.rs` | `Field` (text, caret, selection, word moves, undo, paste, WM_CHAR surrogates), `FieldView` (drawing, scroll, hit test), `ime` |
| `answer.rs` | the live answer view: fit font, Tab's forms, closed forms, the steady answer, the copied flash |
| `history.rs` | the tape view, its keys and clicks, and its persistence (engine `load_history` / `save_history`) |
| `cheats.rs` | `?` / `help`: the cheat sheet where the tape is |
| `line.rs` | the input line's side: decorations, squiggle, completion, the draft, the empty bar's rotating examples (and counting opens), the d/dt chips, the caret's blink |
| `text_rules.rs` | typing rules: words to symbols, bracket pairing and wrapping, flattened pastes |
| `drag.rs` | moving and widening the bar, remembered per monitor |
| `keys.rs` | the in-bar keybind router (`settings.keybinds` over the Windows defaults); `close_front_sheet` for Ctrl+W |
| `hotkey.rs` | the global shortcut: chord parsing, RegisterHotKey with retries, re-registration triggers, the "held by X" guess |
| `autostart.rs` | the Run entry: only an installed build writes it, always pointing at itself with `--autostart` |
| `store.rs` | settings.json (`Store`, the Tauri shape), `config_dir`, `Draft` |
| `tray.rs` | the notification-area icon, its menu, balloons |
| `clip.rs` | clipboard text |
| `bench.rs` | the dev bench: spec, steps, stats, the named pipe, memory counters |
| `log.rs` | the log file, ms since process creation, the panic hook |
| `settings_window.rs` | the settings window (drawn with render.rs; made on first open, dropped on close); `set_messages_hook` for the Messages page |
| `graph.rs` | the graph panel under the composer (`graph x^2`, `plot …`) |
| `systems.rs` | `sys2`…`sys5`: equation rows under the composer, solved live, saved as one row |
| `typst_row.rs` | the typeset maths row under the composer |
| `commands.rs` | the word commands that aren't maths (`periodic`, `idt …`, `constants`, `games` and a game's name, `message`, `tutorial`, `skip`, `hello`): their words in the answer slot, ↵ runs them, the games list above the bar |
| `sheets.rs` | the periodic table and identity sheets, each a no-activate window beside the bar; `close_front` for Ctrl+W |
| `messaging.rs` | messages through the relay: the `message` line, the cards under the bar, the name; registers settings' Messages page (`set_messages_hook`) |
| `threads.rs` | message threads under the cards (`messages`, `messages <name>`): the history (`messages.json`), the list, a thread's lines with Copy / Save / Retry, its reply line and 📎, Trust New Key and the safety code; the one-time privacy notice |
| `share_drop.rs` | the bar as an OLE drop target once messaging is in use: the hint while files are dragged over the bar or a thread |
| `clip_image.rs` | a copied image as `clipboard.png` (the clipboard's PNG, else its bitmap encoded here) |
| `share_debug.rs` | the bench pipe's `share {…}` commands for `tests/e2e/share-e2e.ps1` (two copies through a local relay) |
| `relay.rs` | the relay over WinHTTP: messaging's POSTs and inbox socket, and the games' sockets (`Socket::open_url`) |
| `tour.rs` | onboarding and the walkthrough: the tour's line, its step example in the empty bar (in place of line.rs's), hints; `will_show` runs after line.rs counted the open |
| `hover.rs` | the pointer over the tape and the answer: the row and cell lit, the hand cursor, the system tooltip after a 450 ms rest; triple-click selects the line |
| `contrast.rs` | contrast themes: the theme from the system colours while one is on (`QCALC_CONTRAST=1` to look without one) |
| `a11y.rs` | with a screen reader on, the bar's name is the line and its answer, with a name-change event (`QCALC_READER=1` for the test) |
| `edge.rs` | the edge tools over the composer's corners (settings, deg · a/b · sf, clear) and their tips |
| `later.rs` | keyed one-shot timers that run a closure on the UI thread |
| `effects.rs`, `effects/` | the easter eggs, each in its own short-lived layered window |
| `games/` | the native games, each in a window and a process of its own (`qcalc.exe --game <kind>`, games/README.md): `open(kind)`, `is_open`, `command`/`hint` for the bar; online play over relay.rs |
| `update.rs` | self-update: `latest-native.json` on the GitHub release, minisign-verified with the Tauri updater's key, installed while away |
| `rates.rs` | the ECB daily rates (the core's rules) on a background thread, into `rates.json` and the engine |
| `analytics.rs` | the usage counts (the core's rules), `track(app, event)` |
| `bg.rs` | background threads' way back to the UI thread (`post`), and `start` for the background jobs after the first frame |
| `net.rs` | HTTPS GETs over WinHTTP for the background jobs |
| `migrate.rs` | the Tauri app's history out of its WebView2 LevelDB, once |
| `installer/` | the NSIS installer (`qcalc.nsi`) and `build.ps1` (installer, signature, feed) |

Each module's doc comment names its Mac / Tauri spec.

## Threading model

- **UI thread** (main): the Win32 message loop; owns `App` in a thread-local (`app::with_app`). Two windows:
  the hidden *host* (hotkey, tray, power / session / TaskbarCreated broadcasts, WM_COPYDATA, engine results,
  bench) and the *bar*.
- **Engine thread** `qcalc-engine` (32 MB stack): owns `Engine`, whose sync calls run right on it
  (`qcalc_ffi::run_engine_calls_here`: no hop to the FFI's evaluation thread). Its warm-up (`warm_up`) runs at
  launch when the bar shows, else with the first showing. UI code submits closures: `app.engine.submit(Lane::X, move |cx| { ...; done(move |app| ...) })`.
  A lane keeps only its newest waiting job; every job has the lane's generation, and its `Done` runs on the UI
  thread (posted as `WM_ENGINE`) only if it is still the lane's newest. Use `Lane::Misc` for work that must
  never be dropped (load / save). `cx.context(&rows, view)` is the cached `context_from_history`.
- **Typst thread** `qcalc-typst`: owns the `TypstRenderer`; started by the first typeset,
  ended (with its caches) when the bar hides; `app.engine.typst(|r| ...)`. Typst's rayon pool has one thread.
- **Bench pipe thread**: only with `--bench-pipe` / `QCALC_BENCH=1`.

Never hold anything across threads but owned data (`Rows` is an `Arc`); `HWND` crosses as `usize`.

## View / paint model

- State lives in `BarModel`. **Every mutation sets `changed`** (`model.touch()`).
- After each message, `App::after` checks `take_changed()`: it re-lays out the bar (`Bar::layout`, pure DIP
  maths from the model), queues `Op::Reframe` if the height or the composer's offset moved, and marks a paint.
- The render target is **software** by default (render.rs `hardware()`): for a window this size it draws as fast,
  is made in ms instead of 150-300 ms (no GPU driver load on the first showing) and keeps ~50 MB of device memory
  out of the process. The bar draws through a flip-model swap chain on a shared WARP device (`Surface::new_flip`,
  one rasterising thread); settings and sheets use window targets. `QCALC_D2D=hwnd` / `hardware` switch.
- `run_ops` (after the borrow ends) runs the ops, then `InvalidateRect` + `UpdateWindow`: the frame goes out
  synchronously, `D2D1_PRESENT_OPTIONS_IMMEDIATELY`, no vsync wait.
- `Bar::paint` draws the whole bar each frame (it's small): background, tape, composer, hint, panels. All
  coordinates are DIPs (the render target's DPI is the monitor's).
- Win32 calls that send messages back (SetWindowPos, ShowWindow, SetForegroundWindow, TrackPopupMenu,
  UpdateWindow, DestroyWindow) are **never** made inside `with_app`: push an `Op` (add a variant if needed).
- Showing: `prepare_show` (monitor under the pointer, the remembered spot, layout), then cloak, SetWindowPos,
  SetForegroundWindow, paint, uncloak. Hiding: SW_HIDE, `after_hide`, which lets go of what only a showing needs
  (layouts, the Typst thread, the swap chain and the WARP device); 30 s later `trim` empties the working set.

## Adding a panel (graph, sys, typst row, message cards)

1. State in its own struct in your module (`pub struct GraphState`), held as an `Option` field on `BarModel`
   (add the field and set it from `evaluate_live`'s command branch; call `touch()`).
2. Height: in `Bar::layout` (window.rs) add `if let Some(p) = &model.graph { rect = Rect::new(0, height, width,
   graph::height(p)); height += ... }` and store the rect in a new `BarLayout` field.
3. Paint: in `Bar::paint`, after the hint, call `graph::paint(c, state, layout.graph)`.
4. Engine work on its own lane (add a `Lane` variant), results via `done(move |app| app.model.graph = ...)`.
5. Mouse: route in `App::click` (app.rs) by testing your `BarLayout` rect.
6. Keys: in `BarModel::key_down` before the field's keys, only while your panel has the focus.

The composer stays still while panels grow below it and the tape grows above it (`Bar::reframe`).

## Running and driving it

- `qcalc.exe` shows the bar; `--autostart` starts in the tray (sign-in); Alt+Space (or the stored chord) toggles.
- Settings, history and rates: `%APPDATA%\com.maxconine.qcalc\` (`settings.json` shared with the Tauri app,
  `history.json` new, `rates.json`). `QCALC_DATA=<dir>` uses another folder (tests, the bench).
- Log: `%LOCALAPPDATA%\com.maxconine.qcalc\qcalc.log` (`QCALC_LOG=<file>` for another). Lines from this app say
  `[native +<ms since process start>]`.
- Bench: `qcalc.exe --bench-pipe`, then `scripts/bench.ps1` (keystroke timing over `bench/inputs`-style specs,
  `-Mem` for memory after a hide and trim, `-Launch` for warm launch). Or `qcalc.exe --bench spec.json` with
  `"out"` in the spec. Pipe protocol and spec: `bench.rs`.

## The hotkey after a restart (why hotkey.rs and autostart.rs look like this)

The Tauri app's shortcut often didn't work after Windows restarted. Two causes: its Run entry was rewritten by
whatever exe ran, so a dev build re-pointed sign-in at `target\release\qcalc.exe`; and RegisterHotKey was tried
once at launch, losing to a launcher that held Alt+Space for a moment at sign-in (or the old process after an
update). Here only an installed exe writes the Run entry (and repairs a bad one), and registration retries
(1 s × 30, then 10 s × 18) and restarts on resume, unlock and Explorer restart, logging every attempt; only when
retries run out does the tray say which launcher likely holds the chord.

## Known gaps (next agents)

The games ember & frost and trails (win-games2; the games list says they aren't in yet); Mica / acrylic backdrop;
a full UI Automation tree (a11y.rs only names the bar: the tape's rows, the caret and selection aren't exposed);
a fade on showing (the Mac's 0.1 s: a flip swap chain can't be a layered window, so it needs DirectComposition).
