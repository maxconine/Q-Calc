#!/bin/zsh
# Driven checks for the line's ⌘ keys with real key events (drive.sh key:), on a running dev build (QCALC_DEV_ID for a
# re-signed copy). ⌘A ⌘C ⌘X ⌘V had done nothing when they came to the line's keyDown (no key equivalent offered).
# Uses the clipboard and puts it back.
#   QCALC_DEV_ID=<bundle id> zsh native/mac/scripts/test-keys.sh
drive="$(cd "$(dirname "$0")" && pwd)/drive.sh"
old="$(pbpaste)"
fails=0
check() { if [[ "$2" == "$3" ]]; then echo "ok   $1"; else echo "FAIL $1: got '$2', want '$3'"; fails=$((fails + 1)); fi }
zsh "$drive" show key:cmd+a key:backspace "keys:12+3" key:cmd+a key:cmd+c
check "⌘A ⌘C copies the line" "$(pbpaste)" "12+3"
printf '42' | pbcopy
zsh "$drive" "type:1+" key:cmd+v key:cmd+a key:cmd+x
check "⌘V pastes, ⌘A ⌘X cuts it all" "$(pbpaste)" "1+42"
zsh "$drive" "type:7" key:cmd+a key:9 key:cmd+a key:cmd+c
check "⌘A then typing replaces the line" "$(pbpaste)" "9"
zsh "$drive" key:cmd+a key:backspace hide
printf '%s' "$old" | pbcopy
exit $fails
