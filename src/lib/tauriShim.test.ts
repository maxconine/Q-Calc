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
    addEventListener: (type: string, f: Listener) => (listeners[type] ??= []).push(f),
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
  const boot = { overlay, settings: SETTINGS, onboarding: { opens: 2, commits: 0, hints: 16, done: false }, rates: overlay ? { base: 'EUR', rates: { USD: 1.1 } } : null }
  const source = shim.replace('__QCALC_BOOT__', JSON.stringify(boot))
  new Function('window', 'document', 'CustomEvent', 'MutationObserver', source)(win, doc, function () {}, FakeObserver)
  const fire = (type: string, e: Record<string, unknown>) => {
    let stopped = false
    for (const f of listeners[type] ?? []) f({ preventDefault() {}, stopPropagation: () => (stopped = true), ...e })
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
    expect(fire('keydown', { key: ',', ctrlKey: true, altKey: false, shiftKey: false, metaKey: false })).toBe(true)
    expect(sent).toEqual([['host', { type: 'openSettings' }]])
    addRoot()
    expect(root.dataset.host).toBe('windows')
    expect(root.dataset.theme).toBe('dark')
    expect(classes).toContain('quick-native')
  })

  it('esc hides, ctrl+comma opens settings', () => {
    const { sent, fire } = boot(true)
    expect(fire('keydown', { key: 'Escape' })).toBe(true)
    expect(fire('keydown', { key: ',', ctrlKey: true, altKey: false, shiftKey: false, metaKey: false })).toBe(true)
    expect(sent).toEqual([
      ['host', { type: 'dismiss' }],
      ['host', { type: 'openSettings' }],
    ])
  })

  it('a double click on the top edge recentres; one lower down does not', () => {
    const { sent, fire } = boot(true)
    const target = { closest: () => null }
    fire('mousedown', { detail: 2, button: 0, clientX: 300, clientY: 40, target })
    fire('mousedown', { detail: 1, button: 0, clientX: 300, clientY: 4, target })
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
