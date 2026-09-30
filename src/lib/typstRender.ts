import { createTypstCompiler, createTypstRenderer, loadFonts, type TypstCompiler, type TypstRenderer } from '@myriaddreamin/typst.ts'
import { TYPST_PREAMBLE } from './typstMath'

const FONT_FILES = ['NewCMMath-Regular.otf', 'NewCM10-Regular.otf', 'NewCM10-Bold.otf']
// a failed load (missing file, out of memory) waits this long before the ~30 MB is fetched again
const RETRY_MS = 10_000
// typst keeps a memo cache per compile; it is cleared now and then so a long session stays small
const RESET_EVERY = 200
const CACHE_SIZE = 48

type Engine = {
  compiler: TypstCompiler
  renderer: TypstRenderer
}

let ready: Promise<Engine> | null = null
let failedAt = 0
const svgCache = new Map<string, string>()

class StaleRender extends Error {}
class EngineUnavailable extends Error {}

function fontUrl(file: string): string {
  return new URL(`typst/${file}`, document.baseURI).href
}

// A file:// response has status 0. Passing that Response to WebAssembly.instantiateStreaming
// throws, and the preview never appears. Bytes instantiate either way.
async function readBytes(url: string): Promise<Uint8Array> {
  const response = await fetch(url)
  if (!response.ok && response.status !== 0) throw new Error(`Could not load ${url}`)
  const bytes = new Uint8Array(await response.arrayBuffer())
  if (!bytes.byteLength) throw new Error(`Could not load ${url}`)
  return bytes
}

function artifactBytes(compiled: unknown): Uint8Array {
  if (compiled instanceof Uint8Array) return compiled
  if (compiled && typeof compiled === 'object' && 'result' in compiled) {
    const result = (compiled as { result?: Uint8Array }).result
    if (result instanceof Uint8Array && result.byteLength) return result
  }
  throw new Error('typst compile failed')
}

function engine(): Promise<Engine> {
  if (ready) return ready
  if (failedAt && Date.now() - failedAt < RETRY_MS) return Promise.reject(new EngineUnavailable('typst is not loaded'))
  ready = (async () => {
    const [compilerWasm, rendererWasm] = await Promise.all([
      import('@myriaddreamin/typst-ts-web-compiler/wasm?url'),
      import('@myriaddreamin/typst-ts-renderer/wasm?url'),
    ])
    let [compilerBytes, rendererBytes, ...fonts]: (Uint8Array | null)[] = await Promise.all([
      readBytes(compilerWasm.default),
      readBytes(rendererWasm.default),
      ...FONT_FILES.map((file) => readBytes(fontUrl(file))),
    ])
    const compiler = createTypstCompiler()
    const renderer = createTypstRenderer()
    await Promise.all([
      compiler.init({
        getModule: () => compilerBytes!,
        beforeBuild: [loadFonts(fonts as Uint8Array[], { assets: false })],
      }),
      renderer.init({ getModule: () => rendererBytes! }),
    ])
    // the modules are instantiated; the ~30 MB of wasm bytes can go
    compilerBytes = null
    rendererBytes = null
    fonts = []
    compiler.addSource('/macros.typ', TYPST_PREAMBLE)
    failedAt = 0
    return { compiler, renderer }
  })().catch((err: unknown) => {
    ready = null
    failedAt = Date.now()
    throw err
  })
  return ready
}

let docId = 0
let lastPath = ''

// The renderer's svg carries a global stylesheet (with `svg { fill: none }`, which blanks other
// icons on the page), and a transparent, position: fixed text layer per glyph for selection.
// The preview needs neither. Black becomes currentColor, so the page's text color applies.
export function cleanSvg(svg: string): string {
  return svg
    .replace(/<script[\s\S]*?<\/script>/gi, '')
    .replace(/<style[\s\S]*?<\/style>/gi, '')
    .replace(/<foreignObject[\s\S]*?<\/foreignObject>/gi, '')
    .replace(/<g transform="[^"]*">\s*<\/g>/g, '')
    .replace(/ data-tid="[^"]*"/g, '')
    .replace(/(fill|stroke)="(?:#000|#000000|black)"/gi, '$1="currentColor"')
}

async function compileSvg(mainContent: string): Promise<string> {
  const { compiler, renderer } = await engine()
  // addSource keeps the first body for a path, so each edit is a new file and the last one is dropped.
  // The macros stay parsed between edits; the cache is only reset now and then.
  if (docId % RESET_EVERY === RESET_EVERY - 1) {
    await compiler.reset()
    compiler.resetShadow()
    compiler.addSource('/macros.typ', TYPST_PREAMBLE)
    lastPath = ''
  }
  const mainPath = `/m${++docId}.typ`
  compiler.addSource(mainPath, mainContent)
  if (lastPath) compiler.unmapShadow(lastPath)
  lastPath = mainPath
  const compiled = await compiler.compile({
    mainFilePath: mainPath,
    root: '/',
    inputs: {},
    diagnostics: 'none',
  })
  const svg = await renderer.renderSvg({
    format: 'vector',
    artifactContent: artifactBytes(compiled),
    data_selection: { body: true, defs: true, css: false, js: false },
  })
  return cleanSvg(svg)
}

let pending: { src: string; resolve: (svg: string) => void; reject: (err: unknown) => void } | null = null
let running = false

// One compile at a time, and only the newest waiting source: while typing fast, the edits that
// were overtaken are never compiled.
async function pump(): Promise<void> {
  if (running) return
  running = true
  try {
    while (pending) {
      const job = pending
      pending = null
      try {
        const cached = svgCache.get(job.src)
        const svg = cached ?? (await compileSvg(job.src))
        if (!cached) {
          svgCache.set(job.src, svg)
          if (svgCache.size > CACHE_SIZE) {
            const oldest = svgCache.keys().next().value
            if (oldest) svgCache.delete(oldest)
          }
        }
        job.resolve(svg)
      } catch (err) {
        job.reject(err)
      }
    }
  } finally {
    running = false
  }
}

let warm = false

/**
 * Load the compiler and fonts once the window has settled, and typeset one formula so the first
 * keystroke doesn't pay for font shaping.
 */
export function warmupTypst(): void {
  if (warm) return
  warm = true
  setTimeout(() => {
    void engine()
      .then(() => renderTypstSvg('#set page(width: auto, height: auto)\n$ x^2 $'))
      .catch(() => {
        warm = false
      })
  }, 400)
}

export function isStaleRender(err: unknown): boolean {
  return err instanceof StaleRender
}

export function renderTypstSvg(mainContent: string): Promise<string> {
  const cached = svgCache.get(mainContent)
  if (cached) {
    // a newer edit replaced by a cached one isn't wanted any more
    if (pending) {
      pending.reject(new StaleRender('superseded'))
      pending = null
    }
    return Promise.resolve(cached)
  }
  return new Promise((resolve, reject) => {
    if (pending) pending.reject(new StaleRender('superseded'))
    pending = { src: mainContent, resolve, reject }
    void pump()
  })
}
