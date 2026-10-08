import AppKit

// one sheet, as the web page sends it; the identities themselves live in src/lib/identities.ts
struct IdentitySheet {
    struct Run {
        let text: String
        // "sup", "sub", "supsub" (a subscript inside a superscript) or "" for the baseline
        let shift: String
    }

    struct Item {
        let runs: [Run]
        let copy: String
        let note: String
    }

    struct Group {
        let title: String
        let items: [Item]
    }

    struct Section {
        let title: String
        let groups: [Group]
    }

    let id: String
    let title: String
    let sections: [Section]

    init?(_ d: [String: Any]) {
        guard let id = d["id"] as? String, let title = d["title"] as? String else { return nil }
        self.id = id
        self.title = title
        sections = (d["sections"] as? [Any] ?? []).compactMap { any in
            guard let s = any as? [String: Any], let title = s["title"] as? String else { return nil }
            let groups: [Group] = (s["groups"] as? [Any] ?? []).compactMap { any in
                guard let g = any as? [String: Any], let title = g["title"] as? String else { return nil }
                let items: [Item] = (g["items"] as? [Any] ?? []).compactMap { any in
                    guard let i = any as? [String: Any], let copy = i["copy"] as? String else { return nil }
                    let runs: [Run] = (i["runs"] as? [Any] ?? []).compactMap { any in
                        guard let r = any as? [String: Any], let text = r["text"] as? String else { return nil }
                        return Run(text: text, shift: r["shift"] as? String ?? "")
                    }
                    return runs.isEmpty ? nil : Item(runs: runs, copy: copy, note: i["note"] as? String ?? "")
                }
                return items.isEmpty ? nil : Group(title: title, items: items)
            }
            return groups.isEmpty ? nil : Section(title: title, groups: groups)
        }
        if sections.isEmpty { return nil }
    }
}

// the scrolled content: sections down the page, each one's groups in two columns
private final class IdentitySheetView: NSView {
    override var isFlipped: Bool { true }
    override var mouseDownCanMoveWindow: Bool { false }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
}

// the window's own content, flipped so the header stays pinned to the top as it resizes
private final class IdentityRoot: NSView {
    override var isFlipped: Bool { true }
}

private final class IdentityRow: NSView {
    let item: IdentitySheet.Item
    var onPick: ((IdentityRow) -> Void)?
    var copied = false { didSet { if copied != oldValue { needsDisplay = true } } }
    private var hovered = false { didSet { if hovered != oldValue { fill() } } }
    private var pressed = false { didSet { if pressed != oldValue { fill() } } }

    private static let size: CGFloat = 13
    private static let font = NSFont.systemFont(ofSize: size)
    private static let raised = NSFont.systemFont(ofSize: 9.5)
    private static let deeper = NSFont.systemFont(ofSize: 8)
    private static let small = NSFont.systemFont(ofSize: 11)

    init(_ item: IdentitySheet.Item, frame: NSRect) {
        self.item = item
        super.init(frame: frame)
        // the tint is the layer's own background, so hover and press recolor it without redrawing
        wantsLayer = true
        fill()
        let label = item.runs.map(\.text).joined()
        toolTip = "Click to copy \(item.copy)"
        setAccessibilityElement(true)
        setAccessibilityRole(.button)
        setAccessibilityLabel(item.note.isEmpty ? label : "\(label), \(item.note)")
        setAccessibilityHelp("Copies \(item.copy)")
        addTrackingArea(NSTrackingArea(
            rect: .zero,
            options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect],
            owner: self,
            userInfo: nil
        ))
    }

    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }
    override var mouseDownCanMoveWindow: Bool { false }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func mouseEntered(with event: NSEvent) {
        hovered = true
    }

    override func mouseExited(with event: NSEvent) {
        hovered = false
    }

    override func mouseDown(with event: NSEvent) {
        pressed = true
    }

    override func mouseDragged(with event: NSEvent) {
        pressed = bounds.contains(convert(event.locationInWindow, from: nil))
    }

    override func mouseUp(with event: NSEvent) {
        pressed = false
        if bounds.contains(convert(event.locationInWindow, from: nil)) { onPick?(self) }
    }

    override func accessibilityPerformPress() -> Bool {
        onPick?(self)
        return true
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        fill()
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        fill()
    }

    // the page's --fill-hover and --fill-selected: the ink at 5% and 8%
    private func fill() {
        let alpha: CGFloat = pressed ? 0.08 : hovered ? 0.05 : 0
        var color: CGColor?
        effectiveAppearance.performAsCurrentDrawingAppearance {
            color = NSColor.labelColor.withAlphaComponent(alpha).cgColor
        }
        layer?.backgroundColor = color
        layer?.cornerRadius = 6
    }

    // label colors are dynamic, so building the text at draw time follows light and dark
    override func draw(_ dirtyRect: NSRect) {
        let math = Self.math(item.runs, copied ? .secondaryLabelColor : .labelColor)
        let side = copied ? "✓" : item.note
        let note = NSAttributedString(string: side, attributes: [.font: Self.small, .foregroundColor: NSColor.secondaryLabelColor])
        let noteSize = note.size()
        // both share the plain text's baseline, so a raised run doesn't push the line down
        let baseline = (bounds.height + Self.font.ascender + Self.font.descender) / 2
        math.draw(at: NSPoint(x: 6, y: baseline - Self.font.ascender))
        if !side.isEmpty {
            note.draw(at: NSPoint(x: bounds.width - 6 - noteSize.width, y: baseline - Self.small.ascender))
        }
    }

    private static func math(_ runs: [IdentitySheet.Run], _ color: NSColor) -> NSAttributedString {
        let text = NSMutableAttributedString()
        for run in runs {
            let font: NSFont
            let offset: CGFloat
            switch run.shift {
            case "sup": (font, offset) = (raised, 5)
            case "sub": (font, offset) = (raised, -3)
            case "supsub": (font, offset) = (deeper, 2)
            default: (font, offset) = (Self.font, 0)
            }
            text.append(NSAttributedString(string: run.text, attributes: [.font: font, .foregroundColor: color, .baselineOffset: offset]))
        }
        return text
    }
}

final class IdentityWindowController: NSObject {
    // reuses the periodic table's panel: never key, so the overlay keeps its caret and stays up while rows are clicked
    private var panel: PeriodicPanel?
    private var status: NSTextField?
    private var copiedRow: IdentityRow?
    private var clearCopied: DispatchWorkItem?
    private var settingsObserver: NSObjectProtocol?
    private let id: String
    // a second sheet opens a little down and right of the first rather than on top of it
    private let cascade: Int

    private let inset: CGFloat = 16
    private let top: CGFloat = 30
    private let column: CGFloat = 344
    private let rowHeight: CGFloat = 22
    private let groupTitle: CGFloat = 20
    private let sectionTitle: CGFloat = 26
    private let maxHeight: CGFloat = 620
    private static let hint = "click to copy"

    init(id: String, cascade: Int) {
        self.id = id
        self.cascade = cascade
        super.init()
    }

    deinit {
        if let settingsObserver {
            NotificationCenter.default.removeObserver(settingsObserver)
        }
    }

    // a second open just brings the same window forward
    func show(_ sheet: IdentitySheet) {
        if panel == nil { build(sheet) }
        applyAppearance()
        panel?.orderFrontRegardless()
    }

    private func build(_ sheet: IdentitySheet) {
        let width = inset * 3 + column * 2
        let content = IdentitySheetView(frame: NSRect(x: 0, y: 0, width: width, height: 0))
        var y: CGFloat = 0
        for section in sheet.sections {
            let title = label(section.title, size: 12, weight: .semibold, color: .labelColor)
            title.frame = NSRect(x: inset, y: y + 4, width: width - inset * 2, height: 16)
            content.addSubview(title)
            let rule = NSBox(frame: NSRect(x: inset, y: y + sectionTitle - 3, width: width - inset * 2, height: 1))
            rule.boxType = .separator
            content.addSubview(rule)
            // each group goes in whichever column is shorter so far
            var columns: [CGFloat] = [y + sectionTitle, y + sectionTitle]
            for group in section.groups {
                let c = columns[1] < columns[0] ? 1 : 0
                let x = inset + CGFloat(c) * (column + inset)
                var gy = columns[c] + 6
                let name = label(group.title, size: 11, weight: .medium, color: .secondaryLabelColor)
                name.frame = NSRect(x: x + 6, y: gy + 2, width: column - 6, height: 14)
                content.addSubview(name)
                gy += groupTitle
                for item in group.items {
                    let row = IdentityRow(item, frame: NSRect(x: x, y: gy, width: column, height: rowHeight))
                    row.onPick = { [weak self] in self?.copy($0) }
                    content.addSubview(row)
                    gy += rowHeight
                }
                columns[c] = gy
            }
            y = max(columns[0], columns[1]) + 14
        }
        content.setFrameSize(NSSize(width: width, height: y + inset - 14))

        let full = content.frame.height + top
        let size = NSSize(width: width, height: min(full, maxHeight))
        let panel = PeriodicPanel(
            contentRect: NSRect(origin: .zero, size: size),
            styleMask: [.titled, .closable, .resizable, .fullSizeContentView, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.title = sheet.title
        panel.titleVisibility = .hidden
        panel.titlebarAppearsTransparent = true
        panel.standardWindowButton(.miniaturizeButton)?.isHidden = true
        panel.standardWindowButton(.zoomButton)?.isHidden = true
        panel.backgroundColor = PeriodicWindowController.background
        panel.isFloatingPanel = true
        panel.level = .floating
        panel.hidesOnDeactivate = false
        panel.becomesKeyOnlyIfNeeded = true
        panel.isMovableByWindowBackground = true
        panel.isReleasedWhenClosed = false
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        // only the height changes: the two columns are laid out at one width
        panel.contentMinSize = NSSize(width: width, height: min(full, 220))
        panel.contentMaxSize = NSSize(width: width, height: full)

        let root = IdentityRoot(frame: NSRect(origin: .zero, size: size))
        let heading = label(sheet.title, size: 13, weight: .semibold, color: .labelColor)
        let status = label(Self.hint, size: 11, weight: .regular, color: .secondaryLabelColor)
        heading.alignment = .right
        status.alignment = .left
        heading.frame = NSRect(x: 0, y: 7, width: width / 2 - 4, height: 17)
        status.frame = NSRect(x: width / 2 + 4, y: 9, width: width / 2 - 4, height: 14)
        heading.autoresizingMask = [.maxYMargin]
        status.autoresizingMask = [.maxYMargin]
        root.addSubview(heading)
        root.addSubview(status)
        self.status = status

        let scroll = NSScrollView(frame: NSRect(x: 0, y: top, width: width, height: size.height - top))
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.documentView = content
        scroll.autoresizingMask = [.width, .height]
        root.addSubview(scroll)
        panel.contentView = root

        let frameName = "IdentitySheet-\(id)"
        if !panel.setFrameUsingName(frameName) {
            panel.center()
            let offset = CGFloat(cascade) * 28
            panel.setFrameOrigin(NSPoint(x: panel.frame.minX + offset, y: panel.frame.minY - offset))
        }
        panel.setFrameAutosaveName(frameName)
        self.panel = panel
        observeSettings()
    }

    private func label(_ text: String, size: CGFloat, weight: NSFont.Weight, color: NSColor) -> NSTextField {
        let field = NSTextField(labelWithString: text)
        field.font = .systemFont(ofSize: size, weight: weight)
        field.textColor = color
        field.lineBreakMode = .byTruncatingTail
        return field
    }

    // the row and the header both say so for a moment, as the overlay's ✓ does after a copy
    private func copy(_ row: IdentityRow) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(row.item.copy, forType: .string)
        copiedRow?.copied = false
        row.copied = true
        copiedRow = row
        status?.stringValue = "✓ copied"
        clearCopied?.cancel()
        let clear = DispatchWorkItem { [weak self] in
            self?.copiedRow?.copied = false
            self?.copiedRow = nil
            self?.status?.stringValue = Self.hint
        }
        clearCopied = clear
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.2, execute: clear)
    }

    private func observeSettings() {
        guard settingsObserver == nil else { return }
        settingsObserver = NotificationCenter.default.addObserver(
            forName: .qcalcSettingsChanged,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            self?.applyAppearance()
        }
    }

    private func applyAppearance() {
        panel?.appearance = AppSettings.shared.nsAppearance
    }
}
