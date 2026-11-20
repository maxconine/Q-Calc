import Foundation

/// Standalone test for which sheet row a Related formula opens. Not part of the app target.
///   swiftc macos/SheetMatch.swift macos/sheet_match_test.swift -o /tmp/qcalc-match-test && /tmp/qcalc-match-test
@main
enum SheetMatchTest {
    static var failed = 0

    static func check(_ formula: String, _ cell: String, _ want: Bool) {
        if SheetMatch.same(formula, cell: cell) != want {
            failed += 1
            print("FAIL: \(formula) vs \(cell): want \(want)")
        }
    }

    static func main() {
        // as before: the same characters, or flipped
        check("τ = Tρ/J", "τ = Tρ/J", true)
        check("y_{n+1} = y_{n} + h f", "y_{n+1} = y_n + h f", true)
        check("Tρ/J = τ", "τ = Tρ/J", true)
        check("a − b", "a - b", true)
        // a chained row holds the formula's sides in order (these opened a search before)
        check("P = VI = I^{2}R", "P = V I = I^{2} R = V^{2}/R", true)
        check("Y = 1/Z", "Y = 1/Z = G + jB", true)
        check("PF = cos(θ)", "PF = P/|S| = cos(θ)", true)
        check("S = P + jQ", "S = V_{rms} I_{rms}^{*} = P + jQ", true)
        check("P = Fv", "P = dW/dt = F v", true)
        check("1/(1 − x) = Σ x^{n}", "1/(1 − x) = Σ x^{n} = 1 + x + x^{2} + x^{3} + …", true)
        check("cos(θ) = PF", "PF = P/|S| = cos(θ)", true)
        // not the same formula
        check("P = Fv", "W = F d cos(θ)", false)
        check("W = F · d", "W = F d cos(θ)", false)
        check("U = (1/2) C V^{2}", "U = (1/2) L I^{2}", false)
        check("x = 1", "y = x = 2", false)
        check("", "", false)
        check("n^{r}", "SNR = 6.02n + 1.76 dB", false)
        // a side alone never matches a chain
        check("P", "P = V I = I^{2} R", false)
        if failed > 0 { exit(1) }
        print("sheet match tests passed")
    }
}
