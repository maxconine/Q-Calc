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
    enum Lane: Hashable { case live, forms, graph, commit, misc, selection }

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
