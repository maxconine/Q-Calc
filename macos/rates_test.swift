import Foundation

/// Standalone test for ECBRates. Not part of the app target.
///   swiftc macos/ECBRates.swift macos/rates_test.swift -o /tmp/qcalc-rates-test && /tmp/qcalc-rates-test
@main
enum RatesTest {
    static let daily = """
    <?xml version="1.0" encoding="UTF-8"?>
    <gesmes:Envelope xmlns:gesmes="http://www.gesmes.org/xml/2002-08-01" xmlns="http://www.ecb.int/vocabulary/2002-08-01/eurofxref">
      <gesmes:subject>Reference rates</gesmes:subject>
      <Cube>
        <Cube time='2026-10-22'>
          <Cube currency='USD' rate='1.1204'/>
          <Cube currency='JPY' rate='175.4'/>
          <Cube currency='GBP' rate='0.8375'/>
        </Cube>
      </Cube>
    </gesmes:Envelope>
    """

    static func main() {
        let r = ECBRates.parse(Data(daily.utf8))
        check(r?.date == "2026-10-22", "date")
        check(r?.rates == ["USD": 1.1204, "JPY": 175.4, "GBP": 0.8375], "rates")

        // a currency twice, a bad date, no envelope, two days: not trusted
        check(ECBRates.parse(Data(daily.replacingOccurrences(of: "'GBP'", with: "'USD'").utf8)) == nil, "duplicate")
        check(ECBRates.parse(Data(daily.replacingOccurrences(of: "2026-10-22", with: "yesterday").utf8)) == nil, "date")
        check(ECBRates.parse(Data("<Cube><Cube time='2026-10-22'><Cube currency='USD' rate='1.1'/></Cube></Cube>".utf8)) == nil, "envelope")
        check(ECBRates.parse(Data("not xml".utf8)) == nil, "junk")
        // a nonsense rate is skipped, the rest kept
        let odd = ECBRates.parse(Data(daily.replacingOccurrences(of: "'175.4'", with: "'-3'").utf8))
        check(odd?.rates["JPY"] == nil && odd?.rates["USD"] == 1.1204, "bad rate")

        // a day between tries, a week before rates go stale
        let day = ECBRates.day
        var cache = ECBRates.Cache(checked: 1000, fetched: 1000, rates: r)
        check(!cache.due(1000 + day - 1) && cache.due(1000 + day), "due after a day")
        check(cache.due(10), "a clock set back asks again")
        check(cache.pageValue(1000 + 6 * day) != nil, "fresh for a week")
        check(cache.pageValue(1000 + 7 * day) == nil, "stale after a week")
        // a failed try is asked again in half an hour, not a day; a clock set back still asks at once
        var failed = ECBRates.Cache(checked: 1000, fetched: 500, rates: r)
        failed.failed(1000)
        check(!failed.due(1000 + ECBRates.retry - 1) && failed.due(1000 + ECBRates.retry), "retry after a failure")
        check(failed.fetched == 500 && failed.rates == r, "a failure keeps the cache")
        cache.rates = nil
        check(cache.pageValue(1000) == nil, "nothing to give")

        // the page gets { base, rates }, or null
        let page = ECBRates.javaScript(ECBRates.Cache(checked: 0, fetched: 0, rates: r).pageValue(10))
        check(page == #"{"base":"EUR","rates":{"GBP":0.8375,"JPY":175.4,"USD":1.1204},"date":"2026-10-22"}"#, "page json: \(page)")
        check(ECBRates.javaScript(nil) == "null", "null")

        // the cache round-trips, and the windows app's file reads the same
        let saved = ECBRates.Cache(checked: 5, fetched: 4, rates: r)
        check(ECBRates.Cache.from(saved.json()!) == saved, "round trip")
        let windows = #"{"checked":5,"fetched":4,"date":"2026-10-22","rates":{"USD":1.1204,"EUR":1,"bad":2,"GBP":-1}}"#
        check(ECBRates.Cache.from(Data(windows.utf8)).rates == ECBRates.Rates(date: "2026-10-22", rates: ["USD": 1.1204]), "windows file")
        check(ECBRates.Cache.from(Data("nope".utf8)) == ECBRates.Cache(), "junk cache")
        offline(r!)
        print("rates test passed")
    }

    // no network: the ecb asked at a port nothing listens on (never the real host, no system settings touched).
    // the cached rates stay, the page value doesn't change, and the next try is in half an hour
    static func offline(_ r: ECBRates.Rates) {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("qcalc-rates-\(UUID().uuidString)")
        let path = dir.appendingPathComponent("rates.json")
        let now = Date().timeIntervalSince1970
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try? ECBRates.Cache(checked: now - 2 * ECBRates.day, fetched: now - 2 * ECBRates.day, rates: r).json()!.write(to: path)
        let rates = ECBRates(path: path, url: URL(string: "http://127.0.0.1:9/eurofxref-daily.xml")!)
        let before = rates.pageJSON
        var changed = false
        rates.onChange = { _ in changed = true }
        let finished = DispatchSemaphore(value: 0)
        rates.refresh { finished.signal() }
        check(finished.wait(timeout: .now() + 30) == .success, "offline try finishes")
        let saved = ECBRates.Cache.from(try! Data(contentsOf: path))
        check(rates.pageJSON == before && before.contains("USD") && !changed, "offline keeps the rates")
        check(saved.rates == r && !saved.due(now + 60) && saved.due(now + ECBRates.retry + 60), "offline retries in half an hour: \(saved)")
        // with nothing cached, the page gets null (the bar then says the rates haven't loaded)
        let empty = ECBRates(path: dir.appendingPathComponent("none.json"), url: URL(string: "http://127.0.0.1:9/")!)
        let second = DispatchSemaphore(value: 0)
        empty.refresh { second.signal() }
        check(second.wait(timeout: .now() + 30) == .success && empty.pageJSON == "null", "offline with no cache")
        try? FileManager.default.removeItem(at: dir)
    }

    static func check(_ ok: Bool, _ what: String) {
        if !ok {
            print("FAIL: \(what)")
            exit(1)
        }
    }
}
