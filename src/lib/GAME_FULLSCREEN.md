# Game fullscreen (⌃F)

A game panel can take over the whole display. **Ctrl+F** toggles it, **Esc** or Ctrl+F again leaves it.

- `src/lib/gameFullscreen.ts`: the pure parts (chord matcher, hint text, letterbox maths), tested in `gameFullscreen.test.ts`
- `src/components/useGameFullscreen.ts`: the hook a panel calls
- `src/components/GameFullscreen.css`: the fullscreen layout (imported by the hook; nothing to import yourself)
- `macos/Overlay.swift`: `setGameFullscreen`, which grows the panel over the display in the Mac app

## Adopting it in a panel

```tsx
import { useGameFullscreen } from './useGameFullscreen'

const fs = useGameFullscreen<HTMLDivElement>({
  base: { w: 640, h: 360 }, // the picture at normal size, css px (this is the default)
  chrome: 96,               // height the panel spends around the picture at normal size: head + hud + keys line + gaps
  pad: 12,                  // margin kept at each screen edge at normal size
})

return (
  <div ref={fs.ref} className="ember">          {/* the root: gets .game-fullscreen and covers the screen */}
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

The hook returns `{ ref, on, toggle, exit, escape, hint, stage, size, ui }`.

| | |
|---|---|
| `ref` | put it on the panel's root element (the one that should cover the screen). The hook adds and removes `game-fullscreen` on it; you don't add the class yourself, and a dynamic `className` is fine (it is re-applied after every render) |
| `on` | whether the game is fullscreen now |
| `toggle()` / `exit()` | for a button; ⌃F is already handled by the hook |
| `escape()` | leaves fullscreen and returns `true`, or returns `false` when it wasn't fullscreen. For the esc path, below |
| `hint` | `⌃F fullscreen` on a Mac, `Ctrl+F fullscreen` elsewhere; while on, `⌃F or esc leaves fullscreen` |
| `stage` | `{ x, y, w, h, scale }` in css px: the letterboxed box for the picture on the screen; `null` when not fullscreen |
| `size` | `{ w, h }`: `stage` in device pixels (dpr capped at 3), the size for a canvas backing store; `null` when not fullscreen |
| `ui` | how much larger hud and text are drawn (1 when not fullscreen, at most `maxUi`, default 2) |

Other options: `maxUi`, `integer` (whole scales only, for pixel art), `disabled` (⌃F does nothing and fullscreen is left), `onChange(on)`, `onResize(layout | null)` (called on entering, on every size change, and with `null` on leaving).

### Which element gets what

- **Root** (`fs.ref`): becomes `position: fixed; inset: 0`, a centred flex column, with the theme's `--bg` behind it. Its own padding is replaced by `pad × ui`.
- **`game-fs-column`**: the play column. It gets the picture's letterboxed width, so a canvas inside at `width: 100%` with its `aspect-ratio` (what `.ember-canvas` already is) lands exactly on `stage`. Use `game-fs-stage` instead to size the picture's box directly (`width` and `height`) when there's no such column.
- **`game-fs-ui`**: text and controls around the picture (head, hud, keys line). Drawn `ui` times larger with css `zoom`. Never put it on the canvas or an ancestor of the canvas: the canvas must stay in real css pixels.
- **`game-fs-screen`**: a screen without a picture (title, lobby, level select): the base width, zoomed by `ui`, centred.
- **`game-fs-hint`**: fades in, holds, and is gone after 5 s. Opacity only, so place it however you like; give it a new `key` to replay it.

`chrome` must cover everything stacked above and below the picture at normal size. If it's too small the column is taller than the screen and its ends are clipped; too large only costs a little picture.

### The entry hint

Show `fs.hint` when the player enters a level (or the game): render an element with `game-fs-hint` and a `key` that changes on each entry. No timer needed. Ember already replays its level hint with `key={hud.entry}`; the fullscreen hint can sit under it, or in the keys line (`… · ⌃F fullscreen`), which never goes away. Use `fs.hint` for the text rather than writing the shortcut out: it is worded per platform and changes while fullscreen.

### Resizing the canvas

The css makes the canvas the right size; the backing store is the game's job. A renderer that already sizes itself from `getBoundingClientRect()` × `devicePixelRatio` under a `ResizeObserver` (as `EmberRenderer.resize()` does) needs nothing more: the observer fires when fullscreen begins and ends. Otherwise use `fs.size` (in an effect on `[fs.size?.w, fs.size?.h]`) or `onResize`, and set `canvas.width/height` from it. `backingSize(css, dpr)` in `gameFullscreen.ts` is the same sum for other boxes.

### Esc

Esc while fullscreen leaves fullscreen and nothing else; the next Esc is the game's.

- Mac app: the shell handles it before the page sees the key.
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

## Native protocol (Mac app)

Page to shell, through `window.webkit.messageHandlers.qcalc.postMessage`:

```js
{ type: 'gameFullscreen', on: true }   // grow the panel over the display it is on
{ type: 'gameFullscreen', on: false }  // back to the bar's frame
```

Shell to page:

```js
window.__qcalcGameFullscreen(false)  // the shell left fullscreen itself (esc, or the panel hid), or refused to enter
```

In `Overlay.swift`, `setGameFullscreen`:

- **On**: remembers the panel's frame, drops its shadow, raises it one level above `.statusBar` (over the menu bar, Dock and status items), sets its frame to `screen.frame` and keeps it key. The frame moves in one step behind a ~0.2 s fade rather than animating: the page needs a frame or two to lay out at the new size, and an animating frame would let a size report or a hide land halfway. Refused (and the page told `false`) when the panel isn't up or shows the plain fallback bar.
- **While on**: `size` messages are ignored (`applySize` returns early), and the edge strips neither drag nor resize. Losing focus, a click in another app, the hotkey and ⌘W still hide the panel as usual.
- **Off**: restores the remembered frame, level and shadow, then calls `__qcalcSize()` so the page reports its current height.
- **Left automatically** on Esc (first Esc only leaves fullscreen), on every `hide` (lost focus, hotkey, ⌘W, the page's `dismiss`), before every show, on a display change or wake, when the page's process ends, and when the panel or page is rebuilt. The bar can't come back the size of the screen.

## Browser and Windows

The root goes fullscreen through the Fullscreen API (`requestFullscreen`, with the `webkit` fallback). If that is refused, the same class gives a fixed layout filling the window. `on` follows `fullscreenchange`, so the browser's own Esc is noticed.

The Windows shell receives the same `gameFullscreen` message (its `host` command ignores unknown types today). Until `src-tauri/src/main.rs` handles it by making the window fullscreen, Windows depends on WebView2 granting element fullscreen; where it doesn't, the game only fills the bar's own window.
