import AppKit
import QCalcCore
import SwiftUI

// `sys N` (SystemPanel.tsx, engine/system.ts sysCommand): ↵ opens N equation fields under the bar, solved live as
// they're typed (solveLive) and saved as one history row that reopens them. The solving is the engine's
// `engine/system` `solveLive` through callJson; it answers once WP-SOLVE ports system.ts.

enum SysCommand: Equatable {
    case count(Int)
    case hint(String)

    /// sysCommand: `sys`/`system` with how many equations, up to five
    static func of(_ text: String) -> SysCommand? {
        let t = text.trimmingCharacters(in: .whitespaces)
        guard let re = try? NSRegularExpression(pattern: #"^sys(?:tem)?\s*(\d+)?\s*$"#, options: .caseInsensitive),
              let m = re.firstMatch(in: t, range: NSRange(t.startIndex..., in: t)) else { return nil }
        guard let r = Range(m.range(at: 1), in: t), let n = Int(t[r]) else { return .hint("how many equations? sys2, sys3… up to sys5") }
        return (1...5).contains(n) ? .count(n) : .hint("up to 5 equations")
    }

    /// an empty line, or one still typing its way to `sys` / `system` (a closed system's equations wait through it)
    static func onTheWay(_ text: String) -> Bool {
        let t = text.trimmingCharacters(in: .whitespaces).lowercased()
        return t.isEmpty || "system".hasPrefix(t)
    }
}

extension BarModel {
    /// ↵ on `sys N`: the fields (or, already open, down to the first or save)
    func enterSystem(_ count: Int) -> Bool {
        if sysLines?.count != count {
            sysLines = Array(repeating: "", count: count)
            sysFocus = 0
            return true
        }
        if sysLines?.allSatisfy({ $0.trimmingCharacters(in: .whitespaces).isEmpty }) == true {
            sysFocus = 0
        } else {
            commitSystem()
        }
        return true
    }

    func setSysLine(_ i: Int, _ value: String) {
        guard var lines = sysLines, lines.indices.contains(i) else { return }
        lines[i] = Symbols.pretty(value, caret: (value as NSString).length, keepWords: settings.keepWords).text
        sysLines = lines
        solveSystemLive()
    }

    /// → at the end of an equation types in the parens it would be read with (inferParens), as → and Tab do on
    /// the bar; false when there's nothing to add, or the caret isn't at the end, so → moves as usual
    func fillSysParens(_ i: Int) -> Bool {
        guard let lines = sysLines, lines.indices.contains(i),
              let editor = PasteWatch.keyEditor,
              editor.selectedRange() == NSRange(location: (editor.string as NSString).length, length: 0),
              let fill = logic.call("engine/parens", "inferParens", [lines[i]]) as? [String: Any],
              let filled = fill["filled"] as? String, filled != lines[i] else { return false }
        setSysLine(i, filled)
        return true
    }

    /// ↵ in an equation: the next one, or save from the last
    func sysEnter(_ i: Int) {
        guard let lines = sysLines else { return }
        if i + 1 < lines.count { sysFocus = i + 1 } else { commitSystem() }
    }

    /// ⌘⌫ in an equation (native only): empties it, or, already empty, closes the system for a fresh bar
    func sysDeleteLine(_ i: Int) {
        guard let lines = sysLines, lines.indices.contains(i) else { return }
        if lines[i].trimmingCharacters(in: .whitespaces).isEmpty {
            closeSystem()
            text = ""
        } else {
            setSysLine(i, "")
        }
    }

    func solveSystemLive() {
        guard let lines = sysLines else { return }
        let units = settings.defaultUnits, angle = angleRad ? "rad" : "deg"
        let fractions = settings.fractionMode, figures = Int(options.sigFigs)
        engine.submit(.forms, { [lines] engine -> (String, Bool)? in
            guard let json = try? engine.callJson(module: "engine/system", function: "solveLive",
                                                 argsJson: Self.json([lines, units, angle])),
                  let o = try? JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any],
                  let display = o["display"] as? String else { return nil }
            let message = (o["message"] as? Bool) == true
            // native only: fractions only with fraction form on
            guard !fractions, !message,
                  let plain = try? engine.callJson(module: "lib/answer", function: "systemDecimals", argsJson: Self.json([display, figures])),
                  let decimals = try? JSONSerialization.jsonObject(with: Data(plain.utf8), options: [.fragmentsAllowed]) as? String else {
                return (display, message)
            }
            return (decimals, message)
        }) { [weak self] answer in
            guard let self, self.sysLines == lines else { return }
            self.sysAnswer = answer.map { SysAnswer(display: $0.0, message: $0.1) }
        }
    }

    static func json(_ any: Any) -> String {
        (try? JSONSerialization.data(withJSONObject: any)).flatMap { String(data: $0, encoding: .utf8) } ?? "[]"
    }

    /// saved as the web saves it: expr `sys`, the equations, and what the panel showed
    func commitSystem() {
        guard let lines = sysLines else { return }
        let written = lines.map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }.joined(separator: "; ")
        let shown = sysAnswer?.display.trimmingCharacters(in: .whitespaces) ?? ""
        guard !written.isEmpty, !shown.isEmpty, sysAnswer?.message != true else {
            hint = "nothing to save yet · finish the equations, or esc to close"
            track("error.blank")
            return
        }
        let row = HistoryRow(id: "\(Int(Date().timeIntervalSince1970 * 1000))-sys", expr: "sys", display: shown, kind: .system,
                             equations: lines.map { $0.trimmingCharacters(in: .whitespaces) }, at: Int64(Date().timeIntervalSince1970 * 1000))
        history.replace(rows + [row])
        rows = history.rows
        refreshRecent()
        Analytics.shared.trackCalc(expr: "sys", system: true)
        closeSystem()
        text = ""
    }

    func closeSystem() {
        sysClosed = nil
        sysLines = nil
        sysAnswer = nil
        sysFocus = nil
        focusField?()
    }

    /// ⌘Z / ⇧⌘Z in an equation: its value put back as it was, not read again for symbols (FieldUndo)
    func restoreSysLine(_ i: Int, _ value: String) {
        guard var lines = sysLines, lines.indices.contains(i), lines[i] != value else { return }
        lines[i] = value
        sysLines = lines
        solveSystemLive()
    }

    /// a system row in the tape reopens its fields (openHistorySystem)
    func openHistorySystem(_ i: Int) {
        guard rows.indices.contains(i), rows[i].kind == .system, let eqs = rows[i].equations, !eqs.isEmpty else { return }
        openSystem(Array(eqs.prefix(5)))
    }

    /// the system fields, opened with these equations (openSystem)
    func openSystem(_ lines: [String]) {
        selected = nil
        sysLines = lines
        setField?("sys \(lines.count)")
        sysLines = lines
        solveSystemLive()
    }

    /// pasteSystem: two to five equations pasted (pastedEquations of the cleaned text) open as a system, after the
    /// fields before the one pasted into; anything else is plain text (false)
    func pasteSystem(_ raw: String, keeping before: [String] = []) -> Bool {
        guard raw.range(of: #"[\r\n;]"#, options: .regularExpression) != nil,
              let clean = logic.string("lib/paste", "cleanPastedText", [raw]),
              let lines = logic.call("engine/system", "pastedEquations", [clean]) as? [String] else { return false }
        openSystem(Array((before + lines).prefix(5)))
        return true
    }
}

private extension View {
    /// ↑ ↓ between equations (and ↑ from the first back to the bar), Tab / ⇧Tab round them, → at the end fills
    /// parens; key presses need macOS 14, so on 12 and 13 Tab moves between them by the key view loop instead
    @ViewBuilder func arrowKeys(up: @escaping () -> Void, down: @escaping () -> Void, tab: @escaping (Bool) -> Void,
                                right: @escaping () -> Bool) -> some View {
        if #available(macOS 14.0, *) {
            self.onKeyPress(.upArrow) { up(); return .handled }
                .onKeyPress(.downArrow) { down(); return .handled }
                .onKeyPress(.tab, phases: .down) { press in tab(press.modifiers.contains(.shift)); return .handled }
                // ⇧Tab can come as the back-tab character rather than Tab with shift
                .onKeyPress(KeyEquivalent("\u{19}"), phases: .down) { _ in tab(true); return .handled }
                .onKeyPress(.rightArrow) { right() ? .handled : .ignored }
        } else {
            self
        }
    }
}

struct SysAnswer: Equatable {
    var display: String
    var message: Bool
}

/// .sys: a card under the bar (tape colour, 10 pt corners, the card shadow), 40 pt fields on --bg at 17 pt
struct SystemPanel: View {
    @ObservedObject var model: BarModel
    @FocusState private var focused: Int?
    @State private var paste = PasteWatch()
    /// each equation's own ⌘Z / ⇧⌘Z (QA item 76)
    @State private var undo = FieldUndo<Int>()

    var body: some View {
        VStack(spacing: 2) {
            ForEach(Array((model.sysLines ?? []).enumerated()), id: \.offset) { i, line in
                TextField("", text: Binding(get: { line }, set: { model.setSysLine(i, $0) }))
                    .textFieldStyle(.plain)
                    .font(.system(size: Theme.row))
                    .mutedPrompt("equation \(i + 1)", shown: line.isEmpty, size: Theme.row, color: Color.qMuted.opacity(0.8),
                                 kerning: Theme.displayTracking(Theme.row))
                    .viewTracking(Theme.displayTracking(Theme.row))
                    .foregroundStyle(Color.qInk)
                    .padding(.horizontal, 12)
                    .frame(height: 40)
                    .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(Color.qBg))
                    .overlay(RoundedRectangle(cornerRadius: Theme.radiusS)
                        .strokeBorder(focused == i ? Color.qAccent.opacity(0.55) : .clear, lineWidth: 1))
                    .noWritingTools()
                    .tint(Color.qInk)
                    .autocorrectionDisabled()
                    .focused($focused, equals: i)
                    .onSubmit { model.sysEnter(i) }
                    .arrowKeys(up: {
                        if i == 0 {
                            // after SwiftUI has let go of the field, or it takes the keys back off the bar
                            focused = nil
                            DispatchQueue.main.async { model.focusField?() }
                        } else {
                            focused = i - 1
                        }
                    }, down: {
                        if i + 1 < (model.sysLines?.count ?? 0) { focused = i + 1 }
                    }, tab: { back in
                        // Tab / ⇧Tab go round the equations (native only)
                        let n = model.sysLines?.count ?? 1
                        focused = (i + (back ? n - 1 : 1)) % n
                    }, right: {
                        model.fillSysParens(i)
                    })
                    .accessibilityLabel("Equation \(i + 1)")
            }
        }
        .padding(8)
        .background(RoundedRectangle(cornerRadius: Theme.radiusCard).fill(Color.qBgTape)
            .shadow(color: Surfaces.cardShadow, radius: 1.5, y: 1))
        .overlay(RoundedRectangle(cornerRadius: Theme.radiusCard).strokeBorder(Surfaces.cardRing, lineWidth: 0.5))
        .padding(.horizontal, 12)
        .padding(.bottom, 12)
        .onChange(of: model.sysFocus) { f in
            guard let f else { return }
            model.sysFocus = nil
            // the bar took the keys through AppKit, so `focused` can still say f, and setting it again would do nothing
            focused = nil
            DispatchQueue.main.async { focused = f }
        }
        .onAppear {
            undo.reset()
            observe(model.sysLines ?? [])
            paste.edit = { [undo] in undo.note($0, now: FieldUndo<Int>.nowMs) }
            paste.undo = { [weak model, undo, paste] redo in
                guard let model, let i = paste.field, let step = undo.step(i, redo: redo) else { return }
                model.restoreSysLine(i, step.value)
                PasteWatch.select(step)
            }
            focused = model.sysFocus ?? 0
            // spent, or asking for the same field again later wouldn't count as a change
            model.sysFocus = nil
            // ⌘V of several equations into field i fills the system from there on (onPasteLines)
            paste.start(paste: { [weak model] in
                guard let model, let i = paste.field, let raw = NSPasteboard.general.string(forType: .string) else { return false }
                return model.pasteSystem(raw, keeping: Array((model.sysLines ?? []).prefix(i)))
            }, deleteLine: { [weak model] in
                // the equation's own editor has the keys, and isn't mid-IME
                guard let model, let i = paste.field,
                      let editor = PasteWatch.keyEditor,
                      editor.isFieldEditor, !editor.hasMarkedText() else { return false }
                model.sysDeleteLine(i)
                return true
            })
        }
        .onDisappear { paste.stop() }
        .onChange(of: model.sysLines) { lines in
            guard let lines else { return }
            observe(lines)
        }
        .onChange(of: focused) { field in
            paste.field = field
            // the caret at the end of the equation, not the whole of it selected for the next key to replace
            guard field != nil else { return }
            DispatchQueue.main.async {
                guard let editor = PasteWatch.keyEditor, editor.isFieldEditor else { return }
                editor.setSelectedRange(NSRange(location: (editor.string as NSString).length, length: 0))
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("System of equations")
    }

    /// the equations into their undo; a system of another size starts afresh
    private func observe(_ lines: [String]) {
        let values = Dictionary(uniqueKeysWithValues: lines.enumerated().map { ($0.offset, $0.element) })
        if undo.count != values.count { undo.reset() }
        undo.observe(values, caret: { i in i == focused ? PasteWatch.editorSelection(lines[i]) : nil }, now: FieldUndo<Int>.nowMs)
    }
}

/// ⌘V and ⌘⌫ while a system field has the keys, offered to `paste` and `deleteLine` first; with `undo` set, ⌘Z /
/// ⇧⌘Z go to it (the field's own undo, FieldUndo) and every key and click is told to `edit` first
final class PasteWatch {
    var field: Int?
    /// ⌘Z (false) or ⇧⌘Z (true) in the field
    var undo: ((Bool) -> Void)?
    /// what each key (or a click) is to the field's typing run
    var edit: ((FieldKeyEdit) -> Void)?
    private var monitor: Any?

    func start(paste: @escaping () -> Bool, deleteLine: @escaping () -> Bool) {
        stop()
        monitor = NSEvent.addLocalMonitorForEvents(matching: [.keyDown, .leftMouseDown]) { [weak self] event in
            guard let self, self.field != nil else { return event }
            if event.type == .leftMouseDown {
                self.edit?(.breakRun)
                return event
            }
            let mods = event.modifierFlags.intersection([.command, .option, .control, .shift])
            let editor = PasteWatch.keyEditor
            // mid-IME the keys are the input method's
            if editor?.hasMarkedText() == true { return event }
            // the field's, not the line's (SwiftUI's focus can still name a field once the line has the keys)
            if let undo = self.undo, editor?.isFieldEditor == true, !(editor is BarTextView), mods == .command || mods == [.command, .shift],
               event.keyCode == 6 || event.charactersIgnoringModifiers?.lowercased() == "z" {
                // the field editor's own undo has nothing (SwiftUI sets the text again each key): never left to it
                undo(mods.contains(.shift))
                return nil
            }
            self.edit?(FieldKeyEdit.of(keyCode: event.keyCode, command: mods.contains(.command),
                                        control: mods.contains(.control), characters: event.characters ?? ""))
            guard mods == .command else { return event }
            if event.keyCode == 51 { return deleteLine() ? nil : event }
            guard event.charactersIgnoringModifiers?.lowercased() == "v" else { return event }
            return paste() ? nil : event
        }
    }

    /// the text view with the bar's keys: the key window's, else the bar's own. The bar is a nonactivating panel, so
    /// q calc needn't be the active app while it's up and NSApp.keyWindow can be nil there; read from that alone, ⌘Z,
    /// ⌘⌫ and → filling parens in an equation or cell found no editor and did nothing (QA item 86)
    static var keyEditor: NSTextView? {
        (NSApp.keyWindow ?? NSApp.windows.first { $0 is OverlayPanel && $0.isVisible })?.firstResponder as? NSTextView
    }

    /// the field editor's text and selection, when it's showing `value`
    static func editorSelection(_ value: String) -> NSRange? {
        guard let editor = PasteWatch.keyEditor, editor.isFieldEditor,
              editor.string == value else { return nil }
        return editor.selectedRange()
    }

    /// after an undo step is put back: its selection, once SwiftUI has given the field its text (the caret would be
    /// left at the end)
    static func select(_ step: Undo.Step) {
        func apply() {
            guard let editor = PasteWatch.keyEditor, editor.isFieldEditor,
                  editor.string == step.value else { return }
            let n = (step.value as NSString).length, start = min(step.start, n)
            editor.setSelectedRange(NSRange(location: start, length: max(0, min(step.end, n) - start)))
        }
        DispatchQueue.main.async {
            apply()
            DispatchQueue.main.async { apply() }
        }
    }

    func stop() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
    }

    deinit { stop() }
}
