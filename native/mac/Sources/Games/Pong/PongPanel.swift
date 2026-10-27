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
private struct PongCourt: View {
    let view: PongView
    let mine: GameSide
    let colors: GameColors
    @Environment(\.displayScale) private var scale

    var body: some View {
        Canvas { ctx, size in
            let k = size.width / geo.courtW * scale
            func rect(_ x: Double, _ y: Double, _ w: Double, _ h: Double) -> Path {
                let x0 = (x * k).rounded(), y0 = (y * k).rounded()
                let x1 = ((x + w) * k).rounded(), y1 = ((y + h) * k).rounded()
                return Path(CGRect(x: x0 / scale, y: y0 / scale, width: (x1 - x0) / scale, height: (y1 - y0) / scale))
            }
            var dashes = Path()
            var y = 4.0
            while y < geo.courtH {
                dashes.addPath(rect(geo.courtW / 2 - 1, y, 2, 8))
                y += 16
            }
            ctx.fill(dashes, with: .color(colors.muted.opacity(0.35).color))
            for (side, py) in [(GameSide.host, view.paddleHost), (.guest, view.paddleGuest)] {
                let x = side == .host ? geo.paddleInset : geo.courtW - geo.paddleInset - geo.paddleW
                ctx.fill(rect(x, py - geo.paddleH / 2, geo.paddleW, geo.paddleH), with: .color((side == mine ? colors.accent : colors.ink).color))
            }
            if view.phase == .play {
                ctx.fill(rect(view.ballX - geo.ballSize / 2, view.ballY - geo.ballSize / 2, geo.ballSize, geo.ballSize), with: .color(colors.ink.color))
            }
        }
    }
}
