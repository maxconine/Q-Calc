import Foundation
import JavaScriptCore
import QCalcCore

/// The TS app logic by module and name (`lib/blankReason` `blankReason`…), the same shape as `Engine.callJson`.
/// The Rust engine answers whatever it has ported; the rest runs as the TS itself (Resources/applogic.js, built by
/// applogic/build.sh) in JavaScriptCore — an in-process interpreter, not a web view, so no WebContent process.
/// When WP-APP-LOGIC registers a function in the engine, calls move there with no change here.
///
/// Not thread-safe: make one per thread (BarModel keeps one on the main thread, EngineThread one of its own).
final class AppLogic {
    private let engine: Engine
    private let context: JSContext
    private let caller: JSValue?
    /// (module.function)s the engine said it doesn't have, so they go straight to the bundle
    private var unported: Set<String> = []
    /// `ok(text)` for blankReason and missingUnit: whether a piece of text has a real answer
    var ok: (String) -> Bool = { _ in false }

    init(engine: Engine) {
        self.engine = engine
        context = JSContext(virtualMachine: JSVirtualMachine())!
        context.name = "Q Calc app logic"
        context.exceptionHandler = { _, value in NSLog("Q Calc app logic: %@", value?.toString() ?? "?") }
        let host = JSValue(newObjectIn: context)!
        let call: @convention(block) (String, String, String) -> String = { [weak engine] module, fn, args in
            guard let engine, let out = try? engine.callJson(module: module, function: fn, argsJson: args) else { return "{}" }
            return "{\"value\":\(out)}"
        }
        host.setObject(call, forKeyedSubscript: "call" as NSString)
        context.setObject(host, forKeyedSubscript: "__host" as NSString)
        if let url = Bundle.main.url(forResource: "applogic", withExtension: "js"), let source = try? String(contentsOf: url) {
            context.evaluateScript(source, withSourceURL: url)
        } else {
            NSLog("Q Calc: applogic.js is missing from the bundle")
        }
        caller = context.objectForKeyedSubscript("QCalcLogic")?.objectForKeyedSubscript("call")
        let okBlock: @convention(block) (String) -> Bool = { [weak self] text in self?.ok(text) ?? false }
        host.setObject(okBlock, forKeyedSubscript: "ok" as NSString)
    }

    /// The function's result as Foundation JSON values (`NSNull` for null), or nil when neither side has it.
    func call(_ module: String, _ fn: String, _ args: [Any]) -> Any? {
        guard let data = try? JSONSerialization.data(withJSONObject: args, options: [.fragmentsAllowed]),
              let argsJson = String(data: data, encoding: .utf8) else { return nil }
        let key = "\(module).\(fn)"
        if !unported.contains(key) {
            do {
                let out = try engine.callJson(module: module, function: fn, argsJson: argsJson)
                return try? JSONSerialization.jsonObject(with: Data(out.utf8), options: [.fragmentsAllowed])
            } catch CoreError.Unported {
                unported.insert(key)
            } catch {
                unported.insert(key)
            }
        }
        guard let caller, let reply = caller.call(withArguments: [module, fn, argsJson])?.toString(),
              let object = try? JSONSerialization.jsonObject(with: Data(reply.utf8)) as? [String: Any],
              let value = object["value"] else { return nil }
        return value
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
