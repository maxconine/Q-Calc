import { nativeHandler, nativeWindow } from './bridge'

// the mac app's web storage is wiped every launch, so game saves are kept on the swift side too. it hands them
// back as __QCALC_GAME_SAVES, they go back into local storage before any game reads them, and every later write
// to one of these keys is mirrored back. windows and the browser keep local storage themselves and get no
// __QCALC_GAME_SAVES, so nothing happens there

export const GAME_SAVE_KEYS: readonly string[] = [
  'qcalc-equal', // streak, stats, today's daily
  'qcalc-numbers', // 24 and countdown stats
  'qcalc-crack', // best level
  'qcalc-crack-muted',
  'qcalc-ember', // ember & frost progress (version 2 since ember & frost 2) and mute
  'qcalc-c4-ghost',
  'qcalc-host-locked', // the lobby's last open/locked pick
]

// per value, in characters; swift holds the same line
export const GAME_SAVE_MAX = 32 * 1024

export type GameSaveMessage = { type: 'gameSave'; key: string; value: string | null }

type StorageLike = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>
type SaveWindow = Window & { __QCALC_GAME_SAVES?: unknown }

export function isGameSaveKey(key: unknown): key is string {
  return typeof key === 'string' && GAME_SAVE_KEYS.includes(key)
}

function fits(value: unknown): value is string {
  return typeof value === 'string' && value.length <= GAME_SAVE_MAX
}

// what the native copy puts back: known keys and sane values only, and never over what the page already has,
// since after a page reload in the same run local storage is newer than the copy injected at launch
export function savesToHydrate(injected: unknown, has: (key: string) => boolean): Array<[string, string]> {
  if (!injected || typeof injected !== 'object' || Array.isArray(injected)) return []
  const out: Array<[string, string]> = []
  for (const [key, value] of Object.entries(injected as Record<string, unknown>)) {
    if (isGameSaveKey(key) && fits(value) && !has(key)) out.push([key, value])
  }
  return out
}

export function hydrateGameSaves(injected: unknown, storage: StorageLike): void {
  const has = (key: string) => {
    try {
      return storage.getItem(key) != null
    } catch {
      return false
    }
  }
  for (const [key, value] of savesToHydrate(injected, has)) {
    try {
      storage.setItem(key, value)
    } catch {
      // a rejected write: the game just starts fresh this time
    }
  }
}

// wraps setItem/removeItem on the prototype (assigning on the storage itself would store the function as an
// item), so any game's write to a known key reaches native without the game knowing. sent even if the web
// store rejects the write; too-big values aren't sent, so native keeps the last one that fit
export function mirrorGameSaves(proto: StorageLike, target: object, post: (m: GameSaveMessage) => void): void {
  const setItem = proto.setItem
  const removeItem = proto.removeItem
  const send = (key: string, value: string | null) => {
    try {
      post({ type: 'gameSave', key, value })
    } catch {
      // no native side to tell
    }
  }
  proto.setItem = function (this: StorageLike, key: string, value: string) {
    const k = String(key)
    const v = String(value)
    if (this === target && isGameSaveKey(k) && fits(v)) send(k, v)
    return setItem.call(this, key, value)
  }
  proto.removeItem = function (this: StorageLike, key: string) {
    const k = String(key)
    if (this === target && isGameSaveKey(k)) send(k, null)
    return removeItem.call(this, key)
  }
}

let installed = false

// run before anything reads a save; does nothing outside the mac app
export function installGameSaves(): void {
  if (installed) return
  const w = nativeWindow() as SaveWindow | undefined
  const handler = nativeHandler()
  if (!w || w.__QCALC_GAME_SAVES === undefined || !handler || typeof Storage === 'undefined') return
  let storage: Storage
  try {
    storage = w.localStorage
  } catch {
    return
  }
  installed = true
  hydrateGameSaves(w.__QCALC_GAME_SAVES, storage)
  mirrorGameSaves(Storage.prototype, storage, (m) => handler.postMessage(m))
}
