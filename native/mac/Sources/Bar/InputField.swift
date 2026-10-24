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

    static let fontSize: CGFloat = 24

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
        allowsUndo = true
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
        font = .systemFont(ofSize: Self.fontSize, weight: .regular)
        textColor = .labelColor
        insertionPointColor = .controlAccentColor
        typingAttributes = [.font: font!, .foregroundColor: NSColor.labelColor]
        setAccessibilityLabel("Calculation")
        setAccessibilityHelp("Type maths; the answer appears as you type. Return saves it to the history.")
        setAccessibilityRole(.textField)
    }

    private func updateInset() {
        let lead = ghostLead.isEmpty ? 0 : (ghostLead as NSString).size(withAttributes: [.font: font!]).width + 2
        textContainerInset = NSSize(width: lead, height: 0)
        needsDisplay = true
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
        return super.performKeyEquivalent(with: event)
    }

    // never a newline, never a tab character: those are the bar's
    override func insertNewline(_ sender: Any?) {}
    override func insertTab(_ sender: Any?) {}
    override func insertBacktab(_ sender: Any?) {}
    override func cancelOperation(_ sender: Any?) { onEscape?() }

    // plain text only, one line (cleanPastedText collapses line breaks)
    override func paste(_ sender: Any?) { pastePlain() }
    override func pasteAsPlainText(_ sender: Any?) { pastePlain() }

    private func pastePlain() {
        guard let raw = NSPasteboard.general.string(forType: .string) else { return }
        let clean = raw.replacingOccurrences(of: #"\s*[\r\n]+\s*"#, with: " ", options: .regularExpression)
            .trimmingCharacters(in: .whitespaces)
        insertText(clean, replacementRange: selectedRange())
    }


    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        let attrs: [NSAttributedString.Key: Any] = [.font: font!, .foregroundColor: NSColor.tertiaryLabelColor]
        if !ghostLead.isEmpty {
            (ghostLead as NSString).draw(at: NSPoint(x: 0, y: 0), withAttributes: attrs)
        }
        // the completion's rest, faint, right after the text while the caret is at the end
        if !ghostTail.isEmpty, selectedRange().location == (string as NSString).length, let lm = layoutManager, let tc = textContainer {
            let used = lm.usedRect(for: tc)
            (ghostTail as NSString).draw(at: NSPoint(x: textContainerInset.width + used.maxX, y: 0), withAttributes: attrs)
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
        text.minSize = NSSize(width: 0, height: 32)
        text.frame = NSRect(x: 0, y: 0, width: 400, height: 32)
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
            text.setSelectedRange(NSRange(location: (model.text as NSString).length, length: 0))
            text.undoManager?.removeAllActions(withTarget: text.textStorage as Any)
        }
        text.placeholder = placeholder
        text.ghostLead = model.chained ? "ans" : ""
        text.ghostTail = model.completion ?? ""
        text.textColor = .labelColor
        // keep the caret in view as the line grows
        text.sizeToFit()
        text.frame.size.width = max(scroll.contentSize.width, text.frame.width)
        text.scrollRangeToVisible(text.selectedRange())
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        let model: BarModel
        weak var text: BarTextView?

        init(model: BarModel) { self.model = model }

        func textDidChange(_ notification: Notification) {
            guard let text else { return }
            if model.text != text.string { model.text = text.string }
            text.sizeToFit()
            text.scrollRangeToVisible(text.selectedRange())
        }
    }
}
