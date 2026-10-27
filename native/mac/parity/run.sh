#!/bin/zsh
# Paired pictures for native/mac/PARITY.md: each scenario rendered by the current app's own page (dist/quick.html
# in WebReference, with the web-view app's boot flags) and by the running native dev build (its in-app
# snapshots), in light and dark, into native/mac/parity-screenshots/<scenario>-<theme>-{current,native}.png.
#
#   npm run build && zsh native/mac/build.sh --run && zsh native/mac/parity/run.sh [scenario…]
set -euo pipefail
MAC="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$MAC/../.." && pwd)"
OUT="$MAC/parity-screenshots"
TOOLS="${TMPDIR:-/tmp}/qcalc-parity"
mkdir -p "$OUT" "$TOOLS"
xcrun swiftc -parse-as-library -O -module-cache-path "$TOOLS/mc" "$MAC/parity/WebReference.swift" -o "$TOOLS/webref"
DRIVE=(zsh "$MAC/scripts/drive.sh")

CARD='[{"id":"p1","from":"max","text":"lunch at noon? bring the 4.2 m tape","ask":false},{"id":"p2","from":"sam","text":"","ask":true}]'
typeset -A WEB NATIVE
WEB[live]="insert:2+3"
NATIVE[live]="type:2+3"
WEB[dual]="insert:sqrt(8)"
NATIVE[dual]="type:sqrt(8)"
WEB[history]="insert:x = 4|key:Enter|wait:250|insert:2^10|key:Enter|wait:250|insert:sqrt(8)|key:Enter|wait:250|insert:*3"
NATIVE[history]="type:x = 4|key:enter|sleep:0.3|type:2^10|key:enter|sleep:0.3|type:sqrt(8)|key:enter|sleep:0.3|type:*3"
WEB[select]="insert:x = 4|key:Enter|wait:250|insert:2^10|key:Enter|wait:250|insert:sqrt(8)|key:Enter|wait:250|key:ArrowUp|key:ArrowUp"
NATIVE[select]="type:x = 4|key:enter|sleep:0.3|type:2^10|key:enter|sleep:0.3|type:sqrt(8)|key:enter|sleep:0.3|key:up|key:up"
WEB[graph]="insert:graph x^2-3|wait:600"
NATIVE[graph]="type:graph x^2-3|sleep:0.8"
WEB[banner]="js:window.__qcalcInbox && window.__qcalcInbox($CARD)|wait:300"
NATIVE[banner]="card:max|ask:sam"
WEB[games]="insert:games"
NATIVE[games]="type:games"
WEB[help]="insert:?"
NATIVE[help]="type:?"

scenarios=("$@")
(( ${#scenarios} )) || scenarios=(live dual history select graph banner games help)
for theme in light dark; do
  for s in "${scenarios[@]}"; do
    "$TOOLS/webref" "$ROOT/dist" "$theme" "$OUT/$s-$theme-current.png" "${(@s:|:)WEB[$s]}" wait:700 >/dev/null
    "${DRIVE[@]}" "theme:$theme" hide show clear sleep:0.3 "${(@s:|:)NATIVE[$s]}" sleep:0.8 "snap:$TOOLS/$s-$theme" >/dev/null
    mv "$TOOLS/$s-$theme-bar.png" "$OUT/$s-$theme-native.png"
    echo "$s ($theme)"
  done
done
"${DRIVE[@]}" clear hide
