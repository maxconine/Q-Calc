import AppKit
import QCalcCore
import Carbon
import Combine
import SwiftUI

/// A message card under the bar (messageBanner.ts): someone's words with a reply line, or someone new asking.
struct BannerCard: Identifiable, Equatable {
    enum Reply: Equatable { case idle, sending, sent, failed(String) }
    let id: String
    let from: String
    let text: String
    let ask: Bool
    var draft = ""
    var reply = Reply.idle
    var shownAt = Date()
}

/// What the input draws over its text.
struct Decorations: Equatable {
    var leadingParens = 0
    var trailingParens = 0
    var units: [NSRange] = []
    var words: [NSRange] = []
    var squiggle: NSRange?
    /// a unit offered where a number was owed one (missingUnit): drawn muted after the number
    var offer: (at: Int, text: String)?

    static func == (a: Decorations, b: Decorations) -> Bool {
        a.leadingParens == b.leadingParens && a.trailingParens == b.trailingParens && a.units == b.units
            && a.words == b.words && a.squiggle == b.squiggle && a.offer?.at == b.offer?.at && a.offer?.text == b.offer?.text
    }
}

/// The graph panel under the bar: a range, and the curves the engine sampled over it.
struct GraphState: Equatable {
    var expression: String
    var xMin: Double = -10
    var xMax: Double = 10
    var yMin: Double = -10
    var yMax: Double = 10
    var curves: [GraphCurve] = []
    var failed = false
    /// not yet sampled: the engine's own y range (graphHome) may set the view
    var fresh = true
    /// from the engine's GraphResult (WP-GRAPH): roots and critical points, marked on the curve and listed below
    var roots: [Double] = []
    var critical: [CriticalPoint] = []
    var homeY: (Double, Double)?

    static func == (a: GraphState, b: GraphState) -> Bool {
        a.expression == b.expression && a.xMin == b.xMin && a.xMax == b.xMax && a.yMin == b.yMin && a.yMax == b.yMax
            && a.curves == b.curves && a.failed == b.failed && a.roots == b.roots && a.critical == b.critical
    }
}

/// Everything the bar shows and every key it takes. The SwiftUI views only draw this; AppKit (OverlayController)
/// owns the window. The engine is only ever called through `engine` (its own thread); nothing here blocks the
/// main thread on maths.
final class BarModel: ObservableObject {
    // the line
    @Published var text = "" { didSet { if text != oldValue { textChanged(from: oldValue) } } }
    /// what a command line says in the answer slot (`periodic table`, `↵ play pong`), as .live.message does
    @Published private(set) var commandMessage: String?
    @Published private(set) var live = LiveAnswer.blank
    /// a closed form the engine's PSLQ search found after the answer (π²/6), shown in place of `live.exact`
    @Published private(set) var upgradedExact: String?
    /// the rest of the word being typed, drawn after the caret; Tab or → takes it
    @Published private(set) var completion: String?
    @Published private(set) var decorations = Decorations()
    /// d/dt letters the user moved by hand (TimeLetters.tsx); sent as EvalOptions.timeVarying
    @Published var timeVarying: [String: Bool] = [:]
    /// the last real answer, dimmed, while the line is briefly unreadable (useSteadyAnswer)
    @Published private(set) var steady: String?
    @Published private(set) var chained = false
    @Published private(set) var forms: [String] = []
    @Published private(set) var formIndex = 0
    @Published private(set) var copied = false
    @Published var hint: String?
    // the tape
    @Published private(set) var rows: [HistoryRow] = []
    @Published private(set) var selected: Int?
    /// which half of the selected row ↵ inserts: false the expression, true the answer
    /// the part of the selected row ← → picked: 0 its expression, 1… its answers (historyPicks)
    @Published private(set) var pick = 1
    @Published private(set) var tapeOpen = false
    @Published private(set) var recentFrom = 0
    // sheets above the bar
    @Published private(set) var gamesShown = false
    @Published private(set) var gamePick: Int?
    @Published private(set) var helpShown = false
    // panels below
    @Published private(set) var graph: GraphState?
    @Published var cards: [BannerCard] = []
    @Published var cardsHovered = false
    /// keyboard focus is on an edge tool (Full Keyboard Access), which shows them as hover does
    @Published var edgeFocus = false
    @Published var replyFocused: String?
    // settings mirrored for the views
    @Published private(set) var angleRad = false
    @Published private(set) var fractions = false
    @Published private(set) var sigFigMode = false

    let settings = AppSettings.shared
    let history: HistoryStore
    let engine: EngineThread
    /// the engine itself, only for its async calls (closed forms, graphs), which never block
    let core: Engine
    let sheets = SheetData.shared
    let math: MathRenderer = PlainMathRenderer()

    // what the bar asks of its window
    var onHide: ((String) -> Void)?
    var onSettings: (() -> Void)?
    var onPeriodic: (() -> Void)?
    var onIdentities: ((String, Int) -> Void)?
    var onGame: ((String) -> Void)?
    var onEffect: ((String) -> Void)?
    var onTutorial: (() -> Void)?
    /// the input field, so the window can give it the keys
    var focusField: (() -> Void)?
    var fieldSelection: (() -> String)?
    var caretAtEnd: (() -> Bool)?
    /// replaces the whole line as typing would (symbols, undo), for scripted input
    var setField: ((String) -> Void)?

    private var deleted: (row: HistoryRow, index: Int)?
    private var lastLiveText = ""
    private var steadyTimer: Timer?
    private var recentTimer: Timer?
    private var copiedTimer: Timer?
    private var cardTimer: Timer?
    private var effectArmed = true
    private var bag: Set<AnyCancellable> = []
    private static let draftKey = "qcalc.draft"

    init(engine: EngineThread, core: Engine, history: HistoryStore) {
        self.engine = engine
        self.core = core
        self.history = history
        rows = history.rows
        syncSettings()
        settings.objectWillChange
            .debounce(for: .milliseconds(1), scheduler: RunLoop.main)
            .sink { [weak self] in self?.settingsChanged() }
            .store(in: &bag)
        NotificationCenter.default.publisher(for: .qcalcInboxChanged)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in self?.pullInbox() }
            .store(in: &bag)
        NotificationCenter.default.publisher(for: .qcalcMessagingChanged)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in self?.dropAnsweredCards() }
            .store(in: &bag)
        refreshRecent()
    }

    // MARK: - settings

    private func syncSettings() {
        angleRad = settings.angleMode == "rad"
        fractions = settings.fractionMode
        sigFigMode = settings.sigFigMode
    }

    private func settingsChanged() {
        syncSettings()
        if settings.shareUsage != lastShareUsage { lastShareUsage = settings.shareUsage; Analytics.shared.setSharing(settings.shareUsage) }
        refreshRecent()
        evaluateLive()
    }

    private var options: EvalOptions { EvalOptions.from(settings) }

    /// What one keystroke asks the engine, built on the engine thread: the context comes from the rows the
    /// history setting shows (contextFromHistory), and the engine says whether the line chains from `ans`.
    private static func request(_ engine: Engine, text: String, rows: [HistoryRow], view: HistoryView, options: EvalOptions) -> LiveRequest {
        let context = engine.contextFromHistory(rows: rows, view: view)
        return LiveRequest(text: text, options: options, context: context,
                           chained: engine.chains(text: text, context: context), nowMs: view.nowMs)
    }

    // MARK: - the live answer

    private var closedFormTask: Task<Void, Never>?

    private func textChanged(from old: String) {
        // typing past `?` or `help` keeps just the new text (afterHelpInput)
        let was = old.trimmingCharacters(in: .whitespaces).lowercased()
        if was == "?" || was == "help", text.count > old.count, text.hasPrefix(old) {
            text = String(text.dropFirst(old.count))
            return
        }
        commandMessage = nil
        unitOffer = nil
        formIndex = 0
        prefixShown = nil
        prefixValue = nil
        forms = []
        copied = false
        completion = nil
        upgradedExact = nil
        closedFormTask?.cancel()
        if selected != nil, !text.isEmpty { selected = nil }
        gamesShown = BarCommand.of(text, sheets: sheets) == .games
        if !gamesShown { gamePick = nil }
        helpShown = false
        if text.isEmpty { hint = nil }
        evaluateLive()
    }

    func evaluateLive() {
        let typed = text
        let command = BarCommand.of(typed, sheets: sheets)
        if let command {
            commandMessage = command.hint(messaging: Messages.shared)
            if command == .help, !helpShown { track("help") }
            helpShown = command == .help
            if case let .graph(body) = command { showGraph(body) } else { graph = nil }
            setLive(.blank, for: typed)
            engine.cancel(.live)
            return
        }
        graph = nil
        commandMessage = nil
        unitOffer = nil
        if typed.trimmingCharacters(in: .whitespaces).isEmpty {
            engine.cancel(.live)
            chained = false
            setLive(.blank, for: typed)
            return
        }
        let rows = self.rows, view = HistoryView.from(settings), options = self.options
        let thread = engine
        let keepWords = settings.keepWords
        let timeVarying = self.timeVarying
        engine.submit(.live, { engine -> (LiveAnswer, Bool, Decorations, String?) in
            var request = Self.request(engine, text: typed, rows: rows, view: view, options: options)
            request.options.timeVarying = timeVarying
            let answer = engine.live(request: request)
            let deco = Self.decorations(thread.logic, engine: engine, request: request, answer: answer, keepWords: keepWords)
            // completionFor: the engine's when it answers, else the TS through the app-logic bundle
            var completion = answer.completion
            if completion?.isEmpty ?? true {
                let names: [String: Any] = ["variables": Array(request.context.variables.keys), "functions": Array(request.context.functions.keys),
                                            "ans": request.context.ans != nil]
                completion = thread.logic.string("lib/completion", "completionFor", [typed, names])
            }
            return (answer, request.chained, deco, completion)
        }) { [weak self] answer, chain, deco, completion in
            guard let self, self.text == typed else { return }
            self.chained = chain
            if !answer.isBlank { self.hint = nil }
            self.completion = completion.flatMap { $0.isEmpty ? nil : $0 }
            self.decorations = deco
            self.setLive(answer, for: typed)
            self.squiggleLater(typed)
            self.upgradeClosedForm(answer, for: typed)
        }
    }

    /// What the input draws over the text (QuickInput.tsx): grey inferred parens (inferParens), unit and known-word
    /// colours (unitSpans, knownWordSpans) and, for a blank answer, where it stops reading (blankReason).
    private static func decorations(_ logic: AppLogic, engine: Engine, request: LiveRequest, answer: LiveAnswer, keepWords: Bool) -> Decorations {
        let text = request.text
        var d = Decorations()
        if let fill = logic.call("engine/parens", "inferParens", [text]) as? [String: Any] {
            d.leadingParens = (fill["leading"] as? NSNumber)?.intValue ?? 0
            d.trailingParens = (fill["trailing"] as? NSNumber)?.intValue ?? 0
        }
        let variables = Array(request.context.variables.keys), functions = Array(request.context.functions.keys)
        d.units = logic.spans("lib/unitSpans", "unitSpans", [text, variables, functions])
        d.words = logic.spans("lib/knownWords", "knownWordSpans", [text, functions, request.context.ans != nil])
        return d
    }

    /// blankReason with `ok` answered by the engine (chained as the line is); runs on the engine thread
    private static func blankSpan(_ logic: AppLogic, engine: Engine, request: LiveRequest) -> NSRange? {
        logic.ok = { piece in
            var r = request
            r.text = request.chained ? Chain.expr(piece) : piece
            r.chained = false
            return !engine.live(request: r).isBlank
        }
        defer { logic.ok = { _ in false } }
        let names: [String: Any] = ["variables": Array(request.context.variables.keys), "functions": Array(request.context.functions.keys)]
        return logic.spans("lib/blankReason", "blankReason", [request.text, names]).first
    }

    private var squiggleTimer: Timer?
    private lazy var lastShareUsage = AppSettings.shared.shareUsage
    /// the line a unit was offered for, and what ↵ again makes it
    private var unitOffer: (typed: String, filled: String)?

    // the squiggle shows once typing has paused 700 ms on a blank line (SQUIGGLE_IDLE_MS), never for commands
    private func squiggleLater(_ typed: String) {
        squiggleTimer?.invalidate()
        guard live.isBlank, !typed.trimmingCharacters(in: .whitespaces).isEmpty, BarCommand.of(typed, sheets: sheets) == nil else { return }
        squiggleTimer = Timer.scheduledTimer(withTimeInterval: 0.7, repeats: false) { [weak self] _ in
            guard let self, self.text == typed, self.live.isBlank else { return }
            let rows = self.rows, view = HistoryView.from(self.settings), options = self.options, thread = self.engine
            self.engine.submit(.forms, { engine -> NSRange? in
                let request = Self.request(engine, text: typed, rows: rows, view: view, options: options)
                if thread.logic.bool("lib/nativeEval", "looksLikeNaturalLanguage", [typed]) { return nil }
                return Self.blankSpan(thread.logic, engine: engine, request: request)
            }) { [weak self] span in
                guard let self, self.text == typed, self.live.isBlank else { return }
                self.decorations.squiggle = span
            }
        }
    }

    // the closed-form search runs on the engine's long-work pool; typing again cancels it (its CancelToken)
    private func upgradeClosedForm(_ answer: LiveAnswer, for typed: String) {
        closedFormTask?.cancel()
        guard let job = answer.closedFormJob else { return }
        let engine = core
        closedFormTask = Task.detached(priority: .utility) { [weak self] in
            guard let exact = try? await engine.closedForm(jobJson: job), !exact.isEmpty else { return }
            await MainActor.run {
                guard let self, self.text == typed else { return }
                self.upgradedExact = exact
            }
        }
    }

    /// Accept the completion ghost (Tab, or → at the end of the line).
    func acceptCompletion() -> Bool {
        guard let completion, !completion.isEmpty else { return false }
        insert(completion)
        return true
    }

    private func setLive(_ answer: LiveAnswer, for typed: String) {
        if answer.isBlank, !live.isBlank, !typed.isEmpty, isEdit(of: lastLiveText, typed) {
            // keep the last real answer up, dimmed, while the edit is half-typed
            steady = shownAnswer
            steadyTimer?.invalidate()
            steadyTimer = Timer.scheduledTimer(withTimeInterval: 1.6, repeats: false) { [weak self] _ in self?.steady = nil }
        } else if !answer.isBlank || typed.isEmpty {
            steady = nil
        }
        live = answer
        if !answer.isBlank { lastLiveText = typed }
        checkEffects(answer)
    }

    // isEditOf: one line typed on from, or backed off to, the other
    private func isEdit(of before: String, _ now: String) -> Bool {
        guard !before.isEmpty else { return false }
        return now.hasPrefix(before) || before.hasPrefix(now) || abs(now.count - before.count) <= 2
    }

    /// The answer the slot shows and ⌘C copies: the cycled form, else exact or approximate per the setting.
    var shownAnswer: String {
        if let prefixShown { return prefixShown }
        if formIndex > 0, forms.indices.contains(formIndex) { return forms[formIndex] }
        if settings.answerForm == "exact", let exact, !exact.isEmpty { return exact }
        return live.display
    }

    /// the closed form beside the decimal, upgraded by the PSLQ search when it found one
    var exact: String? { upgradedExact ?? live.exact }

    var hasDual: Bool {
        guard formIndex == 0, let exact = exact?.trimmingCharacters(in: .whitespaces), !exact.isEmpty else { return false }
        return exact != live.display.trimmingCharacters(in: .whitespaces)
    }

    /// The engine's own pretty text for the live display; Pretty.answer (a port of prettyAnswer) for anything else.
    func pretty(_ s: String) -> String { s == live.display && !live.pretty.isEmpty ? live.pretty : Pretty.answer(s) }

    // the 420 / 67 / 69 effects fire once as the answer arrives, and again only after it changes away
    private func checkEffects(_ answer: LiveAnswer) {
        guard let n = answer.number, answer.unit == nil else {
            effectArmed = true
            return
        }
        let kind: String? = n == 420 ? "smoke" : n == 67 ? "arms" : n == 69 ? "fold" : nil
        guard let kind else {
            effectArmed = true
            return
        }
        guard effectArmed else { return }
        effectArmed = false
        onEffect?(kind)
    }

    // MARK: - answer forms (Tab)

    /// The engine's answerForms (bridged to lib/answerForms until WP-APP-LOGIC); while that answers nothing, the
    /// same cycle built here: the decimal, the exact form, a fraction and scientific.
    func cycleForms(back: Bool = false) {
        guard !live.isBlank, let n = live.number else { return }
        track("form.tab")
        if !forms.isEmpty {
            formIndex = (formIndex + (back ? forms.count - 1 : 1)) % forms.count
            return
        }
        let typed = text
        let rows = self.rows, view = HistoryView.from(settings), options = self.options
        let display = live.display, exact = self.exact, unit = live.unit, valueJson = live.valueJson, meas = live.meas
        let figs = settings.significantFigures
        engine.submit(.forms, { engine -> [String] in
            var out = [display]
            let source = FormSource(valueJson: valueJson ?? "", display: display, exact: exact, meas: meas,
                                    sigFigs: options.sigFigs, sigFigMode: options.sigFigMode, rationalize: options.rationalize)
            let views = valueJson == nil ? [] : engine.answerForms(source: source)
            if !views.isEmpty {
                out += views.map(\.display)
            } else {
                if let exact, !exact.isEmpty { out.append(exact) }
                var asFraction = options
                asFraction.fractionMode = true
                let f = engine.live(request: Self.request(engine, text: typed, rows: rows, view: view, options: asFraction)).display
                if f.contains("/"), f.filter(\.isNumber).count <= 8 { out.append(f) }
                let mag = n == 0 ? 0 : Int(floor(log10(abs(n))))
                if abs(mag) >= 3 { out.append(Pretty.scientific(n, sigFigs: figs) + (unit.map { " \($0)" } ?? "")) }
            }
            var seen = Set<String>()
            return out.filter { seen.insert($0.trimmingCharacters(in: .whitespaces)).inserted && !$0.contains("undefined") }
        }) { [weak self] list in
            guard let self, self.text == typed else { return }
            self.forms = list
            self.formIndex = list.count > 1 ? 1 : 0
        }
    }

    // MARK: - commit (↵)

    func enter(option: Bool = false) {
        if let i = selected, rows.indices.contains(i) {
            enterOnRow(i, other: option)
            return
        }
        if gamesShown {
            guard let g = gamePick else { commit(); return }
            let game = BarCommand.gameList[g]
            play(game.kind)
            return
        }
        if let command = BarCommand.of(text, sheets: sheets) {
            run(command)
            return
        }
        commit()
    }

    private func run(_ command: BarCommand) {
        switch command {
        case .periodic:
            clearLine()
            onPeriodic?()
            track("periodic")
        case let .identities(id, section, _):
            clearLine()
            onIdentities?(id, section)
        case .games:
            break
        case let .game(kind, _):
            play(kind)
        case let .message(_, name, said):
            let m = Messages.shared
            if m.name.isEmpty || m.link == .paused {
                onSettings?()
                return
            }
            guard !name.isEmpty, !said.isEmpty else { return }
            hint = "sending to \(name)…"
            m.send(to: name, text: said) { [weak self] status in
                guard let self else { return }
                self.hint = Messages.sendStatusText(status, to: name)
                if status == "delivered" { self.clearLine(keepHint: true) }
            }
        case .help:
            helpShown = false
            text = ""
        case .settings:
            clearLine()
            onSettings?()
        case .tutorial:
            clearLine()
            onTutorial?()
        case .graph:
            break
        case .greeting:
            hint = "hi! type some maths, like 2^10 or sqrt(2), and press ↵"
        }
    }

    private func play(_ kind: String) {
        clearLine()
        onGame?(kind)
    }

    private func clearLine(keepHint: Bool = false) {
        let h = hint
        text = ""
        selected = nil
        tapeOpen = false
        if keepHint { hint = h }
    }

    /// Saves the line to the tape. A blank line says why rather than saving nothing.
    /// Enter through `Engine.commit`: the row it saves (chains written out, definitions kept) and the new tape, or
    /// the hint saying why nothing was saved. `leaving` is the bar hiding with "keep unfinished input" off.
    func commit(leaving: Bool = false) {
        let typed = text
        guard !typed.trimmingCharacters(in: .whitespaces).isEmpty else { return }
        // ↵ again on an offered unit takes it (missingUnit's `filled`)
        if !leaving, let offer = unitOffer, offer.typed == typed {
            unitOffer = nil
            decorations.offer = nil
            hint = nil
            setField?(offer.filled)
            return
        }
        let rows = self.rows, view = HistoryView.from(settings), options = self.options, thread = engine
        let improper = live.improper, history = !rows.isEmpty
        engine.submit(.commit, { engine -> (CommitResult, String?, (at: Int, text: String, filled: String)?, NSRange?) in
            let request = Self.request(engine, text: typed, rows: rows, view: view, options: options)
            let done = engine.commit(request: request, history: rows)
            guard !done.saved else { return (done, nil, nil, nil) }
            // units that don't add up because one number has none: the unit it was owed, offered (offerUnit)
            if improper {
                thread.logic.ok = { piece in
                    var r = request
                    r.text = request.chained ? Chain.expr(piece) : piece
                    r.chained = false
                    let a = engine.live(request: r)
                    return !a.isBlank && !a.improper
                }
                defer { thread.logic.ok = { _ in false } }
                if let fix = thread.logic.call("lib/missingUnit", "missingUnit", [typed]) as? [String: Any],
                   let at = (fix["at"] as? NSNumber)?.intValue, let text = fix["text"] as? String, let filled = fix["filled"] as? String {
                    return (done, fix["hint"] as? String, (at, text, filled), nil)
                }
            }
            // enter that saved nothing says why (enterHint), and the squiggle shows where
            let graph = typed.trimmingCharacters(in: .whitespaces).lowercased().hasPrefix("graph")
            let span = graph ? nil : Self.blankSpan(thread.logic, engine: engine, request: request)
            let facts: [String: Any] = [
                "expr": typed, "span": span.map { ["start": $0.location, "end": NSMaxRange($0)] } ?? NSNull(),
                "improper": improper, "naturalLanguage": thread.logic.bool("lib/nativeEval", "looksLikeNaturalLanguage", [typed]),
                "bareGraph": typed.trimmingCharacters(in: .whitespaces).lowercased() == "graph", "history": history,
            ]
            let hint = done.hint ?? thread.logic.string("lib/blankReason", "enterHint", [facts])
            return (done, hint, nil, span)
        }) { [weak self] result in
            guard let self else { return }
            let (done, failHint, offer, span) = result
            if !done.saved, !leaving, self.text == typed {
                self.hint = failHint ?? "nothing to save yet · finish the line, or esc to close"
                track(offer != nil ? "error.unit" : "error.blank")
                if let offer {
                    self.unitOffer = (typed, offer.filled)
                    self.decorations.offer = (offer.at, offer.text)
                }
                if let span { self.decorations.squiggle = span }
                return
            }
            if done.saved {
                self.history.replace(done.history)
                self.rows = self.history.rows
                self.deleted = nil
                self.refreshRecent()
                AppSettings.shared.mergeOnboarding(["commits": (AppSettings.shared.onboarding["commits"] ?? 0) + 1])
                Analytics.shared.trackCalc(expr: typed, graph: typed.lowercased().hasPrefix("graph"), fn: done.row?.kind == .function,
                                           solve: done.row?.solve != nil, assign: done.row?.kind == .definition,
                                           unit: done.row?.quantity != nil, chained: self.chained)
            }
            guard !leaving, self.text == typed else { return }
            if done.saved {
                self.hint = nil
                self.text = done.input
            } else {
                self.hint = done.hint ?? "nothing to save yet · finish the line, or esc to close"
            }
        }
    }

    // MARK: - the tape

    /// The rows the tape shows: all of them while it's open, the recent ones otherwise (per the setting).
    var visibleRange: Range<Int> {
        if tapeOpen || settings.historyShow == "always" { return 0..<rows.count }
        if settings.historyShow == "arrow" { return rows.count..<rows.count }
        return min(recentFrom, rows.count)..<rows.count
    }

    func refreshRecent() {
        recentFrom = history.recentStart()
        recentTimer?.invalidate()
        if let next = history.nextRecentExpiry() {
            recentTimer = Timer.scheduledTimer(withTimeInterval: max(1, next.timeIntervalSinceNow), repeats: false) { [weak self] _ in
                self?.refreshRecent()
            }
        }
    }

    /// historyPicks (src/lib/answer.ts): what ← → step through on a row after its expression, as each inserts, and
    /// the pick ↑ lands on.
    func picks(_ row: HistoryRow) -> (answers: [String], start: Int) {
        if row.kind == .function || row.kind == .system { return ([], 0) }
        if row.kind == .definition { return ([row.expr], 1) }
        if let solve = row.solve {
            guard solve.outcome == .roots else { return ([], 0) }
            return (solve.roots.map { String(format: "%.\(max(1, settings.significantFigures))g", $0) }, 1)
        }
        if row.hasDual, let exact = row.exact { return ([exact, row.display], settings.answerForm == "exact" ? 1 : 2) }
        return ([settings.answerForm == "exact" ? (row.exact ?? row.display) : row.display], 1)
    }

    /// ↑ ↓ keep to the side ← → last chose (historyPickOn)
    private var sideIsExpr = false

    private func land(on i: Int) {
        selected = i
        pick = sideIsExpr ? 0 : picks(rows[i]).start
    }

    func moveSelection(up: Bool) {
        if gamesShown {
            // in the games list ↑ starts at the game nearest the bar and climbs; ↓ past the last leaves it
            let last = BarCommand.gameList.count - 1
            if up { gamePick = gamePick.map { max(0, $0 - 1) } ?? last } else if let g = gamePick { gamePick = g >= last ? nil : g + 1 }
            return
        }
        if helpShown { helpShown = false }
        guard !rows.isEmpty else { return }
        if up {
            let fresh = !tapeOpen || selected == nil
            if fresh { sideIsExpr = false }
            tapeOpen = true
            land(on: fresh ? rows.count - 1 : max(0, selected! - 1))
        } else {
            guard tapeOpen else { return }
            guard let i = selected, i < rows.count - 1 else {
                selected = nil
                focusField?()
                return
            }
            land(on: i + 1)
        }
    }

    /// ← → on a highlighted row (onSide)
    func pickSide(_ dir: Int) {
        guard let i = selected, rows.indices.contains(i) else { return }
        let next = min(picks(rows[i]).answers.count, max(0, pick + dir))
        sideIsExpr = next == 0
        pick = next
    }

    func select(_ i: Int?) {
        selected = i
    }

    var historySelected: Bool { selected != nil }

    /// ↵ on a highlighted row: whatever ← → picked (insertHistoryPick); ⌥↵ the other half (insertHistoryOther)
    func enterOnRow(_ i: Int, other: Bool) {
        guard rows.indices.contains(i) else { return }
        let row = rows[i]
        if other {
            let expr = row.kind == .definition || row.kind == .function || settings.historyInsert == "answer"
            insertFromRow(i, answer: !expr)
            return
        }
        let answers = picks(row).answers
        if pick > 0, !answers.isEmpty {
            finishRowInsert(answers[min(pick, answers.count) - 1])
        } else {
            insertFromRow(i, answer: false)
        }
    }

    /// A click on a row's half (clicks insert the side they hit).
    func insertFromRow(_ i: Int, answer: Bool) {
        guard rows.indices.contains(i) else { return }
        let row = rows[i]
        let piece = answer && row.kind != .definition ? (settings.answerForm == "exact" ? (row.exact ?? row.display) : row.display) : row.expr
        finishRowInsert(piece)
    }

    private func finishRowInsert(_ piece: String) {
        track("history.insert")
        selected = nil
        tapeOpen = false
        insert(piece)
    }

    /// Text into the line at the caret (history rows, periodic tiles); the field does the caret part.
    var insertIntoField: ((String) -> Void)?

    func insert(_ piece: String) {
        if let insertIntoField { insertIntoField(piece) } else { text += piece }
    }

    func deleteSelected() -> Bool {
        guard let i = selected, let row = history.remove(at: i) else { return false }
        deleted = (row, i)
        rows = history.rows
        selected = rows.isEmpty ? nil : min(i, rows.count - 1)
        refreshRecent()
        return true
    }

    func undoDelete() -> Bool {
        guard let d = deleted else { return false }
        track("history.undo")
        history.insert(d.row, at: d.index)
        rows = history.rows
        deleted = nil
        selected = d.index
        refreshRecent()
        return true
    }

    func clearHistory() {
        history.clear()
        rows = []
        selected = nil
        tapeOpen = false
        hint = "history cleared"
    }

    // MARK: - SI prefix (⌥↑ ⌥↓)

    @Published private(set) var prefixShown: String?
    private var prefixValue: Any?

    /// onPrefixStep: the answer one SI prefix up or down (stepPrefix), shown until the line changes
    func stepPrefix(_ dir: Int) {
        let base: Any? = prefixValue ?? live.valueJson.flatMap { try? JSONSerialization.jsonObject(with: Data($0.utf8)) }
        guard let base, let next = logic.call("engine/units", "stepPrefix", [base, dir]) as? [String: Any],
              next["unitId"] != nil else { return }
        prefixValue = next
        prefixShown = logic.string("engine/format", "formatValue", [next, settings.significantFigures])
        track("prefix.step")
    }

    // MARK: - undo (undo.ts: typing runs merge within UNDO_RUN_MS, symbol swaps don't break them)

    /// the main thread's app logic (undo, onboarding, tour, analytics, typst copy)
    lazy var logic = AppLogic(engine: core)
    private var undoState: Any = [String: Any]()
    private var undoReady = false

    func resetUndo() {
        undoState = logic.call("lib/undo", "undoStart", [text]) ?? [String: Any]()
        undoReady = true
    }

    func recordEdit(_ value: String, start: Int, end: Int, kind inputType: String?) {
        if !undoReady { resetUndo() }
        let kind = logic.call("lib/undo", "editKind", [inputType ?? NSNull()]) ?? NSNull()
        let state: [String: Any] = ["value": value, "start": start, "end": end]
        if let next = logic.call("lib/undo", "recordEdit", [undoState, state, kind, Date().timeIntervalSince1970 * 1000]) {
            undoState = next
        }
    }

    func breakUndoRun() {
        guard undoReady, let next = logic.call("lib/undo", "breakRun", [undoState]) else { return }
        undoState = next
    }

    private func applyUndo(_ fn: String) -> Bool {
        if !undoReady { resetUndo() }
        guard let next = logic.call("lib/undo", fn, [undoState]) as? [String: Any],
              let current = next["current"] as? [String: Any], let value = current["value"] as? String else { return false }
        undoState = next
        let start = (current["start"] as? NSNumber)?.intValue ?? (value as NSString).length
        let end = (current["end"] as? NSNumber)?.intValue ?? start
        restoreField?(value, NSRange(location: start, length: max(0, end - start)))
        return true
    }

    func undoText() -> Bool { applyUndo("undo") }
    func redoText() -> Bool { applyUndo("redo") }

    /// puts a value and selection back without recording it as an edit
    var restoreField: ((String, NSRange) -> Void)?

    // MARK: - copy

    func copyAnswer(_ value: String? = nil) {
        let shown = value ?? shownAnswer
        guard !shown.isEmpty else { return }
        track("copy.answer")
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(UnitlessCopy.copied(shown), forType: .string)
        flashCopied()
    }

    func copyLine() {
        guard !live.isBlank else { return }
        track("copy.line")
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString("\(text) = \(shownAnswer)", forType: .string)
        flashCopied()
    }

    func copyRow(_ row: HistoryRow, answer: Bool) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(answer ? UnitlessCopy.copied(row.display) : row.expr, forType: .string)
        flashCopied()
    }

    private func flashCopied() {
        copied = true
        copiedTimer?.invalidate()
        copiedTimer = Timer.scheduledTimer(withTimeInterval: 1.2, repeats: false) { [weak self] _ in self?.copied = false }
        NSAccessibility.post(element: NSApp as Any, notification: .announcementRequested,
                             userInfo: [.announcement: "Copied", .priority: NSAccessibilityPriorityLevel.high.rawValue])
    }

    // MARK: - toggles

    func toggleAngle() { settings.setAngleMode(angleRad ? "deg" : "rad", notifyWeb: true) }
    func toggleFractions() { settings.setFractionMode(!fractions, notifyWeb: true) }
    func toggleSigFigMode() { settings.setSigFigMode(!sigFigMode, notifyWeb: true) }

    // MARK: - keys

    /// The bar's keyboard map (MAC_UI_PLAN.md §7). true when the key was the bar's; the field handles the rest.
    func key(_ event: NSEvent) -> Bool {
        let flags = event.modifierFlags.intersection([.command, .option, .control, .shift])
        let code = Int(event.keyCode)
        if let action = boundAction(event) {
            perform(action)
            return true
        }
        switch code {
        case kVK_Return, kVK_ANSI_KeypadEnter:
            guard flags.isSubset(of: [.option]) else { return false }
            enter(option: flags.contains(.option))
            return true
        case kVK_UpArrow, kVK_DownArrow:
            // ⌥↑ / ⌥↓ step the answer's SI prefix (stepPrefix)
            if flags == [.option] {
                stepPrefix(code == kVK_UpArrow ? 1 : -1)
                return true
            }
            guard flags.isEmpty else { return false }
            moveSelection(up: code == kVK_UpArrow)
            return true
        case kVK_LeftArrow, kVK_RightArrow:
            guard flags.isEmpty else { return false }
            if selected != nil {
                pickSide(code == kVK_RightArrow ? 1 : -1)
                return true
            }
            // → at the end of the line takes the completion
            if code == kVK_RightArrow, caretAtEnd?() ?? false { return acceptCompletion() }
            return false
        case kVK_Tab:
            guard flags.isSubset(of: [.shift]) else { return false }
            if !flags.contains(.shift), acceptCompletion() { return true }
            cycleForms(back: flags.contains(.shift))
            return true
        case kVK_Delete:
            if flags == [.command], selected != nil { return deleteSelected() }
            return false
        case kVK_ANSI_Z:
            if flags == [.command] { return undoDelete() || undoText() }
            if flags == [.command, .shift] { return redoText() }
            return false
        default:
            return false
        }
    }

    private func boundAction(_ event: NSEvent) -> String? {
        guard let chord = Chord(event: event, typed: true) else { return nil }
        for (id, _) in KeyActions.all where id != "show" {
            guard KeyActions.chord(id, in: settings.keybinds) == chord.text else { continue }
            // ⌘C copies the field's selection when there is one, as any text field does
            if id == "copyAnswer", !(fieldSelection?() ?? "").isEmpty { return nil }
            // ⌃C clears history only while the field has nothing selected, never mid-IME
            return id
        }
        return nil
    }

    func perform(_ action: String) {
        switch action {
        case "settings": onSettings?()
        case "copyAnswer": copyAnswer()
        case "copyLine": copyLine()
        case "angle": toggleAngle()
        case "fraction": toggleFractions()
        case "sigFigs": toggleSigFigMode()
        case "clear": clearHistory()
        default: break
        }
    }

    /// Esc closes the innermost layer; false means nothing was open, so the bar hides.
    func escape() -> Bool {
        if replyFocused != nil {
            replyFocused = nil
            focusField?()
            return true
        }
        if !cards.isEmpty {
            cards = []
            return true
        }
        if helpShown || gamesShown {
            helpShown = false
            gamesShown = false
            return true
        }
        if selected != nil {
            selected = nil
            if settings.historyShow != "always" { tapeOpen = false }
            return true
        }
        if graph != nil, !text.isEmpty {
            text = ""
            return true
        }
        return false
    }

    // MARK: - showing and hiding

    func willShow() {
        selected = nil
        tapeOpen = false
        helpShown = false
        refreshRecent()
        rows = history.rows
        restoreDraft()
        pullInbox()
        AppSettings.shared.mergeOnboarding(["opens": (AppSettings.shared.onboarding["opens"] ?? 0) + 1])
        track("open")
    }

    func willHide() {
        replyFocused = nil
        let draftSeconds = settings.draftSeconds
        if text.trimmingCharacters(in: .whitespaces).isEmpty {
            UserDefaults.standard.removeObject(forKey: Self.draftKey)
        } else if draftSeconds > 0 {
            UserDefaults.standard.set(["expr": text, "savedAt": Date().timeIntervalSince1970 * 1000], forKey: Self.draftKey)
        } else {
            if !live.isBlank { commit(leaving: true) }
            text = ""
        }
    }

    // shouldRestoreDraft: the line comes back if the bar was away less than the setting
    private func restoreDraft() {
        guard let d = UserDefaults.standard.dictionary(forKey: Self.draftKey),
              let expr = d["expr"] as? String, let savedAt = d["savedAt"] as? Double else {
            if settings.draftSeconds <= 0 { text = "" }
            return
        }
        let age = Date().timeIntervalSince1970 * 1000 - savedAt
        if settings.draftSeconds > 0, age < Double(settings.draftSeconds) * 1000 {
            if text != expr { text = expr }
        } else {
            text = ""
            UserDefaults.standard.removeObject(forKey: Self.draftKey)
        }
    }

    // MARK: - graph (placeholder until the GRAPH package)

    func showGraph(_ body: String) {
        var state = graph?.expression == body ? graph! : GraphState(expression: body)
        state.expression = body
        graph = state
        sampleGraph()
    }

    func panGraph(dx: Double, dy: Double) {
        guard var g = graph else { return }
        let w = g.xMax - g.xMin, h = g.yMax - g.yMin
        g.xMin -= dx * w; g.xMax -= dx * w
        g.yMin += dy * h; g.yMax += dy * h
        graph = g
        sampleGraph()
    }

    func zoomGraph(_ factor: Double) {
        guard var g = graph, factor.isFinite, factor > 0 else { return }
        let cx = (g.xMin + g.xMax) / 2, cy = (g.yMin + g.yMax) / 2
        let w = (g.xMax - g.xMin) * factor / 2, h = (g.yMax - g.yMin) * factor / 2
        guard w > 1e-9, w < 1e9 else { return }
        g.xMin = cx - w; g.xMax = cx + w; g.yMin = cy - h; g.yMax = cy + h
        graph = g
        sampleGraph()
    }

    func homeGraph() {
        guard let g = graph else { return }
        graph = GraphState(expression: g.expression)
        sampleGraph()
    }

    private var graphTask: Task<Void, Never>?

    // Engine.graphAsync on the long-work pool, cancelled (CancelToken) by the next pan, zoom or keystroke. It's
    // bridged to engine/graph's buildGraph, which answers nil until WP-GRAPH; then the JSON-hatch sampler fills in
    private func sampleGraph() {
        guard let g = graph else { return }
        let expr = g.expression, lo = g.xMin, hi = g.xMax
        let rows = self.rows, view = HistoryView.from(settings), settings = self.options
        let engine = core
        graphTask?.cancel()
        graphTask = Task.detached(priority: .userInitiated) { [weak self] in
            let options = GraphOptions(domainLo: lo, domainHi: hi, sampleCount: 241)
            var curves: [GraphCurve]?
            var yRange: (Double, Double)?
            var roots: [Double] = [], critical: [CriticalPoint] = []
            if let result = try? await engine.graphAsync(input: "graph \(expr)", options: options, settings: settings),
               result.error == nil, !result.points.isEmpty {
                curves = [GraphCurve(label: result.label, points: result.points)]
                if result.yMax > result.yMin { yRange = (result.yMin, result.yMax) }
                roots = result.roots
                critical = result.criticalPoints
            } else if !Task.isCancelled {
                let context = engine.contextFromHistory(rows: rows, view: view)
                curves = GraphFallback.sample(engine, expression: expr, xMin: lo, xMax: hi, samples: 241, context: context, settings: settings)
                // the home view's y range fits the curve, as graphHome's does, with a tenth spare each side
                let ys = curves?.flatMap { $0.points.compactMap(\.y) } ?? []
                if let a = ys.min(), let b = ys.max() {
                    let span = max(b - a, 1e-9)
                    yRange = (a - span * 0.1, b + span * 0.1)
                }
            }
            if Task.isCancelled { return }
            await MainActor.run {
                guard let self, var now = self.graph, now.expression == expr, now.xMin == lo else { return }
                now.curves = curves ?? []
                now.failed = curves == nil
                now.roots = roots
                now.critical = critical
                if let yRange, now.fresh { (now.yMin, now.yMax) = yRange; now.homeY = yRange }
                now.fresh = false
                self.graph = now
            }
        }
    }

    // MARK: - message cards

    func pullInbox() {
        guard Messages.shared.barOpen,
              let rows = try? JSONSerialization.jsonObject(with: Data(Messages.shared.inboxJSON.utf8)) as? [[String: Any]]
        else { return }
        var ids: [String] = []
        for o in rows {
            guard let id = o["id"] as? String, let from = o["from"] as? String else { continue }
            ids.append(id)
            let ask = o["ask"] as? Bool ?? false
            let text = MessageInbox.cleanText(o["text"] as? String ?? "", max: Messages.textMax)
            let card = BannerCard(id: id, from: from, text: ask ? "" : text, ask: ask)
            if cards.contains(where: { $0.id == id || (ask && $0.ask && $0.from == from) }) { continue }
            cards.insert(card, at: 0)
        }
        if cards.count > 3 { cards = Array(cards.prefix(3)) }
        if !ids.isEmpty { Messages.shared.seen(ids) }
        scheduleCardExpiry()
    }

    private func dropAnsweredCards() {
        let m = Messages.shared
        cards.removeAll { m.blocked.contains($0.from) || ($0.ask && m.accepted.contains($0.from)) }
    }

    /// cardHeld: a card waits while the pointer is on the cards, or while its reply is typed or sent
    private func held(_ card: BannerCard) -> Bool {
        cardsHovered || replyFocused == card.id || !card.draft.isEmpty || card.reply == .sending
    }

    func scheduleCardExpiry() {
        cardTimer?.invalidate()
        guard !cards.isEmpty else { return }
        cardTimer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in self?.expireCards() }
    }

    private func expireCards() {
        let now = Date()
        for i in cards.indices where held(cards[i]) { cards[i].shownAt = now }
        cards.removeAll { now.timeIntervalSince($0.shownAt) >= 10 }
        if cards.isEmpty { cardTimer?.invalidate() }
    }

    func setDraft(_ id: String, _ draft: String) {
        guard let i = cards.firstIndex(where: { $0.id == id }) else { return }
        cards[i].draft = draft
    }

    func sendReply(_ id: String) {
        guard let i = cards.firstIndex(where: { $0.id == id }) else { return }
        let card = cards[i]
        let said = String(card.draft.trimmingCharacters(in: .whitespacesAndNewlines).prefix(Messages.textMax))
        guard !said.isEmpty, card.reply != .sending else { return }
        cards[i].reply = .sending
        Messages.shared.send(to: card.from, text: said) { [weak self] status in
            guard let self, let j = self.cards.firstIndex(where: { $0.id == id }) else { return }
            if status == "delivered" {
                self.cards[j].reply = .sent
                self.cards[j].draft = ""
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { [weak self] in
                    self?.cards.removeAll { $0.id == id }
                    if self?.replyFocused == id {
                        self?.replyFocused = nil
                        self?.focusField?()
                    }
                }
            } else {
                self.cards[j].reply = .failed(Messages.sendStatusText(status, to: card.from))
            }
        }
    }

    func answerAsk(_ id: String, allow: Bool) {
        guard let card = cards.first(where: { $0.id == id }) else { return }
        if allow { Messages.shared.allow(card.from) } else { Messages.shared.block(card.from) }
        cards.removeAll { $0.id == id }
    }

    func dismissCard(_ id: String) {
        cards.removeAll { $0.id == id }
        if replyFocused == id {
            replyFocused = nil
            focusField?()
        }
    }
}
