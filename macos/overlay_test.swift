import CoreGraphics
import Foundation

/// Standalone test for the overlay's show check. Not part of the app target.
///   swiftc macos/OverlayHealth.swift macos/overlay_test.swift -o /tmp/qcalc-overlay-test && /tmp/qcalc-overlay-test
@main
enum OverlayTest {
    static let screen = CGRect(x: 0, y: 0, width: 1512, height: 944)
    static let other = CGRect(x: 1512, y: 0, width: 1920, height: 1080)

    static func main() {
        healthy()
        problems()
        recoveries()
        hotkey()
        focus()
        print("overlay tests passed")
    }

    static func check(_ ok: Bool, _ what: String) {
        if !ok {
            fputs("failed: \(what)\n", stderr)
            exit(1)
        }
    }

    static func shown() -> OverlaySnapshot {
        var s = OverlaySnapshot()
        s.frame = CGRect(x: 416, y: 627, width: 680, height: 72)
        s.targetScreen = screen
        s.screens = [screen, other]
        return s
    }

    static func healthy() {
        check(OverlayHealth.problems(shown()).isEmpty, "a shown overlay is fine")
        var loading = shown()
        loading.pageReady = false
        loading.painted = false
        loading.pageLoadingFor = 0.4
        check(OverlayHealth.problems(loading).isEmpty, "a page that just started loading isn't a problem yet")
        var fallback = shown()
        fallback.usesFallback = true
        fallback.painted = false
        check(OverlayHealth.problems(fallback).isEmpty, "the plain bar needs no paint answer")
    }

    static func problems() {
        var gone = shown()
        gone.ordered = false
        gone.onActiveSpace = false
        check(OverlayHealth.problems(gone) == [.orderedOut], "ordered out says only that")

        var elsewhere = shown()
        elsewhere.onActiveSpace = false
        elsewhere.windowServerOnScreen = false
        check(OverlayHealth.problems(elsewhere) == [.offActiveSpace, .windowServerOff], "on another space")

        var moved = shown()
        moved.frame.origin.x = 2000
        check(OverlayHealth.problems(moved) == [.wrongScreen], "on the other display")
        moved.frame.origin.x = 9000
        check(OverlayHealth.problems(moved) == [.offScreen], "off every display")
        var edge = shown()
        edge.frame.origin.x = screen.maxX - 500
        check(OverlayHealth.problems(edge).isEmpty, "mostly on the screen is on it")

        var stuck = shown()
        stuck.pageReady = false
        stuck.pageLoadingFor = 5
        check(OverlayHealth.problems(stuck) == [.pageLoading], "a stuck load")

        var blank = shown()
        blank.painted = false
        blank.alpha = 0
        check(OverlayHealth.problems(blank) == [.transparent, .notPainted], "transparent and undrawn")

        var empty = shown()
        empty.contentInPanel = false
        empty.painted = false
        check(OverlayHealth.problems(empty) == [.contentMissing], "no page in the panel")
    }

    static func recoveries() {
        let r = OverlayHealth.recovery
        check(r([], 0) == .none, "nothing wrong, nothing done")
        check(r([.orderedOut], 0) == .reorder, "ordered out is shown again")
        check(r([.offActiveSpace, .windowServerOff], 0) == .freshPanel, "another space gets a fresh panel")
        check(r([.notPainted], 0) == .wait, "an undrawn page gets a second look first")
        check(r([.notPainted], 1) == .freshPage, "then a fresh page")
        check(r([.notPainted, .orderedOut], 0) == .reorder, "the heavier fix wins")
        check(r([.pageLoading], 0) == .freshPage, "a stuck load reloads")
        check(r([.orderedOut], 1) == .freshPage, "the same fix isn't tried twice")
        check(r([.offActiveSpace], 2) == .none, "after two recoveries only the page can still be swapped")
        check(r([.notPainted], 2) == .fallback, "a page that never draws gives way to the plain bar")
        check(r([.notPainted], OverlayHealth.attempts) == .none, "and then it stops")
    }

    static func hotkey() {
        check(OverlayHealth.hotkeyHides(shown()), "a visible overlay hides")
        var elsewhere = shown()
        elsewhere.onActiveSpace = false
        check(!OverlayHealth.hotkeyHides(elsewhere), "one on another space is brought here instead")
        var hidden = shown()
        hidden.appHidden = true
        check(!OverlayHealth.hotkeyHides(hidden), "one in a hidden app is shown")
        hidden = shown()
        hidden.ordered = false
        check(!OverlayHealth.hotkeyHides(hidden), "one that's ordered out is shown")
    }

    static func focus() {
        check(!OverlayHealth.focusLossHides(sinceShow: 0.1), "focus taken during a show keeps it up")
        check(OverlayHealth.focusLossHides(sinceShow: 2), "focus lost later hides it")
    }
}
