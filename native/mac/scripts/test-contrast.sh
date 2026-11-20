#!/bin/bash
# Headless checks for Increase Contrast (QA item 90): builds Theme.swift with Tests/contrast/main.swift and measures
# the faded greys against WCAG AA; then the equal game's contrast mark colours, white text on each.
#   native/mac/scripts/test-contrast.sh
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
arch="${ARCH:-$(uname -m)}"
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
xcrun swiftc -O -target "$arch-apple-macos12.0" \
  "$here/Sources/Bar/Theme.swift" \
  "$here/Tests/contrast/main.swift" \
  -o "$out/contrast-test"
"$out/contrast-test"
# equal: the colours picked under `colors.contrast ?` carry white letters
python3 -I - "$here/Sources/Games/Equal/EqualPanel.swift" <<'PY'
import re, sys
src = open(sys.argv[1]).read()
def lum(h):
    def l(v):
        v /= 255
        return v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4
    return 0.2126 * l(h >> 16 & 255) + 0.7152 * l(h >> 8 & 255) + 0.0722 * l(h & 255)
bad = 0
for name in ("hit", "near", "miss"):
    line = next(l for l in src.splitlines() if re.search(rf"private var {name}: RGBA", l))
    picked = line.split("colors.contrast")[1].split(": colors.dark")[0]
    for h in re.findall(r"0x([0-9a-f]{6})", picked):
        r = 1.05 / (lum(int(h, 16)) + 0.05)
        ok = r >= 4.5
        bad += not ok
        print(("ok   " if ok else "FAIL ") + f"equal contrast {name} #{h}: white reads {r:.2f}:1")
print("equal all passed" if not bad else f"equal {bad} failed")
sys.exit(1 if bad else 0)
PY
