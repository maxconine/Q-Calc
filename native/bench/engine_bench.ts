// The TS engine alone, on the inputs engine_bench.rs times in the Rust core. engine.sh bundles this with esbuild and
// runs it under node and JavaScriptCore's `jsc`, with `var BENCH = {inputs, heavy, passes}` prepended.
//   keystrokes: evaluateSheet([prefix]) for every prefix of every input; pass 1 is the warm-up, the rest are timed
//   heavy:      each case once cold (its first call) and then `passes * 3` times warm
import { evaluateSheet } from '../../src/engine/evaluate'
import { buildGraph } from '../../src/engine/graph'

import { identifyJob, type ClosedFormJob } from '../../src/engine/closedForm'

type Heavy = { name: string; kind: 'sheet' | 'graph' | 'closedForm'; lines?: string[]; input?: string; job?: ClosedFormJob }
declare const BENCH: { inputs: string[]; heavy: Heavy[]; passes: number }
declare const print: ((s: string) => void) | undefined
declare const preciseTime: (() => number) | undefined

const g = globalThis as { performance?: { now(): number } }
const now = g.performance ? () => g.performance!.now() : () => preciseTime!() * 1000
const say = (s: string) => (typeof print === 'function' ? print(s) : console.log(s))

const t0 = now()
evaluateSheet(['1+1'])
const firstCall = now() - t0

const keys: number[] = []
let keyTotal = 0
let firstPass = 0
for (let pass = 0; pass <= BENCH.passes; pass++) {
  for (const input of BENCH.inputs) {
    const chars = Array.from(input)
    for (let k = 1; k <= chars.length; k++) {
      const text = chars.slice(0, k).join('')
      const s = now()
      evaluateSheet([text], {})
      const ms = now() - s
      if (pass === 0) firstPass += ms
      if (pass > 0) {
        keys.push(ms)
        keyTotal += ms
      }
    }
  }
}

// closedForm: the PSLQ search the bar runs after a definite integral's numeric answer
const run = (h: Heavy) =>
  h.kind === 'closedForm' ? identifyJob(h.job!) : h.kind === 'graph' ? buildGraph(h.input!, {}) : evaluateSheet(h.lines!, {})
// what each case answered (the last line's display, or the graph's point count), so a fast case is a real answer
const answer = (h: Heavy) => {
  if (h.kind === 'closedForm') return identifyJob(h.job!) ?? 'none'
  if (h.kind === 'graph') return `${buildGraph(h.input!, {})?.points.length ?? 0} points`
  const lines = evaluateSheet(h.lines!, {})
  return lines[lines.length - 1].display
}
const heavy = BENCH.heavy.map((h) => {
  let s = now()
  run(h)
  const cold = now() - s
  const warm: number[] = []
  for (let i = 0; i < BENCH.passes * 3; i++) {
    s = now()
    run(h)
    warm.push(now() - s)
  }
  return { name: h.name, cold, warm, answer: answer(h) }
})

say(JSON.stringify({ firstCall, firstPass, keys, keyTotal: keyTotal / BENCH.passes, heavy }))
