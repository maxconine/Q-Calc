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
    /// native only (MessageFiles.swift): shared clipboard text, or a file, instead of words
    var clip: String? = nil
    var file: ThreadFile? = nil
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

/// Everything the bar shows and every key it takes. The SwiftUI views only draw this; AppKit (OverlayController)
/// owns the window. The engine is only ever called through `engine` (its own thread); nothing here blocks the
/// main thread on maths.
final class BarModel: ObservableObject {
    // the line
    @Published var text = "" { didSet { if text != oldValue { textChanged(from: oldValue) } } }
    /// what a command line says in the answer slot (`periodic table`, `↵ play pong`), as .live.message does
    @Published var commandMessage: String?
    @Published private(set) var live = LiveAnswer.blank
    /// a closed form the engine's PSLQ search found after the answer (π²/6), shown in place of `live.exact`
    @Published private(set) var upgradedExact: String? { didSet { if upgradedExact != oldValue { refreshTypeset() } } }
    /// the rest of the word being typed, drawn after the caret; Tab or → takes it
    @Published var completion: String?
    @Published var decorations = Decorations()
    // `sys N` (Systems.swift)
    @Published var sysLines: [String]?
    @Published var sysAnswer: SysAnswer?
    @Published var sysFocus: Int?
    // onboarding and the walkthrough (Tour.swift)
    @Published var example: ExampleShown?
    /// .tour-line role="status": VoiceOver hears each step as it comes
    @Published var tourLine: TourLineShown? {
        didSet { if let tourLine, tourLine.text != oldValue?.text { Self.announce(tourLine.text) } }
    }
    var replay: Int?
    var rotation: (on: Bool, tick: Int) = (false, 0)
    var rotationTimer: Timer?
    var pauseTimer: Timer?
    var ghostExample: String?
    var ghostPlain: String?
    var hintGhost: (hint: String, expr: String, plain: Bool)?
    /// the first run's lead example, `Press ⌃⌥Space anytime`
    var firstRunHotkey: String?
    /// d/dt letters the user moved by hand (TimeLetters.tsx); sent as EvalOptions.timeVarying
    @Published var timeVarying: [String: Bool] = [:]
    /// the last real answer, dimmed, while the line is briefly unreadable (useSteadyAnswer)
    @Published private(set) var steady: String?
    @Published private(set) var chained = false
    @Published private(set) var forms: [String] = [] { didSet { if forms != oldValue { refreshTypeset() } } }
    @Published private(set) var formIndex = 0 { didSet { if formIndex != oldValue { refreshTypeset() } } }
    @Published private(set) var copied = false
    /// the status line under the bar; VoiceOver hears each new one, since nothing moves its focus there
    @Published var hint: String? {
        didSet { if let hint, !hint.isEmpty, hint != oldValue { Self.announce(hint) } }
    }
    /// the hint from before files were dragged over the bar, put back when they leave (Messages/ShareCommands.swift)
    var dragHint: String?
    // the tape
    @Published var rows: [HistoryRow] = []
    @Published var selected: Int?
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
    @Published var graph: GraphState?
    @Published var cards: [BannerCard] = []
    @Published var cardsHovered = false
    /// keyboard focus is on an edge tool (Full Keyboard Access), which shows them as hover does
    @Published var edgeFocus = false
    @Published var replyFocused: String?
    /// the message threads under the bar (native only, Messages/MessageThreads.swift): the conversations list or
    /// one thread, its reply line's words and how its last send went
    @Published var messagesPanel: MessagesPanel?
    /// the card whose reply line Tab asks to focus (tabToCard)
    @Published var cardFocusRequest: String?
    /// the selected part of the line's own answer (inputSelected)
    @Published var selectionAnswer: String?
    @Published var threadDraft = ""
    @Published var threadReply = BannerCard.Reply.idle {
        didSet {
            guard threadReply != oldValue else { return }
            switch threadReply {
            case .sent: Self.announce("sent")
            case let .failed(note): Self.announce(note)
            default: break
            }
        }
    }
    // settings mirrored for the views
    @Published private(set) var angleRad = false
    @Published private(set) var fractions = false
    @Published private(set) var sigFigMode = false

    let settings = AppSettings.shared
    let history: HistoryStore
    let engine: EngineThread
    /// the engine itself, only for its async calls (closed forms, graphs), which never block
    let core: Engine
    let sheets: SheetData = SheetData.shared
    /// the typeset preview row (MathText.swift); nil hides it
    @Published private(set) var typeset: Typeset?
    private lazy var typesetter = MathTypesetter(engine: core)
    private var typesetAsked: TypesetRequest?

    // what the bar asks of its window
    var onHide: ((String) -> Void)?
    /// the bar back up (after the file picker took the focus and hid it)
    var onShow: ((String) -> Void)?
    var onSettings: (() -> Void)?
    /// settings at the Messages tab, where the name is set (native only; the web-view app opens General)
    var onMessagesSettings: (() -> Void)?
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
    /// dev builds only: told each time the live answer lands for a typed line (DevBench's keystroke timing)
    var onLive: ((String) -> Void)?
    /// sanitizeOnboarding and tourRunning for the stored onboarding state they were worked out from (Tour.swift):
    /// both pure, and asked on every keystroke
    var onboardingMemo: (raw: [String: Int], state: [String: Any], running: Bool?)?

    private var deleted: (row: HistoryRow, index: Int)?
    private var lastLiveText = ""
    private var steadyTimer: Timer?
    private var recentTimer: Timer?
    private var copiedTimer: Timer?
    private var cardTimer: Timer?
    private let effectGate = EggGate()
    private let lineEggGate = LineEggGate()
    private let konamiGate = KonamiGate()
    private var effectSettle: Timer?
    /// the line a black hole, the π digits or the barrel roll is playing for; typing on ends it
    private var effectLine: String?
    /// the black hole has the answer: the slot hides it until it comes back (the answer itself never changes)
    @Published var answerVeiled = false
    /// the Konami code's green-phosphor CRT look, for this run only (never saved)
    @Published private(set) var crt = false
    /// where the answer slot sits, in the window's content (top-left origin), for the effects that use it
    var answerFrame: CGRect = .zero
    /// an answer-bound effect (black hole, π digits, barrel roll) should end now: the line changed under it
    var onEffectInterrupt: (() -> Void)?
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
        // clips and files say things in the core's words (Messages/MessageFiles.swift)
        MessageFiles.shared.logic = logic
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

    var options: EvalOptions { EvalOptions.from(settings) }

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
        // the conversations list is picked from with the line empty; typing leaves it
        if case .list = messagesPanel, !text.isEmpty { messagesPanel = nil }
        if text.isEmpty { hint = nil; decorations = Decorations() }
        if !text.isEmpty { dismissRotation() }
        evaluateLive()
        refreshTour()
    }

    func evaluateLive() {
        let typed = text
        // `sys`: a hint until there's a count; the panel's own answer once its fields are open
        let sys = SysCommand.of(typed)
        if case let .count(n) = sys, sysLines?.count == n {
            commandMessage = nil
            setLive(.blank, for: typed)
            engine.cancel(.live)
            return
        }
        if sysLines != nil { sysLines = nil; sysAnswer = nil }
        if case let .hint(h) = sys {
            commandMessage = h
            setLive(.blank, for: typed)
            engine.cancel(.live)
            return
        }
        if let threads = threadCommand(typed) {
            nameCheckTimer?.invalidate()
            graph = nil
            helpShown = false
            messagesLine(typed, threads)
            setLive(.blank, for: typed)
            engine.cancel(.live)
            return
        }
        if shareCommand(typed) != nil {
            nameCheckTimer?.invalidate()
            graph = nil
            helpShown = false
            shareLine(typed)
            setLive(.blank, for: typed)
            engine.cancel(.live)
            return
        }
        // a file dropped on the bar waits only while the line asks for its name
        MessageFiles.shared.pendingDrop = []
        // the Konami code typed out says what ↵ does, as a command does
        if isKonamiLine(text: typed) {
            nameCheckTimer?.invalidate()
            graph = nil
            helpShown = false
            decorations = Decorations()
            completion = nil
            commandMessage = logic.string("lib/onboarding", "konamiHint", [crt])
            setLive(.blank, for: typed)
            engine.cancel(.live)
            return
        }
        let command = BarCommand.of(typed, sheets: sheets)
        if let command {
            var found: Bool?
            if case let .message(_, name, _) = command {
                found = recipientFound(name)
                messageLine(typed, name: name, found: found)
                checkName(name)
            } else {
                nameCheckTimer?.invalidate()
            }
            commandMessage = command.hint(messaging: Messages.shared, found: found)
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
            // no grey parens left over from the line that was just deleted
            decorations = Decorations()
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
            self.pauseHintLater()
            self.refreshTour()
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

    /// blankReason, each piece read by the engine as the line is (its context, chained as it is); runs on the
    /// engine thread
    private static func blankSpan(engine: Engine, request: LiveRequest) -> NSRange? {
        guard let s = engine.blankSpan(request: request), s.end > s.start else { return nil }
        return NSRange(location: Int(s.start), length: Int(s.end - s.start))
    }

    private var squiggleTimer: Timer?
    var graphTask: Task<Void, Never>?
    private lazy var lastShareUsage = AppSettings.shared.shareUsage
    /// the line a unit was offered for, and what ↵ again makes it
    private var unitOffer: (typed: String, filled: String)?

    // the squiggle shows once typing has paused 700 ms on a blank line (SQUIGGLE_IDLE_MS), never for commands
    private func squiggleLater(_ typed: String) {
        squiggleTimer?.invalidate()
        guard live.isBlank, !typed.trimmingCharacters(in: .whitespaces).isEmpty, BarCommand.of(typed, sheets: sheets) == nil,
              !isBarrelRoll(text: typed), !isKonamiLine(text: typed) else { return }
        squiggleTimer = Timer.scheduledTimer(withTimeInterval: 0.7, repeats: false) { [weak self] _ in
            guard let self, self.text == typed, self.live.isBlank else { return }
            let rows = self.rows, view = HistoryView.from(self.settings), options = self.options, thread = self.engine
            self.engine.submit(.forms, { engine -> NSRange? in
                let request = Self.request(engine, text: typed, rows: rows, view: view, options: options)
                if thread.logic.bool("lib/nativeEval", "looksLikeNaturalLanguage", [typed]) { return nil }
                return Self.blankSpan(engine: engine, request: request)
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

    /// Type in the grey parens the engine would add (inferParens), both ends: Tab, or → at the end of the line
    /// (native only)
    func acceptParens() -> Bool {
        guard let fill = logic.call("engine/parens", "inferParens", [text]) as? [String: Any] else { return false }
        let lead = (fill["leading"] as? NSNumber)?.intValue ?? 0, trail = (fill["trailing"] as? NSNumber)?.intValue ?? 0
        guard lead + trail > 0 else { return false }
        let filled = String(repeating: "(", count: lead) + text + String(repeating: ")", count: trail)
        if let setField { setField(filled) } else { text = filled }
        return true
    }

    /// selDisplay (QuickCalc.tsx): part of the line selected is evaluated alone, with no history, and shown muted
    /// in the answer slot (`.live.live-selection`, aria-live), unless it's the whole line, a graph's, or reads the
    /// same as its value. Nil otherwise.
    func inputSelected(_ line: String, _ range: NSRange) {
        let ns = line as NSString
        let part = range.length > 0 && NSMaxRange(range) <= ns.length
            ? ns.substring(with: range).trimmingCharacters(in: .whitespaces) : ""
        guard !part.isEmpty, part != line.trimmingCharacters(in: .whitespaces), graph == nil else {
            if selectionAnswer != nil { selectionAnswer = nil }
            engine.cancel(.selection)
            return
        }
        let view = HistoryView.from(settings), options = self.options
        engine.submit(.selection, { engine -> String? in
            let answer = engine.live(request: Self.request(engine, text: part, rows: [], view: view, options: options))
            let shown = answer.display
            return shown.isEmpty || shown == part || answer.improper ? nil : shown
        }) { [weak self] shown in
            guard let self, self.text == line else { return }
            self.selectionAnswer = shown
            if let shown { Self.announce(shown) }
        }
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
        refreshTypeset()
        onLive?(typed)
    }

    /// The typeset row exactly when the web app shows it (QuickCalc.tsx `typstShown`): the setting on, no command or
    /// panel of its own (graph, sys, periodic, sheets, games, help, the tutorial), and Typst would show something
    /// the bar doesn't (`typstPreviewUseful`). The answer beside it is the one the slot shows, nothing for a message.
    func refreshTypeset() {
        let command = BarCommand.of(text, sheets: sheets)
        guard settings.typstPreview, !text.trimmingCharacters(in: .whitespaces).isEmpty, command == nil,
              SysCommand.of(text) == nil, sysLines == nil, graph == nil, !helpShown, !gamesShown else {
            typesetAsked = nil
            typeset = nil
            return
        }
        let solve = live.solve
        let rootsOf = solve?.outcome == .roots ? solve?.variable ?? "" : ""
        let tabbed = formIndex > 0 && forms.indices.contains(formIndex)
        let request = TypesetRequest(
            expr: chained ? Chain.expr(text) : text,
            exact: prefixShown != nil || tabbed ? nil : exact,
            display: prefixShown ?? (tabbed ? forms[formIndex] : live.display),
            solvedFor: rootsOf,
            message: solve != nil && rootsOf.isEmpty)
        guard request != typesetAsked else { return }
        typesetAsked = request
        typesetter.typeset(request) { [weak self] outcome in
            guard let self, self.typesetAsked == request else { return }
            switch outcome {
            case .hidden: self.typeset = nil
            case let .drawn(paths, doc): self.typeset = Typeset(paths: paths, doc: doc)
            case let .failed(doc): if self.typeset?.doc != doc { self.typeset?.stale = true }
            }
        }
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

    // the 420 / 67 / 69 effects: the answer rests on the number for a moment (so 4200 typed passes through 420
    // without firing), then the core's gate fires it once, and again only after the answer was something else
    // The native-only line eggs (divide by zero, Euler's identity, Pi Day, the barrel roll) settle the same way,
    // read from the line and the answer shown; one of them wins over a number egg.
    private func checkEffects(_ answer: LiveAnswer) {
        let n = answer.unit == nil ? answer.number : nil
        let typed = text, display = answer.display, number = answer.number
        if let line = effectLine, line != typed {
            effectLine = nil
            onEffectInterrupt?()
        }
        effectSettle?.invalidate()
        effectSettle = Timer.scheduledTimer(withTimeInterval: eggSettleMs() / 1000, repeats: false) { [weak self] _ in
            guard let self else { return }
            let egg = self.effectGate.settle(n: n)
            if let line = self.lineEggGate.settle(text: typed, display: display, number: number, piDay: Self.piDay) {
                self.effectLine = line == .euler ? nil : typed
                self.onEffect?(Self.effectName(line))
            } else if let egg {
                self.onEffect?(egg == .smoke ? "smoke" : egg == .arms ? "arms" : "fold")
            }
        }
    }

    static func effectName(_ line: LineEggKind) -> String {
        switch line {
        case .void: return "void"
        case .euler: return "euler"
        case .piDigits: return "pidigits"
        case .barrelRoll: return "roll"
        }
    }

    /// read once: the composer asks on every redraw, and the environment is copied out whole each time it's read
    private static let piDayForced = AppInfo.isDev && ProcessInfo.processInfo.environment["QCALC_PI_DAY"] == "1"

    /// March 14 by the local calendar (dev builds: QCALC_PI_DAY=1 makes any day Pi Day, to try it)
    static var piDay: Bool {
        if piDayForced { return true }
        let d = Calendar.current.dateComponents([.month, .day], from: Date())
        return isPiDay(month: UInt32(d.month ?? 0), day: UInt32(d.day ?? 0))
    }

    /// One key down in the bar (the app's key monitor sees it before anything takes it): ↑↑↓↓←→←→BA turns the
    /// CRT look on, and again off.
    func konamiKey(_ event: NSEvent) {
        guard !event.isARepeat else { return }
        let key: KonamiKey
        let mods = event.modifierFlags.intersection([.command, .control, .option])
        switch event.keyCode {
        case 126: key = .up
        case 125: key = .down
        case 123: key = .left
        case 124: key = .right
        default:
            let c = event.charactersIgnoringModifiers?.lowercased()
            key = !mods.isEmpty ? .other : c == "b" ? .b : c == "a" ? .a : .other
        }
        let before = konamiGate.progress()
        if konamiGate.push(key: key) {
            crt.toggle()
            OverlayLog.note("konami: crt \(crt ? "on" : "off")")
        } else if before >= 4, konamiGate.progress() <= before {
            // how far a try got, and the key that broke it, to tell a missed key from one the bar never saw
            OverlayLog.note("konami: \(before)/10, then \(key) (key code \(event.keyCode))")
        }
    }

    // MARK: - answer forms (⌃E)

    /// ⌃E (⌃⇧E back; Tab on the page, native moved it so Tab fills parens): the engine's answerForms for the answer as shown, cycled (useAnswerForms)
    func cycleForms(back: Bool = false) {
        guard !live.isBlank, live.valueJson != nil else { return }
        track("form.tab")
        if !forms.isEmpty {
            formIndex = (formIndex + (back ? forms.count - 1 : 1)) % forms.count
            return
        }
        let typed = text
        let options = self.options
        let display = live.display, exact = self.exact, valueJson = live.valueJson, meas = live.meas
        engine.submit(.forms, { engine -> [String] in
            var out = [display]
            let source = FormSource(valueJson: valueJson ?? "", display: display, exact: exact, meas: meas,
                                    sigFigs: options.sigFigs, sigFigMode: options.sigFigMode, rationalize: options.rationalize)
            let views = valueJson == nil ? [] : engine.answerForms(source: source)
            // the engine's answerForms (WP-APP-LOGIC): the other ways ⌃E shows this answer
            out += views.map(\.display)
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
        if text.isEmpty, listEnter() { return }
        if let threads = threadCommand(text) {
            runThreadCommand(threads)
            return
        }
        if let share = shareCommand(text) {
            runShare(share)
            return
        }
        if gamesShown {
            guard let g = gamePick else { commit(); return }
            let game = BarCommand.gameList[g]
            play(game.kind)
            return
        }
        if case let .count(n) = SysCommand.of(text), enterSystem(n) { return }
        // `do a barrel roll` saves nothing and says nothing: ↵ rolls now if the line hasn't settled yet, and clears it
        if isBarrelRoll(text: text) {
            effectSettle?.fire()
            effectLine = nil
            clearLine()
            return
        }
        // the code typed out (or `konami`) toggles the CRT look as the keys do, and saves nothing
        if isKonamiLine(text: text) {
            crt.toggle()
            OverlayLog.note("konami (typed): crt \(crt ? "on" : "off")")
            clearLine()
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
            if m.name.isEmpty {
                (onMessagesSettings ?? onSettings)?()
                return
            }
            guard !name.isEmpty, !said.isEmpty else { return }
            hint = "sending to \(name)…"
            m.send(to: name, text: said) { [weak self] status in
                guard let self else { return }
                // a send says whether the name is anyone's too, and who was sent to finishes first next time
                self.nameCheck.noteSend(name: name, status: status, nowMs: Self.nowMs)
                if status == "delivered" { m.noteSent(name) }
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
            startTutorial()
        case .skip:
            clearLine()
            skipTutorial()
        case .graph:
            break
        case .greeting:
            clearLine()
        }
    }

    private func play(_ kind: String) {
        clearLine()
        onGame?(kind)
    }

    func clearLine(keepHint: Bool = false) {
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
        let improper = live.improper, history = !rows.isEmpty, facts = hintFacts(typed)
        engine.submit(.commit, { engine -> (CommitResult, String?, (at: Int, text: String, filled: String)?, NSRange?) in
            let request = Self.request(engine, text: typed, rows: rows, view: view, options: options)
            let done = engine.commit(request: request, history: rows)
            guard !done.saved else { return (done, nil, nil, nil) }
            // units that don't add up because one number has none: the unit it was owed, offered (offerUnit)
            if improper {
                if let fix = engine.missingUnit(request: request) {
                    return (done, fix.hint, (Int(fix.at), fix.text, fix.filled), nil)
                }
            }
            // enter that saved nothing says why (enterHint), and the squiggle shows where
            let graph = typed.trimmingCharacters(in: .whitespaces).lowercased().hasPrefix("graph")
            let span = graph ? nil : Self.blankSpan(engine: engine, request: request)
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
                self.hint = self.onboardingAfterCommit(expr: typed, facts: facts, solve: done.row?.solve != nil, graph: typed.lowercased().hasPrefix("graph"))
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
        if listMove(up: up) { return }
        if gamesShown {
            // in the games list ↑ starts at the game nearest the bar and climbs; ↓ past the last leaves it
            let last = BarCommand.gameList.count - 1
            if up { gamePick = gamePick.map { max(0, $0 - 1) } ?? last } else if let g = gamePick { gamePick = g >= last ? nil : g + 1 }
            if let g = gamePick { Self.announce("\(BarCommand.gameList[g].name), \(BarCommand.gameList[g].blurb)") }
            return
        }
        if helpShown { helpShown = false }
        guard !rows.isEmpty else { return }
        if up {
            let fresh = !tapeOpen || selected == nil
            if fresh { sideIsExpr = false }
            tapeOpen = true
            land(on: fresh ? rows.count - 1 : max(0, selected! - 1))
            announceSelected()
        } else {
            guard tapeOpen else { return }
            guard let i = selected, i < rows.count - 1 else {
                selected = nil
                focusField?()
                return
            }
            land(on: i + 1)
            announceSelected()
        }
    }

    /// VoiceOver keeps its cursor on the line while ↑ ↓ walk the tape, so it hears the row each lands on (native
    /// only; the page's rows are only aria-selected), as the row reads (`HistoryRowView`)
    private func announceSelected() {
        guard let i = selected, rows.indices.contains(i) else { return }
        Self.announce("\(rows[i].expr) equals \(rows[i].display)")
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
        if row.kind == .system { openHistorySystem(i); return }
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
        if row.kind == .system {
            openHistorySystem(i)
            return
        }
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

    /// a d/dt chip: move the letter to the other side (EvalOptions.timeVarying), and answer again
    func setTimeVarying(_ letter: String, _ varying: Bool) {
        timeVarying[letter] = varying
        evaluateLive()
        focusField?()
    }

    // MARK: - SI prefix (⌥↑ ⌥↓)

    @Published private(set) var prefixShown: String? { didSet { if prefixShown != oldValue { refreshTypeset() } } }
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
    private var undoState = Undo.start("")
    private var undoReady = false

    func resetUndo() {
        undoState = Undo.start(text)
        undoReady = true
    }

    func recordEdit(_ value: String, start: Int, end: Int, kind inputType: String?) {
        if !undoReady { resetUndo() }
        undoState = Undo.record(undoState, Undo.Step(value: value, start: start, end: end), Undo.editKind(inputType),
                                now: Date().timeIntervalSince1970 * 1000)
    }

    func breakUndoRun() {
        guard undoReady else { return }
        undoState = Undo.breakRun(undoState)
    }

    private func applyUndo(_ redo: Bool) -> Bool {
        if !undoReady { resetUndo() }
        guard let next = redo ? Undo.redo(undoState) : Undo.undo(undoState) else { return false }
        undoState = next
        let c = next.current
        restoreField?(c.value, NSRange(location: c.start, length: max(0, c.end - c.start)))
        return true
    }

    func undoText() -> Bool { applyUndo(false) }
    func redoText() -> Bool { applyUndo(true) }

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

    /// a status VoiceOver reads out without moving its cursor
    static func announce(_ text: String, high: Bool = false) {
        NSAccessibility.post(element: NSApp as Any, notification: .announcementRequested,
                             userInfo: [.announcement: text,
                                        .priority: (high ? NSAccessibilityPriorityLevel.high : .medium).rawValue])
    }

    private func flashCopied() {
        copied = true
        copiedTimer?.invalidate()
        copiedTimer = Timer.scheduledTimer(withTimeInterval: 1.2, repeats: false) { [weak self] _ in self?.copied = false }
        NSAccessibility.post(element: NSApp as Any, notification: .announcementRequested,
                             userInfo: [.announcement: "Copied", .priority: NSAccessibilityPriorityLevel.high.rawValue])
    }

    // MARK: - toggles

    func toggleAngle() {
        let rad = !angleRad
        settings.setAngleMode(rad ? "rad" : "deg", notifyWeb: true)
        announceMode("angle", rad)
    }
    func toggleFractions() {
        let on = !fractions
        settings.setFractionMode(on, notifyWeb: true)
        announceMode("fraction", on)
    }
    func toggleSigFigMode() {
        let on = !sigFigMode
        settings.setSigFigMode(on, notifyWeb: true)
        announceMode("sigFigs", on)
    }

    /// VoiceOver hears where a toggle left the mode (native only; the edge tools showing it are hidden off hover)
    private func announceMode(_ action: String, _ on: Bool) {
        if let line = logic.string("lib/onboarding", "modeAnnouncement", [action, on]), !line.isEmpty { Self.announce(line) }
    }

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
            if code == kVK_RightArrow, caretAtEnd?() ?? false { return acceptCompletion() || acceptParens() }
            return false
        case kVK_Tab:
            guard flags.isSubset(of: [.shift]) else { return false }
            if !flags.contains(.shift) { _ = fillTourStep() || acceptCompletion() || acceptParens() || tabToCard() }
            return true
        case kVK_ANSI_E:
            // the answer's other forms (base, exponent, fraction…), which Tab cycles on the page
            guard flags == [.control] || flags == [.control, .shift] else { return false }
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
        if sysLines != nil {
            closeSystem()
            text = ""
            return true
        }
        if replyFocused != nil {
            replyFocused = nil
            focusField?()
            return true
        }
        if closeMessages() { return true }
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
        beginShowing()
    }

    func willHide() {
        replyFocused = nil
        _ = closeMessages(refocus: false)
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


    // MARK: - the message line (nameCheck.ts, messaging.ts)

    /// the relay's answers about names, kept a while (core's name_check)
    private let nameCheck = NameCheck()
    var nameCheckTimer: Timer?

    private static var nowMs: Double { Date().timeIntervalSince1970 * 1000 }

    /// whether the name after `message` is anyone's: nil until the relay has said, and whenever it can't (no name
    /// of your own, the relay out of reach), so the bar never calls a name missing that the relay didn't
    func recipientFound(_ name: String) -> Bool? {
        let m = Messages.shared
        guard !name.isEmpty, !m.name.isEmpty, m.link != .offline else { return nil }
        return nameCheck.cached(name: name, nowMs: Self.nowMs)
    }

    /// asks the relay about the name once typing has rested, unless it's known or already being asked
    private func checkName(_ name: String) {
        nameCheckTimer?.invalidate()
        guard !name.isEmpty, !Messages.shared.name.isEmpty, Messages.shared.link != .offline, recipientFound(name) == nil else { return }
        nameCheckTimer = Timer.scheduledTimer(withTimeInterval: Double(nameCheckDelayMs()) / 1000, repeats: false) { [weak self] _ in
            guard let self, self.nameCheck.start(name: name, nowMs: Self.nowMs) else { return }
            Messages.shared.lookup(name) { [weak self] found in
                guard let self else { return }
                self.nameCheck.finish(name: name, found: found, nowMs: Self.nowMs)
                if found != nil, case let .message(_, now, _)? = BarCommand.of(self.text, sheets: self.sheets), now == name {
                    self.evaluateLive()
                }
            }
        }
    }

    /// the name painted green once it's someone's, and Tab finishing the names you've sent to first
    private func messageLine(_ typed: String, name: String, found: Bool?) {
        var deco = Decorations()
        if found == true, let span = logic.call("lib/messaging", "messageNameSpan", [typed]) as? [String: Any],
           let start = span["start"] as? Int, let end = span["end"] as? Int, end <= (typed as NSString).length {
            let range = NSRange(location: start, length: end - start)
            if (typed as NSString).substring(with: range).lowercased() == name { deco.units = [range] }
        }
        decorations = deco
        let messaging = (try? JSONSerialization.jsonObject(with: Data(Messages.shared.webJSON.utf8))) ?? [:]
        let contacts = logic.call("lib/messaging", "messageContacts", [messaging]) as? [String] ?? []
        let rest = logic.string("lib/completion", "completionFor", [typed, ["contacts": contacts]])
        completion = rest.flatMap { $0.isEmpty ? nil : $0 }
    }

    // MARK: - message cards

    func pullInbox() {
        guard Messages.shared.barOpen,
              let rows = try? JSONSerialization.jsonObject(with: Data(Messages.shared.inboxJSON.utf8)) as? [[String: Any]]
        else { return }
        var ids: [String] = []
        let before = Set(cards.map(\.id))
        for o in rows {
            guard let id = o["id"] as? String, let from = o["from"] as? String else { continue }
            ids.append(id)
            let ask = o["ask"] as? Bool ?? false
            let text = MessageInbox.cleanText(o["text"] as? String ?? "", max: Messages.textMax)
            let card = BannerCard(id: id, from: from, text: ask ? "" : text, ask: ask)
            // the open thread with them shows it (MessageHistory has it already), so no card
            if !ask, threadOpen(with: from) { continue }
            if cards.contains(where: { $0.id == id || (ask && $0.ask && $0.from == from) }) { continue }
            cards.insert(card, at: 0)
        }
        // clips and files (MessageFiles.swift); the open thread with them shows them already
        for card in MessageFiles.shared.takeCards() where !threadOpen(with: card.from) {
            cards.insert(card, at: 0)
        }
        if cards.count > 3 { cards = Array(cards.prefix(3)) }
        // VoiceOver hears each new card (they hide on their own after 10 s)
        for card in cards.reversed() where !before.contains(card.id) {
            let kind = card.ask ? "ask" : card.clip != nil ? "clip" : card.file != nil ? "file" : "text"
            let said = card.file?.name ?? card.text
            if let line = logic.string("lib/messaging", "cardAnnouncement", [card.from, kind, said]) { Self.announce(line) }
        }
        if !ids.isEmpty { Messages.shared.seen(ids) }
        scheduleCardExpiry()
    }

    private func dropAnsweredCards() {
        let m = Messages.shared
        cards.removeAll { m.blocked.contains($0.from) || ($0.ask && m.accepted.contains($0.from)) }
    }

    /// cardHeld: a card waits while the pointer is on the cards, or while its reply is typed or sent; a file's
    /// waits while it's on its way
    private func held(_ card: BannerCard) -> Bool {
        if let file = card.file, let t = MessageFiles.shared.transfers[file.id],
           ["accepting", "waiting", "downloading", "decrypting"].contains(t.state) { return true }
        return cardsHovered || replyFocused == card.id || !card.draft.isEmpty || card.reply == .sending
    }

    func scheduleCardExpiry() {
        cardTimer?.invalidate()
        guard !cards.isEmpty else { return }
        cardTimer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in self?.expireCards() }
    }

    private func expireCards() {
        let now = Date()
        for i in cards.indices where held(cards[i]) { cards[i].shownAt = now }
        // 10 s, as the page; with VoiceOver on, 30 (native only), since reading a card and its buttons takes longer
        let shown: TimeInterval = NSWorkspace.shared.isVoiceOverEnabled ? 30 : 10
        cards.removeAll { now.timeIntervalSince($0.shownAt) >= shown }
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
                Self.announce("sent")
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
                let note = Messages.sendStatusText(status, to: card.from)
                Self.announce(note)
                self.cards[j].reply = .failed(note)
            }
        }
    }

    func answerAsk(_ id: String, allow: Bool) {
        guard let card = cards.first(where: { $0.id == id }) else { return }
        if allow { Messages.shared.allow(card.from) } else { Messages.shared.block(card.from) }
        cards.removeAll { $0.id == id }
        // Allow / Block had the keyboard (Full Keyboard Access, VoiceOver) and is gone: the line takes it back
        focusField?()
    }

    /// Tab on an empty line with a message card up moves to the newest card's reply line (native only: the line
    /// keeps Tab for completion and parens, so the keyboard had no way into a card before it went)
    func tabToCard() -> Bool {
        guard text.isEmpty, let card = cards.first(where: { !$0.ask && $0.reply != .sent }) else { return false }
        cardFocusRequest = card.id
        return true
    }

    func dismissCard(_ id: String) {
        cards.removeAll { $0.id == id }
        if replyFocused == id {
            replyFocused = nil
            focusField?()
        }
    }
}
