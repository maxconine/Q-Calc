// Merges the recorder's raw per-worker files into native/core/golden/<area>.jsonl, deduplicated and sorted.
// Usage: node native/golden-capture/merge.mjs [rawDir] [outDir]
import { mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const root = new URL('../..', import.meta.url).pathname
const rawDir = process.argv[2] ?? join(root, 'native/core/golden/.raw')
const outDir = process.argv[3] ?? join(root, 'native/core/golden')

// modules whose answers depend on the clock: their records keep the time they were taken at
const CLOCKED = new Set(['engine/phrases', 'engine/phraseTime', 'engine/phraseMoney', 'lib/history', 'lib/historyShow', 'lib/draft', 'lib/onboarding', 'lib/steady', 'lib/undo'])

const hasLetters = (s) =>
  /[A-Za-zÀ-ʯͰ-Ͽ]/.test(s.replace(/0x[0-9a-f]+|0b[01]+|0o[0-7]+|(?<=\d)[eE][-+]?(?=\d)/gi, ''))

/** The area an evaluate call belongs to: which part of the engine answered it. */
function evalArea(rec) {
  const [lines, opts = {}] = rec.args
  const texts = (Array.isArray(lines) ? lines : [lines]).map((l) => (typeof l === 'string' ? l : (l?.text ?? '')))
  const t = new Set(rec.trace ?? [])
  const has = (...names) => names.some((n) => t.has(n))
  if (texts.length > 1 || ['variables', 'functions', 'ans', 'quantities', 'measures'].some((k) => k in opts)) return 'eval.sheet'
  const text = texts[0] ?? ''
  if (has('engine/calculus.evaluateCalculus', 'engine/calculus.embedIntegrals')) return 'eval.calculus'
  if (has('engine/system.parseSystemCall', 'engine/system.solveSystemCall')) return 'eval.system'
  if (has('engine/isolate.isolateVariable', 'engine/isolate.parseNamedSolve', 'engine/solve.solveEquation', 'engine/unitSolve.solveWithUnitsOne', 'engine/unitSolve.isolateWithUnits')) return 'eval.solve'
  if (has('engine/matrix.matrixAnswer')) return 'eval.matrix'
  if (has('engine/chem.chemAnswer', 'engine/chem.molarMassAnswer', 'engine/chem.gasConstant')) return 'eval.chem'
  if (has('engine/sums.sumAnswer')) return 'eval.sums'
  if (opts.sigFigMode || /±|∓|\+-|\\pm/.test(text) || has('engine/measure.measure')) return 'eval.measure'
  if (has('engine/units.tryConvert')) return 'eval.units'
  if (/\\[A-Za-z]/.test(text) || has('engine/plainMath.latexToAscii') || /^\s*\$[^$]+\$\s*$/.test(text)) return 'eval.latex'
  // lists and ranges (`[1...5]`, `[1,2,3] * 2`) are matrices and list functions, not plain arithmetic
  if (/\.\.\./.test(text)) return 'eval.functions'
  if (/\[/.test(text)) return 'eval.matrix'
  if (hasLetters(text)) return has('engine/scientific.evalScientific') ? 'eval.functions' : 'eval.blank'
  return 'eval.arithmetic'
}

function areaOf(rec) {
  if (rec.module === 'engine/evaluate' && (rec.fn === 'evaluateLine' || rec.fn === 'evaluateSheet')) return evalArea(rec)
  return rec.module.replace('/', '.')
}

const byKey = new Map()
let raw = 0
let files = []
try {
  files = readdirSync(rawDir).filter((f) => f.endsWith('.jsonl'))
} catch {
  console.error(`no raw capture in ${rawDir}; run native/golden-capture/capture.sh`)
  process.exit(1)
}
for (const f of files.sort()) {
  for (const line of readFileSync(join(rawDir, f), 'utf8').split('\n')) {
    if (!line) continue
    raw++
    const rec = JSON.parse(line)
    const key = `${rec.module}\u0000${rec.fn}\u0000${JSON.stringify(rec.args)}`
    const result = JSON.stringify(rec.throws != null ? { throws: rec.throws } : rec.out)
    const seen = byKey.get(key)
    if (seen) {
      // the same call answered two ways in two places: something besides its arguments decides it
      if (seen.result !== result) seen.rec.nondet = true
      continue
    }
    byKey.set(key, { rec, result })
  }
}

const areas = new Map()
for (const { rec } of byKey.values()) {
  const area = areaOf(rec)
  const out = { fn: rec.fn, args: rec.args }
  if (rec.throws != null) out.throws = rec.throws
  else out.out = rec.out
  if (rec.nondet) out.nondet = true
  if (CLOCKED.has(rec.module) || rec.nondet) out.now = rec.now
  out.src = rec.src
  if (!areas.has(area)) areas.set(area, [])
  areas.get(area).push(out)
}

mkdirSync(outDir, { recursive: true })
for (const f of readdirSync(outDir)) if (f.endsWith('.jsonl')) rmSync(join(outDir, f))
const summary = {}
for (const [area, recs] of [...areas].sort(([a], [b]) => a.localeCompare(b))) {
  const lines = recs.map((r) => JSON.stringify(r)).sort()
  writeFileSync(join(outDir, `${area}.jsonl`), `${lines.join('\n')}\n`)
  summary[area] = { records: recs.length, nondet: recs.filter((r) => r.nondet).length }
}
writeFileSync(join(outDir, 'summary.json'), `${JSON.stringify({ rawCalls: raw, unique: byKey.size, areas: summary }, null, 2)}\n`)
console.log(`${raw} recorded calls, ${byKey.size} unique, ${areas.size} areas`)
for (const [area, s] of Object.entries(summary)) console.log(`  ${area.padEnd(28)} ${String(s.records).padStart(7)}${s.nondet ? `  (${s.nondet} nondet)` : ''}`)
