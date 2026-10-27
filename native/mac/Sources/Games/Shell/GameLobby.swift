import QCalcCore
import SwiftUI

// The lobby every game opens on (components/GameLobby.tsx; its rules are the core's `lobby_*`, from lib/lobby.ts).
// First vs computer or vs a friend; vs computer is the game's own options, 1, 2, 3…; vs a friend is the
// same-keyboard options, host, join with a code and the open rooms for this game, by the host's nickname, 🔒 when
// they take a code. Once a game starts it takes the panel and talks to the transport itself; when it ends it
// hands back a line to show.

/// What a game over the link is given.
struct GameProps {
    let transport: PeerTransport
    let role: GameSide
    let peer: String
    /// The game is over for good: back to the friend screen, with this line.
    let onEnd: (String?) -> Void
}

/// What a game offers on this computer: solo levels or modes, and a friend on the same keyboard.
struct LocalGames {
    /// The line over the solo options.
    var prompt: String
    var options: [String]
    /// Two on one keyboard, on the friend screen.
    var together: [String] = []
    /// `option` counts through options, then together; call `onEnd` when the game is over.
    var play: (_ option: Int, _ onEnd: @escaping (String?) -> Void) -> AnyView
}

/// Everything `<GameLobby …>` took as props.
struct LobbyConfig {
    /// The game's kind, which the room lists go by.
    var game: String
    /// The game's name, as typed in the bar.
    var name: String
    /// The line under the title.
    var blurb: String
    /// nil where there's no link: only the local options show.
    var transport: PeerTransport?
    /// The game over the link.
    var online: ((GameProps) -> AnyView)?
    /// The link just opened, before the next message: a game that must exist by then makes it here.
    var onOpen: ((GameSide, String) -> Void)?
    var local: LocalGames?
    /// The header's line when there's no link.
    var sub: String?
    /// More under the options (the controls, a setting).
    var footer: AnyView?
    /// The game's own lobby keys, before the lobby's; true takes the key.
    var onKey: ((GameKey) -> Bool)?
    /// Esc while playing: nothing is running, so leave without a pause.
    var idle: (() -> Bool)?
    /// Esc's leave while playing, when the game has somewhere nearer to go than the lobby; false goes to the lobby.
    var leave: (() -> Bool)?
    /// The lobby brings its own panel padding (`.pong`): the game it opens has a panel of its own.
    var frame = false
    /// The game over the link draws its own header, so the lobby's would be a second one.
    var ownHead = false
}

final class LobbyModel: ObservableObject {
    @Published private(set) var state: LobbyState
    @Published private(set) var nick = GameNickname.load()
    /// The relay sent no room list, so rooms go by code.
    @Published private(set) var codeOnly = false
    /// Mid-game, whose link dropped: the game stands still until they're back.
    @Published private(set) var away: PeerWho?

    var config: LobbyConfig
    private weak var shell: GameShellModel?
    private var cleanups: [() -> Void] = []
    private var watching = false

    private static let lockedKey = "qcalc-host-locked"

    init(config: LobbyConfig) {
        self.config = config
        let link = config.online != nil ? config.transport : nil
        state = lobbyHome(screen: lobbyFirstScreen(online: link != nil, solo: UInt32(config.local?.options.count ?? 0),
                                                   together: UInt32(config.local?.together.count ?? 0)), note: nil)
    }

    deinit {
        for c in cleanups { c() }
        // the link goes when the game does, however it closes
        link?.close()
        if watching { link?.unwatch() }
    }

    var link: PeerTransport? { config.online != nil ? config.transport : nil }
    var solo: Int { config.local?.options.count ?? 0 }
    var together: Int { config.local?.together.count ?? 0 }

    var ctx: LobbyContext {
        LobbyContext(online: link != nil, codeOnly: codeOnly, codeLength: UInt32(link?.codeLength ?? 0),
                     solo: UInt32(solo), together: UInt32(together), peerWord: link?.peerWord ?? "computer")
    }

    var first: LobbyScreen { lobbyFirstScreen(online: link != nil, solo: UInt32(solo), together: UInt32(together)) }

    private var lockedPref: Bool { GameSaves.load(Self.lockedKey) == "1" }

    /// Hooks the lobby into the shell and the link, once.
    func attach(_ shell: GameShellModel) {
        guard self.shell == nil else { return }
        self.shell = shell
        cleanups.append(shell.register(ShellHooks(
            back: { [weak self] in
                guard let self, let back = lobbyBack(state: self.state, ctx: self.ctx) else { return false }
                self.set(back.state)
                self.run(back.effect)
                return true
            },
            leave: { [weak self] in
                guard let self else { return }
                if self.config.leave?() == true { return }
                if case let .local(option) = self.state {
                    self.set(lobbyHome(screen: lobbyOptionScreen(option: option, solo: UInt32(self.solo)), note: nil))
                } else {
                    self.toFriend(nil)
                }
            },
            idle: { [weak self] in self?.config.idle?() ?? false },
            hold: { [weak self] in self.map { lobbyHoldsLink(state: $0.state) } ?? false }
        )))
        cleanups.append(shell.keys.push { [weak self] key in self?.key(key) ?? false })
        if let link {
            cleanups.append(link.subscribe { [weak self] e in self?.heard(e) })
        }
        set(state)
    }

    func set(_ next: LobbyState) {
        state = next
        // where the player is, for esc: the lobby, a game on this computer, or one over the link
        switch next {
        case .game: shell?.setMode(.online)
        case .local: shell?.setMode(.local)
        default: shell?.setMode(.lobby)
        }
        // the list is watched while the lobby's own screens show, so the friend screen has it as it opens
        let watch: Bool = { if case .home = next { return link != nil } else { return false } }()
        if watch != watching {
            watching = watch
            if watch { link?.watch(config.game) } else { link?.unwatch() }
        }
    }

    func run(_ effect: LobbyEffect?) {
        guard let effect else { return }
        switch effect {
        case let .host(locked):
            GameSaves.store(Self.lockedKey, locked ? "1" : "0")
            link?.host(game: config.game, name: nick, locked: locked)
        case let .join(room, code):
            link?.join(roomId: room.id, code: code, name: nick)
        case .cancel:
            link?.close()
        case let .rename(name):
            nick = GameNickname.save(name)
        case .fullscreen:
            shell?.toggleFullscreen()
        }
    }

    /// A game over the link ended: back to the friend screen.
    func toFriend(_ note: String?) {
        link?.close()
        set(lobbyHome(screen: .friend, note: note))
    }

    /// A local game ended.
    func localEnded(_ option: UInt32, _ note: String?) {
        set(lobbyHome(screen: lobbyOptionScreen(option: option, solo: UInt32(solo)), note: note))
    }

    /// A key's worth, from a click.
    func click(_ key: String) {
        let step = lobbyKey(state: state, key: key, ctx: ctx, lockedPref: lockedPref, name: nick)
        set(step.state)
        run(step.effect)
    }

    func join(_ room: GameRoom) {
        let step = lobbyJoinRoom(room: room)
        set(step.state)
        run(step.effect)
    }

    private func heard(_ e: GamePeerEvent) {
        switch e {
        case let .rooms(_, _, codeOnly): self.codeOnly = codeOnly
        case let .away(who):
            GameTime.pause()
            away = who
        case .back, .closed:
            GameTime.resume()
            away = nil
        default: break
        }
        let next = lobbyEvent(state: state, event: e, ctx: ctx)
        guard next != state else { return }
        if case let .game(role, peer) = next, case .open = e { config.onOpen?(role, peer) }
        set(next)
    }

    // enter, arrows, backspace and letters would otherwise reach the bar
    private func key(_ k: GameKey) -> Bool {
        switch state {
        case .game, .local: return false
        default: break
        }
        guard k.down, k.plain, k.key != "Escape" else { return false }
        if case .home = state, config.onKey?(k) == true { return true }
        let step = lobbyKey(state: state, key: k.key, ctx: ctx, lockedPref: lockedPref, name: nick)
        if k.isRepeat && step.effect != nil { return true }
        if step.state != state { set(step.state) }
        run(step.effect)
        return true
    }
}

/// `<GameLobby>`: the lobby's screens, or the game it started.
struct GameLobby: View {
    @StateObject private var model: LobbyModel
    @EnvironmentObject private var shell: GameShellModel
    @Environment(\.gameColors) private var colors

    init(_ config: LobbyConfig) {
        _model = StateObject(wrappedValue: LobbyModel(config: config))
    }

    var body: some View {
        content
            .onAppear { model.attach(shell) }
    }

    @ViewBuilder private var content: some View {
        let c = model.config
        switch model.state {
        case let .local(option):
            if let local = c.local {
                local.play(Int(option)) { [weak model] note in model?.localEnded(option, note) }
            }
        case let .game(role, peer):
            if let link = model.link, let online = c.online {
                let props = GameProps(transport: link, role: role, peer: peer) { [weak model] note in model?.toFriend(note) }
                VStack(alignment: .leading, spacing: 0) {
                    if !c.ownHead { LobbyHead(name: c.name, sub: "with \(peer)") }
                    online(props)
                }
                .overlay(alignment: .top) {
                    if let away = model.away {
                        GameToast(text: away == .peer ? "\(peer) stepped away · the game waits for them" : "reconnecting to your game…")
                            .padding(.top, GameMetrics.sp3)
                    }
                }
            }
        default:
            let screen = VStack(alignment: .leading, spacing: 0) {
                LobbyHead(name: c.name, sub: model.link?.label ?? (c.sub ?? ""))
                LobbyBody(model: model)
            }
            if c.frame { GamePanel { screen } } else { screen }
        }
    }
}

/// `.pong-body`: a centred column, 12 apart, at least 168 high.
struct PongBody<Content: View>: View {
    let content: Content
    init(@ViewBuilder content: () -> Content) { self.content = content() }

    var body: some View {
        VStack(spacing: GameMetrics.sp3) { content }
            .frame(maxWidth: .infinity)
            .frame(minHeight: 168 - GameMetrics.sp3 - GameMetrics.sp1)
            .padding(.top, GameMetrics.sp3)
            .padding(.bottom, GameMetrics.sp1)
    }
}

private struct LobbyBody: View {
    @ObservedObject var model: LobbyModel
    @EnvironmentObject private var shell: GameShellModel
    @Environment(\.gameColors) private var colors

    private var fsLabel: String { gameFullscreenLabel(on: shell.fullscreen, windowed: shell.windowed) }

    private var back: some View { PongLink(title: "⌫ back") { model.click("Backspace") } }

    // "you're teal otter · n renames", the name a link that's ink
    private func you() -> some View {
        HStack(spacing: 0) {
            Text("you’re ")
            PongLink(title: model.nick, ink: true) { model.click("n") }
            Text(" · n renames")
        }
    }

    /// A note line built of runs, some of them links.
    private func note<C: View>(@ViewBuilder _ c: () -> C, wait: Bool = false) -> some View {
        HStack(spacing: 0) { c() }
            .font(.game(GameMetrics.fsSmall))
            .foregroundColor(colors.muted.color)
            .modifier(PongWait(on: wait))
    }

    var body: some View {
        let c = model.config
        switch model.state {
        case let .home(screen, rooms, pick, noteText, offline):
            let first = model.first
            let backHint = screen != first
            let tail = noteText.map { PongNote($0) }
            if screen == .start {
                PongBody {
                    PongLine(c.blurb)
                    HStack(spacing: GameMetrics.sp3) {
                        ForEach(Array(["vs computer", "vs a friend"].enumerated()), id: \.offset) { i, choice in
                            PongButton(choice, kbd: "\(i + 1)", on: i == pick, choice: true) { model.click("\(i + 1)") }
                        }
                    }
                    if model.link != nil { note { you() } } else { PongNote("←→ pick · ↵ go") }
                    tail
                }
            } else if screen == .solo {
                let options = c.local?.options ?? []
                PongBody {
                    PongLine(c.blurb)
                    VStack(spacing: GameMetrics.sp2) {
                        if let prompt = c.local?.prompt, !prompt.isEmpty {
                            Text(prompt).font(.game(GameMetrics.fsCaption)).foregroundColor(colors.muted.color)
                        }
                        HStack(spacing: GameMetrics.sp3) {
                            ForEach(Array(options.enumerated()), id: \.offset) { i, option in
                                PongButton(option, kbd: "\(i + 1)", on: i == pick) { model.set(.local(option: UInt32(i))) }
                            }
                        }
                    }
                    c.footer
                    note {
                        Text(options.count > 1 ? "↑↓ pick · ↵ play · " : "↵ plays · ")
                        if backHint { back; Text(" · ") }
                        Text("f \(fsLabel)")
                    }
                    tail
                }
            } else {
                let together = c.local?.together ?? []
                let solo = model.solo
                PongBody {
                    PongLine(c.blurb)
                    WrapRow(spacing: GameMetrics.sp3) {
                        ForEach(Array(together.enumerated()), id: \.offset) { i, option in
                            PongButton(option, kbd: "\(i + 1)", on: model.link == nil && i == pick) { model.set(.local(option: UInt32(solo + i))) }
                        }
                        if model.link != nil {
                            PongButton("host", kbd: "H") { model.click("h") }
                            PongButton("join with a code", kbd: "J") { model.click("j") }
                        }
                    }
                    if let link = model.link {
                        if model.codeOnly {
                            note {
                                Text("no room list from the game server yet, so rooms go by code · ")
                                if backHint { back; Text(" · ") }
                                you()
                            }
                        } else {
                            VStack(spacing: GameMetrics.sp2) {
                                Text("rooms open now").font(.game(GameMetrics.fsCaption)).foregroundColor(colors.muted.color)
                                if offline {
                                    PongNote(peerCloseText(reason: .network, word: link.peerWord), wait: true)
                                } else if !rooms.isEmpty {
                                    RoomList(rooms: rooms, pick: Int(pick), game: c.name) { model.join($0) }
                                } else {
                                    PongNote("nobody’s hosting \(c.name) right now").padding(.vertical, GameMetrics.sp2)
                                }
                                note {
                                    if !rooms.isEmpty { Text("↑↓ pick · ↵ join · ") }
                                    if backHint { back; Text(" · ") }
                                    you()
                                }
                            }
                            .frame(maxWidth: .infinity)
                        }
                    } else {
                        note {
                            Text(together.count > 1 ? "↑↓ pick · ↵ play · " : "↵ plays · ")
                            if backHint { back; Text(" · ") }
                            Text("f \(fsLabel)")
                        }
                    }
                    c.footer
                    tail
                }
            }
        case let .setup(locked):
            PongBody {
                PongLine("host a room as \(model.nick)")
                HStack(spacing: GameMetrics.sp3) {
                    PongButton("open", kbd: "O", on: !locked) { model.click("o") }
                    PongButton("🔒 locked", kbd: "L", on: locked) { model.click("l") }
                }
                note {
                    Text(locked ? "only someone with the code you send can join" : "anyone who opens \(c.name) can join")
                    Text(" · ↵ host · ")
                    back
                }
            }
        case let .hosting(locked, code, renewed):
            let n = model.link?.codeLength ?? 0
            PongBody {
                if locked {
                    let shown = (code?.isEmpty == false ? code! : String(repeating: "·", count: n)).map(String.init)
                    PongCode(digits: shown)
                    PongLine((renewed ? "too many wrong codes, so here’s a new one. " : "")
                        + (model.codeOnly ? "send your friend this code · on their Q Calc: type \(c.name), J, then the code"
                                          : "send your friend this code · they pick \(model.nick) in the \(c.name) list"))
                } else {
                    PongLine(text: Text("your room is open · it’s listed as ") + Text(model.nick).bold())
                }
                PongNote("waiting for someone to join…", wait: true)
                note { back }
            }
        case let .code(room, digits, noteText):
            codeScreen(room: room, digits: digits, note: noteText, pairing: false)
        case let .pairing(room):
            if !room.locked {
                PongBody {
                    PongLine("joining \(room.name)…").modifier(PongWait(on: true))
                    note { back }
                }
            } else {
                codeScreen(room: room, digits: "", note: nil, pairing: true)
            }
        case let .rename(draft, _):
            PongBody {
                PongLine("what the room lists call you")
                HStack(spacing: 1) {
                    Text(draft).font(.game(GameMetrics.fsBody)).foregroundColor(colors.ink.color)
                    Caret()
                }
                .frame(minWidth: 220 - 2 * GameMetrics.sp3, minHeight: GameMetrics.fsBody * 1.4)
                .padding(.vertical, 6)
                .padding(.horizontal, GameMetrics.sp3)
                .insetRing(colors.accent.mix(0.55, .clear), width: 1, radius: GameMetrics.radiusS)
                PongNote("↵ keep · tab picks another · esc back")
            }
        case .game, .local:
            EmptyView()
        }
    }

    private func codeScreen(room: GameRoom, digits: String, note noteText: String?, pairing: Bool) -> some View {
        let n = model.link?.codeLength ?? 0
        let d = Array(digits).map(String.init)
        return PongBody {
            PongLine(room.id.isEmpty ? "the code your friend sent" : "🔒 the code for \(room.name)’s room")
            PongCode(digits: (0..<n).map { i in pairing ? "•" : (i < d.count ? d[i] : "") }, entry: true, busy: pairing,
                     caret: pairing ? nil : d.count)
            note {
                Text(pairing ? "joining… · " : noteText.map { "\($0) · " } ?? "type the \(n) digits · ")
                back
            }
        }
    }
}

/// `.pong-peers.lobby-list`: the rooms, five rows high before it scrolls.
private struct RoomList: View {
    let rooms: [GameRoom]
    let pick: Int
    let game: String
    let join: (GameRoom) -> Void
    @Environment(\.gameColors) private var colors

    var body: some View {
        ScrollView {
            VStack(spacing: 2) {
                ForEach(Array(rooms.enumerated()), id: \.element.id) { i, r in
                    RoomRow(room: r, on: i == pick, game: game) { join(r) }
                }
            }
        }
        .frame(width: 360)
        .frame(maxHeight: 5 * (GameMetrics.tapeRow + 2))
        .fixedSize(horizontal: false, vertical: rooms.count <= 5)
    }
}

private struct RoomRow: View {
    let room: GameRoom
    let on: Bool
    let game: String
    let action: () -> Void
    @Environment(\.gameColors) private var colors
    @State private var hover = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: GameMetrics.sp3) {
                Text(room.name).font(.game(GameMetrics.fsBody)).foregroundColor(colors.ink.color)
                    .lineLimit(1).truncationMode(.tail).frame(maxWidth: .infinity, alignment: .leading)
                Text(game).font(.game(GameMetrics.fsSmall)).foregroundColor(colors.muted.color)
                Text(room.locked ? "🔒" : "").font(.game(GameMetrics.fsSmall)).frame(width: 1.4 * GameMetrics.fsBody, alignment: .trailing)
            }
            .padding(.horizontal, GameMetrics.sp3)
            .frame(height: GameMetrics.tapeRow)
            .background(RoundedRectangle(cornerRadius: GameMetrics.radiusS)
                .fill(on ? colors.fillSelected.color : hover ? colors.fillHover.color : .clear))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hover = $0 }
    }
}

/// `.lobby-caret`: a 1 px accent bar blinking once a second.
private struct Caret: View {
    @Environment(\.gameColors) private var colors

    var body: some View {
        TimelineView(.periodic(from: .now, by: 0.5)) { ctx in
            // .lobby-caret's blink stops under Reduce Motion, the caret left showing
            let on = GameMotion.reduced || Int(ctx.date.timeIntervalSinceReferenceDate * 2) % 2 == 0
            Rectangle().fill(colors.accent.color).frame(width: 1, height: GameMetrics.fsBody * 1.1).opacity(on ? 1 : 0)
        }
    }
}

/// FlowRow where there is one (macOS 13), and on 12 the same rows built from measured sizes.
struct WrapRow<Content: View>: View {
    var spacing: CGFloat
    @ViewBuilder var content: () -> Content

    var body: some View {
        if #available(macOS 13, *) {
            FlowRow(spacing: spacing) { content() }
        } else {
            _VariadicView.Tree(FlowRoot(spacing: spacing)) { content() }
        }
    }
}

/// Buttons in a row that wraps onto the next line when the panel is narrow (`flex-wrap: wrap`), centred.
@available(macOS 13, *)
struct FlowRow: Layout {
    var spacing: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let rows = arrange(proposal.width ?? .infinity, subviews)
        let h = rows.map(\.height).reduce(0, +) + spacing * CGFloat(max(0, rows.count - 1))
        let w = rows.map(\.width).max() ?? 0
        return CGSize(width: w, height: h)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var y = bounds.minY
        for row in arrange(bounds.width, subviews) {
            var x = bounds.midX - row.width / 2
            for i in row.items {
                let s = subviews[i].sizeThatFits(.unspecified)
                subviews[i].place(at: CGPoint(x: x, y: y + (row.height - s.height) / 2), proposal: .unspecified)
                x += s.width + spacing
            }
            y += row.height + spacing
        }
    }

    private func arrange(_ width: CGFloat, _ subviews: Subviews) -> [(items: [Int], width: CGFloat, height: CGFloat)] {
        var rows: [(items: [Int], width: CGFloat, height: CGFloat)] = []
        var cur: (items: [Int], width: CGFloat, height: CGFloat) = ([], 0, 0)
        for i in subviews.indices {
            let s = subviews[i].sizeThatFits(.unspecified)
            let add = cur.items.isEmpty ? s.width : cur.width + spacing + s.width
            if !cur.items.isEmpty && add > width {
                rows.append(cur)
                cur = ([i], s.width, s.height)
            } else {
                cur = (cur.items + [i], add, max(cur.height, s.height))
            }
        }
        if !cur.items.isEmpty { rows.append(cur) }
        return rows
    }
}

/// FlowRow on macOS 12 (no Layout protocol): each button's size and the row's width are measured a pass later,
/// then the buttons go into centred rows by the same rule as `FlowRow.arrange`.
private struct FlowRoot: _VariadicView_MultiViewRoot {
    var spacing: CGFloat

    func body(children: _VariadicView.Children) -> some View {
        FlowRows(children: children, spacing: spacing)
    }
}

private struct FlowRows: View {
    let children: _VariadicView.Children
    let spacing: CGFloat
    @State private var sizes: [Int: CGSize] = [:]
    @State private var width: CGFloat = .infinity

    var body: some View {
        VStack(spacing: spacing) {
            ForEach(Array(rows().enumerated()), id: \.offset) { _, row in
                HStack(alignment: .center, spacing: spacing) {
                    ForEach(row, id: \.self) { i in
                        children[i].fixedSize().background(GeometryReader { g in
                            Color.clear.preference(key: FlowSizes.self, value: [i: g.size])
                        })
                    }
                }
            }
        }
        .frame(maxWidth: .infinity)
        .background(GeometryReader { g in Color.clear.preference(key: FlowWidth.self, value: g.size.width) })
        .onPreferenceChange(FlowSizes.self) { sizes = $0 }
        .onPreferenceChange(FlowWidth.self) { width = $0 }
    }

    private func rows() -> [[Int]] {
        var rows: [[Int]] = []
        var cur: [Int] = []
        var curWidth: CGFloat = 0
        for i in children.indices {
            let w = sizes[i]?.width ?? 0
            let add = cur.isEmpty ? w : curWidth + spacing + w
            if !cur.isEmpty && add > width {
                rows.append(cur)
                cur = [i]
                curWidth = w
            } else {
                cur.append(i)
                curWidth = add
            }
        }
        if !cur.isEmpty { rows.append(cur) }
        return rows
    }
}

private struct FlowSizes: PreferenceKey {
    static var defaultValue: [Int: CGSize] = [:]
    static func reduce(value: inout [Int: CGSize], nextValue: () -> [Int: CGSize]) {
        value.merge(nextValue()) { $1 }
    }
}

private struct FlowWidth: PreferenceKey {
    static var defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = nextValue() }
}
