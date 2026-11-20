import AppKit
import SwiftUI

// The web app's `--shadow-card` and `--control-shadow`, light and dark (src/index.css). The native cards drew the
// light values in dark too: a black hairline on a #252527 card (no edge at all) and a 6% shadow where the page has 30%.
enum Surfaces {
    private static func rgba(_ light: (CGFloat, CGFloat), _ dark: (CGFloat, CGFloat)) -> Color {
        Color(nsColor: NSColor(name: nil) { a in
            let (white, alpha) = a.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua ? dark : light
            return NSColor(srgbRed: white, green: white, blue: white, alpha: alpha)
        })
    }

    /// `--shadow-card`'s 0.5 px ring: black 10% / white 10%
    static let cardRing = rgba((0, 0.1), (1, 0.1))
    /// `--shadow-card`'s 1 px 3 px drop: black 6% / 30%
    static let cardShadow = rgba((0, 0.06), (0, 0.3))
    /// `--control-shadow`'s 0.5 px ring: black 12% / 30%
    static let controlRing = rgba((0, 0.12), (0, 0.3))
    /// `--control-shadow`'s 1 px 2 px drop: black 10% / 25%
    static let controlShadow = rgba((0, 0.1), (0, 0.25))
}
