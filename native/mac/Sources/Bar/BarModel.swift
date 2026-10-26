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
}

/// Everything the bar shows and every key it takes. The SwiftUI views only draw this; AppKit (OverlayController)
/// owns the window. The engine is only ever called through `engine` (its own thread); nothing here blocks the
/// main thread on maths.
final class BarModel: ObservableObject {
    // the line
    @Published var text = "" { didSet { if text != oldValue { textChanged() } } }
    @Published private(set) var live = LiveAnswer.blank
    /// a closed form the engine's PSLQ search found after the answer (π²/6), shown in place of `live.exact`
    @Published private(set) var upgradedExact: String?
    /// the rest of the word being typed, drawn after the caret; Tab or → takes it
    @Published private(set) var completion: String?
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

    private func textChanged() {
        formIndex = 0
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
            hint = command.hint(messaging: Messages.shared)
            if case let .graph(body) = command { showGraph(body) } else { graph = nil }
            setLive(.blank, for: typed)
            engine.cancel(.live)
            return
        }
        graph = nil
        if typed.trimmingCharacters(in: .whitespaces).isEmpty {
            engine.cancel(.live)
            chained = false
            setLive(.blank, for: typed)
            return
        }
        let rows = self.rows, view = HistoryView.from(settings), options = self.options
        engine.submit(.live, { engine -> (LiveAnswer, Bool) in
            let request = Self.request(engine, text: typed, rows: rows, view: view, options: options)
            return (engine.live(request: request), request.chained)
        }) { [weak self] answer, chain in
            guard let self, self.text == typed else { return }
            self.chained = chain
            if !answer.isBlank { self.hint = nil }
            self.completion = answer.completion.flatMap { $0.isEmpty ? nil : $0 }
            self.setLive(answer, for: typed)
            self.upgradeClosedForm(answer, for: typed)
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
            helpShown = true
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
        let rows = self.rows, view = HistoryView.from(settings), options = self.options
        engine.submit(.commit, { engine -> CommitResult in
            engine.commit(request: Self.request(engine, text: typed, rows: rows, view: view, options: options), history: rows)
        }) { [weak self] done in
            guard let self else { return }
            if done.saved {
                self.history.replace(done.history)
                self.rows = self.history.rows
                self.deleted = nil
                self.refreshRecent()
                AppSettings.shared.mergeOnboarding(["commits": (AppSettings.shared.onboarding["commits"] ?? 0) + 1])
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

    // MARK: - copy

    func copyAnswer(_ value: String? = nil) {
        let shown = value ?? shownAnswer
        guard !shown.isEmpty else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(UnitlessCopy.copied(shown), forType: .string)
        flashCopied()
    }

    func copyLine() {
        guard !live.isBlank else { return }
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
            if flags == [.command], text.isEmpty { return undoDelete() }
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
            if let result = try? await engine.graphAsync(input: "graph \(expr)", options: options, settings: settings),
               result.error == nil, !result.points.isEmpty {
                curves = [GraphCurve(label: result.label, points: result.points)]
                if result.yMax > result.yMin { yRange = (result.yMin, result.yMax) }
            } else if !Task.isCancelled {
                let context = engine.contextFromHistory(rows: rows, view: view)
                curves = GraphFallback.sample(engine, expression: expr, xMin: lo, xMax: hi, samples: 241, context: context, settings: settings)
            }
            if Task.isCancelled { return }
            await MainActor.run {
                guard let self, var now = self.graph, now.expression == expr, now.xMin == lo else { return }
                now.curves = curves ?? []
                now.failed = curves == nil
                if let yRange, now.fresh { (now.yMin, now.yMax) = yRange }
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
