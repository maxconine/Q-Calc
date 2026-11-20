#!/bin/zsh
# Runs every check script one after another and prints a pass/fail table (QA item 102):
#  - headless: each mac/scripts/test-*.sh that builds its rules into a command-line binary, test-compat12.sh, and the
#    web-view app's standalone tests (macos/*_test.swift, each compiled with the one file it tests);
#  - driven (after zsh native/mac/build.sh): the scripts that copy and re-sign the dev build themselves, then the ones
#    that drive QCALC_DEV_ID, against one copy this script re-signs under its own id, support folder, defaults and
#    shortcut (a running Q Calc Dev is left alone). Keys are posted to the app's windows, so this runs in the background.
#   zsh native/mac/scripts/test-all.sh               # everything
#   zsh native/mac/scripts/test-all.sh --headless    # no app needed
#   zsh native/mac/scripts/test-all.sh tape sys      # only those (script names without test- / .sh, or a macos test)
# Each check's output goes to a log in a temporary folder, named at the end; exits with the number that failed.
here="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$here/../.." && pwd)"
logs="$(mktemp -d -t qcalc-test-all)"
headless_only=0 only=()
for a in "$@"; do [[ "$a" == --headless ]] && headless_only=1 || only+=("$a"); done
wanted() { (( ${#only} == 0 )) || (( ${only[(Ie)$1]} )); }

headless=(cards compat12 contrast eggs games input placement puzzles share stacked tape tour)
macos_tests=(dictionary:DictionaryLookup math:MathEval messages:MessageInbox overlay:OverlayHealth rates:ECBRates
  sheet_match:SheetMatch sheet_place:SheetPlace)
self_copying=(first-run game-drive sheet-links sys)
on_copy=(graph keys periodic-keys voiceover leaks)
# a new script is listed here or it's flagged, so none is left out of the run
for f in "$here"/scripts/test-*.sh; do
  n="${${f:t}#test-}"; n="${n%.sh}"
  [[ "$n" == all ]] && continue
  (( ${headless[(Ie)$n]} + ${self_copying[(Ie)$n]} + ${on_copy[(Ie)$n]} )) || echo "note: test-$n.sh isn't in test-all.sh's lists"
done

rows=() failed=0
record() { # name, exit code, seconds, log
  local ok fail
  ok=$(grep -c '^ok' "$4" 2>/dev/null); fail=$(grep -c '^FAIL' "$4" 2>/dev/null)
  if (( $2 == 0 )); then rows+=("$(printf '%-18s pass  %4s ok  %3ss' "$1" "$ok" "$3")")
  else rows+=("$(printf '%-18s FAIL  %4s ok  %3s failed  exit %s  %ss' "$1" "$ok" "$fail" "$2" "$3")"); failed=$((failed + 1)); fi
  print -r -- "${rows[-1]}"
}
run() { # name, command…
  local name="$1" s=$SECONDS; shift
  local log="$logs/${name//\//-}.log"
  "$@" > "$log" 2>&1
  record "$name" $? $((SECONDS - s)) "$log"
}

for n in $headless; do wanted "$n" && run "$n" bash "$here/scripts/test-$n.sh"; done
for p in $macos_tests; do
  t="${p%%:*}" src="${p##*:}"
  wanted "$t" || continue
  run "macos/$t" zsh -c "swiftc '$root/macos/$src.swift' '$root/macos/${t}_test.swift' -o '$logs/$t.bin' && '$logs/$t.bin'"
done

if (( ! headless_only )); then
  if [[ ! -d "$here/dist/Q Calc Dev.app" ]]; then
    echo "no dev build (zsh native/mac/build.sh): driven checks skipped"; failed=$((failed + 1))
  else
    for n in $self_copying; do wanted "$n" && run "$n" zsh "$here/scripts/test-$n.sh"; done
    copy_wanted=0; for n in $on_copy; do wanted "$n" && copy_wanted=1; done
    if (( copy_wanted )); then
      id="com.maxconine.qcalc.native-testall$$" folder="Q Calc TestAll $$" work="$(mktemp -d)"
      app="$work/$folder.app"
      cleanup() {
        osascript -e "tell application id \"$id\" to quit" >/dev/null 2>&1
        sleep 1
        pkill -f "$app/Contents/MacOS/" 2>/dev/null
        defaults delete "$id" >/dev/null 2>&1
        rm -rf "$HOME/Library/Application Support/$folder" "$HOME/Library/Logs/$folder" "$work"
      }
      trap cleanup EXIT
      cp -R "$here/dist/Q Calc Dev.app" "$app"
      /usr/libexec/PlistBuddy -c "Set CFBundleIdentifier $id" -c "Set QCSupportFolder '$folder'" -c "Set CFBundleName '$folder'" \
        -c "Set CFBundleDisplayName '$folder'" -c "Set QCDefaultHotKey ctrl+alt+shift+k" "$app/Contents/Info.plist"
      xattr -cr "$app"
      codesign --force --deep -s - "$app" 2>/dev/null
      # past the first run's walkthrough
      defaults write "$id" qcalc.onboarding -dict commits 600 done 1 hints 2089970 opens 3000
      open -n "$app"
      for _ in {1..40}; do lsappinfo info -only pid -app "$id" | grep -q '=' && break; sleep 0.25; done
      sleep 3
      for n in $on_copy; do
        wanted "$n" || continue
        if [[ "$n" == leaks ]]; then run "$n" env QCALC_DEV_ID="$id" zsh "$here/scripts/test-$n.sh" 8
        else run "$n" env QCALC_DEV_ID="$id" zsh "$here/scripts/test-$n.sh"; fi
        QCALC_DEV_ID="$id" zsh "$here/scripts/drive.sh" esc esc "type:" hide >/dev/null 2>&1
      done
    fi
  fi
fi

echo
echo "── $(( ${#rows} )) checks, $failed failed ── logs: $logs"
print -l -- $rows
exit $failed
