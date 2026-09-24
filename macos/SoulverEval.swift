import Foundation
import SoulverCore

enum SoulverEval {
    struct Answer {
        var display: String
        var number: Double?
    }

    private static let lock = NSLock()
    private static let rates = ECBCurrencyRateProvider()

    private static let calculator: Calculator = {
        var customization = EngineCustomization.standard
        customization.currencyRateProvider = rates
        customization.featureFlags.variableDeclarations = true
        let calc = Calculator(customization: customization)
        // formatting is set per call in evaluate, from the sig figs setting
        Task { _ = await rates.updateRates() }
        return calc
    }()

    // builds the calculator in the background so the first keystroke is fast
    static func warm() {
        DispatchQueue.global(qos: .utility).async { _ = evaluate("1+1") }
    }

    static func evaluate(
        _ expression: String,
        ans: Double? = nil,
        variables: [String: Double] = [:],
        sigFigs: Int = 12
    ) -> Answer? {
        // soulvercore reads √ but not ∛ (it answers ∛27 with 27)
        let src = MathEval.fillParens(expression.trimmingCharacters(in: .whitespacesAndNewlines))
            .replacingOccurrences(of: "∛", with: "cbrt")
        guard !src.isEmpty else { return nil }

        lock.lock()
        defer { lock.unlock() }

        var formatting = FormattingPreferences()
        formatting.dp = min(16, max(2, sigFigs))
        formatting.thousandsSeparatorDisabled = true
        calculator.formattingPreferences = formatting

        var vars: [Variable] = variables.compactMap { name, value in
            let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmed.isEmpty else { return nil }
            return Variable(name: trimmed, value: string(from: value))
        }
        if let ans {
            vars.append(Variable(name: "ans", value: string(from: ans)))
        }
        let list = vars.isEmpty ? nil : VariableList(variables: vars)
        return answer(from: calculator.calculate(src, with: list))
    }

    private static func answer(from result: CalculationResult) -> Answer? {
        if result.isEmptyResult || result.isFailedResult || result.isPendingResult {
            return nil
        }
        switch result.evaluationResult {
        case .error, .failed, .none, .pending:
            return nil
        default:
            break
        }

        let display = result.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !display.isEmpty else { return nil }
        // soulvercore reports unit mismatch as "Error: incompatible units" with isFailedResult false
        let lowered = display.lowercased()
        if lowered.hasPrefix("error") {
            if lowered.contains("unit") {
                return Answer(display: "improper unit conversion", number: nil)
            }
            return nil
        }
        if let decimal = result.evaluationResult.decimalValue, decimal.isNaN || decimal.isInfinite {
            return nil
        }
        return Answer(display: display, number: double(from: result.evaluationResult.decimalValue))
    }

    private static func string(from value: Double) -> String {
        guard value.isFinite else { return "0" }
        if value.rounded() == value, abs(value) < 1e15 {
            return String(Int(value))
        }
        return String(value)
    }

    private static func double(from decimal: Decimal?) -> Double? {
        guard let decimal, decimal.isFinite, !decimal.isNaN else { return nil }
        let n = (decimal as NSDecimalNumber).doubleValue
        return n.isFinite ? n : nil
    }
}
