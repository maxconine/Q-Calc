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
        Piece(u: u, p: p, range: 0..<u.count, size: size, line: size)
            .fixedSize()
    }

    /// the characters in `range`, a stack that starts in it drawn as one piece
    private struct Piece: View {
        let u: [UInt16]
        let p: StackedMath.Parsed
        let range: Range<Int>
        let size: CGFloat
        /// the row's own size: the bar's sizes (BarTextView.slotSize) go by it
        let line: CGFloat

        private enum Part {
            case text(String), fraction(Range<Int>, Range<Int>), power(Range<Int>, raise: CGFloat), root(Range<Int>, cube: Bool), gap
            /// shown brackets grown around a stack: what's between them, and whether they're square
            case group(Range<Int>, square: Bool)
        }

        private var parts: [Part] {
            var out: [Part] = []
            var run: [UInt16] = []
            func flush() {
                if !run.isEmpty { out.append(.text(Self.spaced(run, afterStack: out.last.map { if case .text = $0 { return false }; return true } ?? false))) }
                run = []
            }
            let closeOf = StackedMath.pairs(u).closeOf
            var i = range.lowerBound
            while i < range.upperBound {
                // a root over a bracketed group: the sign and its bar drawn around it, as the bar draws them
                if u[i] == 0x221A || u[i] == 0x221B, i + 1 < range.upperBound, u[i + 1] == 0x28,
                   let c = closeOf[i + 1], c < range.upperBound {
                    flush()
                    out.append(.root((i + 1)..<(c + 1), cube: u[i] == 0x221B))
                    i = c + 1
                    continue
                }
                // a root over a run of digits, letters and π (radicandRanges): its bar over it, as the bar draws it
                if u[i] == 0x221A || u[i] == 0x221B, i + 1 < range.upperBound, !p.stacks.contains(where: { $0.start == i }) {
                    var j = i + 1
                    while j < range.upperBound, !p.hidden.contains(j), let c = UnicodeScalar(u[j]),
                          CharacterSet.alphanumerics.contains(c) || u[j] == 0x2E { j += 1 }
                    if j > i + 1 {
                        flush()
                        out.append(.root((i + 1)..<j, cube: u[i] == 0x221B))
                        i = j
                        continue
                    }
                }
                if let b = p.tall.first(where: { $0.open == i && $0.close < range.upperBound }) {
                    flush()
                    out.append(.group((i + 1)..<b.close, square: u[i] == 0x5B))
                    i = b.close + 1
                    continue
                }
                if let s = p.stacks.first(where: { $0.start == i && $0.end <= range.upperBound }) {
                    flush()
                    if s.isFraction, let num = s.num {
                        out.append(.fraction(num.lo..<num.hi, s.slot.lo..<s.slot.hi))
                    } else {
                        out.append(.power(s.slot.lo..<s.slot.hi, raise: BarTextView.supShift(p, s, parent: size, line: line)))
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
                        let f = BarTextView.slotSize(parent: size, fraction: true, line: line)
                        // TeX's gap off the bar (3θ), less the room a line of text keeps under its baseline and over
                        // its digits
                        let gap = (TeX.ruleThickness / 2 + TeX.fractionGapMin) * size
                        VStack(spacing: 0) {
                            slot(num, f, overhang: true).padding(.bottom, gap - f * 0.21)
                            Rectangle().frame(height: max(1, size * TeX.ruleThickness))
                            slot(den, f, overhang: true).padding(.top, gap - f * 0.23)
                        }
                        .fixedSize()
                        .padding(.horizontal, size * 0.08)
                    case .group(let r, let square):
                        let w = size * 0.33
                        Piece(u: u, p: p, range: r, size: size, line: line)
                            .padding(.horizontal, w)
                            .overlay { TallBrackets(size: size, width: w, square: square) }
                    case .root(let r, let cube):
                        Piece(u: u, p: p, range: r, size: size, line: line)
                            .fixedSize()
                            .padding(.leading, RootSign.width(size))
                            // TeX's gap over what's under it (a line of text keeps a quarter of its size over its digits)
                            .padding(.top, max(0, TeX.radicalGap(size) + RootSign.lineWidth(size) / 2 + 1 - size * 0.25))
                            .overlay { TallRoot(size: size).stroke(style: StrokeStyle(lineWidth: RootSign.lineWidth(size), lineCap: .round, lineJoin: .round)) }
                            .overlay(alignment: .topLeading) {
                                if cube { Text("3").font(.system(size: size * RootSign.indexScale)) }
                            }
                    case .power(let exp, let raise):
                        let f = BarTextView.slotSize(parent: size, fraction: false, line: line)
                        // raised by an offset, which takes no height: padding made a denominator with a power in
                        // it taller than the row, and the row cut it off. Centred, a smaller line's baseline already
                        // sits 0.357 of the difference in sizes higher (the system font's ascent less half its height)
                        slot(exp, f).offset(y: -(raise - 0.357 * (size - f))).padding(.top, size * 0.12)
                    }
                }
            }
        }

        @ViewBuilder private func slot(_ r: Range<Int>, _ f: CGFloat, overhang: Bool = false) -> some View {
            if r.isEmpty {
                RoundedRectangle(cornerRadius: f * 0.14).opacity(0.18).frame(width: f * 0.62, height: f * 0.8)
            } else {
                // the bar reaches a little past the wider slot
                AnyView(Piece(u: u, p: p, range: r, size: f, line: line)).padding(.horizontal, overhang ? f * BarTextView.barOverhang : 0)
            }
        }
    }
}

/// A root's line around everything it takes: its bar along the top, its foot at the bottom
private struct TallRoot: Shape {
    let size: CGFloat
    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.addLines(RootSign.points(left: rect.minX, width: RootSign.width(size), end: rect.maxX,
                                      top: rect.minY + 1, foot: rect.maxY - 1))
        return path
    }
}

/// Grown brackets either side of what they hold (TallBracketShape, as the bar draws them)
private struct TallBrackets: Shape {
    let size: CGFloat
    let width: CGFloat
    let square: Bool
    func path(in rect: CGRect) -> Path {
        var path = Path(TallBracketShape.path(left: rect.minX, width: width, top: rect.minY, bottom: rect.maxY, size: size, square: square, closing: false))
        path.addPath(Path(TallBracketShape.path(left: rect.maxX - width, width: width, top: rect.minY, bottom: rect.maxY, size: size, square: square, closing: true)))
        return path
    }
}
