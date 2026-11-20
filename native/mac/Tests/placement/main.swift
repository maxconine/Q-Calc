// Headless checks for where the bar's window goes across displays (native/mac/scripts/test-placement.sh):
// BarPlacement.swift's width fitting and pulling a frame back onto a screen. CoreGraphics only: no app, no window.

import CoreGraphics

var failures = 0

func check<T: Equatable>(_ what: String, _ got: T, _ want: T) {
    let ok = got == want
    print(ok ? "ok   \(what)" : "FAIL \(what)\n     got  \(got)\n     want \(want)")
    if !ok { failures += 1 }
}

let laptop = CGRect(x: 0, y: 0, width: 1440, height: 875)          // visible frame, menu bar and Dock taken off
let external = CGRect(x: 1440, y: -200, width: 2560, height: 1415)

// widths
check("fits on a big screen as dragged", BarPlacement.fittedWidth(1800, minimum: 340, visible: external), 1800)
check("a width dragged on a big display fits a laptop", BarPlacement.fittedWidth(1800, minimum: 340, visible: laptop), 1400)
check("never under the minimum", BarPlacement.fittedWidth(200, minimum: 340, visible: laptop), 340)
check("a tiny screen still gets the minimum", BarPlacement.fittedWidth(680, minimum: 340, visible: CGRect(x: 0, y: 0, width: 300, height: 500)), 340)
check("no screen: as asked", BarPlacement.fittedWidth(680, minimum: 340, visible: nil), 680)

// pulled back on
let inside = CGRect(x: 100, y: 300, width: 680, height: 64)
check("a bar inside is left alone", BarPlacement.keptOnScreen(inside, minimumWidth: 340, visible: laptop), inside)
check("hanging off the right edge moves in",
      BarPlacement.keptOnScreen(CGRect(x: 1000, y: 300, width: 680, height: 64), minimumWidth: 340, visible: laptop),
      CGRect(x: 760, y: 300, width: 680, height: 64))
check("under the Dock moves up",
      BarPlacement.keptOnScreen(CGRect(x: 100, y: -30, width: 680, height: 200), minimumWidth: 340, visible: laptop),
      CGRect(x: 100, y: 0, width: 680, height: 200))
check("too wide for the screen narrows and moves in",
      BarPlacement.keptOnScreen(CGRect(x: 200, y: 300, width: 1800, height: 64), minimumWidth: 340, visible: laptop),
      CGRect(x: 40, y: 300, width: 1400, height: 64))

// which screen
check("inside one visible frame: on screen", BarPlacement.isOffScreen(inside, visibleFrames: [laptop, external]), false)
check("straddling two displays: needs pulling", BarPlacement.isOffScreen(CGRect(x: 1200, y: 300, width: 680, height: 64), visibleFrames: [laptop, external]), true)
check("on the external display", BarPlacement.mostlyOn(CGRect(x: 1600, y: 300, width: 680, height: 64), screens: [laptop, external]), Optional(1))
check("straddling: the screen with more of it", BarPlacement.mostlyOn(CGRect(x: 1200, y: 300, width: 680, height: 64), screens: [laptop, external]), Optional(1))
check("its display unplugged: on none", BarPlacement.mostlyOn(CGRect(x: 5000, y: 300, width: 680, height: 64), screens: [laptop]), Optional<Int>.none)
check("touching an edge only is not on it", BarPlacement.mostlyOn(CGRect(x: 1440, y: 300, width: 680, height: 64), screens: [laptop]), Optional<Int>.none)

print(failures == 0 ? "all placement checks pass" : "\(failures) placement check(s) failed")
if failures > 0 { exit(1) }
