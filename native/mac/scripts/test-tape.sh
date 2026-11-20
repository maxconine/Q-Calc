#!/bin/bash
# Headless checks for the tape's selection, scrolling and VoiceOver rules: builds TapeRules.swift with
# Tests/tape/main.swift into one command-line binary and runs it. No app, no window.
#   native/mac/scripts/test-tape.sh
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
arch="${ARCH:-$(uname -m)}"
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
xcrun swiftc -O -target "$arch-apple-macos12.0" \
  "$here/Sources/Bar/TapeRules.swift" \
  "$here/Tests/tape/main.swift" \
  -o "$out/tape-test"
"$out/tape-test"
