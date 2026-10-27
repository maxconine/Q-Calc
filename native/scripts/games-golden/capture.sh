#!/bin/zsh
# Regenerates native/core/golden/games/ from the TS games: every ember level's data and walkthrough with the state
# hashes, wire text and camera along the way, trails bots move by move, and the small helpers. The Rust tests in
# native/core/tests/games_golden.rs replay them. Needs the repo's node_modules (here, or in a checkout above).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$ROOT"
VITEST="node_modules/.bin/vitest"
[[ -x "$VITEST" ]] || VITEST="$(cd "$ROOT" && while [[ "$PWD" != / && ! -x node_modules/.bin/vitest ]]; do cd ..; done; echo "$PWD/node_modules/.bin/vitest")"
# the level packs report "no test found" for the swapped describeLevel; that is expected
"$VITEST" run --config native/scripts/games-golden/vitest.config.ts --reporter=dot "$@" || true
# the engine tests' own calls
rm -rf native/core/golden/games/ember-calls
"$VITEST" run --config native/scripts/games-golden/engine.vitest.config.ts --reporter=dot "$@"
ls native/core/golden/games native/core/golden/games/ember | head -40
