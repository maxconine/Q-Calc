import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

const root = dirname(fileURLToPath(import.meta.url))

export default defineConfig({
  plugins: [react()],
  base: './',
  server: { host: '127.0.0.1' },
  build: {
    rollupOptions: {
      input: {
        index: resolve(root, 'index.html'),
        quick: resolve(root, 'quick.html'),
      },
    },
  },
})
