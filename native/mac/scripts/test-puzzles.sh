#!/bin/bash
# Headless checks for what VoiceOver reads in equal, 24/countdown and trails: builds PuzzleSpeech.swift with
# Tests/puzzles/main.swift into one command-line binary and runs it. No app, no window.
#   native/mac/scripts/test-puzzles.sh
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
arch="${ARCH:-$(uname -m)}"
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
xcrun swiftc -O -target "$arch-apple-macos12.0" \
  "$here/Sources/Games/PuzzleSpeech.swift" \
  "$here/Tests/puzzles/main.swift" \
  -o "$out/puzzles-test"
"$out/puzzles-test"
