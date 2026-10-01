#!/bin/zsh
set -euo pipefail

# Points the cask at a release: one zip for apple silicon, one for intel, picked by homebrew's `arch`.
#   macos/update-cask.sh <version> <apple-silicon sha256> <intel sha256>

if [[ $# -ne 3 ]]; then
  echo "Usage: macos/update-cask.sh <version> <apple-silicon sha256> <intel sha256>" >&2
  exit 1
fi

VERSION="$1"
ARM_SHA="$2"
INTEL_SHA="$3"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CASK="$ROOT/Casks/q-calc.rb"

if [[ ! -f "$CASK" ]]; then
  echo "Missing $CASK" >&2
  exit 1
fi
for sha in "$ARM_SHA" "$INTEL_SHA"; do
  if [[ ! "$sha" =~ '^[0-9a-f]{64}$' ]]; then
    echo "Not a sha256: $sha" >&2
    exit 1
  fi
done

VERSION="$VERSION" ARM_SHA="$ARM_SHA" INTEL_SHA="$INTEL_SHA" python3 - "$CASK" <<'PY'
import os, re, sys

path = sys.argv[1]
cask = open(path, encoding="utf-8").read()
env = os.environ

# the arch line, version, both checksums and the url, whatever shape the cask had before
head = (
    '  arch arm: "apple-silicon", intel: "intel"\n\n'
    f'  version "{env["VERSION"]}"\n'
    f'  sha256 arm:   "{env["ARM_SHA"]}",\n'
    f'         intel: "{env["INTEL_SHA"]}"\n\n'
    '  url "https://github.com/maxconine/Q-Calc/releases/download/v#{version}/Q-Calc-#{version}-#{arch}.zip"\n'
)
block = re.compile(r'(?:  arch [^\n]*\n\n?)?  version "[^"]*"\n(?:  sha256 [^\n]*\n(?:         [^\n]*\n)?)\n?  url "[^"]*"\n', re.S)
if not block.search(cask):
    sys.exit("couldn't find the version, sha256 and url lines in the cask")
cask = block.sub(lambda _: head, cask, count=1)
open(path, "w", encoding="utf-8").write(cask)
PY

ruby -c "$CASK" >/dev/null
echo "Updated $CASK to $VERSION (apple silicon $ARM_SHA, intel $INTEL_SHA)"
