import Foundation
import QCalcCore

/// The tape: the engine loads, commits and saves it (`<dataDir>/history.json`, the TS HistoryRow JSON); this keeps
/// the copy the views draw and works out which rows show (historyShow.ts).
final class HistoryStore {
    static let recentMs: Int64 = 5 * 60 * 1000
    static let recentRows = 5

    private(set) var rows: [HistoryRow] = []
    private let engine: Engine
    private let queue = DispatchQueue(label: "qcalc.history", qos: .utility)

    init(engine: Engine) {
        self.engine = engine
        rows = engine.loadHistory()
    }

    /// The history after a commit, or any edit; saved off the main thread.
    func replace(_ next: [HistoryRow]) {
        rows = next
        let engine = self.engine
        queue.async { _ = try? engine.saveHistory(rows: next) }
    }

    @discardableResult
    func remove(at index: Int) -> HistoryRow? {
        guard rows.indices.contains(index) else { return nil }
        var next = rows
        let row = next.remove(at: index)
        replace(next)
        return row
    }

    func insert(_ row: HistoryRow, at index: Int) {
        var next = rows
        next.insert(row, at: min(max(0, index), next.count))
        replace(next)
    }

    func clear() { replace([]) }

    private static func nowMs(_ now: Date) -> Int64 { Int64(now.timeIntervalSince1970 * 1000) }

    /// recentStart: the first row of the recent view (rows.count when none)
    func recentStart(now: Date = Date()) -> Int {
        let t = Self.nowMs(now)
        var i = rows.count
        while i > 0, rows.count - i < Self.recentRows, let at = rows[i - 1].at, at <= t, t - at <= Self.recentMs { i -= 1 }
        return i
    }

    /// nextRecentExpiry: when the recent view next changes on its own
    func nextRecentExpiry(now: Date = Date()) -> Date? {
        let ends = rows[recentStart(now: now)...].compactMap { $0.at.map { Date(timeIntervalSince1970: Double($0 + Self.recentMs + 1) / 1000) } }
        return ends.min()
    }
}
