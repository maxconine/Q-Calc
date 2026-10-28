import AppKit
import SwiftUI

/// The bar's one-line input: an NSTextView, because a TextField can't take every key first, and later needs
/// attributed spans and ghost text (MAC_UI_PLAN.md §2). No Writing Tools, no "Write with Siri", no inline
/// predictions, no autocorrection, no smart quotes: it's for maths.
final class BarTextView: NSTextView {
    var onKey: ((NSEvent) -> Bool)?
    var onEscape: (() -> Void)?
    /// a paste with line breaks or `;`: true when it was taken (equations opened as a system)
    var onPasteLines: ((String) -> Bool)?
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
    private var gapsFor = ""

    func setAngleMarks(_ marks: [Int], text mark: String) {
        guard marks != angleMarks || mark != angleMarkText || gapsFor != string else { return }
        angleMarks = marks
        angleMarkText = mark
        applyGaps()
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
    private var coloured: (units: [NSRange], words: [NSRange]) = ([], [])

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
    func colour(units: [NSRange], words: [NSRange]) {
        guard units != coloured.units || words != coloured.words, let lm = layoutManager else { return }
        coloured = (units, words)
        let all = NSRange(location: 0, length: (string as NSString).length)
        lm.removeTemporaryAttribute(.foregroundColor, forCharacterRange: all)
        let word = Theme.accent.blended(withFraction: 0.15, of: Theme.ink) ?? Theme.accent
        for r in words where NSMaxRange(r) <= all.length { lm.addTemporaryAttribute(.foregroundColor, value: word, forCharacterRange: r) }
        for r in units where NSMaxRange(r) <= all.length { lm.addTemporaryAttribute(.foregroundColor, value: Theme.accent, forCharacterRange: r) }
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
            let onWindow = self.convert(NSRect(x: rect.minX, y: 0, width: rect.width, height: self.bounds.height), to: nil)
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
        guard let storage = textStorage, storage.length > 0 else { return }
        storage.setAttributes(Self.attributes, range: NSRange(location: 0, length: storage.length))
        styleBounds()
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
        guard let storage = textStorage, !limits.isEmpty else { return }
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
    }

    /// .bounds-hole: an empty limit's muted box, 0.62 × 0.8 em at the limits' size
    private func drawHoles() {
        guard let lm = layoutManager, let tc = textContainer, !holes.isEmpty else { return }
        let em = Self.fontSize * 0.56
        for h in holes where h.sign < (string as NSString).length {
            let g = lm.glyphIndexForCharacter(at: h.sign)
            let r = lm.boundingRect(forGlyphRange: NSRange(location: g, length: 1), in: tc)
                .offsetBy(dx: textContainerInset.width, dy: textContainerInset.height)
            let y = h.upper ? r.minY + 2 : r.maxY - em * 0.8 - 1
            let box = NSRect(x: r.maxX + (h.upper ? em * 0.3 : em * 0.05), y: y, width: em * 0.62, height: em * 0.8)
            Theme.muted.withAlphaComponent(0.18).setFill()
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

    // backspace in `[]` takes both; a +- that meets at the caret becomes ± (joinPlusMinusAt)
    override func deleteForward(_ sender: Any?) {
        editKind = "deleteContentForward"
        super.deleteForward(sender)
    }

    override func deleteBackward(_ sender: Any?) {
        editKind = "deleteContentBackward"
        let sel = selectedRange()
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
        storage.addLayoutManager(layout)
        let container = NSTextContainer(size: NSSize(width: CGFloat.greatestFiniteMagnitude, height: 40))
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
    }

    /// the grey leading parens, a view of their own: drawn by the text view itself they sat in the inset, which
    /// the text view doesn't repaint, so a new one never showed and an old one stayed after the line was cleared
    private let leadView = GhostLeadView()

    private func updateInset() {
        // as wide as typed `(` with the line's tracking, so filling one in moves nothing
        let lead = ghostLead.isEmpty ? 0 : (ghostLead as NSString).size(withAttributes: Self.attributes).width
        // the line sits in the middle of its 36 pt box, as the web input's does
        let line = layoutManager?.defaultLineHeight(for: font!) ?? Self.fontSize * 1.2
        textContainerInset = NSSize(width: lead, height: max(0, (bounds.height - line) / 2))
        if leadView.superview == nil { addSubview(leadView) }
        leadView.text = ghostLead
        leadView.attributes = [.font: font!, .foregroundColor: Theme.muted, .kern: Theme.displayTracking(Self.fontSize)]
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

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        guard window?.firstResponder === self, !hasMarkedText() else { return super.performKeyEquivalent(with: event) }
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
        insertText("", replacementRange: selectedRange())
    }

    // plain text only, one line (cleanPastedText collapses line breaks)
    override func paste(_ sender: Any?) { pastePlain() }
    override func pasteAsPlainText(_ sender: Any?) { pastePlain() }

    private func pastePlain() {
        editKind = "insertFromPaste"
        guard let raw = NSPasteboard.general.string(forType: .string) else { return }
        if onPasteLines?(raw) == true { return }
        let clean = raw.replacingOccurrences(of: #"\s*[\r\n]+\s*"#, with: " ", options: .regularExpression)
            .trimmingCharacters(in: .whitespaces)
        insertText(clean, replacementRange: selectedRange())
    }


    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        let attrs: [NSAttributedString.Key: Any] = [.font: font!, .foregroundColor: Theme.muted.withAlphaComponent(0.8), .kern: Theme.displayTracking(Self.fontSize)]
        // .quick-inferred: the grey parens the engine would add, ahead of the text and after it
        let inferred: [NSAttributedString.Key: Any] = [.font: font!, .foregroundColor: Theme.muted, .kern: Theme.displayTracking(Self.fontSize)]
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
        // a radical's bar over what it takes, as Radical.tsx draws it in the web input
        if let lm = layoutManager, let tc = textContainer {
            Theme.ink.setFill()
            for range in Self.radicandRanges(string) {
                let glyphs = lm.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
                var rect = lm.boundingRect(forGlyphRange: glyphs, in: tc)
                rect = rect.offsetBy(dx: textContainerInset.width, dy: textContainerInset.height)
                NSRect(x: rect.minX, y: rect.minY + Self.fontSize * 0.14, width: rect.width, height: 1.3).fill()
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
                                                 withAttributes: [.font: font!, .foregroundColor: Theme.muted, .kern: Theme.displayTracking(Self.fontSize)])
            }
        }
        if let offer, offer.at > 0, offer.at <= (string as NSString).length, let lm = layoutManager, let tc = textContainer {
            let glyph = lm.glyphIndexForCharacter(at: offer.at - 1)
            let rect = lm.boundingRect(forGlyphRange: NSRange(location: glyph, length: 1), in: tc)
            let width = (offer.text as NSString).size(withAttributes: Self.attributes).width
            let muted: [NSAttributedString.Key: Any] = [.font: font!, .foregroundColor: Theme.muted, .kern: Theme.displayTracking(Self.fontSize)]
            (offer.text as NSString).draw(at: NSPoint(x: textContainerInset.width + rect.maxX - width, y: textContainerInset.height), withAttributes: muted)
        }
        if let lm = layoutManager, let tc = textContainer {
            let used = lm.usedRect(for: tc)
            var x = textContainerInset.width + used.maxX
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
        let scroll = NSScrollView()
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = false
        scroll.hasHorizontalScroller = false
        scroll.verticalScrollElasticity = .none
        scroll.horizontalScrollElasticity = .none
        let text = BarTextView()
        text.delegate = context.coordinator
        text.onKey = { [weak model] event in model?.key(event) ?? false }
        text.onPasteLines = { [weak model] raw in model?.pasteSystem(raw) ?? false }
        text.onDropFiles = { [weak model] urls in model?.dropFiles(urls, to: nil) ?? false }
        text.onDragFiles = { [weak model] over, n in model?.dragHover(over, count: n) }
        text.onEscape = { [weak model] in
            guard let model else { return }
            if !model.escape() { model.onHide?("esc") }
        }
        text.minSize = NSSize(width: 0, height: 36)
        text.frame = NSRect(x: 0, y: 0, width: 400, height: 36)
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
        text.colour(units: deco.units, words: deco.words)
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
            // a part of the line selected is answered on its own in the answer slot (selDisplay)
            model.inputSelected(text.string, text.selectedRange())
            guard text.editKind == nil else { return }
            model.breakUndoRun()
        }

        func textDidChange(_ notification: Notification) {
            guard let text else { return }
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
