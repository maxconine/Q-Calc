#!/bin/bash
# Headless checks for end-to-end sharing: builds ShareCrypt.swift and FileCrypt.swift with Tests/share/main.swift
# into one command-line binary and runs it. No app, no window, no Keychain, no network.
#   native/mac/scripts/test-share.sh            # this Mac's architecture
#   ARCH=arm64 native/mac/scripts/test-share.sh
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
arch="${ARCH:-$(uname -m)}"
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
xcrun swiftc -O -target "$arch-apple-macos12.0" \
  "$here/Sources/Messages/FileCrypt.swift" \
  "$here/Sources/Messages/ShareCrypt.swift" \
  "$here/Tests/share/main.swift" \
  -o "$out/share-test"
"$out/share-test"
