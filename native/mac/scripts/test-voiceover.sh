#!/bin/zsh
# Driven checks for what VoiceOver finds on tonight's controls (QA item 110), on a running dev build (QCALC_DEV_ID for
# a re-signed copy): axtree: dumps the bar's accessibility tree from inside the app (no Accessibility permission).
# The live roots' letter is named (it read "x ="), a dual's halves say which half, a unit isn't read twice, the graph's
# head icons and reset are named (reset read "reset"), the tape's roots, dual halves and copy buttons say what they are.
#   QCALC_DEV_ID=<bundle id> zsh native/mac/scripts/test-voiceover.sh
drive="$(cd "$(dirname "$0")" && pwd)/drive.sh"
tree="$(mktemp -t qcalc-ax)"
fails=0
ax() { zsh "$drive" "$@" "axtree:$tree"; }
has() { if grep -qF -- "| $2 |" "$tree"; then echo "ok   $1"; else echo "FAIL $1: no '$2'"; fails=$((fails + 1)); fi }
lacks() { if grep -qF -- "| $2 |" "$tree"; then echo "FAIL $1: found '$2'"; fails=$((fails + 1)); else echo "ok   $1"; fi }

ax show "point:off" clear "type:x^2 - 3x + 2 = 0" sleep:1.5
has "live roots: the letter copies them all" "Copy all the solutions"
has "live roots: each its own solution" "Solution 1"
has "… and the second" "Solution 2"
ax "type:x^2 = 2" sleep:1.5
has "live dual: the exact half says so" "Exact value ±√2"
has "live dual: the decimal half says so" "Approximation ±1.41421"
ax "type:3 m + 2 m" sleep:1
has "an answer with a unit reads it once" "Answer 5 m"
ax "type:graph x^2" sleep:1.5
for name in "Zoom out" "Zoom in" "Add an equation" "Export a picture of the graph" "Graph settings" "Reset view"; do
  has "graph head: $name" "$name"
done

ax clear "type:x^2 - 3x + 2 = 0" sleep:1 barkey:enter sleep:0.5 "type:sqrt(8)" sleep:1 barkey:enter sleep:0.5 \
  "type:note hello" sleep:0.5 barkey:enter sleep:0.5 barkey:up sleep:1
has "tape roots: the letter copies them all" "Copy all the solutions"
has "tape roots: each its own solution" "Solution 2"
has "tape dual: the exact half says so" "Exact value 2√2"
has "tape dual: the decimal half says so" "Approximation 2.82843"
has "tape copy button names the answer" "Copy answer"
has "a note's copy button names the note" "Copy note"
zsh "$drive" barkey:escape clear
rm -f "$tree"
exit $fails
