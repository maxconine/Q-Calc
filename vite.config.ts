import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

const root = dirname(fileURLToPath(import.meta.url))
const { version } = JSON.parse(readFileSync(resolve(root, 'package.json'), 'utf8')) as { version: string }

export default defineConfig({
  plugins: [react()],
  base: './',
  define: { __APP_VERSION__: JSON.stringify(version) },
  server: { host: '127.0.0.1' },
  build: {
    rollupOptions: {
      input: {
        index: resolve(root, 'index.html'),
        quick: resolve(root, 'quick.html'),
        settings: resolve(root, 'settings.html'),
      },
    },
  },
})
