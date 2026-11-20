import Foundation

// Pure rules for the effects and the Konami code (no AppKit), so mac/scripts/test-eggs.sh can check them alone.
enum EggRules {
    /// The Konami code's B and A go on into the line (the field still gets every key), so a finished code left `ba`
    /// in it. `before` is the line as the B came, `after` the line once the A landed: when `after` is `before` with
    /// `ba` (any case) put in at one spot, the line to put back and its caret (UTF-16); nil when anything else changed.
    static func konamiLeftover(before: String, after: String) -> (text: String, caret: Int)? {
        let b = Array(before.utf16), a = Array(after.utf16)
        guard a.count == b.count + 2 else { return nil }
        var p = 0
        while p < b.count, a[p] == b[p] { p += 1 }
        let typed = String(utf16CodeUnits: [a[p], a[p + 1]], count: 2).lowercased()
        guard typed == "ba", Array(a[(p + 2)...]) == Array(b[p...]) else { return nil }
        return (before, p)
    }

    /// The hotkey hides the bar when it's up: up includes a barrel roll, which holds the bar at alpha 0 while its
    /// picture spins (the hotkey read that as hidden and showed it again instead).
    static func barIsUp(visible: Bool, onActiveSpace: Bool, alpha: Double, rolling: Bool) -> Bool {
        visible && onActiveSpace && (alpha > 0 || rolling)
    }
}
