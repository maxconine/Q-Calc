#!/bin/zsh
# Driven checks for an explanation's Related links with the keys (QA item 79), on a copy of the dev build re-signed
# under its own id (a running Q Calc Dev is left alone). Tab selects the next link, ⇧Tab the one before, ↵ follows
# the selected one to its own explanation, where Tab starts again at its first link. Keys go straight to the
# explanation (sheetkey:), so this runs with q calc in the background or the screen locked.
#   zsh native/mac/scripts/test-sheet-links.sh [snap dir]   (after zsh native/mac/build.sh)
here="$(cd "$(dirname "$0")/.." && pwd)"
id="com.maxconine.qcalc.native-links$$"
folder="Q Calc Links $$"
work="$(mktemp -d)"
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
drive() { QCALC_DEV_ID="$id" zsh "$here/scripts/drive.sh" "$@"; }
state() { drive "sheetexplain:$work/e" sleep:0.3; tr '\n' '|' < "$work/e"; }
drive identities sleep:1.5 "sheetfilter:P = V I" sleep:0.5 sheetpick sleep:0.8
start="$(state)"
title="${start%%|*}"
echo "explanation: $title"
check "the explanation has the keys, nothing selected" "${start#*|}" ""
drive sheetkey:tab
first="$(state)"
[[ -n "${first#*|}" ]] && echo "ok   Tab selects the first link (${first#*|})" || { echo "FAIL Tab selected nothing"; fails=$((fails + 1)); }
drive sheetkey:tab
second="$(state)"
[[ -n "${second#*|}" && "$second" != "$first" ]] && echo "ok   Tab again, the next link (${second#*|})" || { echo "FAIL second Tab: '$second'"; fails=$((fails + 1)); }
drive sheetkey:tab sheetkey:tab
check "past the last link the keys move on, then come round to the first" "$(state)" "$first"
drive sheetkey:tab sheetkey:shift+tab
check "⇧Tab goes back to the first" "$(state)" "$first"
# the first link, V = IR, has its own explanation (Ohm's law)
drive sheetkey:enter sleep:0.5
[[ -n "$1" ]] && drive "snap:$1/followed"
after="$(state)"
[[ "${after%%|*}" != "$title" && "$after" != "(none)" ]] && echo "ok   ↵ follows it (${after%%|*})" || { echo "FAIL ↵ didn't follow: '$after'"; fails=$((fails + 1)); }
check "the new explanation has the keys, nothing selected" "${after#*|}" ""
drive sheetkey:tab
check "Tab there lands on its first link" "$(state | cut -d'|' -f2)" "I = V/R"
drive "sheetkey:cmd+[" sleep:0.5
check "⌘[ goes back to the first explanation" "$(state | cut -d'|' -f1)" "$title"
exit $fails
