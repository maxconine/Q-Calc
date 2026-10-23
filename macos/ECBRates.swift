import Foundation

// the ecb's daily reference rates, which currency conversions use: euro is the base, each rate is units per euro.
// the same source, cache and freshness rules as the windows app (src-tauri/src/rates.rs). the ecb is asked at most
// once a day, the answer is kept on disk so a launch without a network still converts, and rates over a week old
// aren't used. self-contained (no web view) so a native front end can use it too; the overlay hands `pageJSON` to
// the page as window.__QCALC_RATES
final class ECBRates {
    static let shared = ECBRates()

    static let url = URL(string: "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml")!
    static let day: TimeInterval = 24 * 60 * 60
    // older than this and a conversion would quote a rate nobody trades at any more
    static let stale: TimeInterval = 7 * day

    struct Rates: Equatable {
        var date: String
        var rates: [String: Double]
    }

    struct Cache: Equatable {
        // the last attempt, kept even when it failed, so the ecb is asked at most once a day
        var checked: TimeInterval = 0
        var fetched: TimeInterval = 0
        var rates: Rates?

        func due(_ now: TimeInterval) -> Bool {
            now < checked || now - checked >= ECBRates.day
        }

        // what the page reads as __QCALC_RATES, or nil when there's nothing fresh enough
        func pageValue(_ now: TimeInterval) -> [String: Any]? {
            guard let rates, now >= fetched, now - fetched < ECBRates.stale else { return nil }
            return ["base": "EUR", "rates": rates.rates]
        }

        func json() -> Data? {
            var out: [String: Any] = ["checked": checked, "fetched": fetched]
            if let rates {
                out["date"] = rates.date
                out["rates"] = rates.rates
            }
            return try? JSONSerialization.data(withJSONObject: out, options: [.sortedKeys])
        }

        // field by field, so one bad value can't throw the rest away
        static func from(_ data: Data) -> Cache {
            guard let v = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return Cache() }
            var cache = Cache()
            cache.checked = (v["checked"] as? NSNumber)?.doubleValue ?? 0
            cache.fetched = (v["fetched"] as? NSNumber)?.doubleValue ?? 0
            if let date = v["date"] as? String, isDate(date), let all = v["rates"] as? [String: Any] {
                var rates: [String: Double] = [:]
                for (code, n) in all {
                    guard isCode(code), code != "EUR", let n = (n as? NSNumber)?.doubleValue, n.isFinite, n > 0 else { continue }
                    rates[code] = n
                }
                if !rates.isEmpty { cache.rates = Rates(date: date, rates: rates) }
            }
            return cache
        }
    }

    private let lock = NSLock()
    private var cache: Cache
    private let path: URL?
    private var timer: Timer?
    // called on the main thread with the new page value (or nil) whenever it changes
    var onChange: ((String) -> Void)?

    init(path: URL? = ECBRates.defaultPath) {
        self.path = path
        cache = path.flatMap { try? Data(contentsOf: $0) }.map(Cache.from) ?? Cache()
    }

    static var defaultPath: URL? {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first?
            .appendingPathComponent("Q Calc", isDirectory: true)
            .appendingPathComponent("rates.json")
    }

    /// `{ base, rates }` as JavaScript, or `null`, for the page's __QCALC_RATES.
    var pageJSON: String {
        lock.lock()
        defer { lock.unlock() }
        return Self.javaScript(cache.pageValue(Date().timeIntervalSince1970))
    }

    // written by hand so a rate reads as typed (0.8375, not JSONSerialization's 0.83750000000000002)
    static func javaScript(_ value: [String: Any]?) -> String {
        guard let value, let base = value["base"] as? String, let rates = value["rates"] as? [String: Double] else { return "null" }
        let rows = rates.keys.sorted().compactMap { code -> String? in
            guard let n = rates[code], n.isFinite, isCode(code) else { return nil }
            return "\"\(code)\":\(n)"
        }
        return "{\"base\":\"\(base)\",\"rates\":{\(rows.joined(separator: ","))}}"
    }

    /// Checks now and then every hour; fetches only when a day has passed since the last try.
    func start() {
        guard timer == nil else { return }
        refresh()
        let timer = Timer(timeInterval: 60 * 60, repeats: true) { [weak self] _ in self?.refresh() }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    func refresh() {
        let now = Date().timeIntervalSince1970
        lock.lock()
        let due = cache.due(now)
        if due { cache.checked = now }
        lock.unlock()
        guard due else { return }
        var request = URLRequest(url: Self.url, timeoutInterval: 20)
        request.cachePolicy = .reloadIgnoringLocalCacheData
        URLSession.shared.dataTask(with: request) { [weak self] data, _, _ in
            guard let self else { return }
            let before = self.pageJSON
            self.lock.lock()
            if let data, data.count < 1 << 20, let fresh = Self.parse(data) {
                self.cache.rates = fresh
                self.cache.fetched = Date().timeIntervalSince1970
            }
            self.save()
            self.lock.unlock()
            let after = self.pageJSON
            if after != before {
                DispatchQueue.main.async { self.onChange?(after) }
            }
        }.resume()
    }

    // under the lock
    private func save() {
        guard let path, let data = cache.json() else { return }
        try? FileManager.default.createDirectory(at: path.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? data.write(to: path, options: .atomic)
    }

    // the daily file: one dated <Cube time=…> holding <Cube currency=… rate=…/> rows
    static func parse(_ data: Data) -> Rates? {
        let reader = Reader()
        let parser = XMLParser(data: data)
        parser.delegate = reader
        guard parser.parse(), reader.envelope, !reader.broken, reader.days == 1, let date = reader.date, !reader.rates.isEmpty
        else { return nil }
        return Rates(date: date, rates: reader.rates)
    }

    private final class Reader: NSObject, XMLParserDelegate {
        var envelope = false
        var days = 0
        var date: String?
        var rates: [String: Double] = [:]
        var broken = false

        func parser(_ parser: XMLParser, didStartElement name: String, namespaceURI: String?, qualifiedName: String?, attributes: [String: String] = [:]) {
            let local = name.split(separator: ":").last.map(String.init) ?? name
            if local == "Envelope" { envelope = true }
            guard local == "Cube" else { return }
            if let time = attributes["time"] {
                days += 1
                date = isDate(time) ? time : nil
            }
            if let code = attributes["currency"], let text = attributes["rate"] {
                guard isCode(code), code != "EUR", let n = Double(text), n.isFinite, n > 0 else { return }
                // the same currency twice can't be told apart, so the file isn't trusted
                if rates.updateValue(n, forKey: code) != nil { broken = true }
            }
        }
    }

    static func isCode(_ s: String) -> Bool {
        s.count == 3 && s.allSatisfy { $0.isASCII && $0.isUppercase }
    }

    static func isDate(_ s: String) -> Bool {
        let parts = s.split(separator: "-")
        return parts.count == 3 && parts[0].count == 4 && parts[1].count == 2 && parts[2].count == 2
            && parts.allSatisfy { $0.allSatisfy(\.isNumber) }
    }
}
