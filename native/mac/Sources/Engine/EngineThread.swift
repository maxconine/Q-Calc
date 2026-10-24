import Foundation

// One thread owns the engine. It has a 32 MB stack because deep inputs recurse (ARCHITECTURE.md §5: a dispatch
// thread's 512 KB overflows on the golden fixtures' nesting tests), and every engine call runs on it, never on the
// main thread.
//
// Jobs come in lanes. A lane holds at most one waiting job: a newer one replaces it, so a burst of keystrokes
// evaluates only the latest text, and the stale ones never run. Each job carries a generation; a result whose
// generation is no longer the lane's newest is dropped before it reaches the main thread, so a slow answer can't
// overwrite a newer one. When QCalcCore brings a CancelToken, `cancel` also flips the running job's token.
final class EngineThread {
    enum Lane: Hashable { case live, forms, graph, misc }

    private struct Job {
        let generation: Int
        let work: (CalcEngine) -> Void
    }

    private let engine: CalcEngine
    private let lock = NSCondition()
    private var waiting: [Lane: Job] = [:]
    private var order: [Lane] = []
    private var newest: [Lane: Int] = [:]
    private var thread: Thread?

    init(engine: CalcEngine) {
        self.engine = engine
        let thread = Thread { [weak self] in self?.run() }
        thread.stackSize = 32 << 20
        thread.name = "qcalc.engine"
        thread.qualityOfService = .userInteractive
        self.thread = thread
        thread.start()
    }

    /// Runs `work` on the engine thread and hands its result to `done` on the main thread, unless a newer job was
    /// submitted to the same lane meanwhile.
    func submit<T>(_ lane: Lane, _ work: @escaping (CalcEngine) -> T, done: @escaping (T) -> Void) {
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
    func sync<T>(_ work: @escaping (CalcEngine) -> T) -> T {
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
