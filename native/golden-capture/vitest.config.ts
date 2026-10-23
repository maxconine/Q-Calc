// The golden capture run: the app's own test files plus the generated corpus, with every engine call recorded.
// `npm test` uses vite.config.ts and never loads this file or its setup.
import { fileURLToPath } from 'node:url'
import { defineConfig, mergeConfig } from 'vitest/config'
import base from '../../vite.config'

const root = fileURLToPath(new URL('../..', import.meta.url))

// games and transport keep their TS code in the on-demand web view; nothing in them is ported
const GAMES = [
  'connect4', 'crack', 'ember', 'equal', 'fourTwenty', 'games', 'gameFullscreen', 'gameSaves', 'gameShell', 'lobby',
  'numbers', 'peer', 'pong', 'sixtyNine', 'sixtySeven', 'smokeRoom', 'trails', 'ultimate', 'hello', 'nickname',
  'analytics', 'tauriShim', 'benchmark', 'emberDraw', 'emberFx',
]

export default mergeConfig(
  base,
  defineConfig({
    root,
    test: {
      include: ['src/**/*.test.ts', 'native/golden-capture/corpus/*.golden.ts'],
      exclude: ['**/node_modules/**', ...GAMES.map((g) => `src/**/${g}*.test.ts`)],
      setupFiles: ['native/golden-capture/setup.ts'],
      // the recorder runs each call twice, and the corpus is large
      testTimeout: 600_000,
      hookTimeout: 600_000,
    },
  }),
)
