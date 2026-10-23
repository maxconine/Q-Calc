// fullscreen for the games (⌃F): the pure parts. the chord, the wording of its hint, and where a game's
// picture sits on a screen that isn't its shape. in the apps it's a big window of the game's own rather than
// the whole display. useGameFullscreen does the rest; GAME_FULLSCREEN.md says how a panel adopts it

// on the panel's root while it fills the screen
export const GAME_FULLSCREEN_CLASS = 'game-fullscreen'
// on <html> meanwhile, for page-level rules (no scroll, no edge grips)
export const GAME_FULLSCREEN_ROOT_CLASS = 'game-fullscreen-on'
// the mac shell sets this on <html> to the title bar's height, the strip its traffic lights sit in
export const GAME_FULLSCREEN_INSET_VAR = '--game-fs-inset'

export type Size = { w: number; h: number }
// a rect inside its container, and how many times the game's own size it is
export type Box = { x: number; y: number; w: number; h: number; scale: number }

type Chord = Pick<KeyboardEvent, 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey' | 'key'> & { code?: string }

// ctrl+F and nothing else held: ⌘F, ⌃⇧F and ⌃⌥F are other shortcuts. by the layout's letter; a layout
// without latin letters falls back to the key where f sits on qwerty, like the mac app's ⌘W
export function isFullscreenChord(e: Chord): boolean {
  if (!e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return false
  const key = e.key.toLowerCase()
  if (key === 'f') return true
  // ctrl+F can arrive as the control character it types
  if (key === '\u0006') return true
  const latin = key.length === 1 && key >= 'a' && key <= 'z'
  return !latin && key.length === 1 && e.code === 'KeyF'
}

// escape with nothing held, the other way out
export function isPlainEscape(e: Chord): boolean {
  return (e.key === 'Escape' || e.key === 'Esc') && !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey
}

// macs write the control key as a glyph, like the rest of the app's hints. `windowed`: the apps open a big
// window rather than the whole screen
export function fullscreenHint(mac: boolean, on = false, windowed = false): string {
  const chord = mac ? '⌃F' : 'Ctrl+F'
  if (windowed) return on ? `${chord} or esc back to the bar` : `${chord} big window`
  return on ? `${chord} or esc leaves fullscreen` : `${chord} fullscreen`
}

// what the button and the f key are called
export function fullscreenLabel(on: boolean, windowed = false): string {
  if (windowed) return on ? 'back to the bar' : 'big window'
  return on ? 'leave full screen' : 'full screen'
}

// the windows shell says so itself; a browser is asked what it runs on
export function macLike(windowsHost: boolean, platform: string): boolean {
  return !windowsHost && /mac|iphone|ipad/i.test(platform)
}

const EMPTY: Box = { x: 0, y: 0, w: 0, h: 0, scale: 0 }

// the largest centred rect of the game's shape that fits the container, on whole css pixels. `integer`
// keeps the scale a whole number once there's room for one, for pixel art that mustn't blur
export function letterbox(container: Size, base: Size, opts: { integer?: boolean } = {}): Box {
  if (!(container.w > 0 && container.h > 0 && base.w > 0 && base.h > 0)) return EMPTY
  let scale = Math.min(container.w / base.w, container.h / base.h)
  if (!Number.isFinite(scale) || scale <= 0) return EMPTY
  if (opts.integer && scale >= 1) scale = Math.floor(scale)
  const w = Math.min(Math.floor(container.w), Math.max(1, Math.floor(base.w * scale + 1e-6)))
  // from the width, so the shape survives the rounding
  const h = Math.min(Math.floor(container.h), Math.max(1, Math.round((w * base.h) / base.w)))
  return { x: Math.floor((container.w - w) / 2), y: Math.floor((container.h - h) / 2), w, h, scale: w / base.w }
}

export type FullscreenLayoutOptions = {
  // the game's picture at its normal size, in css pixels
  base: Size
  // height the panel spends around the picture at normal size: hud, keys line, gaps
  chrome?: number
  // room kept clear at each screen edge, at normal size
  pad?: number
  // the hud and text never grow past this many times their normal size
  maxUi?: number
  integer?: boolean
}

export type FullscreenLayout = {
  viewport: Size
  // where the picture goes, inside the viewport
  stage: Box
  // how much larger the hud and text are drawn
  ui: number
}

// the hud grows as if the whole small panel were scaled up to the screen (capped, so a 5k display doesn't
// get inch-high text), and the picture takes the largest box of its shape in what's left
export function fullscreenLayout(viewport: Size, opts: FullscreenLayoutOptions): FullscreenLayout {
  const { base, chrome = 0, pad = 0, maxUi = 2 } = opts
  const fit = Math.min(viewport.w / (base.w + 2 * pad), viewport.h / (base.h + chrome + 2 * pad))
  const ui = Number.isFinite(fit) ? Math.floor(Math.min(Math.max(fit, 1), Math.max(1, maxUi)) * 100) / 100 : 1
  const top = (pad + chrome) * ui
  const room = { w: viewport.w - 2 * pad * ui, h: viewport.h - top - pad * ui }
  const box = letterbox(room, base, { integer: opts.integer })
  // the chrome's room is split evenly above and below the picture, which is how the css centres the column
  const stage = box.w ? { ...box, x: box.x + Math.floor(pad * ui), y: box.y + Math.floor(pad * ui + (chrome * ui) / 2) } : box
  return { viewport, stage, ui }
}

// the panel's own size at normal size, for the shell to give the game's window that shape
export function naturalSize(opts: Pick<FullscreenLayoutOptions, 'base' | 'chrome' | 'pad'>): Size {
  const { base, chrome = 0, pad = 0 } = opts
  return { w: Math.round(base.w + 2 * pad), h: Math.round(base.h + chrome + 2 * pad) }
}

// the canvas's backing store for a css size: device pixels, so it's crisp on a retina screen, capped so a
// huge display doesn't cost a frame rate
export function backingSize(css: Size, dpr: number, maxDpr = 3): Size {
  const ratio = Math.min(Math.max(Number.isFinite(dpr) ? dpr : 1, 1), maxDpr)
  return { w: Math.max(0, Math.round(css.w * ratio)), h: Math.max(0, Math.round(css.h * ratio)) }
}
