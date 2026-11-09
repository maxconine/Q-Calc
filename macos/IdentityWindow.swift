import AppKit

// one sheet, as the web page sends it; the identities themselves live in src/lib/identities.ts
struct IdentitySheet {
    // something typeset ahead of time (the native app's formula sheets), drawn in page points from the top left
    struct Drawn {
        let size: CGSize
        let draw: (CGContext) -> Void
    }

    struct Run {
        let text: String
        // "sup", "sub", "supsub" (a subscript inside a superscript) or "" for the baseline
        let shift: String
    }

    // a label on a figure, centred on a point of the figure
    struct Spot {
        let center: CGPoint
        let drawn: Drawn
        let copy: String
    }

    // a piece of a row; an empty copy is only drawn
    struct Part {
        let drawn: Drawn
        let copy: String
    }

    struct Item {
        let runs: [Run]
        let copy: String
        let note: String
        // leading table cells, drawn in fixed columns ahead of the runs: a prefix's name and symbol, a constant's symbol
        let lead: [[Run]]
        // what a click on each lead cell copies, in place of `copy`; empty for a label
        var leadCopy: [String] = []
        // the bar line a click runs, in place of a copy
        var open = ""
        // typeset, when the sheet is: the lead cells then the runs, and the note
        var cells: [Drawn] = []
        var drawnNote: Drawn?
        // a whole typeset picture rather than a row: centred, and no click
        var figure = false
        // its labels, each clicked to copy
        // pieces of the row that copy alone, drawn in a line in place of the runs
        var parts: [Part] = []
        var spots: [Spot] = []
    }

    struct Group {
        let title: String
        let items: [Item]
    }

    struct Section {
        let title: String
        // its groups one under another at the sheet's full width, for rows too long for a column
        let wide: Bool
        // its lead columns' widths, for a table with wide cells (the native app's sheets); nil for the usual ones
        let leadWidths: [CGFloat]?
        let groups: [Group]
    }

    let id: String
    let title: String
    let sections: [Section]

    // `drawn` looks up what a document ("…Doc" in the dictionary) typeset to
    init?(_ d: [String: Any], drawn: @escaping (String) -> Drawn? = { _ in nil }) {
        let cell: (Any?) -> Drawn? = { ($0 as? String).flatMap(drawn) }
        guard let id = d["id"] as? String, let title = d["title"] as? String else { return nil }
        self.id = id
        self.title = title
        sections = (d["sections"] as? [Any] ?? []).compactMap { any in
            guard let s = any as? [String: Any], let title = s["title"] as? String else { return nil }
            let groups: [Group] = (s["groups"] as? [Any] ?? []).compactMap { any in
                guard let g = any as? [String: Any], let title = g["title"] as? String else { return nil }
                let items: [Item] = (g["items"] as? [Any] ?? []).compactMap { any in
                    guard let i = any as? [String: Any], let copy = i["copy"] as? String else { return nil }
                    let runs = Self.runs(i["runs"])
                    let lead = (i["lead"] as? [Any] ?? []).map { $0 is String ? [Run(text: $0 as! String, shift: "")] : Self.runs($0) }
                    return runs.isEmpty ? nil : Item(runs: runs, copy: copy, note: i["note"] as? String ?? "", lead: lead,
                                                     leadCopy: i["leadCopy"] as? [String] ?? [], open: i["open"] as? String ?? "",
                                                     cells: (i["cellDocs"] as? [String] ?? []).compactMap(drawn), drawnNote: cell(i["noteDoc"]),
                                                     figure: i["figure"] as? Bool ?? false,
                                                     parts: (i["parts"] as? [[String: Any]] ?? []).compactMap { part in
                                                         guard let picture = cell(part["doc"]), let copy = part["copy"] as? String else { return nil }
                                                         return Part(drawn: picture, copy: copy)
                                                     },
                                                     spots: (i["spots"] as? [[String: Any]] ?? []).compactMap { spot in
                                                         guard let picture = cell(spot["doc"]), let copy = spot["copy"] as? String,
                                                               let x = (spot["x"] as? NSNumber)?.doubleValue, let y = (spot["y"] as? NSNumber)?.doubleValue else { return nil }
                                                         return Spot(center: CGPoint(x: x, y: y), drawn: picture, copy: copy)
                                                     })
                }
                return items.isEmpty ? nil : Group(title: title, items: items)
            }
            return groups.isEmpty ? nil : Section(title: title, wide: s["wide"] as? Bool ?? false,
                                                 leadWidths: (s["leadWidths"] as? [NSNumber])?.map { CGFloat($0.doubleValue) }, groups: groups)
        }
        if sections.isEmpty { return nil }
    }

    private static func runs(_ any: Any?) -> [Run] {
        (any as? [Any] ?? []).compactMap { any in
            guard let r = any as? [String: Any], let text = r["text"] as? String else { return nil }
            return Run(text: text, shift: r["shift"] as? String ?? "")
        }
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
    weak var controller: IdentityWindowController?

    // ⌘[ and ⌘], while the sheet has the keys (the bar sends them on while it's up: OverlayController)
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if let step = IdentityWindowController.navigationStep(event), let controller, controller.go(step) { return true }
        return super.performKeyEquivalent(with: event)
    }

    // a two-finger swipe over the header; the rows' scroll view does the same over the page
    override func scrollWheel(with event: NSEvent) {
        if controller?.trackSwipe(event) != true { super.scrollWheel(with: event) }
    }

    // a three-finger swipe, when the trackpad is set to swipe pages that way: left is positive, back
    override func swipe(with event: NSEvent) {
        if event.deltaX > 0.5 { _ = controller?.go(-1) } else if event.deltaX < -0.5 { _ = controller?.go(1) } else { super.swipe(with: event) }
    }
}

// the page's scroll view: a sideways two-finger swipe goes back or forward rather than scrolling
private final class IdentityScroll: NSScrollView {
    weak var controller: IdentityWindowController?

    override func scrollWheel(with event: NSEvent) {
        if controller?.trackSwipe(event) != true { super.scrollWheel(with: event) }
    }
}

private final class IdentityRow: NSView {
    let item: IdentitySheet.Item
    var onPick: ((IdentityRow) -> Void)?
    // a row that opens a sheet: its right-click menu's Open in New Window
    var onOpenNew: ((IdentityRow) -> Void)?
    var copied = false { didSet { if copied != oldValue { needsDisplay = true } } }
    private var hovered = false { didSet { if hovered != oldValue { fill() } } }
    private var pressed = false { didSet { if pressed != oldValue { fill() } } }

    private static let size: CGFloat = 13
    private static let font = NSFont.systemFont(ofSize: size)
    private static let raised = NSFont.systemFont(ofSize: 9.5)
    private static let deeper = NSFont.systemFont(ofSize: 8)
    private static let small = NSFont.systemFont(ofSize: 11)
    // the page's LEAD_WIDTHS
    private static let defaultLeadWidths: [CGFloat] = [64, 28]
    private let leadWidths: [CGFloat]
    // a table's heading row: no copy, so it reads muted and takes no clicks
    private var isHeading: Bool { item.copy.isEmpty && !item.figure }

    init(_ item: IdentitySheet.Item, leadWidths: [CGFloat]? = nil, frame: NSRect) {
        self.item = item
        self.leadWidths = leadWidths ?? Self.defaultLeadWidths
        super.init(frame: frame)
        // the tint is the layer's own background, so hover and press recolor it without redrawing
        wantsLayer = true
        // a redraw or a tint changes at once, with no fade
        layer?.actions = ["contents": NSNull(), "backgroundColor": NSNull()]
        fill()
        let label = (item.lead + [item.runs]).map { $0.map(\.text).joined() }.joined(separator: " ")
        setAccessibilityElement(true)
        setAccessibilityLabel(item.note.isEmpty ? label : "\(label), \(item.note)")
        if item.figure {
            toolTip = "Click a label to copy it"
            setAccessibilityRole(.staticText)
            setAccessibilityLabel(item.spots.map(\.copy).filter { !$0.isEmpty }.joined(separator: ", "))
            return
        }
        guard !item.copy.isEmpty else {
            setAccessibilityRole(.staticText)
            return
        }
        setAccessibilityRole(.button)
        if !item.open.isEmpty {
            toolTip = "Click to open"
            setAccessibilityHelp("Opens \(item.open)")
        } else if item.leadCopy.contains(where: { !$0.isEmpty }) || !item.parts.isEmpty {
            toolTip = "Click a cell to copy it"
            setAccessibilityHelp("Copies \(item.copy)")
        } else {
            toolTip = "Click to copy \(item.copy)"
            setAccessibilityHelp("Copies \(item.copy)")
        }
        addTrackingArea(NSTrackingArea(
            rect: .zero,
            options: [.mouseEnteredAndExited, .mouseMoved, .activeAlways, .inVisibleRect],
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
        pointer(event)
    }

    override func mouseExited(with event: NSEvent) {
        hovered = false
        if boxed != nil { boxed = nil; needsDisplay = true }
    }

    override func mouseMoved(with event: NSEvent) {
        pointer(event)
    }

    // a typeset row boxes just the text a click there would copy
    private var boxed: NSRect?
    // what was just copied stays boxed for the moment the row says so
    private var copiedBox: NSRect?
    private func pointer(_ event: NSEvent) {
        guard typeset else { return }
        let box = target(at: convert(event.locationInWindow, from: nil))?.box
        if box != boxed { boxed = box; needsDisplay = true }
    }

    private var typeset: Bool { !item.cells.isEmpty }
    private static let partGap: CGFloat = 3

    // where a figure's page starts in this row
    private var figureOrigin: CGPoint {
        let page = item.cells.first?.size ?? .zero
        return CGPoint(x: (bounds.width - page.width) / 2, y: (bounds.height - page.height) / 2)
    }

    // the text under a point of a typeset row: the box round it, and what a click there copies
    private func target(at p: NSPoint) -> (box: NSRect, copy: String)? {
        let pad: CGFloat = 3
        if item.figure {
            let origin = figureOrigin
            for spot in item.spots where !spot.copy.isEmpty {
                let size = spot.drawn.size
                let box = NSRect(x: origin.x + spot.center.x - size.width / 2, y: origin.y + spot.center.y - size.height / 2,
                                 width: size.width, height: size.height).insetBy(dx: -pad, dy: -2)
                if box.contains(p) { return (box, spot.copy) }
            }
            return nil
        }
        guard typeset, !item.copy.isEmpty else { return nil }
        if !item.parts.isEmpty {
            // a piece under the pointer copies alone; anywhere else on the row, all of it
            var left: CGFloat = 6
            var line = NSRect.null
            for part in item.parts {
                let size = part.drawn.size
                let box = NSRect(x: left - pad + 1, y: (bounds.height - size.height) / 2 - 2, width: size.width + pad * 2 - 2, height: size.height + 4)
                line = line.union(box)
                if !part.copy.isEmpty, p.x >= box.minX, p.x < box.maxX { return (box, part.copy) }
                left += size.width + Self.partGap
            }
            return (line, item.copy)
        }
        // each cell's box in the row, its text's size at its column's place
        var x: CGFloat = 6
        var boxes: [NSRect] = []
        for (i, cell) in item.cells.enumerated() {
            // the whole cell and a little more: a fraction's bar and limits stay well inside
            boxes.append(NSRect(x: x - pad, y: (bounds.height - cell.size.height) / 2 - 3,
                                width: cell.size.width + pad * 2, height: cell.size.height + 6))
            x += i < leadWidths.count ? leadWidths[i] : leadWidths.last ?? 0
        }
        guard let last = boxes.last else { return nil }
        if !item.open.isEmpty { return (boxes.reduce(last) { $0.union($1) }, item.copy) }
        var right: CGFloat = 6
        for i in item.lead.indices where i < boxes.count - 1 {
            right += i < leadWidths.count ? leadWidths[i] : leadWidths.last ?? 0
            if p.x < right {
                if i < item.leadCopy.count, !item.leadCopy[i].isEmpty { return (boxes[i], item.leadCopy[i]) }
                break
            }
        }
        return (last, item.copy)
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        guard onOpenNew != nil else { return super.menu(for: event) }
        let menu = NSMenu()
        let item = NSMenuItem(title: "Open in New Window", action: #selector(openNew), keyEquivalent: "")
        item.target = self
        menu.addItem(item)
        return menu
    }

    @objc func openNew() {
        onOpenNew?(self)
    }

    override func mouseDown(with event: NSEvent) {
        menuUp = false
        // a control-click is a right-click
        if event.modifierFlags.contains(.control), let menu = menu(for: event) {
            menuUp = true
            NSMenu.popUpContextMenu(menu, with: event, for: self)
            return
        }
        pressed = true
        // a typeset row copies the moment it's pressed, and shows it then
        guard typeset, item.open.isEmpty else { return }
        pickedPoint = convert(event.locationInWindow, from: nil)
        pickedOnDown = true
        onPick?(self)
        needsDisplay = true
        displayIfNeeded()
    }
    private var pickedOnDown = false

    override func mouseDragged(with event: NSEvent) {
        pressed = bounds.contains(convert(event.locationInWindow, from: nil))
    }

    private var menuUp = false

    override func mouseUp(with event: NSEvent) {
        pressed = false
        if menuUp {
            menuUp = false
            return
        }
        if pickedOnDown {
            pickedOnDown = false
            return
        }
        let at = convert(event.locationInWindow, from: nil)
        pickedPoint = at
        if bounds.contains(at) { onPick?(self) }
    }

    // what this row's click copies
    private var pickedPoint: NSPoint?
    var pickedCopy: String {
        defer { pickedPoint = nil }
        guard let at = pickedPoint else { return item.copy }
        guard typeset else { return item.copy }
        let hit = target(at: at)
        copiedBox = hit?.box
        return hit?.copy ?? ""
    }

    // the height this row wants: its tallest typeset cell, with some air
    static func height(_ item: IdentitySheet.Item, least: CGFloat) -> CGFloat {
        // room for the box round the text as well
        max(least, (item.cells.map(\.size.height).max() ?? 0) + 10)
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
        // a typeset row boxes its text instead
        let alpha: CGFloat = item.copy.isEmpty || !item.cells.isEmpty ? 0 : pressed ? 0.08 : hovered ? 0.05 : 0
        var color: CGColor?
        effectiveAppearance.performAsCurrentDrawingAppearance {
            color = NSColor.labelColor.withAlphaComponent(alpha).cgColor
        }
        layer?.backgroundColor = color
        layer?.cornerRadius = 6
    }

    // label colors are dynamic, so building the text at draw time follows light and dark
    // a typeset row: each cell centred on the row's height, the note at the right
    private func drawTypeset(_ cg: CGContext) {
        if let shown = copied ? copiedBox ?? boxed : boxed {
            let box = NSBezierPath(roundedRect: shown, xRadius: 4, yRadius: 4)
            NSColor.labelColor.withAlphaComponent(copied || pressed ? 0.12 : 0.05).setFill()
            box.fill()
            NSColor.labelColor.withAlphaComponent(copied ? 0.6 : 0.3).setStroke()
            box.lineWidth = 1
            box.stroke()
        }
        if item.figure {
            let origin = figureOrigin
            cg.saveGState()
            cg.translateBy(x: origin.x, y: origin.y)
            item.cells.first?.draw(cg)
            for spot in item.spots {
                cg.saveGState()
                cg.translateBy(x: spot.center.x - spot.drawn.size.width / 2, y: spot.center.y - spot.drawn.size.height / 2)
                spot.drawn.draw(cg)
                cg.restoreGState()
            }
            cg.restoreGState()
            return
        }
        var x: CGFloat = 6
        let place: (IdentitySheet.Drawn, CGFloat) -> Void = { [bounds] cell, at in
            cg.saveGState()
            cg.translateBy(x: at, y: (bounds.height - cell.size.height) / 2)
            cell.draw(cg)
            cg.restoreGState()
        }
        cg.saveGState()
        if copied || isHeading { cg.setAlpha(0.6) }
        for part in item.parts {
            place(part.drawn, x)
            x += part.drawn.size.width + Self.partGap
        }
        for (i, cell) in item.cells.enumerated() where item.parts.isEmpty {
            place(cell, x)
            x += i < leadWidths.count ? leadWidths[i] : leadWidths.last ?? 0
        }
        cg.restoreGState()
        if copied {
            let tick = NSAttributedString(string: "✓", attributes: [.font: Self.small, .foregroundColor: NSColor.secondaryLabelColor])
            tick.draw(at: NSPoint(x: bounds.width - 6 - tick.size().width, y: (bounds.height - tick.size().height) / 2))
        } else if let note = item.drawnNote {
            cg.saveGState()
            cg.setAlpha(0.65)
            place(note, bounds.width - 6 - note.size.width)
            cg.restoreGState()
        } else if !item.note.isEmpty {
            // words in the app's own font
            let note = NSAttributedString(string: item.note, attributes: [.font: Self.small, .foregroundColor: NSColor.secondaryLabelColor])
            note.draw(at: NSPoint(x: bounds.width - 6 - note.size().width, y: (bounds.height - note.size().height) / 2))
        }
    }

    override func draw(_ dirtyRect: NSRect) {
        if !item.cells.isEmpty, let cg = NSGraphicsContext.current?.cgContext {
            drawTypeset(cg)
            return
        }
        let ink: NSColor = copied || isHeading ? .secondaryLabelColor : .labelColor
        let math = Self.math(item.runs, ink)
        let side = copied ? "✓" : item.note
        let note = NSAttributedString(string: side, attributes: [.font: Self.small, .foregroundColor: NSColor.secondaryLabelColor])
        let noteSize = note.size()
        // both share the plain text's baseline, so a raised run doesn't push the line down
        let baseline = (bounds.height + Self.font.ascender + Self.font.descender) / 2
        var x: CGFloat = 6
        for (i, cell) in item.lead.enumerated() {
            Self.math(cell, ink).draw(at: NSPoint(x: x, y: baseline - Self.font.ascender))
            x += i < leadWidths.count ? leadWidths[i] : leadWidths.last ?? 0
        }
        math.draw(at: NSPoint(x: x, y: baseline - Self.font.ascender))
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

// One window of sheets, a page at a time: a row that opens a sheet (`formulas`) replaces the page, and back and forward
// (⌘[ ⌘], the header's arrows, a sideways swipe) step through what was shown. Each window keeps its own pages.
final class IdentityWindowController: NSObject, NSSearchFieldDelegate, NSWindowDelegate {
    // reuses the periodic table's panel: never key, so the overlay keeps its caret and stays up while rows are clicked
    private var panel: PeriodicPanel?
    private var status: NSTextField?
    private var copiedRow: IdentityRow?
    private var clearCopied: DispatchWorkItem?
    private var settingsObserver: NSObjectProtocol?
    // a row that opens something (the formulas page): the bar line to run
    var onOpen: ((String) -> Void)?
    // the same line, from a row's Open in New Window; with none, rows have no menu
    var onOpenNew: ((String) -> Void)?
    // the pages shown in this window, and which one is up; each remembers where it was scrolled to
    private struct Page {
        let sheet: IdentitySheet
        var scrolled: CGFloat = 0
    }
    private var pages: [Page] = []
    private var at = -1
    private var heading: NSTextField?
    private var backButton: NSButton?
    private var forwardButton: NSButton?
    private var idleHint = IdentityWindowController.hint
    private var hasFigure = false
    // the general pasteboard, except when a dev build's test clicks use one of their own
    static var board = NSPasteboard.general
    private var scroll: NSScrollView?
    // each section's top in the scrolled content, for opening at one (`idt summation`)
    private var sectionTops: [Int: CGFloat] = [:]
    // the sheet, kept to lay out again as the filter changes
    private var sheet: IdentitySheet?
    // typing while the sheet has the keys filters its rows; Esc clears it, then closes
    private var filterField: NSSearchField?
    private var keyObserver: NSObjectProtocol?
    private let id: String
    // a second sheet opens a little down and right of the first rather than on top of it
    private let cascade: Int

    private let inset: CGFloat = 16
    // the header, then the filter field under it
    private let top: CGFloat = 60
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
        if let keyObserver {
            NotificationCenter.default.removeObserver(keyObserver)
        }
    }

    // a second open just brings the same window forward, at the section asked for. `bar` is the bar's frame and the
    // visible frame of its screen: a sheet that would cover the bar moves beside it
    // another sheet replaces the page, the one before a step back; a window that was closed starts its pages afresh
    func show(_ sheet: IdentitySheet, section: Int = 0, bar: (frame: NSRect, visible: NSRect)? = nil) {
        if panel == nil {
            build(sheet)
        } else if panel?.isVisible != true {
            pages = [Page(sheet: sheet)]
            at = 0
            render()
        } else if pages.indices.contains(at), pages[at].sheet.id != sheet.id {
            pages[at].scrolled = scroll?.contentView.bounds.minY ?? 0
            // the last 50 pages are plenty to go back through
            pages = Array((Array(pages[...at]) + [Page(sheet: sheet)]).suffix(50))
            at = pages.count - 1
            render()
        }
        applyAppearance()
        // a picture opens at its full size, even over the bar, rather than cut down to fit beside it
        if !hasFigure, let panel, let bar, SheetPlace.needsMoving(panel.frame, bar: bar.frame, visible: bar.visible) {
            let least = panel.frameRect(forContentRect: NSRect(origin: .zero, size: panel.contentMinSize)).height
            panel.setFrame(SheetPlace.beside(panel.frame.size, bar: bar.frame, visible: bar.visible, give: .height(min: least)), display: true)
        }
        scrollTo(section)
        panel?.orderFrontRegardless()
    }

    private func scrollTo(_ section: Int) {
        guard let scroll, let content = scroll.documentView, let at = sectionTops[section] else { return }
        let most = max(0, content.frame.height - scroll.contentView.bounds.height)
        scroll.contentView.scroll(to: NSPoint(x: 0, y: min(at, most)))
        scroll.reflectScrolledClipView(scroll.contentView)
    }

    // the rows a filter leaves, laid out afresh; the figure is never filtered
    private func layout(_ content: NSView, _ sheet: IdentitySheet, filter: String) {
        content.subviews.forEach { $0.removeFromSuperview() }
        sectionTops = [:]
        idleHint = Self.hint
        let width = content.frame.width
        let words = Self.searchWords(filter)
        var y: CGFloat = 0
        var matched = false
        for (index, section) in sheet.sections.enumerated() {
            // a picture needs no headings over it
            let bare = section.groups.contains { $0.items.contains(where: \.figure) }
            // each group's rows the filter keeps: a heading row stays when its table has a match below it
            let groups: [(IdentitySheet.Group, [IdentitySheet.Item])] = section.groups.compactMap { group in
                if words.isEmpty || bare { return (group, group.items) }
                let kept = group.items.filter { item in
                    item.copy.isEmpty || Self.matches(words, [section.title, group.title] + Self.searchText(item))
                }
                if !kept.contains(where: { !$0.copy.isEmpty }) { return nil }
                matched = true
                return (group, kept)
            }
            if groups.isEmpty { continue }
            sectionTops[index] = y
            let titleFrame = NSRect(x: inset, y: y + 4, width: width - inset * 2, height: 16)
            if !bare {
                let title = label(section.title, size: 12, weight: .semibold, color: .labelColor)
                title.frame = titleFrame
                content.addSubview(title)
                let rule = NSBox(frame: NSRect(x: inset, y: y + sectionTitle - 3, width: width - inset * 2, height: 1))
                rule.boxType = .separator
                content.addSubview(rule)
            }
            // each group goes in whichever column is shorter so far; a wide section has the one, the sheet's width
            let first = bare ? y - 6 : y + sectionTitle
            var columns: [CGFloat] = section.wide ? [first] : [first, first]
            let span = section.wide ? width - inset * 2 : column
            for (group, items) in groups {
                let c = columns.count > 1 && columns[1] < columns[0] ? 1 : 0
                let x = inset + CGFloat(c) * (column + inset)
                var gy = columns[c] + 6
                let nameFrame = NSRect(x: x + 6, y: gy + 2, width: span - 6, height: 14)
                if !bare {
                    let name = label(group.title, size: 11, weight: .medium, color: .secondaryLabelColor)
                    name.frame = nameFrame
                    content.addSubview(name)
                }
                if !bare { gy += groupTitle }
                for item in items {
                    let height = IdentityRow.height(item, least: rowHeight)
                    let row = IdentityRow(item, leadWidths: section.leadWidths, frame: NSRect(x: x, y: gy, width: span, height: height))
                    if !item.open.isEmpty {
                        row.onPick = { [weak self] in self?.onOpen?($0.item.open) }
                        if onOpenNew != nil { row.onOpenNew = { [weak self] in self?.onOpenNew?($0.item.open) } }
                        idleHint = "click to open"
                    } else if !item.copy.isEmpty || item.figure {
                        row.onPick = { [weak self] in self?.copy($0) }
                    }
                    content.addSubview(row)
                    gy += height
                }
                columns[c] = gy
            }
            y = (columns.max() ?? y) + 14
        }
        // nothing left but (perhaps) the figure: a quiet line says so
        // a sheet that is only a picture has nothing to filter, so it never says so
        let onlyFigure = sheet.sections.allSatisfy { $0.groups.contains { $0.items.contains(where: \.figure) } }
        if !words.isEmpty, !matched, !onlyFigure {
            let none = label("no matches", size: 12, weight: .regular, color: .tertiaryLabelColor)
            none.alignment = .center
            none.frame = NSRect(x: inset, y: y + 10, width: width - inset * 2, height: 16)
            content.addSubview(none)
            y += 40
        }
        content.setFrameSize(NSSize(width: width, height: max(0, y + inset - 14)))
    }

    // what a filter is matched on: the row's words, its note and what it copies (symbols such as τ included)
    private static func searchText(_ item: IdentitySheet.Item) -> [String] {
        let runs = (item.lead + [item.runs]).map { $0.map(\.text).joined() }
        return runs + [item.note, item.copy, item.open] + item.leadCopy + item.parts.map(\.copy)
    }

    private static func fold(_ text: String) -> String {
        text.folding(options: [.caseInsensitive, .diacriticInsensitive, .widthInsensitive], locale: nil)
            .replacingOccurrences(of: "\u{2212}", with: "-")
    }

    // letters and digits alone, so `tmax` finds τ_max's `t_max` and `x2` finds x^{2}
    private static func compact(_ text: String) -> String {
        String(text.unicodeScalars.filter { CharacterSet.alphanumerics.contains($0) }.map(Character.init))
    }

    private static func searchWords(_ filter: String) -> [String] {
        fold(filter).split(whereSeparator: \.isWhitespace).map(String.init)
    }

    // every word of the filter is somewhere in the row, its group's or its section's title
    private static func matches(_ words: [String], _ texts: [String]) -> Bool {
        let hay = fold(texts.joined(separator: " "))
        // squeezed a word at a time, so `sin` doesn't run across `s in`; and again with greek spelled in latin
        // (τ is t, σ is s, θ is th), so `tmax` finds τ_max while a typed `τ` still finds only τ
        let squeezed = hay.split(whereSeparator: \.isWhitespace).map { compact(String($0)) }
        let tight = squeezed + squeezed.map(latin).filter { !squeezed.contains($0) }
        return words.allSatisfy { word in
            hay.contains(word) || { let w = compact(word); return !w.isEmpty && tight.contains { $0.contains(w) } }()
        }
    }

    // greek in latin letters (ICU's Greek-Latin), folded again: τ_max is t_max, ω is o, Δ is d
    private static func latin(_ text: String) -> String {
        fold(text.applyingTransform(.toLatin, reverse: false) ?? text)
    }

    /// the filter as if typed (the dev build's `sheetfilter:` command, which works while q calc isn't in front)
    func setFilter(_ text: String) {
        filterField?.stringValue = text
        refilter()
    }

    // the filter changed: the rows again, from the top
    private var shownFilter = ""
    private func refilter() {
        guard let sheet, let content = scroll?.documentView else { return }
        let text = filterField?.stringValue ?? ""
        guard text != shownFilter else { return }
        shownFilter = text
        copiedRow = nil
        layout(content, sheet, filter: text)
        scroll?.contentView.scroll(to: .zero)
        scroll?.reflectScrolledClipView(scroll!.contentView)
    }

    private func build(_ sheet: IdentitySheet) {
        self.sheet = sheet
        pages = [Page(sheet: sheet)]
        at = 0
        let width = inset * 3 + column * 2
        let content = IdentitySheetView(frame: NSRect(x: 0, y: 0, width: width, height: 0))
        layout(content, sheet, filter: "")

        let full = content.frame.height + top
        hasFigure = sheet.sections.contains { $0.groups.contains { $0.items.contains(where: \.figure) } }
        let tallest = hasFigure ? max(maxHeight, (NSScreen.main?.visibleFrame.height ?? maxHeight) - 8) : maxHeight
        let size = NSSize(width: width, height: min(full, tallest))
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
        // a row boxes what's under the pointer, even with the window not key
        panel.acceptsMouseMovedEvents = true
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
        root.controller = self
        let status = label(idleHint, size: 11, weight: .regular, color: .secondaryLabelColor)
        status.alignment = .left
        let heading = label(sheet.title, size: 13, weight: .semibold, color: .labelColor)
        heading.alignment = .right
        heading.frame = NSRect(x: 0, y: 7, width: width / 2 - 4, height: 17)
        status.frame = NSRect(x: width / 2 + 4, y: 9, width: width / 2 - 4, height: 14)
        heading.autoresizingMask = [.maxYMargin]
        status.autoresizingMask = [.maxYMargin]
        root.addSubview(heading)
        root.addSubview(status)
        self.status = status
        self.heading = heading
        // back and forward, beside the close button, once there's somewhere to go
        let back = arrow("chevron.left", "Back (⌘[)", #selector(goBack), x: 30)
        let forward = arrow("chevron.right", "Forward (⌘])", #selector(goForward), x: 52)
        root.addSubview(back)
        root.addSubview(forward)
        backButton = back
        forwardButton = forward

        // the filter, in the app's font; its ⓧ clears it
        let filter = NSSearchField(frame: NSRect(x: inset, y: 31, width: width - inset * 2, height: 22))
        filter.font = .systemFont(ofSize: 13)
        filter.placeholderString = "Filter"
        filter.focusRingType = .none
        filter.sendsWholeSearchString = false
        filter.sendsSearchStringImmediately = true
        filter.delegate = self
        // the ⓧ sends the action with no text change of its own
        filter.target = self
        filter.action = #selector(filterChanged)
        filter.autoresizingMask = [.maxYMargin, .width]
        filter.setAccessibilityLabel("Filter \(sheet.title)")
        root.addSubview(filter)
        filterField = filter

        let scroll = IdentityScroll(frame: NSRect(x: 0, y: top, width: width, height: size.height - top))
        scroll.controller = self
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.documentView = content
        scroll.autoresizingMask = [.width, .height]
        root.addSubview(scroll)
        self.scroll = scroll
        panel.contentView = root

        let frameName = "IdentitySheet-\(id)"
        if !panel.setFrameUsingName(frameName) {
            panel.center()
            let offset = CGFloat(cascade) * 28
            panel.setFrameOrigin(NSPoint(x: panel.frame.minX + offset, y: panel.frame.minY - offset))
        }
        panel.setFrameAutosaveName(frameName)
        panel.delegate = self
        self.panel = panel
        updateArrows()
        panel.initialFirstResponder = filter
        // whenever the sheet takes the keys, they go to the filter
        keyObserver = NotificationCenter.default.addObserver(forName: NSWindow.didBecomeKeyNotification, object: panel, queue: .main) {
            [weak self] _ in
            guard let self, let panel = self.panel, let filter = self.filterField, filter.currentEditor() == nil else { return }
            panel.makeFirstResponder(filter)
        }
        observeSettings()
    }

    private func arrow(_ symbol: String, _ tip: String, _ action: Selector, x: CGFloat) -> NSButton {
        let button = NSButton(frame: NSRect(x: x, y: 5, width: 20, height: 20))
        button.image = NSImage(systemSymbolName: symbol, accessibilityDescription: tip)?
            .withSymbolConfiguration(.init(pointSize: 12, weight: .semibold))
        button.isBordered = false
        button.imagePosition = .imageOnly
        button.contentTintColor = .secondaryLabelColor
        button.toolTip = tip
        button.setAccessibilityLabel(tip.components(separatedBy: " (").first)
        button.target = self
        button.action = action
        button.autoresizingMask = [.maxYMargin]
        return button
    }

    @objc private func goBack() { _ = go(-1) }
    @objc private func goForward() { _ = go(1) }

    var isOpen: Bool { panel?.isVisible == true }
    var canGoBack: Bool { at > 0 }
    var canGoForward: Bool { at + 1 < pages.count }
    var pageTitles: [String] { pages.map(\.sheet.title) }
    var pageIndex: Int { at }

    /// a step back (-1) or forward (1) through this window's pages; false when there's nowhere to go
    @discardableResult
    func go(_ step: Int) -> Bool {
        let to = at + step
        guard pages.indices.contains(to), pages.indices.contains(at) else { return false }
        pages[at].scrolled = scroll?.contentView.bounds.minY ?? 0
        at = to
        render()
        if let scroll, let content = scroll.documentView {
            let most = max(0, content.frame.height - scroll.contentView.bounds.height)
            scroll.contentView.scroll(to: NSPoint(x: 0, y: min(pages[at].scrolled, most)))
            scroll.reflectScrolledClipView(scroll.contentView)
        }
        return true
    }

    // the page that's up, laid out afresh with the filter cleared; the window keeps its top edge and takes the page's height
    private func render() {
        guard pages.indices.contains(at), let panel, let scroll, let content = scroll.documentView else { return }
        let sheet = pages[at].sheet
        self.sheet = sheet
        clearCopied?.cancel()
        copiedRow = nil
        filterField?.stringValue = ""
        shownFilter = ""
        filterField?.setAccessibilityLabel("Filter \(sheet.title)")
        layout(content, sheet, filter: "")
        status?.stringValue = idleHint
        heading?.stringValue = sheet.title
        panel.title = sheet.title
        hasFigure = sheet.sections.contains { $0.groups.contains { $0.items.contains(where: \.figure) } }
        let full = content.frame.height + top
        let visible = (panel.screen ?? NSScreen.main)?.visibleFrame
        let tallest = hasFigure ? max(maxHeight, (visible?.height ?? maxHeight) - 8) : maxHeight
        let width = content.frame.width
        panel.contentMinSize = NSSize(width: width, height: min(full, 220))
        panel.contentMaxSize = NSSize(width: width, height: full)
        var frame = panel.frameRect(forContentRect: NSRect(origin: .zero, size: NSSize(width: width, height: min(full, tallest))))
        frame.origin = NSPoint(x: panel.frame.minX, y: panel.frame.maxY - frame.height)
        // a taller page that would run off the bottom moves up
        if let visible, frame.minY < visible.minY { frame.origin.y = min(visible.minY, visible.maxY - frame.height) }
        panel.setFrame(frame, display: true)
        scroll.contentView.scroll(to: .zero)
        scroll.reflectScrolledClipView(scroll.contentView)
        slide(0)
        updateArrows()
    }

    private func updateArrows() {
        let any = pages.count > 1
        backButton?.isHidden = !any
        forwardButton?.isHidden = !any
        backButton?.isEnabled = canGoBack
        forwardButton?.isEnabled = canGoForward
    }

    // the page follows the fingers a little while a swipe is tracked
    private func slide(_ amount: CGFloat) {
        guard let scroll else { return }
        scroll.frame.origin.x = amount * 90
        scroll.alphaValue = 1 - min(1, abs(amount)) * 0.6
    }

    /// a sideways two-finger swipe, when the trackpad's "Swipe between pages" is on: true when it's taken here
    // a gesture's first event often moves nowhere yet, so its first move decides
    private var swipeUndecided = false

    func trackSwipe(_ event: NSEvent) -> Bool {
        if event.phase == .began { swipeUndecided = true }
        guard swipeUndecided, event.phase == .began || event.phase == .changed else { return false }
        if event.scrollingDeltaX == 0, event.scrollingDeltaY == 0 { return false }
        swipeUndecided = false
        guard NSEvent.isSwipeTrackingFromScrollEventsEnabled, event.hasPreciseScrollingDeltas,
              abs(event.scrollingDeltaX) > abs(event.scrollingDeltaY), canGoBack || canGoForward else { return false }
        // positive is a swipe to the right, back; negative forward
        var last: CGFloat = 0
        event.trackSwipeEvent(options: [.lockDirection, .clampGestureAmount],
                              dampenAmountThresholdMin: canGoForward ? -1 : 0, max: canGoBack ? 1 : 0) { [weak self] amount, phase, complete, _ in
            guard let self else { return }
            if amount != 0 { last = amount }
            self.slide(amount)
            if phase == .ended { self.finishSwipe(last) }
            if complete { self.slide(0) }
        }
        return true
    }

    /// where a swipe that went through lands: what its gesture amount's sign says (dev builds drive this directly)
    func finishSwipe(_ amount: CGFloat) {
        if amount > 0 { go(-1) } else if amount < 0 { go(1) }
    }

    /// ⌘[ is back and ⌘] forward
    static func navigationStep(_ event: NSEvent) -> Int? {
        guard event.type == .keyDown, event.modifierFlags.intersection([.command, .option, .control, .shift]) == [.command] else { return nil }
        switch event.keyCode {
        case 33: return -1 // kVK_ANSI_LeftBracket
        case 30: return 1 // kVK_ANSI_RightBracket
        default: return nil
        }
    }

    /// the sheet window in front, if it is one
    static func front() -> IdentityWindowController? {
        PeriodicPanel.front()?.delegate as? IdentityWindowController
    }

    /// dev builds: the row at a point in the page's content (from its top left) and its Open in New Window
    func openNew(at point: NSPoint) -> Bool {
        guard let content = scroll?.documentView else { return false }
        let p = NSPoint(x: point.x, y: point.y + (scroll?.contentView.bounds.minY ?? 0) - top)
        guard let row = content.subviews.compactMap({ $0 as? IdentityRow }).first(where: { $0.frame.contains(p) }),
              let event = NSEvent.mouseEvent(with: .rightMouseDown, location: .zero, modifierFlags: [], timestamp: 0,
                                             windowNumber: panel?.windowNumber ?? 0, context: nil, eventNumber: 0, clickCount: 1, pressure: 1),
              row.menu(for: event)?.items.contains(where: { $0.title == "Open in New Window" }) == true else { return false }
        row.openNew()
        return true
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
        let text = row.pickedCopy
        // on a figure, between its labels
        guard !text.isEmpty else { return }
        Self.board.clearContents()
        Self.board.setString(text, forType: .string)
        copiedRow?.copied = false
        row.copied = true
        copiedRow = row
        status?.stringValue = "✓ copied"
        clearCopied?.cancel()
        let clear = DispatchWorkItem { [weak self] in
            self?.copiedRow?.copied = false
            self?.copiedRow = nil
            self?.status?.stringValue = self?.idleHint ?? Self.hint
        }
        clearCopied = clear
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.25, execute: clear)
    }

    // Esc clears the filter first, then closes the sheet
    func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
        guard control === filterField, selector == #selector(NSResponder.cancelOperation(_:)) else { return false }
        if filterField?.stringValue.isEmpty == false {
            filterField?.stringValue = ""
            refilter()
        } else {
            panel?.close()
        }
        return true
    }

    func controlTextDidChange(_ note: Notification) {
        refilter()
    }

    @objc private func filterChanged() {
        refilter()
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
