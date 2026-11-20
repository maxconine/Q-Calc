#!/bin/zsh
# Driven checks for the graph panel's pointer and trackpad (drive.sh drag:/scroll:/graphstate:), on a running dev build
# (QCALC_DEV_ID for a re-signed copy): a drag pans with the pointer, a wheel or two fingers zoom about the pointer in x
# and y (y zoomed about the middle before), a 3D drag orbits and tilts within its limits, ⇧ pans in 3D, `and` and
# polar graphs, an implicit surface; the head's zoom buttons, a pinch (pinch:) and a double-click (dblclick:, QA item 85).
# Chip hover (SwiftUI onHover) isn't drivable: synthetic mouse events never reach it. Points are in the bar from its top-left, for the default 680-point-wide bar.
#   QCALC_DEV_ID=<bundle id> zsh native/mac/scripts/test-graph.sh
drive="$(cd "$(dirname "$0")" && pwd)/drive.sh"
state="$(mktemp -t qcalc-graph)"
fails=0
get() { zsh "$drive" "graphstate:$state"; grep "^$1=" "$state" | cut -d= -f2-; }
check() { if [[ "$2" == "$3" ]]; then echo "ok   $1"; else echo "FAIL $1: got '$2', want '$3'"; fails=$((fails + 1)); fi }
near() { python3 -c "import sys; a,b,t=map(float,sys.argv[1:]); print('yes' if abs(a-b)<=t else 'no %g vs %g'%(a,b))" "$@"; }
lo() { print -r -- "${1%%…*}"; }
hi() { print -r -- "${1##*…}"; }

zsh "$drive" show "point:off" "type:graph x^2" sleep:1.5
x0="$(get x)"
zsh "$drive" "drag:350,200,450,200"
x1="$(get x)"
check "a drag right moves the window left" "$(python3 -c "print('yes' if $(lo "$x1") < $(lo "$x0") else 'no')")" yes
zsh "$drive" "scroll:350,200,-30,0,precise"
y0="$(get fixedY)" x2="$(get x)"
# a wheel in then out at the same spot puts both windows back (the pivot stays under the pointer)
zsh "$drive" "scroll:80,150,3" "scroll:80,150,-3"
check "wheel in/out about the pointer: x back" "$(near "$(lo "$(get x)")" "$(lo "$x2")" 1e-6)" yes
check "wheel in/out about the pointer: y back" "$(near "$(hi "$(get fixedY)")" "$(hi "$y0")" 1e-6)" yes
# near the top left, the top left hardly moves while the far side does
zsh "$drive" "scroll:80,130,3"
check "zoom near the top left keeps the top in place" "$(near "$(hi "$(get fixedY)")" "$(hi "$y0")" "$(python3 -c "print(($(hi "$y0") - $(lo "$y0")) * 0.12)")")" yes

# QA item 85: the head's zoom buttons, a pinch and a double-click. Out twice then in twice is home again (reset greys;
# it had drifted a few ulps and stayed lit); a pinch zooms about the pointer (it zoomed about the middle) and out then
# back in puts the window back; a double-click resets
zsh "$drive" "type:graph x^2 - 4" sleep:1.5
home="$(get x)"
# the head sits 15 points lower while a hint shows under the line: press where zoom out is, else below
zsh "$drive" "click:468,93"
[[ "$(get x)" == "$home" ]] && zoomY=108 || { zoomY=93; zsh "$drive" "click:502,$zoomY"; }
check "the zoom buttons zoom about the middle" "$(zsh "$drive" "click:468,$zoomY"; get x)" "-3.75…3.75"
zsh "$drive" "click:468,$zoomY" "click:502,$zoomY" "click:502,$zoomY"
check "zoom out twice, in twice: home again" "$(get atHome)" true
check "… at the very window it opened on" "$(get x)" "$home"
zsh "$drive" "pinch:150,160,0.3"
check "a pinch near the left zooms about the pointer" "$(python3 -c "print('yes' if $(hi "$(get x)") < 2 and $(lo "$(get x)") > -3 else 'no')")" yes
zsh "$drive" "pinch:150,160,-0.3"
check "a pinch out and back in is home again" "$(get atHome)" true
zsh "$drive" "drag:300,200,380,230" "scroll:400,200,4"
check "pan and zoom leave home" "$(get atHome)" false
zsh "$drive" "dblclick:300,200"
check "a double-click resets" "$(get atHome)" true

zsh "$drive" "type:graph sin(x) and cos(x)" sleep:1.5
check "'and' graphs a second equation" "$(get layers | grep -c 'cos(x)|curve')" 1
zsh "$drive" "type:graph r = 1 + cos(θ)" sleep:1.5
check "polar r = 1 + cos θ is an implicit curve" "$(get kind)" implicit
zsh "$drive" "scroll:600,150,5"
check "zooming an implicit curve off-centre moves its y centre" "$([[ "$(get yCenter)" != nil ]] && echo yes || echo no)" yes

zsh "$drive" "type:graph z = x^2 - y^2" sleep:1.5
zsh "$drive" "drag:300,250,400,250"
check "a 3D drag turns the surface" "$(near "$(get yaw)" -1.146830866805696 0.01)" yes
zsh "$drive" "drag:300,150,300,440"
check "tilt stops at the top view" "$(get pitch)" 1.45
zsh "$drive" "drag:300,440,300,130" "drag:300,440,300,130"
check "tilt stops above the floor" "$(get pitch)" 0.05
x3="$(get x)"
zsh "$drive" "drag:300,250,350,250,shift"
check "⇧ drag in 3D pans" "$([[ "$(get x)" != "$x3" ]] && echo yes || echo no)" yes
check "⇧ drag in 3D pans, not orbits" "$(near "$(get yaw)" -1.146830866805696 0.01)" yes

zsh "$drive" "type:graph x^2+y^2+z^2=1" sleep:2
check "an implicit surface opens in 3D" "$(get view3D)" true
check "an implicit surface frames the sphere" "$(get x)" "-1.4…1.4"

zsh "$drive" "type:" hide
rm -f "$state"
exit $fails
