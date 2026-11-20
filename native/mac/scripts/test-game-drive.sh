#!/bin/zsh
# Driven checks for the native games (QA items 84, 91), on a copy of the dev build re-signed under its own id (a running
# Q Calc Dev is left alone); keys go straight to the game's window (gamekey:), so this runs with q calc in the
# background or the screen locked. Solo only, nothing goes online.
#   zsh native/mac/scripts/test-game-drive.sh [snap dir]   (after zsh native/mac/build.sh)
here="$(cd "$(dirname "$0")/.." && pwd)"
id="com.maxconine.qcalc.native-gamedrive$$"
folder="Q Calc Game Drive $$"
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
state() { drive "gamestate:$work/s" sleep:0.2; grep "^$1=" "$work/s" | cut -d= -f2-; }
# the first run's walkthrough has the bar's line; skip it
drive show "type:skip" enter sleep:0.5
drive show "type:crack" enter sleep:1.5
check "crack opens in the bar's place" "$(state open)/$(state big)" "1/0"
drive gamekey:enter sleep:0.8 gamekey:ctrl+f sleep:0.8
check "⌃F mid-level: the big window" "$(state big)" "1"
[[ -n "$1" ]] && drive "snap:$1/crack-big"
drive gamekey:escape sleep:0.6
check "esc: back to the bar" "$(state big)" "0"
drive gamekey:escape sleep:0.5 gamekey:escape sleep:0.5 gamekey:escape sleep:0.6
check "esc, esc, esc: paused, the lobby, gone" "$(state open)" "0"
# 24: a round under way lives through the big window and back, the expression kept and the caret at its end
drive show "type:24 game" enter sleep:1.5 gamekey:enter sleep:1 "gamekeys:(1+2)" gamekey:ctrl+f sleep:0.8
check "24 in the big window" "$(state big)" "1"
drive gamekey:escape sleep:0.8 "gamekeys:*8" sleep:0.3
[[ -n "$1" ]] && drive "snap:$1/24-back"
check "back from the big window the round goes on, typing carries on" "$(state field)" "(1+2)*8"
# QA item 91: q calc isn't the active app here (NSApp.keyWindow nil); Tab closes the parens, a key goes in at the caret
drive "gamekeys:*(4" gamekey:tab sleep:0.4
check "24: Tab closes the open paren" "$(state field)" "(1+2)*8*(4)"
drive gamekey:left sleep:0.2 gamekey:5 sleep:0.4
check "24: a key goes in at the caret, not on the end" "$(state field)" "(1+2)*8*(45)"
drive theme:light sleep:0.5
check "the theme follows settings" "$(state dark)" "0"
drive theme:system closegame
exit $fails
