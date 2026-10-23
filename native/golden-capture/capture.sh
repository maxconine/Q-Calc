#!/bin/zsh
# Regenerates native/core/golden/*.jsonl from the TS engine: runs the app's tests and the generated corpus with
# every engine call recorded, then merges. Takes a few minutes. The app's own `npm test` is untouched by this.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
RAW="$ROOT/native/core/golden/.raw"
rm -rf "$RAW"
cd "$ROOT"
# test failures don't stop the capture: a failing assertion still recorded what the engine said
npx vitest run --config native/golden-capture/vitest.config.ts --reporter=dot "$@" || true
node native/golden-capture/merge.mjs "$RAW" "$ROOT/native/core/golden"
rm -rf "$RAW"
