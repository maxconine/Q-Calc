# Game fullscreen (⌃F)

A game panel can get much bigger. In the Mac and Windows apps **Ctrl+F** turns the bar into a large, centred, ordinary window (title bar, close, minimise, zoom, resizable), like any desktop app's; in a browser it's real fullscreen. **Esc**, Ctrl+F again or the window's close button goes back to the bar. The code still calls it "fullscreen".

- `src/lib/gameFullscreen.ts`: the pure parts (chord matcher, hint and button wording, letterbox maths, the panel's natural size), tested in `gameFullscreen.test.ts`
- `src/components/useGameFullscreen.ts`: the hook a panel calls
- `src/components/GameFullscreen.css`: the fullscreen layout (imported by the hook; nothing to import yourself)
- `macos/Overlay.swift`: `setGameWindow`, which turns the panel into the game's window in the Mac app; the window's size is `GameWindow` in `macos/OverlayHealth.swift`, tested in `overlay_test.swift`
- `src-tauri/src/main.rs`: `set_game_window`, the same on Windows; its size is `place::game_window`

## Adopting it in a panel

In Q Calc every game opens inside `GameShell`, which calls the hook once with its root as `ref`, so every game has ⌃F already; f in the lobby or on the paused card and the small fullscreen buttons toggle the same one. A panel reads it with `useShellFullscreen()` (`lib/gameShellContext.ts`) and gives its own `base`/`chrome`/`pad` with `useGameShell()?.fullscreenLayout(...)`, as ember does. What follows is how the hook works for a panel that holds it itself.

```tsx
import { useGameFullscreen } from './useGameFullscreen'

const fs = useGameFullscreen<HTMLDivElement>({
  base: { w: 640, h: 360 }, // the picture at normal size, css px (this is the default)
  chrome: 96,               // height the panel spends around the picture at normal size: head + hud + keys line + gaps
  pad: 12,                  // margin kept at each screen edge at normal size
})

return (
  <div ref={fs.ref} className="ember">          {/* the root: gets .game-fullscreen and covers the window */}
    {head}                                        {/* add game-fs-ui so it scales with the hud */}
    <div className="ember-game game-fs-column">   {/* hud + stage + keys line */}
      <div className="ember-hud game-fs-ui">…</div>
      <div className="ember-stage">
        <canvas className="ember-canvas" />
        <div key={entry} className="ember-hint game-fs-hint">{fs.hint}</div>
      </div>
      <p className="ember-keys game-fs-ui">…</p>
    </div>
  </div>
)
```

The hook returns `{ ref, on, toggle, exit, escape, hint, stage, size, ui, windowed }`.

| | |
|---|---|
| `ref` | put it on the panel's root element (the one that should cover the screen). The hook adds and removes `game-fullscreen` on it; you don't add the class yourself, and a dynamic `className` is fine (it is re-applied after every render) |
| `on` | whether the game is fullscreen now |
| `toggle()` / `exit()` | for a button; ⌃F is already handled by the hook |
| `escape()` | leaves fullscreen and returns `true`, or returns `false` when it wasn't fullscreen. For the esc path, below |
| `hint` | in the apps `⌃F big window` on a Mac, `Ctrl+F big window` on Windows, and while on `⌃F or esc back to the bar`; in a browser `⌃F fullscreen` / `⌃F or esc leaves fullscreen` |
| `stage` | `{ x, y, w, h, scale }` in css px: the letterboxed box for the picture on the screen; `null` when not fullscreen |
| `size` | `{ w, h }`: `stage` in device pixels (dpr capped at 3), the size for a canvas backing store; `null` when not fullscreen |
| `ui` | how much larger hud and text are drawn (1 when not fullscreen, at most `maxUi`, default 2) |
| `windowed` | the apps open a window rather than real fullscreen; `fullscreenLabel(on, windowed)` words a button for it (`big window` / `back to the bar`) |

Other options: `maxUi`, `integer` (whole scales only, for pixel art), `disabled` (⌃F does nothing and fullscreen is left), `onChange(on)`, `onResize(layout | null)` (called on entering, on every size change, and with `null` on leaving).

### Which element gets what

- **Root** (`fs.ref`): becomes `position: fixed; inset: 0`, a centred flex column, with the theme's `--bg` behind it. Its own padding is replaced by `pad × ui`, plus the Mac title bar's strip on top (`--game-fs-inset`, below), so the traffic lights never sit on the game.
- **`game-fs-column`**: the play column. It gets the picture's letterboxed width, so a canvas inside at `width: 100%` with its `aspect-ratio` (what `.ember-canvas` already is) lands exactly on `stage`. Use `game-fs-stage` instead to size the picture's box directly (`width` and `height`) when there's no such column.
- **`game-fs-ui`**: text and controls around the picture (head, hud, keys line). Drawn `ui` times larger with css `zoom`. Never put it on the canvas or an ancestor of the canvas: the canvas must stay in real css pixels.
- **`game-fs-screen`**: a screen without a picture (title, lobby, level select): the base width, zoomed by `ui`, centred.
- **`game-fs-hint`**: fades in, holds, and is gone after 5 s. Opacity only, so place it however you like; give it a new `key` to replay it.

`chrome` must cover everything stacked above and below the picture at normal size. If it's too small the column is taller than the screen and its ends are clipped; too large only costs a little picture. `base`, `chrome` and `pad` together are also the panel's natural size, which the apps give their window's shape.

A rule of your own that sizes from `100vh` should take `var(--game-fs-inset, 0px)` off it, as `GameShell.css` and `EmberPanel.css` do.

### The entry hint

Show `fs.hint` when the player enters a level (or the game): render an element with `game-fs-hint` and a `key` that changes on each entry. No timer needed. Ember already replays its level hint with `key={hud.entry}`; the fullscreen hint can sit under it, or in the keys line (`… · ⌃F fullscreen`), which never goes away. Use `fs.hint` for the text rather than writing the shortcut out: it is worded per platform and changes while fullscreen.

### Resizing the canvas

The css makes the canvas the right size; the backing store is the game's job. A renderer that already sizes itself from `getBoundingClientRect()` × `devicePixelRatio` under a `ResizeObserver` (as `EmberRenderer.resize()` does) needs nothing more: the observer fires when fullscreen begins and ends, and as the app's window is resized. Otherwise use `fs.size` (in an effect on `[fs.size?.w, fs.size?.h]`) or `onResize`, and set `canvas.width/height` from it. `backingSize(css, dpr)` in `gameFullscreen.ts` is the same sum for other boxes.

### Esc

Esc while fullscreen leaves fullscreen and nothing else; the next Esc is the game's.

- Mac app: the shell handles it before the page sees the key (⌘W and the red button do the same).
- Browser: the browser's own Esc ends real fullscreen, and the hook also listens for Esc itself.
- QuickCalc has its own capture-phase Esc handler that may run before the hook's, so a panel with an `escapeRef` must ask the hook first:

```ts
useEffect(() => {
  const esc = () => fs.escape() || back()
  escapeRef.current = esc
  return () => { if (escapeRef.current === esc) escapeRef.current = null }
}, [back, escapeRef, fs.escape])
```

The panel's own "esc" button should do the same (`fs.escape() || back() || onClose()`).

## What the panel must not do

- Don't post `gameFullscreen` to the shell, call `requestFullscreen`, or add `game-fullscreen` yourself. The hook owns all three.
- Don't handle Ctrl+F. The hook swallows it in the capture phase (`stopImmediatePropagation`), so it never reaches the game or the calculator. `plainKey` handlers already ignore it.
- Don't set `style.minHeight` on the root's parent: the hook holds the parent at the panel's height while the root is out of the flow, so the bar behind doesn't collapse and report a new height to the shell.
- Don't put `transform`, `filter`, `backdrop-filter`, `contain` or `will-change: transform` on an ancestor of the root: any of them makes `position: fixed` relative to that ancestor instead of the window. (`.spotlight`'s `overflow: hidden` is fine.)
- Don't unmount the root to change screens. The hook lives as long as the component that calls it; when that unmounts (the game closing any way at all) fullscreen is left.
- One fullscreen-capable panel at a time: the shell callback `window.__qcalcGameFullscreen` belongs to the mounted hook.

## Native protocol (the apps)

Page to shell, through `window.webkit.messageHandlers.qcalc.postMessage` (the Windows shim routes it to the `host` command):

```js
{ type: 'gameFullscreen', on: true, w: 672, h: 392 }  // the game's window, in the shape of the panel's natural size (css px)
{ type: 'gameFullscreen', on: false }                 // back to the bar
```

Shell to page:

```js
window.__qcalcGameFullscreen(false)  // the shell left the window itself (esc, close button, the panel hid), or refused to enter
document.documentElement.style.setProperty('--game-fs-inset', '28px')  // mac: the title bar's strip, removed on leaving
```

### Mac (`setGameWindow` in `Overlay.swift`)

- **On**: remembers the bar's frame and turns the same panel into an ordinary window: titled, closable, miniaturizable, resizable, full-size content under a transparent title bar with no title, `.normal` level, one space, the green button zooming (`.fullScreenNone`) rather than going fullscreen, a solid background with the system's rounded corners and shadow, and the web view's own 12 px corner taken off. Its frame is `GameWindow.frame`: the natural size scaled up to fill 80% of the visible frame of the bar's display, with the title bar on top, never smaller than the natural size, centred; it resizes down to the natural size. The page is told the title bar's height first, then the change happens in one step behind a ~0.2 s fade. The app is activated (the panel stays non-activating, so the shell does it on entering, on a click in the window and on restoring from the Dock) and the web view takes the keys. Refused (and the page told `false`) when the panel isn't up or shows the plain fallback bar.
- **While on**: `size` messages are ignored (`applySize` returns early) and the bar's edge strips neither drag nor resize; the window's own frame does. A press in the title bar strip drags the window (a double click zooms or minimises as the system setting says), since the page lies under it. Clicking away leaves it up (a game is open), ⌘M and the yellow button minimise it, and the hotkey brings a minimised or covered window forward; on the window in front it hides as usual.
- **Off**: the red button, ⌘W, Esc, ⌃F, or the page. Restores the bar's style, level, spaces, background and frame, then calls `__qcalcSize()` so the page reports its current height. A minimised window is swapped for a fresh panel in the bar's frame (its Dock tile closed).
- **Left automatically** on every `hide` (hotkey, the page's `dismiss`), before every show, when the page's process ends, and when the panel or page is rebuilt. When the bar hides, the app that was in front before the game's window activated Q Calc gets the front back.

### Windows (`set_game_window` in `main.rs`)

- **On**: remembers the bar's position and size, takes off dwm's flyout edge (`frame::bare`), and gives the window windows' own frame: decorations, shadow, resizable, minimise and maximise, in the taskbar, no longer always on top. Its inner size is `place::game_window` (the natural size scaled to 80% of the monitor's work area less the frame, never smaller than the natural size), with that natural size as its minimum, centred on the monitor, focused.
- **Off**: the close button (`CloseRequested` is turned into this), Esc, Ctrl+F or the page. Restores the bar's frame and flags and dwm's edge, and asks the page for its size.
- Hiding the bar leaves it first; the hotkey brings a minimised or covered window forward. The shim's edge strips stand down while `game-fullscreen-on` is on `<html>`.

## Browser

The root goes fullscreen through the Fullscreen API (`requestFullscreen`, with the `webkit` fallback). If that is refused, the same class gives a fixed layout filling the window. `on` follows `fullscreenchange`, so the browser's own Esc is noticed.
