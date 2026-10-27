import AppKit
import QCalcCore
import QuartzCore
import SwiftUI

// ultimate tic-tac-toe: `ultimate` then enter (components/UltimatePanel.tsx and the .ut-* rules in index.css).
// connect 4's panel and status line on a chalkboard: hand-drawn lines (the wobble is in the paths, from the core's
// chalk port) in neon chalk, you green and them orange. only the newest mark and the boards you can play in
// animate, once.

private let skills: [(String, UtSkill)] = [("easy", .easy), ("normal", .normal), ("hard", .hard)]

struct UltimatePanel: View {
    var body: some View {
        GamePanel {
            GameLobby(LobbyConfig(
                game: "ultimate", name: "ultimate", blurb: "win three small boards in a row.",
                transport: RelayTransport.shared,
                online: { p in AnyView(UltimateGameView(props: p, solo: nil)) },
                local: LocalGames(prompt: "play the computer", options: skills.map(\.0)) { i, onEnd in
                    AnyView(UltimateGameView(props: nil, solo: skills[i].1, onEnd: onEnd))
                }
            ))
        }
    }
}

// geometry in svg units: nine 3×3 boards on a slate, the big # between them
private enum G {
    static let cell = 30.0, board = 90.0, gap = 14.0, pad = 10.0
    static let w = 3 * board + 2 * gap
    static func bx(_ b: Int) -> Double { Double(b % 3) * (board + gap) }
    static func by(_ b: Int) -> Double { Double(b / 3) * (board + gap) }
    static func cx(_ c: Int) -> Double { bx(c / 9) + Double(c % 9 % 3) * cell }
    static func cy(_ c: Int) -> Double { by(c / 9) + Double(c % 9 / 3) * cell }
    // the 9×9 grid the arrows move over, and back to a cell
    static func gridRow(_ c: Int) -> Int { (c / 9) / 3 * 3 + (c % 9) / 3 }
    static func gridCol(_ c: Int) -> Int { (c / 9) % 3 * 3 + (c % 9) % 3 }
    static func atGrid(_ row: Int, _ col: Int) -> Int { (row / 3 * 3 + col / 3) * 9 + (row % 3) * 3 + col % 3 }
    static let centre = 4
}

// the host plays x and the guest o, whoever goes first
private func markOf(_ who: GameSide) -> String { who == .host ? "x" : "o" }

final class UltimateModel: ObservableObject {
    @Published private(set) var view: UtView
    let game: UltimateGame
    let role: GameSide
    private let transport: PeerTransport?
    private let onEnd: (String?) -> Void
    private var off: (() -> Void)?
    private var timer: Timer?
    private var done = false
    /// Each move: the board pick is spent, and the cursor jumps to the board the next move is sent to.
    var onMoved: ((UtView) -> Void)?
    private var moved: UInt32 = 0

    init(transport: PeerTransport?, role: GameSide, solo: UtSkill?, onEnd: @escaping (String?) -> Void) {
        self.transport = transport
        self.role = solo != nil ? .host : role
        self.onEnd = onEnd
        game = UltimateGame(role: role, now: GameTime.now(), solo: solo)
        view = game.view()
        off = transport?.subscribe { [weak self] e in
            guard let self else { return }
            switch e {
            case let .message(data):
                self.game.receive(text: data, now: GameTime.now())
                self.sync()
            case let .closed(reason):
                self.finish(peerCloseText(reason: reason, word: transport?.peerWord ?? "computer"))
            default: break
            }
        }
        let t = Timer(timeInterval: 0.2, repeats: true) { [weak self] _ in
            guard let self else { return }
            _ = self.game.tick(now: GameTime.now())
            self.sync()
        }
        RunLoop.main.add(t, forMode: .common)
        timer = t
        sync()
    }

    deinit { stop() }

    func stop() {
        timer?.invalidate()
        timer = nil
        off?()
        off = nil
    }

    private func finish(_ note: String?) {
        guard !done else { return }
        done = true
        stop()
        onEnd(note)
    }

    func sync() {
        for text in game.takeSent() { transport?.send(text) }
        let v = game.view()
        if let ended = v.ended { return finish(ended.isEmpty ? nil : ended) }
        guard v.rev != view.rev else { return }
        view = v
        if v.moves != moved {
            moved = v.moves
            onMoved?(v)
        }
    }

    func play(_ cell: Int) {
        _ = game.play(cell: UInt32(cell), now: GameTime.now())
        sync()
    }

    func rematch() {
        game.rematch(now: GameTime.now())
        sync()
    }
}

/// The chalk colours: the slate is dark in both themes, so they're fixed; the names above follow the theme.
private enum Chalk {
    static let chalk = RGBA(0xefece2), mine = RGBA(0x6ee69e), theirs = RGBA(0xff9a5c)
    static func theirsName(_ dark: Bool) -> RGBA { dark ? RGBA(0xff9a5c) : RGBA(0xd4520f) }
    static let font: Font = NSFont(name: "ChalkboardSE-Regular", size: 13) != nil ? .custom("ChalkboardSE-Regular", size: 13) : .game(13)
}

struct UltimateGameView: View {
    let props: GameProps?
    let solo: UtSkill?
    @StateObject private var model: UltimateModel
    @EnvironmentObject private var shell: GameShellModel
    @Environment(\.gameColors) private var colors
    @State private var cursor = G.centre * 9 + G.centre
    // free to play anywhere: the board 1–9 picked, so the next digit picks a cell in it
    @State private var picked: Int?
    // aiming: the mouse is over the board, or a cell was just picked with the keys; the cursor only shows then
    @State private var aiming = false

    init(props: GameProps?, solo: UtSkill?, onEnd: ((String?) -> Void)? = nil) {
        self.props = props
        self.solo = solo
        let end = onEnd ?? props?.onEnd ?? { _ in }
        _model = StateObject(wrappedValue: UltimateModel(transport: props?.transport, role: props?.role ?? .host, solo: solo, onEnd: end))
    }

    private var peer: String { props?.peer ?? "computer" }

    var body: some View {
        let v = model.view
        let role = model.role
        let myTurn = v.started && !v.over && v.turn == role
        let zone = v.forced.map(Int.init) ?? picked
        let mineName = colors.accent.color, theirName = Chalk.theirsName(colors.dark).color
        VStack(spacing: GameMetrics.sp2) {
            PongScore(left: "\(role == .host ? "you" : peer) · x", right: "o · \(role == .guest ? "you" : peer)",
                      score: (Int(v.winsHost), Int(v.winsGuest)), leftMine: false, rightMine: false,
                      leftColor: role == .host ? mineName : theirName, rightColor: role == .guest ? mineName : theirName)
            Text(status(v, myTurn: myTurn))
                .font(Chalk.font)
                .foregroundColor(myTurn || v.winner == role ? colors.accent.color : colors.ink.color)
                .lineLimit(1).truncationMode(.tail)
                .frame(maxWidth: .infinity)
                .modifier(PongWait(on: !v.started))
            UltimateBoard(view: v, role: role, live: myTurn, zone: zone, cursor: myTurn && aiming ? cursor : nil,
                          canPlay: { model.game.canPlay(cell: UInt32($0), who: role) },
                          onHover: { c in cursor = c; aiming = true }, onLeave: { aiming = false }, onPick: { model.play($0) })
            PongNote("\(myTurn && zone == nil ? "1–9 pick a board" : "1–9 or arrows pick a cell") · ↵ play · \(solo.map { s in "\(skills.first { $0.1 == s }!.0) · " } ?? "")esc leaves")
        }
        .onAppear {
            model.onMoved = { v in
                picked = nil
                if let f = v.forced { cursor = Int(f) * 9 + cursor % 9 }
            }
        }
        .gameKeys(shell) { key in
            guard key.down, key.plain, key.key != "Escape" else { return false }
            let state = model.game.view()
            let n = Int(key.key).flatMap { (1...9).contains($0) ? $0 - 1 : nil }
            let step: [String: (Int, Int)] = ["ArrowUp": (-1, 0), "ArrowDown": (1, 0), "ArrowLeft": (0, -1), "ArrowRight": (0, 1)]
            if let (dr, dc) = step[key.key] {
                cursor = G.atGrid((G.gridRow(cursor) + dr + 9) % 9, (G.gridCol(cursor) + dc + 9) % 9)
                aiming = true
            } else if let n {
                if let zone = state.forced.map(Int.init) ?? picked {
                    cursor = zone * 9 + n
                } else if state.open[n] {
                    picked = n
                    cursor = n * 9 + G.centre
                }
                aiming = true
            } else if key.key == "Backspace" {
                picked = nil
            } else if key.key == "Enter" || key.key == " " {
                if !key.isRepeat {
                    if state.over { model.rematch() }
                    // a hidden cursor shows first, so ↵ never plays a cell you can't see
                    else if !aiming { aiming = true }
                    else { model.play(cursor) }
                }
            } else if !key.isCharacter {
                return false
            }
            return true
        }
        .onDisappear { model.stop() }
    }

    private func status(_ v: UtView, myTurn: Bool) -> String {
        let their = peer
        if !v.started { return "waiting for \(their)…" }
        if v.over {
            let who = v.winner == model.role ? "you win" : v.winner != nil ? "\(their) wins" : "draw"
            if solo != nil { return "\(who) · ↵ play again" }
            return "\(who) · \(v.againMine ? "waiting for a rematch…" : v.againTheirs ? "\(their) wants a rematch · ↵" : "↵ rematch")"
        }
        if v.thinking { return v.moves == 0 ? "the computer goes first…" : "the computer is thinking…" }
        if v.moves == 0 { return myTurn ? "you go first · \(markOf(model.role))" : "\(their) goes first" }
        if myTurn { return v.forced == nil ? "your turn · play anywhere" : "your turn" }
        return "\(their)’s turn"
    }
}

/// When things appeared, for their one-off draw-in: the newest mark, the open boxes, the trail.
private final class InkClock {
    var last: UInt32?
    var lastAt = -10.0
    var live = false
    var liveAt = -10.0
    func note(last l: UInt32?, live lv: Bool, _ now: Double) {
        if l != last { last = l; lastAt = now }
        if lv != live { live = lv; if lv { liveAt = now } }
    }
    func moving(_ now: Double) -> Bool { now - lastAt < 1.2 || now - liveAt < 0.5 }
}

private struct UltimateBoard: View {
    let view: UtView
    let role: GameSide
    let live: Bool
    let zone: Int?
    let cursor: Int?
    let canPlay: (Int) -> Bool
    let onHover: (Int) -> Void
    let onLeave: () -> Void
    let onPick: (Int) -> Void
    @State private var clock = InkClock()
    @State private var settled = 0

    private static let span = G.w + 2 * G.pad

    var body: some View {
        let now = CACurrentMediaTime()
        let _ = clock.note(last: view.last, live: live, now)
        let _ = settled
        let size: CGFloat = 340
        let aimOk = cursor.map(canPlay) ?? false
        Group {
            if clock.moving(now) {
                TimelineView(.animation) { _ in Canvas { ctx, sz in draw(ctx, sz, aimOk) } }
            } else {
                Canvas { ctx, sz in draw(ctx, sz, aimOk) }
            }
        }
        .frame(width: size, height: size)
        .background(SlateBackground())
        .clipShape(RoundedRectangle(cornerRadius: 14))
        .overlay(RoundedRectangle(cornerRadius: 14).strokeBorder(Color.white.opacity(0.06), lineWidth: 1))
        .contentShape(Rectangle())
        .pointerHover { p in
            if let p {
                let c = cellAt(p, size)
                if c >= 0 { onHover(c) }
            } else {
                onLeave()
            }
        }
        .spatialTap { p in
            let c = cellAt(p, size)
            if c >= 0 { onPick(c) }
        }
        .onChange(of: view.rev) { _ in settle() }
        .onChange(of: live) { _ in settle() }
        .frame(maxWidth: .infinity)
    }

    private func settle() {
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.25) { settled += 1 }
    }

    // the cell under a point on the board, or -1 in a gap or off it
    private func cellAt(_ p: CGPoint, _ size: CGFloat) -> Int {
        let x = p.x / size * Self.span - G.pad
        let y = p.y / size * Self.span - G.pad
        let col = Int(floor(x / (G.board + G.gap)))
        let row = Int(floor(y / (G.board + G.gap)))
        let ix = x - Double(col) * (G.board + G.gap)
        let iy = y - Double(row) * (G.board + G.gap)
        if col < 0 || col > 2 || row < 0 || row > 2 || ix >= G.board || iy >= G.board { return -1 }
        return (row * 3 + col) * 9 + Int(floor(iy / G.cell)) * 3 + Int(floor(ix / G.cell))
    }

    private static let dust: [(x: Double, y: Double, r: Double)] = (0..<70).map { i in
        let h = sin(Double(i) * 12.9898) * 43758.5453
        let k = sin(Double(i) * 78.233) * 12345.678
        let frac = { (v: Double) in v - floor(v) }
        let jsMod = { (a: Double, b: Double) in a.truncatingRemainder(dividingBy: b) }
        return (frac(h) * (G.w + 2 * G.pad) - G.pad, frac(k) * (G.w + 2 * G.pad) - G.pad,
                0.4 + jsMod(jsMod(h * 7, 1) + 1, 1) * 0.9)
    }

    // the slate's lines never change, so their paths are made once
    private static let lines: [(d: String, big: Bool)] = {
        var out: [(String, Bool)] = []
        for k in [1, 2] {
            let p = Double(k) * (G.board + G.gap) - G.gap / 2
            out.append((utChalkLine(ax: p, ay: -4, bx: p, by: G.w + 4, seed: Double(100 + k), amp: 1.6), true))
            out.append((utChalkLine(ax: -4, ay: p, bx: G.w + 4, by: p, seed: Double(200 + k), amp: 1.6), true))
        }
        for b in 0..<9 {
            for k in [1, 2] {
                let kk = Double(k)
                out.append((utChalkLine(ax: G.bx(b) + kk * G.cell, ay: G.by(b) + 5, bx: G.bx(b) + kk * G.cell, by: G.by(b) + G.board - 5,
                                        seed: Double(b * 10 + k), amp: 1.1), false))
                out.append((utChalkLine(ax: G.bx(b) + 5, ay: G.by(b) + kk * G.cell, bx: G.bx(b) + G.board - 5, by: G.by(b) + kk * G.cell,
                                        seed: Double(b * 10 + k + 5), amp: 1.1), false))
            }
        }
        return out
    }()

    private func draw(_ ctx: GraphicsContext, _ sz: CGSize, _ aimOk: Bool) {
        var ctx = ctx
        let k = sz.width / Self.span
        ctx.scaleBy(x: k, y: k)
        ctx.translateBy(x: G.pad, y: G.pad)
        let now = CACurrentMediaTime()
        let round = StrokeStyle(lineWidth: 1, lineCap: .round, lineJoin: .round)
        func stroke(_ c: GraphicsContext, _ p: Path, _ color: RGBA, _ width: Double) {
            var s = round
            s.lineWidth = width
            c.stroke(p, with: .color(color.color), style: s)
        }
        // the dust and the grid
        for d in Self.dust {
            ctx.fill(Path(ellipseIn: CGRect(x: d.x - d.r, y: d.y - d.r, width: 2 * d.r, height: 2 * d.r)), with: .color(Chalk.chalk.opacity(0.07).color))
        }
        for l in Self.lines {
            stroke(ctx, SVGPath.path(l.d), Chalk.chalk.opacity(l.big ? 0.85 : 0.3), l.big ? 3.2 : 1.4)
        }
        // the boards you may play in, chalked round as your turn comes
        if live {
            let t = cssEase(min(1, (now - clock.liveAt) / 0.38), 0, 0, 0.58, 1)
            for b in 0..<9 where view.open[b] {
                let p = SVGPath.path(utChalkBox(x: G.bx(b) - 4, y: G.by(b) - 4, w: G.board + 8, h: G.board + 8, seed: Double(300 + b)))
                let zoneOn = zone == b
                stroke(ctx, p.trimmedPath(from: 0, to: t), Chalk.mine.opacity(zoneOn ? 1 : 0.45), zoneOn ? 2.4 : 1.5)
            }
        }
        let last = view.last.map(Int.init) ?? -1
        // where the last move sent you, as a dotted chalk arc
        if live, last >= 0, let f = view.forced.map(Int.init), last / 9 != f {
            let x1 = G.cx(last) + G.cell / 2, y1 = G.cy(last) + G.cell / 2
            let x2 = G.bx(f) + G.board / 2, y2 = G.by(f) + G.board / 2
            let bend = 0.18
            var p = Path()
            p.move(to: CGPoint(x: x1, y: y1))
            p.addQuadCurve(to: CGPoint(x: x2, y: y2), control: CGPoint(x: (x1 + x2) / 2 - (y2 - y1) * bend, y: (y1 + y2) / 2 + (x2 - x1) * bend))
            let o = 0.6 * cssEase(max(0, min(1, (now - clock.lastAt - 0.15) / 0.3)), 0, 0, 0.58, 1)
            ctx.stroke(p, with: .color(Chalk.mine.opacity(o).color), style: StrokeStyle(lineWidth: 1.6, lineCap: .round, dash: [0.5, 6]))
        }
        if let cursor {
            let r = Path(roundedRect: CGRect(x: G.cx(cursor) + 3, y: G.cy(cursor) + 3, width: G.cell - 6, height: G.cell - 6), cornerRadius: 7)
            ctx.fill(r, with: .color(aimOk ? Chalk.mine.opacity(0.2).color : Chalk.chalk.opacity(0.06).color))
        }
        // the pieces
        let tone = { (who: GameSide) in who == role ? Chalk.mine : Chalk.theirs }
        let since = now - clock.lastAt
        func ink(_ d: String, _ color: RGBA, glow: Double, glowOp: Double, core: Double, dim: Bool, fresh: (dur: Double, delay: Double)?) {
            var t = 1.0
            if let fresh { t = cssEase(max(0, min(1, (since - fresh.delay) / fresh.dur)), 0, 0, 0.58, 1) }
            guard t > 0 else { return }
            let p = SVGPath.path(d).trimmedPath(from: 0, to: t)
            var c = ctx
            if dim { c.opacity = 0.22 }
            stroke(c, p, color.opacity(glowOp), glow)
            stroke(c, p, color, core)
        }
        for b in 0..<9 {
            if let m = view.boards[b], m != .draw {
                ctx.fill(Path(ellipseIn: CGRect(x: G.bx(b), y: G.by(b) + G.board / 2 - G.board / 2.3, width: G.board, height: 2 * G.board / 2.3)),
                         with: .color(Chalk.chalk.opacity(0.045).color))
            }
        }
        for i in 0..<81 {
            guard let who = view.cells[i] else { continue }
            ink(utChalkMark(kind: markOf(who), x: G.cx(i), y: G.cy(i), size: G.cell, seed: Double(i)), tone(who),
                glow: 7, glowOp: 0.2, core: 2.7, dim: view.boards[i / 9] != nil, fresh: i == last ? (0.32, 0) : nil)
        }
        let won = Set(view.line.map(Int.init))
        for b in 0..<9 {
            guard let m = view.boards[b], m != .draw else { continue }
            let who: GameSide = m == .host ? .host : .guest
            // a decided board holding the last move was decided by it
            let fresh = last >= 0 && last / 9 == b
            ink(utChalkMark(kind: markOf(who), x: G.bx(b), y: G.by(b), size: G.board, seed: Double(50 + b)), tone(who),
                glow: 16, glowOp: 0.16, core: 6.5, dim: !view.line.isEmpty && !won.contains(b), fresh: fresh ? (0.56, 0.12) : nil)
        }
        if view.line.count == 3, let winner = view.winner {
            let a = Int(view.line[0]), z = Int(view.line[2])
            let ax = G.bx(a) + G.board / 2, ay = G.by(a) + G.board / 2
            let zx = G.bx(z) + G.board / 2, zy = G.by(z) + G.board / 2
            let ext = 0.18
            ink(utChalkLine(ax: ax - (zx - ax) * ext, ay: ay - (zy - ay) * ext, bx: zx + (zx - ax) * ext, by: zy + (zy - ay) * ext, seed: 999, amp: 2.5),
                tone(winner), glow: 14, glowOp: 0.3, core: 4, dim: false, fresh: (0.5, 0.65))
        }
    }
}

/// The slate: dark in both themes, a soft light top left and a shadow bottom right, and the inset shading.
private struct SlateBackground: View {
    var body: some View {
        GeometryReader { geo in
            let w = geo.size.width, h = geo.size.height
            ZStack {
                Color(.sRGB, red: 0x1f / 255, green: 0x2a / 255, blue: 0x25 / 255)
                // radial-gradient(90% 70% at 85% 95%, rgba(0,0,0,.28), transparent 60%)
                EllipticalGradient(colors: [Color.black.opacity(0.28), .clear], center: UnitPoint(x: 0.85, y: 0.95),
                                   startRadiusFraction: 0, endRadiusFraction: 0.6)
                    .frame(width: w * 0.9 * 2, height: h * 0.7 * 2)
                    .position(x: w * 0.85, y: h * 0.95)
                // radial-gradient(120% 90% at 30% 15%, rgba(255,255,255,.06), transparent 60%)
                EllipticalGradient(colors: [Color.white.opacity(0.06), .clear], center: .center,
                                   startRadiusFraction: 0, endRadiusFraction: 0.6)
                    .frame(width: w * 1.2 * 2, height: h * 0.9 * 2)
                    .position(x: w * 0.3, y: h * 0.15)
                // inset 0 2px 14px rgba(0,0,0,.35)
                RoundedRectangle(cornerRadius: 14)
                    .stroke(Color.black.opacity(0.35), lineWidth: 14)
                    .blur(radius: 7)
                    .offset(y: 2)
            }
            .clipped()
        }
    }
}
