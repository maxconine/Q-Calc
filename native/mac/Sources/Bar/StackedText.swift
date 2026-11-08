import SwiftUI

/// A history row's expression with its fractions and powers drawn stacked, as the bar draws them while
/// "Stacked fractions and powers" is on (StackedMath.parse, BarTextView's measures). Plain text otherwise.
struct StackedText: View {
    let text: String
    let size: CGFloat

    /// whether `text` has anything to stack (the setting on, and a fraction or power the bar would stack)
    static func stacks(_ text: String) -> Bool {
        AppSettings.shared.stackedMath && (text.contains("/") || text.contains("^")) && !StackedMath.parse(text).stacks.isEmpty
    }

    var body: some View {
        let p = StackedMath.parse(text)
        let u = Array(text.utf16)
        Piece(u: u, p: p, range: 0..<u.count, size: size)
            .fixedSize()
    }

    /// the characters in `range`, a stack that starts in it drawn as one piece
    private struct Piece: View {
        let u: [UInt16]
        let p: StackedMath.Parsed
        let range: Range<Int>
        let size: CGFloat
        /// inside another stack's slot: a fraction there is smaller again, as in the bar
        var nested = false

        private enum Part { case text(String), fraction(Range<Int>, Range<Int>), power(Range<Int>), gap }

        private var parts: [Part] {
            var out: [Part] = []
            var run: [UInt16] = []
            func flush() {
                if !run.isEmpty { out.append(.text(Self.spaced(run, afterStack: out.last.map { if case .text = $0 { return false }; return true } ?? false))) }
                run = []
            }
            var i = range.lowerBound
            while i < range.upperBound {
                if let s = p.stacks.first(where: { $0.start == i && $0.end <= range.upperBound }) {
                    flush()
                    if s.isFraction, let num = s.num {
                        out.append(.fraction(num.lo..<num.hi, s.slot.lo..<s.slot.hi))
                    } else {
                        out.append(.power(s.slot.lo..<s.slot.hi))
                    }
                    i = s.end
                    continue
                }
                if p.hidden.contains(i) {
                    if p.dots.contains(i) { flush(); out.append(.gap) }
                    i += 1
                    continue
                }
                run.append(u[i])
                i += 1
            }
            flush()
            return out
        }

        /// room around `+ − =` between two terms and after `; ,`, as the bar spaces them
        static func spaced(_ run: [UInt16], afterStack: Bool) -> String {
            var out = ""
            let binary: Set<UInt16> = [0x2B, 0x2D, 0x2212, 0x3D, 0x3C, 0x3E, 0x2264, 0x2265, 0xB1, 0x2260]
            for (k, c) in run.enumerated() {
                let ch = String(utf16CodeUnits: [c], count: 1)
                let prev: UInt16? = k > 0 ? run[k - 1] : nil
                let term = prev.map { StackedMath.isWord($0) || $0 == 0x29 || $0 == 0x5D || $0 == 0x21 || $0 == 0x25 } ?? afterStack
                let exponent = (c == 0x2D || c == 0x2B) && (prev == 0x65 || prev == 0x45) && k > 1 && StackedMath.isDigit(run[k - 2])
                if binary.contains(c), term, !exponent, k + 1 < run.count || afterStack {
                    out += "\u{2009}" + ch + "\u{2009}"
                } else if c == 0x3B || c == 0x2C {
                    out += ch + "\u{2009}"
                } else {
                    out += ch
                }
            }
            return out
        }

        var body: some View {
            HStack(alignment: .center, spacing: 0) {
                ForEach(Array(parts.enumerated()), id: \.offset) { _, part in
                    switch part {
                    case .text(let s):
                        Text(s).font(.system(size: size))
                    case .gap:
                        Color.clear.frame(width: size * 0.14, height: 1)
                    case .fraction(let num, let den):
                        let f = max(7, size * (nested ? 0.7 : BarTextView.fractionScale))
                        VStack(spacing: f * 0.12) {
                            slot(num, f, overhang: true)
                            Rectangle().frame(height: max(1, size * 0.06))
                            slot(den, f, overhang: true)
                        }
                        .fixedSize()
                        .padding(.horizontal, size * 0.08)
                    case .power(let exp):
                        let f = max(6, size * BarTextView.powerScale)
                        // raised by an offset, which takes no height: padding made a denominator with a power in
                        // it taller than the row, and the row cut it off
                        slot(exp, f).offset(y: -size * 0.32).padding(.top, size * 0.12)
                    }
                }
            }
        }

        @ViewBuilder private func slot(_ r: Range<Int>, _ f: CGFloat, overhang: Bool = false) -> some View {
            if r.isEmpty {
                RoundedRectangle(cornerRadius: f * 0.14).opacity(0.18).frame(width: f * 0.62, height: f * 0.8)
            } else {
                // the bar reaches a little past the wider slot
                AnyView(Piece(u: u, p: p, range: r, size: f, nested: true)).padding(.horizontal, overhang ? f * BarTextView.barOverhang : 0)
            }
        }
    }
}
