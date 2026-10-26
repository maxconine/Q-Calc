import AppKit
import SwiftUI

// The bar's look, value for value from the web app's CSS (src/index.css :root and its dark block), so the native
// bar matches the one users have: same colours, type sizes, tracking, radii and spacing.
enum Theme {
    private static func dynamic(_ light: UInt32, _ dark: UInt32, alpha: CGFloat = 1, darkAlpha: CGFloat? = nil) -> NSColor {
        NSColor(name: nil) { appearance in
            let isDark = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            let hex = isDark ? dark : light
            return NSColor(srgbRed: CGFloat((hex >> 16) & 0xff) / 255, green: CGFloat((hex >> 8) & 0xff) / 255,
                           blue: CGFloat(hex & 0xff) / 255, alpha: isDark ? (darkAlpha ?? alpha) : alpha)
        }
    }

    // --bg, --bg-tape, --ink, --muted, --answer, --answer-hover, --accent, --accent-hover
    static let bg = dynamic(0xffffff, 0x2c2c2e)
    static let bgTape = dynamic(0xf5f5f7, 0x252527)
    static let ink = dynamic(0x1d1d1f, 0xf5f5f7)
    static let muted = dynamic(0x86868b, 0x98989d)
    static let answer = dynamic(0x1d1d1f, 0xf5f5f7)
    static let answerHover = dynamic(0x48484a, 0xd1d1d6)
    static let accent = dynamic(0x1d7a4c, 0x63d392)
    static let accentHover = dynamic(0x159a5a, 0x7ee2a8)
    // --line: rgba(0,0,0,.08) / rgba(255,255,255,.09)
    static let line = dynamic(0x000000, 0xffffff, alpha: 0.08, darkAlpha: 0.09)

    // type: --fs-display 24, --fs-answer-part 20, --fs-row 17, --fs-body 13, --fs-small 12, --fs-caption 11
    static let display: CGFloat = 24
    static let answerPart: CGFloat = 20
    static let row: CGFloat = 17
    static let body: CGFloat = 13
    static let small: CGFloat = 12
    static let caption: CGFloat = 11
    /// --track-display: -0.021em
    static func displayTracking(_ size: CGFloat) -> CGFloat { -0.021 * size }

    // --radius 12, --radius-card 10, --radius-s 6, --tape-row-height 36, --tape-visible 5, --gutter 16
    static let radius: CGFloat = 12
    static let radiusCard: CGFloat = 10
    static let radiusS: CGFloat = 6
    static let tapeRow: CGFloat = 36
    static let tapeVisible = 5
    static let gutter: CGFloat = 16
    static let composerHeight: CGFloat = 72
}

extension Color {
    static let qBg = Color(nsColor: Theme.bg)
    static let qBgTape = Color(nsColor: Theme.bgTape)
    static let qInk = Color(nsColor: Theme.ink)
    static let qMuted = Color(nsColor: Theme.muted)
    static let qAnswer = Color(nsColor: Theme.answer)
    static let qAnswerHover = Color(nsColor: Theme.answerHover)
    static let qAccent = Color(nsColor: Theme.accent)
    static let qAccentHover = Color(nsColor: Theme.accentHover)
    static let qLine = Color(nsColor: Theme.line)
    /// --fill-hover / --fill-selected: ink at 5% and 8%
    static let qFillHover = Color(nsColor: Theme.ink).opacity(0.05)
    static let qFillSelected = Color(nsColor: Theme.ink).opacity(0.08)
}
