import AppKit

// one tile, as the web page sends it; the data itself lives in src/lib/elements.ts
struct PeriodicElement {
    let n: Int
    let symbol: String
    let name: String
    let mass: String
    let insert: String
    let row: Int
    let col: Int
    let category: String

    init?(_ any: Any) {
        guard let d = any as? [String: Any],
              let n = (d["n"] as? NSNumber)?.intValue,
              let symbol = d["symbol"] as? String,
              let name = d["name"] as? String,
              let mass = d["mass"] as? String,
              let insert = d["insert"] as? String,
              let row = (d["row"] as? NSNumber)?.intValue,
              let col = (d["col"] as? NSNumber)?.intValue
        else { return nil }
        self.n = n
        self.symbol = symbol
        self.name = name
        self.mass = mass
        self.insert = insert
        self.row = row
        self.col = col
        self.category = d["category"] as? String ?? ""
    }

    static func list(from any: Any?) -> [PeriodicElement] {
        (any as? [Any] ?? []).compactMap(PeriodicElement.init)
    }
}

// never key, so the overlay keeps focus and its caret while tiles are clicked
final class PeriodicPanel: NSPanel {
    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}

// lays the table out at one size and scales it to whatever the window is
private final class ScaledView: NSView {
    let designSize: NSSize

    init(designSize: NSSize) {
        self.designSize = designSize
        super.init(frame: NSRect(origin: .zero, size: designSize))
    }

    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }
    override var mouseDownCanMoveWindow: Bool { false }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        setBoundsSize(designSize)
        // tiles own their layers, so a new scale doesn't redraw them by itself; this keeps the text sharp
        for tile in subviews where tile is PeriodicTile { tile.needsDisplay = true }
    }

    // the background drags the window, with a hand that closes while it moves
    override func mouseDown(with event: NSEvent) {
        NSCursor.closedHand.set()
        window?.performDrag(with: event)
        NSCursor.openHand.set()
    }
}

// the panel never becomes key, so cursor rects don't fire; this sets the cursor by hand
private final class PeriodicRoot: NSView {
    override init(frame: NSRect) {
        super.init(frame: frame)
        addTrackingArea(NSTrackingArea(
            rect: .zero,
            options: [.mouseMoved, .mouseEnteredAndExited, .activeAlways, .inVisibleRect],
            owner: self,
            userInfo: nil
        ))
    }

    required init?(coder: NSCoder) { nil }

    override func mouseMoved(with event: NSEvent) {
        guard let superview else { return }
        switch hitTest(superview.convert(event.locationInWindow, from: nil)) {
        case is ResizeGrip: ResizeGrip.cursor.set()
        case is ScaledView, is NSTextField: NSCursor.openHand.set()
        default: NSCursor.arrow.set()
        }
    }

    override func mouseExited(with event: NSEvent) {
        if NSEvent.pressedMouseButtons == 0 { NSCursor.arrow.set() }
    }
}

private final class PeriodicTile: NSView {
    let element: PeriodicElement
    var onPick: ((PeriodicElement) -> Void)?
    var onHover: ((PeriodicElement, Bool) -> Void)?
    private var hovered = false { didSet { if hovered != oldValue { fill() } } }
    private var pressed = false { didSet { if pressed != oldValue { fill() } } }
    private let number: NSAttributedString
    private let symbol: NSAttributedString
    private let mass: NSAttributedString
    private let symbolSize: NSSize
    private let massSize: NSSize

    private static let small = NSFont.monospacedDigitSystemFont(ofSize: 8.5, weight: .regular)
    private static let large = NSFont.systemFont(ofSize: 17, weight: .medium)

    init(_ element: PeriodicElement, frame: NSRect) {
        self.element = element
        // label colors are dynamic, so these still follow light and dark when drawn
        number = Self.text(String(element.n), Self.small, .secondaryLabelColor)
        symbol = Self.text(element.symbol, Self.large, .labelColor)
        mass = Self.text(element.mass, Self.small, .secondaryLabelColor)
        symbolSize = symbol.size()
        massSize = mass.size()
        super.init(frame: frame)
        // the tint is the layer's own background, so hover and press recolor it without redrawing
        wantsLayer = true
        fill()
        toolTip = "\(element.name) · \(element.symbol) · \(element.n)"
        setAccessibilityElement(true)
        setAccessibilityRole(.button)
        setAccessibilityLabel("\(element.name), \(element.n), \(element.mass)")
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
        onHover?(element, true)
    }

    override func mouseExited(with event: NSEvent) {
        hovered = false
        onHover?(element, false)
    }

    override func mouseDown(with event: NSEvent) {
        pressed = true
    }

    override func mouseDragged(with event: NSEvent) {
        pressed = bounds.contains(convert(event.locationInWindow, from: nil))
    }

    override func mouseUp(with event: NSEvent) {
        pressed = false
        if bounds.contains(convert(event.locationInWindow, from: nil)) { onPick?(element) }
    }

    override func accessibilityPerformPress() -> Bool {
        onPick?(element)
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

    private func fill() {
        let alpha: CGFloat = pressed ? 0.3 : hovered ? 0.2 : 0.1
        var color: CGColor?
        effectiveAppearance.performAsCurrentDrawingAppearance {
            color = Self.tint(element.category).withAlphaComponent(alpha).cgColor
        }
        layer?.backgroundColor = color
        layer?.cornerRadius = 6
    }

    override func draw(_ dirtyRect: NSRect) {
        number.draw(at: NSPoint(x: 4, y: 3))
        symbol.draw(at: NSPoint(x: (bounds.width - symbolSize.width) / 2, y: 11))
        mass.draw(at: NSPoint(x: (bounds.width - massSize.width) / 2, y: bounds.height - massSize.height - 4))
    }

    private static func text(_ s: String, _ font: NSFont, _ color: NSColor) -> NSAttributedString {
        NSAttributedString(string: s, attributes: [.font: font, .foregroundColor: color])
    }

    private static func tint(_ category: String) -> NSColor {
        switch category {
        case "alkali", "alkaline": return .systemOrange
        case "transition": return .systemBlue
        case "post", "metalloid": return .systemTeal
        case "nonmetal", "halogen": return .systemGreen
        case "noble": return .systemPurple
        case "lanthanide", "actinide": return .systemPink
        default: return .systemGray
        }
    }
}

// the window edges are too thin to find, so a corner grip resizes it, keeping its shape
private final class ResizeGrip: NSView {
    var minSize = NSSize.zero
    var maxSize = NSSize.zero
    private var start: (mouse: NSPoint, frame: NSRect)?
    private var hovered = false { didSet { needsDisplay = true } }

    override init(frame: NSRect) {
        super.init(frame: frame)
        addTrackingArea(NSTrackingArea(
            rect: .zero,
            options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect],
            owner: self,
            userInfo: nil
        ))
    }

    required init?(coder: NSCoder) { nil }

    override var mouseDownCanMoveWindow: Bool { false }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    // appkit's own diagonal resize cursor; the sdk has no public one
    static let cursor: NSCursor = {
        let pick = NSSelectorFromString("_windowResizeNorthWestSouthEastCursor")
        guard NSCursor.responds(to: pick),
              let cursor = NSCursor.perform(pick)?.takeUnretainedValue() as? NSCursor
        else { return .crosshair }
        return cursor
    }()

    override func mouseEntered(with event: NSEvent) {
        hovered = true
    }

    override func mouseExited(with event: NSEvent) {
        hovered = false
    }

    override func mouseDown(with event: NSEvent) {
        guard let window else { return }
        start = (NSEvent.mouseLocation, window.frame)
        Self.cursor.set()
    }

    override func mouseDragged(with event: NSEvent) {
        guard let window, let start else { return }
        Self.cursor.set()
        let mouse = NSEvent.mouseLocation
        let content = window.contentRect(forFrameRect: start.frame).size
        let ratio = content.height / content.width
        // follow whichever way the pointer moved further, so the corner stays under it
        let dx = mouse.x - start.mouse.x
        let dy = (start.mouse.y - mouse.y) / ratio
        let width = min(max(content.width + max(dx, dy), minSize.width), maxSize.width)
        let size = NSSize(width: width, height: width * ratio)
        let frame = window.frameRect(forContentRect: NSRect(origin: .zero, size: size))
        window.setFrame(
            NSRect(x: start.frame.minX, y: start.frame.maxY - frame.height, width: frame.width, height: frame.height),
            display: true
        )
    }

    override func mouseUp(with event: NSEvent) {
        start = nil
        needsDisplay = true
    }

    override func draw(_ dirtyRect: NSRect) {
        (hovered || start != nil ? NSColor.secondaryLabelColor : NSColor.tertiaryLabelColor).setStroke()
        let path = NSBezierPath()
        path.lineWidth = 1.2
        path.lineCapStyle = .round
        for d in [4.0, 8.0] as [CGFloat] {
            path.move(to: NSPoint(x: bounds.maxX - 3 - d, y: 3))
            path.line(to: NSPoint(x: bounds.maxX - 3, y: 3 + d))
        }
        path.stroke()
    }
}

final class PeriodicWindowController: NSObject {
    private var panel: PeriodicPanel?
    private var detailName: NSTextField?
    private var detailSub: NSTextField?
    private var detailN: Int?
    private var settingsObserver: NSObjectProtocol?
    var onPick: ((String) -> Void)?

    private static let frameName = "PeriodicTable"
    private let tile = NSSize(width: 44, height: 48)
    private let gap: CGFloat = 3
    private let inset: CGFloat = 16
    private let top: CGFloat = 30
    // between the table and the lanthanide and actinide rows
    private let fGap: CGFloat = 10

    // the overlay's own --bg, so the two read as one app
    private static let background = NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            ? NSColor(srgbRed: 44 / 255, green: 44 / 255, blue: 46 / 255, alpha: 1)
            : .white
    }

    deinit {
        if let settingsObserver {
            NotificationCenter.default.removeObserver(settingsObserver)
        }
    }

    // a second open just brings the same window forward
    func show(_ elements: @autoclosure () -> [PeriodicElement]) {
        if panel == nil {
            let elements = elements()
            guard !elements.isEmpty else { return }
            build(elements)
        }
        applyAppearance()
        panel?.orderFrontRegardless()
    }

    private func cell(row: Int, col: Int) -> NSRect {
        let x = inset + CGFloat(col - 1) * (tile.width + gap)
        let y = row <= 7
            ? top + CGFloat(row - 1) * (tile.height + gap)
            : top + 7 * (tile.height + gap) + fGap + CGFloat(row - 9) * (tile.height + gap)
        return NSRect(origin: NSPoint(x: x, y: y), size: tile)
    }

    private func build(_ elements: [PeriodicElement]) {
        let lastRow = cell(row: 10, col: 18)
        let size = NSSize(width: lastRow.maxX + inset, height: lastRow.maxY + inset)
        let panel = PeriodicPanel(
            contentRect: NSRect(origin: .zero, size: size),
            styleMask: [.titled, .closable, .resizable, .fullSizeContentView, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.title = "Periodic Table"
        panel.titleVisibility = .hidden
        panel.titlebarAppearsTransparent = true
        panel.standardWindowButton(.miniaturizeButton)?.isHidden = true
        panel.standardWindowButton(.zoomButton)?.isHidden = true
        panel.backgroundColor = Self.background
        panel.isFloatingPanel = true
        panel.level = .floating
        panel.hidesOnDeactivate = false
        panel.becomesKeyOnlyIfNeeded = true
        panel.isMovableByWindowBackground = true
        panel.isReleasedWhenClosed = false
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        panel.contentAspectRatio = size
        panel.contentMinSize = NSSize(width: size.width * 0.6, height: size.height * 0.6)
        panel.contentMaxSize = NSSize(width: size.width * 2.5, height: size.height * 2.5)

        let grid = ScaledView(designSize: size)
        let detailFrame = cell(row: 1, col: 3).union(cell(row: 3, col: 12))
        let name = label(size: 15, weight: .medium, color: .labelColor)
        let sub = label(size: 11, weight: .regular, color: .secondaryLabelColor)
        name.frame = NSRect(x: detailFrame.minX, y: detailFrame.midY - 22, width: detailFrame.width, height: 20)
        sub.frame = NSRect(x: detailFrame.minX, y: detailFrame.midY + 1, width: detailFrame.width, height: 16)
        grid.addSubview(name)
        grid.addSubview(sub)
        detailName = name
        detailSub = sub

        for element in elements {
            let view = PeriodicTile(element, frame: cell(row: element.row, col: element.col))
            view.onPick = { [weak self] in self?.onPick?($0.insert) }
            view.onHover = { [weak self] in self?.showDetail($0, $1) }
            grid.addSubview(view)
        }
        let root = PeriodicRoot(frame: grid.frame)
        grid.autoresizingMask = [.width, .height]
        root.addSubview(grid)
        let grip = ResizeGrip(frame: NSRect(x: size.width - 18, y: 0, width: 18, height: 18))
        grip.autoresizingMask = [.minXMargin, .maxYMargin]
        grip.minSize = panel.contentMinSize
        grip.maxSize = panel.contentMaxSize
        root.addSubview(grip)
        panel.contentView = root

        if !panel.setFrameUsingName(Self.frameName) { panel.center() }
        panel.setFrameAutosaveName(Self.frameName)
        self.panel = panel
        observeSettings()
    }

    private func label(size: CGFloat, weight: NSFont.Weight, color: NSColor) -> NSTextField {
        let field = NSTextField(labelWithString: "")
        field.font = size < 12 ? .monospacedDigitSystemFont(ofSize: size, weight: weight) : .systemFont(ofSize: size, weight: weight)
        field.textColor = color
        field.alignment = .center
        return field
    }

    // leaving a tile only clears the detail if no other tile has taken it since
    private func showDetail(_ element: PeriodicElement, _ entered: Bool) {
        if !entered, detailN != element.n { return }
        detailN = entered ? element.n : nil
        detailName?.stringValue = entered ? element.name : ""
        detailSub?.stringValue = entered ? "\(element.symbol) · \(element.n)" : ""
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
