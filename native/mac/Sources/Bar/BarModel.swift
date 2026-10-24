import AppKit
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
}

/// Everything the bar shows and every key it takes. The SwiftUI views only draw this; AppKit (OverlayController)
/// owns the window. The engine is only ever called through `engine` (its own thread); nothing here blocks the
/// main thread on maths.
final class BarModel: ObservableObject {
    // the line
    @Published var text = "" { didSet { if text != oldValue { textChanged() } } }
    @Published private(set) var live = EngineAnswer.blank
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
    @Published private(set) var pickAnswer = false
    @Published private(set) var tapeOpen = false
    @Published private(set) var recentFrom = 0
    // sheets above the bar
    @Published private(set) var gamesShown = false
    @Published private(set) var gamePick = 0
    @Published private(set) var helpShown = false
    // panels below
    @Published private(set) var graph: GraphState?
    @Published var cards: [BannerCard] = []
    @Published var cardsHovered = false
    @Published var replyFocused: String?
    // settings mirrored for the views
    @Published private(set) var angleRad = false
    @Published private(set) var fractions = false
    @Published private(set) var sigFigMode = false

    let settings = AppSettings.shared
    let history: HistoryStore
    let engine: EngineThread
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

    private var deleted: (row: HistoryRow, index: Int)?
    private var lastLiveText = ""
    private var steadyTimer: Timer?
    private var recentTimer: Timer?
    private var copiedTimer: Timer?
    private var cardTimer: Timer?
    private var effectArmed = true
    private var bag: Set<AnyCancellable> = []
    private static let draftKey = "qcalc.draft"

    init(engine: EngineThread, history: HistoryStore) {
        self.engine = engine
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

    private var options: EngineOptions {
        var o = EngineOptions.from(settings)
        if let last = rows.last(where: { $0.kind != "function" && $0.kind != "definition" }) {
            o.ans = last.n
            o.ansQuantity = last.quantity
        }
        return o
    }

    /// definitions in the rows you can see, ahead of the live line, so `x = 2` then `x^2` works
    private var contextLines: [String] {
        let start = history.scopeStart(show: settings.historyShow)
        return rows[start...].filter(\.defines).map(\.expr)
    }

    // MARK: - the live answer

    private var lastAnswer: HistoryRow? { rows.last { $0.kind == nil && !$0.display.isEmpty } }

    private func textChanged() {
        formIndex = 0
        forms = []
        copied = false
        if selected != nil, !text.isEmpty { selected = nil }
        gamesShown = BarCommand.of(text, sheets: sheets) == .games
        if !gamesShown { gamePick = 0 }
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
        let last = lastAnswer
        let chain = Chain.chains(typed, lastPlain: last?.display, lastHasUnit: last?.quantity != nil)
        let line = chain ? Chain.expr(typed) : typed
        let lines = contextLines + [line]
        let options = self.options
        engine.submit(.live, { engine in engine.evaluate(lines: lines, options: options).last ?? .blank }) { [weak self] answer in
            guard let self, self.text == typed else { return }
            self.chained = chain
            if !answer.isBlank { self.hint = nil }
            self.setLive(answer, for: typed)
        }
    }

    private func setLive(_ answer: EngineAnswer, for typed: String) {
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
        if settings.answerForm == "exact", let exact = live.exact, !exact.isEmpty { return exact }
        return live.display
    }

    var hasDual: Bool {
        guard formIndex == 0, let exact = live.exact?.trimmingCharacters(in: .whitespaces), !exact.isEmpty else { return false }
        return exact != live.display.trimmingCharacters(in: .whitespaces)
    }

    func pretty(_ s: String) -> String { Pretty.answer(s) }

    // the 420 / 67 / 69 effects fire once as the answer arrives, and again only after it changes away
    private func checkEffects(_ answer: EngineAnswer) {
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

    /// answerForms.ts until the core's answer_forms: the decimal, the exact form, a fraction and scientific,
    /// each worked out on the engine thread
    func cycleForms(back: Bool = false) {
        guard !live.isBlank, let n = live.number else { return }
        if !forms.isEmpty {
            formIndex = (formIndex + (back ? forms.count - 1 : 1)) % forms.count
            return
        }
        let typed = text
        let chain = chained
        let line = chain ? Chain.expr(typed) : typed
        let lines = contextLines + [line]
        var asFraction = options
        asFraction.fractionMode = true
        let display = live.display, exact = live.exact, unit = live.unit
        let figs = settings.significantFigures
        engine.submit(.forms, { engine -> [String] in
            var out = [display]
            if let exact, !exact.isEmpty { out.append(exact) }
            if let f = engine.evaluate(lines: lines, options: asFraction).last?.display, f.contains("/"),
               f.filter(\.isNumber).count <= 8 { out.append(f) }
            let mag = n == 0 ? 0 : Int(floor(log10(abs(n))))
            if abs(mag) >= 3 { out.append(Pretty.scientific(n, sigFigs: figs) + (unit.map { " \($0)" } ?? "")) }
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
            insertFromRow(i, answer: option ? !pickAnswer : pickAnswer)
            return
        }
        if gamesShown {
            let game = BarCommand.games[gamePick]
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
    @discardableResult
    func commit() -> Bool {
        let typed = text
        guard !typed.trimmingCharacters(in: .whitespaces).isEmpty else { return false }
        guard !live.isBlank, live.raw.hasSuffix(typed) || chained || live.raw == typed else {
            hint = "nothing to save yet · finish the line, or esc to close"
            return false
        }
        let expr = chained ? Chain.historyExpr(typed, plain: lastAnswer?.display ?? "") : typed
        var row = HistoryRow(id: HistoryRow.newID(), expr: expr, display: live.display, at: Date().timeIntervalSince1970 * 1000)
        row.exact = live.exact
        row.n = live.number
        row.quantity = live.quantity
        switch live.kind {
        case .assignment: row.kind = "definition"
        case .function:
            row.kind = "function"
            if let fn = Self.functionFields(live.json) { (row.fnName, row.fnParams, row.fnBody) = fn }
        default: break
        }
        history.append(row)
        rows = history.rows
        deleted = nil
        text = ""
        hint = nil
        refreshRecent()
        AppSettings.shared.mergeOnboarding(["commits": (AppSettings.shared.onboarding["commits"] ?? 0) + 1])
        return true
    }

    private static func functionFields(_ json: String) -> (String, [String], String)? {
        guard let o = try? JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any],
              let name = o["fnName"] as? String else { return nil }
        return (name, o["fnParams"] as? [String] ?? [], o["fnBody"] as? String ?? "")
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

    func moveSelection(up: Bool) {
        if gamesShown {
            gamePick = (gamePick + (up ? BarCommand.games.count - 1 : 1)) % BarCommand.games.count
            return
        }
        guard !rows.isEmpty else { return }
        if let i = selected {
            if up {
                selected = max(0, i - 1)
            } else if i + 1 < rows.count {
                selected = i + 1
            } else {
                selected = nil
                if settings.historyShow != "always" { tapeOpen = false }
            }
        } else if up {
            tapeOpen = true
            selected = rows.count - 1
            pickAnswer = settings.historyInsert == "answer"
        }
    }

    func pickSide(answer: Bool) { pickAnswer = answer }

    func select(_ i: Int?) {
        selected = i
    }

    var historySelected: Bool { selected != nil }

    func insertFromRow(_ i: Int, answer: Bool) {
        guard rows.indices.contains(i) else { return }
        let row = rows[i]
        let piece = answer ? (settings.answerForm == "exact" ? (row.exact ?? row.display) : row.display) : row.expr
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
            guard flags.isEmpty, selected != nil else { return false }
            pickSide(answer: code == kVK_RightArrow)
            return true
        case kVK_Tab:
            guard flags.isSubset(of: [.shift]) else { return false }
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
            if !live.isBlank { commit() }
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

    private func sampleGraph() {
        guard let g = graph else { return }
        let expr = g.expression, lo = g.xMin, hi = g.xMax
        let context = contextLines, options = self.options
        engine.submit(.graph, { engine in
            engine.graph(expression: expr, xMin: lo, xMax: hi, samples: 241, context: context, options: options)
        }) { [weak self] curves in
            guard let self, var now = self.graph, now.expression == expr, now.xMin == lo else { return }
            now.curves = curves ?? []
            now.failed = curves == nil
            self.graph = now
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
