import Foundation

/// Standalone smoke test for MathEval. Not part of the app target.
///   swiftc macos/MathEval.swift macos/math_test.swift -o /tmp/qcalc-math-test && /tmp/qcalc-math-test
@main
enum MathTest {
    static func main() {
        expect("2+2", 4)
        expect("3.13*0.3*10^-3", 0.000939)
        expect("10^-3", 0.001)
        expect("1.5e-4", 0.00015)
        expect("(1+2)*3", 9)
        expect("5+3)*2", 16)
        expect("100/2+5)", 55)
        expect("2^8", 256)
        expect("-(2+3)*4", -20)
        expect("1/10^-3", 1000)
        print("swift math benchmark passed")
    }

    static func expect(_ expr: String, _ value: Double, eps: Double = 1e-9) {
        guard let got = MathEval.evaluate(expr) else {
            fputs("nil for \(expr)\n", stderr)
            exit(1)
        }
        if abs(got - value) > eps {
            fputs("\(expr) => \(got), expected \(value)\n", stderr)
            exit(1)
        }
    }
}
