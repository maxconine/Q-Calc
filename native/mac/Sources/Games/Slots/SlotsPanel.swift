import QCalcCore
import QuartzCore
import SwiftUI

// slots: `slots` then enter. Native only (no web original): a three-reel, one-line machine played for free chips,
// kept as `qcalc-slots`. One player, no link: straight to the machine, with no lobby, and one esc (or the esc button)
// goes back to the bar; there is nothing to pause, as the reels never move on their own. The rules
// are the core's (`slots_*`, core/src/games/slots.rs); this draws the reels and takes the keys.
//
// The reels are Core Animation layers, not a canvas redrawn each frame: each reel is one strip image (drawn once,
// sharp and blurred), and a spin hands the render server its whole path as keyframes, so the app does no work per
// frame and none at all while the machine stands.

private let saveKey = "qcalc-slots"
private let strip = slotsStrip()
private let payTableRows = slotsPayTable()

private func loadGame() -> SlotsState { slotsLoad(raw: GameSaves.load(saveKey)) }

/// No lobby: the shell stays in its lobby mode, where esc has no screen to step back to and closes the game.
struct SlotsPanel: View {
    var body: some View { SlotsGame() }
}

/// The machine's state: the core's `SlotsState`, where each reel started its spin, and the last result. It
/// publishes only on a spin, a result and a bet, never per frame.
private final class SlotsModel: ObservableObject {
    @Published private(set) var s: SlotsState
    /// Bumped when the reels' paths change (a spin, a press), so the reels plan their animation again.
    private(set) var plan = 0
    @Published private(set) var last: SlotsEvent?
    private(set) var lastAt = CACurrentMediaTime()
    /// The chips shown count up from here to `s.chips` after a win.
    private(set) var chipsFrom: UInt32
    private(set) var chipsAt = 0.0
    private var timer: Timer?

    init() {
        let s = loadGame()
        self.s = s
        chipsFrom = s.chips
    }

    deinit { timer?.invalidate() }

    private func apply(_ step: SlotsStep) {
        let before = s.chips
        s = step.state
        if step.event == .stop { plan += 1 }
        guard let e = step.event, e != .spin, e != .stop else { return }
        timer?.invalidate()
        timer = nil
        last = e
        lastAt = CACurrentMediaTime()
        if s.chips > before, !GameMotion.reduced {
            chipsFrom = before
            chipsAt = lastAt
            // once the count is done, one more look so the counting timeline pauses
            DispatchQueue.main.asyncAfter(deadline: .now() + countDuration + 0.05) { [weak self] in self?.objectWillChange.send() }
        } else {
            chipsFrom = s.chips
        }
        GameSaves.store(saveKey, slotsSave(state: s))
        if e != .refill {
            BarModel.announce(GameSpeech.slotsLine(s.reels.map { strip[Int($0)] }, result: Self.resultText(e, win: s.win)))
        } else {
            BarModel.announce(Self.resultText(e, win: s.win))
        }
    }

    /// The result under the machine for the last event.
    static func resultText(_ e: SlotsEvent?, win: UInt32) -> String {
        switch e {
        case .big: return "jackpot +\(win)"
        case .win: return "+\(win)"
        case .lose: return "no win"
        case .refill: return "+100 free chips"
        default: return ""
        }
    }

    /// Space, ↵ or a click: spins, or while spinning stops the next reel.
    func spin() {
        if s.phase == .spinning {
            apply(slotsPress(state: s, now: GameTime.now()))
            return
        }
        last = nil
        let step = slotsSpin(state: s, now: GameTime.now())
        apply(step)
        guard step.event == .spin else { return }
        plan += 1
        chipsFrom = s.chips
        // the bet is taken now; saved as what the spin will pay, so closing mid-spin loses nothing
        GameSaves.store(saveKey, slotsSave(state: s))
        // the core pays once its clock passes the last stop; a light timer asks, rather than a frame loop
        let t = Timer(timeInterval: 0.03, repeats: true) { [weak self] _ in
            guard let self, self.s.phase == .spinning else { return }
            self.apply(slotsTick(state: self.s, now: GameTime.now()))
        }
        RunLoop.main.add(t, forMode: .common)
        timer = t
    }

    func bet(_ n: Int) {
        let next = slotsSetBet(state: s, bet: Int64(n))
        guard next != s else { return }
        s = next
        GameSaves.store(saveKey, slotsSave(state: s))
    }

    var countDuration: Double {
        let gain = Double(s.chips > chipsFrom ? s.chips - chipsFrom : 0)
        return min(1.4, 0.35 + 0.18 * log2(gain + 1))
    }

    var counting: Bool { chipsFrom < s.chips && CACurrentMediaTime() - chipsAt < countDuration }

    /// The chips as shown: a win counts up, quicker for small ones.
    func shownChips() -> UInt32 {
        guard counting else { return s.chips }
        let u = (CACurrentMediaTime() - chipsAt) / countDuration
        return chipsFrom + UInt32((Double(s.chips - chipsFrom) * cssEase(max(0, u), 0.25, 0.1, 0.25, 1)).rounded(.down))
    }

    /// What the reels draw from: changes only on a spin, a result or a theme change.
    func reels(_ colors: GameColors) -> ReelsInput {
        ReelsInput(state: s, plan: plan,
                   win: s.phase == .idle && (last == .win || last == .big), big: s.phase == .idle && last == .big,
                   colors: colors)
    }
}

private struct SlotsGame: View {
    @StateObject private var model = SlotsModel()
    @EnvironmentObject private var shell: GameShellModel
    @Environment(\.gameColors) private var colors
    @Environment(\.gameStage) private var stage

    var body: some View {
        GamePanel {
            VStack(spacing: GameMetrics.sp3) {
                // slots · chips, and the way out, on one line
                HStack(alignment: .firstTextBaseline, spacing: GameMetrics.sp3) {
                    Text("slots").font(.game(GameMetrics.fsBody, .semibold))
                    // only this text redraws while a win counts up, and nothing does otherwise
                    TimelineView(.animation(minimumInterval: 1 / 30, paused: !model.counting)) { _ in
                        Text("\(model.shownChips()) chips").font(.game(GameMetrics.fsBody)).monospacedDigit()
                            .foregroundColor(colors.muted.color)
                    }
                    Spacer(minLength: 0)
                    EscButton(title: "esc to leave") { shell.escape() }.padding(.trailing, -GameMetrics.sp2)
                }
                SlotsReels(input: model.reels(colors))
                    .frame(width: machineW, height: machineW * 0.52)
                    .accessibilityElement()
                    .accessibilityLabel("reels")
                    .accessibilityValue(accessibility)
                    .accessibilityAddTraits(.isButton)
                    .accessibilityHint(model.s.phase == .spinning ? "stops a reel" : "spins")
                    .accessibilityAction { model.spin() }
                    .contentShape(Rectangle())
                    .onTapGesture { model.spin() }
                    .frame(maxWidth: .infinity)
                controls
                    .frame(width: machineW)
                    .frame(maxWidth: .infinity)
                // what pays, small and quiet
                Text(payTableRows.map { "\($0.line) ×\($0.times)" }.joined(separator: "   "))
                    .font(.game(GameMetrics.fsCaption)).monospacedDigit()
                    .foregroundColor(colors.muted.color)
                    .lineLimit(1).minimumScaleFactor(0.8)
                    .frame(maxWidth: .infinity)
            }
        }
        .gameKeys(shell) { key in
            guard key.plain, key.key != "Escape" else { return false }
            if !key.down { return key.key == " " || key.key == "Enter" }
            switch key.key {
            case " ", "Enter":
                if !key.isRepeat { model.spin() }
            case "ArrowLeft", "ArrowDown", "-":
                model.bet(Int(model.s.bet) - 1)
            case "ArrowRight", "ArrowUp", "+", "=":
                model.bet(Int(model.s.bet) + 1)
            case "1", "2", "3", "4", "5", "6", "7", "8", "9":
                model.bet(Int(key.key)!)
            case "0":
                model.bet(10)
            default:
                if !key.isCharacter && key.key != "Backspace" { return false }
            }
            return true
        }
    }

    private var machineW: CGFloat {
        guard let stage else { return 300 }
        return max(260, min(420, stage.width - 2 * GameMetrics.gutter, (stage.gameH - 200) / 0.52))
    }

    /// Under the machine: the bet on the left; on the right what the last spin did, or before any, what pays.
    private var controls: some View {
        let s = model.s
        let idle = s.phase == .idle
        return HStack(spacing: GameMetrics.sp2) {
            Text("bet").foregroundColor(colors.muted.color)
            Button("−") { model.bet(Int(s.bet) - 1) }
                .buttonStyle(.plain).disabled(!idle || s.bet <= 1).accessibilityLabel("lower the bet")
            Text("\(s.bet)").font(.game(GameMetrics.fsRow, .semibold)).monospacedDigit().frame(minWidth: 22)
            Button("+") { model.bet(Int(s.bet) + 1) }
                .buttonStyle(.plain).disabled(!idle || s.bet >= 10).accessibilityLabel("raise the bet")
            Spacer(minLength: GameMetrics.sp3)
            if result.isEmpty {
                Text(hint).foregroundColor(colors.muted.color)
            } else {
                Text(result).font(.game(GameMetrics.fsBody, .semibold)).foregroundColor(resultColor.color).monospacedDigit()
            }
        }
        .font(.game(GameMetrics.fsBody))
    }

    /// Before a result: how to play, or out of chips.
    private var hint: String {
        let s = model.s
        if s.phase == .spinning { return "space stops a reel" }
        if s.chips == 0 { return "space for 100 free chips" }
        return "space or click to spin"
    }

    private var result: String {
        let s = model.s
        guard s.phase == .idle else { return "" }
        return SlotsModel.resultText(model.last, win: s.win)
    }

    private var resultColor: RGBA {
        switch model.last {
        case .big, .win, .refill: return colors.accent
        default: return colors.muted
        }
    }

    private var accessibility: String {
        let s = model.s
        if s.phase == .spinning { return "spinning" }
        return GameSpeech.slotsLine(s.reels.map { strip[Int($0)] }, result: result)
    }
}

// MARK: - the reels

struct ReelsInput: Equatable {
    var state: SlotsState
    /// Bumped on each spin and press: the reels plan their paths again.
    var plan: Int
    var win: Bool
    var big: Bool
    var colors: GameColors

    static func == (a: ReelsInput, b: ReelsInput) -> Bool {
        a.plan == b.plan && a.win == b.win && a.big == b.big && a.colors == b.colors && a.state.reels == b.state.reels
    }
}

private struct SlotsReels: NSViewRepresentable {
    let input: ReelsInput

    func makeNSView(context: Context) -> ReelsView { ReelsView() }

    func updateNSView(_ view: ReelsView, context: Context) { view.show(input) }
}

/// The machine in layers. Built again only when its size or colours change; a spin adds keyframe animations.
final class ReelsView: NSView {
    private var input: ReelsInput?
    private var builtFor: (CGSize, GameColors)?
    private var shownPlan: Int?

    private let body = CALayer()
    private var windows: [CALayer] = []
    private var sharp: [CALayer] = []
    private var blurred: [CALayer] = []
    private var dims: [CALayer] = []
    private let payLine = CAShapeLayer()

    // the strip images carry this many stops past each end, so a window never shows past them (blur included)
    private static let pad = 3
    private var cell: CGFloat = 0

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.addSublayer(body)
        layer?.addSublayer(payLine)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    // the gestures on the SwiftUI side get every event
    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func layout() {
        super.layout()
        if let input { show(input) }
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        builtFor = nil
        if let input { show(input) }
    }

    func show(_ next: ReelsInput) {
        let old = input
        input = next
        guard bounds.width > 4 else { return }
        if builtFor == nil || builtFor!.0 != bounds.size || builtFor!.1 != next.colors {
            build(next.colors)
            builtFor = (bounds.size, next.colors)
            shownPlan = next.plan
            place(next, animated: false)
            // resized, full screen or a new theme mid-spin: the new layers roll on from where the reels are
            let settle = (0..<3).map { slotsSettledAt(state: next.state, reel: UInt32($0)) }
            if GameSpeech.slotsKeepRolling(settledAt: settle, now: GameTime.now()) { place(next, animated: true) }
            light(next, animated: false)
        } else {
            // a spin or a press plans the reels again; the spin's end needs nothing, as the animations finish where
            // the model is
            if next.plan != shownPlan { place(next, animated: true) }
            if let old, old.win != next.win || old.big != next.big { light(next, animated: true) }
        }
    }

    // MARK: building

    private func build(_ colors: GameColors) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        windows.forEach { $0.removeFromSuperlayer() }
        windows = []; sharp = []; blurred = []; dims = []
        let scale = window?.backingScaleFactor ?? 2
        let size = bounds.size
        let pad: CGFloat = 10, gap: CGFloat = 8

        body.frame = bounds
        body.cornerRadius = 14
        body.backgroundColor = colors.bgTape.cg
        body.borderColor = colors.line.cg
        body.borderWidth = 1

        let reelW = (size.width - 2 * pad - 2 * gap) / 3
        let reelH = size.height - 2 * pad
        cell = reelH / 3
        let images = Self.stripImages(width: reelW, cell: cell, scale: scale, colors: colors)
        let stripH = cell * CGFloat(strip.count + 2 * Self.pad)

        for i in 0..<3 {
            let w = CALayer()
            w.frame = CGRect(x: pad + CGFloat(i) * (reelW + gap), y: pad, width: reelW, height: reelH)
            w.cornerRadius = 8
            w.masksToBounds = true
            w.backgroundColor = colors.bg.cg
            w.borderColor = colors.line.cg
            w.borderWidth = 1
            for (img, list) in [(images.sharp, 0), (images.blurred, 1)] {
                let l = CALayer()
                l.contents = img
                l.contentsScale = scale
                l.contentsGravity = .resize
                l.anchorPoint = CGPoint(x: 0.5, y: 0)
                l.bounds = CGRect(x: 0, y: 0, width: reelW, height: stripH)
                l.position = CGPoint(x: reelW / 2, y: 0)
                if list == 1 { l.opacity = 0 }
                w.addSublayer(l)
                if list == 0 { sharp.append(l) } else { blurred.append(l) }
            }
            // the rows above and below the line shaded, like the curve of a drum
            let shade = CAGradientLayer()
            shade.frame = w.bounds
            // the background at falling alpha (not toward clear black, which a gradient would draw as grey bands)
            let edge = colors.bg.cg, clear = colors.bg.cg.copy(alpha: 0)!, half = colors.bg.cg.copy(alpha: 0.5)!
            shade.colors = [edge, half, clear, clear, half, edge]
            shade.locations = [0, 0.3, 0.36, 0.64, 0.7, 1]
            w.addSublayer(shade)
            // the whole reel dimmed while it rolls, lifted as it lands
            let dim = CALayer()
            dim.frame = w.bounds
            dim.backgroundColor = colors.bg.mix(0.45, .clear).cg
            dim.opacity = 0
            w.addSublayer(dim)
            dims.append(dim)
            body.addSublayer(w)
            windows.append(w)
        }

        let path = CGMutablePath()
        let y = pad + reelH / 2
        path.move(to: CGPoint(x: pad - 6, y: y))
        path.addLine(to: CGPoint(x: size.width - pad + 6, y: y))
        payLine.frame = bounds
        payLine.path = path
        payLine.lineCap = .round
        payLine.fillColor = nil
    }

    /// One reel's stops drawn top to bottom as a tall image, highest stop at the top so a rising position rolls the
    /// symbols down; and the same smeared along the reel, for speed.
    private static func stripImages(width: CGFloat, cell: CGFloat, scale: CGFloat, colors: GameColors) -> (sharp: CGImage?, blurred: CGImage?) {
        let n = strip.count, rows = n + 2 * pad
        let font = NSFont.systemFont(ofSize: cell * 0.58, weight: .semibold)
        let seven = colors.dark ? RGBA(0xff6b5c) : RGBA(0xd93a2b)
        let gold = colors.dark ? RGBA(0xffd23f) : RGBA(0xe9a800)
        func color(_ sym: String) -> RGBA {
            switch sym {
            case "7": return seven
            case "π": return colors.accent
            case "e": return gold
            default: return colors.ink
            }
        }
        func render(ghosts: Int) -> CGImage? {
            let w = Int((width * scale).rounded(.up)), h = Int((cell * CGFloat(rows) * scale).rounded(.up))
            guard let cg = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: 0,
                                     space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                     bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue)
            else { return nil }
            cg.scaleBy(x: scale, y: scale)
            NSGraphicsContext.saveGraphicsState()
            NSGraphicsContext.current = NSGraphicsContext(cgContext: cg, flipped: false)
            for r in 0..<rows {
                let sym = strip[((r - pad) % n + n) % n]
                if sym.isEmpty { continue }
                // y up: row r's centre
                let y = (CGFloat(r) + 0.5) * cell
                for g in 0..<ghosts {
                    let k = ghosts == 1 ? 0 : CGFloat(g) / CGFloat(ghosts - 1) - 0.5
                    let a = ghosts == 1 ? 1 : 2.2 / Double(ghosts)
                    let str = NSAttributedString(string: sym, attributes: [.font: font, .foregroundColor: NSColor(cgColor: color(sym).mix(min(1, a), .clear).cg)!])
                    let sz = str.size()
                    str.draw(at: CGPoint(x: width / 2 - sz.width / 2, y: y + k * cell * 0.9 - sz.height / 2))
                }
            }
            NSGraphicsContext.restoreGraphicsState()
            return cg.makeImage()
        }
        return (render(ghosts: 1), render(ghosts: 9))
    }

    // MARK: moving

    /// The strip's y (its bottom, y up) that puts position p on the line.
    private func y(_ p: Double, _ i: Int) -> CGFloat {
        let n = Double(strip.count)
        let wrapped = (p.truncatingRemainder(dividingBy: n) + n).truncatingRemainder(dividingBy: n)
        return windows[i].bounds.height / 2 - (CGFloat(wrapped) + CGFloat(Self.pad) + 0.5) * cell
    }

    private func place(_ input: ReelsInput, animated: Bool) {
        guard windows.count == 3 else { return }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        let st = input.state
        if !animated {
            for i in 0..<3 {
                for l in [sharp[i], blurred[i]] {
                    l.removeAllAnimations()
                    l.position.y = y(Double(st.reels[i]), i)
                }
                sharp[i].opacity = 1
                blurred[i].opacity = 0
                dims[i].removeAllAnimations()
                dims[i].opacity = 0
            }
            return
        }
        shownPlan = input.plan
        // the core's game clock (ms) against Core Animation's (s): now in both
        let gnow = GameTime.now(), now = CACurrentMediaTime()
        let reduced = GameMotion.reduced
        // sampled at 240 a second and held between samples (discrete), which wraps the strip without sweeping it
        let step = 1000.0 / 240
        for i in 0..<3 {
            let end = slotsSettledAt(state: st, reel: UInt32(i))
            let landed = y(Double(st.reels[i]), i)
            guard end > gnow else { continue }
            let landIn = max(0, (st.land[i] - gnow) / 1000)
            let ys: [CGFloat], blur: [Float], times: [NSNumber], total: Double
            if reduced {
                // the reel stands, then shows where it landed
                let here = y(slotsPath(state: st, reel: UInt32(i), start: gnow, step: 1, count: 1)[0], i)
                total = max(0.001, landIn)
                ys = [here, landed]; blur = [0, 0]; times = [0, 1]
            } else {
                let count = max(2, Int(((end - gnow) / step).rounded(.up)) + 1)
                let path = slotsPath(state: st, reel: UInt32(i), start: gnow, step: step, count: UInt32(count))
                total = Double(count - 1) * step / 1000
                ys = path.map { y($0, i) }
                // blur by stops moved per 60th of a second
                blur = path.indices.map { k in
                    let d = k == 0 ? abs(path[1] - path[0]) : abs(path[k] - path[k - 1])
                    return Float(min(1, max(0, (d * 4 - 0.15) / 0.3)))
                }
                times = path.indices.map { NSNumber(value: Double($0) / Double(count - 1)) }
            }
            func keyframes(_ key: String, _ values: [Any]) -> CAKeyframeAnimation {
                let a = CAKeyframeAnimation(keyPath: key)
                a.values = values
                a.keyTimes = times
                a.calculationMode = .discrete
                a.duration = total
                a.beginTime = now
                return a
            }
            for l in [sharp[i], blurred[i]] {
                l.removeAllAnimations()
                l.position.y = landed
                l.add(keyframes("position.y", ys), forKey: "spin")
            }
            blurred[i].opacity = 0
            blurred[i].add(keyframes("opacity", blur), forKey: "fade")
            sharp[i].opacity = 1
            sharp[i].add(keyframes("opacity", blur.map { 1 - 0.85 * $0 }), forKey: "fade")
            // dimmed while it rolls; lifted over 180 ms as it lands
            dims[i].removeAllAnimations()
            dims[i].opacity = 0
            if st.land[i] > gnow {
                let lift = CABasicAnimation(keyPath: "opacity")
                lift.fromValue = 1
                lift.toValue = 0
                lift.beginTime = now + landIn
                lift.duration = reduced ? 0.001 : 0.18
                lift.fillMode = .backwards
                dims[i].add(lift, forKey: "land")
            }
        }
    }

    /// The pay line: muted, lit on a win (gold, pulsing, for a big one).
    private func light(_ input: ReelsInput, animated: Bool) {
        let c = input.colors
        let gold = c.dark ? RGBA(0xffd23f) : RGBA(0xe9a800)
        CATransaction.begin()
        CATransaction.setDisableActions(!animated || GameMotion.reduced)
        CATransaction.setAnimationDuration(0.25)
        payLine.strokeColor = (input.win ? (input.big ? gold : c.accent) : c.muted.mix(0.5, .clear)).cg
        payLine.lineWidth = input.win ? 3 : 1.5
        CATransaction.commit()
        payLine.removeAnimation(forKey: "pulse")
        if input.big, animated, !GameMotion.reduced {
            let pulse = CABasicAnimation(keyPath: "opacity")
            pulse.fromValue = 1
            pulse.toValue = 0.4
            pulse.duration = 0.31
            pulse.autoreverses = true
            pulse.repeatCount = 4
            pulse.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
            payLine.add(pulse, forKey: "pulse")
        }
    }
}
