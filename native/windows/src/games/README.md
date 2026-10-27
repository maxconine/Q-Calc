# games/: the native games

Pong, connect 4, ultimate tic-tac-toe, crack, equal and 24 / countdown, each in a game window of its own, drawn
with Direct2D over the core's rules, computers and online sessions (`qcalc_core::games`, called directly; the
spec is native/mac/Sources/Games). Nothing here borrows `App`: the window has its own thread-local, factory,
render target, fonts, saves and relay link, all made when a game opens and dropped when it closes.

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
| `pong.rs` `connect4.rs` `ultimate.rs` `crack.rs` `equal.rs` `numbers.rs` | the six games | the panels |

## The window

- `open(kind)` defers to the next turn of the message loop (a thread timer), closes a game that's open, and opens
  the window centred near the top of the monitor under the pointer, the card's 680 DIPs wide and as tall as what it
  draws (the window follows the card's height). The bar's `pong` + ↵ (model.rs) opens it and puts the bar away.
- Full screen (Ctrl+F, f on the lobby and the paused card, the buttons): a decorated, resizable window at 80% of the
  monitor (place.rs `game_window`), the game area centred and boards growing with it (`Env::stage`); esc or Ctrl+F
  back. Esc layers: full screen → a lobby step (closing from the first screen) → pause (a local game) or "esc again
  to leave" (online) → leave. Ctrl+W and Alt+F4 close. A local game pauses when the window loses focus.
- Drawing is immediate mode: each frame lays out and paints, and clicks are tested against the regions it drew
  (`Hit`). A press on a board is the game's (the window has a real caption, so nothing drags).
- Frames: a hardware render target (QCALC_D2D=software forces the software one), presented immediately and paced
  by `DwmFlush`, only while the last frame drew something that moves (`ui.animate()`) and the window is shown and
  not paused. A game's 200 ms tick runs only while it asks (`Play::ticks`); the relay's timer only to its next
  deadline. A still game window costs no CPU.

## Adding a game (ember & frost, trails)

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
7. Sound (ember): nothing here plays sound yet; make the device only while the game is open, as the target is.
