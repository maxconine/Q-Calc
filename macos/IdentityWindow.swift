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
        // the native app's explanation, opened by a click beside the copy
        var explain: Explanation?
    }

    // what a row says, when it holds, what each letter is and a worked example (core `Explanation`); its text
    // raises `^{…}` and lowers `_{…}` as a row's `show` does
    struct Explanation {
        let title: String
        let about: String
        let assumptions: [String]
        // symbol, meaning, unit
        let vars: [[String]]
        // formula, when it's used
        let related: [[String]]
        let problem: String
        let steps: [String]
        let answer: String
        // a bar line that works the example out
        let tryLine: String

        init?(_ any: Any?) {
            guard let d = any as? [String: Any], let title = d["title"] as? String else { return nil }
            self.title = title
            about = d["about"] as? String ?? ""
            assumptions = d["assumptions"] as? [String] ?? []
            vars = d["vars"] as? [[String]] ?? []
            related = d["related"] as? [[String]] ?? []
            let example = d["example"] as? [String: Any] ?? [:]
            problem = example["problem"] as? String ?? ""
            steps = example["steps"] as? [String] ?? []
            answer = example["answer"] as? String ?? ""
            tryLine = d["try"] as? String ?? ""
        }
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
                                                     },
                                                     explain: Explanation(i["explain"]))
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

// ⌘ and a key, by the layout's letter or, on a layout without latin letters, where it sits on qwerty (`code`), with
// shift only when asked for
private func sheetCommand(_ event: NSEvent, _ letter: String, _ code: UInt16, shift: Bool = false) -> Bool {
    guard event.type == .keyDown else { return false }
    guard event.modifierFlags.intersection([.command, .option, .control, .shift]) == (shift ? [.command, .shift] : [.command]) else {
        return false
    }
    let chars = event.charactersIgnoringModifiers?.lowercased() ?? ""
    if chars == letter { return true }
    return !chars.isEmpty && !chars.unicodeScalars.allSatisfy(\.isASCII) && event.keyCode == code
}

// cut, copy, paste, select all, undo and redo for the text with the keys: q calc has no menu bar while the bar is
// up, so nothing sent them (⌘V didn't paste in a sheet's filter, ⌘C didn't copy from an explanation)
private func sheetEditKey(_ event: NSEvent, in window: NSWindow?) -> Bool {
    let keys: [(String, UInt16, Bool, Selector)] = [
        ("x", 7, false, #selector(NSText.cut(_:))), ("c", 8, false, #selector(NSText.copy(_:))),
        ("v", 9, false, #selector(NSText.paste(_:))), ("a", 0, false, #selector(NSText.selectAll(_:))),
        ("z", 6, false, Selector(("undo:"))), ("z", 6, true, Selector(("redo:"))),
    ]
    guard let responder = window?.firstResponder,
          let action = keys.first(where: { sheetCommand(event, $0.0, $0.1, shift: $0.2) })?.3 else { return false }
    return responder.tryToPerform(action, with: nil)
}

// the window's own content, flipped so the header stays pinned to the top as it resizes
private final class IdentityRoot: NSView {
    override var isFlipped: Bool { true }
    weak var controller: IdentityWindowController?

    // ⌘[ and ⌘], while the sheet has the keys (the bar sends them on while it's up: OverlayController); ⌘F goes to
    // the filter; and the edit keys, which the filter had no menu to send them (⌘V didn't paste)
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if let step = IdentityWindowController.navigationStep(event), let controller, controller.navigate(step) { return true }
        if sheetCommand(event, "f", 3), let controller { controller.focusFilter(); return true }
        if sheetEditKey(event, in: window) { return true }
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

// An explanation's text, its links reached with the keys as a web page's are: Tab selects the next link (⇧Tab the
// one before), scrolled into view, past the last one the keys go on to Back and Try it; ↵ or space follows the
// selected link. VoiceOver reads each link as a link (NSTextView's own AXLink children, named by their text).
final class LinkTextView: NSTextView {
    /// every link's range, in order
    var linkRanges: [NSRange] {
        guard let storage = textStorage else { return [] }
        var out: [NSRange] = []
        storage.enumerateAttribute(.link, in: NSRange(location: 0, length: storage.length)) { value, range, _ in
            if value != nil { out.append(range) }
        }
        return out
    }

    /// the link the selection is on, if any
    var selectedLink: NSRange? {
        let at = selectedRange()
        return linkRanges.first { NSIntersectionRange($0, at).length > 0 || (at.length == 0 && NSLocationInRange(at.location, $0)) }
    }

    override func keyDown(with event: NSEvent) {
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        guard flags.isDisjoint(with: [.command, .control, .option]) else { return super.keyDown(with: event) }
        switch event.keyCode {
        case 48: // tab
            stepLink(flags.contains(.shift) ? -1 : 1)
        case 36, 76, 49: // return, enter, space
            if let link = selectedLink, followLink(link) { return }
            super.keyDown(with: event)
        default:
            super.keyDown(with: event)
        }
    }

    // the next (or previous) link from the selection; none left, the keys go on to the next key view and the
    // selection starts over, so coming back round lands on the first link again
    func stepLink(_ step: Int) {
        let links = linkRanges
        let at = selectedRange()
        let from = at.length > 0 ? NSMaxRange(at) : at.location
        let next = step > 0 ? links.first { $0.location >= from } : links.last { $0.location < at.location }
        guard let next else {
            setSelectedRange(NSRange(location: 0, length: 0))
            if step > 0 { window?.selectNextKeyView(self) } else { window?.selectPreviousKeyView(self) }
            return
        }
        setSelectedRange(next)
        scrollRangeToVisible(next)
        // VoiceOver: say it's a link, with its words, as Safari's rotor would
        let words = (string as NSString).substring(with: next)
        NSAccessibility.post(element: self, notification: .announcementRequested,
                             userInfo: [.announcement: "\(words), link", .priority: NSAccessibilityPriorityLevel.high.rawValue])
    }

    @discardableResult
    func followLink(_ range: NSRange) -> Bool {
        guard let link = textStorage?.attribute(.link, at: range.location, effectiveRange: nil) else { return false }
        clicked(onLink: link, at: range.location)
        return true
    }
}

// A row's explanation: the equation, what it's for, when it holds, each letter, a worked example and, last, related
// formulas, scrolling past 520 pt; a Try button puts the example's line in the bar. Each related formula is a link
// (`onRelated`, its index): to its own explanation where it has one, else a search for it. Back returns to the
// explanation that linked here.
private final class ExplanationView: NSView, NSTextViewDelegate {
    private static let width: CGFloat = 420
    private static let pad: CGFloat = 16
    private static let tallest: CGFloat = 520
    private static let linkScheme = "qcalc-related"
    private let onTry: ((String) -> Void)?
    private let onRelated: ((Int) -> Void)?
    private let onBack: (() -> Void)?
    private let tryLine: String
    // the popover's keys once a click gave it them: ⌘W closes the sheet, ⌘[ and ⌘] step through the explanations
    var onClose: (() -> Void)?
    var onStep: ((Int) -> Bool)?
    /// the explanation's text, which takes the keys in the popover (Tab through its Related links, ↵ follows one)
    let textView: LinkTextView

    init(_ e: IdentitySheet.Explanation, equation item: IdentitySheet.Item, onTry: ((String) -> Void)?,
         onRelated: ((Int) -> Void)? = nil, onBack: (() -> Void)? = nil) {
        self.onTry = onTry
        self.onRelated = onRelated
        self.onBack = onBack
        tryLine = e.tryLine
        let inner = Self.width - Self.pad * 2
        let view = LinkTextView(frame: NSRect(x: 0, y: 0, width: inner, height: 10))
        textView = view
        super.init(frame: .zero)
        let text = Self.text(e, item, links: onRelated != nil)
        view.isEditable = false
        view.isSelectable = true
        view.drawsBackground = false
        view.textContainerInset = .zero
        view.textContainer?.lineFragmentPadding = 0
        view.textContainer?.widthTracksTextView = true
        view.textStorage?.setAttributedString(text)
        // the selection starts at the top, so the first Tab is the first link
        view.setSelectedRange(NSRange(location: 0, length: 0))
        view.linkTextAttributes = [.foregroundColor: NSColor.controlAccentColor, .cursor: NSCursor.pointingHand]
        view.delegate = self
        view.setAccessibilityLabel(e.title)
        view.layoutManager?.ensureLayout(for: view.textContainer!)
        let used = ceil(view.layoutManager?.usedRect(for: view.textContainer!).height ?? 100)
        view.frame.size.height = used
        let footer: CGFloat = onTry == nil && onBack == nil ? 0 : 40
        let shown = min(used, Self.tallest - footer - Self.pad * 2)
        let scroll = NSScrollView(frame: NSRect(x: Self.pad, y: footer + Self.pad, width: inner, height: shown))
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = used > shown
        scroll.autohidesScrollers = true
        scroll.documentView = view
        addSubview(scroll)
        frame.size = NSSize(width: Self.width, height: shown + footer + Self.pad * 2)
        var lineStart = Self.pad
        if onBack != nil {
            let back = NSButton(title: "‹ Back", target: self, action: #selector(goBack))
            back.bezelStyle = .rounded
            back.sizeToFit()
            back.frame.origin = NSPoint(x: Self.pad, y: Self.pad - 2)
            back.toolTip = "Back to the explanation before"
            addSubview(back)
            lineStart = back.frame.maxX + 10
        }
        if onTry != nil {
            let button = NSButton(title: "Try it in the bar", target: self, action: #selector(tryIt))
            button.bezelStyle = .rounded
            button.sizeToFit()
            button.frame.origin = NSPoint(x: Self.width - Self.pad - button.frame.width, y: Self.pad - 2)
            button.toolTip = e.tryLine
            addSubview(button)
            let line = NSTextField(labelWithString: e.tryLine)
            line.font = .monospacedSystemFont(ofSize: 11, weight: .regular)
            line.textColor = .secondaryLabelColor
            line.lineBreakMode = .byTruncatingTail
            line.frame = NSRect(x: lineStart, y: Self.pad + 4, width: max(0, button.frame.minX - lineStart - 8), height: 16)
            addSubview(line)
        }
        DispatchQueue.main.async { view.scroll(.zero) }
    }

    required init?(coder: NSCoder) { nil }

    @objc private func tryIt() {
        onTry?(tryLine)
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if sheetCommand(event, "w", 13), let onClose { onClose(); return true }
        if let step = IdentityWindowController.navigationStep(event), onStep?(step) == true { return true }
        if sheetEditKey(event, in: window) { return true }
        return super.performKeyEquivalent(with: event)
    }

    @objc private func goBack() {
        onBack?()
    }

    func textView(_ textView: NSTextView, clickedOnLink link: Any, at charIndex: Int) -> Bool {
        guard let url = link as? URL, url.scheme == Self.linkScheme, let i = Int(url.host ?? "") else { return false }
        onRelated?(i)
        return true
    }

    private static let body = NSFont.systemFont(ofSize: 12.5)

    /// the popover's text took the keys: Tab lands on the first Related link (or the text, when it has none)
    func takeKeys() {
        guard let window = textView.window else { return }
        window.makeKey()
        window.makeFirstResponder(textView)
    }

    // the whole explanation as one text
    private static func text(_ e: IdentitySheet.Explanation, _ item: IdentitySheet.Item, links: Bool) -> NSAttributedString {
        let out = NSMutableAttributedString()
        out.append(rich(e.title, size: 15, weight: .semibold))
        out.append(NSAttributedString(string: "\n"))
        out.append(equation(item))
        if !e.about.isEmpty {
            out.append(paragraph(rich(e.about, size: 12.5), space: 8))
        }
        if !e.assumptions.isEmpty {
            out.append(heading("Assumptions"))
            for a in e.assumptions { out.append(bullet(rich(a, size: 12.5))) }
        }
        if !e.vars.isEmpty {
            out.append(heading("Variables"))
            let width = e.vars.map { rich($0.first ?? "", size: 13).size().width }.max() ?? 20
            let column = min(max(width + 12, 36), 110)
            for v in e.vars where v.count >= 2 {
                let line = NSMutableAttributedString()
                line.append(rich(v[0], size: 13))
                line.append(NSAttributedString(string: "\t"))
                line.append(rich(v[1], size: 12.5))
                if v.count > 2, !v[2].isEmpty {
                    line.append(rich(", in " + v[2], size: 12, color: .secondaryLabelColor))
                }
                out.append(table(line, column: column))
            }
        }
        if !e.problem.isEmpty {
            out.append(heading("Example"))
            out.append(paragraph(rich(e.problem, size: 12.5), space: 2))
            for (i, step) in e.steps.enumerated() {
                let line = NSMutableAttributedString(string: "\(i + 1).\t", attributes: [.font: body, .foregroundColor: NSColor.secondaryLabelColor])
                line.append(rich(step, size: 12.5))
                out.append(table(line, column: 20))
            }
            if !e.answer.isEmpty {
                let line = NSMutableAttributedString(string: "Answer  ", attributes: [.font: NSFont.systemFont(ofSize: 12.5, weight: .semibold), .foregroundColor: NSColor.labelColor])
                line.append(rich(e.answer, size: 12.5, weight: .semibold))
                out.append(paragraph(line, space: 6))
            }
        }
        if !e.related.isEmpty {
            out.append(heading("Related"))
            for (i, r) in e.related.enumerated() where !r.isEmpty {
                let line = NSMutableAttributedString(attributedString: rich(r[0], size: 13))
                // the formula is the link; its note stays plain
                if links, let url = URL(string: "\(linkScheme)://\(i)") {
                    line.addAttribute(.link, value: url, range: NSRange(location: 0, length: line.length))
                }
                if r.count > 1, !r[1].isEmpty {
                    line.append(NSAttributedString(string: "   "))
                    line.append(rich(r[1], size: 12, color: .secondaryLabelColor))
                }
                out.append(bullet(line))
            }
        }
        return out
    }

    // the row's equation, typeset as the sheet draws it where it can be, centred
    private static func equation(_ item: IdentitySheet.Item) -> NSAttributedString {
        let gap: CGFloat = 24
        let line: NSAttributedString
        if !item.cells.isEmpty {
            let cells = item.cells
            let size = NSSize(width: cells.map(\.size.width).reduce(0, +) + gap * CGFloat(cells.count - 1),
                              height: cells.map(\.size.height).max() ?? 0)
            let image = NSImage(size: size, flipped: true) { _ in
                guard let cg = NSGraphicsContext.current?.cgContext else { return false }
                var x: CGFloat = 0
                for cell in cells {
                    cg.saveGState()
                    cg.translateBy(x: x, y: (size.height - cell.size.height) / 2)
                    cell.draw(cg)
                    cg.restoreGState()
                    x += cell.size.width + gap
                }
                return true
            }
            let attachment = NSTextAttachment()
            attachment.image = image
            attachment.bounds = NSRect(origin: .zero, size: size)
            line = NSAttributedString(attachment: attachment)
        } else {
            let runs = item.lead + [item.runs]
            let text = NSMutableAttributedString()
            for (i, cell) in runs.enumerated() {
                if i > 0 { text.append(NSAttributedString(string: "    ")) }
                text.append(math(cell, size: 16, color: .labelColor, weight: .regular))
            }
            line = text
        }
        let style = NSMutableParagraphStyle()
        style.alignment = .center
        style.paragraphSpacingBefore = 10
        style.paragraphSpacing = 4
        let out = NSMutableAttributedString(attributedString: line)
        out.append(NSAttributedString(string: "\n"))
        out.addAttribute(.paragraphStyle, value: style, range: NSRange(location: 0, length: out.length))
        return out
    }

    private static func heading(_ title: String) -> NSAttributedString {
        let style = NSMutableParagraphStyle()
        style.paragraphSpacingBefore = 12
        style.paragraphSpacing = 3
        return NSAttributedString(string: title + "\n", attributes: [.font: NSFont.systemFont(ofSize: 11, weight: .semibold),
                                                                     .foregroundColor: NSColor.secondaryLabelColor, .paragraphStyle: style])
    }

    private static func paragraph(_ text: NSAttributedString, space: CGFloat) -> NSAttributedString {
        let style = NSMutableParagraphStyle()
        style.paragraphSpacingBefore = space
        style.lineSpacing = 1.5
        let out = NSMutableAttributedString(attributedString: text)
        out.append(NSAttributedString(string: "\n", attributes: [.font: body]))
        out.addAttribute(.paragraphStyle, value: style, range: NSRange(location: 0, length: out.length))
        return out
    }

    private static func bullet(_ text: NSAttributedString) -> NSAttributedString {
        let line = NSMutableAttributedString(string: "•\t", attributes: [.font: body, .foregroundColor: NSColor.secondaryLabelColor])
        line.append(text)
        return table(line, column: 14)
    }

    // a first cell, a tab, and the rest wrapping under the rest
    private static func table(_ line: NSAttributedString, column: CGFloat) -> NSAttributedString {
        let style = NSMutableParagraphStyle()
        style.tabStops = [NSTextTab(textAlignment: .left, location: column)]
        style.defaultTabInterval = column
        style.headIndent = column
        style.paragraphSpacing = 3
        style.lineSpacing = 1.5
        let out = NSMutableAttributedString(attributedString: line)
        out.append(NSAttributedString(string: "\n", attributes: [.font: body]))
        out.addAttribute(.paragraphStyle, value: style, range: NSRange(location: 0, length: out.length))
        return out
    }

    // `^{…}` raised and `_{…}` lowered, as a row's `show` reads; a raised run's own `_{…}` is lowered within it
    static func runs(_ text: String) -> [IdentitySheet.Run] {
        var out: [IdentitySheet.Run] = []
        var current = ""
        var stack: [String] = []
        let chars = Array(text)
        var i = 0
        func flush() {
            if !current.isEmpty { out.append(IdentitySheet.Run(text: current, shift: stack.last ?? "")) }
            current = ""
        }
        while i < chars.count {
            let c = chars[i]
            if (c == "^" || c == "_"), i + 1 < chars.count, chars[i + 1] == "{" {
                flush()
                let outer = stack.last ?? ""
                stack.append(outer == "sup" && c == "_" ? "supsub" : outer.isEmpty ? (c == "^" ? "sup" : "sub") : outer)
                i += 2
                continue
            }
            if c == "}", !stack.isEmpty {
                flush()
                stack.removeLast()
                i += 1
                continue
            }
            current.append(c)
            i += 1
        }
        flush()
        return out
    }

    private static func rich(_ text: String, size: CGFloat, weight: NSFont.Weight = .regular, color: NSColor = .labelColor) -> NSAttributedString {
        math(runs(text), size: size, color: color, weight: weight)
    }

    private static func math(_ runs: [IdentitySheet.Run], size: CGFloat, color: NSColor, weight: NSFont.Weight) -> NSAttributedString {
        let text = NSMutableAttributedString()
        for run in runs {
            let font: NSFont
            let offset: CGFloat
            switch run.shift {
            case "sup": (font, offset) = (.systemFont(ofSize: size * 0.73, weight: weight), size * 0.38)
            case "sub": (font, offset) = (.systemFont(ofSize: size * 0.73, weight: weight), -size * 0.23)
            case "supsub": (font, offset) = (.systemFont(ofSize: size * 0.62, weight: weight), size * 0.15)
            default: (font, offset) = (.systemFont(ofSize: size, weight: weight), 0)
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
    // an explanation's Try button: its line, into the bar
    var onTry: ((String) -> Void)?
    // every sheet (the native app's): a search lists matches from the others too, and a Related formula opens its
    // own explanation wherever it is. Nil in the web-view app, which searches the open sheet alone
    static var allSheets: (() -> [IdentitySheet])?
    // the explanations a Related link went through, for Back: what's shown, last
    private var explained: [(IdentitySheet.Explanation, IdentitySheet.Item)] = []
    // the ones ⌘[ stepped back from, for ⌘]
    private var explainedAhead: [(IdentitySheet.Explanation, IdentitySheet.Item)] = []
    // the first row a search leaves, for ↵ in the filter
    private weak var firstPick: IdentityRow?
    private weak var explainedRow: IdentityRow?
    // the explanation a click opened beside its row
    private var explainer: NSPopover?
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
    private static let explainHint = "click to copy and explain"

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

    // a search's rows from other sheets, at most this many
    private static let elsewhereMost = 60

    // the rows a filter leaves, laid out afresh; the figure is never filtered. A filter lists this sheet's matches,
    // then (the native app) every other sheet's under "<sheet> · <section>"; a row matches when it has every word,
    // or, when no row anywhere does, all but one of them ("closest matches")
    private func layout(_ content: NSView, _ sheet: IdentitySheet, filter: String) {
        content.subviews.forEach { $0.removeFromSuperview() }
        sectionTops = [:]
        firstPick = nil
        idleHint = Self.hint
        let width = content.frame.width
        let words = Self.searchWords(filter)
        var y: CGFloat = 0
        var matched = false
        // this sheet's sections (with their index, for `idt summation`), then the other sheets' while searching
        var shown: [(index: Int?, title: String, section: IdentitySheet.Section)] =
            sheet.sections.enumerated().map { ($0.offset, $0.element.title, $0.element) }
        let others = words.isEmpty ? [] : (Self.allSheets?() ?? []).filter { $0.id != sheet.id }
        for other in others {
            for section in other.sections where !section.groups.contains(where: { $0.items.contains(where: \.figure) }) {
                shown.append((nil, "\(other.title) · \(section.title)", section))
            }
        }
        // a formula typed with spaces (`P = V I`) is first looked for whole, in one of a row's texts; its words alone
        // match nearly every row (`=`, `v` and `i` are everywhere)
        let formulaLike = words.contains { $0.count == 1 || !$0.allSatisfy(\.isLetter) }
        let phrase = words.count > 1 && formulaLike ? Self.compact(words.joined()) : ""
        func kept(_ need: Int, whole: Bool = false) -> [[(IdentitySheet.Group, [IdentitySheet.Item])]] {
            var elsewhere = 0
            // a row listed once: a one-formula sheet (`ohm's law`) repeats its row from the big sheet
            var seen = Set<String>()
            return shown.map { entry in
                let section = entry.section
                let bare = section.groups.contains { $0.items.contains(where: \.figure) }
                return section.groups.compactMap { group in
                    if words.isEmpty || bare { return entry.index == nil ? nil : (group, group.items) }
                    if entry.index == nil, elsewhere >= Self.elsewhereMost { return nil }
                    let rows = group.items.filter { item in
                        guard !item.copy.isEmpty else { return true }
                        guard whole ? Self.hasPhrase(phrase, Self.formulaText(item))
                                    : Self.matchCount(words, [entry.title, group.title] + Self.searchText(item)) >= need,
                              entry.index != nil || !seen.contains(item.copy) else { return false }
                        seen.insert(item.copy)
                        return true
                    }
                    let count = rows.filter { !$0.copy.isEmpty }.count
                    if count == 0 { return nil }
                    if entry.index == nil { elsewhere += count }
                    return (group, rows)
                }
            }
        }
        // a figure's section is kept whatever the filter, so it isn't a match
        func anyMatch(_ found: [[(IdentitySheet.Group, [IdentitySheet.Item])]]) -> Bool {
            shown.indices.contains { !found[$0].isEmpty && !shown[$0].section.groups.contains { $0.items.contains(where: \.figure) } }
        }
        var found = kept(words.count)
        if phrase.count > 1 {
            let whole = kept(0, whole: true)
            if anyMatch(whole) { found = whole }
        }
        var closest = false
        if words.count > 1, !anyMatch(found) {
            let near = kept(words.count - 1)
            if anyMatch(near) { found = near; closest = true }
        }
        matched = anyMatch(found)
        // a quiet line over a closest-match search, and where the other sheets' matches start
        func say(_ text: String) {
            let note = label(text, size: 11, weight: .regular, color: .tertiaryLabelColor)
            note.frame = NSRect(x: inset, y: y + 2, width: width - inset * 2, height: 14)
            content.addSubview(note)
            y += 20
        }
        if closest { say("no row has every word · closest matches") }
        var saidElsewhere = false
        for (n, entry) in shown.enumerated() {
            let section = entry.section
            let bare = section.groups.contains { $0.items.contains(where: \.figure) }
            let groups = found[n]
            if groups.isEmpty { continue }
            if entry.index == nil, !saidElsewhere {
                saidElsewhere = true
                say("in other sheets")
            }
            if let index = entry.index { sectionTops[index] = y }
            let titleFrame = NSRect(x: inset, y: y + 4, width: width - inset * 2, height: 16)
            if !bare {
                let title = label(entry.title, size: 12, weight: .semibold, color: .labelColor)
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
                        if item.explain != nil { idleHint = Self.explainHint }
                    }
                    content.addSubview(row)
                    if firstPick == nil, !words.isEmpty, row.onPick != nil, !item.figure { firstPick = row }
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

    // what a filter is matched on: the row's words, its note and what it copies (symbols such as τ included), and its
    // explanation's title, related formulas and letters, each letter's unit with that unit's name (`ohm` finds rows in Ω, `newton` and
    // `force` rows in N)
    private static func searchText(_ item: IdentitySheet.Item) -> [String] {
        let runs = (item.lead + [item.runs]).map { $0.map(\.text).joined() }
        var texts = runs + [item.note, item.copy, item.open] + item.leadCopy + item.parts.map(\.copy)
        if let e = item.explain {
            texts.append(e.title)
            // its related formulas and what each is (`effective annual yield` finds compound interest)
            texts += e.related.map { $0.joined(separator: " ") }
            for v in e.vars {
                texts.append(v.dropFirst().joined(separator: " "))
                if v.count > 2, let name = unitNames[v[2].trimmingCharacters(in: .whitespaces)] { texts.append(name) }
            }
        }
        return texts
    }

    // the SI units the explanations give, by name and by what they measure, so a word finds the symbol
    private static let unitNames: [String: String] = [
        "Ω": "ohm ohms resistance resistor", "S": "siemens conductance", "V": "volt volts voltage potential emf",
        "A": "amp amps ampere current", "W": "watt watts power", "J": "joule joules energy work heat",
        "N": "newton newtons force", "N·m": "newton meter torque moment", "Pa": "pascal pressure stress",
        "C": "coulomb charge", "F": "farad capacitance capacitor", "H": "henry inductance inductor",
        "T": "tesla magnetic field flux density", "Wb": "weber magnetic flux", "Hz": "hertz frequency",
        "rad/s": "angular frequency velocity", "kg": "kilogram mass", "m": "meter metre length distance",
        "s": "second seconds time", "K": "kelvin temperature", "°C": "celsius temperature", "mol": "mole moles amount",
        "m/s": "velocity speed", "m/s^{2}": "acceleration", "kg·m/s": "momentum impulse", "m^{2}": "area",
        "m^{3}": "volume", "kg/m^{3}": "density", "W/(m·K)": "thermal conductivity", "N/m": "spring constant stiffness",
        "kg·m^{2}": "moment of inertia", "m^{4}": "second moment of area", "rad": "radian angle", "lm": "lumen", "cd": "candela",
        "V/m": "electric field", "A/m": "magnetic field strength", "Pa·s": "viscosity", "m^{3}/s": "flow rate",
    ]

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

    // how many of the filter's words are somewhere in the row, its group's or its section's title
    private static func matchCount(_ words: [String], _ texts: [String]) -> Int {
        let hay = fold(texts.joined(separator: " "))
        let squeezed = hay.split(whereSeparator: \.isWhitespace).map { compact(String($0)) }
        let tight = squeezed + squeezed.map(latin).filter { !squeezed.contains($0) }
        return words.filter { word in
            hay.contains(word) || { let w = compact(word); return !w.isEmpty && tight.contains { $0.contains(w) } }()
        }.count
    }

    // the row's own formula, as drawn and as copied (not its explanation's related ones)
    private static func formulaText(_ item: IdentitySheet.Item) -> [String] {
        (item.lead + [item.runs]).map { $0.map(\.text).joined() } + [item.copy] + item.leadCopy + item.parts.map(\.copy)
    }

    // the filter's letters and digits, together, inside one of a row's texts
    private static func hasPhrase(_ phrase: String, _ texts: [String]) -> Bool {
        texts.contains { let c = compact(fold($0)); return c.contains(phrase) || latin(c).contains(phrase) }
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
        // no zoom in/out: AppKit runs it as a threaded NSAnimation (_NSWindowTransformAnimation), and a thread was
        // left waiting forever about once per open and close (64 opens filled the user-interactive dispatch pool)
        panel.animationBehavior = .none
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

    /// ⌘[ and ⌘]: through the explanations a Related link went to while one is up, else this window's pages
    @discardableResult
    func navigate(_ step: Int) -> Bool {
        if let explainer, explainer.isShown { return stepExplanation(step) }
        return go(step)
    }

    // back to the explanation that linked here, or forward again to the one stepped back from
    private func stepExplanation(_ step: Int) -> Bool {
        if step < 0, explained.count > 1 {
            explainedAhead.append(explained.removeLast())
        } else if step > 0, let next = explainedAhead.popLast() {
            explained.append(next)
        } else {
            return false
        }
        fillExplainer()
        return true
    }

    /// ↵ in the filter (and the dev build's `sheetpick`): the first row the search leaves, as if clicked
    @discardableResult
    func pickFirst() -> Bool {
        guard let row = firstPick else { return false }
        row.onPick?(row)
        return true
    }

    /// ⌘F: the filter takes the keys, its text selected
    func focusFilter() {
        guard let panel, let filterField else { return }
        panel.makeFirstResponder(filterField)
        filterField.selectText(nil)
    }

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
        explainer?.close()
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
        if let explain = row.item.explain { showExplanation(explain, for: row) }
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

    // a row's explanation, in a popover beside it; another click replaces it
    private func showExplanation(_ explain: IdentitySheet.Explanation, for row: IdentityRow) {
        explainer?.close()
        explained = [(explain, row.item)]
        explainedAhead = []
        explainedRow = row
        let popover = NSPopover()
        popover.behavior = .transient
        popover.animates = false
        popover.appearance = panel?.appearance
        popover.contentViewController = NSViewController()
        explainer = popover
        fillExplainer()
        // beside the row, on whichever side has more room on the screen
        let onScreen = row.window?.convertToScreen(row.convert(row.bounds, to: nil)) ?? .zero
        let screen = (row.window?.screen ?? NSScreen.main)?.visibleFrame ?? .zero
        let edge: NSRectEdge = screen.maxX - onScreen.maxX >= onScreen.minX - screen.minX ? .maxX : .minX
        popover.show(relativeTo: row.bounds, of: row, preferredEdge: edge)
        (popover.contentViewController?.view as? ExplanationView).map { $0.window?.makeFirstResponder($0.textView) }
    }

    // the popover shows the last explanation gone to, with Back when a Related link brought it there
    private func fillExplainer() {
        guard let popover = explainer, let (explain, item) = explained.last else { return }
        let view = ExplanationView(explain, equation: item, onTry: explain.tryLine.isEmpty ? nil : { [weak self, weak popover] line in
            popover?.close()
            self?.onTry?(line)
        }, onRelated: { [weak self] i in
            self?.openRelated(explain.related[i])
        }, onBack: explained.count > 1 ? { [weak self] in
            _ = self?.stepExplanation(-1)
        } : nil)
        view.onClose = { [weak self] in self?.panel?.close() }
        view.onStep = { [weak self] in self?.stepExplanation($0) ?? false }
        // the popover had the keys (a Related link followed with ↵, Back): the new text has them, on its first link
        let hadKeys = popover.contentViewController?.view.window?.isKeyWindow == true
        popover.contentViewController?.view = view
        popover.contentSize = view.frame.size
        // the text is the popover's first responder, so its keys (Tab, ↵) are there whenever the popover has them
        view.window?.makeFirstResponder(view.textView)
        if hadKeys { view.takeKeys() }
    }

    // a Related formula: its own explanation, in the same popover, when some row has one; otherwise the sheets
    // searched for it, its note's words first (`effective annual yield`), closest matches if nothing has them all
    private func openRelated(_ related: [String]) {
        let formula = related.first ?? ""
        if let item = Self.explainedRow(for: formula, in: [sheet].compactMap { $0 } + (Self.allSheets?() ?? [])),
           let explain = item.explain {
            explained.append((explain, item))
            explainedAhead = []
            fillExplainer()
            return
        }
        explainer?.close()
        let note = related.count > 1 ? related[1] : ""
        setFilter(note.isEmpty ? formula : note)
    }

    // the row a formula is (SheetMatch): the best tier wins (an exact cell anywhere over a chain over a looser match),
    // the first such row on a tie
    private static func explainedRow(for formula: String, in sheets: [IdentitySheet]) -> IdentitySheet.Item? {
        guard let wanted = SheetMatch.Formula(formula) else { return nil }
        let rows = sheets.lazy.flatMap(\.sections).flatMap(\.groups).flatMap(\.items).filter { $0.explain != nil }
        var best: (tier: Int, item: IdentitySheet.Item)?
        for item in rows {
            let cells = (item.lead + [item.runs]).map { SheetMatch.markup($0.map { ($0.text, $0.shift) }) }
            guard let tier = wanted.tier(cells), tier < (best?.tier ?? .max) else { continue }
            best = (tier, item)
            if tier == 0 { break }
        }
        return best?.item
    }

    // ↵ in the filter picks the first row it leaves (copies and explains it, or opens its sheet)
    // Esc closes an explanation, then clears the filter, then closes the sheet
    func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
        guard control === filterField else { return false }
        if selector == #selector(NSResponder.insertNewline(_:)) {
            return pickFirst()
        }
        // Tab with an explanation up goes into it, onto its first Related link
        if selector == #selector(NSResponder.insertTab(_:)), let explainer, explainer.isShown,
           let view = explainer.contentViewController?.view as? ExplanationView {
            view.takeKeys()
            view.textView.stepLink(1)
            return true
        }
        guard selector == #selector(NSResponder.cancelOperation(_:)) else { return false }
        if let explainer, explainer.isShown {
            explainer.close()
        } else if filterField?.stringValue.isEmpty == false {
            filterField?.stringValue = ""
            refilter()
        } else {
            panel?.close()
        }
        return true
    }

    // a closed sheet takes its explanation with it
    func windowWillClose(_ notification: Notification) {
        explainer?.close()
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
