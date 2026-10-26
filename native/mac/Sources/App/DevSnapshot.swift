import AppKit
import Carbon

// Dev builds only (driven by native/mac/scripts/drive.sh through a distributed notification): pictures of the
// app's windows drawn by the app itself, so they work without Screen Recording permission or a lit display, and
// real key events sent through NSApp, so the bar's own key handling is what's tested.
enum DevSnapshot {
    /// One PNG per visible window into `prefix-<name>.png`. Vibrancy can't be captured from inside the process,
    /// so each picture sits on the window's own background colour.
    static func writeAll(to prefix: String) {
        for window in NSApp.windows where window.isVisible && window.className != "NSStatusBarWindow" {
            // a titled panel's frame view draws over its content in a cache, so those take the content view
            let frameView = window is NSPanel && !(window is OverlayPanel) && window.styleMask.contains(.titled) && !window.title.contains("Periodic")
                ? window.contentView : (window.contentView?.superview ?? window.contentView)
            guard let view = frameView, view.bounds.width > 4 else { continue }
            let bounds = view.bounds
            let image = NSImage(size: bounds.size)
            image.lockFocus()
            var background = NSColor.windowBackgroundColor
            (window.appearance ?? NSApp.effectiveAppearance).performAsCurrentDrawingAppearance {
                background = NSColor(cgColor: NSColor.windowBackgroundColor.cgColor) ?? .windowBackgroundColor
            }
            // the bar paints its own card; other windows get their background colour behind the capture
            if !(window is OverlayPanel) {
                background.setFill()
                NSBezierPath(roundedRect: NSRect(origin: .zero, size: bounds.size), xRadius: 10, yRadius: 10).fill()
            }
            if !window.title.contains("Periodic") || view.layer == nil {
                // SwiftUI draws through cacheDisplay
                guard let rep = view.bitmapImageRepForCachingDisplay(in: bounds) else { image.unlockFocus(); continue }
                view.cacheDisplay(in: bounds, to: rep)
                rep.draw(in: NSRect(origin: .zero, size: bounds.size))
            } else if let ctx = NSGraphicsContext.current?.cgContext, let layer = view.layer {
                // the sheets' tiles and rows are layer-backed views, which cacheDisplay leaves out
                (window.appearance ?? NSApp.effectiveAppearance).performAsCurrentDrawingAppearance {
                    layer.render(in: ctx)
                }
            }
            image.unlockFocus()
            guard let tiff = image.tiffRepresentation, let png = NSBitmapImageRep(data: tiff)?.representation(using: .png, properties: [:]) else { continue }
            let name: String
            switch window {
            case is OverlayPanel: name = "bar"
            default: name = (window.title.isEmpty ? String(describing: type(of: window)) : window.title)
                .replacingOccurrences(of: " ", with: "-").replacingOccurrences(of: "/", with: "-").lowercased()
            }
            try? png.write(to: URL(fileURLWithPath: "\(prefix)-\(name).png"))
            // the view tree beside it, for windows a picture can't show (or VoiceOver checks)
            func tree(_ v: NSView, _ depth: Int) -> String {
                let label = v.accessibilityLabel() ?? (v as? NSTextField)?.stringValue ?? ""
                var line = String(repeating: "  ", count: depth) + "\(type(of: v)) \(NSStringFromRect(v.frame)) \(label)\n"
                if depth < 6 { for s in v.subviews.prefix(40) { line += tree(s, depth + 1) } }
                return line
            }
            try? tree(view, 0).write(toFile: "\(prefix)-\(name).txt", atomically: true, encoding: .utf8)
        }
    }

    /// A key press, as `enter`, `up`, `down`, `tab`, `escape`, `cmd+w`, `cmd+c`, `ctrl+d`…, to the key window.
    static func key(_ chord: String) {
        guard let parsed = Chord(chord), let code = KeyNames.byName[parsed.key],
              let window = NSApp.keyWindow ?? NSApp.windows.first(where: { $0 is OverlayPanel && $0.isVisible }) else { return }
        let chars: String = {
            switch parsed.key {
            case "enter": return "\r"
            case "tab": return "\t"
            case "escape": return "\u{1b}"
            case "up": return String(Character(UnicodeScalar(NSUpArrowFunctionKey)!))
            case "down": return String(Character(UnicodeScalar(NSDownArrowFunctionKey)!))
            case "left": return String(Character(UnicodeScalar(NSLeftArrowFunctionKey)!))
            case "right": return String(Character(UnicodeScalar(NSRightArrowFunctionKey)!))
            case "backspace": return "\u{7f}"
            default: return parsed.key.count == 1 ? parsed.key : ""
            }
        }()
        for type in [NSEvent.EventType.keyDown, .keyUp] {
            guard let event = NSEvent.keyEvent(with: type, location: .zero, modifierFlags: parsed.menuModifiers,
                                               timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
                                               context: nil, characters: chars, charactersIgnoringModifiers: chars,
                                               isARepeat: false, keyCode: UInt16(code)) else { continue }
            NSApp.sendEvent(event)
        }
    }
}
