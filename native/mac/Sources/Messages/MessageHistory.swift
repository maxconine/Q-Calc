import Foundation

/// One line of a conversation: who said it (you or them), what, and when (ms since 1970).
struct ThreadMessage: Codable, Equatable, Identifiable {
    let id: String
    let at: Double
    /// "sent" (you said it) or "received"
    let dir: String
    let text: String

    var mine: Bool { dir == "sent" }
}

/// The message threads (native only): every message sent (once the relay delivered it) and every message shown
/// from an accepted name, per contact, kept in `<support folder>/messages.json` on this Mac, at most `cap` a
/// contact. Requests nobody answered and blocked names' messages never reach it (Messages.swift calls in only
/// from `deliver` and a delivered `send`). Read at launch and written off the main thread; the main thread only
/// ever touches the copy in memory.
final class MessageHistory: ObservableObject {
    static let shared = MessageHistory(url: AppInfo.supportDirectory.appendingPathComponent("messages.json"))
    static let cap = 200

    /// each contact's messages, oldest first
    @Published private(set) var threads: [String: [ThreadMessage]] = [:]

    private let url: URL
    private let queue = DispatchQueue(label: "com.maxconine.qcalc.message-history", qos: .utility)
    private var loaded = false
    /// changes made before the file was read, applied on top of it
    private var pending: [(inout [String: [ThreadMessage]]) -> Void] = []

    private struct File: Codable {
        var version = 1
        var threads: [String: [ThreadMessage]]
    }

    init(url: URL) {
        self.url = url
        queue.async { [weak self] in
            guard let self else { return }
            let read = (try? Data(contentsOf: url)).flatMap { try? JSONDecoder().decode(File.self, from: $0) }
            DispatchQueue.main.async {
                var threads = Self.sanitized(read?.threads ?? [:])
                for change in self.pending { change(&threads) }
                self.pending = []
                self.loaded = true
                self.threads = threads
                if self.pendingSave { self.save() }
            }
        }
    }

    private var pendingSave = false

    /// only clean names, clean text, the newest `cap` each
    private static func sanitized(_ raw: [String: [ThreadMessage]]) -> [String: [ThreadMessage]] {
        var out: [String: [ThreadMessage]] = [:]
        for (who, list) in raw {
            let name = Messages.clean(who)
            guard !name.isEmpty else { continue }
            let kept = list.filter { ($0.dir == "sent" || $0.dir == "received") && !$0.text.isEmpty }
                .map { ThreadMessage(id: $0.id, at: $0.at, dir: $0.dir, text: MessageInbox.cleanText($0.text, max: Messages.textMax)) }
                .sorted { $0.at < $1.at }
            if !kept.isEmpty { out[name, default: []] += kept.suffix(cap) }
        }
        return out
    }

    private func change(_ apply: @escaping (inout [String: [ThreadMessage]]) -> Void) {
        guard loaded else {
            pending.append(apply)
            pendingSave = true
            return
        }
        var next = threads
        apply(&next)
        threads = next
        save()
    }

    /// a message to or from `who`
    func add(with who: String, mine: Bool, text raw: String, at date: Date = Date()) {
        let name = Messages.clean(who)
        let text = MessageInbox.cleanText(raw, max: Messages.textMax)
        guard !name.isEmpty, !text.isEmpty else { return }
        let message = ThreadMessage(id: UUID().uuidString, at: (date.timeIntervalSince1970 * 1000).rounded(),
                                    dir: mine ? "sent" : "received", text: text)
        change { threads in
            var list = threads[name] ?? []
            list.append(message)
            if list.count > Self.cap { list.removeFirst(list.count - Self.cap) }
            threads[name] = list
        }
    }

    /// their thread goes (settings' Remove / Unblock: forgetting a name)
    func forget(_ who: String) {
        let name = Messages.clean(who)
        guard !name.isEmpty else { return }
        change { $0.removeValue(forKey: name) }
    }

    func clearAll() {
        change { $0 = [:] }
    }

    func thread(_ who: String) -> [ThreadMessage] { threads[who] ?? [] }

    var isEmpty: Bool { threads.values.allSatisfy(\.isEmpty) }

    /// each conversation's latest line, the latest conversation first
    var conversations: [(name: String, last: ThreadMessage)] {
        threads.compactMap { name, list in list.last.map { (name, $0) } }
            .sorted { $0.last.at != $1.last.at ? $0.last.at > $1.last.at : $0.name < $1.name }
    }

    /// the names in the history, the latest conversation first
    var names: [String] { conversations.map(\.name) }

    private func save() {
        pendingSave = false
        let snapshot = File(threads: threads)
        let url = self.url
        queue.async {
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys]
            guard let data = try? encoder.encode(snapshot) else { return }
            try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try? data.write(to: url, options: [.atomic])
        }
    }

    /// waits for the writes queued so far (tests and dev checks)
    func flush() { queue.sync {} }
}

/// How a message's time reads in the list and the thread: the time today, `yesterday`, the weekday this week,
/// else the date; lowercase, as the bar's words are.
enum ThreadTime {
    private static let time: DateFormatter = { let f = DateFormatter(); f.dateStyle = .none; f.timeStyle = .short; return f }()
    private static let weekday: DateFormatter = { let f = DateFormatter(); f.setLocalizedDateFormatFromTemplate("EEE"); return f }()
    private static let day: DateFormatter = { let f = DateFormatter(); f.setLocalizedDateFormatFromTemplate("MMMd"); return f }()

    static func label(_ ms: Double, now: Date = Date()) -> String {
        let date = Date(timeIntervalSince1970: ms / 1000)
        let cal = Calendar.current
        if cal.isDate(date, inSameDayAs: now) { return time.string(from: date).lowercased() }
        if let y = cal.date(byAdding: .day, value: -1, to: now), cal.isDate(date, inSameDayAs: y) { return "yesterday" }
        if let week = cal.date(byAdding: .day, value: -6, to: cal.startOfDay(for: now)), date >= week, date <= now {
            return weekday.string(from: date).lowercased()
        }
        return day.string(from: date).lowercased()
    }

    /// the time with the day when it isn't today, for a thread's lines
    static func full(_ ms: Double, now: Date = Date()) -> String {
        let date = Date(timeIntervalSince1970: ms / 1000)
        if Calendar.current.isDate(date, inSameDayAs: now) { return time.string(from: date).lowercased() }
        return "\(label(ms, now: now)) \(time.string(from: date).lowercased())"
    }
}
