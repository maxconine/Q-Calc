import Foundation
import QCalcCore

// The reference sheets' data from the engine (WP-CHEM, WP-APP-LOGIC), shaped as the web page used to post it so
// PeriodicWindow.swift and IdentityWindow.swift draw it unchanged: `Engine.elements()` placed by gridCell, and a
// sheet from `identitySheetFor` with each line split into raised and lowered runs by `identityRuns`.
final class SheetData {
    struct SheetInfo {
        let id: String
        let hint: String
        let sectionTitles: [String]
    }

    static var shared: SheetData!

    private let engine: Engine
    private let logic: AppLogic
    private var sheetCache: [String: [String: Any]] = [:]
    private lazy var tiles: [Any] = makeTiles()

    init(engine: Engine, logic: AppLogic) {
        self.engine = engine
        self.logic = logic
    }

    // the tiles as openNativePeriodicTable sent them: gridCell's row and column, insertableMass's insert
    private func makeTiles() -> [Any] {
        engine.elements().map { e -> [String: Any] in
            let period = Int(e.period)
            let (row, col): (Int, Int) = e.group.map { (period, Int($0)) } ?? (period + 3, 3 + Int(e.n) - (period == 6 ? 57 : 89))
            let insert = e.mass.replacingOccurrences(of: #"^\[(\d+)\]$"#, with: "$1", options: .regularExpression)
            return ["n": Int(e.n), "symbol": e.symbol, "name": e.name, "mass": e.mass, "insert": insert,
                    "row": row, "col": col, "category": "\(e.category)"]
        }
    }

    var elements: [PeriodicElement] { PeriodicElement.list(from: tiles) }

    private static let commands = ["identities": "identities", "calculus": "calculus", "conversions": "conversions", "constants": "constants"]

    /// a sheet in openNativeIdentitySheet's shape
    private func raw(_ id: String) -> [String: Any]? {
        if let cached = sheetCache[id] { return cached }
        guard let sheet = logic.call("lib/identities", "identitySheetFor", [Self.commands[id] ?? id]) as? [String: Any] else { return nil }
        let runs: (String) -> Any = { [logic] show in logic.call("lib/identities", "identityRuns", [show]) ?? [] }
        let sections = (sheet["sections"] as? [[String: Any]] ?? []).map { s -> [String: Any] in
            let groups = (s["groups"] as? [[String: Any]] ?? []).map { g -> [String: Any] in
                let items = (g["items"] as? [[String: Any]] ?? []).map { i -> [String: Any] in
                    ["runs": runs(i["show"] as? String ?? ""), "copy": i["copy"] as? String ?? "", "note": i["note"] as? String ?? "",
                     "lead": (i["lead"] as? [String] ?? []).map(runs)]
                }
                return ["title": g["title"] as? String ?? "", "items": items]
            }
            return ["title": s["title"] as? String ?? "", "wide": (s["wide"] as? Bool) == true, "groups": groups]
        }
        let out: [String: Any] = ["id": sheet["id"] as? String ?? id, "title": sheet["title"] as? String ?? "",
                                  "hint": sheet["hint"] as? String ?? "", "sections": sections]
        sheetCache[id] = out
        return out
    }

    func sheet(_ id: String) -> SheetInfo? {
        guard let s = raw(id) else { return nil }
        return SheetInfo(id: id, hint: s["hint"] as? String ?? id,
                         sectionTitles: (s["sections"] as? [[String: Any]] ?? []).compactMap { $0["title"] as? String })
    }

    /// identityTargetFor: the sheet a bar line opens on ↵, the section to show first, and what the bar says
    func target(_ text: String) -> (id: String, section: Int, hint: String)? {
        guard let t = logic.call("lib/identities", "identityTargetFor", [text]) as? [String: Any],
              let sheet = t["sheet"] as? [String: Any], let id = sheet["id"] as? String else { return nil }
        return (id, (t["section"] as? NSNumber)?.intValue ?? 0, t["hint"] as? String ?? "")
    }

    func identitySheet(_ id: String) -> IdentitySheet? {
        raw(id).flatMap { IdentitySheet($0) }
    }
}
