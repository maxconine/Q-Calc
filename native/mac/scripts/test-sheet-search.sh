#!/bin/bash
# The sheets' search: recall over Tests/sheetsearch/queries.tsv (each query's row in its first 3 results), built from
# macos/SheetSearch.swift with Tests/sheetsearch/main.swift into one command-line binary. No app, no window.
#   native/mac/scripts/test-sheet-search.sh [--all]
# With --app, also times typing those queries into a running dev build's sheet (identities, then dynamics), each
# keystroke from the text changing to the sheet laid out and drawn (`sheetbench:`).
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
arch="${ARCH:-$(uname -m)}"
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
xcrun swiftc -O -target "$arch-apple-macos12.0" \
  "$here/../../macos/SheetSearch.swift" \
  "$here/Tests/sheetsearch/main.swift" \
  -o "$out/sheet-search"
status=0
"$out/sheet-search" "$here/../core/src/app" "$here/Tests/sheetsearch/queries.tsv" "${@/--app/}" || status=$?
if [[ " $* " == *" --app "* ]]; then
  grep -v '^#' "$here/Tests/sheetsearch/queries.tsv" | cut -f1 > "$out/q.txt"
  for sheet in identities dynamics; do
    # identities opens its sheet by the dev command; another by its name on the bar
    if [[ $sheet == identities ]]; then open=(identities); else open=("type:$sheet" sleep:0.8 enter); fi
    zsh "$here/scripts/drive.sh" show "${open[@]}" sleep:2.5 "sheetbench:$out/q.txt|$out/$sheet.json"
    for _ in {1..240}; do [[ -f "$out/$sheet.json" ]] && break; sleep 0.5; done
    [[ -f "$out/$sheet.json" ]] || { echo "$sheet: no sheet opened"; continue; }
    python3 - "$out/$sheet.json" "$sheet" <<'EOF'
import json, sys
ms = sorted(json.load(open(sys.argv[1]))["ms"])
q = lambda p: ms[min(len(ms) - 1, int(len(ms) * p))]
print(f"{sys.argv[2]}: {len(ms)} keystrokes, median {q(.5):.1f} ms, p90 {q(.9):.1f}, p99 {q(.99):.1f}, max {ms[-1]:.1f}")
EOF
    zsh "$here/scripts/drive.sh" sheetkey:cmd+w
  done
fi
exit $status
