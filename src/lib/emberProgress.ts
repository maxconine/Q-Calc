import type { Score } from './emberTypes'

// ember & frost's saved progress: the best of every finished level, by level id, and the mute switch.
// pure parse/merge here; load/save wrap localStorage since the mac app's store can reject writes

export type LevelBest = {
  completed: boolean
  stars: number // best stars, 0..3
  secs: number | null // best time; null until completed
  gems: number // most gems (both colours) in one finish
}

export type EmberProgress = {
  levels: Record<string, LevelBest>
  muted: boolean
}

const KEY = 'qcalc-ember'
const MAX_STARS = 3

export function emptyProgress(): EmberProgress {
  return { levels: {}, muted: false }
}

const count = (v: unknown, max = Infinity): number =>
  typeof v === 'number' && Number.isFinite(v) && v > 0 ? Math.min(max, Math.floor(v)) : 0

// whatever was stored, made safe: unknown fields dropped, bad numbers zeroed, a level that was never
// finished but claims stars or a time is cut back to nothing
export function sanitizeProgress(raw: unknown): EmberProgress {
  const out = emptyProgress()
  if (!raw || typeof raw !== 'object') return out
  const o = raw as Record<string, unknown>
  out.muted = o.muted === true
  const levels = o.levels
  if (!levels || typeof levels !== 'object' || Array.isArray(levels)) return out
  for (const [id, v] of Object.entries(levels as Record<string, unknown>)) {
    if (!id || !v || typeof v !== 'object') continue
    const l = v as Record<string, unknown>
    if (l.completed !== true) continue
    const secs = typeof l.secs === 'number' && Number.isFinite(l.secs) && l.secs > 0 ? l.secs : null
    out.levels[id] = { completed: true, stars: count(l.stars, MAX_STARS), secs, gems: count(l.gems) }
  }
  return out
}

export function parseProgress(text: string | null): EmberProgress {
  if (!text) return emptyProgress()
  try {
    return sanitizeProgress(JSON.parse(text))
  } catch {
    return emptyProgress()
  }
}

// a finished level: keep the best of each field separately (the fastest run needn't be the gem run)
export function recordResult(progress: EmberProgress, levelId: string, score: Score): EmberProgress {
  const old = progress.levels[levelId]
  const secs = Number.isFinite(score.secs) && score.secs > 0 ? score.secs : null
  const gems = count(score.gems[0]) + count(score.gems[1])
  const best: LevelBest = {
    completed: true,
    stars: Math.max(old?.stars ?? 0, count(score.stars, MAX_STARS)),
    secs: old?.secs != null && (secs == null || old.secs <= secs) ? old.secs : secs,
    gems: Math.max(old?.gems ?? 0, gems),
  }
  return { ...progress, levels: { ...progress.levels, [levelId]: best } }
}

// how many leading levels can be picked: the first always, and each completed one opens the next
export function unlockedCount(progress: EmberProgress, levels: ReadonlyArray<{ id: string }>): number {
  if (!levels.length) return 0
  let n = 1
  while (n < levels.length && progress.levels[levels[n - 1]!.id]?.completed) n++
  return n
}

export function loadProgress(): EmberProgress {
  try {
    return parseProgress(localStorage.getItem(KEY))
  } catch {
    return emptyProgress()
  }
}

export function saveProgress(progress: EmberProgress): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(progress))
  } catch {
    // the mac app's non-persistent web store can reject writes; this run still remembers
  }
}
