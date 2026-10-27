import AppKit
import QCalcCore
import SwiftUI

// pong: `pong` then enter (components/PongPanel.tsx, the .pong-court rules in index.css). The core's `PongGame`
// (lib/pongSession.ts) runs the game; this feeds it keys and frames and draws what it returns.

private let skills: [(String, PongSkill)] = [("easy", .easy), ("normal", .normal), ("hard", .hard)]
private let geo = pongGeometry()

struct PongPanel: View {
    var body: some View {
        GamePanel {
            GameLobby(LobbyConfig(
                game: "pong", name: "pong", blurb: "first to \(geo.winScore).",
                transport: RelayTransport.shared,
                online: { p in AnyView(PongGameView(props: p, solo: nil)) },
                local: LocalGames(prompt: "play the computer", options: skills.map(\.0)) { i, onEnd in
                    AnyView(PongGameView(props: nil, solo: skills[i].1, onEnd: onEnd))
                }
            ))
        }
    }
}

final class PongModel: ObservableObject {
    let game: PongGame
    let role: GameSide
    private let transport: PeerTransport?
    private let onEnd: (String?) -> Void
    private var off: (() -> Void)?
    private var done = false
    // the last view a frame drew
    private(set) var view: PongView
    // what the text around the court shows; republished only when it changes
    @Published private(set) var hud = Hud()

    struct Hud: Equatable {
        var score = (0, 0)
        var phase: PongPhase = .serve
        var winner: GameSide?
        var started = false
        var againMine = false
        var againTheirs = false
        static func == (a: Hud, b: Hud) -> Bool {
            a.score == b.score && a.phase == b.phase && a.winner == b.winner && a.started == b.started
                && a.againMine == b.againMine && a.againTheirs == b.againTheirs
        }
    }

    init(transport: PeerTransport?, role: GameSide, solo: PongSkill?, onEnd: @escaping (String?) -> Void) {
        self.transport = transport
        self.role = solo != nil ? .host : role
        self.onEnd = onEnd
        game = PongGame(role: role, now: GameTime.now(), solo: solo)
        view = game.tick(now: GameTime.now())
        off = transport?.subscribe { [weak self] e in
            guard let self else { return }
            switch e {
            case let .message(data):
                self.game.receive(text: data, now: GameTime.now())
                self.flush()
            case let .closed(reason):
                self.finish(peerCloseText(reason: reason, word: transport?.peerWord ?? "computer"))
            default: break
            }
        }
        flush()
    }

    deinit { stop() }

    func stop() {
        off?()
        off = nil
    }

    private func finish(_ note: String?) {
        guard !done else { return }
        done = true
        stop()
        DispatchQueue.main.async { self.onEnd(note) }
    }

    private func flush() {
        for text in game.takeSent() { transport?.send(text) }
    }

    /// One frame: the session moves on to now.
    func frame() {
        guard !done else { return }
        let v = game.tick(now: GameTime.now())
        flush()
        if let ended = v.ended { return finish(ended.isEmpty ? nil : ended) }
        if v.hasSnap { view = v }
        let next = Hud(score: (Int(v.scoreHost), Int(v.scoreGuest)), phase: v.phase, winner: v.winner, started: v.started,
                       againMine: v.againMine, againTheirs: v.againTheirs)
        if next != hud { DispatchQueue.main.async { self.hud = next } }
    }

    func key(up: Bool, held: Bool) { game.key(up: up, held: held) }
    func release() { game.release() }
    func rematch() {
        game.rematch()
        flush()
    }
}

struct PongGameView: View {
    let props: GameProps?
    let solo: PongSkill?
    @StateObject private var model: PongModel
    @EnvironmentObject private var shell: GameShellModel
    @Environment(\.gameColors) private var colors
    @Environment(\.gameStage) private var stage

    init(props: GameProps?, solo: PongSkill?, onEnd: ((String?) -> Void)? = nil) {
        self.props = props
        self.solo = solo
        let end = onEnd ?? props?.onEnd ?? { _ in }
        _model = StateObject(wrappedValue: PongModel(transport: props?.transport, role: props?.role ?? .host, solo: solo, onEnd: end))
    }

    private var peer: String { props?.peer ?? "computer" }

    var body: some View {
        let hud = model.hud
        let role = model.role
        VStack(spacing: GameMetrics.sp2) {
            PongScore(left: role == .host ? "you" : peer, right: role == .guest ? "you" : peer, score: hud.score,
                      leftMine: role == .host, rightMine: role == .guest)
            court(hud)
            PongNote("↑↓ or W S to move · \(solo.map { s in "\(skills.first { $0.1 == s }!.0) · " } ?? "")esc leaves")
        }
        .gameKeys(shell) { key in
            guard key.plain, key.key != "Escape" else { return false }
            let k = key.key.lowercased()
            if key.key == "ArrowUp" || k == "w" {
                model.key(up: true, held: key.down)
            } else if key.key == "ArrowDown" || k == "s" {
                model.key(up: false, held: key.down)
            } else if key.key == "Enter" {
                if key.down && !key.isRepeat { model.rematch() }
            } else if !key.isCharacter && key.key != "Backspace" {
                return false
            }
            return true
        }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didResignKeyNotification)) { _ in model.release() }
        .onDisappear {
            model.release()
            model.stop()
        }
    }

    private func banner(_ hud: PongModel.Hud) -> String {
        if !hud.started { return "waiting for \(peer)…" }
        if hud.phase == .over {
            let who = hud.winner == model.role ? "you win" : "\(peer) wins"
            if solo != nil { return "\(who) · ↵ play again" }
            return "\(who) · \(hud.againMine ? "waiting for a rematch…" : hud.againTheirs ? "\(peer) wants a rematch · ↵" : "↵ rematch")"
        }
        if hud.phase == .serve && hud.score.0 + hud.score.1 == 0 { return "first to \(geo.winScore)" }
        return ""
    }

    @ViewBuilder private func court(_ hud: PongModel.Hud) -> some View {
        let text = banner(hud)
        TimelineView(.animation) { _ in
            let _ = model.frame()
            PongCourt(view: model.view, mine: model.role, colors: colors)
                .accessibilityElement()
                .accessibilityLabel("court")
        }
        .aspectRatio(geo.courtW / geo.courtH, contentMode: .fit)
        .background(RoundedRectangle(cornerRadius: GameMetrics.radiusS).fill(colors.court.color))
        .insetRing(colors.line, width: 1, radius: GameMetrics.radiusS)
        .clipShape(RoundedRectangle(cornerRadius: GameMetrics.radiusS))
        .overlay {
            if !text.isEmpty {
                Text(text)
                    .font(.game(GameMetrics.fsBody))
                    .foregroundColor(colors.ink.color)
                    .lineLimit(1).truncationMode(.tail)
                    .padding(.vertical, 5)
                    .padding(.horizontal, GameMetrics.sp3)
                    .background(Capsule().fill(colors.bg.color))
                    .boxShadow(colors.cardShadow, radius: 999)
                    .padding(.horizontal, GameMetrics.sp3)
            }
        }
        // full screen: as wide as leaves room for the court's height (--crt-max-w)
        .frame(maxWidth: stage.map { max(0, ($0.gameH - 160) * geo.courtW / geo.courtH) } ?? .infinity)
        .frame(maxWidth: .infinity)
    }
}

/// The court as the page's canvas drew it: the centre line, the paddles, the ball, on whole device pixels.
/// Plain Core Animation layers moved each frame, not a SwiftUI Canvas: a Canvas redrawn every display frame kept
/// about 120 MB of GPU surfaces while a game ran.
private struct PongCourt: NSViewRepresentable {
    let view: PongView
    let mine: GameSide
    let colors: GameColors

    func makeNSView(context: Context) -> CourtView { CourtView() }

    func updateNSView(_ court: CourtView, context: Context) {
        court.show(view, mine: mine, colors: colors)
    }

    final class CourtView: NSView {
        /// y down from the court's top, as the canvas measured
        private let stage = CALayer()
        private let host = CALayer(), guest = CALayer(), ball = CALayer()
        private var dashes: [CALayer] = []
        private var last: (view: PongView, mine: GameSide, colors: GameColors)?

        override init(frame: NSRect) {
            super.init(frame: frame)
            wantsLayer = true
            stage.isGeometryFlipped = true
            for l in [host, guest, ball] { stage.addSublayer(l) }
            layer?.addSublayer(stage)
        }

        required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

        override func hitTest(_ point: NSPoint) -> NSView? { nil }
        override var wantsUpdateLayer: Bool { true }
        override func updateLayer() {}

        // cacheDisplay (DevSnapshot) draws through here, which leaves sublayers out: the same rects, painted
        override func draw(_ dirtyRect: NSRect) {
            guard let cg = NSGraphicsContext.current?.cgContext else { return }
            cg.saveGState()
            cg.translateBy(x: 0, y: bounds.height)
            cg.scaleBy(x: 1, y: -1)
            for l in [host, guest, ball] + dashes where !l.isHidden {
                guard let color = l.backgroundColor else { continue }
                cg.setFillColor(color)
                cg.fill(l.frame)
            }
            cg.restoreGState()
        }

        override func layout() {
            super.layout()
            redraw()
        }

        override func viewDidChangeBackingProperties() {
            super.viewDidChangeBackingProperties()
            redraw()
        }

        func show(_ view: PongView, mine: GameSide, colors: GameColors) {
            last = (view, mine, colors)
            redraw()
        }

        private func redraw() {
            guard let (view, mine, colors) = last else { return }
            let scale = window?.backingScaleFactor ?? 2
            let k = bounds.width / geo.courtW * scale
            func rect(_ x: Double, _ y: Double, _ w: Double, _ h: Double) -> CGRect {
                let x0 = (x * k).rounded(), y0 = (y * k).rounded()
                let x1 = ((x + w) * k).rounded(), y1 = ((y + h) * k).rounded()
                return CGRect(x: x0 / scale, y: y0 / scale, width: (x1 - x0) / scale, height: (y1 - y0) / scale)
            }
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            stage.frame = bounds
            var lines: [CGRect] = []
            var y = 4.0
            while y < geo.courtH {
                lines.append(rect(geo.courtW / 2 - 1, y, 2, 8))
                y += 16
            }
            while dashes.count < lines.count {
                let l = CALayer()
                stage.insertSublayer(l, at: 0)
                dashes.append(l)
            }
            let dash = colors.muted.opacity(0.35).cg
            for (i, l) in dashes.enumerated() {
                l.isHidden = i >= lines.count
                if i < lines.count { l.frame = lines[i] }
                l.backgroundColor = dash
            }
            for (side, py, l) in [(GameSide.host, view.paddleHost, host), (.guest, view.paddleGuest, guest)] {
                let x = side == .host ? geo.paddleInset : geo.courtW - geo.paddleInset - geo.paddleW
                l.frame = rect(x, py - geo.paddleH / 2, geo.paddleW, geo.paddleH)
                l.backgroundColor = (side == mine ? colors.accent : colors.ink).cg
            }
            ball.isHidden = view.phase != .play
            ball.frame = rect(view.ballX - geo.ballSize / 2, view.ballY - geo.ballSize / 2, geo.ballSize, geo.ballSize)
            ball.backgroundColor = colors.ink.cg
            CATransaction.commit()
        }
    }
}
