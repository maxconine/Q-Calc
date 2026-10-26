import SwiftUI

// Radical.tsx's look in plain SwiftUI: a √ with a bar over what it takes (a bracketed group, or a run of digits,
// letters and π), so `2sqrt(2)` reads 2√2̅ as the web bar draws it. The Typst renderer replaces this for full maths.
struct RadicalText: View {
    let text: String
    let size: CGFloat
    var tracking: CGFloat = 0
    var monospaced = false

    private enum Run { case plain(String), root(String) }

    private var runs: [Run] {
        var out: [Run] = []
        var plain = ""
        var chars = Array(text)
        var i = 0
        while i < chars.count {
            guard chars[i] == "√" || chars[i] == "∛", i + 1 < chars.count else {
                plain.append(chars[i]); i += 1; continue
            }
            plain.append(chars[i])
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
            guard j > i + 1 else { i += 1; continue }
            out.append(.plain(plain)); plain = ""
            out.append(.root(String(chars[(i + 1)..<j])))
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
                case let .root(s):
                    styled(s).overlay(alignment: .top) {
                        Rectangle().frame(height: max(1, size / 18)).offset(y: size * 0.08)
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
