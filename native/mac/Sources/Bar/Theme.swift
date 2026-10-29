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

    /// dev builds (`drive.sh contrast:on`) draw as if Increase Contrast were on
    static var devContrast = false
    static var increaseContrast: Bool { devContrast || NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast }

    /// the bar's 90% (`--bg` at .9), solid while Reduce Transparency or Increase Contrast is on (native only)
    static var panelOpacity: Double {
        NSWorkspace.shared.accessibilityDisplayShouldReduceTransparency || increaseContrast ? 1 : 0.9
    }

    // --bg, --bg-tape, --ink, --muted, --answer, --answer-hover, --accent, --accent-hover
    static let bg = dynamic(0xffffff, 0x2c2c2e)
    static let bgTape = dynamic(0xf5f5f7, 0x252527)
    static let ink = dynamic(0x1d1d1f, 0xf5f5f7)
    // Increase Contrast (native only; the page has no such setting) swaps the two that fall short: muted is 3.3:1
    // on the light tape (5.5:1 with it, 6.3:1 in dark), and the hairlines at 8% barely show
    private static let mutedBase = dynamic(0x86868b, 0x98989d)
    private static let mutedContrast = dynamic(0x636366, 0xaeaeb2)
    static var muted: NSColor { increaseContrast ? mutedContrast : mutedBase }
    static func muted(contrast: Bool) -> NSColor { contrast ? mutedContrast : mutedBase }
    static let answer = dynamic(0x1d1d1f, 0xf5f5f7)
    static let answerHover = dynamic(0x48484a, 0xd1d1d6)
    static let accent = dynamic(0x1d7a4c, 0x63d392)
    static let accentHover = dynamic(0x159a5a, 0x7ee2a8)
    /// units, functions and constants in the line, as Settings › Units and functions picks: dark is a step past ink
    static let wordDark = dynamic(0x000000, 0xffffff)
    static func unitColour(_ style: String) -> NSColor {
        switch style {
        case "dark": return wordDark
        case "grey": return muted
        default: return accent
        }
    }
    /// a known word: the unit colour mixed 85% with ink (green only; dark and grey are the same for both)
    static func wordColour(_ style: String) -> NSColor {
        style == "green" ? (accent.blended(withFraction: 0.15, of: ink) ?? accent) : unitColour(style)
    }
    // --line: rgba(0,0,0,.08) / rgba(255,255,255,.09)
    private static let lineBase = dynamic(0x000000, 0xffffff, alpha: 0.08, darkAlpha: 0.09)
    private static let lineContrast = dynamic(0x000000, 0xffffff, alpha: 0.3)
    static var line: NSColor { increaseContrast ? lineContrast : lineBase }
    static func line(contrast: Bool) -> NSColor { contrast ? lineContrast : lineBase }

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
    // two colours each, picked when a view draws (Increase Contrast), so SwiftUI sees a new colour when it flips
    private static let qMutedBase = Color(nsColor: Theme.muted(contrast: false))
    private static let qMutedContrast = Color(nsColor: Theme.muted(contrast: true))
    static var qMuted: Color { Theme.increaseContrast ? qMutedContrast : qMutedBase }
    static let qAnswer = Color(nsColor: Theme.answer)
    static let qAnswerHover = Color(nsColor: Theme.answerHover)
    static let qAccent = Color(nsColor: Theme.accent)
    static let qAccentHover = Color(nsColor: Theme.accentHover)
    private static let qLineBase = Color(nsColor: Theme.line(contrast: false))
    private static let qLineContrast = Color(nsColor: Theme.line(contrast: true))
    static var qLine: Color { Theme.increaseContrast ? qLineContrast : qLineBase }
    /// --fill-hover / --fill-selected: ink at 5% and 8%
    static let qFillHover = Color(nsColor: Theme.ink).opacity(0.05)
    // Increase Contrast: the picked row and the pressed fills read at 16% rather than 8%
    private static let qFillSelectedBase = Color(nsColor: Theme.ink).opacity(0.08)
    private static let qFillSelectedContrast = Color(nsColor: Theme.ink).opacity(0.16)
    static var qFillSelected: Color { Theme.increaseContrast ? qFillSelectedContrast : qFillSelectedBase }
}

extension View {
    /// A plain TextField's prompt in the page's `::placeholder` colour (muted), drawn here: SwiftUI hands AppKit the
    /// prompt without its colour, so AppKit used its own placeholder grey (black at 25%, about 1.8:1 on white) and
    /// `drive.sh snap` pictures drew it in the text colour. Put it right after `.font`, before any padding, and
    /// leave the prompt empty. `font` overrides the system font at `size`.
    func mutedPrompt(_ text: String, shown: Bool, size: CGFloat, font: Font? = nil, color: Color = .qMuted,
                     kerning: CGFloat = 0) -> some View {
        overlay(alignment: .leading) {
            if shown {
                Text(text)
                    .font(font ?? .system(size: size))
                    .kerning(kerning)
                    .foregroundStyle(color)
                    .lineLimit(1)
                    .allowsHitTesting(false)
                    .accessibilityHidden(true)
            }
        }
    }
}
