#!/bin/zsh
# Long-run growth check on a running dev build (QCALC_DEV_ID for a re-signed copy): opens and closes the formula sheets
# and the periodic table N times (default 20) and fails if the app's thread count grew by more than 3. Each sheet open
# and close had left a thread behind for good, waiting in AppKit's threaded window zoom animation
# (_NSWindowTransformAnimation); `threads:<file>` (dev builds) writes every thread's name, queue and stack.
# Memory, threads and ports come from `top` (`footprint`, `sample` and `vmmap` need Developer Mode here).
#   QCALC_DEV_ID=<bundle id> zsh native/mac/scripts/test-leaks.sh [N]
id="${QCALC_DEV_ID:-com.maxconine.qcalc.native-dev}"
n="${1:-20}"
pid="$(lsappinfo info -only pid -app "$id" | sed -n 's/.*=//p')"
[[ -z "$pid" ]] && { echo "no running dev build"; exit 2; }
stats() { top -l 1 -pid "$pid" -stats mem,th,ports | tail -1; }
threads() { ps -M -p "$pid" | tail -n +2 | wc -l | tr -d ' '; }
post() {
  osascript -l JavaScript -e "
    ObjC.import('Foundation');
    function run(argv) {
      var c = \$.NSDistributedNotificationCenter.defaultCenter;
      for (var r = 0; r < $n; r++) for (var i = 0; i < argv.length; i++) {
        c.postNotificationNameObjectUserInfoDeliverImmediately('$id.command', argv[i], \$(), true);
        \$.NSRunLoop.currentRunLoop.runUntilDate(\$.NSDate.dateWithTimeIntervalSinceNow(0.7));
      }
    }" "$@" >/dev/null
}
n_save=$n; n=1; post show; n=$n_save
sleep 3
before="$(threads)"; echo "before: mem/threads/ports $(stats)"
post "type:formulas" enter key:cmd+w "type:periodic" enter key:cmd+w
n=1; post "type:" hide
sleep 5
after="$(threads)"; echo "after $n_save× sheets + periodic: mem/threads/ports $(stats)"
if (( after - before > 3 )); then echo "FAIL threads grew $before → $after"; exit 1; fi
echo "ok   threads $before → $after"
