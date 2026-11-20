import Foundation

// The walkthrough's and the cheat sheet's small decisions, as QuickCalc.tsx makes them, kept apart from BarModel so
// `scripts/test-tour.sh` can check them headlessly. Foundation only.

enum TourRules {
    /// The shortcut the walkthrough and `?` teach: the one that got registered, or none when no shortcut works (the
    /// page's `nativeInfo.hotkey` is empty then, so the hotkey step and the `show / hide` row are left out).
    static func hotkeyTitle(active: String?, chosen: String, failed: Bool) -> String {
        failed ? "" : (active ?? chosen)
    }

    /// A line that plots (`graph …` or `plot …`), counted toward the walkthrough's graph extra.
    static func isGraphLine(_ text: String) -> Bool {
        text.range(of: #"^\s*(?:graph|plot)\s"#, options: [.regularExpression, .caseInsensitive]) != nil
    }

    /// The hint after a save or a copy: `that’s the basics · ? lists everything` when it finished the basics, else
    /// the one-time hint it picked (none while the basics are left).
    static func afterHint(basicsLeftBefore: Bool, basicsLeftAfter: Bool, doneHint: String, next: String?) -> String? {
        basicsLeftBefore && !basicsLeftAfter ? doneHint : next
    }

    /// The ⌘C hint (`⌘C copies the answer`, bit 16): a copy has taught it, so it's marked with the copy step.
    static let copyHintBit = 16

    /// The `?` sheet's key rows: the shortcut first when there is one, and no row whose key is unset (a keybind
    /// cleared in Settings), as the core's `cheatSheet` leaves them out.
    static func keyRows(hotkey: String, rows: [(String, String)]) -> [(String, String)] {
        (hotkey.isEmpty ? [] : [(hotkey, "show / hide")]) + rows.filter { !$0.0.trimmingCharacters(in: .whitespaces).isEmpty }
    }
}
