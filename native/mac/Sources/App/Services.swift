import AppKit
import QCalcCore

// Q Calc's own entries in the Services menu (any app's app menu > Services and its right-click menus on selected
// text), declared under NSServices in Info.plist:
//   Calculate with Q Calc: replaces the selection with its answer (`12*7` → `84`), as the answer copies (core
//     answerCopy, so Copy without units applies); several lines evaluate together (`x = 3` then `x^2`) and the last
//     answer is used. Nothing answers (blank, or still working after a few seconds): the selection stays and it beeps.
//   Open in Q Calc: shows the bar with the selection typed in (its lines joined by spaces).
// The line is evaluated alone, without the tape's variables or ans, so the same text gives the same answer anywhere.
final class ServiceProvider: NSObject {
    private let engine: EngineThread
    private let logic: AppLogic
    private let open: (String) -> Void

    init(engine: EngineThread, logic: AppLogic, open: @escaping (String) -> Void) {
        self.engine = engine
        self.logic = logic
        self.open = open
    }

    /// the text a service was handed: one string, its lines trimmed, blank lines dropped
    static func lines(_ text: String) -> [String] {
        text.components(separatedBy: .newlines).map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
    }

    @objc func calculate(_ pboard: NSPasteboard, userData: String?, error: AutoreleasingUnsafeMutablePointer<NSString?>) {
        let lines = Self.lines(pboard.string(forType: .string) ?? "")
        guard !lines.isEmpty else { return fail("Select something to calculate.", error) }
        let options = EvalOptions.from(AppSettings.shared)
        // the Services request waits on the main thread, so a slow line gives up rather than hanging the app
        guard let answers = engine.wait(.service, timeout: 4, { $0.evaluate(lines: lines, options: options) }),
              let last = answers.last, !last.display.isEmpty else {
            return fail("Q Calc has no answer for that.", error)
        }
        let copied = logic.string("lib/touches", "answerCopy", [last.display, AppSettings.shared.copyUnitless]) ?? last.display
        guard !copied.isEmpty else { return fail("Q Calc has no answer for that.", error) }
        OverlayLog.note("service: calculate")
        pboard.clearContents()
        pboard.setString(copied, forType: .string)
    }

    @objc func openInQCalc(_ pboard: NSPasteboard, userData: String?, error: AutoreleasingUnsafeMutablePointer<NSString?>) {
        // the bar is one line: a selection wrapped over several lines comes in joined by spaces
        let text = Self.lines(pboard.string(forType: .string) ?? "").joined(separator: " ")
        guard !text.isEmpty else { return fail("Select something to open in Q Calc.", error) }
        OverlayLog.note("service: open")
        open(text)
    }

    private func fail(_ message: String, _ error: AutoreleasingUnsafeMutablePointer<NSString?>) {
        NSSound.beep()
        error.pointee = message as NSString
    }
}
