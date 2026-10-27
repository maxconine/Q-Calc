import AppKit
import QCalcCore
import QuartzCore
import SwiftUI

// equal: `equal` then enter (components/EqualPanel.tsx and EqualPanel.css). guess the hidden 8-character equation
// in six. daily is one puzzle a day for everyone, saved with the streak; practice deals a fresh one each time.
// The rules are the core's (`equal_*`, from lib/equal.ts).

private let saveKey = "qcalc-equal"
private let tries = 6, len = 8
private let keyRows = ["1234567890", "+-*/="]

private func glyph(_ c: Character) -> String {
    switch c {
    case "*": return "×"
    case "/": return "÷"
    case "-": return "−"
    default: return String(c)
    }
}

private func todayKey() -> String {
    let c = Calendar.current.dateComponents([.year, .month, .day], from: Date())
    return String(format: "%04d-%02d-%02d", c.year ?? 2026, c.month ?? 1, c.day ?? 1)
}

struct EqualPanel: View {
    var body: some View {
        GameLobby(LobbyConfig(
            game: "equal", name: "equal", blurb: "guess the hidden equation in six tries.",
            transport: nil,
            local: LocalGames(prompt: "", options: ["daily", "practice"]) { i, _ in AnyView(EqualGame(start: i == 1)) },
            frame: true
        ))
    }
}

final class EqualModel: ObservableObject {
    @Published var practiceMode: Bool
    @Published var save = equalLoadSave(raw: GameSaves.load(saveKey))
    @Published var today = todayKey()
    @Published var practice = (answer: equalPractice(), guesses: [String]())
    @Published var input = ""
    @Published var note = ""
    // the current row shakes on a refused guess; the row just entered flips over
    @Published var shakeAt = -10.0
    @Published var fresh = -1
    @Published var freshAt = -10.0
    @Published var copied = false

    init(practice: Bool) { practiceMode = practice }

    var daily: Bool { !practiceMode }
    var answer: String { daily ? equalDailyAnswer(date: today) : practice.answer }
    var guesses: [String] { daily ? (save.daily?.date == today ? save.daily!.guesses : []) : practice.guesses }
    var over: Bool { guesses.contains(answer) || guesses.count >= tries }
    var won: Bool { guesses.contains(answer) }
    var title: String { daily ? "equal #\(equalDailyNumber(date: today))" : "equal practice" }

    func switchTo(practice p: Bool) {
        practiceMode = p
        input = ""
        note = ""
        fresh = -1
        copied = false
        // past midnight, daily moves on to the new day's puzzle
        if !p { today = todayKey() }
    }

    func nextPractice() {
        practice = (equalPractice(), [])
        input = ""
        note = ""
        fresh = -1
        copied = false
    }

    private func refuse(_ text: String) {
        note = text
        shakeAt = CACurrentMediaTime()
        settle()
    }

    // one more render once the shake or the flip is over, so the grid stops drawing every frame
    private func settle() {
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.3) { [weak self] in self?.objectWillChange.send() }
    }

    func submit() {
        if over { return }
        if let problem = equalGuessProblem(guess: input) { return refuse(problem) }
        let next = guesses + [input]
        fresh = guesses.count
        freshAt = CACurrentMediaTime()
        settle()
        input = ""
        note = ""
        if !daily {
            practice.guesses = next
            return
        }
        let done = next.contains(answer) || next.count >= tries
        let stats = done ? equalRecordDaily(stats: save.stats, date: today, guesses: next, answer: answer) : save.stats
        let saved = EqualSave(daily: EqualDaily(date: today, guesses: next), stats: stats)
        GameSaves.store(saveKey, equalSaveJson(save: saved))
        save = saved
    }

    func press(_ k: String) {
        if k == "Enter" {
            if over && !daily { nextPractice() } else { submit() }
        } else if over {
            if daily { note = "today’s is done · P for practice" }
        } else if k == "Backspace" {
            if !input.isEmpty { input.removeLast() }
            note = ""
        } else if input.count < len {
            input += k
        }
    }

    func copy() {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(equalShareText(guesses: guesses, answer: answer, title: title), forType: .string)
        copied = true
    }
}

private struct EqualGame: View {
    @StateObject private var m: EqualModel
    @EnvironmentObject private var shell: GameShellModel
    @Environment(\.gameColors) private var colors

    init(start practice: Bool) {
        _m = StateObject(wrappedValue: EqualModel(practice: practice))
    }

    private var hit: RGBA { colors.dark ? RGBA(0x3f9e64) : RGBA(0x2e8b57) }
    private var near: RGBA { colors.dark ? RGBA(0xc7a33a) : RGBA(0xc9a227) }
    private var miss: RGBA { colors.dark ? RGBA(0x545458) : RGBA(0x8e8e93) }
    private func fill(_ mk: EqualMark) -> RGBA { mk == .hit ? hit : mk == .near ? near : miss }

    var body: some View {
        let guesses = m.guesses
        let over = m.over
        GamePanel {
            VStack(spacing: 0) {
                head
                grid(guesses, over: over)
                    .padding(.top, GameMetrics.sp3)
                    .padding(.bottom, GameMetrics.sp2)
                if over { summary(guesses) } else { keypad(guesses) }
            }
        }
        // left open past midnight, daily moves on by itself once the board is empty or done
        .onReceive(Timer.publish(every: 30, on: .main, in: .common).autoconnect()) { _ in
            if m.daily && (m.guesses.isEmpty || m.over) { m.today = todayKey() }
        }
        .gameKeys(shell) { key in
            guard key.down, key.plain, key.key != "Escape" else { return false }
            let k = ["x", "X", "×"].contains(key.key) ? "*" : key.key == "÷" ? "/" : key.key
            if (k.count == 1 && "0123456789+-*/=".contains(k)) || k == "Backspace" || k == "Enter" {
                if !(k == "Enter" && key.isRepeat) { m.press(k) }
            } else if k == "d" || k == "D" {
                m.switchTo(practice: false)
            } else if k == "p" || k == "P" {
                m.switchTo(practice: true)
            } else if (k == "c" || k == "C") && m.over {
                // C copies and N deals again only once a game is over
                m.copy()
            } else if (k == "n" || k == "N") && m.over && !m.daily {
                m.nextPractice()
            } else if !key.isCharacter && !k.hasPrefix("Arrow") {
                return false
            }
            return true
        }
    }

    private var head: some View {
        let streak = equalCurrentStreak(stats: m.save.stats, today: m.today)
        return HStack(alignment: .firstTextBaseline, spacing: GameMetrics.sp3) {
            Text("equal").font(.game(GameMetrics.fsBody, .semibold))
            Text(m.daily ? "#\(equalDailyNumber(date: m.today)) · streak \(streak)" : "practice")
                .font(.game(GameMetrics.fsSmall)).foregroundColor(colors.muted.color)
            Spacer(minLength: 0)
            HStack(spacing: 0) {
                ForEach([false, true], id: \.self) { p in
                    let on = m.practiceMode == p
                    Button { m.switchTo(practice: p) } label: {
                        Text(p ? "practice" : "daily")
                            .font(.game(GameMetrics.fsCaption))
                            .foregroundColor(on ? colors.ink.color : colors.muted.color)
                            .padding(.vertical, 2).padding(.horizontal, GameMetrics.sp2)
                            .background(RoundedRectangle(cornerRadius: 4).fill(on ? colors.control.color : .clear))
                            .boxShadow(on ? colors.controlShadow : [], radius: 4)
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(2)
            .background(RoundedRectangle(cornerRadius: GameMetrics.radiusS).fill(colors.fillTrack.color))
            .alignmentGuide(.firstTextBaseline) { d in d[VerticalAlignment.center] + 4 }
            EscButton { shell.escape() }
                .padding(.leading, GameMetrics.sp2 - GameMetrics.sp3)
                .padding(.trailing, -GameMetrics.sp2)
        }
    }

    private func grid(_ guesses: [String], over: Bool) -> some View {
        TimelineView(.animation(minimumInterval: nil, paused: CACurrentMediaTime() - max(m.shakeAt, m.freshAt) > 1.2)) { _ in
            let now = CACurrentMediaTime()
            VStack(spacing: 5) {
                ForEach(0..<tries, id: \.self) { r in
                    let done = r < guesses.count
                    let current = r == guesses.count && !over
                    let text = Array(done ? guesses[r] : current ? m.input : "")
                    let marks = done ? equalScore(guess: guesses[r], answer: m.answer) : nil
                    HStack(spacing: 5) {
                        ForEach(0..<len, id: \.self) { i in
                            tile(i < text.count ? text[i] : nil, mark: marks?[i], flip: r == m.fresh && marks != nil ? max(0, min(1, (now - m.freshAt - Double(i) * 0.07) / 0.42)) : 1)
                        }
                    }
                    .offset(x: current ? shake(now - m.shakeAt) : 0)
                }
            }
            .frame(maxWidth: .infinity)
        }
    }

    // equal-shake: 0.36 s through ±6, ±4, 3 px
    private func shake(_ t: Double) -> CGFloat {
        let frames: [(Double, Double)] = [(0, 0), (0.2, -6), (0.4, 6), (0.6, -4), (0.8, 3), (1, 0)]
        let u = t / 0.36
        guard u >= 0, u < 1, !GameMotion.reduced else { return 0 }
        for i in 1..<frames.count where u <= frames[i].0 {
            let k = (u - frames[i - 1].0) / (frames[i].0 - frames[i - 1].0)
            return frames[i - 1].1 + (frames[i].1 - frames[i - 1].1) * cssEase(k, 0.25, 0.1, 0.25, 1)
        }
        return 0
    }

    private func tile(_ c: Character?, mark mk: EqualMark?, flip: Double) -> some View {
        let shape = RoundedRectangle(cornerRadius: GameMetrics.radiusS)
        // equal-flip: rotateX 90° → 0, ease, 0.42 s, held at 90° until its delay
        let angle = GameMotion.reduced ? 0 : 90 * (1 - cssEase(flip, 0.25, 0.1, 0.25, 1))
        return Text(c.map(glyph) ?? "")
            .font(.gameDigits(GameMetrics.fsRow, .semibold))
            .foregroundColor(mk != nil ? .white : colors.ink.color)
            .frame(width: 32, height: 34)
            .background(shape.fill(mk.map { fill($0).color } ?? .clear))
            .overlay {
                if mk == nil {
                    shape.strokeBorder(c != nil ? colors.ink.mix(0.4, .clear).color : colors.ink.mix(0.14, .clear).color, lineWidth: c != nil ? 1.5 : 1)
                }
            }
            .rotation3DEffect(.degrees(angle), axis: (x: 1, y: 0, z: 0), perspective: 0)
    }

    private func keypad(_ guesses: [String]) -> some View {
        let keys = equalKeyMarks(guesses: guesses, answer: m.answer)
        let left = tries - guesses.count
        return VStack(spacing: 0) {
            Text(m.note.isEmpty ? "\(left) \(left == 1 ? "guess" : "guesses") left" : m.note)
                .font(.game(GameMetrics.fsSmall)).foregroundColor(colors.muted.color)
                .frame(minHeight: 16)
                .padding(.bottom, GameMetrics.sp2)
                // .equal-note aria-live="polite"
                .liveRegion(m.note.isEmpty ? "\(left) \(left == 1 ? "guess" : "guesses") left" : m.note)
            VStack(spacing: 5) {
                ForEach(0..<keyRows.count, id: \.self) { r in
                    HStack(spacing: 5) {
                        ForEach(Array(keyRows[r]), id: \.self) { k in
                            EqualKey(label: glyph(k), mark: keys[String(k)], fill: keys[String(k)].map(fill)) { m.press(String(k)) }
                        }
                        if r == 1 {
                            EqualKey(label: "⌫", wide: true) { m.press("Backspace") }
                            EqualKey(label: "↵", wide: true) { m.press("Enter") }
                        }
                    }
                }
            }
            PongNote("green: right spot · yellow: elsewhere · grey: not in it · D daily · P practice · esc leaves")
                .padding(.top, GameMetrics.sp2)
        }
        .frame(maxWidth: .infinity)
    }

    private func summary(_ guesses: [String]) -> some View {
        let stats = m.save.stats
        let streak = equalCurrentStreak(stats: stats, today: m.today)
        let answerText = m.answer.map(glyph).joined()
        let pct = stats.played > 0 ? Int((Double(stats.wins) / Double(stats.played) * 100).rounded()) : 0
        return VStack(spacing: GameMetrics.sp2) {
            (Text(m.won ? "got it in \(guesses.count)" : "out of guesses").foregroundColor(m.won ? colors.accent.color : colors.ink.color)
                + Text(" · the answer was ").foregroundColor(m.won ? colors.accent.color : colors.ink.color)
                + Text(answerText).bold().foregroundColor(colors.ink.color))
                .font(.gameDigits(GameMetrics.fsBody))
                .multilineTextAlignment(.center)
            if m.daily {
                PongNote("streak \(streak) · best \(stats.best) · played \(stats.played) · \(pct)% won · next one tomorrow")
            }
            HStack(spacing: GameMetrics.sp3) {
                PongButton(m.copied ? "copied" : "copy result", kbd: "C") { m.copy() }
                if m.daily {
                    PongButton("practice", kbd: "P") { m.switchTo(practice: true) }
                } else {
                    PongButton("next puzzle", kbd: "↵") { m.nextPractice() }
                }
            }
            if !m.note.isEmpty { PongNote(m.note) }
        }
        .padding(.bottom, GameMetrics.sp1)
        .frame(maxWidth: .infinity)
    }
}

/// `.equal-key`: 30 high, the control fill; a marked key takes its colour (a miss at 70%).
private struct EqualKey: View {
    let label: String
    var wide = false
    var mark: EqualMark? = nil
    var fill: RGBA? = nil
    let action: () -> Void
    @Environment(\.gameColors) private var colors
    @State private var hover = false

    var body: some View {
        Button(action: action) {
            Text(label)
                .font(.game(GameMetrics.fsBody, .semibold))
                .foregroundColor(fill != nil ? .white : colors.ink.color)
                .padding(.horizontal, GameMetrics.sp1)
                .frame(minWidth: wide ? 46 : 30, minHeight: 30, maxHeight: 30)
                .background(RoundedRectangle(cornerRadius: GameMetrics.radiusS)
                    .fill((fill ?? (hover ? colors.control.mix(0.92, colors.ink) : colors.control)).color))
                .boxShadow(fill == nil ? colors.controlShadow : [], radius: GameMetrics.radiusS)
                .opacity(mark == .miss ? 0.7 : 1)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hover = $0 }
    }
}
