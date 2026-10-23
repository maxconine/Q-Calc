import CoreGraphics
import Foundation

/// Standalone test for where a reference sheet opens. Not part of the app target.
///   swiftc macos/SheetPlace.swift macos/sheet_place_test.swift -o /tmp/qcalc-place-test && /tmp/qcalc-place-test
@main
enum SheetPlaceTest {
    static let screen = CGRect(x: 0, y: 0, width: 1512, height: 944)
    // the bar where it opens, 28% down
    static let bar = CGRect(x: 416, y: 627, width: 680, height: 72)
    static let table = CGSize(width: 832, height: 530)

    static func main() {
        below()
        beside()
        shorter()
        anywhere()
        moving()
        print("sheet place tests passed")
    }

    static func check(_ ok: Bool, _ what: String) {
        if !ok {
            fputs("failed: \(what)\n", stderr)
            exit(1)
        }
    }

    static func clear(_ r: CGRect, _ bar: CGRect, _ screen: CGRect, _ what: String) {
        check(!r.intersects(bar), "\(what): off the bar")
        check(screen.contains(r), "\(what): on the screen")
    }

    static func below() {
        let r = SheetPlace.beside(table, bar: bar, visible: screen)
        clear(r, bar, screen, "the table")
        check(r.maxY == bar.minY - SheetPlace.margin, "the table hangs a margin under the bar")
        check(r.midX == bar.midX, "centred under it")
        // a bar near a side edge: still under it, pulled onto the screen
        let side = CGRect(x: 20, y: 627, width: 680, height: 72)
        let s = SheetPlace.beside(table, bar: side, visible: screen)
        clear(s, side, screen, "under a bar at the left")
        check(s.minX == 0, "pulled in to the left edge")
    }

    static func beside() {
        // a bar dragged low on a wide screen: no room under it, so it goes to the right
        let wide = CGRect(x: 0, y: 0, width: 2560, height: 1400)
        let low = CGRect(x: 200, y: 200, width: 680, height: 72)
        let r = SheetPlace.beside(table, bar: low, visible: wide)
        clear(r, low, wide, "beside a low bar")
        check(r.minX == low.maxX + SheetPlace.margin, "to its right")
        // at the right edge it goes left
        let edge = CGRect(x: 1800, y: 200, width: 680, height: 72)
        let l = SheetPlace.beside(table, bar: edge, visible: wide)
        clear(l, edge, wide, "beside a bar at the right")
        check(l.maxX == edge.minX - SheetPlace.margin, "to its left")
    }

    static func shorter() {
        // an identity sheet too tall for the room under the bar gets shorter rather than covering it
        let sheet = CGSize(width: 736, height: 700)
        let r = SheetPlace.beside(sheet, bar: bar, visible: screen, give: .height(min: 220))
        clear(r, bar, screen, "a tall sheet")
        check(r.height == bar.minY - SheetPlace.margin, "as tall as the room under the bar")
        // the table scales instead, keeping its shape
        let small = CGRect(x: 0, y: 0, width: 1000, height: 700)
        let mid = CGRect(x: 160, y: 400, width: 680, height: 72)
        let t = SheetPlace.beside(table, bar: mid, visible: small, give: .scale(min: 0.6))
        clear(t, mid, small, "a scaled table")
        check(abs(t.width / t.height - table.width / table.height) < 0.001, "the same shape")
        check(t.maxY == mid.minY - SheetPlace.margin, "under the bar")
    }

    static func anywhere() {
        // a small screen where nothing fits and nothing may give: on the screen all the same
        let small = CGRect(x: 0, y: 0, width: 1000, height: 700)
        let mid = CGRect(x: 160, y: 400, width: 680, height: 72)
        let r = SheetPlace.beside(table, bar: mid, visible: small)
        check(small.contains(r), "a crowded screen keeps it on screen")
        check(r.size == table, "at its own size")
    }

    static func moving() {
        check(SheetPlace.needsMoving(CGRect(x: 400, y: 300, width: 832, height: 530), bar: bar, visible: screen), "over the bar")
        check(!SheetPlace.needsMoving(CGRect(x: 400, y: 20, width: 832, height: 530), bar: bar, visible: screen), "left where it was put")
        check(SheetPlace.needsMoving(CGRect(x: 3000, y: 20, width: 832, height: 530), bar: bar, visible: screen), "off the bar's screen")
    }
}
