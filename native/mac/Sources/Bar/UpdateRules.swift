import Foundation

// The rules for the bar's update line (UpdateNotice.swift), Foundation only so Tests/update checks them headless.
enum UpdateRules {
    /// "3.0.10" > "3.0.9"; a leading v, missing parts (3.1 is 3.1.0) and a -beta tail are allowed
    static func isNewer(_ latest: String, than current: String) -> Bool {
        let a = parts(latest), b = parts(current)
        guard !a.isEmpty else { return false }
        for i in 0..<max(a.count, b.count) {
            let x = i < a.count ? a[i] : 0, y = i < b.count ? b[i] : 0
            if x != y { return x > y }
        }
        return false
    }

    private static func parts(_ v: String) -> [Int] {
        var s = v.trimmingCharacters(in: .whitespaces)
        if s.first == "v" || s.first == "V" { s.removeFirst() }
        let core = s.split(separator: "-", maxSplits: 1).first.map(String.init) ?? ""
        let nums = core.split(separator: ".", omittingEmptySubsequences: false).map { Int($0) }
        return nums.contains(nil) ? [] : nums.compactMap { $0 }
    }

    /// what the line says, or nil when there's nothing to say: no newer version, or this one was let go this run
    static func line(latest: String?, current: String, ready: Bool, dismissed: String?) -> (text: String, action: String)? {
        guard let latest, isNewer(latest, than: current), latest != dismissed else { return nil }
        return ready ? ("Update ready: \(latest)", "install now") : ("Update available: \(latest)", "update")
    }
}
