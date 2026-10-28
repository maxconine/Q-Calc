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
                // .c4-status aria-live="polite"
                .liveRegion(status(v, myTurn: myTurn))
            Connect4Board(view: v, role: role, cursor: myTurn ? col : nil, ghost: ghostOn && aiming,
                          onHover: { c in col = c; aiming = true }, onLeave: { aiming = false }, onPick: { model.play($0) })
                .accessibilityElement(children: .contain)
                .accessibilityLabel("board")
            PongNote("← → or 1–7 to pick · ↵ or ↓ drop · G \(ghostOn ? "hides" : "shows") the preview · \(skillName.map { "\($0) · " } ?? "")esc leaves")
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
            } else if key.key == "Enter" || key.key == " " || key.key == "ArrowDown" {
                if !key.isRepeat {
                    if model.game.view().over { model.rematch() } else { model.play(col) }
                }
            } else if !key.isCharacter && key.key != "Backspace" && !key.key.hasPrefix("Arrow") {
                return false
            }
            return true
        }
        // the big window around the square board, not the bar's 680-wide card
        .onAppear { shell.bigWidth = 460 }
        .onDisappear {
            model.stop()
            shell.bigWidth = nil
        }
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

    var body: some View {
        let size = boardSize
        // Core Animation layers (C4Layers): the board is built once, each disc is a layer, and the fall and the held
        // disc's slide are Core Animation's, so nothing is redrawn on the CPU per frame (a bitmap redrawn at 60 fps
        // was slow full screen) and no GPU surfaces pile up (a SwiftUI Canvas had kept ~125 MB)
        C4Layers(state: C4Layers.State(view: view, role: role, cursor: cursor, ghost: ghost, colors: colors))
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
        // a big window made from the lobby's shorter shape has less room than 300: the board fits it, never cut off
        return max(160, min(stage.width - 2 * GameMetrics.gutter, stage.gameH - 200))
    }
}

/// The board as layers, in the svg's units scaled to the view: the frame and the 42 holes one shape each (rebuilt
/// only for new colours), the ghost, a layer per disc and the held disc. A new last disc falls from the lane to its
/// hole (260 ms, cubic-bezier(0.5, 0, 0.75, 0.3)); the held disc slides between columns (90 ms ease-out).
private struct C4Layers: NSViewRepresentable {
    struct State: Equatable {
        let view: C4View
        let role: GameSide
        let cursor: Int?
        let ghost: Bool
        let colors: GameColors
    }
    let state: State

    static let cell = 40.0, r = 15.0, lane = 40.0
    static let w = 7 * cell, h = lane + 6 * cell
    static func cx(_ col: Int) -> Double { Double(col) * cell + cell / 2 }
    static func cy(_ row: Int) -> Double { lane + Double(5 - row) * cell + cell / 2 }

    func makeNSView(context: Context) -> BoardView { BoardView() }
    func updateNSView(_ v: BoardView, context: Context) { v.apply(state) }

    final class BoardView: NSView {
        private let content = CALayer()
        private let frameLayer = CAShapeLayer()
        private let holes = CAShapeLayer()
        private let ghostLayer = CAShapeLayer()
        private var discs: [CAShapeLayer] = []
        private let held = CAShapeLayer()
        private var last: State?

        override init(frame: NSRect) {
            super.init(frame: frame)
            wantsLayer = true
            layer?.addSublayer(content)
            content.anchorPoint = .zero
            content.bounds = CGRect(x: 0, y: 0, width: C4Layers.w, height: C4Layers.w)
            frameLayer.path = CGPath(roundedRect: CGRect(x: 0, y: C4Layers.lane, width: C4Layers.w, height: 6 * C4Layers.cell),
                                     cornerWidth: 10, cornerHeight: 10, transform: nil)
            frameLayer.lineWidth = 1
            let holePaths = CGMutablePath()
            for row in 0..<6 { for c in 0..<7 { holePaths.addEllipse(in: Self.circle(C4Layers.cx(c), C4Layers.cy(row))) } }
            holes.path = holePaths
            holes.lineWidth = 1
            ghostLayer.path = Self.disc
            held.path = Self.disc
            for layer in [frameLayer, holes, ghostLayer] { content.addSublayer(layer) }
            for _ in 0..<42 {
                let d = CAShapeLayer()
                d.path = Self.disc
                d.isHidden = true
                content.addSublayer(d)
                discs.append(d)
            }
            content.addSublayer(held)
        }

        required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

        override var isFlipped: Bool { true }
        // the gestures on the SwiftUI side get every event
        override func hitTest(_ point: NSPoint) -> NSView? { nil }

        /// a disc's circle centred on the layer's position
        private static let disc = CGPath(ellipseIn: CGRect(x: -C4Layers.r, y: -C4Layers.r, width: 2 * C4Layers.r, height: 2 * C4Layers.r), transform: nil)
        private static func circle(_ x: Double, _ y: Double) -> CGRect {
            CGRect(x: x - C4Layers.r, y: y - C4Layers.r, width: 2 * C4Layers.r, height: 2 * C4Layers.r)
        }

        override func layout() {
            super.layout()
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            let k = bounds.width / C4Layers.w
            content.setAffineTransform(CGAffineTransform(scaleX: k, y: k))
            content.position = .zero
            CATransaction.commit()
        }

        override func viewDidChangeBackingProperties() {
            super.viewDidChangeBackingProperties()
            let scale = window?.backingScaleFactor ?? 2
            for l in [frameLayer, holes, ghostLayer, held] + discs { l.contentsScale = scale }
        }

        func apply(_ s: State) {
            guard s != last else { return }
            let before = last
            last = s
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            let c = s.colors
            if before?.colors != c {
                frameLayer.fillColor = c.court.cg
                frameLayer.strokeColor = c.line.cg
                holes.fillColor = c.bg.cg
                holes.strokeColor = c.line.mix(0.6, .clear).cg
            }
            // the ghost: where the held disc would land
            if s.ghost, let cur = s.cursor, s.view.landing[cur] >= 0 {
                ghostLayer.isHidden = false
                ghostLayer.position = CGPoint(x: C4Layers.cx(cur), y: C4Layers.cy(Int(s.view.landing[cur])))
                ghostLayer.fillColor = c.accent.mix(0.22, .clear).cg
            } else {
                ghostLayer.isHidden = true
            }
            let line = Set(s.view.line.map(Int.init))
            let lastDisc = s.view.last.map(Int.init)
            for i in 0..<42 {
                let d = discs[i]
                guard let who = s.view.board[i] else { d.isHidden = true; continue }
                let col = i % 7, row = i / 7
                let target = CGPoint(x: C4Layers.cx(col), y: C4Layers.cy(row))
                let dim = s.view.over && !s.view.line.isEmpty && !line.contains(i)
                d.fillColor = (who == s.role ? c.accent : c.ink).opacity(dim ? 0.3 : 1).cg
                d.strokeColor = c.bg.cg
                d.lineWidth = line.contains(i) ? 3 : 0
                let isNew = d.isHidden
                d.isHidden = false
                d.position = target
                // c4-fall: the newest disc drops from the lane to its hole
                if isNew, i == lastDisc, before != nil, !GameMotion.reduced {
                    let fall = CABasicAnimation(keyPath: "position.y")
                    fall.fromValue = C4Layers.lane / 2
                    fall.toValue = target.y
                    fall.duration = 0.26
                    fall.timingFunction = CAMediaTimingFunction(controlPoints: 0.5, 0, 0.75, 0.3)
                    d.add(fall, forKey: "fall")
                }
            }
            // the disc you're holding, sliding between columns
            if let cur = s.cursor {
                let full = s.view.landing[cur] < 0
                held.fillColor = c.accent.opacity(full ? 0.25 : 0.9).cg
                let to = CGPoint(x: C4Layers.cx(cur), y: C4Layers.lane / 2)
                let from = held.isHidden ? to : (held.presentation()?.position ?? held.position)
                held.isHidden = false
                held.position = to
                if from != to, !GameMotion.reduced {
                    let slide = CABasicAnimation(keyPath: "position.x")
                    slide.fromValue = from.x
                    slide.toValue = to.x
                    slide.duration = 0.09
                    slide.timingFunction = CAMediaTimingFunction(controlPoints: 0, 0, 0.58, 1)
                    held.add(slide, forKey: "slide")
                }
            } else {
                held.isHidden = true
            }
            CATransaction.commit()
        }
    }
}
