#!/bin/zsh
# A game's web view exists only while the game is open: counts WebKit WebContent processes before, during and
# after a game in the running dev build. Prints the snapshot of the game window too (its view tree).
MAC="$(cd "$(dirname "$0")/.." && pwd)"
D=(zsh "$MAC/scripts/drive.sh")
count() { ps -axo command= | grep -c "[W]ebKit.WebContent"; }
echo "WebContent before: $(count)"
"${D[@]}" show "type:pong" key:enter sleep:3
echo "WebContent with pong open: $(count)"
"${D[@]}" "key:escape" sleep:1 "key:escape" sleep:1 "key:escape" sleep:3
echo "WebContent after closing: $(count)"
grep -E "game|effect" "$HOME/Library/Logs/Q Calc Dev/overlay.log" | tail -4
