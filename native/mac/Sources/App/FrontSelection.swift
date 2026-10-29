import AppKit
import ApplicationServices

/// The text highlighted in the app in front, read through Accessibility as the bar opens (Settings › Bring in
/// highlighted text, off to start). Read off the main thread with a short timeout, so a slow or hung app never
/// holds the bar up.
enum FrontSelection {
    /// the most it takes, so a whole highlighted document doesn't land in the line
    static let limit = 2000

    /// macOS's Accessibility prompt, only when the setting is turned on (it's off to start, so nobody is asked
    /// unless they want this); true when it's allowed
    @discardableResult
    static func ask() -> Bool {
        if AXIsProcessTrusted() { return true }
        let prompt = kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String
        return AXIsProcessTrustedWithOptions([prompt: true] as CFDictionary)
    }

    /// the highlighted text of the app in front (not Q Calc), on the main queue: nil when there's none, it's off,
    /// or Accessibility isn't allowed (never asked here: only turning the setting on asks)
    static func read(_ done: @escaping (String?) -> Void) {
        guard AppSettings.shared.bringSelection, AXIsProcessTrusted(),
              let front = NSWorkspace.shared.frontmostApplication,
              front.processIdentifier != ProcessInfo.processInfo.processIdentifier else { return done(nil) }
        let pid = front.processIdentifier
        DispatchQueue.global(qos: .userInitiated).async {
            let text = selectedText(pid)
            DispatchQueue.main.async { done(text) }
        }
    }

    private static func selectedText(_ pid: pid_t) -> String? {
        let app = AXUIElementCreateApplication(pid)
        AXUIElementSetMessagingTimeout(app, 0.25)
        var focused: CFTypeRef?
        guard AXUIElementCopyAttributeValue(app, kAXFocusedUIElementAttribute as CFString, &focused) == .success,
              let focused, CFGetTypeID(focused) == AXUIElementGetTypeID() else { return nil }
        let element = focused as! AXUIElement
        AXUIElementSetMessagingTimeout(element, 0.25)
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, kAXSelectedTextAttribute as CFString, &value) == .success,
              let text = value as? String else { return nil }
        let line = PlainText.flatten(text)
        return line.isEmpty ? nil : String(line.prefix(limit))
    }
}
