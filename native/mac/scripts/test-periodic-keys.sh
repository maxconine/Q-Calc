#!/bin/zsh
# Driven checks for the periodic table's own keys while it has them (the bar away), on a running dev build
# (QCALC_DEV_ID for a re-signed copy). ↵ there did nothing; now ↵ puts the formula's molar mass in the bar, ⌫ takes the
# last element away and esc clears the formula, then closes the table. Keys go straight to the table (sheetkey:), so
# this runs with q calc in the background or the screen locked.
#   QCALC_DEV_ID=<bundle id> zsh native/mac/scripts/test-periodic-keys.sh
drive="$(cd "$(dirname "$0")" && pwd)/drive.sh"
out="$(mktemp -d)"
fails=0
check() { if [[ "$2" == "$3" ]]; then echo "ok   $1"; else echo "FAIL $1: got '$2', want '$3'"; fails=$((fails + 1)); fi }
# H at 38,54 and O at 700,105 (points from the table's top left at its first size)
h="sheetclick:Periodic Table:38,54"; o="sheetclick:Periodic Table:700,105"
zsh "$drive" show "type:" periodic sleep:1 hide "$h" "$h" "$o" sheetkey:backspace sheetkey:enter sleep:0.5 "line:$out/line"
check "H H O ⌫ ↵ puts H₂ in the bar" "$(cat "$out/line")" "2.016"
zsh "$drive" hide "$o" sheetkey:escape sheetkey:enter sleep:0.5 "line:$out/line"
check "esc clears the formula (↵ then does nothing)" "$(cat "$out/line")" "2.016"
zsh "$drive" sheetkey:escape sleep:0.5 "snap:$out/s"
check "a second esc closes the table" "$(ls "$out" | grep -c periodic)" "0"
zsh "$drive" "type:" hide
exit $fails
