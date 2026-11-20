#!/bin/bash
# Headless checks for the bar's typing rules (words to symbols, brackets, undo runs): builds Symbols.swift and Undo.swift with
# Tests/input/main.swift into one command-line binary and runs it. No app, no window.
#   native/mac/scripts/test-input.sh
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
arch="${ARCH:-$(uname -m)}"
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
xcrun swiftc -O -target "$arch-apple-macos12.0" \
  "$here/Sources/Bar/Symbols.swift" "$here/Sources/Bar/Undo.swift" \
  "$here/Tests/input/main.swift" \
  -o "$out/input-test"
"$out/input-test"
