import QCalcCore
import SwiftUI

// trails: `trails` then enter (components/TrailsPanel.tsx, TrailsPanel.css). The shared lobby picks the computer,
// two players on one keyboard, or a room online; the core's TrailsMatch runs the match (rounds, the computer, the
// lockstep link) and the arena below draws it as the page's canvas did.

private let TCOLS = 64.0
private let TROWS = 26.0
private let BURST_MS = 900.0
private let HOT_CELLS = 14

struct TrailsPanel: View {
    var body: some View {
        GamePanel {
            GameLobby(LobbyConfig(
                game: "trails", name: "trails", blurb: "leave a trail, don’t hit one. the last one moving takes the round.",
                transport: RelayTransport.shared,
                online: { p in AnyView(TrailsArenaView(props: p, vs: nil, onEnd: p.onEnd)) },
                local: LocalGames(prompt: "play the computer", options: ["easy", "normal", "hard"], together: ["two players (same keyboard)"]) { i, onEnd in
                    AnyView(TrailsPlay(vs: i, onEnd: onEnd))
                },
                sub: "first to 3 rounds",
                footer: AnyView(PongNote("vs the computer or online, steer with WASD or the arrows · two players: WASD and arrows"))
            ))
        }
    }
}

private struct TrailsPlay: View {
    let vs: Int
    let onEnd: (String?) -> Void
    var body: some View {
        VStack(spacing: GameMetrics.sp2) {
            LobbyHead(name: "trails", sub: vs == 3 ? "one keyboard" : "vs the computer · \(["easy", "normal", "hard"][vs])")
            TrailsArenaView(props: nil, vs: vs, onEnd: onEnd)
        }
    }
}

final class TrailsModel: ObservableObject {
    @Published private(set) var frame: TrailsFrame?
    let match: TrailsMatch
    let online: Bool
    private let transport: PeerTransport?
    private let onEnd: (String?) -> Void
    private var off: (() -> Void)?
    private var done = false
    private var shown = ""
    private var timer: Timer?
    private var lastFrame = 0.0

    init(props: GameProps?, vs: Int?, onEnd: @escaping (String?) -> Void) {
        online = props != nil
        transport = props?.transport
        self.onEnd = onEnd
        if let props {
            match = TrailsMatch.online(host: props.role == .host, peer: props.peer, now: CACurrentMediaTime() * 1000)
        } else {
            match = TrailsMatch.local(vs: UInt32(vs ?? 0), now: GameTime.now(), seed: UInt64.random(in: 1...UInt64.max))
        }
        off = transport?.subscribe { [weak self] e in
            guard let self else { return }
            switch e {
            case let .message(data): self.match.receive(text: data, now: self.clock())
            case let .closed(reason): self.finish(peerCloseText(reason: reason, word: self.transport?.peerWord ?? "friend"))
            default: break
            }
        }
        if online {
            // a hidden window gets no frames: a slow timer keeps the link warm, as the page's interval did
            let t = Timer(timeInterval: 0.25, repeats: true) { [weak self] _ in
                guard let self else { return }
                if self.clock() - self.lastFrame > 200 { _ = self.pump() }
            }
            RunLoop.main.add(t, forMode: .common)
            timer = t
        }
    }

    deinit { stop() }

    /// online games run on performance.now(), the others on the game clock (which pauses)
    func clock() -> Double { online ? CACurrentMediaTime() * 1000 : GameTime.now() }

    func stop() {
        timer?.invalidate()
        timer = nil
        off?()
        off = nil
        if !done {
            done = true
            match.leave()
            for t in match.takeOutbox() { transport?.send(t) }
        }
    }

    private func finish(_ note: String?) {
        guard !done else { return }
        done = true
        for t in match.takeOutbox() { transport?.send(t) }
        timer?.invalidate()
        off?()
        off = nil
        onEnd(note)
    }

    func pump() -> TrailsFrame? {
        guard !done, let f = match.tick(now: clock()) else { return nil }
        for t in match.takeOutbox() { transport?.send(t) }
        if let ended = f.ended {
            finish(ended.isEmpty ? nil : ended)
            return nil
        }
        if f.toMenu {
            finish(nil)
            return nil
        }
        let key = "\(f.score)|\(f.banner)|\(f.big)|\(f.names)|\(f.note)"
        if key != shown {
            shown = key
            frame = f
        }
        return f
    }

    private var current: TrailsFrame?

    func tick() {
        lastFrame = clock()
        current = pump()
    }

    func paint(_ g: Canvas2D, colors: GameColors) {
        guard let f = current else { return }
        trailsDraw(g, f, colors: colors, now: clock())
    }

    func key(_ k: GameKey) -> Bool {
        guard k.down, k.plain, k.key != "Escape" else { return false }
        let bar = k.isCharacter || k.key == "Enter" || k.key.hasPrefix("Arrow") || k.key == "Backspace"
        if !bar { return false }
        let key = k.isCharacter ? k.key.lowercased() : k.key
        let wasd: [String: UInt8] = ["w": 0, "d": 1, "s": 2, "a": 3]
        let arrows: [String: UInt8] = ["ArrowUp": 0, "ArrowRight": 1, "ArrowDown": 2, "ArrowLeft": 3]
        let now = clock()
        if key == "Backspace" {
            match.key(key: .back, repeat: k.isRepeat, now: now)
        } else if key == "Enter" {
            match.key(key: .enter, repeat: k.isRepeat, now: now)
        } else if let d = wasd[key] {
            match.key(key: .wasd(dir: d), repeat: k.isRepeat, now: now)
        } else if let d = arrows[key] {
            match.key(key: .arrow(dir: d), repeat: k.isRepeat, now: now)
        }
        for t in match.takeOutbox() { transport?.send(t) }
        return true
    }
}

struct TrailsArenaView: View {
    @StateObject private var model: TrailsModel
    @EnvironmentObject private var shell: GameShellModel
    @Environment(\.gameColors) private var colors

    init(props: GameProps?, vs: Int?, onEnd: @escaping (String?) -> Void) {
        _model = StateObject(wrappedValue: TrailsModel(props: props, vs: vs, onEnd: onEnd))
    }

    private var rival: Color { colors.dark ? Color(red: 1, green: 154 / 255, blue: 92 / 255) : Color(red: 212 / 255, green: 82 / 255, blue: 15 / 255) }

    var body: some View {
        let f = model.frame
        let names = f?.names ?? ["you", "computer"]
        let score = f?.score ?? [0, 0]
        let colors = self.colors
        VStack(spacing: GameMetrics.sp2) {
            PongScore(left: names[0], right: names.count > 1 ? names[1] : "", score: (Int(score[0]), Int(score.count > 1 ? score[1] : 0)),
                      leftMine: true, rightMine: false, leftColor: colors.accent.color, rightColor: rival)
            ZStack {
                GameCanvas(frame: { model.tick() }, draw: { g, _ in model.paint(g, colors: colors) })
                    .aspectRatio(640 / 260, contentMode: .fit)
                    .accessibilityElement()
                    .accessibilityLabel("arena")
                if let f, !f.banner.isEmpty {
                    Text(f.banner)
                        .font(f.big ? .gameDigits(GameMetrics.fsDisplay, .semibold) : .game(GameMetrics.fsBody))
                        .foregroundColor(colors.ink.color)
                        .frame(minWidth: f.big ? 44 : nil)
                        .padding(.vertical, 6).padding(.horizontal, GameMetrics.sp3)
                        .background(RoundedRectangle(cornerRadius: GameMetrics.radiusS).fill(colors.bg.opacity(0.92).color))
                        .boxShadow(colors.cardShadow, radius: GameMetrics.radiusS)
                }
            }
            .background(colors.court.color)
            .clipShape(RoundedRectangle(cornerRadius: GameMetrics.radiusS))
            PongNote(f?.note ?? "")
        }
        .gameKeys(shell) { k in model.key(k) }
        .onDisappear { model.stop() }
    }
}

// ---- the arena on a canvas (TrailsPanel.tsx draw)

private func rgbOfColor(_ c: RGBA) -> RGB { (c.r * 255, c.g * 255, c.b * 255) }

private func trgba(_ c: RGB, _ a: Double) -> Col { Col(c.0, c.1, c.2, a) }

private func hot(_ c: RGB, _ k: Double) -> RGB {
    (jsRound(c.0 + (255 - c.0) * k), jsRound(c.1 + (255 - c.1) * k), jsRound(c.2 + (255 - c.2) * k))
}

func trailsDraw(_ ctx: Canvas2D, _ r: TrailsFrame, colors: GameColors, now: Double) {
    let W = Double(ctx.width), H = Double(ctx.height)
    let dpr = W / (TCOLS * 10)
    let c = W / TCOLS
    let dark = colors.dark
    let ink = rgbOfColor(colors.ink)
    ctx.clearRect(0, 0, W, H)
    let DX = [0.0, 1, 0, -1], DY = [-1.0, 0, 1, 0]

    let floor = CanvasGradient(.radial(CGPoint(x: W / 2, y: H / 2), H * 0.1, CGPoint(x: W / 2, y: H / 2), W * 0.62))
    floor.addColorStop(0, trgba(ink, dark ? 0.06 : 0.025))
    floor.addColorStop(1, trgba(ink, dark ? 0.015 : 0.06))
    ctx.fill(floor)
    ctx.beginPath()
    ctx.roundRect(0, 0, W, H, 6 * dpr)
    ctx.fill()

    for x in 1..<Int(TCOLS) {
        ctx.fill(trgba(ink, x % 4 != 0 ? (dark ? 0.035 : 0.04) : dark ? 0.08 : 0.09))
        ctx.fillRect(jsRound(Double(x) * c), 0, dpr, H)
    }
    for y in 1..<Int(TROWS) {
        ctx.fill(trgba(ink, y % 4 != 0 ? (dark ? 0.035 : 0.04) : dark ? 0.08 : 0.09))
        ctx.fillRect(0, jsRound(Double(y) * c), W, dpr)
    }

    let mine = rgbOfColor(colors.accent)
    let rival: RGB = dark ? (255, 154, 92) : (212, 82, 15)
    let colorOf = { (p: Int) in p == 0 ? mine : rival }

    let rim = CanvasGradient(.linear(CGPoint(x: 0, y: 0), CGPoint(x: W, y: 0)))
    rim.addColorStop(0, trgba(mine, dark ? 0.55 : 0.45))
    rim.addColorStop(0.5, trgba(ink, dark ? 0.18 : 0.15))
    rim.addColorStop(1, trgba(rival, dark ? 0.55 : 0.45))
    ctx.stroke(rim)
    ctx.lineWidth = 1.5 * dpr
    ctx.beginPath()
    ctx.roundRect(dpr, dpr, W - 2 * dpr, H - 2 * dpr, 6 * dpr)
    ctx.stroke()

    let centre = { (i: Int32) -> (Double, Double) in ((Double(i % 64) + 0.5) * c, (Double(i / 64) + 0.5) * c) }
    let headAt = { (cy: TrailsCycleView) -> (Double, Double) in
        let glide = cy.alive && !r.over ? r.t : 0
        let next = cy.next
        let dir = Int(next != nil && (Int(next!) + 2) % 4 != Int(cy.dir) ? next! : cy.dir)
        return ((Double(cy.x) + 0.5 + DX[dir] * glide) * c, (Double(cy.y) + 0.5 + DY[dir] * glide) * c)
    }
    let wall = { (cy: TrailsCycleView, from: Int, width: Double, style: Col, blur: Double, glow: Col) in
        var pts = cy.path.dropFirst(from).map(centre)
        if pts.isEmpty { return }
        pts.append(headAt(cy))
        ctx.stroke(style)
        ctx.lineWidth = width
        ctx.lineCap = .round
        ctx.lineJoin = .round
        ctx.shadowColor = glow
        ctx.shadowBlur = blur
        ctx.beginPath()
        for (n, p) in pts.enumerated() {
            if n > 0 { ctx.lineTo(p.0, p.1) } else { ctx.moveTo(p.0, p.1) }
        }
        ctx.stroke()
        ctx.shadowBlur = 0
    }

    for (p, cy) in r.cycles.enumerated() {
        let col = colorOf(p)
        let dim = cy.alive ? 1.0 : 0.55
        wall(cy, 0, c * 1.2, trgba(col, (dark ? 0.16 : 0.12) * dim), 0, .clear)
        wall(cy, 0, c * 0.6, trgba(col, 0.9 * dim), dark ? 10 * dpr : 4 * dpr, trgba(col, dark ? 0.8 : 0.45))
        wall(cy, 0, max(dpr, c * 0.18), trgba(hot(col, dark ? 0.65 : 0.45), 0.9 * dim), 0, .clear)
        if cy.alive { wall(cy, max(0, cy.path.count - HOT_CELLS), c * 0.62, trgba(hot(col, 0.3), 0.55), 0, .clear) }
    }
    for (p, cy) in r.cycles.enumerated() where cy.alive {
        trailsHead(ctx, cy, colorOf(p), headAt(cy), c, dpr, now, dark)
    }
    for (p, cy) in r.cycles.enumerated() where !cy.alive {
        trailsBurst(ctx, cy, colorOf(p), c, dpr, r.crashMs ?? BURST_MS, dark)
    }
}

private func trailsHead(_ ctx: Canvas2D, _ cy: TrailsCycleView, _ col: RGB, _ at: (Double, Double), _ c: Double, _ dpr: Double, _ now: Double, _ dark: Bool) {
    let (x, y) = at
    let pulse = 0.5 + 0.5 * sin(now / 160)
    let halo = CanvasGradient(.radial(CGPoint(x: x, y: y), 0, CGPoint(x: x, y: y), c * (2.3 + 0.45 * pulse)))
    halo.addColorStop(0, trgba(col, dark ? 0.55 : 0.35))
    halo.addColorStop(1, trgba(col, 0))
    ctx.fill(halo)
    ctx.beginPath()
    ctx.arc(x, y, c * (2.3 + 0.45 * pulse), 0, .pi * 2)
    ctx.fill()
    ctx.save()
    ctx.translate(x, y)
    ctx.rotate(Double(cy.dir) * .pi / 2)
    ctx.shadowColor = trgba(col, 0.9)
    ctx.shadowBlur = (dark ? 14 : 6) * dpr
    ctx.fill(trgba(col, 1))
    ctx.beginPath()
    ctx.roundRect(-c * 0.55, -c * 0.8, c * 1.1, c * 1.45, c * 0.5)
    ctx.fill()
    ctx.shadowBlur = 0
    ctx.fill(trgba(hot(col, 0.85), 0.95))
    ctx.beginPath()
    ctx.ellipse(0, -c * 0.42, c * 0.28, c * 0.22, 0, 0, .pi * 2)
    ctx.fill()
    ctx.restore()
}

private func trailsBurst(_ ctx: Canvas2D, _ cy: TrailsCycleView, _ col: RGB, _ c: Double, _ dpr: Double, _ ms: Double, _ dark: Bool) {
    let DX = [0.0, 1, 0, -1], DY = [-1.0, 0, 1, 0]
    let bx = (Double(cy.x) + 0.5 + DX[Int(cy.dir)] * 0.5) * c
    let by = (Double(cy.y) + 0.5 + DY[Int(cy.dir)] * 0.5) * c
    let k = min(1, ms / BURST_MS)
    let fade = 1 - k
    let eased = 1 - pow(1 - k, 3)
    if fade > 0 {
        let flash = CanvasGradient(.radial(CGPoint(x: bx, y: by), 0, CGPoint(x: bx, y: by), c * 3))
        flash.addColorStop(0, trgba(hot(col, 0.7), 0.9 * fade))
        flash.addColorStop(1, trgba(col, 0))
        ctx.fill(flash)
        ctx.beginPath()
        ctx.arc(bx, by, c * 3, 0, .pi * 2)
        ctx.fill()
        ctx.stroke(trgba(col, 0.8 * fade))
        ctx.lineWidth = max(dpr, c * 0.18 * fade)
        ctx.beginPath()
        ctx.arc(bx, by, c * (0.6 + 4.5 * eased), 0, .pi * 2)
        ctx.stroke()
    }
    ctx.lineCap = .round
    for n in 0..<14 {
        let seed = sin((Double(cy.x) * 31 + Double(cy.y) * 17 + Double(n) * 7.3) * 12.9898) * 43758.5453
        let rnd = seed - seed.rounded(.down)
        let a = Double(n) / 14 * .pi * 2 + rnd * 0.6
        let reach = c * (1.6 + rnd * 2.6) * eased
        let len = c * (0.5 + rnd * 0.6) * fade
        let sx = bx + cos(a) * reach, sy = by + sin(a) * reach
        ctx.stroke(trgba(n % 3 != 0 ? col : hot(col, 0.7), max(0, fade)))
        ctx.lineWidth = max(dpr, c * 0.12)
        ctx.beginPath()
        ctx.moveTo(sx, sy)
        ctx.lineTo(sx - cos(a) * len, sy - sin(a) * len)
        ctx.stroke()
    }
    ctx.fill(trgba(col, dark ? 0.35 : 0.3))
    ctx.beginPath()
    ctx.arc(bx, by, c * 0.35, 0, .pi * 2)
    ctx.fill()
}
