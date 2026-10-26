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

    /// ↵ in an equation: the next one, or save from the last
    func sysEnter(_ i: Int) {
        guard let lines = sysLines else { return }
        if i + 1 < lines.count { sysFocus = i + 1 } else { commitSystem() }
    }

    func solveSystemLive() {
        guard let lines = sysLines else { return }
        let units = settings.defaultUnits, angle = angleRad ? "rad" : "deg"
        engine.submit(.forms, { [lines] engine -> (String, Bool)? in
            guard let json = try? engine.callJson(module: "engine/system", function: "solveLive",
                                                 argsJson: Self.json([lines, units, angle])),
                  let o = try? JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any],
                  let display = o["display"] as? String else { return nil }
            return (display, (o["message"] as? Bool) == true)
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
        sysLines = nil
        sysAnswer = nil
        sysFocus = nil
        focusField?()
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
    /// ↑ ↓ between equations (and ↑ from the first back to the bar); key presses need macOS 14, so on 13 Tab
    /// moves between them instead
    @ViewBuilder func arrowKeys(up: @escaping () -> Void, down: @escaping () -> Void) -> some View {
        if #available(macOS 14.0, *) {
            self.onKeyPress(.upArrow) { up(); return .handled }
                .onKeyPress(.downArrow) { down(); return .handled }
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

    var body: some View {
        VStack(spacing: 2) {
            ForEach(Array((model.sysLines ?? []).enumerated()), id: \.offset) { i, line in
                TextField("", text: Binding(get: { line }, set: { model.setSysLine(i, $0) }),
                          prompt: Text("equation \(i + 1)").foregroundColor(Color.qMuted.opacity(0.8)))
                    .textFieldStyle(.plain)
                    .font(.system(size: Theme.row))
                    .tracking(Theme.displayTracking(Theme.row))
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
                        if i == 0 { model.focusField?() } else { focused = i - 1 }
                    }, down: {
                        if i + 1 < (model.sysLines?.count ?? 0) { focused = i + 1 }
                    })
                    .accessibilityLabel("Equation \(i + 1)")
            }
        }
        .padding(8)
        .background(RoundedRectangle(cornerRadius: Theme.radiusCard).fill(Color.qBgTape)
            .shadow(color: .black.opacity(0.06), radius: 1.5, y: 1))
        .overlay(RoundedRectangle(cornerRadius: Theme.radiusCard).strokeBorder(Color.black.opacity(0.1), lineWidth: 0.5))
        .padding(.horizontal, 12)
        .padding(.bottom, 12)
        .onChange(of: model.sysFocus) { f in
            if let f { focused = f; model.sysFocus = nil }
        }
        .onAppear {
            focused = model.sysFocus ?? 0
            // ⌘V of several equations into field i fills the system from there on (onPasteLines)
            paste.start { [weak model] in
                guard let model, let i = paste.field, let raw = NSPasteboard.general.string(forType: .string) else { return false }
                return model.pasteSystem(raw, keeping: Array((model.sysLines ?? []).prefix(i)))
            }
        }
        .onDisappear { paste.stop() }
        .onChange(of: focused) { paste.field = $0 }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("System of equations")
    }
}

/// ⌘V while a system field has the keys, offered to `take` first
final class PasteWatch {
    var field: Int?
    private var monitor: Any?

    func start(_ take: @escaping () -> Bool) {
        stop()
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            let mods = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
            guard self?.field != nil, mods == .command, event.charactersIgnoringModifiers?.lowercased() == "v" else { return event }
            return take() ? nil : event
        }
    }

    func stop() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
    }

    deinit { stop() }
}
