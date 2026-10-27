import QCalcCore
import QuartzCore
import SwiftUI

// crack: `crack` then enter (components/CrackPanel.tsx and CrackPanel.css). one player, no link: the shared
// lobby, then the dial; esc pauses, then leaves. The rules are the core's (`crack_*`, from lib/crack.ts).

private let bestKey = "qcalc-crack"

private func loadBest() -> Int { Int(crackParseBest(raw: GameSaves.load(bestKey))) }

private func saveBest(_ level: Int) {
    if level > loadBest() { GameSaves.store(bestKey, String(level)) }
}

struct CrackPanel: View {
    @EnvironmentObject private var shell: GameShellModel

    var body: some View {
        GameLobby(LobbyConfig(
            game: "crack", name: "crack", blurb: "tap as the needle crosses the dot. each level spins faster.",
            transport: nil,
            local: LocalGames(prompt: "", options: ["play"]) { _, _ in AnyView(CrackGame()) },
            sub: "best level \(loadBest())",
            frame: true
        ))
    }
}

/// The dial's state between frames: the core's `CrackState` and the last popped dot.
private final class CrackModel: ObservableObject {
    var s: CrackState
    @Published var best: Int
    // the last popped dot and when, so each pop plays its animation
    var pop: (angle: Double, at: Double)?
    // when the phase last changed, in real time, for the shake, the click and the colour fades
    var phaseAt = CACurrentMediaTime()
    var lastPhase: CrackPhase
    // the mouse is down on the dial (a press is on pointer-down, once)
    var pressing = false

    init() {
        let best = loadBest()
        self.best = best
        s = crackStart(level: UInt32(best), now: GameTime.now(), angle: 0, dir: 1)
        lastPhase = s.phase
    }

    func apply(_ step: CrackStep) {
        let prev = s
        s = step.state
        if step.event == .hit || step.event == .clear { pop = (prev.dot, CACurrentMediaTime()) }
        if s.level > prev.level {
            saveBest(Int(s.level))
            if Int(s.level) > best { best = Int(s.level) }
        }
        if s.phase != lastPhase {
            lastPhase = s.phase
            phaseAt = CACurrentMediaTime()
        }
    }

    func frame() {
        if s.phase == .ready { return }
        apply(crackTick(state: s, now: GameTime.now()))
    }

    func hit() { apply(crackPress(state: s, now: GameTime.now())) }

    func restart() {
        pop = nil
        apply(CrackStep(state: crackStart(level: 1, now: GameTime.now(), angle: 0, dir: 1), event: nil))
    }
}

private struct CrackGame: View {
    @StateObject private var model = CrackModel()
    @EnvironmentObject private var shell: GameShellModel
    @Environment(\.gameColors) private var colors
    @Environment(\.gameStage) private var stage

    var body: some View {
        GamePanel {
            VStack(spacing: 0) {
                HStack(alignment: .firstTextBaseline, spacing: GameMetrics.sp3) {
                    Text("crack").font(.game(GameMetrics.fsBody, .semibold))
                    Text("best level \(model.best)").font(.game(GameMetrics.fsSmall)).foregroundColor(colors.muted.color)
                    Spacer(minLength: 0)
                    EscButton { shell.escape() }.padding(.trailing, -GameMetrics.sp2)
                }
                TimelineView(.animation) { _ in
                    let _ = model.frame()
                    VStack(spacing: 0) {
                        CrackDial(s: model.s, pop: model.pop, phaseAt: model.phaseAt, colors: colors)
                            .frame(width: dialSize, height: dialSize)
                            .contentShape(Rectangle())
                            .gesture(DragGesture(minimumDistance: 0)
                                .onChanged { _ in
                                    if !model.pressing { model.pressing = true; model.hit() }
                                }
                                .onEnded { _ in model.pressing = false })
                            .padding(.top, GameMetrics.sp2)
                            .padding(.bottom, GameMetrics.sp1)
                            .frame(maxWidth: .infinity)
                        PongNote(note)
                    }
                }
            }
        }
        .gameKeys(shell) { key in
            guard key.plain, key.key != "Escape" else { return false }
            if !key.down { return key.key == " " || key.key == "Enter" }
            if key.key == " " || key.key == "Enter" {
                if !key.isRepeat { model.hit() }
            } else if key.key == "1" && !key.isRepeat {
                // back to the first level; the best stays
                model.restart()
            } else if !key.isCharacter && key.key != "Backspace" {
                return false
            }
            return true
        }
    }

    private var dialSize: CGFloat {
        guard let stage else { return 200 }
        return max(200, min(stage.width - 2 * GameMetrics.gutter, stage.gameH - 160))
    }

    private var note: String {
        let s = model.s
        let restart = s.level > 1 ? " · 1 back to level 1" : ""
        return s.phase == .ready
            ? "space, ↵ or a click starts the needle\(restart) · esc leaves"
            : "press as the needle crosses the dot\(restart) · esc leaves"
    }
}

/// The dial in its 200-unit box (the svg of CrackPanel.tsx), drawn with its CSS animations: the shake on a miss,
/// the click-open on a clear, the dot's pop.
private struct CrackDial: View {
    let s: CrackState
    let pop: (angle: Double, at: Double)?
    let phaseAt: Double
    let colors: GameColors

    private static let c = 100.0, trackR = 76.0, tickR = 58.0
    private static let dotR = (76 * crackHitWindow() * 0.85).rounded()

    var dotColor: RGBA { colors.dark ? RGBA(0xffd23f) : RGBA(0xe9a800) }
    var missColor: RGBA { colors.dark ? RGBA(0xff6b5c) : RGBA(0xd93a2b) }

    var body: some View {
        Canvas { ctx, size in
            let now = CACurrentMediaTime()
            let since = now - phaseAt
            let k = size.width / 200
            ctx.scaleBy(x: k, y: k)
            // a miss shakes the whole dial
            if s.phase == .fail { ctx.translateBy(x: Self.shake(since), y: 0) }
            let p = { (a: Double, r: Double) in CGPoint(x: Self.c + r * sin(a), y: Self.c - r * cos(a)) }
            let circle = { (r: Double) in Path(ellipseIn: CGRect(x: Self.c - r, y: Self.c - r, width: 2 * r, height: 2 * r)) }

            let bezel = circle(Self.trackR + 18)
            ctx.fill(bezel, with: .color(colors.court.color))
            ctx.stroke(bezel, with: .color(colors.line.color), lineWidth: 1)

            // the track's colour fades over 0.2 s into the phase's
            let base = colors.ink.mix(0.11, .clear)
            let lit: RGBA = s.phase == .fail ? missColor.mix(0.45, .clear) : s.phase == .clear ? colors.accent.mix(0.55, .clear) : base
            let fade = min(1, since / 0.2)
            ctx.stroke(circle(Self.trackR), with: .color(lit.mix(fade, base).color), lineWidth: 26)

            // the tick ring; a clear turns it a click, like a tumbler giving
            var ticks = ctx
            if s.phase == .clear {
                let turn = 27 * Self.clickEase(min(1, since / 0.5))
                ticks.translateBy(x: Self.c, y: Self.c)
                ticks.rotate(by: .degrees(turn))
                ticks.translateBy(x: -Self.c, y: -Self.c)
            }
            for i in 0..<40 {
                let a = Double(i) / 40 * 2 * .pi
                let major = i % 5 == 0
                var line = Path()
                line.move(to: p(a, Self.tickR))
                line.addLine(to: p(a, Self.tickR - (major ? 7 : 3.5)))
                ticks.stroke(line, with: .color(colors.muted.opacity(major ? 0.9 : 0.55).color),
                             style: StrokeStyle(lineWidth: major ? 1.6 : 1, lineCap: .round))
            }

            if s.phase != .clear {
                let d = p(s.dot, Self.trackR)
                ctx.fill(Path(ellipseIn: CGRect(x: d.x - Self.dotR, y: d.y - Self.dotR, width: 2 * Self.dotR, height: 2 * Self.dotR)),
                         with: .color(dotColor.color))
            }
            if let pop {
                let t = (now - pop.at) / 0.35
                if t < 1 {
                    let eased = Self.easeOut(t)
                    let r = Self.dotR * (1 + 1.2 * eased)
                    let d = p(pop.angle, Self.trackR)
                    ctx.stroke(Path(ellipseIn: CGRect(x: d.x - r, y: d.y - r, width: 2 * r, height: 2 * r)),
                               with: .color(dotColor.opacity(1 - eased).color), lineWidth: 3 * (1 + 1.2 * eased))
                }
            }

            let needleColor: RGBA = s.phase == .fail ? missColor : s.phase == .clear ? colors.accent : colors.ink
            var needle = Path()
            needle.move(to: p(s.angle, Self.trackR - 13))
            needle.addLine(to: p(s.angle, Self.trackR + 13))
            ctx.stroke(needle, with: .color(needleColor.color), style: StrokeStyle(lineWidth: 5, lineCap: .round))

            let big = s.phase == .clear ? "open" : "\(s.level)"
            let small = s.phase == .clear ? "level \(s.level) cracked" : s.phase == .fail ? "missed" : "\(s.left) to go"
            ctx.draw(Text(big).font(.gameDigits(30, .semibold)).foregroundColor((s.phase == .clear ? colors.accent : colors.ink).color),
                     at: CGPoint(x: Self.c, y: Self.c + 4), anchor: UnitPoint(x: 0.5, y: Self.baseline(30, .semibold)))
            ctx.draw(Text(small).font(.game(11)).foregroundColor((s.phase == .fail ? missColor : colors.muted).color),
                     at: CGPoint(x: Self.c, y: Self.c + 24), anchor: UnitPoint(x: 0.5, y: Self.baseline(11, .regular)))
        }
    }

    // svg text sits on its baseline at y: the share of the line box above the baseline
    private static func baseline(_ size: CGFloat, _ weight: NSFont.Weight) -> CGFloat {
        let f = NSFont.systemFont(ofSize: size, weight: weight)
        let lineH = f.ascender - f.descender + f.leading
        return f.ascender / lineH
    }

    // crack-shake: 0.4 s, ease-in-out between the keyframes
    private static func shake(_ t: Double) -> Double {
        let frames: [(Double, Double)] = [(0, 0), (0.15, -7), (0.35, 6), (0.55, -4), (0.75, 2), (1, 0)]
        let u = t / 0.4
        guard u < 1 else { return 0 }
        for i in 1..<frames.count where u <= frames[i].0 {
            let (t0, v0) = frames[i - 1], (t1, v1) = frames[i]
            let k = (u - t0) / (t1 - t0)
            return v0 + (v1 - v0) * cssEase(k, 0.42, 0, 0.58, 1)
        }
        return 0
    }

    // crack-click's cubic-bezier(0.3, 1.6, 0.5, 1), which overshoots and settles
    private static func clickEase(_ t: Double) -> Double { cssEase(t, 0.3, 1.6, 0.5, 1) }

    // ease-out: cubic-bezier(0, 0, 0.58, 1)
    static func easeOut(_ t: Double) -> Double { cssEase(t, 0, 0, 0.58, 1) }
}

/// A CSS cubic-bezier timing function at progress t.
func cssEase(_ t: Double, _ x1: Double, _ y1: Double, _ x2: Double, _ y2: Double) -> Double {
    if t <= 0 { return 0 }
    if t >= 1 { return 1 }
    let bx = { (s: Double) in 3 * x1 * s * (1 - s) * (1 - s) + 3 * x2 * s * s * (1 - s) + s * s * s }
    let by = { (s: Double) in 3 * y1 * s * (1 - s) * (1 - s) + 3 * y2 * s * s * (1 - s) + s * s * s }
    var lo = 0.0, hi = 1.0, s = t
    for _ in 0..<30 {
        s = (lo + hi) / 2
        if bx(s) < t { lo = s } else { hi = s }
    }
    return by(s)
}
