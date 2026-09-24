import { createTypstCompiler, createTypstRenderer, loadFonts, type TypstCompiler, type TypstRenderer } from '@myriaddreamin/typst.ts'
import { TYPST_PREAMBLE } from './typstMath'

const FONT_FILES = ['NewCMMath-Regular.otf', 'NewCM10-Regular.otf', 'NewCM10-Bold.otf']

type Engine = {
  compiler: TypstCompiler
  renderer: TypstRenderer
}

let ready: Promise<Engine> | null = null
const svgCache = new Map<string, string>()

class StaleRender extends Error {}

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
  if (!ready) {
    ready = (async () => {
      const [compilerWasm, rendererWasm] = await Promise.all([
        import('@myriaddreamin/typst-ts-web-compiler/wasm?url'),
        import('@myriaddreamin/typst-ts-renderer/wasm?url'),
      ])
      const [compilerBytes, rendererBytes, ...fonts] = await Promise.all([
        readBytes(compilerWasm.default),
        readBytes(rendererWasm.default),
        ...FONT_FILES.map((file) => readBytes(fontUrl(file))),
      ])
      const compiler = createTypstCompiler()
      const renderer = createTypstRenderer()
      await Promise.all([
        compiler.init({
          getModule: () => compilerBytes,
          beforeBuild: [loadFonts(fonts, { assets: false })],
        }),
        renderer.init({ getModule: () => rendererBytes }),
      ])
      compiler.addSource('/macros.typ', TYPST_PREAMBLE)
      return { compiler, renderer }
    })().catch((err: unknown) => {
      ready = null
      throw err
    })
  }
  return ready
}

let docId = 0

async function compileSvg(mainContent: string): Promise<string> {
  const { compiler, renderer } = await engine()
  // addSource keeps the first body for a path, so each edit is a new file.
  // The macros file stays put and is not parsed again.
  const mainPath = `/m${++docId}.typ`
  await compiler.reset()
  compiler.addSource('/macros.typ', TYPST_PREAMBLE)
  compiler.addSource(mainPath, mainContent)
  const compiled = await compiler.compile({
    mainFilePath: mainPath,
    root: '/',
    inputs: {},
    diagnostics: 'none',
  })
  const svg = await renderer.renderSvg({
    format: 'vector',
    artifactContent: artifactBytes(compiled),
    data_selection: { body: true, defs: true, css: true, js: false },
  })
  return svg.replace(/<script[\s\S]*?<\/script>/gi, '')
}

let pending: { src: string; resolve: (svg: string) => void; reject: (err: unknown) => void } | null = null
let running = false

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
          if (svgCache.size > 32) {
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

/** Load the compiler and macros before the first keystroke. */
export function warmupTypst(): void {
  void engine().catch(() => {})
}

export function isStaleRender(err: unknown): boolean {
  return err instanceof StaleRender
}

export function renderTypstSvg(mainContent: string): Promise<string> {
  const cached = svgCache.get(mainContent)
  if (cached) return Promise.resolve(cached)
  return new Promise((resolve, reject) => {
    if (pending) pending.reject(new StaleRender('superseded'))
    pending = { src: mainContent, resolve, reject }
    void pump()
  })
}
