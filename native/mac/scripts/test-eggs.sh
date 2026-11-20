#!/bin/bash
# Headless checks for the effects' and the Konami code's rules: builds EggRules.swift with Tests/eggs/main.swift into
# one command-line binary and runs it. No app, no window.
#   native/mac/scripts/test-eggs.sh
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
arch="${ARCH:-$(uname -m)}"
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
xcrun swiftc -O -target "$arch-apple-macos12.0" \
  "$here/Sources/Effects/EggRules.swift" \
  "$here/Tests/eggs/main.swift" \
  -o "$out/eggs-test"
"$out/eggs-test"
