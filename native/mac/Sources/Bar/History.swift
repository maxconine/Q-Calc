import Foundation

/// One row of the tape, in the TS `HistoryRow` shape (src/lib/history.ts), so history.json is the file the core's
/// persistableHistory/normalizeHistoryRow will read and write (ARCHITECTURE.md §8).
struct HistoryRow: Codable, Equatable, Identifiable {
    var id: String
    var expr: String
    var display: String
    var exact: String?
    var n: Double?
    var quantity: String?
    var kind: String?
    var fnName: String?
    var fnParams: [String]?
    var fnBody: String?
    /// commit time in ms
    var at: Double?

    var hasDual: Bool {
        guard let exact = exact?.trimmingCharacters(in: .whitespaces), !exact.isEmpty else { return false }
        return exact != display.trimmingCharacters(in: .whitespaces)
    }

    /// The row defines something later lines can use.
    var defines: Bool { kind == "definition" || kind == "function" }

    static func newID() -> String {
        let ms = String(Int(Date().timeIntervalSince1970 * 1000), radix: 36)
        let rand = String(UInt32.random(in: 0...UInt32.max), radix: 36)
        return "\(ms)-\(rand.prefix(6))"
    }
}

/// The tape and its file. MAX_HISTORY and the recent window are the TS's (history.ts, historyShow.ts).
final class HistoryStore {
    static let maxRows = 10
    static let recentSeconds: TimeInterval = 5 * 60
    static let recentRows = 5
    private static let maxDisplay = 96
    private static let maxExpr = 240

    private(set) var rows: [HistoryRow] = []
    private let url: URL
    private let queue = DispatchQueue(label: "qcalc.history", qos: .utility)

    init(directory: URL) {
        url = directory.appendingPathComponent("history.json")
        load()
    }

    private struct File: Codable {
        var version: Int
        var rows: [HistoryRow]
    }

    private func load() {
        guard let data = try? Data(contentsOf: url), let file = try? JSONDecoder().decode(File.self, from: data) else { return }
        rows = Array(file.rows.suffix(Self.maxRows))
    }

    private func save() {
        let file = File(version: 1, rows: rows)
        let url = self.url
        queue.async {
            guard let data = try? JSONEncoder().encode(file) else { return }
            try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try? data.write(to: url, options: .atomic)
        }
    }

    func append(_ row: HistoryRow) {
        var row = row
        if row.display.count > Self.maxDisplay { row.display = String(row.display.prefix(Self.maxDisplay - 1)) + "…" }
        if row.expr.count > Self.maxExpr { row.expr = String(row.expr.prefix(Self.maxExpr - 1)) + "…" }
        // the same line again moves to the bottom rather than repeating
        if let last = rows.last, last.expr == row.expr, last.display == row.display { rows.removeLast() }
        rows.append(row)
        if rows.count > Self.maxRows { rows.removeFirst(rows.count - Self.maxRows) }
        save()
    }

    @discardableResult
    func remove(at index: Int) -> HistoryRow? {
        guard rows.indices.contains(index) else { return nil }
        let row = rows.remove(at: index)
        save()
        return row
    }

    func insert(_ row: HistoryRow, at index: Int) {
        rows.insert(row, at: min(max(0, index), rows.count))
        save()
    }

    func clear() {
        rows = []
        save()
    }

    /// recentStart: the first row of the recent view (rows.count when none)
    func recentStart(now: Date = Date()) -> Int {
        let nowMs = now.timeIntervalSince1970 * 1000
        var i = rows.count
        while i > 0, rows.count - i < Self.recentRows, let at = rows[i - 1].at, at <= nowMs, nowMs - at <= Self.recentSeconds * 1000 {
            i -= 1
        }
        return i
    }

    /// When the recent view next changes on its own.
    func nextRecentExpiry(now: Date = Date()) -> Date? {
        let start = recentStart(now: now)
        let ends = rows[start...].compactMap { $0.at.map { Date(timeIntervalSince1970: ($0 + Self.recentSeconds * 1000 + 1) / 1000) } }
        return ends.min()
    }

    /// scopeStart: variables count only from rows you can see
    func scopeStart(show: String, now: Date = Date()) -> Int {
        show == "always" ? 0 : recentStart(now: now)
    }
}
