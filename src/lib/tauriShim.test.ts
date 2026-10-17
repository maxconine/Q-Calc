import { describe, expect, it } from 'vitest'
import shim from '../../src-tauri/src/shim.js?raw'

type Listener = (e: Record<string, unknown>) => void
type FakeWindow = Record<string, unknown> & { webkit?: { messageHandlers: Record<string, { postMessage: (m: unknown) => unknown }> } }

const SETTINGS = { theme: 'dark', sigFigs: 9, hotkey: 'Alt+Space', hotkeyFailed: false }

// runs the shim the way rust injects it, against a stand-in window and document
// rootLater: like webview2, the script runs before the document has its <html>; call addRoot() to parse it in
function boot(overlay: boolean, own?: Record<string, { postMessage: (m: unknown) => unknown }>, rootLater = false) {
  const sent: unknown[] = []
  const listeners: Record<string, Listener[]> = {}
  const classes: string[] = []
  const win: FakeWindow = {
    __TAURI_INTERNALS__: {
      invoke: (cmd: string, args: { message: unknown }) => {
        sent.push([cmd, args.message])
        return Promise.resolve()
      },
    },
    innerWidth: 680,
    innerHeight: 72,
    addEventListener: (type: string, f: Listener) => (listeners[type] ??= []).push(f),
    removeEventListener: (type: string, f: Listener) => {
      listeners[type] = (listeners[type] ?? []).filter((g) => g !== f)
    },
    dispatchEvent: () => true,
  }
  if (own) Object.defineProperty(win, 'webkit', { get: () => ({ messageHandlers: own }), configurable: true })
  const root = { classList: { add: (c: string) => classes.push(c) }, style: {} as Record<string, string>, dataset: {} as Record<string, string> }
  const doc: { documentElement: typeof root | null; querySelector: () => null; activeElement: null } = {
    documentElement: rootLater ? null : root,
    querySelector: () => null,
    activeElement: null,
  }
  const watchers: Array<() => void> = []
  function FakeObserver(this: { disconnect: () => void; observe: () => void }, callback: () => void) {
    let on = false
    const watcher = () => on && callback()
    this.observe = () => {
      on = true
      watchers.push(watcher)
    }
    this.disconnect = () => (on = false)
  }
  const addRoot = () => {
    doc.documentElement = root
    for (const w of watchers) w()
  }
  const boot = { overlay, settings: SETTINGS, onboarding: { opens: 2, commits: 0, hints: 16, done: false }, rates: overlay ? { base: 'EUR', rates: { USD: 1.1 } } : null, os: 'Windows 11 26200' }
  const source = shim.replace('__QCALC_BOOT__', JSON.stringify(boot))
  new Function('window', 'document', 'CustomEvent', 'MutationObserver', source)(win, doc, function () {}, FakeObserver)
  const fire = (type: string, e: Record<string, unknown>) => {
    let stopped = false
    for (const f of [...(listeners[type] ?? [])]) f({ preventDefault() {}, stopPropagation: () => (stopped = true), ...e })
    return stopped
  }
  return { win, root, classes, sent, fire, addRoot }
}

describe('windows shim', () => {
  it('gives the page the mac door into rust', () => {
    const { win, sent } = boot(true)
    win.webkit!.messageHandlers.qcalc!.postMessage({ type: 'size', height: 100 })
    expect(sent).toEqual([['host', { type: 'size', height: 100 }]])
  })

  it('keeps wkwebview’s own handlers reachable under tauri dev on a mac', () => {
    const ipc = { postMessage: () => 'ipc' }
    const { win, sent } = boot(true, { ipc })
    expect(win.webkit!.messageHandlers.ipc).toBe(ipc)
    win.webkit!.messageHandlers.qcalc!.postMessage({ type: 'dismiss' })
    expect(sent).toEqual([['host', { type: 'dismiss' }]])
  })

  it('injects the host, settings, onboarding, rates and theme before the page runs', () => {
    const { win, root, classes } = boot(true)
    expect(win.__QCALC_PLATFORM).toBe('windows')
    expect(win.__QCALC_NATIVE).toBe(true)
    expect(win.__QCALC_SMOKE_ROOM).toBe(true)
    expect(win.__QCALC_SETTINGS).toEqual(SETTINGS)
    expect(win.__QCALC_ONBOARDING).toEqual({ opens: 2, commits: 0, hints: 16, done: false })
    expect(win.__QCALC_RATES).toEqual({ base: 'EUR', rates: { USD: 1.1 } })
    expect(root.dataset.theme).toBe('dark')
    expect(root.dataset.host).toBe('windows')
    expect(classes).toContain('quick-native')
  })

  it('waits for the <html> element webview2 hasn’t made yet, and still wires up the keys', () => {
    const { root, classes, sent, fire, addRoot } = boot(true, undefined, true)
    expect(root.dataset.host).toBeUndefined()
    expect(fire('keydown', { key: 'Escape' })).toBe(true)
    expect(sent).toEqual([['host', { type: 'dismiss' }]])
    addRoot()
    expect(root.dataset.host).toBe('windows')
    expect(root.dataset.theme).toBe('dark')
    expect(classes).toContain('quick-native')
  })

  it('esc hides; the settings key is the page’s, since the user picks it', () => {
    const { sent, fire } = boot(true)
    expect(fire('keydown', { key: 'Escape' })).toBe(true)
    fire('keydown', { key: ',', ctrlKey: true, altKey: false, shiftKey: false, metaKey: false })
    expect(sent).toEqual([['host', { type: 'dismiss' }]])
  })

  it('esc steps back through a layer of the page first, and ctrl+w closes whatever is open', () => {
    const { win, sent, fire } = boot(true)
    let layers = 1
    win.__qcalcEscape = () => layers-- > 0
    expect(fire('keydown', { key: 'Escape' })).toBe(true)
    expect(sent).toEqual([])
    fire('keydown', { key: 'Escape' })
    expect(sent).toEqual([['host', { type: 'dismiss' }]])
    layers = 1
    expect(fire('keydown', { key: 'w', ctrlKey: true, altKey: false, shiftKey: false, metaKey: false })).toBe(true)
    expect(sent).toHaveLength(2)
    // typing a w, or ctrl+shift+w, is the page's
    expect(fire('keydown', { key: 'w', ctrlKey: false })).toBe(false)
    expect(fire('keydown', { key: 'W', ctrlKey: true, shiftKey: true })).toBe(false)
    expect(sent).toHaveLength(2)
  })

  it('ctrl+w closes an open reference sheet before the bar', () => {
    const { win, sent, fire } = boot(true)
    let sheets = 1
    win.__qcalcCloseSheet = () => sheets-- > 0
    expect(fire('keydown', { key: 'w', ctrlKey: true, altKey: false, shiftKey: false, metaKey: false })).toBe(true)
    expect(sent).toEqual([])
    fire('keydown', { key: 'w', ctrlKey: true, altKey: false, shiftKey: false, metaKey: false })
    expect(sent).toEqual([['host', { type: 'dismiss' }]])
  })

  it('leaves esc to an open game', () => {
    const { win, sent, fire } = boot(true)
    win.__QCALC_GAME_OPEN = true
    expect(fire('keydown', { key: 'Escape' })).toBe(false)
    expect(sent).toEqual([])
  })

  it('tells the page which windows it is on, for the usage counts', () => {
    expect(boot(true).win.__QCALC_OS).toBe('Windows 11 26200')
    expect(boot(false).win.__QCALC_OS).toBeUndefined()
  })

  it('a press along the top edge or a top corner moves the bar, before the input can take it', () => {
    const { sent, fire } = boot(true)
    const target = { closest: () => null }
    expect(fire('mousedown', { detail: 1, button: 0, clientX: 300, clientY: 4, target })).toBe(true)
    expect(fire('mousedown', { detail: 1, button: 0, clientX: 675, clientY: 30, target })).toBe(true)
    expect(sent).toEqual([
      ['host', { type: 'drag' }],
      ['host', { type: 'drag' }],
    ])
    // the middle of the bar, a right click, and a button or the tape are the page's
    expect(fire('mousedown', { detail: 1, button: 0, clientX: 300, clientY: 40, target })).toBe(false)
    expect(fire('mousedown', { detail: 1, button: 2, clientX: 300, clientY: 4, target })).toBe(false)
    expect(fire('mousedown', { detail: 1, button: 0, clientX: 300, clientY: 4, target: { closest: () => ({}) } })).toBe(false)
    expect(sent).toHaveLength(2)
  })

  it('a thin strip along the bottom moves the bar too, as on the mac', () => {
    const { sent, fire } = boot(true)
    const target = { closest: () => null }
    expect(fire('mousedown', { detail: 1, button: 0, clientX: 300, clientY: 69, target })).toBe(true)
    expect(fire('mousedown', { detail: 1, button: 0, clientX: 300, clientY: 60, target })).toBe(false)
    expect(sent).toEqual([['host', { type: 'drag' }]])
  })

  it('dragging a side edge widens the bar by what the pointer moved on screen', () => {
    const { sent, fire } = boot(true)
    const target = { closest: () => null }
    expect(fire('mousedown', { detail: 1, button: 0, clientX: 677, clientY: 50, screenX: 1000, target })).toBe(true)
    fire('mousemove', { screenX: 1030, clientX: 707, clientY: 50, target })
    fire('mousemove', { screenX: 1020, clientX: 697, clientY: 50, target })
    fire('mouseup', {})
    // let go, so later moves widen nothing
    fire('mousemove', { screenX: 1200, clientX: 300, clientY: 50, target })
    expect(sent).toEqual([
      ['host', { type: 'widen', side: 'right', dx: 30 }],
      ['host', { type: 'widen', side: 'right', dx: -10 }],
      ['host', { type: 'widenEnd', side: 'right' }],
    ])
    sent.length = 0
    // the left edge grows the bar as the pointer goes left
    fire('mousedown', { detail: 1, button: 0, clientX: 3, clientY: 50, screenX: 400, target })
    fire('mousemove', { screenX: 360, clientX: 3, clientY: 50, target })
    fire('mouseup', {})
    expect(sent).toEqual([
      ['host', { type: 'widen', side: 'left', dx: 40 }],
      ['host', { type: 'widenEnd', side: 'left' }],
    ])
  })

  it('a double click on the top edge recentres; one lower down does not', () => {
    const { sent, fire } = boot(true)
    const target = { closest: () => null }
    fire('mousedown', { detail: 2, button: 0, clientX: 300, clientY: 40, target })
    expect(sent).toEqual([])
    fire('mousedown', { detail: 2, button: 0, clientX: 300, clientY: 4, target })
    fire('mousedown', { detail: 2, button: 0, clientX: 675, clientY: 30, target })
    expect(sent).toEqual([
      ['host', { type: 'recenter' }],
      ['host', { type: 'recenter' }],
    ])
  })

  it('the settings window gets the door and settings, not the overlay’s keys', () => {
    const { win, root, classes, sent, fire } = boot(false)
    expect(win.__QCALC_SETTINGS).toEqual(SETTINGS)
    expect(root.dataset.host).toBe('windows')
    expect(win.__QCALC_NATIVE).toBeUndefined()
    expect(win.__QCALC_SMOKE_ROOM).toBeUndefined()
    expect(win.__QCALC_RATES).toBeUndefined()
    expect(classes).toEqual([])
    fire('keydown', { key: 'Escape' })
    expect(sent).toEqual([])
  })

  it('reload and print never reach the web view', () => {
    const { fire } = boot(false)
    for (const e of [{ key: 'F5' }, { key: 'r', ctrlKey: true }, { key: 'P', ctrlKey: true }]) {
      let prevented = false
      fire('keydown', { ...e, preventDefault: () => (prevented = true) })
      expect(prevented).toBe(true)
    }
  })
})
