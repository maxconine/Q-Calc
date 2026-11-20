import Foundation
import QCalcCore

// The one serial thread the bar calls the engine from, so the main thread never waits on maths. (QCalcCore hops
// each call to its own 32 MB evaluation thread; this thread only waits for the answer.)
//
// Jobs come in lanes. A lane holds at most one waiting job: a newer one replaces it, so a burst of keystrokes
// evaluates only the latest text, and the stale ones never run. Each job carries a generation; a result whose
// generation is no longer the lane's newest is dropped before it reaches the main thread, so a slow answer can't
// overwrite a newer one. Long work (closed forms, graphs) goes through QCalcCore's async calls with a CancelToken
// instead, cancelled when the line changes.
final class EngineThread {
    enum Lane: Hashable { case live, forms, graph, commit, misc, selection, service, warm }

    private struct Job {
        let generation: Int
        let work: (Engine) -> Void
    }

    private let engine: Engine
    /// the app logic for jobs on this thread (blankReason and friends call back into the engine); only touched
    /// from inside a job
    private var threadLogic: AppLogic?
    var logic: AppLogic {
        if let threadLogic { return threadLogic }
        let made = AppLogic(engine: engine)
        threadLogic = made
        return made
    }
    private let lock = NSCondition()
    private var waiting: [Lane: Job] = [:]
    private var order: [Lane] = []
    private var newest: [Lane: Int] = [:]
    private var thread: Thread?

    init(engine: Engine) {
        self.engine = engine
        let thread = Thread { [weak self] in self?.run() }
        thread.stackSize = 4 << 20
        thread.name = "qcalc.engine"
        thread.qualityOfService = .userInteractive
        self.thread = thread
        thread.start()
        // the evaluation and app-logic threads keep regexes of their own, freed while the bar is hidden (`hidden`)
        _ = engine.ownCompiled()
    }

    // MARK: - compiled regexes while hidden (QA 69, as the Windows app does)

    /// How long the bar stays hidden before the engine's compiled regexes go (a quick hide and show keeps them).
    static let releaseAfter: TimeInterval = 30
    /// Main thread only.
    private var releaseTimer: DispatchWorkItem?
    private var released = false

    /// The bar hid: after `releaseAfter` with no showing, the evaluation and app-logic threads' compiled regexes and
    /// their caches (~6-9 MB after some typing) are freed, between jobs. Main thread.
    func hidden() {
        releaseTimer?.cancel()
        let item = DispatchWorkItem { [weak self] in
            guard let self else { return }
            self.released = true
            let t = ProcessInfo.processInfo.systemUptime
            self.submit(.warm, { engine -> (UInt32, Int) in
                let n = engine.releaseCompiled()
                // the freed blocks' pages go back to the system now, not at the next memory pressure
                return (n, malloc_zone_pressure_relief(nil, 0))
            }) { n, bytes in
                OverlayLog.note(String(format: "engine released %d compiled statics, %d KB of pages, in %.1f ms", Int(n),
                                       bytes / 1024, (ProcessInfo.processInfo.systemUptime - t) * 1000))
            }
        }
        releaseTimer = item
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.releaseAfter, execute: item)
    }

    /// The bar shows: a pending release is called off; after one, the keystroke path compiles again in small
    /// steps, each queued behind whatever the bar asked for meanwhile, so a key typed at once goes first. Main thread.
    func shown() {
        releaseTimer?.cancel()
        releaseTimer = nil
        guard released else { return }
        released = false
        cancel(.warm)
        warm(0)
    }

    /// The first uses after a release (as at launch), one step a job.
    private static let warmSteps: [(Engine) -> Void] = {
        let options = defaultEvalOptions()
        var steps: [(Engine) -> Void] = [{ _ = $0.evaluate(lines: ["2+2", "sin(90)", "72 f to c"], options: options) }]
        for text in ["2", "2+", "2+2", "si", "sin(9", "3x+2=1", "5 km in miles", "x^2 = 9"] {
            steps.append { engine in
                _ = engine.live(request: LiveRequest(text: text, options: options, context: EvalContext(), chained: false,
                                                     nowMs: Int64(Date().timeIntervalSince1970 * 1000)))
            }
        }
        steps.append { _ = $0.typstSource(expr: "sqrt(2) x^2", answer: "4", solvedFor: "") }
        return steps
    }()

    private func warm(_ i: Int) {
        guard i < Self.warmSteps.count, !released else { return }
        let step = Self.warmSteps[i]
        submit(.warm, { step($0) }) { [weak self] _ in self?.warm(i + 1) }
    }

    /// Runs `work` on the engine thread and hands its result to `done` on the main thread, unless a newer job was
    /// submitted to the same lane meanwhile.
    func submit<T>(_ lane: Lane, _ work: @escaping (Engine) -> T, done: @escaping (T) -> Void) {
        lock.lock()
        let generation = (newest[lane] ?? 0) + 1
        newest[lane] = generation
        let job = Job(generation: generation) { [weak self] engine in
            let result = work(engine)
            DispatchQueue.main.async {
                guard let self, self.isNewest(lane, generation) else { return }
                done(result)
            }
        }
        if waiting[lane] == nil { order.append(lane) }
        waiting[lane] = job
        lock.signal()
        lock.unlock()
    }

    /// Drops whatever is waiting in the lane and any result still on its way.
    func cancel(_ lane: Lane) {
        lock.lock()
        newest[lane] = (newest[lane] ?? 0) + 1
        waiting[lane] = nil
        order.removeAll { $0 == lane }
        lock.unlock()
    }

    /// For the rare caller that must wait (tests, the launch timing): runs on the engine thread and blocks.
    func sync<T>(_ work: @escaping (Engine) -> T) -> T {
        let done = DispatchSemaphore(value: 0)
        var out: T?
        lock.lock()
        let generation = (newest[.misc] ?? 0) + 1
        newest[.misc] = generation
        if waiting[.misc] == nil { order.append(.misc) }
        waiting[.misc] = Job(generation: generation) { engine in
            out = work(engine)
            done.signal()
        }
        lock.signal()
        lock.unlock()
        done.wait()
        return out!
    }

    /// Like `sync`, on its own lane and giving up after `timeout` seconds (nil): for callers on the main thread that
    /// must answer in place, like the Services menu's Calculate. A job still running when it gives up finishes and is
    /// dropped.
    func wait<T>(_ lane: Lane, timeout: TimeInterval, _ work: @escaping (Engine) -> T) -> T? {
        let box = WaitBox<T>()
        lock.lock()
        let generation = (newest[lane] ?? 0) + 1
        newest[lane] = generation
        if waiting[lane] == nil { order.append(lane) }
        waiting[lane] = Job(generation: generation) { engine in
            box.value = work(engine)
            box.done.signal()
        }
        lock.signal()
        lock.unlock()
        guard box.done.wait(timeout: .now() + timeout) == .success else {
            cancel(lane)
            return nil
        }
        return box.value
    }

    private func isNewest(_ lane: Lane, _ generation: Int) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return newest[lane] == generation
    }

    private func run() {
        while true {
            lock.lock()
            while order.isEmpty { lock.wait() }
            let lane = order.removeFirst()
            let job = waiting.removeValue(forKey: lane)
            let current = newest[lane]
            lock.unlock()
            guard let job, job.generation == current else { continue }
            autoreleasepool { job.work(engine) }
        }
    }
}

private final class WaitBox<T> {
    var value: T?
    let done = DispatchSemaphore(value: 0)
}
