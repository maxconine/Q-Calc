#!/bin/bash
# Headless checks for the message cards' rules: builds CardRules.swift with Tests/cards/main.swift into one
# command-line binary and runs it. No app, no window, no relay.
#   native/mac/scripts/test-cards.sh
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
arch="${ARCH:-$(uname -m)}"
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
xcrun swiftc -O -target "$arch-apple-macos12.0" \
  "$here/Sources/Messages/CardRules.swift" \
  "$here/Tests/cards/main.swift" \
  -o "$out/cards-test"
"$out/cards-test"
