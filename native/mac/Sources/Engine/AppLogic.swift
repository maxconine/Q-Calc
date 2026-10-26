import Foundation
import QCalcCore

/// The ported TS app logic by module and name (`lib/tour` `tourStep`…), answered by the Rust engine through
/// `Engine.callJson`, with Foundation JSON values in and out. Everything the bar needs is in the core; no script
/// runs in the app.
///
/// Not thread-safe only in that it shares the engine's own rules: make one per thread (BarModel keeps one on the
/// main thread, EngineThread one of its own).
final class AppLogic {
    private let engine: Engine
    /// (module.function)s the engine said it doesn't have, so they aren't asked again
    private var unported: Set<String> = []

    init(engine: Engine) {
        self.engine = engine
    }

    /// The function's result as Foundation JSON values (`NSNull` for null), or nil when the engine has none.
    func call(_ module: String, _ fn: String, _ args: [Any]) -> Any? {
        let key = "\(module).\(fn)"
        guard !unported.contains(key),
              let data = try? JSONSerialization.data(withJSONObject: args, options: [.fragmentsAllowed]),
              let argsJson = String(data: data, encoding: .utf8) else { return nil }
        do {
            let out = try engine.callJson(module: module, function: fn, argsJson: argsJson)
            return try? JSONSerialization.jsonObject(with: Data(out.utf8), options: [.fragmentsAllowed])
        } catch CoreError.Unported {
            NSLog("Q Calc: the engine has no %@", key)
            unported.insert(key)
        } catch {
            NSLog("Q Calc: %@ failed: %@", key, String(describing: error))
        }
        return nil
    }

    func string(_ module: String, _ fn: String, _ args: [Any]) -> String? { call(module, fn, args) as? String }
    func bool(_ module: String, _ fn: String, _ args: [Any]) -> Bool { call(module, fn, args) as? Bool ?? false }

    /// `{start, end}` spans, UTF-16 offsets.
    func spans(_ module: String, _ fn: String, _ args: [Any]) -> [NSRange] {
        (call(module, fn, args) as? [[String: Any]] ?? []).compactMap { s in
            guard let a = (s["start"] as? NSNumber)?.intValue, let b = (s["end"] as? NSNumber)?.intValue, b > a else { return nil }
            return NSRange(location: a, length: b - a)
        }
    }
}
