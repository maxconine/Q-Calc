import Foundation

// The reference sheets' data, read once from Resources/sheets.json (written from the TS by
// scripts/export-sheets.sh in the shapes the page used to post), so PeriodicWindow.swift and IdentityWindow.swift
// draw it unchanged. When QCalcCore exposes elements() and identity_sheet(id), this asks the engine instead.
final class SheetData {
    struct SheetInfo {
        let id: String
        let hint: String
        let sectionTitles: [String]
        let raw: [String: Any]
    }

    static let shared = SheetData()

    private(set) var elementsRaw: [Any] = []
    private var sheets: [String: SheetInfo] = [:]

    private init() {
        guard let url = Bundle.main.url(forResource: "sheets", withExtension: "json"),
              let data = try? Data(contentsOf: url),
              let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else {
            NSLog("Q Calc: sheets.json is missing from the bundle")
            return
        }
        elementsRaw = root["elements"] as? [Any] ?? []
        for case let s as [String: Any] in root["sheets"] as? [Any] ?? [] {
            guard let id = s["id"] as? String else { continue }
            let titles = (s["sections"] as? [[String: Any]] ?? []).compactMap { $0["title"] as? String }
            sheets[id] = SheetInfo(id: id, hint: s["hint"] as? String ?? id, sectionTitles: titles, raw: s)
        }
    }

    var elements: [PeriodicElement] { PeriodicElement.list(from: elementsRaw) }

    func sheet(_ id: String) -> SheetInfo? { sheets[id] }

    func identitySheet(_ id: String) -> IdentitySheet? {
        sheets[id].flatMap { IdentitySheet($0.raw) }
    }
}
