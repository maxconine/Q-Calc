#!/bin/bash
# Headless checks for the games spoken boards and slots rebuild rule: builds GameSpeech.swift with Tests/games/main.swift into one
# command-line binary and runs it. No app, no window.
#   native/mac/scripts/test-games.sh
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
arch="${ARCH:-$(uname -m)}"
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
xcrun swiftc -O -target "$arch-apple-macos12.0" \
  "$here/Sources/Games/Shell/GameSpeech.swift" \
  "$here/Tests/games/main.swift" \
  -o "$out/games-test"
"$out/games-test"
