// The engine-call capture (capture.sh): the ember tests run against a recording engine (engine.setup.ts).
import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vitest/config'

const root = fileURLToPath(new URL('../../..', import.meta.url))

export default defineConfig({
  root,
  test: {
    include: ['src/lib/emberEngine.test.ts', 'src/lib/emberProtocol.test.ts', 'src/lib/emberCamera.test.ts'],
    setupFiles: ['native/scripts/games-golden/engine.setup.ts'],
    testTimeout: 600_000,
  },
})
