#!/bin/zsh
# Launch time and memory of an app bundle, and whether any WebKit WebContent process belongs to the session:
#   zsh native/mac/parity/measure.sh "<path to .app>" <runs> [drive commands for the dev build…]
# Launch time is process start to the end of applicationDidFinishLaunching for the native build (it logs it), and
# process start to the app answering `ps` with its window up for any other app. Memory is the phys_footprint
# `footprint` reports (what Activity Monitor calls Memory), plus RSS.
set -uo pipefail
APP="$1"; RUNS="${2:-3}"; shift 2
EXE="$APP/Contents/MacOS/QCalc"
webcontent() { ps -axo pid=,rss=,command= | grep -i "com.apple.WebKit.WebContent" | grep -v grep; }
mem() {
  local pid=$1
  local fp; fp="$(footprint -p "$pid" 2>/dev/null | awk '/phys_footprint:/ {print $2, $3; exit}')"
  local rss; rss="$(ps -o rss= -p "$pid" | tr -d ' ')"
  echo "footprint ${fp:-n/a}, rss $((rss / 1024)) MB"
}
echo "WebContent processes before launch: $(webcontent | wc -l | tr -d ' ')"
for i in $(seq 1 "$RUNS"); do
  pkill -f "$EXE" 2>/dev/null; sleep 1
  open "$APP"
  sleep 4
  pid="$(pgrep -f "$EXE" | head -1)"
  echo "run $i: pid $pid, idle: $(mem "$pid")"
done
for cmd in "$@"; do zsh "$(dirname "$0")/../scripts/drive.sh" "$cmd"; done
sleep 2
pid="$(pgrep -f "$EXE" | head -1)"
echo "after driving: $(mem "$pid")"
echo "WebContent processes now: $(webcontent | wc -l | tr -d ' ')"
webcontent | head -5
