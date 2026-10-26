import AppKit
import SwiftUI

// Typeset maths. Today the web view compiles Typst to SVG (typstRender.ts); natively, `qcalc-typst` (another
// agent's crate, ARCHITECTURE.md §6) renders the Typst source to an RGBA pixmap. This is the hook it plugs into:
// a renderer that turns the input line into an image at the screen's scale, or nil to fall back to plain text.
//
// To plug Typst in: write `TypstMathRenderer: MathRenderer` that calls the engine's `typst_source(expr, answer,
// solved_for)` and the crate's render function, wrap the pixmap in a CGImage, and return it from
// `BarModel.math`. Rendering must happen off the main thread (the preview row asks through `renderAsync`).
protocol MathRenderer {
    /// An image of the line typeset, or nil when this renderer has nothing better than text.
    func render(expression: String, answer: String, scale: CGFloat, dark: Bool) -> CGImage?
    /// Plain text for the same line, used whenever `render` gives nil.
    func plain(expression: String) -> String
}

/// The renderer until qcalc-typst lands: no image, and the line written out with real symbols.
struct PlainMathRenderer: MathRenderer {
    func render(expression: String, answer: String, scale: CGFloat, dark: Bool) -> CGImage? { nil }

    func plain(expression: String) -> String {
        var s = Pretty.radicals(expression)
        // simple powers as superscripts: x^2 → x², 10^-3 → 10⁻³
        let sup: [Character: Character] = ["0": "⁰", "1": "¹", "2": "²", "3": "³", "4": "⁴", "5": "⁵", "6": "⁶",
                                           "7": "⁷", "8": "⁸", "9": "⁹", "-": "⁻", "n": "ⁿ"]
        if let re = try? NSRegularExpression(pattern: #"\^(-?[0-9]+|n)"#) {
            let ns = s as NSString
            var out = ""
            var last = 0
            for m in re.matches(in: s, range: NSRange(location: 0, length: ns.length)) {
                out += ns.substring(with: NSRange(location: last, length: m.range.location - last))
                out += String(ns.substring(with: m.range(at: 1)).compactMap { sup[$0] })
                last = m.range.location + m.range.length
            }
            out += ns.substring(from: last)
            s = out
        }
        return s.replacingOccurrences(of: "/", with: " ⁄ ").replacingOccurrences(of: "-", with: Pretty.minus)
    }
}

/// The preview row under the bar (TypstPreview.tsx): only for lines with structure worth typesetting.
struct MathPreview: View {
    let expression: String
    let answer: String
    let renderer: MathRenderer
    @Environment(\.colorScheme) private var scheme
    @Environment(\.displayScale) private var scale

    static func useful(_ expression: String) -> Bool {
        expression.contains("^") || expression.contains("sqrt") || expression.contains("/") || expression.contains("√")
    }

    var body: some View {
        Group {
            if let image = renderer.render(expression: expression, answer: answer, scale: scale, dark: scheme == .dark) {
                Image(decorative: image, scale: scale)
            } else {
                Text(renderer.plain(expression: expression))
                    .font(.system(size: 15, design: .serif))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.head)
            }
        }
        .accessibilityLabel("Typeset: \(renderer.plain(expression: expression))")
    }
}
