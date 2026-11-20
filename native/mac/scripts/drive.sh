#!/bin/zsh
# Sends commands to a running dev build (Q Calc Dev only; release builds don't listen), one per argument:
#   zsh native/mac/scripts/drive.sh show "type:2^10" enter "snap:/tmp/shots/light" hide
# Commands: show, hide, settings, theme:<system|light|dark>, type:<text>, enter, up, down, tab, esc,
# key:<chord> (a real key event: key:enter, key:cmd+w), periodic, identities, card[:<from>], ask[:<from>],
# paste[:<system field>], snap:<path prefix> (a PNG per visible window), sleep:<seconds>, keys:<text> (typed into the
# focused field), click:<x>,<y> (in the bar, points from its top-left), settings[:<tab index>], keybinds (the keybinds sheet), forget:<name>,
# contrast:on|off (the bar as with Increase Contrast on), drop:<path>|<path>… (files dropped on the bar),
# click:<x>,<y>,shift|option|control|command (a click with those keys held), point:<x>,<y> (the pointer resting there in the bar), point:off (the pointer leaving the line), note[:<from>] and
# noteask[:<from>] (a message's and a request's note in the corner, as while the bar is away).
# line:<file> (the bar's line written to the file).
# Tape: barkey:<chord> (a key straight to the bar's line, as on a locked screen: barkey:up, barkey:cmd+z), tapestate:<file>
# (open, selected, pick, rows, line, clipboard, then each row).
# Sheets: sheetclick:<title>:<x>,<y>[:<file>] and sheethover:… (points from the sheet's top left; title `popover` for the
# explanation up beside a row), sheetback,
# sheetforward, sheetkey:<chord> and sheetkeys:<text> (a key or text straight to the front sheet or its explanation, which
# works while q calc is not the active app, as on a locked screen), sheetexplain:<file> (the explanation's title and selected text, e.g. a link Tab went to), sheetfilter:<text> (the front sheet's search, as if typed), sheetpick (↵ in it: the first row), sheetswipe:back|forward (a swipe that went through), sheetnew:<x>,<y> (Open in New Window on that row of
# the front sheet), sheetpages:<file> (each sheet window's pages, the one up in [brackets]).
# Graph: drag:<x1>,<y1>,<x2>,<y2>[,shift][,right][,steps] (pan, or orbit in 3D), scroll:<x>,<y>,<dy>[,<dx>][,precise][,shift]
# (wheel lines, or two fingers in points), pinch:<x>,<y>,<m> (a pinch, + spreads), dblclick:<x>,<y>, graphstate:<file> (the
# graph's window and view, key=value lines; chip= the critical point chip hovered or focused, home= its home window).
# Games: gamekey:<chord>[:down|:up] and gamekeys:<text> (a key, one half of it, or text straight to the open game's window,
# which works with q calc in the background or the screen locked), gamestate:<file> (its title, frame, big, dark), closegame.
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
