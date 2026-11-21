import Foundation

// Recall checks for the sheets' search (macos/SheetSearch.swift): every sheet the native app searches (the web's four,
// then native_sheets.json, each row with its explanation), each query in queries.tsv run through it, and the row it
// should find looked for in the first 3 results. Prints each miss and the pass rate, and the search's time.
//   main <core/src/app dir> <queries.tsv> [--all]   (--all prints every query's top 3)

let args = CommandLine.arguments
guard args.count >= 3 else { print("usage: sheet-search <core/src/app> <queries.tsv> [--all]"); exit(2) }
let app = URL(fileURLWithPath: args[1])
let showAll = args.contains("--all")

func json(_ url: URL) -> Any? { (try? Data(contentsOf: url)).flatMap { try? JSONSerialization.jsonObject(with: $0) } }

var explanations: [String: [String: Any]] = [:]
let files = (try? FileManager.default.contentsOfDirectory(at: app.appendingPathComponent("explanations"), includingPropertiesForKeys: nil)) ?? []
for file in files.sorted(by: { $0.lastPathComponent < $1.lastPathComponent }) where file.pathExtension == "json" {
    for (k, v) in json(file) as? [String: [String: Any]] ?? [:] { explanations[k] = v }
}
let sheets = (json(app.appendingPathComponent("identities.json")) as? [[String: Any]] ?? [])
    + (json(app.appendingPathComponent("native_sheets.json")) as? [[String: Any]] ?? [])

// a row's `show` as its runs read: raised and lowered parts without their braces
func plain(_ show: String) -> String {
    show.replacingOccurrences(of: "^{", with: "").replacingOccurrences(of: "_{", with: "")
        .replacingOccurrences(of: "{", with: "").replacingOccurrences(of: "}", with: "")
}

let search = SheetSearch()
var labels: [String] = []
var copies: [String] = []
var sheetOf: [String] = []
let started = Date()
for sheet in sheets {
    let title = sheet["title"] as? String ?? ""
    // a picture isn't searched (its copy is empty); the rows beside it are (IdentityWindow.unindexed)
    for section in sheet["sections"] as? [[String: Any]] ?? [] {
        let groups = section["groups"] as? [[String: Any]] ?? []
        for group in groups {
            for item in group["items"] as? [[String: Any]] ?? [] {
                guard let copy = item["copy"] as? String, !copy.isEmpty, item["figure"] == nil, let show = item["show"] as? String else { continue }
                let lead = item["lead"] as? [String] ?? []
                var r = SheetSearch.RowText()
                r.lead = lead.map(plain)
                r.show = plain(show)
                r.copy = copy
                r.note = item["note"] as? String ?? ""
                r.leadCopy = item["leadCopy"] as? [String] ?? []
                r.parts = (item["parts"] as? [[String: Any]] ?? []).compactMap { $0["copy"] as? String }
                r.open = item["open"] as? String ?? ""
                r.tags = item["tags"] as? String ?? ""
                if let e = explanations[(lead + [show]).joined(separator: " ‖ ")], item["open"] == nil {
                    r.explainTitle = e["title"] as? String ?? ""
                    r.vars = e["vars"] as? [[String]] ?? []
                    r.related = e["related"] as? [[String]] ?? []
                }
                search.add(r, titles: [title, section["title"] as? String ?? "", group["title"] as? String ?? ""])
                labels.append("\(title) · \(r.show) | \(r.note) | \(r.explainTitle)")
                copies.append(copy)
                sheetOf.append(sheet["id"] as? String ?? "")
            }
        }
    }
}
let indexMs = Date().timeIntervalSince(started) * 1000

guard let text = try? String(contentsOfFile: args[2], encoding: .utf8) else { print("no \(args[2])"); exit(2) }
// query, the row it should find, and (optional) the sheet that's open, whose rows get SheetSearch.openSheetBonus
let cases = text.split(separator: "\n").filter { !$0.hasPrefix("#") && $0.contains("\t") }.map { line -> (String, String, String) in
    let cut = line.split(separator: "\t").map(String.init)
    return (cut[0], cut[1], cut.count > 2 ? cut[2] : "")
}
var passed = 0, first = 0
var times: [Double] = []
for (query, want, open) in cases {
    // typed a character at a time, as the sheet sees it
    var result = search.search("")
    for n in 1...query.count {
        let t = Date()
        result = search.search(String(query.prefix(n)))
        times.append(Date().timeIntervalSince(t) * 1000)
    }
    // the best first; one row once (a one-formula sheet repeats its row)
    var seen = Set<String>()
    let rank = { (n: Int) in result.scores[n] + (sheetOf[n] == open ? SheetSearch.openSheetBonus : 0) }
    let ranked = search.ranked(result, bonus: { sheetOf[$0] == open ? SheetSearch.openSheetBonus : 0 })
        .filter { seen.insert(copies[$0]).inserted }
    let top = ranked.prefix(3)
    let hit = top.firstIndex { labels[$0].lowercased().contains(want.lowercased()) }
    if hit != nil { passed += 1 }
    if hit == top.startIndex { first += 1 }
    if hit == nil || showAll {
        let absent = hit == nil && !labels.contains { $0.lowercased().contains(want.lowercased()) } ? " (no row has it)" : ""
        print("\(hit == nil ? "MISS" : "ok  ") \(query)\(open.isEmpty ? "" : " (on \(open))") -> want \(want)\(absent)\(result.closest ? " (closest)" : "") [\(ranked.count) rows]")
        for n in top { print("       \(rank(n))  \(labels[n].prefix(110))") }
    }
}
times.sort()
let pct = Double(passed) / Double(cases.count) * 100
print(String(format: "%d/%d found in the top 3 (%.0f%%), %d first; %d rows indexed in %.0f ms; a keystroke's search median %.2f ms, max %.2f ms",
             passed, cases.count, pct, first, labels.count, indexMs, times[times.count / 2], times.last ?? 0))
exit(passed == cases.count ? 0 : 1)
