#!/bin/bash
# Headless checks for the walkthrough's and the cheat sheet's rules: builds TourRules.swift with
# Tests/tour/main.swift into one command-line binary and runs it. No app, no window.
#   native/mac/scripts/test-tour.sh
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
arch="${ARCH:-$(uname -m)}"
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
xcrun swiftc -O -target "$arch-apple-macos12.0" \
  "$here/Sources/Bar/TourRules.swift" \
  "$here/Tests/tour/main.swift" \
  -o "$out/tour-test"
"$out/tour-test"
