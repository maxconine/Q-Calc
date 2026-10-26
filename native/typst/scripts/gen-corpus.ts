// Builds native/typst/tests/corpus.jsonl. For each preview source the web app would compile
// (typstDocument over the golden inputs), compiles it with typst.ts (the web preview's own WASM Typst) and
// records whether it compiled and the SVG's size, so the native renderer can be checked against it.
// Run from the repo root: npx vite-node native/typst/scripts/gen-corpus.ts "$PWD"
// (NODE_MODULES=<dir> when node_modules isn't in the repo root, as in a worktree)
import { readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const wt = process.argv[2]!
const nm = `${process.env.NODE_MODULES ?? join(wt, 'node_modules')}/@myriaddreamin`
const { typstDocument, typstPreviewUseful, typstAnswer, TYPST_PREAMBLE } = await import(`${wt}/src/lib/typstMath.ts`)
const { createTypstCompiler, createTypstRenderer, loadFonts } = await import(`${nm}/typst.ts/dist/esm/index.mjs`)

type Case = { expr: string; answer: string; solvedFor: string; doc: string; from: string }
const cases = new Map<string, Case>()
const add = (c: Case) => {
  if (!cases.has(c.doc)) cases.set(c.doc, c)
}

const golden = join(wt, 'native/core/golden')
for (const line of readFileSync(join(golden, 'lib.typstMath.jsonl'), 'utf8').split('\n')) {
  if (!line) continue
  const r = JSON.parse(line)
  if (r.fn === 'typstDocument' && typeof r.out === 'string') {
    const [expr, answer = '', solvedFor = ''] = r.args
    add({ expr, answer, solvedFor, doc: r.out, from: 'typstDocument' })
  } else if ((r.fn === 'toTypstMath' || r.fn === 'previewMath' || r.fn === 'typstPreviewUseful') && typeof r.args[0] === 'string') {
    const doc = typstDocument(r.args[0])
    if (doc) add({ expr: r.args[0], answer: '', solvedFor: '', doc, from: r.fn })
  }
}

// every evaluated line the preview would show, with the answer the bar would typeset beside it
const evals: Case[] = []
for (const file of readdirSync(golden).filter((f) => f.startsWith('eval.')).sort()) {
  for (const line of readFileSync(join(golden, file), 'utf8').split('\n')) {
    if (!line) continue
    const r = JSON.parse(line)
    if (r.fn !== 'evaluateLine' || typeof r.args[0] !== 'string' || !r.out || typeof r.out !== 'object') continue
    const expr = r.args[0]
    if (expr.length > 120 || !typstPreviewUseful(expr)) continue
    const answer = typstAnswer(r.out.exact, r.out.display ?? '')
    const doc = typstDocument(expr, answer)
    if (doc) evals.push({ expr, answer, solvedFor: '', doc, from: file.replace(/\.jsonl$/, '') })
  }
}
// a deterministic spread across the areas, capped so the test stays quick
const byArea = new Map<string, Case[]>()
for (const c of evals) byArea.set(c.from, [...(byArea.get(c.from) ?? []), c])
for (const list of byArea.values()) {
  const step = Math.max(1, Math.floor(list.length / 120))
  for (let i = 0; i < list.length; i += step) add(list[i]!)
}

const bytes = (p: string) => new Uint8Array(readFileSync(p))
const fonts = ['NewCMMath-Regular.otf', 'NewCM10-Regular.otf', 'NewCM10-Bold.otf'].map((f) => bytes(join(wt, 'public/typst', f)))
const compiler = createTypstCompiler()
const renderer = createTypstRenderer()
await compiler.init({
  getModule: () => bytes(`${nm}/typst-ts-web-compiler/pkg/typst_ts_web_compiler_bg.wasm`),
  beforeBuild: [loadFonts(fonts, { assets: false })],
})
await renderer.init({ getModule: () => bytes(`${nm}/typst-ts-renderer/pkg/typst_ts_renderer_bg.wasm`) })
compiler.addSource('/macros.typ', TYPST_PREAMBLE)

const out: string[] = []
let n = 0
for (const c of cases.values()) {
  const path = `/m${++n}.typ`
  compiler.addSource(path, c.doc)
  let web: { ok: boolean; width?: number; height?: number; error?: string }
  try {
    const compiled = await compiler.compile({ mainFilePath: path, root: '/', inputs: {}, diagnostics: 'full' })
    const result = compiled?.result
    if (!(result instanceof Uint8Array) || !result.byteLength) {
      web = { ok: false, error: String(compiled?.diagnostics?.[0]?.message ?? 'no artifact') }
    } else {
      const svg: string = await renderer.renderSvg({ format: 'vector', artifactContent: result })
      const m = /<svg[^>]*\bwidth="([\d.]+)(?:pt|px)?"[^>]*\bheight="([\d.]+)(?:pt|px)?"/.exec(svg)
      const vb = /viewBox="0 0 ([\d.]+) ([\d.]+)"/.exec(svg)
      web = { ok: true, width: Number(vb?.[1] ?? m?.[1]), height: Number(vb?.[2] ?? m?.[2]) }
    }
  } catch (err) {
    web = { ok: false, error: String((err as Error)?.message ?? err).slice(0, 200) }
  }
  compiler.unmapShadow(path)
  out.push(JSON.stringify({ ...c, web }))
}
writeFileSync(join(wt, 'native/typst/tests/corpus.jsonl'), out.join('\n') + '\n')
const ok = out.filter((l) => JSON.parse(l).web.ok).length
console.log(`${out.length} sources, ${ok} compile on the web`)
