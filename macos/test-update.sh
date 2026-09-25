#!/bin/zsh
set -euo pipefail

# End-to-end Sparkle test: an old build updates itself to a new one from a local feed.
# It launches Q Calc, so run it while nobody needs the Mac. Your own Q Calc must be quit;
# its settings are backed up and restored.
#
#   zsh macos/test-update.sh <throwaway-private-key-file> <throwaway-public-key-file> [quit|idle]
#
# quit (default): the old app downloads the update, then we quit it and check it was replaced.
# idle: leave the Mac untouched; the update should install itself and relaunch after 20s idle.
# Build first with: zsh macos/build.sh

if [[ $# -lt 2 || $# -gt 3 ]]; then
  echo "Usage: macos/test-update.sh <private-key-file> <public-key-file> [quit|idle]" >&2
  exit 1
fi

PRIVATE_KEY="$1"
PUBLIC_KEY="$(tr -d '[:space:]' < "$2")"
MODE="${3:-quit}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUILT="$ROOT/macos/dist/Q Calc.app"
DOMAIN="com.maxconine.qcalc"
PORT=8765
WORK="$(mktemp -d /tmp/qcalc-update-test.XXXXXX)"
APP="$WORK/Applications/Q Calc.app"
SERVER_PID=""

if [[ ! -d "$BUILT" ]]; then
  echo "Build first: zsh macos/build.sh" >&2
  exit 1
fi
if pgrep -x QCalc >/dev/null; then
  echo "Quit Q Calc first (menu bar icon → Quit Q Calc)." >&2
  exit 1
fi

cleanup() {
  [[ -n "$SERVER_PID" ]] && kill "$SERVER_PID" 2>/dev/null || true
  pkill -f "$WORK/Applications/Q Calc.app/Contents/MacOS/QCalc" 2>/dev/null || true
  defaults delete "$DOMAIN" >/dev/null 2>&1 || true
  if [[ -f "$WORK/defaults-backup.plist" ]]; then
    defaults import "$DOMAIN" "$WORK/defaults-backup.plist"
  fi
  echo "Restored your Q Calc settings. Test files are in $WORK"
}
trap cleanup EXIT

defaults export "$DOMAIN" "$WORK/defaults-backup.plist" 2>/dev/null || true
defaults delete "$DOMAIN" >/dev/null 2>&1 || true

# $1 app, $2 version: point at the local feed and the throwaway key, then re-sign the outer bundle
prepare() {
  local plist="$1/Contents/Info.plist"
  /usr/libexec/PlistBuddy -c "Set :SUFeedURL http://127.0.0.1:${PORT}/appcast.xml" "$plist"
  /usr/libexec/PlistBuddy -c "Set :SUPublicEDKey $PUBLIC_KEY" "$plist"
  /usr/libexec/PlistBuddy -c "Set :CFBundleVersion $2" "$plist"
  /usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $2" "$plist"
  codesign --force --sign - --timestamp=none --identifier "$DOMAIN" "$1"
  codesign --verify --deep --strict "$1"
}

mkdir -p "$WORK/new" "$WORK/Applications" "$WORK/feed"
ditto "$BUILT" "$WORK/new/Q Calc.app"
prepare "$WORK/new/Q Calc.app" 99.0.0
ditto -c -k --keepParent "$WORK/new/Q Calc.app" "$WORK/feed/Q-Calc-99.0.0.zip"

ditto "$BUILT" "$APP"
prepare "$APP" 0.0.1

cp "$ROOT/docs/appcast.xml" "$WORK/feed/appcast.xml"
QCALC_APPCAST="$WORK/feed/appcast.xml" zsh "$ROOT/macos/update-appcast.sh" \
  "$WORK/feed/Q-Calc-99.0.0.zip" "http://127.0.0.1:${PORT}/Q-Calc-99.0.0.zip" < "$PRIVATE_KEY"

(cd "$WORK/feed" && exec python3 -m http.server "$PORT" --bind 127.0.0.1 >"$WORK/server.log" 2>&1) &
SERVER_PID=$!
sleep 1

version() { /usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "$APP/Contents/Info.plist"; }

# a plain kill, so no automation prompt; sparkle's installer waits for the process to exit either way
quit_app() { pkill -f "$APP/Contents/MacOS/QCalc" || true; }

wait_for() {
  local what="$1" seconds="$2"
  local i
  for (( i = 0; i < seconds; i++ )); do
    if eval "$what"; then return 0; fi
    sleep 1
  done
  return 1
}

# sparkle skips the very first launch, so launch, quit, and launch again with no last-check time
echo "First launch…"
open -g "$APP"
wait_for 'pgrep -f "$APP/Contents/MacOS/QCalc" >/dev/null' 20
sleep 5
quit_app
wait_for '! pgrep -f "$APP/Contents/MacOS/QCalc" >/dev/null' 20
defaults delete "$DOMAIN" SULastCheckTime >/dev/null 2>&1 || true
if [[ "$MODE" == "idle" ]]; then
  defaults write "$DOMAIN" qcalc.updateQuietSeconds -float 20
fi

echo "Second launch; waiting for the download…"
open -g "$APP"
if ! wait_for 'grep -q "Q-Calc-99.0.0.zip" "$WORK/server.log"' 90; then
  echo "FAIL: the app never downloaded the update." >&2
  log show --last 5m --predicate 'process == "QCalc" OR process == "Autoupdate"' | tail -40
  exit 1
fi
echo "Downloaded."

if [[ "$MODE" == "idle" ]]; then
  echo "Hands off the keyboard and mouse; waiting for the idle install…"
  if ! wait_for '[[ "$(version)" == "99.0.0" ]]' 180; then
    echo "FAIL: not installed after 3 minutes idle." >&2
    log show --last 5m --predicate 'process == "QCalc" OR process == "Autoupdate"' | tail -40
    exit 1
  fi
  if wait_for 'pgrep -f "$APP/Contents/MacOS/QCalc" >/dev/null' 30; then
    echo "Relaunched after installing."
  else
    echo "FAIL: installed but did not relaunch." >&2
    exit 1
  fi
else
  sleep 10
  quit_app
  if ! wait_for '[[ "$(version)" == "99.0.0" ]]' 120; then
    echo "FAIL: not installed after quitting." >&2
    log show --last 5m --predicate 'process == "QCalc" OR process == "Autoupdate"' | tail -40
    exit 1
  fi
fi

codesign --verify --deep --strict "$APP"
if xattr -r "$APP" | grep -q com.apple.quarantine; then
  echo "FAIL: the updated app is quarantined." >&2
  exit 1
fi
echo "PASS: 0.0.1 updated itself to 99.0.0 ($MODE), signature valid, no quarantine."
