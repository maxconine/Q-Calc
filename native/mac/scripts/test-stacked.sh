#!/bin/bash
# Headless checks for the bar's stacked fractions, powers and roots: builds StackedMath.swift with
# Tests/stacked/main.swift into one command-line binary and runs it. No app, no window.
#   native/mac/scripts/test-stacked.sh
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
arch="${ARCH:-$(uname -m)}"
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
xcrun swiftc -O -target "$arch-apple-macos12.0" \
  "$here/Sources/Bar/StackedMath.swift" \
  "$here/Tests/stacked/main.swift" \
  -o "$out/stacked-test"
"$out/stacked-test"
