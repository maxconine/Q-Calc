#!/bin/zsh
# Sends commands to a running dev build (Q Calc Dev only; release builds don't listen), one per argument:
#   zsh native/mac/scripts/drive.sh show "type:2^10" enter "snap:/tmp/shots/light" hide
# Commands: show, hide, settings, theme:<system|light|dark>, type:<text>, enter, up, down, tab, esc,
# key:<chord> (a real key event: key:enter, key:cmd+w), periodic, identities, card[:<from>], ask[:<from>],
# paste[:<system field>], snap:<path prefix> (a PNG per visible window), sleep:<seconds>, keys:<text> (typed into the
# focused field), click:<x>,<y> (in the bar, points from its top-left), settings[:<tab index>], forget:<name>,
# contrast:on|off (the bar as with Increase Contrast on), drop:<path>|<path>… (files dropped on the bar),
# point:<x>,<y> (the pointer resting there in the bar), point:off (the pointer leaving the line), note[:<from>] and
# noteask[:<from>] (a message's and a request's note in the corner, as while the bar is away).
# Sheets: sheetclick:<title>:<x>,<y>[:<file>] and sheethover:… (points from the sheet's top left), sheetback,
# sheetforward, sheetfilter:<text> (the front sheet's search, as if typed), sheetswipe:back|forward (a swipe that went through), sheetnew:<x>,<y> (Open in New Window on that row of
# the front sheet), sheetpages:<file> (each sheet window's pages, the one up in [brackets]).
# QCALC_DEV_ID=<bundle id> drives a copy of the dev build re-signed under another id instead.
for cmd in "$@"; do
  if [[ "$cmd" == sleep:* ]]; then
    sleep "${cmd#sleep:}"
    continue
  fi
  osascript -l JavaScript -e "
    ObjC.import('Foundation');
    \$.NSDistributedNotificationCenter.defaultCenter.postNotificationNameObjectUserInfoDeliverImmediately(
      '${QCALC_DEV_ID:-com.maxconine.qcalc.native-dev}.command', $(printf '%s' "$cmd" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))'), \$(), true)" >/dev/null
  sleep 0.35
done
