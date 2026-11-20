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
            if window is PeriodicPanel, !window.title.contains("Periodic"),
               let sheet = find(NSScrollView.self, in: view)?.documentView,
               let pdf = NSImage(data: sheet.dataWithPDF(inside: sheet.bounds)) {
                // an identity sheet's rows are layer-backed, so through a PDF, and all of it rather than what's scrolled to
                image.unlockFocus()
                let whole = NSImage(size: sheet.bounds.size)
                whole.lockFocus()
                background.setFill()
                NSRect(origin: .zero, size: sheet.bounds.size).fill()
                pdf.draw(in: NSRect(origin: .zero, size: sheet.bounds.size))
                whole.unlockFocus()
                let name = window.title.replacingOccurrences(of: " ", with: "-").lowercased()
                if let tiff = whole.tiffRepresentation, let png = NSBitmapImageRep(data: tiff)?.representation(using: .png, properties: [:]) {
                    try? png.write(to: URL(fileURLWithPath: "\(prefix)-\(name).png"))
                }
                continue
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
                if depth < 9 { for s in v.subviews.prefix(40) { line += tree(s, depth + 1) } }
                return line
            }
            try? tree(view, 0).write(toFile: "\(prefix)-\(name).txt", atomically: true, encoding: .utf8)
        }
    }

    /// Every thread's name, queue and a short stack (frame pointers, symbols by dladdr) into `path`, grouped by
    /// stack with a count: a leak hunt sees which threads pile up, which `sample` can't show without Developer Mode.
    static func threads(to path: String) {
        var list: thread_act_array_t?
        var count: mach_msg_type_number_t = 0
        guard task_threads(mach_task_self_, &list, &count) == KERN_SUCCESS, let list else { return }
        let me = mach_thread_self()
        var groups: [String: Int] = [:]
        for i in 0..<Int(count) {
            let thread = list[i]
            var lines: [String] = []
            if let pt = pthread_from_mach_thread_np(thread) {
                var name = [CChar](repeating: 0, count: 64)
                pthread_getname_np(pt, &name, name.count)
                lines.append("name: \(String(cString: name))")
            }
            var ident = thread_identifier_info_data_t()
            var identCount = mach_msg_type_number_t(MemoryLayout<thread_identifier_info_data_t>.size / MemoryLayout<natural_t>.size)
            let got = withUnsafeMutablePointer(to: &ident) {
                $0.withMemoryRebound(to: integer_t.self, capacity: Int(identCount)) {
                    thread_info(thread, thread_flavor_t(THREAD_IDENTIFIER_INFO), $0, &identCount)
                }
            }
            if got == KERN_SUCCESS, ident.dispatch_qaddr != 0,
               let queue = UnsafePointer<UnsafeRawPointer?>(bitPattern: UInt(ident.dispatch_qaddr))?.pointee {
                let label = __dispatch_queue_get_label(unsafeBitCast(queue, to: DispatchQueue.self))
                lines.append("queue: \(String(cString: label))")
            }
            #if arch(arm64)
            if thread != me, thread_suspend(thread) == KERN_SUCCESS {
                var state = arm_thread_state64_t()
                var stateCount = mach_msg_type_number_t(MemoryLayout<arm_thread_state64_t>.size / MemoryLayout<natural_t>.size)
                let ok = withUnsafeMutablePointer(to: &state) {
                    $0.withMemoryRebound(to: natural_t.self, capacity: Int(stateCount)) {
                        thread_get_state(thread, ARM_THREAD_STATE64, $0, &stateCount)
                    }
                }
                if ok == KERN_SUCCESS {
                    var pcs = [UInt(state.__pc), UInt(state.__lr)]
                    var fp = UInt(state.__fp)
                    while fp != 0, fp % 8 == 0, pcs.count < 24, let frame = UnsafePointer<UInt>(bitPattern: fp) {
                        let next = frame[0], ret = frame[1]
                        if ret == 0 { break }
                        pcs.append(ret & 0x0000_FFFF_FFFF_FFFF)
                        if next <= fp { break }
                        fp = next
                    }
                    for pc in pcs.map({ $0 & 0x0000_FFFF_FFFF_FFFF }) {
                        var info = Dl_info()
                        if dladdr(UnsafeRawPointer(bitPattern: pc), &info) != 0, let sym = info.dli_sname {
                            lines.append("  \(String(cString: sym))")
                        }
                    }
                }
                thread_resume(thread)
            }
            #endif
            groups[lines.joined(separator: "\n"), default: 0] += 1
            mach_port_deallocate(mach_task_self_, thread)
        }
        mach_port_deallocate(mach_task_self_, me)
        vm_deallocate(mach_task_self_, vm_address_t(UInt(bitPattern: list)), vm_size_t(Int(count) * MemoryLayout<thread_t>.stride))
        let text = "\(count) threads\n\n" + groups.sorted { $0.value > $1.value }.map { "x\($0.value)\n\($0.key)" }.joined(separator: "\n\n")
        try? text.write(toFile: path, atomically: true, encoding: .utf8)
    }

    /// A left click at `x,y` (points from the bar's top-left), sent through the window as a real click is;
    /// `click:<x>,<y>,shift` / `,option` / `,control` / `,command` hold those keys for it (the event's flags, and the
    /// ones a tape row reads, `TapeClick.flags`)
    static func click(_ spot: String, count: Int = 1) {
        let parts = spot.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }
        let xy = parts.prefix(2).compactMap { Double($0) }
        var flags: NSEvent.ModifierFlags = []
        if parts.contains("shift") { flags.insert(.shift) }
        if parts.contains("option") || parts.contains("alt") { flags.insert(.option) }
        if parts.contains("control") || parts.contains("ctrl") { flags.insert(.control) }
        if parts.contains("command") || parts.contains("cmd") { flags.insert(.command) }
        guard xy.count == 2, let window = NSApp.windows.first(where: { $0 is OverlayPanel && $0.isVisible }) else { return }
        let point = NSPoint(x: xy[0], y: window.frame.height - xy[1])
        if !flags.isEmpty {
            TapeClick.dev = flags
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { TapeClick.dev = nil }
        }
        // a double-click (`dblclick:`) is two clicks, the second with a click count of 2, as AppKit sends them
        for n in 1...max(1, count) {
            for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
                guard let event = NSEvent.mouseEvent(with: type, location: point, modifierFlags: flags, timestamp: ProcessInfo.processInfo.systemUptime,
                                                     windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: n,
                                                     pressure: type == .leftMouseDown ? 1 : 0) else { continue }
                window.sendEvent(event)
            }
        }
    }

    /// A pinch of `m` (a magnify event's magnification, + spreads the fingers) at x,y in the bar: `pinch:<x>,<y>,<m>`,
    /// to the graph's plot under it (a magnify event can't be made by hand)
    static func pinch(_ spec: String) {
        let nums = spec.split(separator: ",").compactMap { Double($0.trimmingCharacters(in: .whitespaces)) }
        guard nums.count == 3, let window = NSApp.windows.first(where: { $0 is OverlayPanel && $0.isVisible }),
              let content = window.contentView else { return }
        let local = NSPoint(x: nums[0], y: window.frame.height - nums[1])
        guard let plot = content.hitTest(content.superview?.convert(local, from: nil) ?? local) as? GraphPinchable else { return }
        let at = plot.convert(local, from: nil)
        MainActor.assumeIsolated { plot.devPinch(CGFloat(nums[2]), at: at) }
    }

    /// the critical point chip the pointer or keys are on (`chip=` in `graphstate:`), set by the graph panel
    static var graphChip: String?

    /// A press, a drag in `steps` moves and a release from x1,y1 to x2,y2 in the bar (points from its top-left):
    /// `drag:<x1>,<y1>,<x2>,<y2>[,shift][,right][,steps]`, the graph's pan (2D) or orbit (3D; shift pans), sent to the
    /// window as real mouse events
    static func drag(_ spec: String) {
        let parts = spec.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }
        let xy = parts.prefix(4).compactMap { Double($0) }
        guard xy.count == 4, let window = NSApp.windows.first(where: { $0 is OverlayPanel && $0.isVisible }) else { return }
        let flags: NSEvent.ModifierFlags = parts.contains("shift") ? [.shift] : []
        let right = parts.contains("right")
        let steps = parts.dropFirst(4).compactMap { Int($0) }.first ?? 8
        let h = window.frame.height
        func send(_ type: NSEvent.EventType, _ x: Double, _ y: Double) {
            guard let event = NSEvent.mouseEvent(with: type, location: NSPoint(x: x, y: h - y), modifierFlags: flags,
                                                 timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
                                                 context: nil, eventNumber: 0, clickCount: 1, pressure: type == .leftMouseUp || type == .rightMouseUp ? 0 : 1)
            else { return }
            window.sendEvent(event)
        }
        send(right ? .rightMouseDown : .leftMouseDown, xy[0], xy[1])
        for k in 1...max(1, steps) {
            let t = Double(k) / Double(max(1, steps))
            send(right ? .rightMouseDragged : .leftMouseDragged, xy[0] + (xy[2] - xy[0]) * t, xy[1] + (xy[3] - xy[1]) * t)
        }
        send(right ? .rightMouseUp : .leftMouseUp, xy[2], xy[3])
    }

    /// Two fingers (`precise`, points) or a mouse wheel (lines) at x,y in the bar: `scroll:<x>,<y>,<dy>[,<dx>][,precise][,shift]`,
    /// to the view under it as AppKit would send it
    static func scroll(_ spec: String) {
        let parts = spec.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }
        let nums = parts.compactMap { Double($0) }
        guard nums.count >= 3, let window = NSApp.windows.first(where: { $0 is OverlayPanel && $0.isVisible }),
              let content = window.contentView else { return }
        let precise = parts.contains("precise")
        let local = NSPoint(x: nums[0], y: window.frame.height - nums[1])
        // an NSEvent made from a CGEvent has no window, so its locationInWindow is the screen point: the bar's own
        // point is given as that
        let top = NSScreen.screens.first?.frame.maxY ?? 0
        guard let cg = CGEvent(scrollWheelEvent2Source: nil, units: precise ? .pixel : .line, wheelCount: 2,
                               wheel1: Int32(nums[2]), wheel2: Int32(nums.count > 3 ? nums[3] : 0), wheel3: 0) else { return }
        cg.location = CGPoint(x: local.x, y: top - local.y)
        if parts.contains("shift") { cg.flags = .maskShift }
        guard let event = NSEvent(cgEvent: cg),
              let hit = content.hitTest(content.superview?.convert(local, from: nil) ?? local) else { return }
        hit.scrollWheel(with: event)
    }

    /// The bar's accessibility tree as VoiceOver walks it, one element a line, indented by depth:
    /// `role | label | value | selected` (`axtree:<file>`). Read in the app itself, so no Accessibility permission.
    static func axTree(to path: String) {
        guard let panel = NSApp.windows.first(where: { $0 is OverlayPanel && $0.isVisible }),
              let root = panel.contentView else { try? "none".write(toFile: path, atomically: true, encoding: .utf8); return }
        // SwiftUI builds its elements only once an assistive app is about; these ask for them as one would
        for attribute in ["AXEnhancedUserInterface", "AXManualAccessibility"] {
            NSApp.perform(NSSelectorFromString("accessibilitySetValue:forAttribute:"), with: NSNumber(value: true), with: attribute)
        }
        var out = ""
        var seen = Set<ObjectIdentifier>()
        func walk(_ e: NSAccessibilityProtocol, _ depth: Int) {
            guard depth < 40, seen.insert(ObjectIdentifier(e as AnyObject)).inserted else { return }
            let o = e as AnyObject
            // SwiftUI's elements answer the old attribute calls, not always the new methods
            func attr(_ name: String) -> Any? { o.accessibilityAttributeValue?(NSAccessibility.Attribute(rawValue: name)) ?? nil }
            let role = (o.accessibilityRole?() ?? nil)?.rawValue ?? (attr("AXRole") as? String) ?? ""
            let label = e.accessibilityLabel() ?? (attr("AXDescription") as? String) ?? (attr("AXTitle") as? String) ?? ""
            let value = (o.accessibilityValue?() ?? nil).map { "\($0)" } ?? ""
            let selected = (o.isAccessibilitySelected?() ?? false) ? " | selected" : ""
            let hidden = (o.isAccessibilityElement?() ?? true) ? "" : " | group"
            if !role.isEmpty || !label.isEmpty {
                out += String(repeating: "  ", count: depth) + "\(role) | \(label) | \(value)\(selected)\(hidden)\n"
            }
            for c in (o.accessibilityChildren?() ?? nil) ?? (attr("AXChildren") as? [Any]) ?? [] {
                if let c = c as? NSAccessibilityProtocol { walk(c, depth + 1) }
            }
            // a SwiftUI host hands out its elements only to its own view, not its window's content view
            if let v = e as? NSView { for s in v.subviews { walk(s, depth + 1) } }
        }
        walk(root, 0)
        try? out.write(toFile: path, atomically: true, encoding: .utf8)
    }

    /// The graph panel's window and view, one `key=value` a line, into a file (`graphstate:<file>`)
    static func graphState(_ model: BarModel, to path: String) {
        guard let g = model.graph else { try? "none".write(toFile: path, atomically: true, encoding: .utf8); return }
        let lines = [
            "expression=\(g.expression)", "kind=\(g.kind)", "x=\(g.xMin)…\(g.xMax)", "y=\(g.yMin)…\(g.yMax)",
            "fixedY=\(g.fixedY.map { "\($0.lowerBound)…\($0.upperBound)" } ?? "nil")", "yCenter=\(g.yCenter.map { "\($0)" } ?? "nil")",
            "view3D=\(g.view3D.map { "\($0)" } ?? "nil")", "yaw=\(g.yaw)", "pitch=\(g.pitch)", "atHome=\(g.atHome)",
            "curves=\(g.curves.count)", "points=\(g.curves.map(\.points.count))", "contours=\(g.contours.count)",
            "critical=\(g.critical.count)", "roots=\(g.roots)", "intersections=\(g.intersections.count)",
            "layers=\(g.layers.map { "\($0.expression)|\($0.kind)|\($0.points.count)|\($0.contours.count)|\($0.error ?? "")" })",
            "error=\(g.error ?? "")", "failed=\(g.failed)", "chip=\(graphChip ?? "")",
            "home=\(g.homeLo.map { "\($0)" } ?? "nil")…\(g.homeHi.map { "\($0)" } ?? "nil")|\(g.homeYCenter.map { "\($0)" } ?? "nil")|\(g.heldHomeY.map { "\($0)" } ?? "nil")",
        ]
        try? lines.joined(separator: "\n").write(toFile: path, atomically: true, encoding: .utf8)
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
        // over something else that follows the pointer (the graph's 3D view), to that instead
        if let content = window.contentView,
           let hit = content.hitTest(content.convert(event.locationInWindow, from: nil)),
           !hit.isDescendant(of: line), hit.trackingAreas.contains(where: { $0.options.contains(.mouseMoved) }) {
            hit.mouseMoved(with: event)
            return
        }
        line.mouseMoved(with: event)
    }

    private static func find<T: NSView>(_ type: T.Type, in view: NSView?) -> T? {
        guard let view else { return nil }
        if let hit = view as? T { return hit }
        for sub in view.subviews { if let hit = find(type, in: sub) { return hit } }
        return nil
    }

    /// A real click in a sheet, `<title>:<x>,<y>:<file>`, points from its content's top left, and what it copied
    /// written to the file. It copies to a pasteboard of its own, so the one in use stays as it is. `sheethover:`
    /// moves the pointer there instead, for the box round what a click would copy.
    static func sheetClick(_ arg: String, hover: Bool = false) {
        let parts = arg.components(separatedBy: ":")
        // `popover` is the explanation up beside a row (its Related links, Back, Try it in the bar)
        guard parts.count >= 2, let window = NSApp.windows.first(where: {
            $0.isVisible && (parts[0] == "popover" ? $0.className.contains("Popover") : parts[0] == "sheet" ? $0.isSheet : $0.title == parts[0])
        }) else { return }
        let xy = parts[1].split(separator: ",").compactMap { Double($0) }
        guard xy.count == 2, let content = window.contentView else { return }
        window.orderFrontRegardless()
        let at = NSPoint(x: xy[0], y: content.bounds.height - xy[1])
        let board = NSPasteboard(name: NSPasteboard.Name("qcalc.dev.clicks"))
        board.clearContents()
        IdentityWindowController.board = board
        let kinds: [NSEvent.EventType] = hover ? [.mouseMoved] : [.leftMouseDown, .leftMouseUp]
        let events = kinds.compactMap {
            NSEvent.mouseEvent(with: $0, location: at, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                               windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1)
        }
        if hover {
            // a moved pointer goes to the row itself, since this isn't where the real pointer is
            for event in events { content.hitTest(at)?.mouseMoved(with: event) }
        } else if events.count == 2, content.hitTest(at).map({ $0 is NSText || $0 is NSTextField }) == true {
            // the up waits in the queue: a text field's mouse down tracks the selection until it sees one (a settings
            // field clicked this way hung the app, the up sent after the down never arriving)
            NSApp.postEvent(events[1], atStart: false)
            NSApp.sendEvent(events[0])
        } else {
            for event in events { NSApp.sendEvent(event) }
        }
        // back to the real one, for a person's own clicks
        IdentityWindowController.board = .general
        guard parts.count >= 3 else { return }
        let file = parts[2]
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
            try? (board.string(forType: .string) ?? "(nothing)").write(toFile: file, atomically: true, encoding: .utf8)
        }
    }

    /// A key press, as `enter`, `up`, `down`, `tab`, `escape`, `cmd+w`, `cmd+c`, `ctrl+d`…, to the key window.
    static func key(_ chord: String) {
        guard let window = NSApp.keyWindow ?? NSApp.windows.first(where: { $0 is GameNSWindow && $0.isVisible })
                ?? NSApp.windows.first(where: { $0 is OverlayPanel && $0.isVisible }) else { return }
        for event in keyEvents(chord, window) {
            // posted, not sent: it comes through the event queue, so local key monitors see it as they do a real key
            NSApp.postEvent(event, atStart: false)
        }
    }

    /// A key handed straight to the bar's line, as AppKit would hand it on (⌘ chords as key equivalents first), so the
    /// tape can be driven while q calc isn't the active app and the bar has no key window for `key:` to reach
    /// `watch` sees each key down first, as the bar's local key monitor does (the Konami code), which a key handed
    /// straight to the window skips
    static func barKey(_ chord: String, watch: (NSEvent) -> Void = { _ in }) {
        guard let window = NSApp.windows.first(where: { $0 is OverlayPanel && $0.isVisible }) else { return }
        for event in keyEvents(chord, window) {
            if event.type == .keyDown { watch(event) }
            if event.type == .keyDown, event.modifierFlags.contains(.command), window.performKeyEquivalent(with: event) { return }
            window.sendEvent(event)
        }
    }

    /// The tape as the keys left it: open, the picked row and part, each row (`expr = display`), the line, the clipboard
    static func tapeState(_ model: BarModel, to file: String) {
        let lines = ["open=\(model.tapeOpen)", "selected=\(model.selected.map(String.init) ?? "nil")", "pick=\(model.pick)",
                     "rows=\(model.rows.count)", "line=\(model.text)", "copied=\(model.copied)",
                     "clip=\(NSPasteboard.general.string(forType: .string) ?? "")"]
            + model.rows.map { "row=\($0.expr) = \($0.display)" }
        try? lines.joined(separator: "\n").write(toFile: file, atomically: true, encoding: .utf8)
    }

    /// The front sheet window, or its explanation popover while one is up: a key handed straight to it, as AppKit
    /// would hand it on (⌘ chords as key equivalents first), so a sheet can be driven while q calc isn't the
    /// active app (a locked screen, another app in front) and has no key window for `key:` to reach
    static func sheetKey(_ chord: String) {
        guard let window = sheetKeyWindow() else { return }
        for event in keyEvents(chord, window) {
            if event.type == .keyDown, event.modifierFlags.contains(.command), window.performKeyEquivalent(with: event) { return }
            window.sendEvent(event)
        }
    }

    /// The open game's window: a key handed straight to it (`gamekey:<chord>`, `:down`/`:up` for one half, a held
    /// key), so a game can be driven while q calc isn't the active app (a locked screen), as `key:` can't
    static func gameKey(_ arg: String) {
        guard let window = NSApp.windows.first(where: { $0 is GameNSWindow && $0.isVisible }) else { return }
        var chord = arg, only: NSEvent.EventType?
        if arg.hasSuffix(":down") { chord = String(arg.dropLast(5)); only = .keyDown }
        if arg.hasSuffix(":up") { chord = String(arg.dropLast(3)); only = .keyUp }
        for event in keyEvents(chord, window) where only == nil || event.type == only {
            if event.type == .keyDown, event.modifierFlags.contains(.command), window.performKeyEquivalent(with: event) { return }
            window.sendEvent(event)
        }
    }

    /// The game window's title, frame, whether it is the big window, its appearance and who has the keys, key=value
    /// lines (`open=0` with no game up)
    static func gameState(to file: String) {
        var out = NSApp.windows.contains { $0 is GameNSWindow && $0.contentView != nil } ? "open=hidden" : "open=0"
        if let w = NSApp.windows.first(where: { $0 is GameNSWindow && $0.isVisible }) {
            let f = w.frame
            out = ["open=1", "title=\(w.title)", "frame=\(Int(f.minX)),\(Int(f.minY)),\(Int(f.width)),\(Int(f.height))",
                   "big=\(w.styleMask.contains(.titled) ? 1 : 0)", "dark=\(w.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua ? 1 : 0)",
                   "responder=\(w.firstResponder.map { String(describing: type(of: $0)) } ?? "none")",
                   "field=\((w.firstResponder as? NSTextView)?.string ?? "")"].joined(separator: "\n")
        }
        out += "\napp=\(NSApp.isHidden ? "hidden" : NSApp.isActive ? "active" : "back")"
        try? out.write(toFile: file, atomically: true, encoding: .utf8)
    }

    /// Text typed into the game's own field with the keys (24's line)
    static func gameKeys(_ text: String) {
        guard let window = NSApp.windows.first(where: { $0 is GameNSWindow && $0.isVisible }) else { return }
        (window.firstResponder as? NSTextView)?.insertText(text, replacementRange: NSRange(location: NSNotFound, length: 0))
    }

    /// Text typed into the front sheet's (or its popover's) field with the keys, as `keys:` does for the key window
    static func sheetKeys(_ text: String) {
        (sheetKeyWindow()?.firstResponder as? NSTextView)?.insertText(text, replacementRange: NSRange(location: NSNotFound, length: 0))
    }

    /// The explanation up beside a row: its title, then the text selected in it (`(none)` when no popover is up)
    static func sheetExplain(to file: String) {
        var out = "(none)"
        if let sheet = PeriodicPanel.front(),
           let popover = NSApp.windows.first(where: { $0.isVisible && $0.className.contains("Popover") && $0.parent === sheet }),
           let text = popover.firstResponder as? NSTextView {
            let title = text.string.components(separatedBy: "\n").first ?? ""
            out = title + "\n" + (text.string as NSString).substring(with: text.selectedRange())
        }
        try? out.write(toFile: file, atomically: true, encoding: .utf8)
    }

    /// winkey:<title>:<chord> and winkeys:<title>:<text>: a key or text straight to the visible window with that title
    /// (Settings' tabs are titled General, Units, Messages), as `sheetkey:` does for a sheet, for a locked screen where
    /// no window has the keys. A click first (sheetclick:<title>:<x>,<y>) puts a field in front.
    static func windowKey(_ arg: String, text: Bool = false) {
        guard let cut = arg.firstIndex(of: ":") else { return }
        let title = String(arg[..<cut]), rest = String(arg[arg.index(after: cut)...])
        guard let window = NSApp.windows.first(where: { $0.isVisible && $0.title == title }) else { return }
        if text {
            (window.firstResponder as? NSTextView)?.insertText(rest, replacementRange: NSRange(location: NSNotFound, length: 0))
            return
        }
        for event in keyEvents(rest, window) {
            // ⌘ chords as AppKit offers them: the window's own, then the menu bar's (Settings' Edit menu)
            if event.type == .keyDown, event.modifierFlags.contains(.command),
               window.performKeyEquivalent(with: event) || NSApp.mainMenu?.performKeyEquivalent(with: event) == true { return }
            window.sendEvent(event)
        }
    }

    /// postkey:<title>:<chord>: a key put in the app's event queue for that window, so local key monitors see it as they
    /// see a real key (settings' shortcut recorder); `sheet` names a window's sheet (the keybinds)
    static func postKey(_ arg: String) {
        guard let cut = arg.firstIndex(of: ":") else { return }
        let title = String(arg[..<cut]), chord = String(arg[arg.index(after: cut)...])
        guard let window = NSApp.windows.first(where: { $0.isVisible && (title == "sheet" ? $0.isSheet : $0.title == title) }) else { return }
        for event in keyEvents(chord, window) { NSApp.postEvent(event, atStart: false) }
    }

    private static func sheetKeyWindow() -> NSWindow? {
        guard let sheet = PeriodicPanel.front() else { return nil }
        let popover = NSApp.windows.first { $0.isVisible && $0.className.contains("Popover") && $0.parent === sheet }
        return popover ?? sheet
    }

    private static func keyEvents(_ chord: String, _ window: NSWindow) -> [NSEvent] {
        guard let parsed = Chord(chord), let code = KeyNames.byName[parsed.key] else { return [] }
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
        return [NSEvent.EventType.keyDown, .keyUp].compactMap { type in
            NSEvent.keyEvent(with: type, location: .zero, modifierFlags: parsed.menuModifiers,
                             timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
                             context: nil, characters: chars, charactersIgnoringModifiers: chars,
                             isARepeat: false, keyCode: UInt16(code))
        }
    }
}
