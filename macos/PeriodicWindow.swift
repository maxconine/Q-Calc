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

// never key while the bar is up, so the overlay keeps focus and its caret while tiles are clicked; with the bar away
// a click gives it the keys (without bringing q calc forward), so ⌘W closes it
final class PeriodicPanel: NSPanel {
    override var canBecomeKey: Bool { !NSApp.windows.contains { $0 is OverlayPanel && $0.isVisible } }
    override var canBecomeMain: Bool { false }

    override func sendEvent(_ event: NSEvent) {
        if event.type == .leftMouseDown || event.type == .rightMouseDown, !isKeyWindow, canBecomeKey { makeKey() }
        super.sendEvent(event)
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if isCommandWCloseKey(event) {
            close()
            return true
        }
        return super.performKeyEquivalent(with: event)
    }

    // the front one showing; NSApp.orderedWindows leaves panels out
    static func front() -> PeriodicPanel? {
        (NSWindow.windowNumbers(options: []) ?? [])
            .lazy
            .compactMap { NSApp.window(withWindowNumber: $0.intValue) as? PeriodicPanel }
            .first { $0.isVisible }
    }
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
    // a right click takes one of it back out of the formula
    var onDrop: ((PeriodicElement) -> Void)?
    var onHover: ((PeriodicElement, Bool) -> Void)?
    // how many are in the formula: a picked tile is tinted deeper and ringed, and says ×2 past one
    var count = 0 { didSet { if count != oldValue { fill(); needsDisplay = true } } }
    private var hovered = false { didSet { if hovered != oldValue { fill() } } }
    private var pressed = false { didSet { if pressed != oldValue { fill() } } }
    private let number: NSAttributedString
    private let symbol: NSAttributedString
    private let mass: NSAttributedString
    private let symbolSize: NSSize
    private let massSize: NSSize

    private static let small = NSFont.monospacedDigitSystemFont(ofSize: 8.5, weight: .regular)
    private static let large = NSFont.systemFont(ofSize: 17, weight: .medium)
    private static let badge = NSFont.monospacedDigitSystemFont(ofSize: 8.5, weight: .semibold)

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

    override func rightMouseDown(with event: NSEvent) {}

    override func rightMouseUp(with event: NSEvent) {
        if bounds.contains(convert(event.locationInWindow, from: nil)) { onDrop?(element) }
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
        let alpha: CGFloat = pressed ? 0.4 : count > 0 ? 0.32 : hovered ? 0.2 : 0.1
        var color: CGColor?
        var ring: CGColor?
        effectiveAppearance.performAsCurrentDrawingAppearance {
            color = Self.tint(element.category).withAlphaComponent(alpha).cgColor
            ring = Self.tint(element.category).withAlphaComponent(0.7).cgColor
        }
        layer?.backgroundColor = color
        layer?.borderColor = ring
        layer?.borderWidth = count > 0 ? 1.5 : 0
        layer?.cornerRadius = 6
    }

    override func draw(_ dirtyRect: NSRect) {
        number.draw(at: NSPoint(x: 4, y: 3))
        if count > 1 {
            let times = Self.text("×\(count)", Self.badge, .labelColor)
            times.draw(at: NSPoint(x: bounds.width - times.size().width - 4, y: 3))
        }
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

// a line of the formula card: the formula itself (a click puts its molar mass in the bar), a part of it (a click takes
// one away) or the clear button, each with the tiles' hover and press tint
private final class FormulaText: NSView {
    var onClick: (() -> Void)?
    private let text: NSAttributedString
    private var hovered = false { didSet { if hovered != oldValue { fill() } } }
    private var pressed = false { didSet { if pressed != oldValue { fill() } } }

    init(_ text: NSAttributedString, help: String) {
        self.text = text
        let size = text.size()
        super.init(frame: NSRect(x: 0, y: 0, width: ceil(size.width) + 12, height: ceil(size.height) + 4))
        wantsLayer = true
        toolTip = help
        setAccessibilityElement(true)
        setAccessibilityRole(.button)
        setAccessibilityLabel(text.string)
        setAccessibilityHelp(help)
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

    override func mouseUp(with event: NSEvent) {
        pressed = false
        if bounds.contains(convert(event.locationInWindow, from: nil)) { onClick?() }
    }

    override func accessibilityPerformPress() -> Bool {
        onClick?()
        return true
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        fill()
    }

    private func fill() {
        let alpha: CGFloat = pressed ? 0.08 : hovered ? 0.05 : 0
        var color: CGColor?
        effectiveAppearance.performAsCurrentDrawingAppearance {
            color = NSColor.labelColor.withAlphaComponent(alpha).cgColor
        }
        layer?.backgroundColor = color
        layer?.cornerRadius = 5
    }

    override func draw(_ dirtyRect: NSRect) {
        text.draw(at: NSPoint(x: 6, y: 2))
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
    // one symbol a click, in order: H, H, O is H₂O
    private var picks: [String] = []
    private var tiles: [String: PeriodicTile] = [:]
    // each element's weight in thousandths, so a formula's sum comes out exact, as the page's formulaOf does
    private var masses: [String: Int] = [:]
    private var grid: ScaledView?
    private var formulaViews: [NSView] = []
    private var formulaFrame = NSRect.zero
    var onPick: ((String) -> Void)?

    private static let frameName = "PeriodicTable"
    private let tile = NSSize(width: 44, height: 48)
    private let gap: CGFloat = 3
    private let inset: CGFloat = 16
    private let top: CGFloat = 30
    // between the table and the lanthanide and actinide rows
    private let fGap: CGFloat = 10

    // the overlay's own --bg, so the two read as one app; the identity sheets share it
    static let background = NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            ? NSColor(srgbRed: 44 / 255, green: 44 / 255, blue: 46 / 255, alpha: 1)
            : .white
    }

    deinit {
        if let settingsObserver {
            NotificationCenter.default.removeObserver(settingsObserver)
        }
    }

    // a second open just brings the same window forward. `bar` is the bar's frame and the visible frame of its
    // screen: a table that would cover the bar moves beside it, scaled down a little if it has to be
    func show(_ elements: @autoclosure () -> [PeriodicElement], bar: (frame: NSRect, visible: NSRect)? = nil) {
        if panel == nil {
            let elements = elements()
            guard !elements.isEmpty else { return }
            build(elements)
        }
        applyAppearance()
        if let panel, let bar, SheetPlace.needsMoving(panel.frame, bar: bar.frame, visible: bar.visible) {
            panel.setFrame(SheetPlace.beside(panel.frame.size, bar: bar.frame, visible: bar.visible, give: .scale(min: 0.6)), display: true)
        }
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
        // the hovered element's name in the first row, the formula the picks make in the two under it
        let detailFrame = cell(row: 1, col: 3).union(cell(row: 1, col: 12))
        formulaFrame = cell(row: 2, col: 3).union(cell(row: 3, col: 12))
        let name = label(size: 15, weight: .medium, color: .labelColor)
        let sub = label(size: 11, weight: .regular, color: .secondaryLabelColor)
        name.frame = NSRect(x: detailFrame.minX, y: detailFrame.minY + 6, width: detailFrame.width, height: 20)
        sub.frame = NSRect(x: detailFrame.minX, y: detailFrame.minY + 28, width: detailFrame.width, height: 16)
        grid.addSubview(name)
        grid.addSubview(sub)
        detailName = name
        detailSub = sub

        for element in elements {
            let view = PeriodicTile(element, frame: cell(row: element.row, col: element.col))
            view.onPick = { [weak self] in self?.pick($0.symbol) }
            view.onDrop = { [weak self] in self?.drop($0.symbol) }
            view.onHover = { [weak self] in self?.showDetail($0, $1) }
            grid.addSubview(view)
            tiles[element.symbol] = view
            masses[element.symbol] = Int(((Double(element.insert) ?? 0) * 1000).rounded())
        }
        self.grid = grid
        showFormula()
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

    private func pick(_ symbol: String) {
        picks.append(symbol)
        showFormula()
    }

    private func drop(_ symbol: String) {
        guard let at = picks.lastIndex(of: symbol) else { return }
        picks.remove(at: at)
        showFormula()
    }

    private static let subscripts = Array("₀₁₂₃₄₅₆₇₈₉")

    private static func sub(_ n: Int) -> String {
        n == 1 ? "" : String(String(n).compactMap { $0.wholeNumberValue.map { subscripts[$0] } })
    }

    private static func grams(_ m: Int) -> String {
        guard m % 1000 != 0 else { return String(m / 1000) }
        var digits = String(format: "%03d", m % 1000)
        while digits.hasSuffix("0") { digits.removeLast() }
        return "\(m / 1000).\(digits)"
    }

    // the card under the hover detail: the formula and its molar mass, and each element's share, compact
    private func showFormula() {
        var order: [String] = []
        var counts: [String: Int] = [:]
        for symbol in picks {
            if counts[symbol] == nil { order.append(symbol) }
            counts[symbol, default: 0] += 1
        }
        for (symbol, tile) in tiles { tile.count = counts[symbol] ?? 0 }
        formulaViews.forEach { $0.removeFromSuperview() }
        formulaViews = []
        guard let grid else { return }
        let muted = NSColor.secondaryLabelColor
        guard !order.isEmpty else {
            let hint = label(size: 11, weight: .regular, color: muted)
            hint.font = .systemFont(ofSize: 11)
            hint.stringValue = "click elements to build a formula · right click takes one away"
            hint.frame = NSRect(x: formulaFrame.minX, y: formulaFrame.midY - 8, width: formulaFrame.width, height: 16)
            grid.addSubview(hint)
            formulaViews = [hint]
            return
        }
        let total = order.reduce(0) { $0 + (masses[$1] ?? 0) * counts[$1]! }
        let mass = Self.grams(total)
        let main = NSMutableAttributedString(string: order.map { $0 + Self.sub(counts[$0]!) }.joined(), attributes: [
            .font: NSFont.systemFont(ofSize: 20, weight: .medium), .foregroundColor: NSColor.labelColor,
        ])
        main.append(NSAttributedString(string: "  \(mass) g/mol", attributes: [
            .font: NSFont.monospacedDigitSystemFont(ofSize: 13, weight: .regular), .foregroundColor: NSColor.labelColor,
        ]))
        let formula = FormulaText(main, help: "Put the molar mass in the bar")
        formula.onClick = { [weak self] in self?.onPick?(mass) }
        formula.setFrameOrigin(NSPoint(x: formulaFrame.midX - formula.frame.width / 2, y: formulaFrame.minY + 18))

        let small: [NSAttributedString.Key: Any] = [.font: NSFont.monospacedDigitSystemFont(ofSize: 11, weight: .regular), .foregroundColor: muted]
        var parts: [FormulaText] = order.map { symbol in
            let n = counts[symbol]!
            let text = NSAttributedString(string: "\(symbol)\(Self.sub(n)) \(Self.grams((masses[symbol] ?? 0) * n))", attributes: small)
            let part = FormulaText(text, help: "Take one \(symbol) away")
            part.onClick = { [weak self] in self?.drop(symbol) }
            return part
        }
        let clear = FormulaText(NSAttributedString(string: "×", attributes: small), help: "Clear the formula")
        clear.onClick = { [weak self] in
            self?.picks = []
            self?.showFormula()
        }
        parts.append(clear)
        let width = parts.reduce(0) { $0 + $1.frame.width }
        var x = formulaFrame.midX - width / 2
        for part in parts {
            part.setFrameOrigin(NSPoint(x: x, y: formula.frame.maxY + 4))
            x += part.frame.width
        }
        formulaViews = [formula] + parts
        formulaViews.forEach { grid.addSubview($0) }
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
