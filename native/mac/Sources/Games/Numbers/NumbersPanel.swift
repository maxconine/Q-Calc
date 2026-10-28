import AppKit
import QCalcCore
import SwiftUI

// the numbers game: `24 game` or `countdown` then enter (components/NumbersPanel.tsx and NumbersPanel.css). the
// lobby picks the mode (1 or 2) and countdown's large numbers (← →); rounds run against the clock. The rules are
// the core's (`numbers_*`, from lib/numbers.ts; the faint parens are the bar's own `parens`).

private let statsKey = "qcalc-numbers"
private let seconds24 = 60.0, secondsCountdown = 45.0
private let bad = RGBA(0xd9534f)

private func loadStats() -> NumbersStats { numbersLoadStats(raw: GameSaves.load(statsKey)) }
private func saveStats(_ s: NumbersStats) { GameSaves.store(statsKey, numbersStatsJson(stats: s)) }

final class NumbersLobbyModel: ObservableObject {
    @Published var large = loadStats().large

    func change(_ by: Double) {
        let next = min(4, max(0, large + by))
        large = next
        var s = loadStats()
        s.large = next
        saveStats(s)
    }
}

struct NumbersPanel: View {
    @StateObject private var lobby = NumbersLobbyModel()

    var body: some View {
        GameLobby(LobbyConfig(
            game: "numbers", name: "24 / countdown", blurb: "make the number, against the clock.",
            transport: nil,
            local: LocalGames(prompt: "", options: ["24", "countdown"]) { [lobby] i, onEnd in
                AnyView(NumbersGame(countdown: i == 1, large: lobby.large, onMenu: { onEnd(nil) }))
            },
            footer: AnyView(NumbersFooter(lobby: lobby)),
            onKey: { [lobby] key in
                guard key.key == "ArrowLeft" || key.key == "ArrowRight" else { return false }
                lobby.change(key.key == "ArrowLeft" ? -1 : 1)
                return true
            },
            frame: true
        ))
    }
}

private struct NumbersFooter: View {
    @ObservedObject var lobby: NumbersLobbyModel

    var body: some View {
        PongNote("24: four numbers, each once, make exactly 24 in \(Int(seconds24))s · countdown: six numbers, get near the target in \(Int(secondsCountdown))s, dealing \(Int(lobby.large)) large · ← → changes")
    }
}

/// A round (`Round` in NumbersPanel.tsx).
struct NumbersRound {
    var countdown: Bool
    var deal: NumbersDeal
    var deadline: Double
    var over: (how: String, value: Double?, points: UInt32)?
}

final class NumbersModel: ObservableObject {
    @Published var stats: NumbersStats
    @Published var round: NumbersRound
    @Published var text = ""
    @Published var note = ""
    @Published var now = GameTime.date()
    private var timer: Timer?

    init(countdown: Bool, large: Double) {
        var s = loadStats()
        s.countdown = countdown
        s.large = large
        stats = s
        // the mode and large numbers the lobby picked are kept as the next default
        saveStats(s)
        round = NumbersModel.deal(countdown, large)
        let t = Timer(timeInterval: 0.2, repeats: true) { [weak self] _ in self?.clock() }
        RunLoop.main.add(t, forMode: .common)
        timer = t
    }

    static func deal(_ countdown: Bool, _ large: Double) -> NumbersRound {
        let d = numbersDeal(mode: countdown ? .countdown : .twentyFour, large: large)
        return NumbersRound(countdown: countdown, deal: d, deadline: GameTime.date() + (countdown ? secondsCountdown : seconds24) * 1000, over: nil)
    }

    var mode: NumbersMode { round.countdown ? .countdown : .twentyFour }
    var filled: String { numbersFillParens(text: text) }
    var check: NumbersCheck { numbersCheck(text: filled, nums: round.deal.nums, mode: mode) }

    private func update(_ f: (NumbersStats) -> NumbersStats) {
        stats = f(stats)
        saveStats(stats)
    }

    func start() {
        update { var s = $0; s.countdown = round.countdown; return s }
        round = NumbersModel.deal(round.countdown, stats.large)
        text = ""
        note = ""
        now = GameTime.date()
    }

    // ends the round once; points and streaks are kept as it ends
    func finish(_ how: String, _ value: Double?) {
        guard round.over == nil else { return }
        let points = round.countdown && how != "gave up" ? numbersPoints(target: round.deal.target, value: value) : 0
        if round.countdown { update { numbersRecordCountdown(stats: $0, points: points) } } else { update { numbersRecord24(stats: $0, solved: how == "solved") } }
        round.over = (how, value, points)
        note = ""
    }

    // the clock; when it runs out, countdown takes what's typed if it's a legal answer
    private func clock() {
        guard round.over == nil else { return }
        now = GameTime.date()
        guard now >= round.deadline else { return }
        let c = check
        finish("time", round.countdown && c.valid && c.hasValue ? c.n : nil)
    }

    func submit() {
        guard round.over == nil else { return }
        let c = check
        if !round.countdown {
            if numbersSolves24(text: filled, nums: round.deal.nums) { finish("solved", 24) }
            else if let p = c.problem { note = p }
            else if c.hasValue { note = "that’s \(c.valueText), not 24" }
            else { note = text.trimmingCharacters(in: .whitespaces).isEmpty ? "type an expression" : "not finished" }
            return
        }
        if c.valid && c.hasValue { finish("locked", c.n) }
        else { note = c.problem ?? (text.trimmingCharacters(in: .whitespaces).isEmpty ? "type an expression" : "not finished") }
    }

    /// Leaving mid-round (modes, esc, closing) counts as giving up.
    func leave() {
        timer?.invalidate()
        timer = nil
        if round.over == nil {
            let s = loadStats()
            saveStats(round.countdown ? numbersRecordCountdown(stats: s, points: 0) : numbersRecord24(stats: s, solved: false))
            round.over = ("gave up", nil, 0)
        }
    }
}

private struct NumbersGame: View {
    @StateObject private var m: NumbersModel
    @EnvironmentObject private var shell: GameShellModel
    @Environment(\.gameColors) private var colors
    @FocusState private var focused: Bool
    let onMenu: () -> Void

    init(countdown: Bool, large: Double, onMenu: @escaping () -> Void) {
        _m = StateObject(wrappedValue: NumbersModel(countdown: countdown, large: large))
        self.onMenu = onMenu
    }

    var body: some View {
        let r = m.round
        VStack(spacing: 0) {
            head(r)
            board(r)
            note(r)
        }
        .padding(.vertical, GameMetrics.sp3)
        .padding(.horizontal, GameMetrics.gutter)
        .onAppear { focus() }
        .onDisappear { m.leave() }
        .gameKeys(shell) { key in handle(key) }
    }

    // the input has the keys while a round is up
    private func focus() {
        for ms in [0, 40, 120, 280] {
            DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(ms)) { focused = true }
        }
    }

    private var fieldEditor: NSTextView? { NSApp.keyWindow?.firstResponder as? NSTextView }

    private func handle(_ key: GameKey) -> Bool {
        guard key.down, key.key != "Escape", key.plain else { return false }
        let r = m.round
        if key.key == "Enter" {
            if !key.isRepeat { if r.over != nil { m.start(); focus() } else { m.submit() } }
            return true
        }
        if key.key == "?" {
            if r.over == nil { m.finish("gave up", nil) }
            return true
        }
        if r.over != nil { return key.isCharacter || key.key == "Backspace" || key.key.hasPrefix("Arrow") }
        if let editor = fieldEditor, key.key == "Tab" || key.key == "ArrowRight" {
            let sel = editor.selectedRange()
            let caret = UInt32((editor.string as NSString).substring(to: sel.location).utf16.count)
            if let written = numbersAutofillParens(text: m.text, caret: caret, selectionEnd: caret + UInt32(sel.length)) {
                m.text = written
                DispatchQueue.main.async { editor.setSelectedRange(NSRange(location: (written as NSString).length, length: 0)) }
                return true
            }
            // tab never leaves the field
            if key.key == "Tab" { return true }
        }
        // typing into our own field: let it
        if fieldEditor != nil { return false }
        // focus wandered (a click): put the key where it belongs
        if key.isCharacter { m.text += key.key } else if key.key == "Backspace" { if !m.text.isEmpty { m.text.removeLast() } } else if !key.key.hasPrefix("Arrow") { return false }
        focused = true
        return true
    }

    private func head(_ r: NumbersRound) -> some View {
        let s = m.stats
        let sub = r.countdown
            ? "\(Int(s.points)) pts in \(Int(s.rounds)) · exact run \(Int(s.exactStreak)) · best \(Int(s.bestExact))"
            : "streak \(Int(s.streak24)) · best \(Int(s.best24))"
        return HStack(alignment: .firstTextBaseline, spacing: GameMetrics.sp3) {
            Text(r.countdown ? "countdown" : "24").font(.game(GameMetrics.fsBody, .semibold))
            Text(sub).font(.gameDigits(GameMetrics.fsSmall)).foregroundColor(colors.muted.color)
            Spacer(minLength: 0)
            HStack(spacing: GameMetrics.sp3 - GameMetrics.sp2) {
                EscButton(title: "modes") { onMenu() }
                EscButton(title: "esc leaves") { shell.escape() }.padding(.trailing, -GameMetrics.sp2)
            }
        }
    }

    private func board(_ r: NumbersRound) -> some View {
        let total = (r.countdown ? secondsCountdown : seconds24) * 1000
        let left = max(0, r.deadline - m.now)
        let taken = numbersUsedTiles(text: m.text, nums: r.deal.nums)
        let over = r.over
        return VStack(spacing: GameMetrics.sp2) {
            HStack(alignment: .firstTextBaseline, spacing: GameMetrics.sp2) {
                Text(r.countdown ? "target" : "make").font(.game(GameMetrics.fsSmall)).foregroundColor(colors.muted.color)
                Text(num(r.deal.target)).font(.gameDigits(30, .semibold)).foregroundColor(colors.accent.color)
            }
            HStack(spacing: GameMetrics.sp2) {
                ForEach(Array(r.deal.nums.enumerated()), id: \.offset) { i, n in
                    Button {
                        m.text = m.text.last?.isNumber == true ? "\(m.text) + \(num(n))" : "\(m.text)\(num(n))"
                        focused = true
                    } label: {
                        Text(num(n))
                            .font(.gameDigits(r.countdown ? 18 : 20, .semibold))
                            .foregroundColor(n >= 25 && r.countdown ? colors.accent.color : colors.ink.color)
                            .frame(width: r.countdown ? 46 : 52, height: 44)
                            .background(RoundedRectangle(cornerRadius: GameMetrics.radiusS).fill(colors.control.color))
                            .boxShadow(colors.controlShadow, radius: GameMetrics.radiusS)
                            .opacity(i < taken.count && taken[i] ? 0.3 : 1)
                    }
                    .buttonStyle(.plain)
                    .disabled(over != nil)
                }
            }
            GeometryReader { g in
                ZStack(alignment: .leading) {
                    RoundedRectangle(cornerRadius: 2).fill(colors.fillTrack.color)
                    Rectangle().fill((left < 10_000 ? bad : colors.accent).color)
                        .frame(width: over != nil ? 0 : g.size.width * left / total)
                        .animation(.linear(duration: 0.2), value: left)
                }
                .clipShape(RoundedRectangle(cornerRadius: 2))
            }
            .frame(width: 320, height: 3)
            entry(r, left: left)
            Text(over != nil ? " " : live.text.isEmpty ? " " : live.text)
                .font(.gameDigits(GameMetrics.fsBody))
                .foregroundColor(live.bad ? bad.color : colors.muted.color)
            if let over { result(r, over) }
        }
        .frame(maxWidth: .infinity)
        .padding(.top, GameMetrics.sp3)
    }

    private var live: (text: String, bad: Bool) {
        if !m.note.isEmpty { return (m.note, true) }
        let c = m.check
        if c.hasValue {
            // numbers still to use isn't wrong yet, just not done
            if let p = c.problem { return ("= \(c.valueText) · \(p)", !p.hasPrefix("use ")) }
            return ("= \(c.valueText)", false)
        }
        if let p = c.problem { return (p, true) }
        return ("", false)
    }

    private func entry(_ r: NumbersRound, left: Double) -> some View {
        let ghost = r.over == nil ? String(repeating: ")", count: Int(numbersTrailingParens(text: m.text))) : ""
        return ZStack(alignment: .leading) {
            TextField("", text: Binding(get: { m.text }, set: { m.text = $0.replacingOccurrences(of: "?", with: ""); m.note = "" }))
                .textFieldStyle(.plain)
                .font(.gameDigits(16))
                // .nums-input::placeholder, which the prompt's own colour never reached
                .mutedPrompt(r.countdown ? "e.g. 100 × 7 − 25" : "e.g. (8 − 2) × 4", shown: m.text.isEmpty, size: 16,
                             font: .gameDigits(16), color: colors.muted.opacity(0.6).color)
                .foregroundColor(colors.ink.color)
                .focused($focused)
                .disabled(r.over != nil)
                .noWritingTools()
                .accessibilityLabel("your expression")
                .padding(.leading, GameMetrics.sp3)
                .padding(.trailing, 44)
            if !ghost.isEmpty {
                // the faint closing parens after the typed text
                (Text(m.text).foregroundColor(.clear) + Text(ghost).foregroundColor(colors.muted.opacity(0.55).color))
                    .font(.gameDigits(16))
                    .lineLimit(1)
                    .padding(.leading, GameMetrics.sp3)
                    .allowsHitTesting(false)
            }
            Text(r.over != nil ? "" : "\(Int(ceil(left / 1000)))s")
                .font(.gameDigits(GameMetrics.fsSmall)).foregroundColor(colors.muted.color)
                .frame(maxWidth: .infinity, alignment: .trailing)
                .padding(.trailing, GameMetrics.sp3)
        }
        .frame(width: 320, height: 34)
        .background(RoundedRectangle(cornerRadius: GameMetrics.radiusS).fill(colors.bgTape.color))
        .insetRing(focused ? colors.accent.mix(0.55, .clear) : colors.line, width: 1, radius: GameMetrics.radiusS)
    }

    private func result(_ r: NumbersRound, _ over: (how: String, value: Double?, points: UInt32)) -> some View {
        let text: String
        if !r.countdown {
            text = over.how == "solved" ? "24! streak \(Int(m.stats.streak24))" : over.how == "time" ? "time’s up" : "gave up"
        } else {
            let off = over.value.map { abs(r.deal.target - $0) }
            let what: String
            if over.how == "gave up" { what = "gave up" }
            else if let v = over.value, let off {
                what = off == 0 ? "exactly \(num(r.deal.target))!" : "\(over.how == "time" ? "time’s up · " : "")\(num(v)), \(num(off)) away"
            } else { what = over.how == "time" ? "time’s up · nothing locked in" : "nothing" }
            text = "\(what) · \(over.points) pts"
        }
        let won = over.how == "solved" || over.points == 10
        let solution = r.countdown && r.deal.bestValue != r.deal.target
            ? "\(r.deal.solution) = \(num(r.deal.bestValue)) (closest possible)" : "\(r.deal.solution) = \(num(r.deal.target))"
        return VStack(spacing: 2) {
            Text(text).font(.gameDigits(GameMetrics.fsBody, .semibold)).foregroundColor(won ? colors.accent.color : colors.ink.color)
            if !won { Text(solution).font(.gameDigits(GameMetrics.fsBody)).foregroundColor(colors.muted.color) }
        }
        .multilineTextAlignment(.center)
    }

    private func note(_ r: NumbersRound) -> some View {
        HStack(spacing: 0) {
            Text(r.over != nil ? "↵ next · " : r.countdown ? "↵ lock in · " : "↵ check · ")
            if r.over == nil {
                PongLink(title: "? show a solution") { m.finish("gave up", nil) }
                Text(" · ")
            }
            Text("esc leaves")
        }
        .font(.gameDigits(GameMetrics.fsSmall))
        .foregroundColor(colors.muted.color)
        .frame(maxWidth: .infinity)
        .padding(.top, GameMetrics.sp2)
    }

    private func num(_ v: Double) -> String { v == v.rounded() && abs(v) < 1e15 ? String(Int(v)) : String(v) }
}
