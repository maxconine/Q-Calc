import Foundation
import QCalcCore

// The app's one handle on the QCalcCore engine (native/QCalcCore/API.md). The engine is thread-safe and hops each
// call to its own 32 MB evaluation thread, but sync calls block their caller, so the bar only ever calls it
// through `EngineThread` (lanes, generations, newest-wins) and never from the main thread.

extension EvalOptions {
    /// The settings as the engine takes them, from the same UserDefaults keys the web app used.
    static func from(_ s: AppSettings) -> EvalOptions {
        var o = defaultEvalOptions()
        o.angleRadians = s.angleMode == "rad"
        o.fractionMode = s.fractionMode
        o.sigFigs = UInt32(AppSettings.clampSigFigs(s.significantFigures))
        o.sigFigMode = s.sigFigMode
        o.rationalize = s.rationalize
        o.spaceThousands = s.spaceThousands
        o.defaultUnits = s.defaultUnits
        return o
    }
}

extension HistoryView {
    static func from(_ s: AppSettings, now: Date = Date()) -> HistoryView {
        var view = HistoryView.standard(sigFigs: UInt32(AppSettings.clampSigFigs(s.significantFigures)), now: now)
        view.show = s.historyShow == "always" ? .always : s.historyShow == "arrow" ? .arrow : .recent
        view.answerForm = s.answerForm == "approx" ? .approx : .exact
        return view
    }
}

extension LiveAnswer {
    var isBlank: Bool { display.trimmingCharacters(in: .whitespaces).isEmpty }

    static let blank = LiveAnswer(display: "", pretty: "", exact: nil, dualLabel: "", number: nil, unit: nil,
                                  kind: .empty, blank: nil, completion: nil, solve: nil, closedFormJob: nil,
                                  timeLetters: nil, json: "{}")
}

extension HistoryRow: Identifiable {
    var hasDual: Bool {
        guard let exact = exact?.trimmingCharacters(in: .whitespaces), !exact.isEmpty else { return false }
        return exact != display.trimmingCharacters(in: .whitespaces)
    }
}


enum EngineFactory {
    static func make() -> Engine {
        let dir = AppInfo.supportDirectory
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return Engine(config: EngineConfig(isWindows: false, locale: Locale.current.identifier, dataDir: dir.path))
    }
}
