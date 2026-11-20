import Foundation

/// Whether an explanation's Related formula is the same formula as a sheet row's cell (pure, so it is tested alone:
/// macos/sheet_match_test.swift). The same characters, leaving out spaces, brackets, ^ _ and the way minus and times
/// are written, either way round; or, for a row that chains equalities, the formula's sides in order among the row's
/// (`P = VI = I^{2}R` is `P = V I = I^{2} R = V^{2}/R`, `S = P + jQ` is `S = V_{rms} I_{rms}^{*} = P + jQ`).
enum SheetMatch {
    static func key(_ text: String) -> String {
        let plain = text.replacingOccurrences(of: "\u{2212}", with: "-")
        return String(plain.lowercased().filter { !" \t{}()^_*·×⋅".contains($0) })
    }

    static func same(_ formula: String, cell: String) -> Bool {
        let want = key(formula)
        guard !want.isEmpty else { return false }
        let have = key(cell)
        if have == want { return true }
        let sides = formula.components(separatedBy: "=").map(key)
        guard sides.count >= 2, !sides.contains(where: \.isEmpty) else { return false }
        if sides.count == 2, key(sides[1] + "=" + sides[0]) == have { return true }
        let row = cell.components(separatedBy: "=").map(key)
        guard row.count > sides.count else { return false }
        return inOrder(sides, row) || inOrder(sides.reversed(), row)
    }

    // every one of `part`, in this order, among `whole`
    private static func inOrder(_ part: [String], _ whole: [String]) -> Bool {
        var rest = whole[...]
        for side in part {
            guard let at = rest.firstIndex(of: side) else { return false }
            rest = rest[(at + 1)...]
        }
        return true
    }
}
