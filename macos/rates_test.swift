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
        cache.rates = nil
        check(cache.pageValue(1000) == nil, "nothing to give")

        // the page gets { base, rates }, or null
        let page = ECBRates.javaScript(ECBRates.Cache(checked: 0, fetched: 0, rates: r).pageValue(10))
        check(page == #"{"base":"EUR","rates":{"GBP":0.8375,"JPY":175.4,"USD":1.1204}}"#, "page json: \(page)")
        check(ECBRates.javaScript(nil) == "null", "null")

        // the cache round-trips, and the windows app's file reads the same
        let saved = ECBRates.Cache(checked: 5, fetched: 4, rates: r)
        check(ECBRates.Cache.from(saved.json()!) == saved, "round trip")
        let windows = #"{"checked":5,"fetched":4,"date":"2026-10-22","rates":{"USD":1.1204,"EUR":1,"bad":2,"GBP":-1}}"#
        check(ECBRates.Cache.from(Data(windows.utf8)).rates == ECBRates.Rates(date: "2026-10-22", rates: ["USD": 1.1204]), "windows file")
        check(ECBRates.Cache.from(Data("nope".utf8)) == ECBRates.Cache(), "junk cache")
        print("rates test passed")
    }

    static func check(_ ok: Bool, _ what: String) {
        if !ok {
            print("FAIL: \(what)")
            exit(1)
        }
    }
}
