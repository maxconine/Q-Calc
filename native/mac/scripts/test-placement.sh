#!/bin/bash
# Headless checks for the bar window's placement across displays: builds BarPlacement.swift with
# Tests/placement/main.swift into one command-line binary and runs it. No app, no window.
#   native/mac/scripts/test-placement.sh
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
arch="${ARCH:-$(uname -m)}"
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
xcrun swiftc -O -target "$arch-apple-macos12.0" \
  "$here/Sources/App/BarPlacement.swift" \
  "$here/Tests/placement/main.swift" \
  -o "$out/placement-test"
"$out/placement-test"
