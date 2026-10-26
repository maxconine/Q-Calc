import AppKit
import SwiftUI

/// The bar's one-line input: an NSTextView, because a TextField can't take every key first, and later needs
/// attributed spans and ghost text (MAC_UI_PLAN.md §2). No Writing Tools, no "Write with Siri", no inline
/// predictions, no autocorrection, no smart quotes: it's for maths.
final class BarTextView: NSTextView {
    var onKey: ((NSEvent) -> Bool)?
    var onEscape: (() -> Void)?
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
            restyle()
            if let offer, offer.at > 0, offer.at <= (string as NSString).length, let storage = textStorage {
                let width = (offer.text as NSString).size(withAttributes: Self.attributes).width
                storage.addAttribute(.kern, value: Theme.displayTracking(Self.fontSize) + width, range: NSRange(location: offer.at - 1, length: 1))
            }
            needsDisplay = true
        }
    }
    private var coloured: (units: [NSRange], words: [NSRange]) = ([], [])

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

    private func updateInset() {
        let lead = ghostLead.isEmpty ? 0 : (ghostLead as NSString).size(withAttributes: [.font: font!]).width + 2
        // the line sits in the middle of its 36 pt box, as the web input's does
        let line = layoutManager?.defaultLineHeight(for: font!) ?? Self.fontSize * 1.2
        textContainerInset = NSSize(width: lead, height: max(0, (bounds.height - line) / 2))
        needsDisplay = true
    }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        updateInset()
    }

    override var acceptsFirstResponder: Bool { true }
    override var needsPanelToBecomeKey: Bool { true }

    // the bar's keys first (↵, ↑↓, tab, the rebindable chords); everything else is typing
    override func keyDown(with event: NSEvent) {
        if hasMarkedText() { return super.keyDown(with: event) }
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
        let clean = raw.replacingOccurrences(of: #"\s*[\r\n]+\s*"#, with: " ", options: .regularExpression)
            .trimmingCharacters(in: .whitespaces)
        insertText(clean, replacementRange: selectedRange())
    }


    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        let attrs: [NSAttributedString.Key: Any] = [.font: font!, .foregroundColor: Theme.muted.withAlphaComponent(0.8), .kern: Theme.displayTracking(Self.fontSize)]
        // .quick-inferred: the grey parens the engine would add, ahead of the text and after it
        let inferred: [NSAttributedString.Key: Any] = [.font: font!, .foregroundColor: Theme.muted, .kern: Theme.displayTracking(Self.fontSize)]
        if !ghostLead.isEmpty {
            (ghostLead as NSString).draw(at: NSPoint(x: 0, y: textContainerInset.height), withAttributes: inferred)
        }
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
        text.formatCopy = { [weak model] s in
            guard let model, AppSettings.shared.typstCopy else { return s }
            return model.logic.string("lib/typstMath", "copiedEquation", [s, true]) ?? s
        }
        model.restoreField = { [weak text, weak model] value, sel in
            guard let text else { return }
            text.string = value
            text.restyle()
            text.setSelectedRange(sel)
            if model?.text != value { model?.text = value }
        }
        model.setField = { [weak text] value in
            text?.replace(with: value, selection: NSRange(location: (value as NSString).length, length: 0))
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
            text.setSelectedRange(NSRange(location: (model.text as NSString).length, length: 0))
            model.resetUndo()
        }
        text.placeholder = placeholder
        let deco = model.decorations
        text.ghostLead = String(repeating: "(", count: deco.leadingParens)
        text.trailingParens = deco.trailingParens
        text.squiggle = deco.squiggle
        text.offer = deco.offer
        text.colour(units: deco.units, words: deco.words)
        text.ghostTail = model.completion ?? ""
        text.insertionPointColor = Theme.ink
        // keep the caret in view as the line grows
        text.sizeToFit()
        text.frame.size.width = max(scroll.contentSize.width, text.frame.width)
        text.scrollRangeToVisible(text.selectedRange())
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        let model: BarModel
        weak var text: BarTextView?

        init(model: BarModel) { self.model = model }

        // a click or an arrow key ends the typing run (breakRun)
        func textViewDidChangeSelection(_ notification: Notification) {
            guard let text, text.editKind == nil else { return }
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
            }
            if model.text != text.string { model.text = text.string }
            let sel = text.selectedRange()
            model.recordEdit(text.string, start: sel.location, end: NSMaxRange(sel), kind: text.editKind)
            text.editKind = nil
            text.sizeToFit()
            text.scrollRangeToVisible(text.selectedRange())
        }
    }
}
