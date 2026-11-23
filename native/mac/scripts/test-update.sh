#!/bin/bash
# Headless checks for the update line's rules: builds UpdateRules.swift with
# Tests/update/main.swift into one command-line binary and runs it. No app, no window.
#   native/mac/scripts/test-update.sh
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
arch="${ARCH:-$(uname -m)}"
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
xcrun swiftc -O -target "$arch-apple-macos12.0" \
  "$here/Sources/Bar/UpdateRules.swift" \
  "$here/Tests/update/main.swift" \
  -o "$out/update-test"
"$out/update-test"
