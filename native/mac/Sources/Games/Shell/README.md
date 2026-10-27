# The native game shell

What every game sits in, ported from `src/components/{GameShell,GameLobby}.tsx` and `src/lib/{gameShell,gameClock,
gameKeys,gameSaves,gameFullscreen,lobby,nickname,peer,peerOnline}.ts`. The rules are in Rust
(`native/core/src/games/`, exposed by `native/ffi/src/games.rs`); this folder is the Swift around them.

## Adding a game

1. Add one line to `Games/NativeGames.swift`: `case "ember": return AnyView(EmberPanel())`. A kind listed there
   opens natively; any other still opens the web bundle.
2. The panel is a SwiftUI view. It reads `@EnvironmentObject var shell: GameShellModel` and
   `@Environment(\.gameColors) var colors`, and almost always starts with `GameLobby(LobbyConfig(...))`, the
   `<GameLobby>` props one for one (`game`, `name`, `blurb`, `transport`, `online`, `onOpen`, `local`, `sub`,
   `footer`, `onKey`, `idle`, `leave`, `frame`, `ownHead`). Pass `RelayTransport.shared` as the transport for a
   linked game.
3. Keys: `.gameKeys(shell) { key in … return true }` on the game's view, for as long as it's up. `GameKey` has the
   page's `key`/`code` names (`ArrowUp`, `Enter`, `KeyW`), `down` (key-up events come too), `isRepeat`, `plain`.
   The shell takes ⌃F, esc and keys while paused first; the lobby next; then the game. ⌘ shortcuts never arrive.
4. Time: `GameTime.now()` is `gameNow()` and `GameTime.date()` `gameDateNow()`; they stand still while paused.
   Draw frames with `TimelineView(.animation)` and read `GameTime.now()` inside.
5. Saves: `GameSaves.load(key)` / `GameSaves.store(key, value)` with the page's local-storage keys
   (`qcalc-ember`…; the list is `game_save_keys()` in the core). The values are the strings the page wrote.
6. Online: `GameProps` (`transport`, `role`, `peer`, `onEnd`) as the page had; `transport.subscribe { event in }`
   gives `GamePeerEvent`s (`.message(data:)`, `.closed(reason:)`…) on the main thread, `transport.send(text)`.
   `peerCloseText(reason:word:)` words a close.

## The pieces

| File | What | From |
|---|---|---|
| `GameShell.swift` | `GameShellModel` (mode, pause, esc, full screen, hooks), `GameShellView` (the card, the paused card, the toast, the corner button), `FullscreenButton` | `GameShell.tsx`, `gameShell.ts` |
| `GameLobby.swift` | `GameLobby`, `LobbyConfig`, `LocalGames`, `GameProps` | `GameLobby.tsx`, `lobby.ts` |
| `GameControls.swift` | `GamePanel` (`.pong` padding), `PongButton`, `LobbyHead`, `EscButton`, `PongLine`, `PongNote`, `PongLink`, `PongCode`, `PongScore` | `index.css` `.pong*` |
| `GameTheme.swift` | `GameColors` (every CSS colour, `mix` = `color-mix`), shadows, `GameMetrics` (spacing, type) | `index.css` |
| `GameKeys.swift` | `GameKey`, `GameKeyRouter`, `.gameKeys` | `gameKeys.ts` |
| `GameStore.swift` | `GameTime`, `GameSaves`, `GameNickname` | `gameClock.ts`, `gameSaves.ts`, `nickname.ts` |
| `PeerTransport.swift` | `PeerTransport`, `RelayTransport` (the core's `RelayPeer` over URLSessionWebSocketTask) | `peer.ts`, `peerOnline.ts` |
| `GameWindow.swift` | `NativeGameWindowController`: the window, the key monitor, zoom for full screen, pause on blur | `GameWindowController` |

Full screen is the window zoomed ("big window"), as in the web-view build. While it's on, the game area is
centred and `@Environment(\.gameStage)` gives its height (`--game-h`) so boards can grow; a game with a picture of
its own shape calls `shell.fullscreenLayout(GameFullscreenOptions(...))` and lays itself out with
`gameFullscreenLayout(...)` from the core.
