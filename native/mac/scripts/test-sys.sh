#!/bin/zsh
# Driven checks for the `sys` panel (QA item 86) on a copy of the dev build re-signed under its own id, support folder,
# defaults and shortcut (a running Q Calc Dev is left alone). The bar is a nonactivating panel, so q calc isn't the
# active app while it's up and NSApp.keyWindow can be nil: an equation's ⌘Z / ⌘⌫ / → read the editor from it alone
# and did nothing. Also ⌘Z on the line after a stray ⌫ there (which closes the fields) brings the equations back.
# Runs with the screen locked (keys are posted to the bar).
#   zsh native/mac/scripts/test-sys.sh [snap dir]   (after zsh native/mac/build.sh)
here="$(cd "$(dirname "$0")/.." && pwd)"
id="com.maxconine.qcalc.native-systest$$"
folder="Q Calc Sys Test $$"
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
# past the first run's walkthrough, which has the line
defaults write "$id" qcalc.onboarding -dict commits 600 done 1 hints 2089970 opens 3000
open -n "$app"
for _ in {1..40}; do lsappinfo info -only pid -app "$id" | grep -q '=' && break; sleep 0.25; done
sleep 3
fails=0
check() { if [[ "$2" == "$3" ]]; then echo "ok   $1"; else echo "FAIL $1: got '$2', want '$3'"; fails=$((fails + 1)); fi }
drive() { QCALC_DEV_ID="$id" zsh "$here/scripts/drive.sh" "$@"; }
# the equations as the snapshot's view tree has them, joined by |
fields() { drive "snap:$work/f" sleep:0.2; grep 'AppKitTextField' "$work/f-bar.txt" | sed 's/.*}} //' | paste -sd '|' -; }
drive show "type:sys 2" enter sleep:0.5 "keys:x + y = 3" key:enter "keys:x - y = 1" sleep:0.8
check "two equations typed, ↵ between" "$(fields)" "x + y = 3|x - y = 1"
drive key:tab sleep:0.3 "keys: + 0" sleep:0.3
check "Tab goes round to the first" "$(fields)" "x + y = 3 + 0|x - y = 1"
drive key:cmd+z sleep:0.5
check "⌘Z in an equation undoes its typing" "$(fields)" "x + y = 3|x - y = 1"
drive key:cmd+shift+z sleep:0.5
check "⇧⌘Z redoes it" "$(fields)" "x + y = 3 + 0|x - y = 1"
drive key:cmd+backspace sleep:0.4 key:cmd+z sleep:0.5
check "⌘Z after ⌘⌫ brings the equation back" "$(fields)" "x + y = 3 + 0|x - y = 1"
[[ -n "$1" ]] && drive "snap:$snaps/sys-undo"
drive key:up sleep:0.3 key:backspace sleep:0.4
check "⌫ on the line closes the fields" "$(fields)" ""
drive key:cmd+z sleep:0.6
check "⌘Z on the line brings them back with their equations" "$(fields)" "x + y = 3 + 0|x - y = 1"
drive key:up sleep:0.3 key:backspace sleep:0.3 "keys:12*7" sleep:0.3 key:cmd+a key:backspace "keys:sys 2" sleep:0.5
check "after another calculation sys 2 opens empty" "$(fields)" "|"
drive esc esc hide
exit $fails
