// Links the Rust core's static library through its UniFFI bindings (and QCalcCore's Swift conveniences) and checks
// the API the Mac UI calls: the free foundation calls, the Engine per keystroke, commit and history, rates, the
// JSON escape hatch, the 32 MB evaluation thread with a deep input on a small-stack thread, async calls and
// cancellation. Built and run for each architecture by native/scripts/build-mac.sh.

import Foundation

var failures = 0

func check(_ what: String, _ got: String, _ want: String) {
    if got == want {
        print("ok   \(what) → \(got)")
    } else {
        print("FAIL \(what) → \(got), want \(want)")
        failures += 1
    }
}

func check(_ what: String, _ ok: Bool) {
    check(what, ok ? "yes" : "no", "yes")
}

#if arch(arm64)
let arch = "arm64"
#else
let arch = "x86_64"
#endif
print("qcalc-core \(coreVersion()) on \(arch)")

// ---- the free foundation calls ----

var options = defaultEvalOptions()
let lines = evaluate(lines: ["2+3*4", "0.1+0.2", "1/3", "x = 2", "x^10", "3!", "√8", "255 in hex"], options: options)
check("2+3*4", lines[0].display, "14")
check("0.1+0.2", lines[1].display, "0.3")
check("1/3 at 6 figures", lines[2].display, "0.333333")
check("x = 2 is an assignment", String(describing: lines[3].kind), "assignment")
check("x^10", lines[4].display, "1024")
check("3!", lines[5].display, "6")
check("√8 exact", lines[6].exact ?? "", "2sqrt(2)")
check("255 in hex", lines[7].display, "0xFF")

options.sigFigs = 16
options.fractionMode = true
check("1/3 + 1/6 as a fraction", evaluate(lines: ["1/3 + 1/6"], options: options)[0].display, "1/2")

do {
    let json = try evaluateJson(request: #"{"lines":["2^0.5"],"options":{"sigFigs":16}}"#)
    check("evaluate_json", json.contains(#""display":"1.414213562373095""#) ? "has display" : json, "has display")
    _ = try evaluateJson(request: "not json")
    check("bad JSON throws", "returned", "threw")
} catch CoreError.BadRequest {
    check("bad JSON throws", "threw", "threw")
} catch {
    check("bad JSON throws", "\(error)", "threw")
}

// ---- the Engine, per keystroke ----

let dataDir = FileManager.default.temporaryDirectory
    .appendingPathComponent("qcalc-swift-test-\(arch)-\(ProcessInfo.processInfo.processIdentifier)", isDirectory: true)
defer { try? FileManager.default.removeItem(at: dataDir) }
let engine = Engine(config: EngineConfig(isWindows: false, locale: "en_US", dataDir: dataDir.path))
let settings = defaultEvalOptions()
let now = Int64(1_790_287_960_000)

func request(_ text: String, context: EvalContext = EvalContext(), chained: Bool = false) -> LiveRequest {
    LiveRequest(text: text, options: settings, context: context, chained: chained, nowMs: now)
}

let big = engine.live(request: request("1234567*2"))
check("live display", big.display, "2469134")
check("live pretty", big.pretty, "2,469,134")
check("live number", big.number.map { String($0) } ?? "nil", "2469134.0")
check("live json is the LineResult", big.json.contains(#""display":"2469134""#))
let root = engine.live(request: request("√8"))
check("live dual label", root.dualLabel, "2sqrt(2) ≈ 2.82843")
check("blank live", engine.live(request: request("2+")).display, "")

// commit, history context and a chain
var history: [HistoryRow] = []
let first = engine.commit(request: request("6*7"), history: history)
check("commit saves", first.saved)
history = first.history
check("saved row", history.first?.display ?? "", "42")
let view = HistoryView(show: .always, answerForm: .exact, sigFigs: 6, nowMs: now)
var context = engine.contextFromHistory(rows: history, view: view)
check("ans from history", context.ans.map { String($0) } ?? "nil", "42.0")
check("*2 chains", engine.chains(text: "*2", context: context))
check("-2 doesn't chain", !engine.chains(text: "-2", context: context))
let chained = request("*2", context: context, chained: true)
check("chained live", engine.live(request: chained).display, "84")
let second = engine.commit(request: chained, history: history)
check("the tape writes the answer out", second.row?.expr ?? "", "42*2")
history = second.history
history = engine.commit(request: request("r = 5"), history: history).history
history = engine.commit(request: request("area(r) = pi r^2"), history: history).history
context = engine.contextFromHistory(rows: history, view: view)
check("a variable carries over", context.variables["r"].map { String($0) } ?? "nil", "5.0")
check("a function carries over", engine.live(request: request("area(r)", context: context)).display, "78.5398")
check("a blank commit saves nothing", !engine.commit(request: request("2+"), history: history).saved)

// history file round trip
do {
    let saved = try engine.saveHistory(rows: history)
    check("saveHistory keeps every row", saved.count == history.count)
    check("loadHistory reads them back", engine.loadHistory() == saved)
    let json = engine.historyToJson(rows: saved)
    check("history JSON round trip", engine.historyFromJson(rowsJson: json) == saved)
    let legacy = engine.normalizeHistory(rowsJson: #"[{"latex":"1+1","display":"2"}]"#)
    check("legacy rows migrate", legacy.contains(#""expr":"1+1""#))
} catch {
    check("history file", "\(error)", "saved")
}

// rates: the cache ECBRates.swift writes
do {
    try engine.setRates(ratesJson: #"{"checked":1790287960,"fetched":1790287960,"date":"2026-09-24","rates":{"USD":1.1367,"JPY":180.57}}"#)
    let status = engine.ratesStatus(nowMs: now)
    check("rates read", "\(status.currencies) \(status.fresh) \(status.date ?? "")", "2 true 2026-09-24")
    check("rates go stale after a week", !engine.ratesStatus(nowMs: now + 8 * 86_400_000).fresh)
    try engine.setRates(ratesJson: "nope")
    check("bad rates throw", "returned", "threw")
} catch CoreError.BadRequest {
    check("bad rates throw", "threw", "threw")
} catch {
    check("rates", "\(error)", "ok")
}
check("no rates file yet", (try? engine.reloadRates()) == false)

// the JSON escape hatch
check("callJson", (try? engine.callJson(module: "engine/format", function: "formatNumber", argsJson: "[0.5, 6]")) ?? "", #""0.5""#)
do {
    _ = try engine.callJson(module: "lib/nowhere", function: "nothing", argsJson: "[]")
    check("unported callJson throws", "returned", "threw")
} catch CoreError.Unported {
    check("unported callJson throws", "threw", "threw")
} catch {
    check("unported callJson throws", "\(error)", "threw")
}

// ---- the 32 MB evaluation thread: a deep input from a 512 KB thread ----

// 250 levels is the most the parser reads; evaluating it needs well over 512 KB of stack (it overflows a
// 512 KB thread when called directly), so this only passes because the engine hops to its own 32 MB thread
let deep = String(repeating: "(", count: 250) + "1" + String(repeating: ")", count: 250)
let tooDeep = String(repeating: "(", count: 600) + "1" + String(repeating: ")", count: 600)
let (deepShown, tooDeepShown) = await withCheckedContinuation { (done: CheckedContinuation<(String, String), Never>) in
    let small = Thread {
        let ok = engine.live(request: request(deep)).display
        let blank = engine.live(request: request(tooDeep)).display
        done.resume(returning: (ok, blank))
    }
    small.stackSize = 512 * 1024
    small.start()
}
check("250-deep input from a 512 KB thread", deepShown, "1")
check("600-deep input is blank, not a crash", tooDeepShown, "")
// and from a dispatch queue, as the UI calls it
var queued = ""
DispatchQueue.global().sync { queued = engine.evaluate(lines: [deep], options: settings).first?.display ?? "" }
check("deep input from a dispatch thread", queued, "1")

// ---- async and cancellation ----

do {
    let sheet = try await engine.evaluateAsync(lines: ["x = 3", "x^2"], options: settings, cancel: CancelToken())
    check("evaluateAsync", sheet.last?.display ?? "", "9")
    let liveAsync = try await engine.liveAsync(request: request("2^10"))
    check("liveAsync (Task convenience)", liveAsync.display, "1024")
} catch {
    check("async calls", "\(error)", "answered")
}

func isCancelled(_ body: () async throws -> Void) async -> Bool {
    do {
        try await body()
        return false
    } catch CoreError.Cancelled {
        return true
    } catch {
        print("     threw \(error)")
        return false
    }
}

let token = CancelToken()
token.cancel()
check("a cancelled token stops the call before it starts", await isCancelled {
    _ = try await engine.evaluateAsync(lines: ["1+1"], options: settings, cancel: token)
})

// a long sheet, timed whole at a size that's quick, then cancelled mid-run at forty times the size
let line = "sqrt(2)*(3+4)/7-1.5^2+sin(30)*cos(60)"
let unit = Array(repeating: line, count: 500)
let started = Date()
_ = try? await engine.evaluateAsync(lines: unit, options: settings, cancel: CancelToken())
let unitTime = Date().timeIntervalSince(started)
print(String(format: "     500 lines in %.1f ms", unitTime * 1000))
let long = Array(repeating: line, count: 20_000)
let midToken = CancelToken()
let cancelStart = Date()
let job = Task { await isCancelled { _ = try await engine.evaluateAsync(lines: long, options: settings, cancel: midToken) } }
try? await Task.sleep(nanoseconds: UInt64(max(unitTime / 2, 0.002) * 1e9))
midToken.cancel()
let stopped = await job.value
let cancelTime = Date().timeIntervalSince(cancelStart)
print(String(format: "     cancelled after %.1f ms (the whole sheet takes about %.0f ms)", cancelTime * 1000, unitTime * 40_000))
check("cancel() stops a running job", stopped)
check("…well before it would have finished", cancelTime < unitTime * 10 + 0.05)

// cancelling the Swift Task cancels the Rust work through the QCalcCore convenience
let task = Task { await isCancelled { _ = try await engine.evaluateAsync(lines: long, options: settings) } }
try? await Task.sleep(nanoseconds: 2_000_000)
task.cancel()
check("Task.cancel() stops the engine call", await task.value)

// the engine still answers after cancelled jobs
check("after cancelling", engine.live(request: request("2+2")).display, "4")

// ---- the games: ember & frost, trails and the 420/67/69 gates ----

let eggs = EggGate()
check("420 fires once", eggs.settle(n: 420) == .smoke && eggs.settle(n: 420) == nil && eggs.settle(n: 69) == .fold)
check("smoke has its puffs", smokePuffs().count == 22)
let ember = EmberMatch.local(now: 0, progress: nil)
check("ember has its levels", ember.levels().count >= 7)
ember.play()
var frameSeen: EmberFrame?
for i in 1...30 { frameSeen = ember.tick(now: Double(i) * 1000 / 60) }
check("ember plays", frameSeen?.screen ?? "", "play")
check("ember draws a state", frameSeen?.stateJson?.contains("\"players\"") ?? false)
check("ember's stage crosses as json", ember.stageJson(level: 0, stage: 0)?.contains("\"tiles\"") ?? false)
check("the camera", emberCameraFor(stageW: 640, stageH: 360, ax: 40, ay: 300, bx: 560, by: 300, aspect: 16.0 / 9).map { String($0) }.joined(separator: " "), "0.0 0.0 640.0 360.0 1.0")
let trails = TrailsMatch.local(vs: 3, now: 0, seed: 7)
check("trails counts in", trails.tick(now: 0)?.banner ?? "", "3")

// ---- per keystroke cost ----

let start = Date()
var typed = ""
for ch in "sqrt(2)*(3+4)/7-1.5^2" {
    typed.append(ch)
    _ = engine.live(request: request(typed))
}
print(String(format: "21 keystrokes through Engine.live in %.2f ms", Date().timeIntervalSince(start) * 1000))

// ---- WP-GAMES: the game shell and the six native games ----

let ctx = LobbyContext(online: true, codeOnly: false, codeLength: 6, solo: 3, together: 0, peerWord: "computer")
let start1 = lobbyKey(state: lobbyHome(screen: .start, note: nil), key: "1", ctx: ctx, lockedPref: false, name: "teal otter")
check("lobby: 1 is vs computer", String(describing: start1.state), String(describing: lobbyHome(screen: .solo, note: nil)))
check("esc pauses a local game", String(describing: gameEscAction(mode: .local, fullscreen: false, paused: false, armedAt: nil, idle: false, now: 0)), "pause")
check("clean nickname", cleanNickname(raw: "  teal   otter "), "teal otter")
let gclock = GameClock()
gclock.pause(mono: 1000)
check("the game clock stands still while paused", gclock.now(mono: 6000) == 1000)
let relay = RelayPeer(base: "wss://relay.test/")
relay.host(game: "pong", name: "teal otter", locked: true, now: 0)
if case let .open(_, url)? = relay.takeCommands().first {
    check("relay host url", url, "wss://relay.test/host/pong?name=teal+otter&locked=1&resume=1")
} else {
    check("relay host opens a socket", false)
}
let c4 = Connect4Game(role: .host, now: 0, solo: .easy)
check("connect 4 solo drop", c4.play(col: 3, now: 10))
check("connect 4 computer answers", c4.tick(now: 600).moves == 2)
let ut = UltimateGame(role: .host, now: 0, solo: .normal)
check("ultimate solo move", ut.play(cell: 40, now: 0))
check("chalk line as the page drew it", utChalkLine(ax: 0, ay: 0, bx: 100, by: 0, seed: 1, amp: 1.1), "M-0.7 0.5C33.3 -0.2 66.7 1.1 101.1 0.2")
let pongGame = PongGame(role: .host, now: 0, solo: .hard)
check("pong solo starts", pongGame.tick(now: 16).started)
let dial = crackStart(level: 3, now: 0, angle: 0, dir: 1)
check("crack waits for a press", crackPress(state: dial, now: 0).event == .start)
check("equal's daily", equalDailyAnswer(date: "2026-10-09"), "9/3+8=11")
check("equal guess problem", equalGuessProblem(guess: "12+35=48") ?? "", "that doesn’t add up")
check("24 checks", numbersSolves24(text: "(7 - 8/8) * 4", nums: [4, 7, 8, 8]))
check("countdown points", String(numbersPoints(target: 500, value: 495)), "7")

if failures > 0 {
    print("\(failures) failed")
    exit(1)
}
print("all passed")
