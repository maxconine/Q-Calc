import QCalcCore
import SwiftUI

// ember & frost: `ember` then enter (components/EmberPanel.tsx, EmberPanel.css). The shared lobby picks one keyboard
// or a room online; the core's EmberMatch runs the game (select screen, levels, clock, link, hud, progress) and
// this feeds it keys and frames, draws with EmberRenderer and plays its sounds.

enum EmberTheme {
    static func fire(_ dark: Bool) -> Color { dark ? Color(red: 1, green: 138 / 255, blue: 71 / 255) : Color(red: 232 / 255, green: 100 / 255, blue: 28 / 255) }
    static func frost(_ dark: Bool) -> Color { dark ? Color(red: 95 / 255, green: 205 / 255, blue: 242 / 255) : Color(red: 31 / 255, green: 147 / 255, blue: 198 / 255) }
    static func star(_ dark: Bool) -> Color { dark ? Color(red: 1, green: 196 / 255, blue: 77 / 255) : Color(red: 240 / 255, green: 168 / 255, blue: 26 / 255) }
}

/// the hint for the big window, as fullscreenHint(mac, on, windowed) words it
func gameFullscreenHint(_ on: Bool) -> String { on ? "⌃F or esc back to the bar" : "⌃F big window" }

/// the game that's running, shared by the lobby's handlers and the play view
final class EmberHolder {
    var match: EmberMatch?
    var screen = "select"
}

struct EmberPanel: View {
    @State private var holder = EmberHolder()
    @State private var done = EmberPanel.finished()

    private static func finished() -> Int {
        let m = EmberMatch.local(now: 0, progress: GameSaves.load(emberSaveKey()))
        return m.levels().filter { $0.best != nil }.count
    }

    var body: some View {
        let count = EmberMatch.local(now: 0, progress: nil).levels().count
        GamePanel {
            GameLobby(LobbyConfig(
                game: "ember", name: "ember & frost",
                blurb: "two of you, on one keyboard or two computers anywhere. both get to their doors. you’re ember if you host, frost if you join.",
                transport: RelayTransport.shared,
                online: { p in AnyView(EmberPlayView(holder: holder, props: p, onEnd: p.onEnd)) },
                onOpen: { role, _ in
                    holder.match = EmberMatch.online(host: role == .host, now: GameTime.now(), progress: GameSaves.load(emberSaveKey()))
                    holder.screen = "wait"
                },
                local: LocalGames(prompt: "", options: [], together: ["same keyboard"]) { _, onEnd in
                    holder.match = EmberMatch.local(now: GameTime.now(), progress: GameSaves.load(emberSaveKey()))
                    holder.screen = "select"
                    return AnyView(EmberPlayView(holder: holder, props: nil, onEnd: onEnd))
                },
                sub: done > 0 ? "\(done) of \(count) levels done" : "",
                footer: AnyView(EmberControls(count: count)),
                idle: { holder.screen != "play" },
                leave: {
                    guard holder.screen == "play", let m = holder.match else { return false }
                    m.menu()
                    return true
                },
                ownHead: true
            ))
        }
        .onAppear { done = EmberPanel.finished() }
    }
}

private struct Kbd: View {
    let k: String
    @Environment(\.gameColors) private var colors
    var body: some View {
        Text(k).font(.game(GameMetrics.fsCaption)).foregroundColor(colors.ink.color)
            .frame(minWidth: 18, minHeight: 18).padding(.horizontal, 3)
            .background(RoundedRectangle(cornerRadius: 4).fill(colors.bgTape.color))
            .overlay(RoundedRectangle(cornerRadius: 4).strokeBorder(colors.line.color, lineWidth: 1))
            .padding(.leading, 2)
    }
}

struct EmberControls: View {
    let count: Int
    @Environment(\.gameColors) private var colors

    var body: some View {
        VStack(spacing: GameMetrics.sp3) {
            Text("\(count) long levels, every one open: lights out, gravity upside down, trampolines, keys, and checkpoints on the way.")
                .font(.game(GameMetrics.fsSmall)).foregroundColor(colors.muted.color).multilineTextAlignment(.center)
                .frame(maxWidth: 460)
            HStack(spacing: GameMetrics.sp4) {
                HStack(spacing: 0) {
                    Text("ember ").font(.game(GameMetrics.fsSmall, .semibold)).foregroundColor(EmberTheme.fire(colors.dark))
                    Kbd(k: "W"); Kbd(k: "A"); Kbd(k: "S"); Kbd(k: "D")
                }
                HStack(spacing: 0) {
                    Text("frost ").font(.game(GameMetrics.fsSmall, .semibold)).foregroundColor(EmberTheme.frost(colors.dark))
                    Kbd(k: "←"); Kbd(k: "↑"); Kbd(k: "→"); Kbd(k: "↓")
                }
                HStack(spacing: 0) {
                    Kbd(k: "S"); Text(" / ").foregroundColor(colors.muted.color); Kbd(k: "↓")
                    Text(" use a lever, lamp or switch").foregroundColor(colors.muted.color)
                }.font(.game(GameMetrics.fsSmall))
            }
        }
    }
}

private let MOVE: [String: (Int, Int64)] = [
    "KeyA": (0, Int64(Ember.IN_LEFT)), "KeyD": (0, Int64(Ember.IN_RIGHT)), "KeyW": (0, Int64(Ember.IN_UP)), "KeyS": (0, Int64(Ember.IN_DOWN)),
    "ArrowLeft": (1, Int64(Ember.IN_LEFT)), "ArrowRight": (1, Int64(Ember.IN_RIGHT)), "ArrowUp": (1, Int64(Ember.IN_UP)), "ArrowDown": (1, Int64(Ember.IN_DOWN)),
]

/// EmberPlay's frame loop and keys
final class EmberModel: ObservableObject {
    @Published private(set) var frame: EmberFrame?
    @Published private(set) var cards: [EmberLevelCard] = []
    let match: EmberMatch
    let online: Bool
    let peer: String
    private let holder: EmberHolder
    private let transport: PeerTransport?
    private let onEnd: (String?) -> Void
    private var off: (() -> Void)?
    private var done = false
    private var renderer: EmberRenderer?
    private var stages: [String: EStage] = [:]
    private var draw: EmberDrawInput?
    private let synth = EmberSynth()
    private var held = Set<String>()
    private var asked: (from: Int, to: Int)?
    private var shown = ""
    let reduceMotion = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion

    init(holder: EmberHolder, props: GameProps?, onEnd: @escaping (String?) -> Void) {
        self.holder = holder
        match = holder.match ?? EmberMatch.local(now: GameTime.now(), progress: GameSaves.load(emberSaveKey()))
        holder.match = match
        online = props != nil
        peer = props?.peer ?? ""
        transport = props?.transport
        self.onEnd = onEnd
        cards = match.levels()
        off = transport?.subscribe { [weak self] e in
            guard let self else { return }
            switch e {
            case let .message(data): self.match.receive(text: data, now: GameTime.now())
            case let .closed(reason): self.finish(peerCloseText(reason: reason, word: self.transport?.peerWord ?? "friend"))
            default: break
            }
        }
    }

    deinit { stop() }

    func stop() {
        off?()
        off = nil
        synth.stop()
        if !done {
            done = true
            match.leave()
            for t in match.takeOutbox() { transport?.send(t) }
        }
        if holder.match === match { holder.match = nil }
    }

    private func finish(_ note: String?) {
        guard !done else { return }
        done = true
        off?()
        off = nil
        onEnd(note)
    }

    private func stage(_ level: UInt32, _ index: Int) -> EStage? {
        let k = "\(level)|\(index)"
        if let s = stages[k] { return s }
        let s = EmberJSON.stage(match.stageJson(level: level, stage: UInt32(index)))
        stages[k] = s
        return s
    }

    /// one display frame: tick, send, sounds, the hud, and what to draw
    func tick() {
        guard !done, let f = match.tick(now: GameTime.now()) else { return }
        for t in match.takeOutbox() { transport?.send(t) }
        if let ended = f.ended {
            finish(ended.isEmpty ? nil : ended)
            return
        }
        holder.screen = f.screen
        if f.progressChanged {
            GameSaves.store(emberSaveKey(), match.progressJson())
            cards = match.levels()
        }
        if f.resetRenderer { renderer?.reset() }
        synth.play(f.sounds)
        if f.playing, let cur = EmberJSON.state(f.stateJson), let st = stage(f.level, cur.stage) {
            let prev = EmberJSON.state(f.prevJson)
            draw = EmberDrawInput(level: st, prev: prev, cur: cur, world: EmberWorldNow(tiles: f.tiles, beams: f.beams, gates: f.gates),
                                  alpha: f.alpha, events: (draw?.events ?? []) + EmberJSON.events(f.eventsJson), nowMs: GameTime.now(),
                                  opts: DrawOpts(me: f.me.map(Int.init), reduceMotion: reduceMotion))
        } else {
            draw = nil
        }
        let h = f.hud
        let key = [f.screen, "\(f.level)", "\(f.me.map(String.init) ?? "")", f.note ?? "", h.phase ?? "", "\(h.stage)", "\(h.cp)", "\(h.deaths)", "\(h.keys)",
                   "\(h.gems)", "\(h.over)", "\(h.bannerId)", "\(h.showBanner)", "\(h.entry)", "\(h.score?.stars ?? -1)", h.clock, "\(f.muted)"].joined(separator: "|")
        if key != shown {
            shown = key
            frame = f
        }
    }

    /// the canvas asks for the picture
    func paint(_ g: Canvas2D, dark: Bool) {
        guard var d = draw else { return }
        if renderer == nil { renderer = EmberRenderer(dark: dark) }
        renderer?.refreshColors(dark: dark)
        d.nowMs = GameTime.now()
        renderer?.draw(g, d)
        // the events have been seen
        draw?.events = []
    }

    private func push() {
        var bits: [Int64] = [0, 0]
        for code in held {
            if let m = MOVE[code] { bits[m.0] |= m.1 }
        }
        if online, let me = frame?.me {
            match.keys(player: me, bits: bits[0] | bits[1])
        } else {
            match.keys(player: 0, bits: bits[0])
            match.keys(player: 1, bits: bits[1])
        }
    }

    private func pick(_ dx: Int64, _ dy: Int64) {
        let level = Int(frame?.level ?? 0)
        let base = asked.flatMap { $0.from == level ? $0.to : nil } ?? level
        let next = Int(emberGridStep(from: Int64(base), dx: dx, dy: dy, count: Int64(cards.count), cols: 5))
        if next == base { return }
        asked = (level, next)
        match.pick(index: Int64(next))
    }

    func choose(_ i: Int) {
        match.pick(index: Int64(i))
        match.play()
    }

    func key(_ k: GameKey) -> Bool {
        if k.key == "Escape" { return false }
        let move = MOVE[k.code]
        if !k.down {
            guard held.remove(k.code) != nil else { return false }
            push()
            return true
        }
        if !k.plain && !(move != nil && held.contains(k.code)) { return false }
        let screen = holder.screen
        if move != nil && !held.contains(k.code) {
            held.insert(k.code)
            push()
        }
        let barKey = k.isCharacter || k.key == "Enter" || k.key.hasPrefix("Arrow") || k.key == "Backspace"
        if screen == "select" {
            if let move {
                switch Int(move.1) {
                case Ember.IN_LEFT: pick(-1, 0)
                case Ember.IN_RIGHT: pick(1, 0)
                case Ember.IN_UP: pick(0, -1)
                default: pick(0, 1)
                }
            } else if k.key == "Home" {
                pick(-Int64(cards.count), 0)
            } else if k.key == "End" {
                pick(Int64(cards.count), 0)
            } else if (k.key == "Enter" || k.key == " ") && !k.isRepeat {
                match.play()
            } else if k.key.lowercased() == "m" && !k.isRepeat {
                match.toggleMute()
            }
            return true
        }
        if screen != "play" { return barKey }
        if move != nil { return true }
        if !barKey { return false }
        if k.isRepeat { return true }
        let key = k.key.lowercased()
        if key == "r" {
            if k.shift { match.restart() } else { match.retry() }
        } else if key == "m" {
            match.toggleMute()
        } else if (k.key == "Enter" || k.key == " ") && frame?.hud.phase == "won" {
            match.next()
        }
        return true
    }

    func blur() {
        held.removeAll()
        match.release()
    }
}

struct EmberPlayView: View {
    @StateObject private var model: EmberModel
    @EnvironmentObject private var shell: GameShellModel
    @Environment(\.gameColors) private var colors

    init(holder: EmberHolder, props: GameProps?, onEnd: @escaping (String?) -> Void) {
        _model = StateObject(wrappedValue: EmberModel(holder: holder, props: props, onEnd: onEnd))
    }

    private var screen: String { model.frame?.screen ?? (model.online ? "wait" : "select") }

    var body: some View {
        ZStack(alignment: .top) {
            if screen == "select" {
                VStack(alignment: .leading, spacing: 0) {
                    EmberHead(sub: model.online ? "with \(model.peer)" : "same keyboard")
                    EmberSelect(model: model)
                }
            } else if screen == "wait" {
                VStack(spacing: 0) {
                    EmberHead(sub: model.online ? "with \(model.peer)" : "")
                    PongNote(model.online ? "waiting for \(model.peer)…" : "getting ready…", wait: true)
                        .font(.game(GameMetrics.fsBody))
                        .frame(maxWidth: .infinity, minHeight: 168)
                }
            }
            EmberPlayColumn(model: model, playing: screen == "play")
                .frame(height: screen == "play" ? nil : 1)
                .opacity(screen == "play" ? 1 : 0)
                .allowsHitTesting(screen == "play")
        }
        .gameKeys(shell) { k in model.key(k) }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didResignActiveNotification)) { _ in model.blur() }
        .onDisappear { model.stop() }
    }
}

struct EmberHead: View {
    let sub: String
    @EnvironmentObject private var shell: GameShellModel
    @Environment(\.gameColors) private var colors

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: GameMetrics.sp3) {
            (Text("ember").foregroundColor(EmberTheme.fire(colors.dark)) + Text(" & ").fontWeight(.regular).foregroundColor(colors.muted.color)
                + Text("frost").foregroundColor(EmberTheme.frost(colors.dark)))
                .font(.game(GameMetrics.fsBody, .semibold))
            if !sub.isEmpty { Text(sub).font(.game(GameMetrics.fsSmall)).foregroundColor(colors.muted.color) }
            Spacer(minLength: 0)
            HStack(spacing: 0) {
                FullscreenButton(on: shell.fullscreen) { shell.toggleFullscreen() }
                    .alignmentGuide(.firstTextBaseline) { d in d[VerticalAlignment.center] + 4 }
                EscButton { shell.escape() }.padding(.trailing, -GameMetrics.sp2)
            }
        }
    }
}

// ---- the select screen

private struct Stars: View {
    let n: Int
    var pop = false
    @Environment(\.gameColors) private var colors
    @State private var shown = false

    var body: some View {
        HStack(spacing: pop ? 6 : 1) {
            ForEach(0..<3, id: \.self) { i in
                Text("★")
                    .foregroundColor(i < n ? EmberTheme.star(colors.dark) : colors.muted.color.opacity(0.45))
                    .scaleEffect(pop && i < n && !shown ? 0.2 : 1)
                    .rotationEffect(.degrees(pop && i < n && !shown ? -30 : 0))
                    .opacity(pop && i < n && !shown ? 0 : 1)
                    .animation(pop ? .interpolatingSpring(stiffness: 300, damping: 12).delay(0.18 + Double(i) * 0.16) : nil, value: shown)
            }
        }
        .font(.game(pop ? 30 : GameMetrics.fsCaption))
        .onAppear { shown = true }
    }
}

private struct Pips: View {
    let n: Int
    @Environment(\.gameColors) private var colors

    var body: some View {
        HStack(spacing: 2) {
            ForEach(0..<5, id: \.self) { i in
                RoundedRectangle(cornerRadius: 1)
                    .fill(i < n ? EmberTheme.fire(colors.dark) : colors.muted.color.opacity(0.3))
                    .frame(width: 3, height: 9)
            }
        }
    }
}

final class EmberThumbs {
    static var cache: [String: CGImage] = [:]

    static func image(_ match: EmberMatch, _ index: Int, _ stages: Int, dark: Bool, scale: CGFloat) -> CGImage? {
        let k = min(4, 2 * max(1, Int(scale.rounded())))
        let id = "\(index)|\(dark)|\(k)"
        if let img = cache[id] { return img }
        guard let st = EmberJSON.stage(match.stageJson(level: UInt32(index), stage: 0)) else { return nil }
        let img = renderPicture(width: 144 * k, height: 81 * k) { drawEmberThumb($0, st, stages: stages, dark: dark) }
        cache[id] = img
        return img
    }
}

struct EmberSelect: View {
    @ObservedObject var model: EmberModel
    @Environment(\.gameColors) private var colors
    @Environment(\.displayScale) private var scale
    @State private var hover: (i: Int, under: Int)?

    private func fmtDeaths(_ d: Int64) -> String { d == 0 ? "no deaths" : d == 1 ? "1 death" : "\(d) deaths" }

    var body: some View {
        let level = Int(model.frame?.level ?? 0)
        let cards = model.cards
        let shown = hover.flatMap { $0.under == level && cards.indices.contains($0.i) ? $0.i : nil } ?? level
        let muted = model.frame?.muted ?? false
        VStack(spacing: GameMetrics.sp2) {
            ScrollViewReader { proxy in
                ScrollView(.vertical) {
                    LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: GameMetrics.sp1), count: 5), spacing: GameMetrics.sp1) {
                        ForEach(Array(cards.enumerated()), id: \.offset) { i, c in
                            card(i, c, on: i == level)
                                .id(i)
                                .onHover { inside in if inside { hover = (i, level) } else if hover?.i == i { hover = nil } }
                                .onTapGesture { model.choose(i) }
                        }
                    }
                    .padding(4)
                }
                .frame(maxHeight: 372)
                .padding(.horizontal, -4)
                .onChange(of: level) { l in withAnimation { proxy.scrollTo(l) } }
            }
            if cards.indices.contains(shown) { detail(shown, cards[shown]) }
            let note = model.frame?.note.map { "\($0) · " } ?? ""
            let you = model.frame?.me.map { $0 == 0 ? "you’re ember · " : "you’re frost · " } ?? ""
            PongNote("\(note)\(you)every level is open · arrows or WASD pick · ↵ play · M \(muted ? "unmute" : "mute") · \(gameFullscreenHint(false)) · esc back")
        }
        .padding(.top, GameMetrics.sp2)
    }

    private func card(_ i: Int, _ c: EmberLevelCard, on: Bool) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            ZStack(alignment: .bottomTrailing) {
                if let img = EmberThumbs.image(model.match, i, Int(c.stages), dark: colors.dark, scale: scale) {
                    Image(decorative: img, scale: 1).resizable().aspectRatio(640 / 360, contentMode: .fit)
                } else {
                    Rectangle().fill(colors.bg.mix(0.6, colors.bgTape).color).aspectRatio(640 / 360, contentMode: .fit)
                }
                if c.stages > 1 {
                    Text("\(c.stages) stages").font(.game(10)).foregroundColor(colors.ink.color)
                        .padding(.horizontal, 5).frame(height: 15)
                        .background(Capsule().fill(colors.bg.opacity(0.82).color)).padding(3)
                }
            }
            .clipShape(RoundedRectangle(cornerRadius: 4))
            .overlay(RoundedRectangle(cornerRadius: 4).strokeBorder(colors.line.color, lineWidth: 1))
            (Text("\(i + 1)").fontWeight(.semibold).foregroundColor(colors.muted.color) + Text(" \(c.name)").foregroundColor(colors.ink.color))
                .font(.gameDigits(GameMetrics.fsSmall)).lineLimit(1).truncationMode(.tail)
            HStack(alignment: .firstTextBaseline, spacing: GameMetrics.sp2) {
                Stars(n: Int(c.best?.stars ?? 0))
                if let t = c.best?.secsText { Text(t) }
                Spacer(minLength: 0)
                Pips(n: Int(c.difficulty)).alignmentGuide(.firstTextBaseline) { d in d[VerticalAlignment.center] + 3 }
            }
            .font(.gameDigits(GameMetrics.fsCaption)).foregroundColor(colors.muted.color).frame(minHeight: 15)
        }
        .padding(EdgeInsets(top: 5, leading: 5, bottom: 6, trailing: 5))
        .background(RoundedRectangle(cornerRadius: GameMetrics.radiusS).fill(on ? colors.fillSelected.color : hover?.i == i ? colors.fillHover.color : .clear))
        .overlay(RoundedRectangle(cornerRadius: GameMetrics.radiusS).strokeBorder(on ? EmberTheme.fire(colors.dark).opacity(0.55) : .clear, lineWidth: 1.5))
        .contentShape(Rectangle())
    }

    private func detail(_ i: Int, _ c: EmberLevelCard) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: GameMetrics.sp3) {
            (Text("\(i + 1)").foregroundColor(colors.muted.color) + Text(" \(c.name)").foregroundColor(colors.ink.color)).fontWeight(.semibold)
            Pips(n: Int(c.difficulty)).alignmentGuide(.firstTextBaseline) { d in d[VerticalAlignment.center] + 3 }
            Text(c.length)
            Text("target \(c.parText)")
            if let b = c.best {
                HStack(spacing: 4) {
                    Text("best \(b.secsText ?? "–")")
                    Stars(n: Int(b.stars))
                }.foregroundColor(colors.ink.color)
                if c.gemTotal > 0 { Text("gems \(min(b.gems, c.gemTotal))/\(c.gemTotal)") }
                if let d = b.deaths { Text(fmtDeaths(d)) }
            } else {
                Text("not finished yet\(c.gemTotal > 0 ? " · \(c.gemTotal) gems" : "")")
            }
        }
        .font(.gameDigits(GameMetrics.fsSmall)).foregroundColor(colors.muted.color)
        .frame(maxWidth: .infinity, minHeight: 18)
    }
}

// ---- the play screen

struct EmberPlayColumn: View {
    @ObservedObject var model: EmberModel
    let playing: Bool
    @EnvironmentObject private var shell: GameShellModel
    @Environment(\.gameColors) private var colors

    var body: some View {
        let f = model.frame
        let h = f?.hud
        let card = f.flatMap { model.cards.indices.contains(Int($0.level)) ? model.cards[Int($0.level)] : nil }
        VStack(spacing: GameMetrics.sp2) {
            if let h, let f { hud(f, h, card) }
            ZStack {
                GameCanvas(frame: { model.tick() }, draw: { g, dark in model.paint(g, dark: dark) })
                    .aspectRatio(640 / 360, contentMode: .fit)
                if let h, let f { overlays(f, h, card) }
            }
            .background(colors.bg.mix(0.7, colors.bgTape).color)
            .clipShape(RoundedRectangle(cornerRadius: GameMetrics.radiusS))
            .overlay(RoundedRectangle(cornerRadius: GameMetrics.radiusS).strokeBorder(colors.line.color, lineWidth: 1))
            let muted = f?.muted ?? false
            Text("\(model.online ? "WASD or arrows" : "ember WASD · frost arrows") · S/↓ use · R checkpoint · ⇧R restart level · M \(muted ? "unmute" : "mute") · \(gameFullscreenHint(shell.fullscreen))\(shell.fullscreen ? "" : " · esc levels")")
                .font(.game(GameMetrics.fsSmall)).foregroundColor(colors.muted.color).lineLimit(1).truncationMode(.tail)
        }
    }

    private func hud(_ f: EmberFrame, _ h: EmberHud, _ card: EmberLevelCard?) -> some View {
        let dark = colors.dark
        return HStack(spacing: GameMetrics.sp3) {
            (Text("\(f.level + 1)").fontWeight(.semibold).foregroundColor(colors.muted.color) + Text(" \(h.levelName)"))
                .lineLimit(1).truncationMode(.tail)
            if h.stages > 1 { chip("stage \(h.stage + 1)/\(h.stages)", on: false) }
            if h.cps > 0 { chip("⚑ \(max(0, h.cp + 1))/\(h.cps)", on: h.cp >= 0).id("cp\(h.stage).\(h.cp)") }
            if h.total.reduce(0, +) > 0 { gems(h.gems, h.total) }
            if h.keys > 0 {
                HStack(spacing: 3) {
                    KeyGlyph().frame(width: 15, height: 9)
                    if h.keys > 1 { Text("×\(h.keys)") }
                }.foregroundColor(EmberTheme.star(dark))
            }
            if h.deaths > 0 { Text(h.deaths == 1 ? "1 death" : "\(h.deaths) deaths").foregroundColor(colors.muted.color) }
            if let me = f.me {
                Text("you’re \(me == 0 ? "ember" : "frost")").fontWeight(.semibold).foregroundColor(me == 0 ? EmberTheme.fire(dark) : EmberTheme.frost(dark))
            }
            Spacer(minLength: 0)
            (Text(h.clock).fontWeight(.semibold).foregroundColor(h.over ? colors.muted.color : colors.ink.color)
                + Text(card != nil ? " / \(h.parText)" : "").foregroundColor(colors.muted.color))
                .font(.gameDigits(GameMetrics.fsSmall))
        }
        .font(.game(GameMetrics.fsSmall))
        .foregroundColor(colors.ink.color)
        .frame(minHeight: 20)
    }

    private func chip(_ s: String, on: Bool) -> some View {
        ChipView(text: s, on: on)
    }

    private func gems(_ got: [Int64], _ total: [Int64]) -> some View {
        let fire = EmberTheme.fire(colors.dark), frost = EmberTheme.frost(colors.dark)
        return Group {
            if total[0] + total[1] <= 8 {
                HStack(spacing: 3) {
                    ForEach(0..<Int(total[0]), id: \.self) { i in GemGlyph(color: fire, got: i < got[0]) }
                    ForEach(0..<Int(total[1]), id: \.self) { i in GemGlyph(color: frost, got: i < got[1]) }
                }
            } else {
                HStack(spacing: GameMetrics.sp2) {
                    if total[0] > 0 { HStack(spacing: 5) { GemGlyph(color: fire, got: got[0] == total[0]); Text("\(got[0])/\(total[0])") }.foregroundColor(fire) }
                    if total[1] > 0 { HStack(spacing: 5) { GemGlyph(color: frost, got: got[1] == total[1]); Text("\(got[1])/\(total[1])") }.foregroundColor(frost) }
                }.font(.gameDigits(GameMetrics.fsSmall))
            }
        }
    }

    @ViewBuilder
    private func overlays(_ f: EmberFrame, _ h: EmberHud, _ card: EmberLevelCard?) -> some View {
        ZStack {
            if h.showBanner, h.bannerId > 0 {
                VStack {
                    EmberBannerView(kind: h.bannerKind ?? "hint", title: h.bannerTitle, text: h.bannerText).id(h.bannerId)
                    Spacer()
                }.padding(.top, GameMetrics.sp3)
            }
            if h.phase == "clear" {
                EmberCard {
                    WinTitle(text: h.clearTitle)
                    Text(h.clearNote).font(.game(GameMetrics.fsSmall)).foregroundColor(colors.muted.color)
                }
            }
            if h.phase == "won", let sc = h.score, card != nil {
                EmberCard {
                    WinTitle(text: "both home")
                    Stars(n: Int(sc.stars), pop: true)
                    winLine(sc, card!, h)
                    if let new = h.winNew { Text(new).font(.game(GameMetrics.fsSmall, .semibold)).foregroundColor(colors.accent.color) }
                    Text(h.winFoot).font(.game(GameMetrics.fsSmall)).foregroundColor(colors.muted.color)
                }
            }
            if let note = f.note {
                VStack {
                    Spacer()
                    Text(note).font(.game(GameMetrics.fsSmall)).foregroundColor(colors.muted.color)
                        .lineLimit(1).padding(.vertical, 5).padding(.horizontal, GameMetrics.sp3)
                        .background(Capsule().fill(colors.bg.color)).boxShadow(colors.cardShadow, radius: 999)
                }.padding(.bottom, GameMetrics.sp3)
            }
            if playing {
                VStack {
                    Spacer()
                    HStack {
                        Spacer()
                        Text(gameFullscreenHint(shell.fullscreen)).font(.game(GameMetrics.fsCaption)).foregroundColor(colors.muted.color)
                            .padding(.vertical, 2).padding(.horizontal, GameMetrics.sp2)
                            .background(Capsule().fill(colors.bg.opacity(0.78).color))
                            .modifier(FadeAway(seconds: 5))
                            .id("fs\(h.entry)")
                    }
                }.padding(GameMetrics.sp2)
            }
        }
        .allowsHitTesting(false)
    }

    private func winLine(_ sc: EmberScoreView, _ card: EmberLevelCard, _ h: EmberHud) -> some View {
        let got = sc.gems[0] + sc.gems[1], all = sc.total[0] + sc.total[1]
        var t = Text(emberFmtTime(secs: sc.secs)).foregroundColor(sc.secs <= card.par ? colors.accent.color : colors.ink.color)
            .fontWeight(sc.secs <= card.par ? .semibold : .regular)
        t = t + Text(" / \(card.parText)").foregroundColor(colors.muted.color)
        if all > 0 {
            t = t + Text(" · ") + Text("gems \(got)/\(all)").foregroundColor(got == all ? colors.accent.color : colors.ink.color).fontWeight(got == all ? .semibold : .regular)
        }
        t = t + Text(" · \(h.deaths == 0 ? "no deaths" : h.deaths == 1 ? "1 death" : "\(h.deaths) deaths")")
        return t.font(.gameDigits(GameMetrics.fsBody)).foregroundColor(colors.ink.color)
    }
}

private struct ChipView: View {
    let text: String
    let on: Bool
    @Environment(\.gameColors) private var colors
    @State private var big = false

    var body: some View {
        Text(text).font(.gameDigits(GameMetrics.fsCaption)).padding(.horizontal, 6).frame(height: 18)
            .foregroundColor(on ? colors.accent.color : colors.muted.color)
            .background(Capsule().fill(on ? colors.accent.color.opacity(0.16) : colors.fillTrack.color))
            .scaleEffect(big ? 1.25 : 1)
            .onAppear {
                guard on else { return }
                withAnimation(.easeOut(duration: 0.21)) { big = true }
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.21) { withAnimation(.easeInOut(duration: 0.49)) { big = false } }
            }
    }
}

private struct GemGlyph: View {
    let color: Color
    let got: Bool
    var body: some View {
        RoundedRectangle(cornerRadius: 1)
            .fill(got ? color : .clear)
            .overlay(RoundedRectangle(cornerRadius: 1).strokeBorder(color, lineWidth: 1.5))
            .frame(width: 7, height: 7).rotationEffect(.degrees(45)).opacity(got ? 1 : 0.55)
    }
}

private struct KeyGlyph: View {
    var body: some View {
        Canvas { ctx, _ in
            ctx.stroke(Path(ellipseIn: CGRect(x: 1, y: 1, width: 7, height: 7).insetBy(dx: 1, dy: 1)), with: .foreground, lineWidth: 2)
            ctx.fill(Path(roundedRect: CGRect(x: 7, y: 3.5, width: 8, height: 2), cornerRadius: 1), with: .foreground)
            ctx.fill(Path(CGRect(x: 11.5, y: 5, width: 2, height: 2.5)), with: .foreground)
        }
    }
}

private struct WinTitle: View {
    let text: String
    @Environment(\.gameColors) private var colors
    var body: some View {
        Text(text).font(.game(18, .bold)).tracking(-0.18).foregroundColor(.clear)
            .overlay(LinearGradient(colors: [EmberTheme.fire(colors.dark), EmberTheme.frost(colors.dark)], startPoint: .leading, endPoint: .trailing)
                .mask(Text(text).font(.game(18, .bold)).tracking(-0.18)))
    }
}

private struct EmberCard<Content: View>: View {
    @ViewBuilder var content: Content
    @Environment(\.gameColors) private var colors
    @State private var shown = false

    var body: some View {
        VStack(spacing: GameMetrics.sp2) { content }
            .padding(.vertical, GameMetrics.sp4).padding(.horizontal, GameMetrics.sp5)
            .frame(minWidth: 260)
            .background(RoundedRectangle(cornerRadius: 10).fill(colors.bg.opacity(0.92).color))
            .boxShadow(colors.cardShadow, radius: 10)
            .shadow(color: .black.opacity(0.18), radius: 15, y: 10)
            .scaleEffect(shown ? 1 : 0.9).opacity(shown ? 1 : 0)
            .onAppear { withAnimation(.interpolatingSpring(stiffness: 260, damping: 18)) { shown = true } }
    }
}

/// .ember-banner: in at the top, held, then gone (4.2 s; 2.4 s for a flag or the way back; a death's stays)
struct EmberBannerView: View {
    let kind: String
    let title: String?
    let text: String
    @Environment(\.gameColors) private var colors
    @State private var start = Date()

    var body: some View {
        TimelineView(.animation) { ctx in
            let e = ctx.date.timeIntervalSince(start)
            let (o, dy) = look(e)
            label.opacity(o).offset(y: dy)
        }
        .onAppear { start = Date() }
    }

    private func look(_ e: Double) -> (Double, Double) {
        if kind == "ouch" {
            let k = min(1, e / 0.18)
            return (k, 4 * (1 - k))
        }
        let d = kind == "flag" || kind == "back" ? 2.4 : 4.2
        let p = e / d
        if p < 0.08 { let k = p / 0.08; return (k, -6 * (1 - k)) }
        if p < 0.75 { return (1, 0) }
        return (max(0, 1 - (p - 0.75) / 0.25), 0)
    }

    private var label: some View {
        var t = Text("")
        if kind == "flag" { t = Text("⚑ ") }
        if let title { t = t + Text(title).fontWeight(.semibold) }
        if title != nil && !text.isEmpty { t = t + Text(" · ").foregroundColor(colors.muted.color) }
        t = t + Text(text)
        return t.font(.game(GameMetrics.fsBody, kind == "flag" ? .semibold : .regular))
            .foregroundColor(kind == "flag" ? colors.accent.color : colors.ink.color)
            .lineLimit(1).truncationMode(.tail)
            .padding(.vertical, 5).padding(.horizontal, GameMetrics.sp3)
            .background(Capsule().fill(colors.bg.color))
            .boxShadow(colors.cardShadow, radius: 999)
            .padding(.horizontal, GameMetrics.sp3)
    }
}

/// the fullscreen tip in the corner: there for a few seconds, then gone
struct FadeAway: ViewModifier {
    let seconds: Double
    @State private var gone = false
    func body(content: Content) -> some View {
        content.opacity(gone ? 0 : 1)
            .onAppear { withAnimation(.easeOut(duration: 0.6).delay(seconds)) { gone = true } }
    }
}
