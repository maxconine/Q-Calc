import AppKit
import SwiftUI

// The pieces every game panel is built from, as src/index.css draws them (.pong, .pong-head, .pong-button,
// .pong-note, .pong-link, .pong-code…).

/// `.pong`: the panel's padding (12 above and below, the 16 gutter at the sides).
struct GamePanel<Content: View>: View {
    let content: Content
    init(@ViewBuilder content: () -> Content) { self.content = content() }

    var body: some View {
        content
            .padding(.vertical, GameMetrics.sp3)
            .padding(.horizontal, GameMetrics.gutter)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// `.pong-button`: 34 high, at least 112 wide, the control fill and shadow; `on` rings it in accent;
/// `choice` is the lobby's first screen's bigger pair (46 high, 148 wide, 17 px).
struct PongButton: View {
    let title: String
    let kbd: String?
    var on = false
    var choice = false
    let action: () -> Void
    @Environment(\.gameColors) private var colors
    @State private var hover = false

    init(_ title: String, kbd: String? = nil, on: Bool = false, choice: Bool = false, action: @escaping () -> Void) {
        self.title = title
        self.kbd = kbd
        self.on = on
        self.choice = choice
        self.action = action
    }

    var body: some View {
        Button(action: action) {
            HStack(spacing: GameMetrics.sp2) {
                Text(title).font(.game(choice ? GameMetrics.fsRow : GameMetrics.fsBody)).foregroundColor(colors.ink.color)
                if let kbd { Text(kbd).font(.game(GameMetrics.fsCaption)).foregroundColor(colors.muted.color) }
            }
            .padding(.horizontal, 6)
            .frame(minWidth: choice ? 148 : 112, minHeight: choice ? 46 : 34, maxHeight: choice ? 46 : 34)
            .background(RoundedRectangle(cornerRadius: GameMetrics.radiusS)
                .fill((hover ? colors.control.mix(0.94, colors.ink) : colors.control).color))
            .boxShadow(colors.controlShadow, radius: GameMetrics.radiusS)
            .insetRing(on ? colors.accent.mix(0.55, .clear) : .clear, width: 1.5, radius: GameMetrics.radiusS)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hover = $0 }
    }
}

/// `.pong-head`: the title, the line beside it, and at the right the full-screen button and esc.
struct LobbyHead: View {
    let name: String
    let sub: String
    @EnvironmentObject private var shell: GameShellModel
    @Environment(\.gameColors) private var colors

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: GameMetrics.sp3) {
            Text(name).font(.game(GameMetrics.fsBody, .semibold)).foregroundColor(colors.ink.color)
            Text(sub).font(.game(GameMetrics.fsSmall)).foregroundColor(colors.muted.color)
            Spacer(minLength: 0)
            HStack(spacing: 0) {
                FullscreenButton(on: shell.fullscreen) { shell.toggleFullscreen() }
                    .alignmentGuide(.firstTextBaseline) { d in d[VerticalAlignment.center] + 4 }
                EscButton { shell.escape() }
                    .padding(.trailing, -GameMetrics.sp2)
            }
        }
    }
}

/// `.pong-leave`: the small "esc" at the head's right.
struct EscButton: View {
    var title = "esc"
    let action: () -> Void
    @Environment(\.gameColors) private var colors
    @State private var hover = false

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.game(GameMetrics.fsSmall))
                .foregroundColor(hover ? colors.ink.color : colors.muted.color)
                .padding(.vertical, 3)
                .padding(.horizontal, GameMetrics.sp2)
                .background(RoundedRectangle(cornerRadius: GameMetrics.radiusS).fill(hover ? colors.fillHover.color : .clear))
                .padding(.vertical, -3)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hover = $0 }
    }
}

/// `.pong-line`, 13 px ink, centred.
struct PongLine: View {
    let text: Text
    init(_ s: String) { text = Text(s) }
    init(text: Text) { self.text = text }
    @Environment(\.gameColors) private var colors

    var body: some View {
        text.font(.game(GameMetrics.fsBody)).foregroundColor(colors.ink.color).multilineTextAlignment(.center)
    }
}

/// `.pong-note`, 12 px muted, centred. `wait` pulses it (`.pong-wait`).
struct PongNote: View {
    let text: Text
    var wait = false
    @Environment(\.gameColors) private var colors

    init(_ s: String, wait: Bool = false) {
        text = Text(s)
        self.wait = wait
    }

    init(text: Text, wait: Bool = false) {
        self.text = text
        self.wait = wait
    }

    var body: some View {
        text.font(.game(GameMetrics.fsSmall)).foregroundColor(colors.muted.color).multilineTextAlignment(.center)
            .modifier(PongWait(on: wait))
    }
}

/// The games' `@media (prefers-reduced-motion: reduce)` rules: Reduce Motion stops the pulse, the caret's blink,
/// equal's shake and flip, and crack's shake, click and pop, as the page does.
enum GameMotion {
    static var reduced: Bool { NSWorkspace.shared.accessibilityDisplayShouldReduceMotion }
}

/// `.pong-wait`: opacity down to 0.45 and back every 1.6 s.
struct PongWait: ViewModifier {
    let on: Bool
    @State private var dim = false

    func body(content: Content) -> some View {
        // `@media (prefers-reduced-motion: reduce) { .pong-wait { animation: none } }`
        if on, !GameMotion.reduced {
            TimelineView(.animation(minimumInterval: 1 / 30)) { ctx in
                let t = ctx.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 1.6) / 1.6
                // ease-in-out between 1 and 0.45, lowest half way
                let k = 0.5 - 0.5 * cos(2 * .pi * t)
                content.opacity(1 - 0.55 * k)
            }
        } else {
            content
        }
    }
}

/// `.pong-link`: a word in a note that's also a button; it takes the note's colour, ink on hover.
struct PongLink: View {
    let title: String
    var ink = false
    let action: () -> Void
    @Environment(\.gameColors) private var colors
    @State private var hover = false

    var body: some View {
        Button(action: action) {
            Text(title).foregroundColor(hover || ink ? colors.ink.color : colors.muted.color)
        }
        .buttonStyle(.plain)
        .onHover { hover = $0 }
    }
}

/// `.pong-code`: digits in 44 × 54 tiles. `entry` frames them for typing, with the caret's tile ringed; `busy`
/// greys them while joining.
struct PongCode: View {
    let digits: [String]
    var entry = false
    var busy = false
    var caret: Int?
    @Environment(\.gameColors) private var colors

    var body: some View {
        HStack(spacing: GameMetrics.sp2) {
            ForEach(Array(digits.enumerated()), id: \.offset) { i, d in
                Text(d)
                    .font(.gameDigits(30, .semibold))
                    .foregroundColor(busy ? colors.muted.color : entry ? colors.ink.color : colors.accent.color)
                    .frame(width: 44, height: 54)
                    .background(RoundedRectangle(cornerRadius: GameMetrics.radiusS).fill(colors.bgTape.color))
                    .insetRing(entry ? (caret == i ? colors.accent.mix(0.55, .clear) : colors.line) : .clear, width: 1, radius: GameMetrics.radiusS)
            }
        }
    }
}

/// `.pong-score`: the two names either side of the score, the side you play in accent.
struct PongScore: View {
    let left: String
    let right: String
    let score: (Int, Int)
    var leftMine: Bool
    var rightMine: Bool
    /// ultimate colours both names: you in accent, them orange
    var leftColor: Color? = nil
    var rightColor: Color? = nil
    @Environment(\.gameColors) private var colors

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: GameMetrics.sp3) {
            Text(left)
                .font(.game(GameMetrics.fsSmall))
                .foregroundColor(leftColor ?? (leftMine ? colors.accent.color : colors.muted.color))
                .lineLimit(1).truncationMode(.tail)
                .frame(maxWidth: .infinity, alignment: .leading)
            (Text("\(score.0) ") + Text("–").fontWeight(.regular).foregroundColor(colors.muted.color) + Text(" \(score.1)"))
                .font(.gameDigits(GameMetrics.fsDisplay, .semibold))
                .tracking(GameMetrics.displayTracking(GameMetrics.fsDisplay))
                .foregroundColor(colors.ink.color)
                .fixedSize()
            Text(right)
                .font(.game(GameMetrics.fsSmall))
                .foregroundColor(rightColor ?? (rightMine ? colors.accent.color : colors.muted.color))
                .lineLimit(1).truncationMode(.tail)
                .frame(maxWidth: .infinity, alignment: .trailing)
        }
    }
}
