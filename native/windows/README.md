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
| `engine.rs` | the engine thread (32 MB stack) and the Typst thread, lanes, generations, `Done` closures, the history-context cache |
| `model.rs` | `BarModel` (BarModel.swift port): the line, live answer, chaining, commit, tape selection, in-bar actions |
| `input.rs` | `Field` (text, caret, selection, word moves, undo, paste, WM_CHAR surrogates), `FieldView` (drawing, scroll, hit test), `ime` |
| `answer.rs` | the live answer view (and later: fit font, forms, closed forms, steady answer) |
| `history.rs` | the tape view and its persistence (engine `load_history` / `save_history`) |
| `keys.rs` | the in-bar keybind router (`settings.keybinds` over the Windows defaults) |
| `hotkey.rs` | the global shortcut: chord parsing, RegisterHotKey with retries, re-registration triggers, the "held by X" guess |
| `autostart.rs` | the Run entry: only an installed build writes it, always pointing at itself with `--autostart` |
| `store.rs` | settings.json (`Store`, the Tauri shape), `config_dir`, `Draft` |
| `tray.rs` | the notification-area icon, its menu, balloons |
| `clip.rs` | clipboard text |
| `bench.rs` | the dev bench: spec, steps, stats, the named pipe, memory counters |
| `log.rs` | the log file, ms since process creation, the panic hook |
| `settings_window.rs` | stub: the settings window |
| `graph.rs`, `systems.rs`, `typst_row.rs`, `messaging.rs` | stubs: panels under the composer |
| `sheets.rs` | stub: periodic table and identity sheets (their own windows) |
| `effects.rs` | stub: the easter-egg effects |
| `games/` | the native games, each in a window and a process of its own (`qcalc.exe --game <kind>`, games/README.md): `open(kind)`, `is_open`, `command`/`hint` for the bar |
| `update.rs`, `rates.rs` | stubs: self-update, the ECB rates fetch |

Each stub's doc comment names its Mac / Tauri spec and the hooks it should use.

## Threading model

- **UI thread** (main): the Win32 message loop; owns `App` in a thread-local (`app::with_app`). Two windows:
  the hidden *host* (hotkey, tray, power / session / TaskbarCreated broadcasts, WM_COPYDATA, engine results,
  bench) and the *bar*.
- **Engine thread** `qcalc-engine` (32 MB stack): owns `Engine` (which hops each call to its own 32 MB
  evaluation thread). UI code submits closures: `app.engine.submit(Lane::X, move |cx| { ...; done(move |app| ...) })`.
  A lane keeps only its newest waiting job; every job has the lane's generation, and its `Done` runs on the UI
  thread (posted as `WM_ENGINE`) only if it is still the lane's newest. Use `Lane::Misc` for work that must
  never be dropped (load / save). `cx.context(&rows, view)` is the cached `context_from_history`.
- **Typst thread** `qcalc-typst`: owns the `TypstRenderer`, warmed at launch; `app.engine.typst(|r| ...)`.
- **Bench pipe thread**: only with `--bench-pipe` / `QCALC_BENCH=1`.

Never hold anything across threads but owned data (`Rows` is an `Arc`); `HWND` crosses as `usize`.

## View / paint model

- State lives in `BarModel`. **Every mutation sets `changed`** (`model.touch()`).
- After each message, `App::after` checks `take_changed()`: it re-lays out the bar (`Bar::layout`, pure DIP
  maths from the model), queues `Op::Reframe` if the height or the composer's offset moved, and marks a paint.
- The render target is **software** Direct2D by default (render.rs `hardware()`): for a window this size it draws
  as fast, is made in ~15 ms instead of 150-300 ms (no D3D device or GPU driver load on the first showing) and
  keeps ~50 MB of device memory out of the process. `QCALC_D2D=hardware` switches for experiments.
- `run_ops` (after the borrow ends) runs the ops, then `InvalidateRect` + `UpdateWindow`: the frame goes out
  synchronously, `D2D1_PRESENT_OPTIONS_IMMEDIATELY`, no vsync wait.
- `Bar::paint` draws the whole bar each frame (it's small): background, tape, composer, hint, panels. All
  coordinates are DIPs (the render target's DPI is the monitor's).
- Win32 calls that send messages back (SetWindowPos, ShowWindow, SetForegroundWindow, TrackPopupMenu,
  UpdateWindow, DestroyWindow) are **never** made inside `with_app`: push an `Op` (add a variant if needed).
- Showing: `prepare_show` (monitor under the pointer, the remembered spot, layout), then cloak, SetWindowPos,
  SetForegroundWindow, paint, uncloak. Hiding: SW_HIDE, `after_hide`; 30 s later `trim` drops the render target
  and empties the working set.

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

Everything in the stubs; commands (`help`, `games`, `periodic`, `graph`, `sys`, `message`...); answer forms (Tab),
closed forms, the steady answer, decorations and squiggles, completion ghost text; dragging / widening the bar
(and remembering the spot: `store.positions` is read but not yet written); the draft across restarts (store.rs
`Draft` is ready); migrating history out of the Tauri app's WebView2 localStorage; a blinking caret; Mica /
acrylic backdrop; accessibility (UI Automation); the settings window.
