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
            // an effect over the bar (EggOverlay) is Core Animation alone: its layers as they are on screen now
            if window.title == EggOverlay.title, let layer = window.contentView?.layer {
                let scale = window.backingScaleFactor, size = layer.bounds.size
                guard let ctx = CGContext(data: nil, width: Int(size.width * scale), height: Int(size.height * scale), bitsPerComponent: 8,
                                          bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(),
                                          bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { continue }
                ctx.scaleBy(x: scale, y: scale)
                // a grey ground, so the light and dark effects both show
                ctx.setFillColor(CGColor(gray: 0.5, alpha: 1))
                ctx.fill(CGRect(origin: .zero, size: size))
                // the layers are top-down (flipped), so this puts everything where it is on screen; render(in:)
                // ignores the flip for contents, so text and pictures come out mirrored top to bottom here only
                ctx.translateBy(x: 0, y: size.height)
                ctx.scaleBy(x: 1, y: -1)
                (layer.presentation() ?? layer).render(in: ctx)
                if let image = ctx.makeImage(), let png = NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:]) {
                    try? png.write(to: URL(fileURLWithPath: "\(prefix)-effect.png"))
                }
                continue
            }
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

    /// A left click at `x,y` (points from the bar's top-left), sent through the window as a real click is.
    static func click(_ spot: String) {
        let xy = spot.split(separator: ",").compactMap { Double($0.trimmingCharacters(in: .whitespaces)) }
        guard xy.count == 2, let window = NSApp.windows.first(where: { $0 is OverlayPanel && $0.isVisible }) else { return }
        let point = NSPoint(x: xy[0], y: window.frame.height - xy[1])
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            guard let event = NSEvent.mouseEvent(with: type, location: point, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                                                 windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1,
                                                 pressure: type == .leftMouseDown ? 1 : 0) else { continue }
            window.sendEvent(event)
        }
    }

    /// The pointer resting at x,y in the bar (points from its top-left), or leaving the line with `off`: what the
    /// line's tracking area would say, straight to the line (a synthesized mouse-moved never reaches it)
    static func point(_ spot: String) {
        guard let window = NSApp.windows.first(where: { $0 is OverlayPanel && $0.isVisible }),
              let line = find(BarTextView.self, in: window.contentView) else { return }
        if spot == "off" {
            if let event = NSEvent.enterExitEvent(with: .mouseExited, location: .zero, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                                                  windowNumber: window.windowNumber, context: nil, eventNumber: 0, trackingNumber: 0, userData: nil) {
                line.mouseExited(with: event)
            }
            return
        }
        let xy = spot.split(separator: ",").compactMap { Double($0.trimmingCharacters(in: .whitespaces)) }
        guard xy.count == 2, let event = NSEvent.mouseEvent(with: .mouseMoved, location: NSPoint(x: xy[0], y: window.frame.height - xy[1]), modifierFlags: [],
                                                            timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
                                                            context: nil, eventNumber: 0, clickCount: 0, pressure: 0) else { return }
        line.mouseMoved(with: event)
    }

    private static func find<T: NSView>(_ type: T.Type, in view: NSView?) -> T? {
        guard let view else { return nil }
        if let hit = view as? T { return hit }
        for sub in view.subviews { if let hit = find(type, in: sub) { return hit } }
        return nil
    }

    /// A key press, as `enter`, `up`, `down`, `tab`, `escape`, `cmd+w`, `cmd+c`, `ctrl+d`…, to the key window.
    static func key(_ chord: String) {
        guard let parsed = Chord(chord), let code = KeyNames.byName[parsed.key],
              let window = NSApp.keyWindow ?? NSApp.windows.first(where: { $0 is GameNSWindow && $0.isVisible })
                ?? NSApp.windows.first(where: { $0 is OverlayPanel && $0.isVisible }) else { return }
        let chars: String = {
            switch parsed.key {
            case "enter": return "\r"
            case "tab": return "\t"
            case "escape": return "\u{1b}"
            case "space": return " "
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
