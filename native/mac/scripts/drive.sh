#!/bin/zsh
# Sends commands to a running dev build (Q Calc Dev only; release builds don't listen), one per argument:
#   zsh native/mac/scripts/drive.sh show "type:2^10" enter "snap:/tmp/shots/light" hide
# Commands: show, hide, settings, theme:<system|light|dark>, type:<text>, enter, up, down, tab, esc,
# key:<chord> (a real key event: key:enter, key:cmd+w), periodic, identities, card[:<from>], ask[:<from>],
# paste[:<system field>], snap:<path prefix> (a PNG per visible window), sleep:<seconds>.
for cmd in "$@"; do
  if [[ "$cmd" == sleep:* ]]; then
    sleep "${cmd#sleep:}"
    continue
  fi
  osascript -l JavaScript -e "
    ObjC.import('Foundation');
    \$.NSDistributedNotificationCenter.defaultCenter.postNotificationNameObjectUserInfoDeliverImmediately(
      'com.maxconine.qcalc.native-dev.command', $(printf '%s' "$cmd" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))'), \$(), true)" >/dev/null
  sleep 0.35
done
