import { nativeHandler, nativeWindow } from './bridge'

// the name a room list shows for you: a random one like "teal otter", picked once and kept. never the
// computer's own name. the mac app's web storage is wiped every launch, so it keeps the name itself and hands
// it back as __QCALC_NICKNAME; everywhere else it lives in local storage

const KEY = 'qcalc-nickname'
export const NICKNAME_MAX = 24

const COLOURS = [
  'teal', 'amber', 'coral', 'indigo', 'olive', 'plum', 'rust', 'sage', 'slate', 'ochre', 'cobalt', 'crimson',
  'jade', 'lilac', 'mint', 'navy', 'peach', 'ruby', 'sandy', 'silver', 'golden', 'russet', 'violet', 'azure',
]
const ANIMALS = [
  'otter', 'heron', 'fox', 'badger', 'lynx', 'owl', 'panda', 'gecko', 'koala', 'marten', 'newt', 'orca',
  'puffin', 'quail', 'raven', 'seal', 'tapir', 'wombat', 'yak', 'zebra', 'bison', 'crane', 'dingo', 'ferret',
]

export function randomNickname(rand: () => number = Math.random): string {
  const pick = (list: readonly string[]) => list[Math.floor(rand() * list.length) % list.length]!
  return `${pick(COLOURS)} ${pick(ANIMALS)}`
}

// letters, digits, spaces and a little punctuation, single-spaced, at most NICKNAME_MAX; '' when nothing's left
export function cleanNickname(raw: unknown): string {
  if (typeof raw !== 'string') return ''
  return raw
    .normalize('NFC')
    .replace(/[^\p{L}\p{N} '’.-]/gu, '')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, NICKNAME_MAX)
    .trim()
}

type NickWindow = Window & { __QCALC_NICKNAME?: unknown }

let current: string | null = null

export function loadNickname(): string {
  if (current) return current
  const w = nativeWindow() as NickWindow | undefined
  let stored = cleanNickname(w?.__QCALC_NICKNAME)
  if (!stored) {
    try {
      stored = cleanNickname(localStorage.getItem(KEY))
    } catch {
      stored = ''
    }
  }
  current = stored || randomNickname()
  // a fresh one is kept straight away, so it's the same next time
  if (!stored) saveNickname(current)
  return current
}

export function saveNickname(raw: string): string {
  const name = cleanNickname(raw) || loadNickname()
  current = name
  try {
    localStorage.setItem(KEY, name)
  } catch {
    // private window: kept for this session only
  }
  nativeHandler()?.postMessage({ type: 'nickname', name })
  return name
}
