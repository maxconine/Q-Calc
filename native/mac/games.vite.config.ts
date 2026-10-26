// The native Mac app's only web code: games.html (src/games.tsx) and what it imports, built on its own so none of
// the calculator's pages, chunks or the typst.ts WASM come along. native/mac/build.sh runs it:
//   node_modules/.bin/vite build --config native/mac/games.vite.config.ts
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..')
const { version } = JSON.parse(readFileSync(resolve(root, 'package.json'), 'utf8')) as { version: string }

export default defineConfig({
  root,
  plugins: [react()],
  base: './',
  publicDir: false,
  define: { __APP_VERSION__: JSON.stringify(version) },
  logLevel: 'warn',
  build: {
    outDir: process.env.QCALC_GAMES_OUT ?? resolve(root, 'native/build/games-web'),
    emptyOutDir: true,
    rollupOptions: { input: { games: resolve(root, 'games.html') } },
  },
})
