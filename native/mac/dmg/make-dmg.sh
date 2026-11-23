#!/bin/zsh
# Packs a signed Q Calc.app into a disk image whose window says "To install, drag Q Calc to Applications":
# the app on the left, chevrons, an Applications alias on the right, over background.png (+ @2x).
#
#   zsh native/mac/dmg/make-dmg.sh "<path>/Q Calc.app" "<out>.dmg"
#
# Uses dmgbuild through build_dmg.py (pinned below, installed into native/build/dmgbuild-venv on first use), which writes Finder's
# layout without driving Finder, so it works headless on CI. The app is copied as it is: its signature is untouched.
set -euo pipefail

DMG_DIR="$(cd "$(dirname "$0")" && pwd)"
NATIVE="$(cd "$DMG_DIR/../.." && pwd)"
DMGBUILD_VERSION="1.6.5"
VENV="${QCALC_DMGBUILD_VENV:-$NATIVE/build/dmgbuild-venv}"

APP="${1:?usage: make-dmg.sh <app> <out.dmg>}"
OUT="${2:?usage: make-dmg.sh <app> <out.dmg>}"
[[ -d "$APP" ]] || { echo "No app at $APP" >&2; exit 1; }

if [[ "$("$VENV/bin/python" -c 'import importlib.metadata as m; print(m.version("dmgbuild"))' 2>/dev/null || true)" != "$DMGBUILD_VERSION" ]]; then
  echo "Installing dmgbuild $DMGBUILD_VERSION…"
  rm -rf "$VENV"
  /usr/bin/python3 -m venv "$VENV"
  "$VENV/bin/pip" install --quiet --disable-pip-version-check "dmgbuild==$DMGBUILD_VERSION"
fi

rm -f "$OUT"
"$VENV/bin/python" -I "$DMG_DIR/build_dmg.py" "$DMG_DIR/settings.py" "$(basename "$APP" .app)" "$OUT" \
  "$APP" "$DMG_DIR/background.png"
hdiutil verify -quiet "$OUT"
echo "Packaged $OUT ($(du -h "$OUT" | cut -f1))"
