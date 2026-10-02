import Foundation

// the same rule as unitlessAnswer in src/lib/answer.ts, for the fallback bar that shows when the page can't load:
// `15 m²` copies as 15, `x = 6 m, y = 4 m` as x = 6, y = 4; `5 ft 3 in`, money and messages copy as shown
enum UnitlessCopy {
    private static let number = #"[+\-−±]?(?:[\d.,√∛π/^⁰¹²³⁴⁵⁶⁷⁸⁹⁻·×]|(?<=\d)e[+\-−]?(?=\d)|\s(?=[±×·]))*\d[\d.⁰¹²³⁴⁵⁶⁷⁸⁹⁻]*(?:\s*±\s*[\d.,]+)?"#
    private static let unit = #"[A-Za-z°µμΩÅ][A-Za-z°µμΩÅ²³⁻¹^\d·*/]*"#
    private static let pattern = try! NSRegularExpression(
        pattern: #"(^|=\s*|\(\s*|,\s*|\sor\s+)("# + number + #")\s+"# + unit + #"(?=\s*$|\s*,|\s*\)|\s+or\b)"#
    )

    static func answer(_ text: String) -> String {
        let range = NSRange(text.startIndex..., in: text)
        return pattern.stringByReplacingMatches(in: text, range: range, withTemplate: "$1$2")
    }

    // what a copied answer is, with the setting applied
    static func copied(_ text: String) -> String {
        AppSettings.shared.copyUnitless ? answer(text) : text
    }
}
