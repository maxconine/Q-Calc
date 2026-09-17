#!/bin/zsh
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "Usage: macos/update-cask.sh <version> <sha256>" >&2
  exit 1
fi

VERSION="$1"
SHA256="$2"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CASK="$ROOT/Casks/q-calc.rb"

if [[ ! -f "$CASK" ]]; then
  echo "Missing $CASK" >&2
  exit 1
fi

perl -i -pe "s/^  version \".*\"/  version \"$VERSION\"/" "$CASK"
perl -i -pe "s/^  sha256 .*/  sha256 \"$SHA256\"/" "$CASK"

echo "Updated $CASK to $VERSION ($SHA256)"
