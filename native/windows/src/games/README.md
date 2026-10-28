# games/: the native games

Pong, connect 4, ultimate tic-tac-toe, crack, equal, 24 / countdown, ember & frost and trails, each in a game
window of its own, in a process of its own (`qcalc.exe --game <kind>`, process.rs), drawn with Direct2D over the
core's rules, computers and online sessions (`qcalc_core::games`, called directly; the spec is
native/mac/Sources/Games). Nothing here borrows `App`, and nothing of the bar starts in a game's process (no
engine, Typst, hotkey, tray or bar window): the window has its own thread-local, factory, render target, fonts,
saves and relay link, and closing it ends the process, so the bar's process never holds a game's GPU or heap.

| file | what | Mac spec |
|---|---|---|
| `mod.rs` | `GAMES` (the registry), `open(kind)`, `command` / `hint` / `list` for the bar, `Play` and `GameDef`, `Env` (what a game reaches), the window: creation, wndproc, keys, mouse, esc, full screen, timers, frames | GameWindow.swift, GameShell.swift, GameWindowController.swift |
| `shell.rs` | which keys the shell takes (Ctrl+W, Ctrl+F, esc, the paused card's), over the core's `shell` / `fullscreen` | GameShellModel |
| `lobby.rs` | the lobby's state, effects and screens (over the core's `lobby`), the room list, the code tiles | GameLobby.swift |
| `relay.rs` | the relay link: the core's `OnlinePeer` over relay.rs's WinHTTP WebSocket (SChannel), two threads per socket, `WM_SOCK` back | PeerTransport.swift |
| `store.rs` | `games.json` beside settings.json: `{ "gameSaves": {key: string}, "nickname": "…" }` | GameStore.swift |
| `keys.rs` | `GameKey` (DOM `key` / `code`) from WM_KEYDOWN / WM_CHAR / WM_KEYUP | GameKeys.swift |
| `gfx.rs` | `Gfx` (the window's D2D / DWrite state) and `Ui` (shapes, polylines, text, hit regions) | Canvas, SwiftUI |
| `widgets.rs` | `.pong-button`, head, esc, notes and links, code tiles, score, toast, the full-screen glyph | GameControls.swift |
| `theme.rs` | the games' colours (`color-mix` as CSS does it), spacing, CSS easings | GameTheme.swift |
| `svg.rs` | svg `d` → polylines, trimmed by length (the chalk's draw-in) | SVGPath.swift |
| `process.rs` | the bar's `open` (bring the kind's window forward, or start `--game <kind>`), the game process's `child_main` (its own mutex per kind, the window, the loop) | GameWindowController.swift |
| `canvas.rs` | the web's Canvas 2D context over Direct2D (paths in device space, gradients, dashes, clips, lighter and destination-out, text, offscreen canvases), for ember and trails | Canvas2D.swift |
| `pong.rs` `connect4.rs` `ultimate.rs` `crack.rs` `equal.rs` `numbers.rs` `trails.rs` | the games | the panels |
| `ember/` | ember & frost: the panel (`mod.rs`), the renderer (`draw.rs` `things.rs` `players.rs` `fx.rs`), the synth over WASAPI (`sound.rs`, only while ember plays) | Ember/ |

## The window

- `open_at(kind, spot)` (the bar's `pong` + ↵, commands.rs, which then puts the bar away) brings that kind's window
  forward if it's up (its class is `QCalcGame.<kind>` plus the QCALC_INSTANCE suffix; `is_open` looks for any), else
  starts `qcalc.exe --game <kind> --at <x>,<width>,<top>` (QCALC_INSTANCE, QCALC_DATA and QCALC_LOG go with it). The
  game's process takes its own mutex per kind (a second launch brings the first forward and leaves) and opens **in
  the bar's place** (GameWindow.swift's bar card): a borderless, rounded window with the bar's shadow, at the bar's
  left and width, hanging from the composer's top and as tall as what it draws (the window follows the card's
  height; moved up only when it wouldn't fit, `process::card_frame`). With no bar up it opens where a bar would.
  Its log says how long from the bar's ↵ to the first frame (about 60 ms warm).
- Esc from the lobby's first screen closes the game back to the bar (`process::bar_back`, a second launch's
  hand-off with no arguments, so the bar shows where it was). The bar's hotkey while a game is up closes the game in
  front, or brings one behind forward (`games::hotkey`, from app.rs `toggle`).
- Saves: games.json is read when a game opens and written under a named mutex, read fresh first, so two games (or
  a game and the bar) saving at once both keep theirs.
- Full screen (Ctrl+F, f on the lobby and the paused card, the buttons): a decorated, resizable window at 80% of the
  monitor (place.rs `game_window`), the game area centred and boards growing with it (`Env::stage`); esc, Ctrl+F or
  Ctrl+W back to the bar's card. Only full screen makes it bigger. Esc layers: full screen → a lobby step (closing
  from the first screen) → pause (a local game) or "esc again to leave" (online) → leave. Ctrl+W (from the card) and
  Alt+F4 close. A local game pauses when the window loses focus.
- Drawing is immediate mode: each frame lays out and paints, and clicks are tested against the regions it drew
  (`Hit`). A press on a board is the game's (the window has a real caption, so nothing drags).
- Frames: the first ones go to a software HWND target (~15 ms to make) while `gfx::warm_gpu` makes the D3D11
  device on its own thread (~160 ms, the driver's load); then the window moves to a Direct2D device context over a
  flip-model swap chain on that device (5 ms on the UI thread). QCALC_D2D=software stays software. Presented
  immediately and paced by `DwmFlush`, only while the last frame drew something that moves (`ui.animate()`) and
  the window is shown and not paused. A game's 200 ms tick runs only while it asks (`Play::ticks`); the relay's timer only to its next
  deadline. A still game window costs no CPU.

## Adding a game (as ember & frost and trails were)

1. A file here with `pub static DEF: GameDef` and a type that implements `Play`; add `mod` and the entry in
   `GAMES` (and the array's length). `matches` is the bar's regex for it (BarCommand.gameList), on a line already
   trimmed, collapsed and lower-cased; `hint` its line under the bar.
2. `local(option, env)` makes the game on this computer (options, then `together`); `online` (if any) makes it
   over the link, `Some(|role, peer, env| …)`. The lobby calls them as its state enters `Local` / `Game`.
3. `Play::paint(ui, x, y, w, env)` draws under the lobby's head and returns its height; call `ui.animate()` every
   frame something moves (ember and trails: every frame while running). For a picture of its own shape, lay it out
   with `qcalc_core::games::fullscreen::fullscreen_layout` from `env.stage` when full screen is on.
4. Keys: `Play::key` gets `GameKey`s (down and up, `repeat`), after the shell; true keeps a key. Time:
   `env.now()` (`gameNow`, stands still while paused) and `env.date()`. Randomness: `env.rand()`.
5. Online: `env.sent.push(text)` sends; `Play::heard` gets `PeerEvent::Message { data }` and `Closed { reason }`
   (`close_reason_text(reason, "computer")` words it). End with `env.end = Some(note)`.
6. Saves: `env.store.get(key)` / `set(key, Some(value))` with the page's keys (`qcalc-ember` is already allowed).
7. A canvas-drawn game (the page's `<canvas>`): `Canvas::on(ui, &res, rect)` inside `ui.clip(rect)`, then
   `finish()` and `ui.sync_transform()`; offscreen canvases (`Canvas::offscreen`) belong to the target's
   `generation` and are made again when it changes (the switch to the GPU, a lost device).
8. Sound: ember's `sound::Synth` opens WASAPI on its own thread with the first sound and closes with the game.
