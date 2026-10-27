import QCalcCore
import QuartzCore
import SwiftUI

// connect 4: `connect 4` then enter (components/Connect4Panel.tsx, the .c4-* rules in index.css). The session and
// the computer are the core's (`Connect4Game`, from lib/connect4Session.ts and connect4Bot.ts).

private let skills: [(String, C4Skill)] = [("easy", .easy), ("normal", .normal), ("hard", .hard)]

struct Connect4Panel: View {
    var body: some View {
        GamePanel {
            GameLobby(LobbyConfig(
                game: "connect4", name: "connect 4", blurb: "four in a row wins.",
                transport: RelayTransport.shared,
                online: { p in AnyView(Connect4GameView(props: p, solo: nil)) },
                local: LocalGames(prompt: "play the computer", options: skills.map(\.0)) { i, onEnd in
                    AnyView(Connect4GameView(props: nil, solo: skills[i].1, onEnd: onEnd))
                }
            ))
        }
    }
}

// whether the landing preview shows at all; G flips it, and it's kept for next time
private let ghostKey = "qcalc-c4-ghost"

/// What the board component kept (`Connect4Game`'s effect): the session, its link, the last view shown.
final class Connect4Model: ObservableObject {
    @Published private(set) var view: C4View
    let game: Connect4Game
    let role: GameSide
    private let transport: PeerTransport?
    private var onEnd: (String?) -> Void
    private var off: (() -> Void)?
    private var timer: Timer?
    private var done = false

    init(transport: PeerTransport?, role: GameSide, solo: C4Skill?, onEnd: @escaping (String?) -> Void) {
        self.transport = transport
        self.role = solo != nil ? .host : role
        self.onEnd = onEnd
        game = Connect4Game(role: role, now: GameTime.now(), solo: solo)
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
        if v.rev != view.rev { view = v }
    }

    func play(_ col: Int) {
        _ = game.play(col: UInt32(col), now: GameTime.now())
        sync()
    }

    func rematch() {
        game.rematch(now: GameTime.now())
        sync()
    }
}

struct Connect4GameView: View {
    let props: GameProps?
    let solo: C4Skill?
    @StateObject private var model: Connect4Model
    @EnvironmentObject private var shell: GameShellModel
    @Environment(\.gameColors) private var colors
    @State private var col = 3
    @State private var ghostOn = GameSaves.load(ghostKey) != "off"
    // aiming: the mouse is over the board, or a column was just picked with the keys; the preview only shows then
    @State private var aiming = false

    init(props: GameProps?, solo: C4Skill?, onEnd: ((String?) -> Void)? = nil) {
        self.props = props
        self.solo = solo
        let end = onEnd ?? props?.onEnd ?? { _ in }
        _model = StateObject(wrappedValue: Connect4Model(transport: props?.transport, role: props?.role ?? .host, solo: solo, onEnd: end))
    }

    private var peer: String { props?.peer ?? "computer" }
    private var skillName: String? { solo.map { s in skills.first { $0.1 == s }!.0 } }

    var body: some View {
        let v = model.view
        let role = model.role
        let myTurn = v.started && !v.over && v.turn == role
        VStack(spacing: GameMetrics.sp2) {
            PongScore(left: role == .host ? "you" : peer, right: role == .guest ? "you" : peer,
                      score: (Int(v.winsHost), Int(v.winsGuest)), leftMine: role == .host, rightMine: role == .guest)
            Text(status(v, myTurn: myTurn))
                .font(.game(GameMetrics.fsBody))
                .foregroundColor(myTurn || v.winner == role ? colors.accent.color : colors.ink.color)
                .lineLimit(1).truncationMode(.tail)
                .frame(maxWidth: .infinity)
                .modifier(PongWait(on: !v.started))
            Connect4Board(view: v, role: role, cursor: myTurn ? col : nil, ghost: ghostOn && aiming,
                          onHover: { c in col = c; aiming = true }, onLeave: { aiming = false }, onPick: { model.play($0) })
            PongNote("← → or 1–7 to pick · ↵ drop · G \(ghostOn ? "hides" : "shows") the preview · \(skillName.map { "\($0) · " } ?? "")esc leaves")
        }
        .gameKeys(shell) { key in
            guard key.down, key.plain, key.key != "Escape" else { return false }
            let n = Int(key.key).flatMap { (1...7).contains($0) ? $0 : nil }
            if key.key == "ArrowLeft" || key.key == "ArrowRight" || n != nil { aiming = true }
            if key.key == "ArrowLeft" {
                col = (col + 6) % 7
            } else if key.key == "ArrowRight" {
                col = (col + 1) % 7
            } else if let n {
                col = n - 1
            } else if key.key.lowercased() == "g" {
                if !key.isRepeat {
                    ghostOn.toggle()
                    GameSaves.store(ghostKey, ghostOn ? "on" : "off")
                }
            } else if key.key == "Enter" || key.key == " " {
                if !key.isRepeat {
                    if model.game.view().over { model.rematch() } else { model.play(col) }
                }
            } else if !key.isCharacter && key.key != "Backspace" && !key.key.hasPrefix("Arrow") {
                return false
            }
            return true
        }
        .onDisappear { model.stop() }
    }

    private func status(_ v: C4View, myTurn: Bool) -> String {
        let their = peer
        if !v.started { return "waiting for \(their)…" }
        if v.over {
            let who = v.winner == model.role ? "you win" : v.winner != nil ? "\(their) wins" : "draw"
            if solo != nil { return "\(who) · ↵ play again" }
            return "\(who) · \(v.againMine ? "waiting for a rematch…" : v.againTheirs ? "\(their) wants a rematch · ↵" : "↵ rematch")"
        }
        if v.thinking { return v.moves == 0 ? "the computer goes first…" : "the computer is thinking…" }
        if v.moves == 0 { return myTurn ? "you go first" : "\(their) goes first" }
        return myTurn ? "your turn" : "\(their)’s turn"
    }
}

/// The board in its svg units: a 40-unit lane on top for the disc you're holding, then the 7×6 grid.
private struct Connect4Board: View {
    let view: C4View
    let role: GameSide
    let cursor: Int?
    let ghost: Bool
    let onHover: (Int) -> Void
    let onLeave: () -> Void
    let onPick: (Int) -> Void
    @Environment(\.gameColors) private var colors
    @Environment(\.gameStage) private var stage
    @State private var fall = FallClock()
    @State private var held = HeldClock()
    @State private var settled = 0

    // one more draw once the animations are over, so the board stops on its last frame
    private func settle() {
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.32) { settled += 1 }
    }

    private static let cell = 40.0, r = 15.0, lane = 40.0
    private static let w = 7 * cell, h = lane + 6 * cell
    private static func cx(_ col: Int) -> Double { Double(col) * cell + cell / 2 }
    private static func cy(_ row: Int) -> Double { lane + Double(5 - row) * cell + cell / 2 }

    var body: some View {
        let size = boardSize
        let now = CACurrentMediaTime()
        let _ = fall.note(view.last, now)
        let _ = held.note(cursor, now)
        let _ = settled
        let moving = fall.moving(now) || held.moving(now)
        Group {
            // the disc falling and the held one sliding draw every frame; a still board draws once
            if moving {
                TimelineView(.animation) { _ in Canvas { ctx, sz in draw(ctx, sz) } }
            } else {
                Canvas { ctx, sz in draw(ctx, sz) }
            }
        }
        .onAppear { settle() }
        .onChange(of: view.rev) { _ in settle() }
        .onChange(of: cursor) { _ in settle() }
        .frame(width: size, height: size)
        .contentShape(Rectangle())
        .pointerHover { p in
            if let p { onHover(column(p.x, size)) } else { onLeave() }
        }
        .spatialTap { p in onPick(column(p.x, size)) }
        .frame(maxWidth: .infinity)
    }

    private func column(_ x: CGFloat, _ size: CGFloat) -> Int { max(0, min(6, Int(x / size * 7))) }

    // .c4-board: 300 wide (aspect 1); full screen grows it to min(100%, --game-h − 200)
    private var boardSize: CGFloat {
        guard let stage else { return 300 }
        return max(300, min(stage.width - 2 * GameMetrics.gutter, stage.gameH - 200))
    }

    private func circle(_ x: Double, _ y: Double, _ r: Double) -> Path {
        Path(ellipseIn: CGRect(x: x - r, y: y - r, width: 2 * r, height: 2 * r))
    }

    private func draw(_ ctx: GraphicsContext, _ sz: CGSize) {
        var ctx = ctx
        let k = sz.width / Self.w
        ctx.scaleBy(x: k, y: k)
        let now = CACurrentMediaTime()
        let frame = Path(roundedRect: CGRect(x: 0, y: Self.lane, width: Self.w, height: 6 * Self.cell), cornerRadius: 10)
        ctx.fill(frame, with: .color(colors.court.color))
        ctx.stroke(frame, with: .color(colors.line.color), lineWidth: 1)
        for row in 0..<6 {
            for c in 0..<7 {
                let hole = circle(Self.cx(c), Self.cy(row), Self.r)
                ctx.fill(hole, with: .color(colors.bg.color))
                ctx.stroke(hole, with: .color(colors.line.mix(0.6, .clear).color), lineWidth: 1)
            }
        }
        if ghost, let cursor, view.landing[cursor] >= 0 {
            ctx.fill(circle(Self.cx(cursor), Self.cy(Int(view.landing[cursor])), Self.r), with: .color(colors.accent.mix(0.22, .clear).color))
        }
        let line = Set(view.line.map(Int.init))
        let last = view.last.map(Int.init) ?? -1
        for i in 0..<42 {
            guard let who = view.board[i] else { continue }
            let c = i % 7, row = i / 7
            var y = Self.cy(row)
            if i == last {
                // c4-fall: from the lane down to its hole, 260 ms, cubic-bezier(0.5, 0, 0.75, 0.3)
                let t = min(1, (now - fall.at) / 0.26)
                y -= (Self.cy(row) - Self.lane / 2) * (1 - cssEase(t, 0.5, 0, 0.75, 0.3))
            }
            let disc = circle(Self.cx(c), y, Self.r)
            let dim = view.over && !view.line.isEmpty && !line.contains(i)
            let fill = (who == role ? colors.accent : colors.ink).opacity(dim ? 0.3 : 1)
            ctx.fill(disc, with: .color(fill.color))
            if line.contains(i) { ctx.stroke(disc, with: .color(colors.bg.color), lineWidth: 3) }
        }
        if let cursor {
            let full = view.landing[cursor] < 0
            ctx.fill(circle(held.x(now, Self.cx), Self.lane / 2, Self.r), with: .color(colors.accent.opacity(full ? 0.25 : 0.9).color))
        }
    }
}

/// When the newest disc dropped, for its fall.
private final class FallClock {
    var last: UInt32?
    var at = 0.0
    func note(_ l: UInt32?, _ now: Double) {
        if l != last {
            if l != nil { at = now }
            last = l
        }
    }
    func moving(_ now: Double) -> Bool { now - at < 0.3 }
}

/// The held disc slides between columns (`transition: cx 90ms ease-out`).
private final class HeldClock {
    var to: Int?
    var from = 3
    var at = 0.0
    func note(_ c: Int?, _ now: Double) {
        guard c != to else { return }
        if let c, let t = to { from = t; at = now; to = c } else { to = c; if let c { from = c } }
    }
    func moving(_ now: Double) -> Bool { now - at < 0.12 }
    func x(_ now: Double, _ cx: (Int) -> Double) -> Double {
        guard let to else { return cx(from) }
        let t = min(1, (now - at) / 0.09)
        return cx(from) + (cx(to) - cx(from)) * cssEase(t, 0, 0, 0.58, 1)
    }
}
