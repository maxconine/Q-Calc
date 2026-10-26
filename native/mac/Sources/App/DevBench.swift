import Foundation

/// Dev builds only: the typing runs native/bench/app_bench.py times (`drive.sh "bench:<spec.json>"`). The web-view
/// app runs the same spec in its page (native/bench/page_bench.js), so the two are timed the same way.
///
/// spec: `{"mode": "keys"|"session", "inputs": [...], "whole": false, "delayMs": 30, "enter": false, "clear": true, "out": path}`
/// - keys: each input typed a prefix at a time (or `whole`, all at once); each keystroke is timed from the text
///   changing to its live answer landing in the model (and the main queue turning once more), then the next.
/// - session: typed at a fixed pace without waiting, the answer awaited at the end of each line, ↵ if `enter`,
///   then the line cleared if `clear` (default: when not entered).
/// Writes `{"ms": [...], "idx": [...], "timeouts": n, "wallMs": total}` to `out`.
enum DevBench {
    private struct Spec: Decodable {
        let mode: String
        let inputs: [String]
        let whole: Bool?
        let delayMs: Double?
        let enter: Bool?
        let clear: Bool?
        let out: String
    }

    private static var keep: Runner?

    static func run(spec path: String, model: BarModel) {
        guard let data = FileManager.default.contents(atPath: path),
              let spec = try? JSONDecoder().decode(Spec.self, from: data) else {
            NSLog("Q Calc dev: bad bench spec %@", path)
            return
        }
        let runner = Runner(spec: spec, model: model)
        keep = runner
        runner.start()
    }

    private final class Runner {
        let spec: Spec
        let model: BarModel
        var ms: [Double] = []
        var idx: [Int] = []
        var timeouts = 0
        let started = DispatchTime.now().uptimeNanoseconds
        var waiting: String?
        var landed: (() -> Void)?
        var token = 0

        init(spec: Spec, model: BarModel) {
            self.spec = spec
            self.model = model
        }

        static func now() -> Double { Double(DispatchTime.now().uptimeNanoseconds) / 1_000_000 }

        func start() {
            model.onLive = { [weak self] typed in
                guard let self, typed == self.waiting, let landed = self.landed else { return }
                self.waiting = nil
                self.landed = nil
                landed()
            }
            model.text = ""
            DispatchQueue.main.async { self.input(0) }
        }

        /// sets the text and calls `then` once its answer lands (or after 10 s)
        func type(_ text: String, then: @escaping (Bool) -> Void) {
            token += 1
            let mine = token
            waiting = text
            landed = { then(true) }
            model.text = text
            DispatchQueue.main.asyncAfter(deadline: .now() + 10) { [weak self] in
                guard let self, self.token == mine, self.landed != nil else { return }
                self.waiting = nil
                self.landed = nil
                then(false)
            }
        }

        func prefixes(_ s: String) -> [String] {
            if spec.whole == true { return [s] }
            var out: [String] = []
            var acc = ""
            for c in s { acc.append(c); out.append(acc) }
            return out
        }

        func input(_ i: Int) {
            guard i < spec.inputs.count else { return finish() }
            let steps = prefixes(spec.inputs[i])
            if spec.mode == "session" { paced(i, steps, 0) } else { key(i, steps, 0) }
        }

        func key(_ i: Int, _ steps: [String], _ k: Int) {
            guard k < steps.count else {
                model.text = ""
                DispatchQueue.main.async { self.input(i + 1) }
                return
            }
            let t0 = Self.now()
            type(steps[k]) { ok in
                DispatchQueue.main.async {
                    if ok { self.ms.append(Self.now() - t0); self.idx.append(i) } else { self.timeouts += 1 }
                    self.key(i, steps, k + 1)
                }
            }
        }

        func paced(_ i: Int, _ steps: [String], _ k: Int) {
            let delay = (spec.delayMs ?? 30) / 1000
            if k < steps.count - 1 {
                model.text = steps[k]
                DispatchQueue.main.asyncAfter(deadline: .now() + delay) { self.paced(i, steps, k + 1) }
                return
            }
            type(steps[k]) { ok in
                if !ok { self.timeouts += 1 }
                if self.spec.enter == true { self.model.enter() }
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
                    if self.spec.clear ?? (self.spec.enter != true) { self.model.text = "" }
                    self.input(i + 1)
                }
            }
        }

        func finish() {
            model.onLive = nil
            let wall = Double(DispatchTime.now().uptimeNanoseconds - started) / 1_000_000
            let out: [String: Any] = ["ms": ms, "idx": idx, "timeouts": timeouts, "wallMs": wall]
            if let data = try? JSONSerialization.data(withJSONObject: out) {
                try? data.write(to: URL(fileURLWithPath: spec.out), options: .atomic)
            }
            DevBench.keep = nil
        }
    }
}
