import AppKit
import Combine
import SwiftUI

/// The bar's one-line input: an NSTextView, because a TextField can't take every key first, and later needs
/// attributed spans and ghost text (MAC_UI_PLAN.md §2). No Writing Tools, no "Write with Siri", no inline
/// predictions, no autocorrection, no smart quotes: it's for maths.
final class BarTextView: NSTextView {
    var onKey: ((NSEvent) -> Bool)?
    var onEscape: (() -> Void)?
    /// a paste with line breaks or `;`: true when it was taken (equations opened as a system)
    var onPasteLines: ((String) -> Bool)?
    /// a pasted sentence or paragraph, flattened to one plain line (PlainText)
    var onPastedProse: ((String) -> Void)?
    /// files dropped on the line (native only, ShareCommands.swift): true when they were taken, else the path is
    /// typed as text would be
    var onDropFiles: (([URL]) -> Bool)?
    var placeholder = "" { didSet { needsDisplay = true } }
    /// a faint `ans` ahead of the text while the line chains from the last answer
    var ghostLead = "" { didSet { if ghostLead != oldValue { updateInset() } } }
    /// the completion ghost after the caret (completionFor)
    var ghostTail = "" { didSet { if ghostTail != oldValue { needsDisplay = true } } }
    /// the InputEvent.inputType of the edit in progress (undo.ts editKind)
    var editKind: String?
    var trailingParens = 0 { didSet { if trailingParens != oldValue { needsDisplay = true } } }
    var squiggle: NSRange? { didSet { if squiggle != oldValue { needsDisplay = true } } }
    /// .quick-offered: a unit offered after a number (missingUnit), drawn muted in a gap opened after it
    var offer: (at: Int, text: String)? {
        didSet {
            guard offer?.at != oldValue?.at || offer?.text != oldValue?.text else { return }
            applyGaps()
        }
    }

    /// native only: a faint `°` or ` rad` after each trig argument (core `angleMarks`), saying which angle mode it's
    /// read in. One inside the line opens a gap after the argument as the offer does; one at the end is drawn after
    /// the text (as the grey `)` is), so the caret never moves for it.
    private(set) var angleMarks: [Int] = []
    private(set) var angleMarkText = "°"
    private var angleSpans: [AngleSpan] = []
    private var gapsFor = ""

    /// the marks to draw: each argument's, unless the caret is in it (being typed, its mark would sit where the
    /// caret does; it shows once the caret leaves)
    func setAngleMarks(_ spans: [AngleSpan], text mark: String) {
        angleSpans = spans
        let caret = selectedRange().location
        let marks = spans.filter { caret < $0.start || caret > $0.at }.map(\.at)
        guard marks != angleMarks || mark != angleMarkText || gapsFor != string else { return }
        angleMarks = marks
        angleMarkText = mark
        applyGaps()
    }

    /// the caret moved: a mark hides or shows as it enters or leaves its argument
    func refreshAngleMarks() {
        // a line just typed waits for its own spans (the next update), never the older line's
        guard gapsFor == string else { return }
        setAngleMarks(angleSpans, text: angleMarkText)
    }

    /// the offer's and the angle marks' gaps, on a freshly styled line
    private func applyGaps() {
        restyle()
        gapsFor = string
        guard let storage = textStorage else { return needsDisplay = true }
        let length = (string as NSString).length
        var gaps: [(Int, String)] = angleMarks.filter { $0 > 0 && $0 < length }.map { ($0, angleMarkText) }
        if let offer, offer.at > 0, offer.at <= length { gaps.append((offer.at, offer.text)) }
        for (at, text) in gaps {
            let width = (text as NSString).size(withAttributes: Self.attributes).width
            storage.addAttribute(.kern, value: Theme.displayTracking(Self.fontSize) + width, range: NSRange(location: at - 1, length: 1))
        }
        needsDisplay = true
    }
    private var coloured: (units: [NSRange], words: [NSRange], style: String) = ([], [], "")

    // ---- stacked fractions and powers (StackedMath.swift, StackedLayout.swift)
    var stacked = StackedMath.Parsed()
    var stackBars: [StackBar] = []
    var stackHoles: [StackHole] = []
    var slotLooks: [SlotLook] = []
    var stackCaret: NSRect?
    var stackCaretShown = true
    var stackBlink: Timer?
    var caretInk: NSColor = Theme.ink
    private var stackWatch: AnyCancellable?
    /// where a ⇧← ⇧→ selection started
    private var stackAnchor: Int?
    /// a caret place in the room around a spaced `+ = ;` moves this far back, to the room's middle (spaceOperators)
    var caretShift: [Int: CGFloat] = [:]

    override var insertionPointColor: NSColor? {
        get { super.insertionPointColor }
        set { caretInk = newValue ?? Theme.ink; super.insertionPointColor = stackCaret == nil ? newValue : .clear }
    }

    override func setSelectedRanges(_ ranges: [NSValue], affinity: NSSelectionAffinity, stillSelecting: Bool) {
        super.setSelectedRanges(ranges, affinity: affinity, stillSelecting: stillSelecting)
        if stackCaret != nil || !stacked.stacks.isEmpty || !limits.isEmpty { refreshStackCaret() }
        // an empty limit's box darkens while the caret is in it
        if !holes.isEmpty { needsDisplay = true }
    }

    override func mouseDown(with event: NSEvent) {
        guard Self.stackedOn, !stacked.stacks.isEmpty, event.clickCount == 1,
              event.modifierFlags.intersection([.shift, .command, .option, .control]).isEmpty,
              let at = stackStop(near: convert(event.locationInWindow, from: nil)) else { return super.mouseDown(with: event) }
        window?.makeFirstResponder(self)
        setSelectedRange(NSRange(location: at, length: 0))
        // a drag from there selects to the place nearest the pointer (grown to whole stacks by willChangeSelection)
        while let next = window?.nextEvent(matching: [.leftMouseDragged, .leftMouseUp]) {
            if let to = stackStop(near: convert(next.locationInWindow, from: nil)) {
                setSelectedRange(NSRange(location: min(at, to), length: abs(to - at)))
            }
            if next.type == .leftMouseUp { break }
        }
    }

    // ⇧← ⇧→ one place too, the end that moves being the one away from where the selection started
    override func moveLeftAndModifySelection(_ sender: Any?) { if !stackExtend(-1) { super.moveLeftAndModifySelection(sender) } }
    override func moveRightAndModifySelection(_ sender: Any?) { if !stackExtend(1) { super.moveRightAndModifySelection(sender) } }
    override func moveBackwardAndModifySelection(_ sender: Any?) { if !stackExtend(-1) { super.moveBackwardAndModifySelection(sender) } }
    override func moveForwardAndModifySelection(_ sender: Any?) { if !stackExtend(1) { super.moveForwardAndModifySelection(sender) } }

    private func stackExtend(_ by: Int) -> Bool {
        guard Self.stackedOn, !stacked.stacks.isEmpty else { return false }
        let sel = selectedRange(), n = (string as NSString).length
        let anchor = sel.length == 0 ? sel.location : (stackAnchor == NSMaxRange(sel) ? NSMaxRange(sel) : sel.location)
        let moving = anchor == sel.location ? NSMaxRange(sel) : sel.location
        let to = StackedMath.snap(stacked, min(n, max(0, moving + by)), from: moving, length: n)
        stackAnchor = anchor
        setSelectedRange(NSRange(location: min(anchor, to), length: abs(to - anchor)))
        return true
    }

    // ← → one place along the text (TextKit goes by the glyphs, which overlap in a stacked fraction), never onto a
    // hidden one
    override func moveLeft(_ sender: Any?) { if !stackStep(-1) { super.moveLeft(sender) } }
    override func moveRight(_ sender: Any?) { if !stackStep(1) { super.moveRight(sender) } }
    override func moveBackward(_ sender: Any?) { if !stackStep(-1) { super.moveBackward(sender) } }
    override func moveForward(_ sender: Any?) { if !stackStep(1) { super.moveForward(sender) } }

    private func stackStep(_ by: Int) -> Bool {
        guard Self.stackedOn, !stacked.stacks.isEmpty else { return false }
        let sel = selectedRange(), n = (string as NSString).length
        let to = sel.length > 0 ? (by < 0 ? sel.location : NSMaxRange(sel)) : min(n, max(0, sel.location + by))
        setSelectedRange(NSRange(location: StackedMath.snap(stacked, to, from: sel.location, length: n), length: 0))
        return true
    }

    /// the line again in its look, after the setting changes
    func restyleWithGaps() { applyGaps() }

    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        let urls = sender.draggingPasteboard.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL] ?? []
        if !urls.isEmpty, onDropFiles?(urls) == true { return true }
        return super.performDragOperation(sender)
    }

    /// files over the line: the hint says where they'll go (native only, ShareCommands.swift)
    var onDragFiles: ((Bool, Int) -> Void)?

    private func fileCount(_ sender: NSDraggingInfo) -> Int {
        (sender.draggingPasteboard.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL] ?? []).count
    }

    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        let n = fileCount(sender)
        if n > 0 { onDragFiles?(true, n) }
        return super.draggingEntered(sender)
    }

    override func draggingExited(_ sender: NSDraggingInfo?) {
        onDragFiles?(false, 0)
        super.draggingExited(sender)
    }

    override func concludeDragOperation(_ sender: NSDraggingInfo?) {
        onDragFiles?(false, 0)
        super.concludeDragOperation(sender)
    }

    /// .quick-unit in the accent, .quick-word in accent mixed 85% with ink: temporary attributes, so typing and
    /// undo never see them
    func colour(units: [NSRange], words: [NSRange], style: String) {
        guard units != coloured.units || words != coloured.words || style != coloured.style, let lm = layoutManager else { return }
        coloured = (units, words, style)
        let all = NSRange(location: 0, length: (string as NSString).length)
        lm.removeTemporaryAttribute(.foregroundColor, forCharacterRange: all)
        let word = Theme.wordColour(style), unit = Theme.unitColour(style)
        for r in words where NSMaxRange(r) <= all.length { lm.addTemporaryAttribute(.foregroundColor, value: word, forCharacterRange: r) }
        for r in units where NSMaxRange(r) <= all.length { lm.addTemporaryAttribute(.foregroundColor, value: unit, forCharacterRange: r) }
    }

    // ---- .quick-unit-tip: the full name of the unit, function or constant under the pointer, in a small ink label
    //      over the word. The first waits for a short rest (350 ms); once one has shown, the next word's shows at
    //      once, and an edit under the pointer renames it straight away. It goes when the pointer leaves the line.

    /// each named word in the line (wordNames, from the core)
    var wordNames: [WordName] = [] {
        didSet {
            guard wordNames != oldValue else { return }
            if pointer != nil, tipEngaged { hoverName(now: true) } else if pointer == nil { hideTip() }
        }
    }
    private var pointer: NSPoint?
    private var tipEngaged = false
    private var tipTimer: Timer?
    private var tipShown: (name: String, range: NSRange)?
    private lazy var tip = WordTipWindow()
    private var nameTracking: NSTrackingArea?

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let nameTracking { removeTrackingArea(nameTracking) }
        let area = NSTrackingArea(rect: .zero, options: [.mouseMoved, .mouseEnteredAndExited, .activeAlways, .inVisibleRect], owner: self, userInfo: nil)
        addTrackingArea(area)
        nameTracking = area
    }

    override func mouseMoved(with event: NSEvent) {
        super.mouseMoved(with: event)
        pointer = convert(event.locationInWindow, from: nil)
        if !wordNames.isEmpty || tipShown != nil { hoverName(now: false) }
    }

    override func mouseExited(with event: NSEvent) {
        super.mouseExited(with: event)
        pointer = nil
        tipEngaged = false
        hideTip()
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if window == nil { pointer = nil; tipEngaged = false; hideTip() }
    }

    /// where a word's glyphs are, in the view
    private func wordRect(_ range: NSRange) -> NSRect? {
        guard let lm = layoutManager, let tc = textContainer, NSMaxRange(range) <= (string as NSString).length else { return nil }
        let glyphs = lm.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
        return lm.boundingRect(forGlyphRange: glyphs, in: tc).offsetBy(dx: textContainerInset.width, dy: textContainerInset.height)
    }

    private func hoverName(now: Bool) {
        tipTimer?.invalidate()
        tipTimer = nil
        guard let pointer, let hit = wordNames.first(where: { w in
            guard let r = wordRect(w.range) else { return false }
            return pointer.x >= r.minX && pointer.x <= r.maxX
        }), let rect = wordRect(hit.range) else {
            hideTip()
            return
        }
        if tipShown?.name == hit.name, tipShown?.range == hit.range { return }
        let show = { [weak self] in
            guard let self, let window = self.window, window.isVisible else { return }
            self.tipEngaged = true
            self.tipShown = (hit.name, hit.range)
            // the label's bottom 2 pt into the top of the line's box, its left at the word's (bottom: calc(100% - 2px))
            let onWindow = self.convert(NSRect(x: rect.minX, y: Self.stackRoom, width: rect.width, height: self.bounds.height - 2 * Self.stackRoom), to: nil)
            let onScreen = window.convertToScreen(onWindow)
            self.tip.show(hit.name, at: NSPoint(x: onScreen.minX, y: onScreen.maxY - 2), over: window)
            HoverTip.onScreen()
        }
        if now || tipShown != nil || tipEngaged || HoverTip.wait == 0 {
            show()
        } else {
            tipTimer = Timer.scheduledTimer(withTimeInterval: HoverTip.wait, repeats: false) { _ in show() }
        }
    }

    private func hideTip() {
        tipTimer?.invalidate()
        tipTimer = nil
        guard tipShown != nil else { return }
        tipShown = nil
        tip.hide()
        HoverTip.onScreen()
    }

    static let fontSize: CGFloat = 24
    /// the view reaches this far above and below the bar's 36 pt row (the line stays in the middle), so a stacked
    /// fraction's slots have room past the row without being cut off (InputField's negative padding, LineScroll)
    static let stackRoom: CGFloat = 18
    /// the line's own font (`font` is the first character's, which may be a stacked fraction's hidden bracket)
    static let lineFont = NSFont.systemFont(ofSize: fontSize, weight: .regular)
    static let attributes: [NSAttributedString.Key: Any] = [
        .font: NSFont.systemFont(ofSize: fontSize, weight: .regular),
        .foregroundColor: Theme.ink,
        .kern: Theme.displayTracking(fontSize),
    ]

    /// What each √ or ∛ takes (UTF-16 ranges): a bracketed group, or a run of digits, letters and π.
    static func radicandRanges(_ s: String) -> [NSRange] {
        let u = Array((s as NSString).description.utf16)
        var out: [NSRange] = []
        let root: Set<UInt16> = [0x221A, 0x221B]
        var i = 0
        while i < u.count {
            defer { i += 1 }
            guard root.contains(u[i]), i + 1 < u.count else { continue }
            var j = i + 1
            if u[j] == 0x28 {
                var depth = 0
                while j < u.count {
                    if u[j] == 0x28 { depth += 1 }
                    if u[j] == 0x29 { depth -= 1; if depth == 0 { j += 1; break } }
                    j += 1
                }
            } else {
                while j < u.count, let c = UnicodeScalar(u[j]), CharacterSet.alphanumerics.contains(c) || u[j] == 0x2E { j += 1 }
            }
            if j > i + 1 { out.append(NSRange(location: i + 1, length: j - i - 1)) }
        }
        return out
    }

    /// The whole line in the field's look, after any programmatic change.
    func restyle() {
        guard let storage = textStorage, storage.length > 0 else { stacked = StackedMath.Parsed(); stackBars = []; stackHoles = []; return refreshStackCaret() }
        storage.setAttributes(Self.attributes, range: NSRange(location: 0, length: storage.length))
        styleBounds()
        styleStacks()
        hideRootSigns()
    }

    /// A root with something under it is drawn whole, sign and bar in one line (native only); its glyph keeps the place.
    /// A root with nothing under it yet shows its glyph.
    func hideRootSigns() {
        guard let storage = textStorage, storage.length > 0 else { return }
        let ns = string as NSString
        let drawn = Set(Self.radicandRanges(string).map { $0.location - 1 })
        for i in 0..<ns.length where ns.character(at: i) == 0x221A || ns.character(at: i) == 0x221B {
            storage.addAttribute(.foregroundColor, value: drawn.contains(i) ? NSColor.clear : Theme.ink, range: NSRange(location: i, length: 1))
        }
        // what's typed next never takes the hidden sign's colour
        typingAttributes = Self.attributes
        needsDisplay = true
    }

    // ---- ∫ Σ Π limits (Bounds.tsx, lib/bounds.ts): drawn small beside the sign, upper over lower; the `_ ^ ..`
    //      and braces between them hidden; the caret never stops in those

    /// Whether `s` has a ∫ Σ Π sign (BOUND_SIGNS): without one, boundsIn is empty and afterTyping does nothing, so
    /// the engine isn't asked.
    static func hasBoundSign(_ s: String) -> Bool {
        s.unicodeScalars.contains { $0 == "∫" || $0 == "Σ" || $0 == "∑" || $0 == "Π" || $0 == "∏" }
    }

    /// boundsIn(text): {sign, lower {start, end}, upper {start, end} | null, end}
    var limits: [[String: Any]] = [] { didSet { styleBounds() } }
    private var holes: [(slot: NSRange, upper: Bool, sign: Int)] = []

    private static func slot(_ any: Any?) -> NSRange? {
        guard let s = any as? [String: Any], let a = (s["start"] as? NSNumber)?.intValue, let b = (s["end"] as? NSNumber)?.intValue else { return nil }
        return NSRange(location: a, length: max(0, b - a))
    }

    private func styleBounds() {
        holes = []
        guard let storage = textStorage, !limits.isEmpty else { if stackCaret != nil { refreshStackCaret() }; return }
        let length = storage.length
        let small = NSFont.systemFont(ofSize: Self.fontSize * 0.56)
        let limit: [NSAttributedString.Key: Any] = [.font: small, .foregroundColor: Theme.ink]
        let hidden: [NSAttributedString.Key: Any] = [.font: NSFont.systemFont(ofSize: 0.01), .foregroundColor: NSColor.clear, .kern: 0]
        func width(_ r: NSRange) -> CGFloat { r.length == 0 ? Self.fontSize * 0.62 * 0.56 : storage.attributedSubstring(from: r).size().width }
        for b in limits {
            guard let sign = (b["sign"] as? NSNumber)?.intValue, let end = (b["end"] as? NSNumber)?.intValue,
                  let lower = Self.slot(b["lower"]), end <= length, sign < end else { continue }
            let upper = Self.slot(b["upper"])
            for c in (sign + 1)..<end where !NSLocationInRange(c, lower) && !(upper.map { NSLocationInRange(c, $0) } ?? false) {
                storage.setAttributes(hidden, range: NSRange(location: c, length: 1))
            }
            var low = limit; low[.baselineOffset] = -Self.fontSize * 0.2
            var up = limit; up[.baselineOffset] = Self.fontSize * 0.36
            if lower.length > 0 { storage.setAttributes(low, range: lower) }
            if let upper, upper.length > 0 { storage.setAttributes(up, range: upper) }
            // the upper limit sits over the lower: the lower's last character pulls the pen back under it
            let lw = width(lower), uw = upper.map(width) ?? Self.fontSize * 0.62 * 0.56
            let upperTyped = (upper?.length ?? 0) > 0
            if lower.length > 0 {
                // with no upper text yet the lower keeps its width (the upper's hole is drawn over it)
                storage.addAttribute(.kern, value: upperTyped ? -lw : 2, range: NSRange(location: NSMaxRange(lower) - 1, length: 1))
            }
            if let upper, upper.length > 0 { storage.addAttribute(.kern, value: max(0, lw - uw) + 2, range: NSRange(location: NSMaxRange(upper) - 1, length: 1)) }
            if lower.length == 0 { holes.append((lower, false, sign)) }
            if upper == nil || upper?.length == 0 { holes.append((upper ?? NSRange(location: end, length: 0), true, sign)) }
        }
        refreshStackCaret()
    }

    /// the limit a caret at `c` types into (slotOf: the lower first, both ends counted)
    private func limitSlot(at c: Int) -> (slot: NSRange, upper: Bool, sign: Int)? {
        for b in limits {
            guard let sign = (b["sign"] as? NSNumber)?.intValue, let lower = Self.slot(b["lower"]) else { continue }
            if c >= lower.location, c <= NSMaxRange(lower) { return (lower, false, sign) }
            if let upper = Self.slot(b["upper"]), c >= upper.location, c <= NSMaxRange(upper) { return (upper, true, sign) }
        }
        return nil
    }

    /// .bounds-hole: an empty limit's box, 0.62 × 0.8 em at the limits' size, beside the sign
    private func holeBox(sign: Int, upper: Bool) -> NSRect? {
        guard let lm = layoutManager, let tc = textContainer, sign < (string as NSString).length else { return nil }
        let em = Self.fontSize * 0.56
        let g = lm.glyphIndexForCharacter(at: sign)
        let r = lm.boundingRect(forGlyphRange: NSRange(location: g, length: 1), in: tc)
            .offsetBy(dx: textContainerInset.width, dy: textContainerInset.height)
        let y = upper ? r.minY + 2 : r.maxY - em * 0.8 - 1
        return NSRect(x: r.maxX + (upper ? em * 0.3 : em * 0.05), y: y, width: em * 0.62, height: em * 0.8)
    }

    /// native only (Wolfram's way): the caret in a ∫ Σ Π limit is the limit's size and stands at its height, in the
    /// middle of its box while it's empty, so it's plain which limit the keys go to
    func limitCaretRect(_ c: Int) -> NSRect? {
        guard !limits.isEmpty, let at = limitSlot(at: c), let lm = layoutManager, let tc = textContainer,
              at.sign < (string as NSString).length else { return nil }
        let em = Self.fontSize * 0.56
        if at.slot.length == 0 {
            guard let box = holeBox(sign: at.sign, upper: at.upper) else { return nil }
            return NSRect(x: box.midX - 0.6, y: box.minY - em * 0.12, width: 1.2, height: box.height + em * 0.24)
        }
        lm.ensureLayout(for: tc)
        let g = lm.glyphIndexForCharacter(at: at.slot.location)
        let frag = lm.lineFragmentRect(forGlyphAt: g, effectiveRange: nil)
        let loc = lm.location(forGlyphAt: g)
        // the sign stands on the line's baseline (a limit's own glyph has its raise or drop in it already); the
        // limit's baseline is above it (upper) or below it
        let signGlyph = lm.glyphIndexForCharacter(at: at.sign)
        let line = textContainerInset.height + lm.lineFragmentRect(forGlyphAt: signGlyph, effectiveRange: nil).minY + lm.location(forGlyphAt: signGlyph).y
        let base = line + (at.upper ? -Self.fontSize * 0.36 : Self.fontSize * 0.2)
        let typed = (string as NSString).substring(with: NSRange(location: at.slot.location, length: c - at.slot.location))
        let x = textContainerInset.width + frag.minX + loc.x + (typed as NSString).size(withAttributes: [.font: NSFont.systemFont(ofSize: em)]).width
        return NSRect(x: x - 0.6, y: base - em * 0.92, width: 1.2, height: em * 1.15)
    }

    /// .bounds-hole: an empty limit's muted box, a shade darker while the caret is in it
    private func drawHoles() {
        guard !holes.isEmpty else { return }
        let em = Self.fontSize * 0.56
        let sel = selectedRange()
        let here = sel.length == 0 && window?.firstResponder === self ? limitSlot(at: sel.location) : nil
        for h in holes {
            guard let box = holeBox(sign: h.sign, upper: h.upper) else { continue }
            let active = here.map { $0.slot.length == 0 && $0.sign == h.sign && $0.upper == h.upper } ?? false
            Theme.muted.withAlphaComponent(active ? 0.32 : 0.18).setFill()
            NSBezierPath(roundedRect: box, xRadius: em * 0.14, yRadius: em * 0.14).fill()
        }
    }

    /// Replaces the line, putting the caret (and selection) where asked; one undo step.
    func replace(with text: String, selection: NSRange) {
        let all = NSRange(location: 0, length: (string as NSString).length)
        if shouldChangeText(in: all, replacementString: text) {
            textStorage?.replaceCharacters(in: all, with: NSAttributedString(string: text, attributes: Self.attributes))
            didChangeText()
        }
        setSelectedRange(selection)
    }

    // `[` pairs, `]` steps over, `(`/`[`/`{` over a selection wraps it (bracketKey, wrapSelection)
    override func insertText(_ insert: Any, replacementRange: NSRange) {
        let typed = (insert as? String) ?? (insert as? NSAttributedString)?.string ?? ""
        let sel = selectedRange()
        if Self.stackedOn, !hasMarkedText(), replacementRange.location == NSNotFound, stackedTyping(typed, sel) { return }
        if !hasMarkedText(), replacementRange.location == NSNotFound, ["[", "]", "(", "{"].contains(typed) {
            if let w = Symbols.wrap(string, start: sel.location, end: sel.location + sel.length, key: typed) {
                replace(with: w.text, selection: NSRange(location: w.start, length: w.end - w.start))
                return
            }
            if let b = Symbols.bracketKey(string, start: sel.location, end: sel.location + sel.length, key: typed) {
                replace(with: b.text, selection: NSRange(location: b.caret, length: 0))
                return
            }
        }
        editKind = "insertText"
        super.insertText(insert, replacementRange: replacementRange)
    }

    /// `/`, `^`, `)` and a term after a stack, when the line stacks them: true when the key was taken
    private func stackedTyping(_ typed: String, _ sel: NSRange) -> Bool {
        var edit: StackedMath.Edit?
        switch typed {
        case "/", "÷": edit = StackedMath.slashKey(string, start: sel.location, end: NSMaxRange(sel))
        case "^" where sel.length == 0: edit = StackedMath.powerKey(string, caret: sel.location)
        case ")" where sel.length == 0:
            if let to = StackedMath.closeKey(string, caret: sel.location) {
                setSelectedRange(NSRange(location: StackedMath.snap(stacked, to, from: sel.location, length: (string as NSString).length), length: 0))
                return true
            }
        default:
            if sel.length == 0, (typed as NSString).length == 1 { edit = StackedMath.termAfterStack(string, caret: sel.location, typed: typed) }
        }
        guard let edit else { return false }
        editKind = "insertText"
        replace(with: edit.text, selection: NSRange(location: edit.caret, length: 0))
        return true
    }

    // backspace in `[]` takes both; a +- that meets at the caret becomes ± (joinPlusMinusAt)
    override func deleteForward(_ sender: Any?) {
        editKind = "deleteContentForward"
        // ⌦ before a stack's hidden `/ ^ ( ) ·` steps over it, never takes it (the stack would come apart)
        let sel = selectedRange()
        if Self.stackedOn, sel.length == 0, stacked.hidden.contains(sel.location) {
            let n = (string as NSString).length
            var to = sel.location
            while to < n, stacked.hidden.contains(to) { to += 1 }
            editKind = nil
            setSelectedRange(NSRange(location: StackedMath.snap(stacked, to, from: sel.location, length: n), length: 0))
            return
        }
        if keepSlot(sel.length > 0 || sel.location >= (string as NSString).length ? sel
                    : (string as NSString).rangeOfComposedCharacterSequence(at: sel.location)) { return }
        super.deleteForward(sender)
    }

    // ⌥⌫ ⌘⌫ ⌥⌦ ⌘⌦ inside a slot stop at its edge, and outside one take whole stacks
    override func deleteWordBackward(_ sender: Any?) { if !stackDelete(to: wordEdge(forward: false)) { super.deleteWordBackward(sender) } }
    override func deleteWordForward(_ sender: Any?) { if !stackDelete(to: wordEdge(forward: true)) { super.deleteWordForward(sender) } }
    override func deleteToBeginningOfLine(_ sender: Any?) { if !stackDelete(to: 0) { super.deleteToBeginningOfLine(sender) } }
    override func deleteToEndOfLine(_ sender: Any?) { if !stackDelete(to: (string as NSString).length) { super.deleteToEndOfLine(sender) } }

    private func wordEdge(forward: Bool) -> Int {
        let c = selectedRange().location
        guard let storage = textStorage, storage.length > 0 else { return c }
        return storage.nextWord(from: c, forward: forward)
    }

    private func stackDelete(to: Int) -> Bool {
        let sel = selectedRange(), n = (string as NSString).length
        guard Self.stackedOn, !stacked.stacks.isEmpty, sel.length == 0 else { return false }
        let ns = string as NSString
        // does a term start at i (a digit, a letter, a bracket, a root)?
        let termAt: (Int) -> Bool = { i in
            guard i < ns.length else { return false }
            let c = ns.character(at: i)
            return StackedMath.isDigit(c) || StackedMath.isLetter(c) || c == 0x28 || c == 0x221A || c == 0x221B
        }
        guard let range = StackedMath.deletion(stacked, caret: sel.location, to: to, length: n, next: termAt) else {
            // at a slot's edge: backspace takes the stack apart, ⌦ steps on
            if to < sel.location { deleteBackward(nil) } else { deleteForward(nil) }
            return true
        }
        editKind = to < sel.location ? "deleteWordBackward" : "deleteWordForward"
        if keepSlot(range) { return true }
        replace(with: ns.replacingCharacters(in: range, with: ""), selection: NSRange(location: range.location, length: 0))
        return true
    }

    /// deleting all of a slot with no hidden brackets leaves an empty `()` in its place (StackedMath.emptying),
    /// the caret in it, so the stack keeps its shape
    private func keepSlot(_ r: NSRange) -> Bool {
        guard Self.stackedOn, !stacked.stacks.isEmpty, let lo = StackedMath.emptying(stacked, r) else { return false }
        if editKind == nil { editKind = "deleteContentBackward" }
        replace(with: (string as NSString).replacingCharacters(in: r, with: "()"), selection: NSRange(location: lo + 1, length: 0))
        return true
    }

    override func deleteBackward(_ sender: Any?) {
        editKind = "deleteContentBackward"
        let sel = selectedRange()
        if Self.stackedOn, sel.length == 0, let e = StackedMath.backspace(string, caret: sel.location) {
            if e.text == string { editKind = nil; setSelectedRange(NSRange(location: e.caret, length: 0)) } else { replace(with: e.text, selection: NSRange(location: e.caret, length: 0)) }
            return
        }
        if keepSlot(sel.length > 0 || sel.location == 0 ? sel
                    : (string as NSString).rangeOfComposedCharacterSequence(at: sel.location - 1)) { return }
        if let b = Symbols.bracketKey(string, start: sel.location, end: sel.location + sel.length, key: "backspace") {
            replace(with: b.text, selection: NSRange(location: b.caret, length: 0))
            return
        }
        super.deleteBackward(sender)
        let ns = string as NSString
        let c = selectedRange().location
        if c > 0, c < ns.length, ns.substring(with: NSRange(location: c - 1, length: 2)) == "+-" {
            replace(with: ns.substring(to: c - 1) + "±" + ns.substring(from: c + 1), selection: NSRange(location: c, length: 0))
        }
    }

    override init(frame: NSRect, textContainer: NSTextContainer?) {
        super.init(frame: frame, textContainer: textContainer)
        configure()
    }

    convenience init() {
        let storage = NSTextStorage()
        let layout = NSLayoutManager()
        layout.typesetter = StackTypesetter()
        storage.addLayoutManager(layout)
        // tall enough for a stacked line's fragment: TextKit cuts a line down to its container and draws nothing
        // past it (a denominator was cut off at 40 pt)
        let container = NSTextContainer(size: NSSize(width: CGFloat.greatestFiniteMagnitude, height: 400))
        container.widthTracksTextView = false
        container.lineFragmentPadding = 0
        layout.addTextContainer(container)
        self.init(frame: .zero, textContainer: container)
    }

    required init?(coder: NSCoder) { nil }

    private func configure() {
        isRichText = false
        importsGraphics = false
        allowsUndo = false // undo.ts runs the undo (BarModel.undoText), as the web input does
        usesFindBar = false
        isAutomaticQuoteSubstitutionEnabled = false
        isAutomaticDashSubstitutionEnabled = false
        isAutomaticTextReplacementEnabled = false
        isAutomaticSpellingCorrectionEnabled = false
        isContinuousSpellCheckingEnabled = false
        isGrammarCheckingEnabled = false
        isAutomaticDataDetectionEnabled = false
        isAutomaticLinkDetectionEnabled = false
        isAutomaticTextCompletionEnabled = false
        smartInsertDeleteEnabled = false
        if #available(macOS 14.0, *) { inlinePredictionType = .no }
        if #available(macOS 15.0, *) { writingToolsBehavior = .none }
        isHorizontallyResizable = true
        isVerticallyResizable = false
        maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: 40)
        drawsBackground = false
        // .quick-field: 24 pt regular, -0.021 em, the ink colour, and an ink caret (caret-color: var(--ink))
        font = .systemFont(ofSize: Self.fontSize, weight: .regular)
        textColor = Theme.ink
        insertionPointColor = Theme.ink
        typingAttributes = Self.attributes
        selectedTextAttributes = [.backgroundColor: NSColor.selectedTextBackgroundColor]
        setAccessibilityLabel("Calculation")
        setAccessibilityHelp("Type maths; the answer appears as you type. Return saves it to the history.")
        setAccessibilityRole(.textField)
        stackWatch = AppSettings.shared.$stackedMath.dropFirst().receive(on: RunLoop.main).sink { [weak self] _ in self?.restyleWithGaps() }
    }

    /// the grey leading parens, a view of their own: drawn by the text view itself they sat in the inset, which
    /// the text view doesn't repaint, so a new one never showed and an old one stayed after the line was cleared
    private let leadView = GhostLeadView()

    func updateInset() {
        // as wide as typed `(` with the line's tracking, so filling one in moves nothing
        let lead = ghostLead.isEmpty ? 0 : (ghostLead as NSString).size(withAttributes: Self.attributes).width
        // the line sits in the middle of its 36 pt box, as the web input's does
        let line = layoutManager?.defaultLineHeight(for: Self.lineFont) ?? Self.fontSize * 1.2
        var top = max(0, (bounds.height - line) / 2)
        // a stacked line's fragment is taller than a plain one's (TextKit makes room for a raised numerator), its
        // baseline lower: the line goes up by that much, so its baseline stays where a plain line's is
        if !stacked.stacks.isEmpty, let lm = layoutManager, let tc = textContainer, (string as NSString).length > 0 {
            lm.ensureLayout(for: tc)
            // measured at a stack's hidden `/` or `^`, which sits on the line's own baseline (a raised glyph doesn't)
            let g = lm.glyphIndexForCharacter(at: min(stacked.stacks[0].op, (string as NSString).length - 1))
            let actual = lm.lineFragmentRect(forGlyphAt: g, effectiveRange: nil).minY + lm.location(forGlyphAt: g).y
            let plain = lm.defaultBaselineOffset(for: Self.lineFont)
            top -= max(0, actual - plain)
            // a line taller than its room (a fraction in a denominator) moves up or down so all of it shows: its
            // baseline stays a plain line's until the stacks would run off the bottom or the top
            let e = Self.extent(stacked, NSRange(location: 0, length: (string as NSString).length), size: Self.fontSize)
            let baseline = top + actual
            let lowest = e.height + 2, highest = bounds.height - e.depth - 2
            let to = lowest > highest ? (lowest + highest) / 2 : min(max(baseline, lowest), highest)
            top += to - baseline
        }
        textContainerInset = NSSize(width: lead, height: top)
        if leadView.superview == nil { addSubview(leadView) }
        leadView.text = ghostLead
        leadView.attributes = [.font: Self.lineFont, .foregroundColor: Theme.muted, .kern: Theme.displayTracking(Self.fontSize)]
        leadView.y = textContainerInset.height
        leadView.frame = NSRect(x: 0, y: 0, width: lead, height: bounds.height)
        leadView.isHidden = ghostLead.isEmpty
        leadView.needsDisplay = true
        needsDisplay = true
        // the caret moves with the text when the room for grey ( opens or closes (it had stayed a character off)
        if window != nil { updateInsertionPointStateAndRestartTimer(true) }
    }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        updateInset()
    }

    override var acceptsFirstResponder: Bool { true }
    override var needsPanelToBecomeKey: Bool { true }

    // the bar's keys first (↵, ↑↓, tab, the rebindable chords); everything else is typing
    /// boundKey / wordToSign for a key at the caret: the edit to make, or nil to leave the key alone
    var boundKey: ((String, String, Int) -> (text: String, caret: Int)?)?

    override func keyDown(with event: NSEvent) {
        if hasMarkedText() { return super.keyDown(with: event) }
        let flags = event.modifierFlags.intersection([.command, .option, .control, .shift])
        let sel = selectedRange()
        if stackArrow(event) { return }
        if sel.length == 0, flags.isSubset(of: [.shift]), let boundKey {
            let chars = event.charactersIgnoringModifiers ?? ""
            let name: String? = {
                switch Int(event.keyCode) {
                case 124: return flags.isEmpty ? "ArrowRight" : nil
                case 126: return flags.isEmpty ? "ArrowUp" : nil
                case 125: return flags.isEmpty ? "ArrowDown" : nil
                case 48: return flags.isEmpty ? "Tab" : nil
                case 36, 76: return flags.isEmpty ? "Enter" : nil
                case 51: return flags.isEmpty ? "Backspace" : nil
                case 49: return " "
                default: return event.characters == "^" ? "^" : (chars == "^" ? "^" : nil)
                }
            }()
            if let name, let edit = boundKey(string, name, sel.location) {
                editKind = nil
                replace(with: edit.text, selection: NSRange(location: edit.caret, length: 0))
                return
            }
        }
        if event.keyCode == 53 {
            onEscape?()
            return
        }
        if onKey?(event) == true { return }
        super.keyDown(with: event)
    }

    /// ↑ ↓ between a fraction's top and bottom (stackVertical), before the bar takes them for the history
    func stackArrow(_ event: NSEvent) -> Bool {
        let flags = event.modifierFlags.intersection([.command, .option, .control, .shift])
        guard Self.stackedOn, selectedRange().length == 0, flags.isEmpty, event.keyCode == 126 || event.keyCode == 125,
              let to = stackVertical(up: event.keyCode == 126) else { return false }
        setSelectedRange(NSRange(location: to, length: 0))
        return true
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        guard window?.firstResponder === self, !hasMarkedText() else { return super.performKeyEquivalent(with: event) }
        // arrows carry the function-key flag, so AppKit offers them here before keyDown
        if stackArrow(event) { return true }
        if onKey?(event) == true { return true }
        // the bar has no Edit menu, so the editing keys are the field's own
        if isCommandKey(event, "c", 8) { copy(nil); return true }
        if isCommandKey(event, "x", 7) { cut(nil); return true }
        if isCommandKey(event, "v", 9) { paste(nil); return true }
        if isCommandKey(event, "a", 0) { selectAll(nil); return true }
        return super.performKeyEquivalent(with: event)
    }

    // never a newline, never a tab character: those are the bar's
    override func insertNewline(_ sender: Any?) {}
    override func insertTab(_ sender: Any?) {}
    override func insertBacktab(_ sender: Any?) {}
    override func cancelOperation(_ sender: Any?) { onEscape?() }

    /// what a copy from the field puts on the pasteboard (copiedEquation: Typst math when "Copy Typst compatible" is on)
    var formatCopy: ((String) -> String)?

    override func copy(_ sender: Any?) {
        let selected = (string as NSString).substring(with: selectedRange())
        guard !selected.isEmpty else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(formatCopy?(selected) ?? selected, forType: .string)
    }

    override func cut(_ sender: Any?) {
        copy(sender)
        editKind = "deleteByCut"
        if keepSlot(selectedRange()) { return }
        insertText("", replacementRange: selectedRange())
    }

    // plain text only, one line (cleanPastedText collapses line breaks)
    override func paste(_ sender: Any?) { pastePlain() }
    override func pasteAsPlainText(_ sender: Any?) { pastePlain() }

    private func pastePlain() {
        editKind = "insertFromPaste"
        guard let raw = NSPasteboard.general.string(forType: .string) else { return }
        if onPasteLines?(raw) == true { return }
        // one plain line, as Spotlight takes a paste: no breaks, tabs or indents
        let clean = PlainText.flatten(raw)
        insertText(clean, replacementRange: selectedRange())
        // a pasted sentence or paragraph goes back on the clipboard that way too
        if PlainText.isProse(clean) { onPastedProse?(clean) }
    }


    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        let attrs: [NSAttributedString.Key: Any] = [.font: Self.lineFont, .foregroundColor: Theme.muted.withAlphaComponent(0.8), .kern: Theme.displayTracking(Self.fontSize)]
        // .quick-inferred: the grey parens the engine would add, ahead of the text and after it
        let inferred: [NSAttributedString.Key: Any] = [.font: Self.lineFont, .foregroundColor: Theme.muted, .kern: Theme.displayTracking(Self.fontSize)]
        // .quick-squiggle: a wavy line, muted at 70%, under where a blank answer stops reading
        if let squiggle, let lm = layoutManager, let tc = textContainer, NSMaxRange(squiggle) <= (string as NSString).length {
            let glyphs = lm.glyphRange(forCharacterRange: squiggle, actualCharacterRange: nil)
            let rect = lm.boundingRect(forGlyphRange: glyphs, in: tc).offsetBy(dx: textContainerInset.width, dy: textContainerInset.height)
            let path = NSBezierPath()
            let y = rect.maxY + 1, step: CGFloat = 2
            var x = rect.minX
            path.move(to: NSPoint(x: x, y: y))
            var up = true
            while x < rect.maxX {
                x += step
                path.line(to: NSPoint(x: min(x, rect.maxX), y: y + (up ? -1.5 : 0)))
                up.toggle()
            }
            path.lineWidth = 1
            Theme.muted.withAlphaComponent(0.7).setStroke()
            path.stroke()
        }
        drawHoles()
        drawStacks()
        // a root's sign and the bar over what it takes, one stroke (native only; Radical.tsx draws a glyph and a bar)
        if let lm = layoutManager, let tc = textContainer {
            Theme.ink.setStroke()
            for range in Self.radicandRanges(string) {
                let inset = NSPoint(x: textContainerInset.width, y: textContainerInset.height)
                let sign = lm.boundingRect(forGlyphRange: lm.glyphRange(forCharacterRange: NSRange(location: range.location - 1, length: 1), actualCharacterRange: nil), in: tc)
                    .offsetBy(dx: inset.x, dy: inset.y)
                let under = lm.boundingRect(forGlyphRange: lm.glyphRange(forCharacterRange: range, actualCharacterRange: nil), in: tc)
                    .offsetBy(dx: inset.x, dy: inset.y)
                let glyph = lm.glyphIndexForCharacter(at: range.location)
                let baseline = lm.lineFragmentRect(forGlyphAt: glyph, effectiveRange: nil).minY + lm.location(forGlyphAt: glyph).y + inset.y
                let points = RootSign.points(left: sign.minX, width: sign.width, end: under.maxX, baseline: baseline, size: Self.fontSize)
                let path = NSBezierPath()
                path.move(to: points[0])
                points.dropFirst().forEach { path.line(to: $0) }
                path.lineWidth = RootSign.lineWidth(Self.fontSize)
                path.lineJoinStyle = .round
                path.lineCapStyle = .round
                path.stroke()
                if (string as NSString).character(at: range.location - 1) == 0x221B {
                    let small: [NSAttributedString.Key: Any] = [.font: NSFont.systemFont(ofSize: Self.fontSize * RootSign.indexScale), .foregroundColor: Theme.ink]
                    ("3" as NSString).draw(at: RootSign.indexOrigin(left: sign.minX, baseline: baseline, size: Self.fontSize), withAttributes: small)
                }
            }
        }
        // the completion's rest, faint, right after the text while the caret is at the end
        // the angle marks inside the line, in the gaps opened after their arguments
        if let lm = layoutManager, let tc = textContainer {
            let length = (string as NSString).length
            for at in angleMarks where at > 0 && at < length {
                let glyph = lm.glyphIndexForCharacter(at: at - 1)
                let rect = lm.boundingRect(forGlyphRange: NSRange(location: glyph, length: 1), in: tc)
                let width = (angleMarkText as NSString).size(withAttributes: Self.attributes).width
                (angleMarkText as NSString).draw(at: NSPoint(x: textContainerInset.width + rect.maxX - width, y: textContainerInset.height),
                                                 withAttributes: [.font: Self.lineFont, .foregroundColor: Theme.muted, .kern: Theme.displayTracking(Self.fontSize)])
            }
        }
        if let offer, offer.at > 0, offer.at <= (string as NSString).length, let lm = layoutManager, let tc = textContainer {
            let glyph = lm.glyphIndexForCharacter(at: offer.at - 1)
            let rect = lm.boundingRect(forGlyphRange: NSRange(location: glyph, length: 1), in: tc)
            let width = (offer.text as NSString).size(withAttributes: Self.attributes).width
            let muted: [NSAttributedString.Key: Any] = [.font: Self.lineFont, .foregroundColor: Theme.muted, .kern: Theme.displayTracking(Self.fontSize)]
            (offer.text as NSString).draw(at: NSPoint(x: textContainerInset.width + rect.maxX - width, y: textContainerInset.height), withAttributes: muted)
        }
        if let lm = layoutManager, let tc = textContainer {
            let used = lm.usedRect(for: tc)
            // a stacked fraction's pull leaves the used rect wider than the line
            var x = stacked.stacks.isEmpty ? textContainerInset.width + used.maxX : stackCaretX((string as NSString).length) ?? textContainerInset.width + used.maxX
            if !ghostTail.isEmpty, selectedRange().location == (string as NSString).length {
                (ghostTail as NSString).draw(at: NSPoint(x: x, y: textContainerInset.height), withAttributes: attrs)
                x += (ghostTail as NSString).size(withAttributes: attrs).width
            }
            // an angle mark for an argument still open at the end of the line
            if angleMarks.contains((string as NSString).length) {
                (angleMarkText as NSString).draw(at: NSPoint(x: x, y: textContainerInset.height), withAttributes: inferred)
                x += (angleMarkText as NSString).size(withAttributes: inferred).width
            }
            if trailingParens > 0 {
                (String(repeating: ")", count: trailingParens) as NSString).draw(at: NSPoint(x: x, y: textContainerInset.height), withAttributes: inferred)
            }
        }
        if string.isEmpty, !placeholder.isEmpty {
            let x = textContainerInset.width
            (placeholder as NSString).draw(at: NSPoint(x: x, y: 0), withAttributes: attrs)
        }
    }

    override var isFlipped: Bool { true }
}

/// Hosts BarTextView in a scroll view (no scrollers) so a long line scrolls sideways under the caret.
struct InputField: NSViewRepresentable {
    @ObservedObject var model: BarModel
    var placeholder: String

    func makeCoordinator() -> Coordinator { Coordinator(model: model) }

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = LineScroll()
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = false
        scroll.hasHorizontalScroller = false
        scroll.verticalScrollElasticity = .none
        scroll.horizontalScrollElasticity = .none
        let text = BarTextView()
        text.delegate = context.coordinator
        text.onKey = { [weak model] event in model?.key(event) ?? false }
        text.onPasteLines = { [weak model] raw in model?.pasteSystem(raw) ?? false }
        text.onPastedProse = { [weak model] plain in model?.copyPlain(plain) }
        text.onDropFiles = { [weak model] urls in model?.dropFiles(urls, to: nil) ?? false }
        text.onDragFiles = { [weak model] over, n in model?.dragHover(over, count: n) }
        text.onEscape = { [weak model] in
            guard let model else { return }
            if !model.escape() { model.onHide?("esc") }
        }
        text.minSize = NSSize(width: 0, height: 36 + 2 * BarTextView.stackRoom)
        text.frame = NSRect(x: 0, y: 0, width: 400, height: 36 + 2 * BarTextView.stackRoom)
        text.autoresizingMask = [.height]
        scroll.documentView = text
        context.coordinator.text = text
        model.focusField = { [weak text] in
            guard let text, let window = text.window else { return }
            window.makeFirstResponder(text)
        }
        model.fieldSelection = { [weak text] in
            guard let text else { return "" }
            return (text.string as NSString).substring(with: text.selectedRange())
        }
        text.boundKey = { [weak model] value, key, caret in
            guard let model else { return nil }
            var edit = model.logic.call("lib/bounds", "boundKey", [value, caret, key]) as? [String: Any]
            if edit == nil, key == "Tab" || key == "ArrowRight", !AppSettings.shared.keepWords,
               !value.lowercased().hasPrefix("message") {
                edit = model.logic.call("lib/bounds", "wordToSign", [value, caret]) as? [String: Any]
            }
            guard let edit, let t = edit["text"] as? String, let c = (edit["caret"] as? NSNumber)?.intValue else { return nil }
            return (t, c)
        }
        text.formatCopy = { [weak model] s in
            guard let model, AppSettings.shared.typstCopy else { return s }
            return model.logic.string("lib/typstMath", "copiedEquation", [s, true]) ?? s
        }
        model.restoreField = { [weak text, weak model] value, sel in
            guard let text else { return }
            text.string = value
            text.restyle()
            text.limits = BarTextView.hasBoundSign(value) ? model?.logic.call("lib/bounds", "boundsIn", [value]) as? [[String: Any]] ?? [] : []
            text.setSelectedRange(sel)
            if model?.text != value { model?.text = value }
        }
        model.setField = { [weak text] value in
            text?.replace(with: value, selection: NSRange(location: (value as NSString).length, length: 0))
        }
        model.fieldCaret = { [weak text] in text?.selectedRange().location ?? 0 }
        model.fillParensInField = { [weak text] value, caret in
            guard let text else { return }
            // the grey ones go first, in this same step: the leading ones' room closes as the typed ( takes it
            text.ghostLead = ""
            text.trailingParens = 0
            text.replace(with: value, selection: NSRange(location: caret, length: 0))
        }
        model.caretAtEnd = { [weak text] in
            guard let text else { return false }
            let r = text.selectedRange()
            return r.length == 0 && r.location == (text.string as NSString).length
        }
        model.insertIntoField = { [weak text] piece in
            guard let text else { return }
            text.window?.makeFirstResponder(text)
            text.insertText(piece, replacementRange: text.selectedRange())
        }
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        guard let text = context.coordinator.text else { return }
        if text.string != model.text {
            text.string = model.text
            text.restyle()
            text.limits = BarTextView.hasBoundSign(model.text) ? model.logic.call("lib/bounds", "boundsIn", [model.text]) as? [[String: Any]] ?? [] : []
            text.setSelectedRange(NSRange(location: (model.text as NSString).length, length: 0))
            model.resetUndo()
        }
        text.placeholder = placeholder
        let deco = model.decorations
        text.ghostLead = String(repeating: "(", count: deco.leadingParens)
        text.trailingParens = deco.trailingParens
        text.squiggle = deco.squiggle
        text.offer = deco.offer
        text.setAngleMarks(model.angleMarks(for: text.string), text: model.angleRad ? " rad" : "°")
        text.colour(units: deco.units, words: deco.words, style: AppSettings.shared.wordColor)
        text.wordNames = deco.names
        text.ghostTail = model.completion ?? ""
        text.insertionPointColor = Theme.ink
        // keep the caret in view as the line grows
        text.sizeToFit()
        text.frame.size.width = max(scroll.contentSize.width, text.frame.width)
        // sizeToFit makes the height the line plus both insets, and the inset centres the line in that height, so it
        // grew to 72 pt in the 36 pt clip: with text the caret is scrolled into view, empty it sat half below the clip
        text.frame.size.height = scroll.contentSize.height
        // a line that fits starts at the left edge again (a grey ( going, or the line getting shorter, could leave
        // it scrolled with its first characters cut off)
        if text.frame.width <= scroll.contentSize.width, scroll.contentView.bounds.origin.x != 0 {
            scroll.contentView.scroll(to: .zero)
            scroll.reflectScrolledClipView(scroll.contentView)
        }
        text.scrollRangeToVisible(text.selectedRange())
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        let model: BarModel
        weak var text: BarTextView?

        init(model: BarModel) { self.model = model }

        // the caret never stops among a limit's hidden `_ ^ ..` (snapOffset)
        func textView(_ textView: NSTextView, willChangeSelectionFromCharacterRange old: NSRange,
                      toCharacterRange new: NSRange) -> NSRange {
            // nor on a stacked fraction's or power's hidden `/ ^ ( )` (StackedMath.dead)
            // (the line as it is now: while a character goes in, the selection moves before the line is restyled)
            if let text, new.length > 0, BarTextView.stackedOn, text.string.contains("/") || text.string.contains("^") {
                // a selection never takes part of a stack (typing over it or deleting it would leave its brackets)
                return StackedMath.expand(StackedMath.parse(text.string), new, length: (text.string as NSString).length)
            }
            if let text, new.length == 0, BarTextView.stackedOn, text.string.contains("/") || text.string.contains("^") {
                let now = StackedMath.parse(text.string)
                if now.dead.contains(new.location) {
                    return NSRange(location: StackedMath.snap(now, new.location, from: old.location, length: (text.string as NSString).length), length: 0)
                }
            }
            guard let text, new.length == 0, !text.limits.isEmpty,
                  model.logic.bool("lib/bounds", "isHidden", [text.limits, new.location]),
                  let o = (model.logic.call("lib/bounds", "snapOffset",
                                            [text.limits, new.location, new.location >= old.location ? 1 : -1, (text.string as NSString).length]) as? NSNumber)?.intValue
            else { return new }
            return NSRange(location: o, length: 0)
        }

        // a click or an arrow key ends the typing run (breakRun)
        func textViewDidChangeSelection(_ notification: Notification) {
            guard let text else { return }
            // what's typed next takes the line's own style, never a gap opened after the character before it
            text.typingAttributes = BarTextView.attributes
            text.refreshAngleMarks()
            // a part of the line selected is answered on its own in the answer slot (selDisplay)
            model.inputSelected(text.string, text.selectedRange())
            guard text.editKind == nil else { return }
            model.breakUndoRun()
        }

        func textDidChange(_ notification: Notification) {
            guard let text else { return }
            defer { if BarTextView.stackedOn { text.restyle() } else { text.hideRootSigns() } }
            // typed words become symbols as they're finished (prettyTokens), unless "keep words" is on
            if !text.hasMarkedText() {
                let caret = text.selectedRange().location
                let pretty = Symbols.pretty(text.string, caret: caret, keepWords: AppSettings.shared.keepWords)
                if pretty.text != text.string {
                    text.textStorage?.replaceCharacters(in: NSRange(location: 0, length: (text.string as NSString).length),
                                                        with: NSAttributedString(string: pretty.text, attributes: BarTextView.attributes))
                    text.setSelectedRange(NSRange(location: pretty.caret, length: 0))
                }
                // afterTyping: a sign then a character starts its lower limit, `..` in it moves to the upper
                let typedChar = text.editKind == "insertText"
                let message = text.string.lowercased().trimmingCharacters(in: .whitespaces).hasPrefix("message")
                if typedChar, !message, BarTextView.hasBoundSign(text.string),
                   let edit = model.logic.call("lib/bounds", "afterTyping", [text.string, text.selectedRange().location]) as? [String: Any],
                   let value = edit["text"] as? String, let caret = (edit["caret"] as? NSNumber)?.intValue {
                    text.textStorage?.replaceCharacters(in: NSRange(location: 0, length: (text.string as NSString).length),
                                                        with: NSAttributedString(string: value, attributes: BarTextView.attributes))
                    text.setSelectedRange(NSRange(location: caret, length: 0))
                }
            }
            text.limits = BarTextView.hasBoundSign(text.string) ? model.logic.call("lib/bounds", "boundsIn", [text.string]) as? [[String: Any]] ?? [] : []
            if model.text != text.string { model.text = text.string }
            let sel = text.selectedRange()
            model.recordEdit(text.string, start: sel.location, end: NSMaxRange(sel), kind: text.editKind)
            text.editKind = nil
            text.sizeToFit()
            text.scrollRangeToVisible(text.selectedRange())
        }
    }
}

/// The grey `(`s the engine would add in front of the line, at the text view's left edge (BarTextView.leadView)
final class GhostLeadView: NSView {
    var text = ""
    var attributes: [NSAttributedString.Key: Any] = [:]
    var y: CGFloat = 0

    override var isFlipped: Bool { true }
    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func draw(_ dirtyRect: NSRect) {
        (text as NSString).draw(at: NSPoint(x: 0, y: y), withAttributes: attributes)
    }
}

/// The line's scroll view, `BarTextView.stackRoom` taller than its row above and below: a click in that room goes
/// to the bar (dragging it, as before) unless the line has a stack that may be drawn there.
final class LineScroll: NSScrollView {
    override func hitTest(_ point: NSPoint) -> NSView? {
        let room = BarTextView.stackRoom
        if let text = documentView as? BarTextView, text.stacked.stacks.isEmpty, frame.contains(point),
           point.y < frame.minY + room || point.y > frame.maxY - room { return nil }
        return super.hitTest(point)
    }
}
