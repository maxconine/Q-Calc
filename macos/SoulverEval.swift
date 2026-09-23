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
        var formatting = FormattingPreferences()
        formatting.dp = 10
        formatting.thousandsSeparatorDisabled = true
        calc.formattingPreferences = formatting
        Task { _ = await rates.updateRates() }
        return calc
    }()

    /// Build the calculator and load its tables in the background so the first keystroke is fast.
    static func warm() {
        DispatchQueue.global(qos: .utility).async { _ = evaluate("1+1") }
    }

    static func evaluate(
        _ expression: String,
        ans: Double? = nil,
        variables: [String: Double] = [:],
        sigFigs: Int = 12
    ) -> Answer? {
        let src = MathEval.fillParens(expression.trimmingCharacters(in: .whitespacesAndNewlines))
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
        let result = calculator.calculate(src, with: list)
        return answer(from: result)
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
        // SoulverCore reports unit mismatch / overflow as a string like
        // "Error: incompatible units" while leaving isFailedResult false.
        if display.lowercased().hasPrefix("error") {
            if display.lowercased().contains("unit") {
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
        if value == 0 { return "0" }
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
