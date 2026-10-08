import CoreGraphics
import Foundation

// what the overlay looked like a moment after a show. the controller gathers it from appkit and the
// window server, and the verdict lives here so it can be tested without a window
struct OverlaySnapshot {
    // the panel is ordered in (NSWindow.isVisible)
    var ordered = true
    var appHidden = false
    var onActiveSpace = true
    // kCGWindowIsOnscreen from the window server
    var windowServerOnScreen = true
    // NSWindow.occlusionState contains .visible
    var occlusionVisible = true
    var alpha: Double = 1
    var frame = CGRect.zero
    // the screen picked when the show began, the one with the pointer
    var targetScreen: CGRect?
    var screens: [CGRect] = []
    // the page, or the swiftui fallback, is the panel's content
    var contentInPanel = true
    var usesFallback = false
    var pageReady = true
    var pageLoadingFor: TimeInterval = 0
    // the page answered from a painted frame for this show
    var painted = true
}

enum OverlayProblem: String, CaseIterable {
    case orderedOut = "ordered out"
    case appHidden = "app hidden"
    case offActiveSpace = "not on the active space"
    case windowServerOff = "window server has it off screen"
    case occluded = "occluded"
    case transparent = "still transparent"
    case offScreen = "frame off the screen"
    case wrongScreen = "not on the screen with the pointer"
    case contentMissing = "page not in the panel"
    case pageLoading = "page still loading"
    case notPainted = "page didn't draw"
}

// the least that could put it right, from cheapest up
enum OverlayRecovery: Int, Comparable {
    case none
    // look again a moment later before doing anything heavy
    case wait
    // order the same panel in again, placed and faded in afresh
    case reorder
    // a new panel around the same page
    case freshPanel
    // a new panel and a reloaded page
    case freshPage
    // the plain swiftui bar, when the page can't be made to draw
    case fallback

    static func < (a: OverlayRecovery, b: OverlayRecovery) -> Bool { a.rawValue < b.rawValue }

    var name: String {
        switch self {
        case .none: return "nothing"
        case .wait: return "waiting"
        case .reorder: return "showing it again"
        case .freshPanel: return "a fresh panel"
        case .freshPage: return "a fresh panel and page"
        case .fallback: return "the plain bar"
        }
    }
}

enum OverlayHealth {
    // a page that hasn't loaded by now is stuck, not slow
    static let pageLoadLimit: TimeInterval = 3
    // focus lost this soon after a show was taken by the system (a space switch, a launch), not by the user
    static let focusGrace: TimeInterval = 0.8
    // checks after the last recovery only log
    static let attempts = 3

    static func problems(_ s: OverlaySnapshot) -> [OverlayProblem] {
        var out: [OverlayProblem] = []
        if !s.ordered { out.append(.orderedOut) }
        if s.appHidden { out.append(.appHidden) }
        // the rest describe a window that's ordered in
        guard s.ordered else { return out }
        if !s.onActiveSpace { out.append(.offActiveSpace) }
        if !s.windowServerOnScreen { out.append(.windowServerOff) }
        if !s.occlusionVisible { out.append(.occluded) }
        if s.alpha < 0.99 { out.append(.transparent) }
        if let target = s.targetScreen, !mostlyOn(s.frame, target) {
            out.append(s.screens.contains { mostlyOn(s.frame, $0) } ? .wrongScreen : .offScreen)
        }
        if !s.contentInPanel {
            out.append(.contentMissing)
        } else if !s.usesFallback {
            if !s.pageReady {
                if s.pageLoadingFor > pageLoadLimit { out.append(.pageLoading) }
            } else if !s.painted {
                out.append(.notPainted)
            }
        }
        return out
    }

    // at least half the frame on the screen; an empty frame is on none
    static func mostlyOn(_ frame: CGRect, _ screen: CGRect) -> Bool {
        let shown = frame.intersection(screen)
        guard !shown.isNull, frame.width > 0, frame.height > 0 else { return false }
        return shown.width * shown.height >= frame.width * frame.height * 0.5
    }

    // attempt 0 is the first check after the show; each later one follows a recovery
    static func recovery(for problems: [OverlayProblem], attempt: Int) -> OverlayRecovery {
        guard !problems.isEmpty, attempt < attempts else { return .none }
        let page = problems.contains(.notPainted) || problems.contains(.pageLoading)
        if attempt >= 2 { return page ? .fallback : .none }
        var need = OverlayRecovery.none
        for problem in problems {
            let step: OverlayRecovery
            switch problem {
            case .orderedOut, .appHidden, .transparent, .wrongScreen, .offScreen:
                step = .reorder
            case .offActiveSpace, .windowServerOff, .occluded, .contentMissing:
                step = .freshPanel
            case .notPainted:
                // a page woken from app nap can be slow to answer; reloading it costs the user's draft
                step = attempt == 0 ? .wait : .freshPage
            case .pageLoading:
                step = .freshPage
            }
            need = max(need, step)
        }
        // the same fix twice is a wasted look; go one heavier
        if attempt == 1, need == .reorder || need == .freshPanel { need = .freshPage }
        return need
    }

    // the hotkey hides only an overlay the user can see; one that's ordered in somewhere else is shown again
    static func hotkeyHides(_ s: OverlaySnapshot) -> Bool {
        s.ordered && !s.appHidden && s.onActiveSpace
    }

    // losing focus is a click away, unless it came just after the show
    static func focusLossHides(sinceShow: TimeInterval) -> Bool {
        sinceShow >= focusGrace
    }

    // diary line for a failed check
    static func summary(_ problems: [OverlayProblem]) -> String {
        problems.map(\.rawValue).joined(separator: ", ")
    }
}
