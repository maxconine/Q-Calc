import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type RefObject } from 'react'
import { nativeHandler, nativeWindow, type NativeWindow } from '../lib/bridge'
import {
  backingSize,
  fullscreenHint,
  fullscreenLayout,
  GAME_FULLSCREEN_CLASS,
  GAME_FULLSCREEN_ROOT_CLASS,
  isFullscreenChord,
  isPlainEscape,
  macLike,
  type Box,
  type FullscreenLayout,
  type Size,
} from '../lib/gameFullscreen'
import { swallow } from '../lib/gameKeys'
import { isWindowsHost } from '../lib/platform'
import './GameFullscreen.css'

// fullscreen for a game panel, on ⌃F. in the mac app the shell grows the panel over the whole display and
// the page only has to lay itself out to fill its window; in a browser and the windows shell the panel's
// root goes fullscreen through the Fullscreen API, and fills the window when that's refused.
// GAME_FULLSCREEN.md is the how-to

export type GameFullscreenOptions = {
  // the picture at its normal size in css pixels; the default is ember & frost's 640 × 360
  base?: Size
  // height the panel spends around the picture at normal size (head, hud, keys line, gaps)
  chrome?: number
  // room kept clear at each screen edge at normal size; it becomes the root's padding while fullscreen
  pad?: number
  // the hud never grows past this many times its normal size (2)
  maxUi?: number
  // whole scales only, for pixel art
  integer?: boolean
  // ⌃F does nothing and fullscreen is left, e.g. on a screen that has no business being giant
  disabled?: boolean
  onChange?: (on: boolean) => void
  // on entering, on every change of the screen's size, and with null on leaving
  onResize?: (layout: FullscreenLayout | null) => void
}

export type GameFullscreen<T extends HTMLElement> = {
  // goes on the panel's root element, the one that should cover the screen
  ref: RefObject<T | null>
  on: boolean
  toggle: () => void
  exit: () => void
  // for the panel's esc path: leaves fullscreen and says true, or says false when there was none to leave
  escape: () => boolean
  // what to tell the player: `⌃F fullscreen`, or how to leave while it's on
  hint: string
  // where the picture goes on the screen in css pixels, null when not fullscreen
  stage: Box | null
  // that box in device pixels: the size to give a canvas's backing store
  size: Size | null
  // how much larger the hud is drawn (1 when not fullscreen)
  ui: number
}

type FullscreenWindow = NativeWindow & { __qcalcGameFullscreen?: (on: boolean) => void }
type LegacyElement = HTMLElement & { webkitRequestFullscreen?: () => void }
type LegacyDocument = Document & { webkitFullscreenElement?: Element | null; webkitExitFullscreen?: () => void }

const DEFAULT_BASE: Size = { w: 640, h: 360 }
const VARS = ['--game-fs-w', '--game-fs-h', '--game-fs-ui', '--game-fs-pad', '--game-fs-base-w'] as const

function fullscreenElement(): Element | null {
  const doc = document as LegacyDocument
  return doc.fullscreenElement ?? doc.webkitFullscreenElement ?? null
}

function leaveDomFullscreen(): void {
  const doc = document as LegacyDocument
  try {
    if (doc.exitFullscreen) void doc.exitFullscreen().catch(() => {})
    else doc.webkitExitFullscreen?.()
  } catch {
    // nothing was fullscreen
  }
}

// the mac shell does the growing itself; its web view has no element fullscreen to ask for
function macHost(): boolean {
  return Boolean(nativeHandler()) && !isWindowsHost()
}

function tellHost(on: boolean): void {
  nativeHandler()?.postMessage({ type: 'gameFullscreen', on })
}

function sameLayout(a: FullscreenLayout | null, b: FullscreenLayout | null): boolean {
  if (!a || !b) return a === b
  return a.ui === b.ui && a.stage.x === b.stage.x && a.stage.y === b.stage.y && a.stage.w === b.stage.w && a.stage.h === b.stage.h
}

export function useGameFullscreen<T extends HTMLElement = HTMLDivElement>(options: GameFullscreenOptions = {}): GameFullscreen<T> {
  const ref = useRef<T | null>(null)
  const [on, setOn] = useState(false)
  const [layout, setLayout] = useState<FullscreenLayout | null>(null)
  // read by key handlers and the host's callback, which can't wait for a render
  const onRef = useRef(false)
  // the browser really made the element fullscreen, so its own esc and ui can end it
  const domRef = useRef(false)
  // the panel's height in the bar, held open while its root is lifted out of the flow
  const heightRef = useRef(0)
  const opts = useRef(options)
  opts.current = options
  const { disabled = false } = options

  const set = useCallback((next: boolean) => {
    if (onRef.current === next) return
    onRef.current = next
    setOn(next)
    opts.current.onChange?.(next)
  }, [])

  const exit = useCallback((): boolean => {
    if (!onRef.current) return false
    set(false)
    tellHost(false)
    if (domRef.current || (ref.current && fullscreenElement() === ref.current)) leaveDomFullscreen()
    domRef.current = false
    return true
  }, [set])

  const enter = useCallback(() => {
    const el = ref.current
    if (onRef.current || !el || opts.current.disabled) return
    heightRef.current = el.getBoundingClientRect().height
    set(true)
    tellHost(true)
    if (macHost()) return
    // refused (no user gesture, an iframe without the permission, an old web view): the fixed layout the
    // class gives is the fullscreen, as large as the window
    const legacy = el as LegacyElement
    try {
      if (el.requestFullscreen) {
        void el.requestFullscreen({ navigationUI: 'hide' }).then(
          () => {
            // left again before the browser got there
            if (!onRef.current) leaveDomFullscreen()
            else domRef.current = true
          },
          () => {},
        )
      } else legacy.webkitRequestFullscreen?.()
    } catch {
      // as above
    }
  }, [set])

  const toggle = useCallback(() => {
    if (onRef.current) exit()
    else enter()
  }, [enter, exit])

  const leave = useCallback(() => {
    exit()
  }, [exit])

  // ⌃F, and esc while fullscreen. capture, and stopped dead, so neither reaches the calculator or the game
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (isFullscreenChord(e)) {
        swallow(e)
        e.stopImmediatePropagation()
        if (!e.repeat) toggle()
      } else if (onRef.current && isPlainEscape(e)) {
        swallow(e)
        e.stopImmediatePropagation()
        exit()
      }
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [exit, toggle])

  // the browser's own way out (its esc, its button) and its late answers
  useEffect(() => {
    const onChange = () => {
      const ours = Boolean(ref.current) && fullscreenElement() === ref.current
      if (ours) {
        if (onRef.current) domRef.current = true
        else leaveDomFullscreen()
      } else if (domRef.current) {
        domRef.current = false
        exit()
      }
    }
    document.addEventListener('fullscreenchange', onChange)
    document.addEventListener('webkitfullscreenchange', onChange)
    return () => {
      document.removeEventListener('fullscreenchange', onChange)
      document.removeEventListener('webkitfullscreenchange', onChange)
    }
  }, [exit])

  // the mac shell leaves fullscreen by itself on esc and whenever the panel hides, and says so here
  useEffect(() => {
    const w = nativeWindow() as FullscreenWindow | undefined
    if (!w) return
    const fromHost = (next: boolean) => {
      if (!next) set(false)
      // the shell thinks a game is fullscreen that isn't: put it right
      else if (!onRef.current) tellHost(false)
    }
    w.__qcalcGameFullscreen = fromHost
    return () => {
      if (w.__qcalcGameFullscreen === fromHost) delete w.__qcalcGameFullscreen
    }
  }, [set])

  useEffect(() => {
    if (disabled) exit()
  }, [disabled, exit])

  // every render, so a className react rewrites can't drop it
  useLayoutEffect(() => {
    ref.current?.classList.toggle(GAME_FULLSCREEN_CLASS, on)
  })

  // the layout: measured from the root once the class has made it fill the window, and again whenever the
  // window changes (the mac panel growing, a display change, the browser's fullscreen arriving)
  useLayoutEffect(() => {
    const el = ref.current
    document.documentElement.classList.toggle(GAME_FULLSCREEN_ROOT_CLASS, on)
    if (!on || !el) {
      setLayout(null)
      return
    }
    // the root leaves the flow, and the bar around it would collapse and report that height to the shell
    // (which sizes its window from it); the parent keeps the height it had, so nothing behind the game moves
    const parent = el.parentElement
    const parentMin = parent?.style.minHeight ?? ''
    if (parent && heightRef.current > 0) parent.style.minHeight = `${Math.ceil(heightRef.current)}px`
    el.classList.add(GAME_FULLSCREEN_CLASS)
    const measure = () => {
      const o = opts.current
      const box = el.getBoundingClientRect()
      const viewport = { w: box.width || window.innerWidth, h: box.height || window.innerHeight }
      const base = o.base ?? DEFAULT_BASE
      const next = fullscreenLayout(viewport, { base, chrome: o.chrome, pad: o.pad, maxUi: o.maxUi, integer: o.integer })
      el.style.setProperty('--game-fs-w', `${next.stage.w}px`)
      el.style.setProperty('--game-fs-h', `${next.stage.h}px`)
      el.style.setProperty('--game-fs-ui', String(next.ui))
      el.style.setProperty('--game-fs-pad', `${o.pad ?? 0}px`)
      el.style.setProperty('--game-fs-base-w', `${base.w}px`)
      setLayout((prev) => (sameLayout(prev, next) ? prev : next))
      o.onResize?.(next)
    }
    measure()
    const ro = typeof ResizeObserver !== 'undefined' ? new ResizeObserver(measure) : null
    ro?.observe(el)
    window.addEventListener('resize', measure)
    return () => {
      ro?.disconnect()
      window.removeEventListener('resize', measure)
      for (const name of VARS) el.style.removeProperty(name)
      el.classList.remove(GAME_FULLSCREEN_CLASS)
      if (parent) parent.style.minHeight = parentMin
      document.documentElement.classList.remove(GAME_FULLSCREEN_ROOT_CLASS)
      opts.current.onResize?.(null)
    }
  }, [on])

  // the game closing, however it closes, never leaves the window giant
  useEffect(
    () => () => {
      if (!onRef.current) return
      onRef.current = false
      tellHost(false)
      if (domRef.current) leaveDomFullscreen()
      domRef.current = false
    },
    [],
  )

  const hint = useMemo(() => fullscreenHint(macLike(isWindowsHost(), typeof navigator === 'undefined' ? '' : navigator.platform), on), [on])
  const stage = on ? (layout?.stage ?? null) : null
  const dpr = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1
  const size = useMemo(() => (stage ? backingSize(stage, dpr) : null), [stage, dpr])

  return { ref, on, toggle, exit: leave, escape: exit, hint, stage, size, ui: on ? (layout?.ui ?? 1) : 1 }
}
