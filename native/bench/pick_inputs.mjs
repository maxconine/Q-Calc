// Picks the bench's typing inputs from the golden corpus: single-line evaluateSheet calls with default options
// from every native/core/golden/eval.*.jsonl, 4 to 60 characters, spread evenly over each file, 30 per file at
// most (fewer if the file has fewer). Deterministic, so inputs.json only changes when the corpus does.
//   node native/bench/pick_inputs.mjs   (writes native/bench/inputs.json)
import { readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const golden = join(here, '../core/golden')
const PER_FILE = 30
const picked = []
for (const file of readdirSync(golden).filter((f) => /^eval\..*\.jsonl$/.test(f)).sort()) {
  const seen = new Set()
  const pool = []
  for (const line of readFileSync(join(golden, file), 'utf8').split('\n')) {
    if (!line) continue
    const rec = JSON.parse(line)
    if (rec.fn !== 'evaluateSheet') continue
    const [lines, options] = rec.args
    if (lines.length !== 1 || Object.keys(options ?? {}).length) continue
    const text = typeof lines[0] === 'string' ? lines[0] : lines[0]?.text
    if (typeof text !== 'string' || text.trim().length < 4 || text.length > 60 || seen.has(text)) continue
    seen.add(text)
    pool.push(text)
  }
  const n = Math.min(PER_FILE, pool.length)
  for (let i = 0; i < n; i++) picked.push(pool[Math.floor((i * pool.length) / n)])
}
writeFileSync(join(here, 'inputs.json'), JSON.stringify(picked, null, 0).replace(/","/g, '",\n"') + '\n')
console.log(`${picked.length} inputs, ${picked.reduce((a, s) => a + [...s].length, 0)} keystrokes`)
