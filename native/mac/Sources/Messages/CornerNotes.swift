import AppKit
import SwiftUI

// The note in the corner when a message, a request or a file comes while the bar is away, in place of a macOS
// notification, as the Windows app's notice.rs: raw text on the bar's own colours, the sender in ink, then their
// words (at most three lines), "sent you notes.pdf (4.2 MB)", or "wants to message you". Opaque, no fade, rounded
// and shadowed as the bar is, in the top-right corner of the screen the pointer is on.
//
// - It never takes the keys: a non-activating panel, so whatever is in front keeps the focus, even while it's
//   clicked. Not in the Dock, the app switcher or Mission Control.
// - Several stack newest on top, at most three; a newer one from the same name takes the place of theirs. Each
//   goes 6 s after it came, none while the pointer rests on them (3 s more once it leaves). A click opens the bar
//   on that person's thread (a request's card for someone new); × (on hover) lets one go. The bar showing lets
//   them all go: what they said is in it now.
// - Quiet over a full-screen app or game and while the screen is locked; nothing is lost, the message waits in
//   the inbox for the bar either way. macOS gives no way to read Focus without its permission, so Focus doesn't
//   quiet them; Settings › Messages turns them off.
// - Nothing is made until a note shows; the panel goes with the last one.
final class CornerNotes: ObservableObject {
    static let shared = CornerNotes()
    static let onKey = "qcalc.notes"
    static let textKey = "qcalc.notesText"

    struct Note: Identifiable, Equatable {
        let id = UUID()
        let from: String
        let ask: Bool
        let beside: String
        let body: String
        var until: Date
    }

    static let width: CGFloat = 340
    private static let margin: CGFloat = 12
    private static let most = 3
    private static let life: TimeInterval = 6
    private static let afterLeave: TimeInterval = 3

    @Published private(set) var notes: [Note] = []
    @Published var hovered: UUID?
    /// opens the bar, on `from`'s thread unless it's a request (OverlayController)
    var onOpen: ((_ from: String, _ ask: Bool) -> Void)?
    private var panel: NSPanel?
    private var timer: Timer?
    private var resting = false

    static var on: Bool { UserDefaults.standard.object(forKey: onKey) as? Bool ?? true }
    static var textShown: Bool { UserDefaults.standard.object(forKey: textKey) as? Bool ?? true }

    /// something came from `from` while the bar is away: `body` is their words or what they sent, `ask` a request
    func post(from: String, body: String = "", ask: Bool = false, sharing: Bool = false) {
        guard Self.on, !Messages.shared.barOpen else { return }
        if let why = Self.quiet() {
            OverlayLog.note("note: quiet (\(why)), \(from)'s waits for the bar")
            return
        }
        let beside = ask ? (sharing ? "wants to share with you" : "wants to message you")
            : Self.textShown ? "" : "sent you a message"
        let words = ask || !Self.textShown ? "" : body
        // a newer one from the same name takes the place of theirs (a request stays a request)
        if ask, notes.contains(where: { $0.from == from && $0.ask }) { return }
        notes.removeAll { $0.from == from && !$0.ask }
        notes.insert(Note(from: from, ask: ask, beside: beside, body: words, until: Date() + Self.life), at: 0)
        notes = Array(notes.prefix(Self.most))
        sync()
    }

    /// the bar is showing: what the notes said is in it now
    func clear() {
        guard !notes.isEmpty else { return }
        notes = []
        sync()
    }

    func dismiss(_ id: UUID) {
        notes.removeAll { $0.id == id }
        sync()
    }

    func open(_ note: Note) {
        OverlayLog.note("note: opened \(note.from)'s")
        notes = []
        sync()
        onOpen?(note.from, note.ask)
    }

    /// the pointer on them holds every note; once it leaves each has 3 s more at least
    func rest(_ on: Bool) {
        resting = on
        if !on {
            let soonest = Date() + Self.afterLeave
            for i in notes.indices where notes[i].until < soonest { notes[i].until = soonest }
        }
        arm()
    }

    // the panel made, sized and placed for the notes as they are, or gone with the last
    private func sync() {
        guard !notes.isEmpty else {
            timer?.invalidate()
            panel?.orderOut(nil)
            panel = nil
            resting = false
            hovered = nil
            return
        }
        let panel = panel ?? make()
        self.panel = panel
        panel.appearance = AppSettings.shared.nsAppearance
        // sized once SwiftUI has drawn the notes as they are now
        DispatchQueue.main.async { [weak self] in self?.place(panel) }
        arm()
    }

    private func place(_ panel: NSPanel) {
        guard panel === self.panel, let host = panel.contentView else { return }
        host.layoutSubtreeIfNeeded()
        let size = NSSize(width: Self.width, height: ceil(host.fittingSize.height))
        let screen = NSScreen.screens.first { NSMouseInRect(NSEvent.mouseLocation, $0.frame, false) } ?? NSScreen.main
        let area = screen?.visibleFrame ?? .zero
        let origin = NSPoint(x: area.maxX - size.width - Self.margin, y: area.maxY - size.height - Self.margin)
        panel.setFrame(NSRect(origin: origin, size: size), display: true)
        panel.orderFrontRegardless()
        panel.invalidateShadow()
    }

    private func make() -> NSPanel {
        let panel = NSPanel(contentRect: NSRect(x: 0, y: 0, width: Self.width, height: 60),
                            styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: true)
        panel.level = .statusBar
        panel.isFloatingPanel = true
        panel.hidesOnDeactivate = false
        panel.becomesKeyOnlyIfNeeded = true
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.animationBehavior = .none
        panel.isReleasedWhenClosed = false
        panel.collectionBehavior = [.canJoinAllSpaces, .stationary, .ignoresCycle, .fullScreenNone]
        panel.contentView = NSHostingView(rootView: CornerNotesView(notes: self))
        return panel
    }

    // one timer, for the soonest to go; none while the pointer rests on them
    private func arm() {
        timer?.invalidate()
        guard !resting, let soonest = notes.map(\.until).min() else { return }
        timer = Timer.scheduledTimer(withTimeInterval: max(0.05, soonest.timeIntervalSinceNow), repeats: false) { [weak self] _ in
            guard let self, !self.resting else { return }
            let now = Date()
            self.notes.removeAll { $0.until <= now }
            self.sync()
        }
    }

    /// why a note isn't shown now: the screen locked, or a full-screen window in front
    private static func quiet() -> String? {
        if let session = CGSessionCopyCurrentDictionary() as? [String: Any],
           session["CGSSessionScreenIsLocked"] as? Bool == true {
            return "locked"
        }
        // the frontmost ordinary window (layer 0) covering a whole screen, menu bar and all
        let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
            as? [[String: Any]] ?? []
        let mine = ProcessInfo.processInfo.processIdentifier
        guard let front = windows.first(where: { ($0[kCGWindowLayer as String] as? Int) == 0 }),
              (front[kCGWindowOwnerPID as String] as? pid_t) != mine,
              let bounds = front[kCGWindowBounds as String] as? NSDictionary,
              let rect = CGRect(dictionaryRepresentation: bounds) else { return nil }
        return NSScreen.screens.contains { $0.frame.size == rect.size } ? "full screen" : nil
    }
}

private struct CornerNotesView: View {
    @ObservedObject var notes: CornerNotes

    var body: some View {
        VStack(spacing: 0) {
            ForEach(Array(notes.notes.enumerated()), id: \.element.id) { i, note in
                if i > 0 { Rectangle().fill(Color.qLine).frame(height: 1) }
                row(note)
            }
        }
        .frame(width: CornerNotes.width)
        .background(Color.qBg)
        .clipShape(RoundedRectangle(cornerRadius: Theme.radius, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Theme.radius, style: .continuous).strokeBorder(Color.qLine))
        .onHover { notes.rest($0) }
    }

    private func row(_ note: CornerNotes.Note) -> some View {
        let hovered = notes.hovered == note.id
        return VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 6) {
                Text(note.from)
                    .font(.system(size: Theme.body, weight: .semibold))
                    .foregroundStyle(Color.qInk)
                    .lineLimit(1)
                if !note.beside.isEmpty {
                    Text(note.beside)
                        .font(.system(size: Theme.body))
                        .foregroundStyle(Color.qMuted)
                        .lineLimit(1)
                }
                Spacer(minLength: 20)
            }
            if !note.body.isEmpty {
                Text(note.body)
                    .font(.system(size: Theme.body))
                    .foregroundStyle(Color.qAnswer)
                    .lineLimit(3)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.horizontal, Theme.gutter)
        .padding(.vertical, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(hovered ? Color.qFillHover : .clear)
        .overlay(alignment: .topTrailing) {
            if hovered {
                Button { notes.dismiss(note.id) } label: {
                    Text("×")
                        .font(.system(size: Theme.body))
                        .foregroundStyle(Color.qMuted)
                        .frame(width: 20, height: 20)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .padding(.top, 8)
                .padding(.trailing, 8)
                .accessibilityLabel("Dismiss")
            }
        }
        .contentShape(Rectangle())
        .onTapGesture { notes.open(note) }
        .onHover { on in
            if on { notes.hovered = note.id } else if notes.hovered == note.id { notes.hovered = nil }
        }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isButton)
    }
}
