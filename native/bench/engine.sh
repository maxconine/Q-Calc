#!/bin/zsh
# The engines alone, same inputs (inputs.json, heavy.json): the Rust core (release), and the TS engine bundled by
# esbuild under node and under JavaScriptCore's jsc (the engine the web view runs).
#   zsh native/bench/engine.sh [out dir]   (default native/bench/results; writes engine-{rust,node,jsc}.json)
set -euo pipefail
HERE="${0:A:h}"
ROOT="${HERE:h:h}"
OUT="${1:-$HERE/results}"
PASSES="${PASSES:-3}"
mkdir -p "$OUT"
WORK="$(mktemp -d /tmp/qcalc-bench.XXXXXX)"
trap 'rm -rf "$WORK"' EXIT

print -r -- "{\"inputs\": $(cat "$HERE/inputs.json"), \"heavy\": $(cat "$HERE/heavy.json"), \"passes\": $PASSES}" > "$WORK/bench.json"

echo "rust…"
(cd "$ROOT/native" && cargo build -q --release -p qcalc-core --example engine_bench)
"$ROOT/native/target/release/examples/engine_bench" "$WORK/bench.json" > "$OUT/engine-rust.json"

echo "ts bundle…"
"$ROOT/node_modules/.bin/esbuild" "$HERE/engine_bench.ts" --bundle --format=iife --platform=neutral \
  --main-fields=module,main --target=es2022 --log-level=error --outfile="$WORK/engine.js"
{ print -rn -- "var BENCH = "; cat "$WORK/bench.json"; print ";"; cat "$WORK/engine.js"; } > "$WORK/run.js"
echo "node $(node --version)…"
node "$WORK/run.js" > "$OUT/engine-node.json"
JSC=/System/Library/Frameworks/JavaScriptCore.framework/Versions/Current/Helpers/jsc
if [[ -x "$JSC" ]]; then
  echo "jsc…"
  "$JSC" "$WORK/run.js" > "$OUT/engine-jsc.json"
fi
python3 "$HERE/summarize.py" engine "$OUT"
