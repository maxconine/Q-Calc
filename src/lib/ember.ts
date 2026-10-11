// opening ember & frost from the bar
export const EMBER_HINT = '↵ play ember & frost'

// 'ember', 'ember & frost', 'ember and frost', 'frost & ember', 'fire ice', 'fire & ice', 'fire and ice'.
// whole names only, so 'ember2', 'fire' or 'emberandfrost' stay calculator input. symbols may hug the
// words; 'and' needs its spaces
const AND = '(?: ?[&+] ?| and )'
const EMBER_RE = new RegExp(`^(?:ember(?:${AND}frost)?|frost${AND}ember|fire(?:${AND}| )ice)$`, 'i')

export function isEmberCommand(text: string): boolean {
  return EMBER_RE.test(text.trim().replace(/\s+/g, ' '))
}

// ——— small sums the panel shows

// the running clock and best times: 0:07.4, 2:10.0
export function fmtTime(secs: number): string {
  // by tenths first, so 59.96 reads 1:00.0 and not 0:60.0
  const tenths = Math.round(Math.max(0, Number.isFinite(secs) ? secs : 0) * 10)
  const m = Math.floor(tenths / 600)
  const rest = (tenths - m * 600) / 10
  return `${m}:${rest < 10 ? '0' : ''}${rest.toFixed(1)}`
}

// a target time, which is always whole seconds: 0:45, 2:10
export function fmtPar(secs: number): string {
  const s = Math.round(Math.max(0, Number.isFinite(secs) ? secs : 0))
  const rest = s % 60
  return `${Math.floor(s / 60)}:${rest < 10 ? '0' : ''}${rest}`
}

export const MAX_DIFFICULTY = 5

// levels run easiest first, so a level's difficulty mark is where it stands in the list: 1 for the first,
// MAX_DIFFICULTY for the last, spread evenly between
export function levelDifficulty(index: number, count: number): number {
  if (!(count > 1) || !(index > 0)) return 1
  const at = Math.min(index, count - 1) / (count - 1)
  return 1 + Math.round(at * (MAX_DIFFICULTY - 1))
}

type Sized = { stages: ReadonlyArray<{ map: ReadonlyArray<string> }> }

// how much map a level has, in screens of 32 × 18 tiles over all its stages, to the nearest half
export function levelScreens(def: Sized, viewCols = 32, viewRows = 18): number {
  let n = 0
  for (const st of def.stages) n += ((st.map[0]?.length ?? 0) * st.map.length) / (viewCols * viewRows)
  return Math.max(1, Math.round(n * 2) / 2)
}

// '1 stage · 1 screen', '3 stages · 8 screens'
export function levelLength(def: Sized): string {
  const stages = def.stages.length
  const screens = levelScreens(def)
  return `${stages} ${stages === 1 ? 'stage' : 'stages'} · ${screens} ${screens === 1 ? 'screen' : 'screens'}`
}
