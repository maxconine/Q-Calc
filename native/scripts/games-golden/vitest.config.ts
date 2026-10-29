// The games capture (capture.sh): the ember level packs' tests with describeLevel swapped for a recorder, and
// extras.capture.ts. `npm test` uses vite.config.ts and never loads this file or its setup.
import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vitest/config'

const root = fileURLToPath(new URL('../../..', import.meta.url))

export default defineConfig({
  root,
  test: {
    include: ['src/lib/emberLevels/*.test.ts', 'native/scripts/games-golden/*.capture.ts'],
    setupFiles: ['native/scripts/games-golden/setup.ts'],
    testTimeout: 600_000,
    hookTimeout: 600_000,
  },
})
