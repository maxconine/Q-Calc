#!/bin/zsh
# Bundles native/mac/applogic/entry.ts (src/lib's app logic, engine imports swapped for shims) into
# native/mac/Resources/applogic.js for JavaScriptCore. Needs the repo's node_modules; the output is committed, so
# building the app doesn't need node.   ESBUILD=/path/to/esbuild zsh native/mac/applogic/build.sh
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
ESBUILD="${ESBUILD:-$ROOT/node_modules/.bin/esbuild}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# 1. the constants, from the real modules
"$ESBUILD" "$HERE/constants.ts" --bundle --platform=node --format=esm --log-level=error --outfile="$TMP/constants.mjs"
node "$TMP/constants.mjs" > "$HERE/shims/constants.json"

# 2. the bundle: every engine/<shimmed> import resolves to the shim
cat > "$TMP/plugin.mjs" <<'EOF'
import { createRequire } from 'node:module'
const [entry, shims, out, root] = process.argv.slice(2)
const { build } = createRequire(root + '/package.json')('esbuild')
const map = { scientific: 'scientific', chem: 'chem', parens: 'parens', units: 'units', plainMath: 'plainMath',
  calculus: 'analyticsKinds', matrix: 'analyticsKinds' }
await build({
  entryPoints: [entry], bundle: true, format: 'iife', platform: 'neutral', target: 'safari16', minify: true,
  outfile: out, logLevel: 'warning', mainFields: ['module', 'main'],
  define: { __APP_VERSION__: JSON.stringify(process.env.APP_VERSION ?? '0.0.0') },
  plugins: [{ name: 'engine-shims', setup(b) {
    b.onResolve({ filter: /engine\/(scientific|chem|parens|units|plainMath|calculus|matrix)$/ }, (a) => {
      const name = a.path.split('/').pop()
      return { path: `${shims}/${map[name]}.ts` }
    })
  } }],
})
EOF
NODE_PATH="$ROOT/node_modules" APP_VERSION="$(node -p "require('$ROOT/package.json').version")" \
  node "$TMP/plugin.mjs" "$HERE/entry.ts" "$HERE/shims" "$ROOT/native/mac/Resources/applogic.js" "$ROOT"
echo "Wrote native/mac/Resources/applogic.js ($(wc -c < "$ROOT/native/mac/Resources/applogic.js" | tr -d ' ') bytes)"
