import AppKit
import QCalcCore
import SwiftUI

/// What a game's lobby tells the shell around it (`ShellHooks` in `gameShellContext.ts`).
struct ShellHooks {
    /// One step back through the lobby; false on its first screen, where esc closes the game.
    var back: () -> Bool
    /// Back to the lobby from a running game.
    var leave: () -> Void
    /// Nothing is running, so esc leaves without pausing.
    var idle: () -> Bool
    /// The link must outlive the window hiding: someone's waiting to join, or playing.
    var hold: () -> Bool
}

/// How full screen lays a game out (`GameFullscreenOptions`): the picture's normal size, the room the panel spends
/// around it, the margin. Games that don't say get a 16 px margin, as the page's shell gave them.
struct GameFullscreenOptions: Equatable {
    var baseW: Double = 640
    var baseH: Double = 360
    var chrome: Double = 0
    var pad: Double = 16
    var maxUi: Double = 2
    var integer = false
}

/// The shell around whichever game is open (`components/GameShell.tsx`): esc (leave full screen, step back through
/// the lobby, pause, warn before leaving an online game), the paused card, full screen, and the keys. One per game
/// window; views find it as an `@EnvironmentObject`.
final class GameShellModel: ObservableObject {
    @Published private(set) var mode: GameShellMode = .lobby
    @Published private(set) var paused = false
    @Published private(set) var armedAt: Double?
    @Published private(set) var fullscreen = false
    @Published private(set) var layout = GameFullscreenOptions()

    /// Who gets keys after the shell: the lobby, then the open game (`.gameKeys(shell) { … }`).
    let keys = GameKeyRouter()
    /// The Mac app's full screen is a big window of the game's own ("big window" / "back to the bar").
    let windowed = true
    /// The game's kind (`pong`, `ember`…), for the room lists.
    let kind: String

    /// The window closes (esc from the lobby's first screen, or a page's own esc button there).
    var onClose: () -> Void = {}
    /// Full screen went on or off: the window zooms.
    var onFullscreen: (Bool) -> Void = { _ in }

    private var hooks: ShellHooks?
    private var hooksId = 0
    private var armTimer: Timer?

    init(kind: String) {
        self.kind = kind
    }

    deinit {
        // the game closing ends everything it started
        if GameTime.paused { GameTime.resume() }
    }

    // ---- what the lobby and the games call

    func setMode(_ next: GameShellMode) {
        if next != .local { resume() }
        disarm()
        if mode != next { mode = next }
    }

    /// Returns the unregister.
    func register(_ h: ShellHooks) -> () -> Void {
        hooksId += 1
        let id = hooksId
        hooks = h
        return { [weak self] in
            if self?.hooksId == id { self?.hooks = nil }
        }
    }

    func toggleFullscreen() {
        setFullscreen(!fullscreen)
    }

    func exitFullscreen() {
        setFullscreen(false)
    }

    private func setFullscreen(_ on: Bool) {
        guard on != fullscreen else { return }
        fullscreen = on
        onFullscreen(on)
    }

    /// A game with a picture of its own shape (ember) says how full screen lays it out; returns the undo.
    func fullscreenLayout(_ opts: GameFullscreenOptions) -> () -> Void {
        layout = opts
        return { [weak self] in self?.layout = GameFullscreenOptions() }
    }

    /// What esc does, for an esc button on the page: a lobby screen that can't step back closes the game.
    func escape() {
        if !onEscape() { onClose() }
    }

    /// Whether the open game must stay open while the window is hidden.
    var holds: Bool { hooks?.hold() ?? false }

    func pause() {
        guard !paused else { return }
        GameTime.pause()
        paused = true
    }

    func resume() {
        guard paused else { return }
        GameTime.resume()
        paused = false
    }

    // ---- esc

    /// Esc from the app: true while the game stays open, false to close it.
    @discardableResult
    func onEscape() -> Bool {
        let action = gameEscAction(mode: mode, fullscreen: fullscreen, paused: paused, armedAt: armedAt,
                                   idle: hooks?.idle() ?? false, now: Date().timeIntervalSince1970 * 1000)
        switch action {
        case .unfullscreen:
            exitFullscreen()
            return true
        case .lobby:
            return hooks?.back() ?? false
        case .pause:
            pause()
            return true
        case .arm:
            arm()
            return true
        case .leave:
            resume()
            disarm()
            hooks?.leave()
            return true
        }
    }

    // "esc again to leave" goes after a moment
    private func arm() {
        armedAt = Date().timeIntervalSince1970 * 1000
        armTimer?.invalidate()
        armTimer = Timer.scheduledTimer(withTimeInterval: gameArmMs() / 1000, repeats: false) { [weak self] _ in
            self?.armedAt = nil
        }
    }

    private func disarm() {
        armTimer?.invalidate()
        armTimer = nil
        if armedAt != nil { armedAt = nil }
    }

    // ---- keys and the window

    /// Every key the game window gets. True when something here took it.
    func handle(_ key: GameKey) -> Bool {
        if key.down {
            // ⌃F, and esc while full screen (useGameFullscreen), after the paused gate lets them through
            let gate = paused ? gamePausedKey(key: key.key, meta: key.meta, ctrl: key.ctrl, alt: key.alt) : .pass
            if gate != .pass {
                if !key.isRepeat {
                    if gate == .fullscreen { toggleFullscreen() } else { resume() }
                }
                return true
            }
            if gameIsFullscreenChord(key: key.key, code: key.code, ctrl: key.ctrl, meta: key.meta, alt: key.alt, shift: key.shift) {
                if !key.isRepeat { toggleFullscreen() }
                return true
            }
            let plainEscape = key.key == "Escape" && !key.ctrl && !key.meta && !key.alt && !key.shift
            if fullscreen && plainEscape {
                exitFullscreen()
                return true
            }
            if key.key == "Escape" {
                if !onEscape() { onClose() }
                return true
            }
        }
        return keys.dispatch(key)
    }

    /// A game on this computer pauses when the window loses focus.
    func windowAway() {
        if mode == .local && !(hooks?.idle() ?? false) { pause() }
    }
}

/// The game's area while full screen: its height (`--game-h`), for boards that grow with it. nil when not.
struct GameStage: Equatable {
    var gameH: CGFloat
    var width: CGFloat
}

private struct GameStageKey: EnvironmentKey {
    static let defaultValue: GameStage? = nil
}

extension EnvironmentValues {
    var gameStage: GameStage? {
        get { self[GameStageKey.self] }
        set { self[GameStageKey.self] = newValue }
    }
}

/// The window's content: the bar-styled card the page showed (680 wide at the top left), or while full screen
/// the whole window with the game area centred, plus the paused card, the corner button and the toast.
struct GameShellView<Content: View>: View {
    @ObservedObject var shell: GameShellModel
    @Environment(\.colorScheme) private var scheme
    let content: Content

    init(shell: GameShellModel, @ViewBuilder content: () -> Content) {
        self.shell = shell
        self.content = content()
    }

    var body: some View {
        let colors = GameColors(dark: scheme == .dark)
        GeometryReader { geo in
            Group {
                if shell.fullscreen {
                    let l = gameFullscreenLayout(viewW: geo.size.width, viewH: geo.size.height, baseW: shell.layout.baseW,
                                                 baseH: shell.layout.baseH, chrome: shell.layout.chrome, pad: shell.layout.pad,
                                                 maxUi: shell.layout.maxUi, integer: shell.layout.integer)
                    let pad = CGFloat(shell.layout.pad * l.ui)
                    let gameH = max(0, geo.size.height - 2 * pad)
                    let w = min(geo.size.width - 2 * pad, gameH * 1.8)
                    layers(colors) {
                        content
                            .frame(width: w)
                            .frame(maxHeight: gameH)
                            .environment(\.gameStage, GameStage(gameH: gameH, width: w))
                    }
                    .frame(width: geo.size.width, height: geo.size.height)
                    .background(colors.bg.color)
                } else {
                    layers(colors) { content }
                        .frame(width: min(GameMetrics.panelWidth, max(0, geo.size.width - 24)))
                        .fixedSize(horizontal: false, vertical: true)
                        .background(colors.bg.color)
                        .clipShape(RoundedRectangle(cornerRadius: GameMetrics.radius))
                        .boxShadow(colors.shadow, radius: GameMetrics.radius)
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                }
            }
        }
        // the page ran under the clear title bar but kept its strip clear (--game-fs-inset), so the traffic lights
        // never sit on a heading
        .padding(.top, GameWindow.titlebarHeight)
        .background(shell.fullscreen ? colors.bg.color : Color.clear)
        .ignoresSafeArea()
        .environment(\.gameColors, colors)
        .environmentObject(shell)
        .foregroundColor(colors.ink.color)
    }

    private func layers<C: View>(_ colors: GameColors, @ViewBuilder _ area: () -> C) -> some View {
        ZStack {
            area().blur(radius: shell.paused ? 1.5 : 0)
            // a game on this computer gets the full-screen button in the corner; an online one has the lobby's header
            if shell.mode == .local {
                FullscreenButton(on: shell.fullscreen, corner: true) { shell.toggleFullscreen() }
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)
                    .padding(6)
            }
            if shell.paused { PausedCard(shell: shell) }
            if shell.armedAt != nil {
                GameToast(text: "esc again to leave")
                    .frame(maxHeight: .infinity, alignment: .top)
                    .padding(.top, GameMetrics.sp3)
            }
        }
    }
}

/// The paused overlay: any key carries on, esc leaves to the lobby.
private struct PausedCard: View {
    @ObservedObject var shell: GameShellModel
    @Environment(\.gameColors) private var colors

    var body: some View {
        ZStack {
            colors.bg.mix(0.7, .clear).color
                .contentShape(Rectangle())
                .onTapGesture { shell.resume() }
            VStack(spacing: GameMetrics.sp2) {
                Text("paused")
                    .font(.game(GameMetrics.fsDisplay, .semibold))
                    .tracking(GameMetrics.displayTracking(GameMetrics.fsDisplay))
                    .foregroundColor(colors.ink.color)
                Text("any key carries on · esc leaves to the lobby")
                    .font(.game(GameMetrics.fsSmall))
                    .foregroundColor(colors.muted.color)
                HStack(spacing: GameMetrics.sp3) {
                    PongButton("resume", kbd: "↵") { shell.resume() }
                    PongButton(gameFullscreenLabel(on: shell.fullscreen, windowed: shell.windowed), kbd: "F") { shell.toggleFullscreen() }
                }
                .padding(.top, GameMetrics.sp2)
            }
            .padding(.vertical, GameMetrics.sp4)
            .padding(.horizontal, GameMetrics.sp5)
            .background(colors.bg.color)
            .clipShape(RoundedRectangle(cornerRadius: GameMetrics.radius))
            .boxShadow(colors.cardShadow, radius: GameMetrics.radius)
        }
    }
}

/// A pill over the game (`.game-toast`).
struct GameToast: View {
    let text: String
    @Environment(\.gameColors) private var colors

    var body: some View {
        Text(text)
            .font(.game(GameMetrics.fsSmall))
            .foregroundColor(colors.ink.color)
            .lineLimit(1)
            .padding(.vertical, 5)
            .padding(.horizontal, GameMetrics.sp3)
            .background(Capsule().fill(colors.bg.color))
            .boxShadow(colors.cardShadow, radius: 999)
            .allowsHitTesting(false)
    }
}

/// The small full-screen button beside a game's esc, and in the corner while playing (`FullscreenButton`).
struct FullscreenButton: View {
    let on: Bool
    var corner = false
    let action: () -> Void
    @Environment(\.gameColors) private var colors
    @EnvironmentObject private var shell: GameShellModel
    @State private var hover = false

    var body: some View {
        let label = gameFullscreenLabel(on: on, windowed: shell.windowed)
        Button(action: action) {
            FullscreenGlyph(on: on)
                .stroke(hover ? colors.ink.color : colors.muted.color, style: StrokeStyle(lineWidth: 1.5, lineCap: .round, lineJoin: .round))
                .frame(width: 13, height: 13)
                .frame(width: 24, height: 22)
                .background(RoundedRectangle(cornerRadius: GameMetrics.radiusS).fill(hover ? colors.fillHover.color : .clear))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .opacity(corner && !hover ? 0.5 : 1)
        .onHover { hover = $0 }
        .help(on ? "\(label) (F, ⌃F or esc)" : "\(label) (F or ⌃F)")
        .accessibilityLabel(label)
    }
}

/// The corners in a 16-unit box: inward when on (M6 2v4H2 …), outward when off (M2 6V2h4 …).
struct FullscreenGlyph: Shape {
    let on: Bool

    func path(in rect: CGRect) -> Path {
        let k = rect.width / 16
        func p(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: rect.minX + x * k, y: rect.minY + y * k) }
        var path = Path()
        let corners: [[(CGFloat, CGFloat)]] = on
            ? [[(6, 2), (6, 6), (2, 6)], [(10, 2), (10, 6), (14, 6)], [(6, 14), (6, 10), (2, 10)], [(10, 14), (10, 10), (14, 10)]]
            : [[(2, 6), (2, 2), (6, 2)], [(14, 6), (14, 2), (10, 2)], [(2, 10), (2, 14), (6, 14)], [(14, 10), (14, 14), (10, 14)]]
        for c in corners {
            path.move(to: p(c[0].0, c[0].1))
            path.addLine(to: p(c[1].0, c[1].1))
            path.addLine(to: p(c[2].0, c[2].1))
        }
        return path
    }
}
