#!/bin/zsh
# A fresh install's first run: copies the dev build, re-signs it under its own id, support folder, defaults domain
# and shortcut (⌃⌥⇧K, so a running Q Calc Dev keeps its own), launches it, checks the onboarding counts the first
# show and a second show leave, snaps each, then quits it and removes everything it made. Each show had counted
# as two opens and each save as two commits, so the first run skipped the walkthrough's shortcut step, showed
# `⌘, opens settings`, and the examples stopped after three saves.
#   zsh native/mac/scripts/test-first-run.sh [snap dir]   (after zsh native/mac/build.sh)
here="$(cd "$(dirname "$0")/.." && pwd)"
id="com.maxconine.qcalc.native-firstrun$$"
folder="Q Calc FirstRun $$"
work="$(mktemp -d)"
snaps="${1:-$work}"
app="$work/$folder.app"
cleanup() {
  osascript -e "tell application id \"$id\" to quit" >/dev/null 2>&1
  sleep 1
  pkill -f "$app/Contents/MacOS/" 2>/dev/null
  defaults delete "$id" >/dev/null 2>&1
  rm -rf "$HOME/Library/Application Support/$folder" "$HOME/Library/Logs/$folder" "$work"
}
trap cleanup EXIT
cp -R "$here/dist/Q Calc Dev.app" "$app" || exit 2
/usr/libexec/PlistBuddy -c "Set CFBundleIdentifier $id" -c "Set QCSupportFolder '$folder'" -c "Set CFBundleName '$folder'" \
  -c "Set CFBundleDisplayName '$folder'" -c "Set QCDefaultHotKey ctrl+alt+shift+k" "$app/Contents/Info.plist"
xattr -cr "$app"
codesign --force --deep -s - "$app" 2>/dev/null || exit 2
open -n "$app"
for _ in {1..40}; do lsappinfo info -only pid -app "$id" | grep -q '=' && break; sleep 0.25; done
sleep 3
fails=0
check() { if [[ "$2" == "$3" ]]; then echo "ok   $1"; else echo "FAIL $1: got '$2', want '$3'"; fails=$((fails + 1)); fi }
opens() { defaults read "$id" qcalc.onboarding 2>/dev/null | sed -n 's/.*opens = \([0-9]*\);/\1/p'; }
hints() { defaults read "$id" qcalc.onboarding 2>/dev/null | sed -n 's/.*hints = \([0-9]*\);/\1/p'; }
# the tour's bits (src/lib/tour.ts): on 1<<12, the shortcut step 1<<14; `⌘, opens settings` is onboarding's 512
check "first run is one open" "$(opens)" "1"
check "first run takes the walkthrough" "$(( ${$(hints):-0} & 4096 ))" "4096"
check "first run's shortcut step still to do" "$(( ${$(hints):-0} & 16384 ))" "0"
check "no ⌘, hint on the first open" "$(( ${$(hints):-0} & 512 ))" "0"
commits() { defaults read "$id" qcalc.onboarding 2>/dev/null | sed -n 's/.*commits = \([0-9]*\);/\1/p'; }
QCALC_DEV_ID="$id" zsh "$here/scripts/drive.sh" "snap:$snaps/first-run" "type:12 * 7" enter sleep:1
check "a save is one commit" "$(commits)" "1"
QCALC_DEV_ID="$id" zsh "$here/scripts/drive.sh" hide show sleep:1 "snap:$snaps/second-open"
check "a second show is the second open" "$(opens)" "2"
check "the second open learns the shortcut" "$(( ${$(hints):-0} & 16384 ))" "16384"
QCALC_DEV_ID="$id" zsh "$here/scripts/drive.sh" hide
exit $fails
