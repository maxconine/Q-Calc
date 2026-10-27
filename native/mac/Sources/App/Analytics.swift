import Foundation
import Security

/// Anonymous usage counts, as src/lib/analytics.ts keeps them: a stash of daily counts per event (`bump`), sent every
/// six hours (`dueToSend`, `usagePayload`, `afterSend`) while "Share anonymous usage" is on, forgotten when it's
/// turned off. The stash lives where the web app's did (UserDefaults `qcalc.analytics` through AppSettings). The
/// counting rules are the TS's, ported to the engine (through AppLogic); dev builds count but never send.
final class Analytics {
    static let shared = Analytics()
    private var logic: AppLogic?
    private var stash: Any?
    private var sending = false
    private var saveWork: DispatchWorkItem?

    func start(logic: AppLogic) {
        self.logic = logic
        setSharing(AppSettings.shared.shareUsage)
    }

    private var enabled: Bool { AppSettings.shared.shareUsage && logic != nil }
    private var day: String {
        let now = Date()
        return logic?.string("lib/analytics", "localDay", [now.timeIntervalSince1970 * 1000,
                                                             TimeZone.current.secondsFromGMT(for: now) / 60]) ?? ""
    }

    /// newInstallId: 16 random bytes as 32 hex digits
    private static func newInstallId() -> String {
        var bytes = [UInt8](repeating: 0, count: 16)
        if SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) != errSecSuccess {
            return UUID().uuidString.replacingOccurrences(of: "-", with: "").lowercased()
        }
        return bytes.map { String(format: "%02x", $0) }.joined()
    }

    /// setUsageSharing: on starts (or resumes) the stash with `launch` (and `update` after a new version)
    func setSharing(_ on: Bool) {
        guard let logic else { return }
        guard on else {
            stash = nil
            AppSettings.shared.saveAnalytics(nil)
            return
        }
        let raw = (try? JSONSerialization.jsonObject(with: Data(AppSettings.shared.analyticsJSON().utf8), options: [.fragmentsAllowed])) ?? NSNull()
        let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0.0.0"
        var next: Any
        if let kept = logic.call("lib/analytics", "sanitizeStash", [raw]) as? [String: Any] {
            next = kept
            if let v = kept["version"] as? String, !v.isEmpty, v != version {
                next = logic.call("lib/analytics", "bump", [next, "update", day]) ?? next
            }
        } else {
            next = ["id": Self.newInstallId(),
                    "version": version, "sentAt": 0, "days": [String: Any]()]
        }
        if var d = next as? [String: Any] { d["version"] = version; next = d }
        stash = logic.call("lib/analytics", "bump", [next, "launch", day]) ?? next
        persist()
        maybeSend()
    }

    func track(_ event: String) {
        guard enabled, let logic, let stash else { return }
        self.stash = logic.call("lib/analytics", "bump", [stash, event, day]) ?? stash
        persist()
        maybeSend()
    }

    /// calcKind for a saved line
    func trackCalc(expr: String, system: Bool = false, graph: Bool = false, fn: Bool = false, solve: Bool = false,
                   assign: Bool = false, unit: Bool = false, chained: Bool = false, native: Bool = false) {
        guard enabled, let logic else { return }
        let facts: [String: Any] = ["expr": expr, "system": system, "graph": graph, "fn": fn, "solve": solve, "assign": assign,
                                    "unit": unit, "chained": chained, "native": native]
        if let kind = logic.string("lib/analytics", "calcKind", [facts]) { track(kind) }
    }

    // saved two seconds after the last count, as the page did
    private func persist() {
        saveWork?.cancel()
        let work = DispatchWorkItem { [weak self] in AppSettings.shared.saveAnalytics(self?.stash) }
        saveWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 2, execute: work)
    }

    private func maybeSend() {
        guard enabled, !AppInfo.isDev, !sending, let logic, let stash,
              logic.bool("lib/analytics", "dueToSend", [stash, Date().timeIntervalSince1970 * 1000]),
              let url = (logic.call("lib/analytics", "ANALYTICS_URL", []) as? String).flatMap(URL.init(string:)) else { return }
        let os = ProcessInfo.processInfo.operatingSystemVersion
        guard let body = logic.call("lib/analytics", "usagePayload", [stash, "mac", "macOS \(os.majorVersion).\(os.minorVersion)"]),
              let data = try? JSONSerialization.data(withJSONObject: body) else { return }
        let sent = (stash as? [String: Any])?["days"] ?? [String: Any]()
        sending = true
        var request = URLRequest(url: url, timeoutInterval: 15)
        request.httpMethod = "POST"
        // text/plain keeps it a simple request, as the page's was
        request.setValue("text/plain", forHTTPHeaderField: "content-type")
        request.httpBody = data
        URLSession.shared.dataTask(with: request) { [weak self] _, response, _ in
            DispatchQueue.main.async {
                guard let self else { return }
                self.sending = false
                guard (response as? HTTPURLResponse).map({ (200..<300).contains($0.statusCode) }) == true,
                      self.enabled, let current = self.stash, let logic = self.logic else { return }
                self.stash = logic.call("lib/analytics", "afterSend", [current, sent, self.day, Date().timeIntervalSince1970 * 1000]) ?? current
                self.persist()
            }
        }.resume()
    }
}

/// `track(event)` from anywhere in the bar
func track(_ event: String) { Analytics.shared.track(event) }
