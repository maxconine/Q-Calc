#!/bin/bash
# macOS 12 checks the compiler doesn't make for the app build:
#  1. every SF Symbol name in the Swift sources (native/mac/Sources and macos/) exists on Monterey, or is passed
#     through sfSymbol13(…, else: older); a newer name draws nothing on 12. Ages come from this Mac's
#     CoreGlyphs name_availability.plist.
#  2. the web-view app's Swift (macos/build.sh's file list, which includes the settings forms the native app
#     shares through Sources/Shared) typechecks at arm64-apple-macos12.0.
#   native/mac/scripts/test-compat12.sh
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$here/../.." && pwd)"
fail=0

python3 -I - "$here/Sources" "$root/macos" <<'EOF' || fail=1
import glob, plistlib, re, sys
d = plistlib.load(open('/System/Library/CoreServices/CoreGlyphs.bundle/Contents/Resources/name_availability.plist', 'rb'))
syms, rel = d['symbols'], d['year_to_release']
ctx = re.compile(r'systemName:|systemSymbolName:|symbol:')
bad = 0
for base in sys.argv[1:]:
    for f in glob.glob(base + '/**/*.swift', recursive=True):
        for i, line in enumerate(open(f, errors='ignore'), 1):
            if not ctx.search(line):
                continue
            safe = line.split('sfSymbol13(')[0] if 'sfSymbol13(' in line else line
            for name in re.findall(r'"([a-z0-9]+(?:\.[a-z0-9]+)*)"', safe):
                year = syms.get(name)
                if year and float(rel[year]['macOS']) > 12.0:
                    print(f"FAIL {f}:{i}: SF Symbol '{name}' needs macOS {rel[year]['macOS']}; use sfSymbol13(…, else: …)")
                    bad += 1
print("ok   SF Symbol names exist on macOS 12" if not bad else f"{bad} symbol(s) missing on macOS 12")
sys.exit(1 if bad else 0)
EOF

mac="$root/macos"
sparkle="${QCALC_SPARKLE_DIR:-$mac/vendor/Sparkle-2.10.0}"
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
files=()
while IFS= read -r f; do files+=("$mac/$f"); done < <(sed -n 's/^ *"\$MAC\/\([A-Za-z]*\.swift\)" \\$/\1/p' "$mac/build.sh")
if xcrun swiftc -typecheck -parse-as-library -target "arm64-apple-macos12.0" -F "$sparkle" \
     -module-cache-path "$out/mc" "${files[@]}" 2> "$out/err"; then
  echo "ok   macos/ (${#files[@]} files) typechecks at macOS 12"
else
  grep -E "error" "$out/err" | head -20
  echo "FAIL macos/ doesn't typecheck at macOS 12"
  fail=1
fi
exit $fail
