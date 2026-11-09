import AppKit
import SwiftUI

// Radical.tsx's meaning in plain SwiftUI: a root over what it takes (a bracketed group, or a run of digits, letters and
// π), so `2sqrt(2)` reads 2√2̅. Native only: the sign and bar are one stroke, as Desmos draws them, not a glyph and a
// rectangle that meet at a seam. The Typst renderer replaces this for full maths.
struct RadicalText: View {
    let text: String
    let size: CGFloat
    var tracking: CGFloat = 0
    var monospaced = false

    private enum Run { case plain(String), root(String, cube: Bool) }

    private var runs: [Run] {
        var out: [Run] = []
        var plain = ""
        var chars = Array(text)
        var i = 0
        while i < chars.count {
            guard chars[i] == "√" || chars[i] == "∛", i + 1 < chars.count else {
                plain.append(chars[i]); i += 1; continue
            }
            var j = i + 1
            if chars[j] == "(" {
                var depth = 0
                while j < chars.count {
                    if chars[j] == "(" { depth += 1 }
                    if chars[j] == ")" { depth -= 1; if depth == 0 { j += 1; break } }
                    j += 1
                }
            } else {
                while j < chars.count, chars[j].isLetter || chars[j].isNumber || chars[j] == "." || chars[j] == "π" { j += 1 }
            }
            guard j > i + 1 else { plain.append(chars[i]); i += 1; continue }
            if !plain.isEmpty { out.append(.plain(plain)) }
            plain = ""
            out.append(.root(String(chars[(i + 1)..<j]), cube: chars[i] == "∛"))
            i = j
        }
        if !plain.isEmpty { out.append(.plain(plain)) }
        chars = []
        return out
    }

    var body: some View {
        HStack(spacing: 0) {
            ForEach(Array(runs.enumerated()), id: \.offset) { _, run in
                switch run {
                case let .plain(s): styled(s)
                case let .root(s, cube):
                    styled(s)
                        .padding(.leading, RootSign.width(size))
                        .overlay {
                            RootStroke(size: size)
                                .stroke(style: StrokeStyle(lineWidth: RootSign.lineWidth(size), lineCap: .round, lineJoin: .round))
                        }
                        .overlay(alignment: .topLeading) {
                            if cube {
                                Text("3").font(.system(size: size * RootSign.indexScale))
                                    .offset(y: RootSign.indexOrigin(left: 0, baseline: RootSign.baseline(size), size: size).y)
                            }
                        }
                }
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(text.replacingOccurrences(of: "√", with: "root "))
    }

    private func styled(_ s: String) -> Text {
        Text(s)
            .font(monospaced ? .system(size: size).monospacedDigit() : .system(size: size))
            .tracking(tracking)
    }
}

/// The root's one line: a small tick up to the hook, down to the foot, up to the top, and on over what it takes. The
/// input field strokes it with AppKit and the answers with `RootStroke`, from the same points.
enum RootSign {
    static let indexScale: CGFloat = 0.42

    static func lineWidth(_ size: CGFloat) -> CGFloat { max(1, size / 17) }

    static func width(_ size: CGFloat) -> CGFloat { size * 0.66 }

    /// How far a line of text's baseline is from its top
    static func baseline(_ size: CGFloat) -> CGFloat {
        let font = NSFont.systemFont(ofSize: size)
        return ceil(font.ascender + abs(font.leading) / 2)
    }

    /// Top-left is the origin, y grows down. `left` and `width` are the sign's own space, `end` where the bar stops.
    static func points(left: CGFloat, width: CGFloat, end: CGFloat, baseline: CGFloat, size: CGFloat) -> [CGPoint] {
        let font = NSFont.systemFont(ofSize: size)
        // clear of what's under it, as Desmos leaves it
        return points(left: left, width: width, end: end, top: baseline - font.capHeight - size * 0.2, foot: baseline + size * 0.02)
    }

    /// How far a root's bar (its stroke's top edge) is over the baseline of a plain radicand
    static func top(_ size: CGFloat) -> CGFloat {
        NSFont.systemFont(ofSize: size).capHeight + size * 0.2 + lineWidth(size) / 2
    }

    /// The same line between a given top and foot: a root over a stacked fraction reaches from above its numerator to
    /// below its denominator
    static func points(left: CGFloat, width: CGFloat, end: CGFloat, top: CGFloat, foot: CGFloat) -> [CGPoint] {
        let height = foot - top
        return [
            CGPoint(x: left + width * 0.1, y: foot - height * 0.36),
            CGPoint(x: left + width * 0.26, y: foot - height * 0.44),
            CGPoint(x: left + width * 0.5, y: foot),
            CGPoint(x: left + width * 0.86, y: top),
            CGPoint(x: end, y: top),
        ]
    }

    /// Where a cube root's small 3 goes: in the crook of the sign
    static func indexOrigin(left: CGFloat, baseline: CGFloat, size: CGFloat) -> CGPoint {
        CGPoint(x: left, y: baseline - NSFont.systemFont(ofSize: size).capHeight - size * 0.24)
    }
}

/// `RootSign`'s line, across the view it overlays
private struct RootStroke: Shape {
    let size: CGFloat

    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.addLines(RootSign.points(left: rect.minX, width: RootSign.width(size), end: rect.maxX,
                                      baseline: rect.minY + RootSign.baseline(size), size: size))
        return path
    }
}
