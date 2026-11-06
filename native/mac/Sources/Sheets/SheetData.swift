import AppKit
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
    private var drawnSheets: [String: IdentitySheet] = [:]
    private let typstQueue = DispatchQueue(label: "qcalc.sheets.typst", qos: .userInitiated)
    /// only touched on `typstQueue`; made the first time a sheet with Typst opens
    private var renderer: TypstRenderer?

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

    /// a sheet in openNativeIdentitySheet's shape
    private func raw(_ id: String) -> [String: Any]? {
        if let cached = sheetCache[id] { return cached }
        guard let sheet = logic.call("lib/identities", "identitySheetById", [id]) as? [String: Any] else { return nil }
        let runs: (String) -> Any = { [logic] show in logic.call("lib/identities", "identityRuns", [show]) ?? [] }
        // a sheet with a picture shows only the picture here (the table's still there by its own name)
        let allSections = sheet["sections"] as? [[String: Any]] ?? []
        let pictured = allSections.filter { s in
            (s["groups"] as? [[String: Any]] ?? []).contains { ($0["items"] as? [[String: Any]] ?? []).contains { $0["figure"] != nil } }
        }
        let sections = (pictured.isEmpty ? allSections : pictured).map { s -> [String: Any] in
            let groups = (s["groups"] as? [[String: Any]] ?? []).map { g -> [String: Any] in
                let items = (g["items"] as? [[String: Any]] ?? []).map { i -> [String: Any] in
                    var item: [String: Any] = ["runs": runs(i["show"] as? String ?? ""), "copy": i["copy"] as? String ?? "", "note": i["note"] as? String ?? "",
                                               "lead": (i["lead"] as? [String] ?? []).map(runs)]
                    if let copies = i["leadCopy"] { item["leadCopy"] = copies }
                    if let line = i["open"] { item["open"] = line }
                    if let parts = i["parts"] as? [[String: Any]] {
                        item["parts"] = parts.map { part -> [String: Any] in
                            ["doc": logic.string("lib/identities", "sheetMathDocument", [part["typst"] as? String ?? ""]) ?? "", "copy": part["copy"] as? String ?? ""]
                        }
                        item["partDocs"] = (item["parts"] as? [[String: Any]] ?? []).compactMap { $0["doc"] }
                    }
                    if let figure = i["figure"] as? String {
                        item["cellDocs"] = [figure]
                        item["figure"] = true
                        item["spots"] = i["spots"]
                        item["spotDocs"] = (i["spots"] as? [[String: Any]] ?? []).compactMap { $0["doc"] }
                    }
                    // a sheet with Typst for its rows: the documents to compile, words and maths in the one type
                    if let cells = i["typst"] as? [String] {
                        // words alone are drawn in the app's own font; only maths is typeset
                        item["cellDocs"] = cells.compactMap { cell -> String? in
                            if cell.count > 1, cell.hasPrefix("\""), cell.hasSuffix("\""), !cell.dropFirst().dropLast().contains("\"") {
                                return Self.wordsPrefix + cell.dropFirst().dropLast()
                            }
                            return logic.string("lib/identities", "sheetMathDocument", [cell])
                        }
                        if let maths = i["noteTypst"] as? String {
                            item["noteDoc"] = logic.string("lib/identities", "sheetMathDocument", ["#text(size: 10.5pt)[$\(maths)$]"])
                        }
                    }
                    return item
                }
                return ["title": g["title"] as? String ?? "", "items": items]
            }
            var section: [String: Any] = ["title": s["title"] as? String ?? "", "wide": (s["wide"] as? Bool) == true, "groups": groups]
            if let widths = s["leadWidths"] { section["leadWidths"] = widths }
            return section
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
    /// the last line asked about and its answer: BarCommand.of asks several times a keystroke for the same text
    private var lastTarget: (text: String, target: (id: String, section: Int, hint: String)?)?

    func target(_ text: String) -> (id: String, section: Int, hint: String)? {
        if let last = lastTarget, last.text == text { return last.target }
        var found: (id: String, section: Int, hint: String)?
        if let t = logic.call("lib/identities", "nativeTargetFor", [text]) as? [String: Any],
           let sheet = t["sheet"] as? [String: Any], let id = sheet["id"] as? String {
            found = (id, (t["section"] as? NSNumber)?.intValue ?? 0, t["hint"] as? String ?? "")
        }
        lastTarget = (text, found)
        return found
    }

    /// the sheet, its Typst compiled off the main thread the first time; `done` runs on the main thread
    func identitySheet(_ id: String, done: @escaping (IdentitySheet?) -> Void) {
        guard let dict = raw(id) else { return done(nil) }
        if let ready = drawnSheets[id] { return done(ready) }
        let docs = Self.documents(in: dict)
        guard !docs.isEmpty else {
            return done(IdentitySheet(dict))
        }
        typstQueue.async { [weak self] in
            guard let self else { return }
            if self.renderer == nil { self.renderer = TypstRenderer() }
            var pictures: [String: TypstPaths] = [:]
            for doc in docs where pictures[doc] == nil && !doc.hasPrefix(Self.wordsPrefix) {
                pictures[doc] = try? self.renderer?.paths(source: doc)
            }
            DispatchQueue.main.async {
                let sheet = IdentitySheet(dict) { doc in
                    if doc.hasPrefix(Self.wordsPrefix) { return Self.words(String(doc.dropFirst(Self.wordsPrefix.count))) }
                    return pictures[doc].map { paths in
                        IdentitySheet.Drawn(size: CGSize(width: CGFloat(paths.widthPt), height: CGFloat(paths.heightPt))) { MathPreview.draw(paths, in: $0) }
                    }
                }
                self.drawnSheets[id] = sheet
                done(sheet)
            }
        }
    }

    /// a cell that's only words: this and the words, in place of a document
    private static let wordsPrefix = "words:"

    private static func words(_ text: String) -> IdentitySheet.Drawn {
        let string = NSAttributedString(string: text, attributes: [.font: NSFont.systemFont(ofSize: 13), .foregroundColor: NSColor.labelColor])
        return IdentitySheet.Drawn(size: string.size()) { _ in string.draw(at: .zero) }
    }

    /// every "…Doc" in a sheet's dictionary
    private static func documents(in any: Any?) -> Set<String> {
        var found = Set<String>()
        if let d = any as? [String: Any] {
            for (key, value) in d {
                if key.hasSuffix("Doc"), let doc = value as? String { found.insert(doc) }
                if key == "cellDocs" || key == "spotDocs" || key == "partDocs", let docs = value as? [String] { found.formUnion(docs) }
                found.formUnion(documents(in: value))
            }
        } else if let list = any as? [Any] {
            for value in list { found.formUnion(documents(in: value)) }
        }
        return found
    }
}
