#!/bin/zsh
# Regenerates native/mac/Resources/sheets.json from src/lib/elements.ts and src/lib/identities.ts.
# Needs the repo's node_modules (npm install). The JSON is committed, so building the app doesn't need node.
#   ESBUILD=/path/to/esbuild zsh native/mac/scripts/export-sheets.sh
set -euo pipefail
MAC="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$MAC/../.." && pwd)"
ESBUILD="${ESBUILD:-$ROOT/node_modules/.bin/esbuild}"
OUT="$MAC/Resources/sheets.json"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
"$ESBUILD" "$MAC/scripts/export-sheets.ts" --bundle --platform=node --format=esm --log-level=warning \
  --outfile="$TMP/export.mjs"
mkdir -p "$MAC/Resources"
node "$TMP/export.mjs" > "$OUT"
echo "Wrote $OUT ($(wc -c < "$OUT" | tr -d ' ') bytes)"
