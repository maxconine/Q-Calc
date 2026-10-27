import AppKit
import QCalcCore
import Carbon
import SwiftUI

@main
enum QCalc {
    static let delegate = AppDelegate()

    static func main() {
        let app = NSApplication.shared
        app.delegate = delegate
        app.run()
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
    private var overlay: OverlayController?
    // the note under the menu bar icon naming the shortcut, while it's up
    private var nudge: NSPopover?
    private var nudgeObserver: NSObjectProtocol?
    private var hotKeyRef: EventHotKeyRef?
    private var hotKeyHandlerInstalled = false
    private var statusItem: NSStatusItem?
    private var settingsWindow: SettingsWindowController?
    private var settingsObserver: NSObjectProtocol?

    func applicationWillFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.prohibited)
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        setupStatusItem()
        // before the web view boots, so its injected settings already carry the shortcut
        registerHotKey()
        OverlayLog.note("launch \(Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "dev"), "
            + "macOS \(ProcessInfo.processInfo.operatingSystemVersionString), \(hotKeyDiary())")
        refreshStatusItem()
        settingsObserver = NotificationCenter.default.addObserver(
            forName: .qcalcSettingsChanged, object: nil, queue: .main
        ) { [weak self] _ in self?.refreshStatusItem() }
        // the engine gets its own thread and the tape its file; the bar is native, so it's ready as soon as it's made
        let core = EngineFactory.make()
        SheetData.shared = SheetData(engine: core, logic: AppLogic(engine: core))
        let engine = EngineThread(engine: core)
        let model = BarModel(engine: engine, core: core, history: HistoryStore(engine: core))
        // the ECB rates ECBRates.swift fetches, handed to the engine as they arrive (its cache lives in Q Calc's
        // own folder, so the engine is given the JSON rather than reading <dataDir>/rates.json)
        let feedRates: (String) -> Void = { json in engine.submit(.misc, { try? $0.setRates(ratesJson: json) }) { _ in } }
        ECBRates.shared.onChange = feedRates
        feedRates(ECBRates.shared.pageJSON)
        overlay = OverlayController(model: model)
        overlay?.onSettings = { [weak self] in self?.showSettings() }
        // `message`, `paste to` and `send file to` with no name set: straight to where it's set
        overlay?.onMessagesSettings = { [weak self] in
            self?.showSettings()
            if let tabs = NSApp.windows.lazy.compactMap({ $0.contentViewController as? NSTabViewController }).first,
               let i = tabs.tabViewItems.firstIndex(where: { $0.label == "Messages" }) { tabs.selectedTabViewItemIndex = i }
        }
        overlay?.preload()
        Analytics.shared.start(logic: model.logic)
        // the engine's first call pays for its lazy tables; do it now, off the main thread
        engine.submit(.forms, { $0.evaluate(lines: ["1+1"], options: defaultEvalOptions()) }) { _ in
            OverlayLog.note(String(format: "engine warm %.0f ms after process start", AppInfo.sinceProcessStart * 1000))
        }
        if AppSettings.shared.claimFirstRun() {
            overlay?.showFirstRun()
        } else if AppSettings.shared.claimHotKeyWarning() {
            overlay?.show(from: "shortcut warning")
        }
        // dev builds carry no update key, so Updates never starts there (same appcast and key as macos/ in release)
        Updates.shared.start()
        // with a name set, the inbox waits for messages whether or not the bar is up
        Messages.shared.start()
        scheduleHotKeyNudge()
        let launched = AppInfo.sinceProcessStart
        OverlayLog.note(String(format: "launched in %.0f ms (process start to ready)", launched * 1000))
        if ProcessInfo.processInfo.environment["QCALC_LAUNCH_TIMING"] != nil {
            FileHandle.standardError.write(Data(String(format: "qcalc-launch-ms %.1f\n", launched * 1000).utf8))
        }
        // dev builds only: scripts drive the bar without the hotkey (native/mac/scripts/drive.sh)
        if AppInfo.isDev {
            DistributedNotificationCenter.default().addObserver(forName: Notification.Name("\(Bundle.main.bundleIdentifier ?? "com.maxconine.qcalc.native-dev").command"), object: nil, queue: .main) { [weak self] note in
                self?.devCommand(note.object as? String ?? "")
            }
        }
        if ProcessInfo.processInfo.arguments.contains("--show") {
            DispatchQueue.main.async { [weak self] in self?.overlay?.show(from: "launch argument") }
        }
    }

    // a dev send's result, where a script's snapshot and the log can both see it
    private static func devSaid(_ status: String, _ overlay: OverlayController?) {
        NSLog("Q Calc dev: %@", status)
        OverlayLog.note("dev share: \(status)")
        overlay?.model.hint = status
    }

    // "show", "hide", "settings", "theme:dark", "type:<text>", "enter", "up", "periodic", "identities"
    private func devCommand(_ command: String) {
        guard let overlay else { return }
        let (verb, arg) = command.firstIndex(of: ":").map { (String(command[..<$0]), String(command[command.index(after: $0)...])) } ?? (command, "")
        if verb.hasPrefix("send") { OverlayLog.note("dev: \(verb)") }
        switch verb {
        case "show": overlay.show(from: "dev")
        case "hide": overlay.hide(because: "dev")
        case "settings":
            showSettings()
            // settings:<tab index> opens that tab (2 is Messages)
            if let i = Int(arg), let tabs = NSApp.windows.lazy.compactMap({ $0.contentViewController as? NSTabViewController }).first,
               tabs.tabViewItems.indices.contains(i) { tabs.selectedTabViewItemIndex = i }
        case "theme": AppSettings.shared.setTheme(arg, notifyWeb: true)
        case "type": overlay.model.setField?(arg)
        case "enter": overlay.model.enter()
        case "up": overlay.model.moveSelection(up: true)
        case "down": overlay.model.moveSelection(up: false)
        case "tab": overlay.model.cycleForms()
        case "esc": if !overlay.model.escape() { overlay.hide(because: "dev esc") }
        case "card":
            overlay.model.cards.insert(BannerCard(id: "dev\(Int.random(in: 0...9999))", from: arg.isEmpty ? "max" : arg,
                                                  text: "lunch at noon? bring the 4.2 m tape", ask: false), at: 0)
        case "ask": overlay.model.cards.insert(BannerCard(id: "dev\(Int.random(in: 0...9999))", from: arg.isEmpty ? "sam" : arg, text: "", ask: true), at: 0)
        case "snap": DevSnapshot.writeAll(to: arg)
        case "clear": overlay.model.clearHistory(); overlay.model.text = ""; overlay.model.hint = nil
        case "hover": overlay.model.edgeFocus = arg != "off"
        case "contrast": overlay.devContrast = arg != "off"
        // drop:<path>|<path>… is files dropped on the bar (Messages/ShareCommands.swift)
        case "drop": overlay.model.dropFiles(arg.split(separator: "|").map { URL(fileURLWithPath: String($0)) }, to: nil)
        case "periodic": overlay.model.text = "periodic"; overlay.model.enter()
        case "identities": overlay.model.text = "identities"; overlay.model.enter()
        case "key": DevSnapshot.key(arg)
        // text typed into whatever field has the keys (a reply line, the bar), and a click at x,y in the bar
        // (points from its top-left)
        case "keys": (NSApp.keyWindow?.firstResponder as? NSTextView)?.insertText(arg, replacementRange: NSRange(location: NSNotFound, length: 0))
        case "click": DevSnapshot.click(arg)
        // what settings' Remove / Unblock does for a name
        case "forget": Messages.shared.forget(arg)
        case "bench": DevBench.run(spec: arg, model: overlay.model)
        // sendfile:<name>:<path> offers a file as a drop on their thread would; savefile saves every file that can
        // be (Messages/MessageFiles.swift), as Save does
        case "sendfile":
            let parts = arg.split(separator: ":", maxSplits: 1).map(String.init)
            if parts.count == 2 { MessageFiles.shared.sendFile(URL(fileURLWithPath: parts[1]), to: parts[0]) { [weak overlay] in Self.devSaid($0, overlay) } }
        // sendclip:<name>:<text> shares text as `paste to` shares the clipboard, without touching it
        case "sendclip":
            let parts = arg.split(separator: ":", maxSplits: 1).map(String.init)
            if parts.count == 2 { MessageFiles.shared.sendClip(parts[1].replacingOccurrences(of: "\\n", with: "\n"), to: parts[0]) { [weak overlay] in Self.devSaid($0, overlay) } }
        case "savefile":
            for (id, t) in MessageFiles.shared.transfers where !t.mine { MessageFiles.shared.save(id) }
        // the pasteboard into the field with the keys (the bar's paste path), or, with an index, into system field i
        case "paste":
            if let i = Int(arg), let raw = NSPasteboard.general.string(forType: .string) {
                _ = overlay.model.pasteSystem(raw, keeping: Array((overlay.model.sysLines ?? []).prefix(i)))
            } else {
                let window = NSApp.windows.first { $0 is OverlayPanel && $0.isVisible }
                _ = window?.firstResponder?.tryToPerform(#selector(NSText.paste(_:)), with: nil)
            }
        default: NSLog("Q Calc dev: unknown command %@", command)
        }
        if verb == "card" || verb == "ask" { overlay.model.scheduleCardExpiry() }
    }

    // no opens yet means this is the first run: wait a while in case it gets closed and forgotten.
    // otherwise once launch has settled
    private func scheduleHotKeyNudge() {
        guard AppSettings.shared.wantsHotKeyNudge else { return }
        let delay: TimeInterval = (AppSettings.shared.onboarding["opens"] ?? 0) == 0 ? 90 : 8
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in self?.nudgeHotKey() }
    }

    // never takes focus, and leaves by itself, on a click, or when the overlay shows
    private func nudgeHotKey() {
        guard AppSettings.shared.wantsHotKeyNudge, overlay?.isShown != true, nudge == nil,
              let hotKey = AppSettings.shared.activeHotKey, let button = statusItem?.button else { return }
        AppSettings.shared.noteHotKeyNudge()
        let popover = NSPopover()
        popover.behavior = .applicationDefined
        popover.contentViewController = NSHostingController(rootView: HotKeyNudgeView(hotKey: hotKey.title) { [weak self] in
            self?.closeNudge()
        })
        popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
        nudge = popover
        nudgeObserver = NotificationCenter.default.addObserver(forName: .focusOverlay, object: nil, queue: .main) { [weak self] _ in
            self?.closeNudge()
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 15) { [weak self, weak popover] in
            guard let self, let popover, self.nudge === popover else { return }
            self.closeNudge()
        }
    }

    private func closeNudge() {
        nudge?.close()
        nudge = nil
        if let nudgeObserver { NotificationCenter.default.removeObserver(nudgeObserver) }
        nudgeObserver = nil
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        overlay?.toggle(from: "reopen")
        return false
    }

    func toggleOverlay() {
        overlay?.toggle()
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        buildStatusMenu(menu)
    }

    private func statusBarImage() -> NSImage {
        if let url = Bundle.main.url(forResource: "StatusIcon", withExtension: "png"),
           let image = NSImage(contentsOf: url) {
            image.size = NSSize(width: 18, height: 18)
            image.isTemplate = true
            return image
        }
        let fallback = NSImage(systemSymbolName: "sum", accessibilityDescription: "Q Calc")
        fallback?.isTemplate = true
        return fallback ?? NSImage()
    }

    private func hotKeyDiary() -> String {
        let settings = AppSettings.shared
        guard settings.hotKeyFailed else { return "shortcut \(settings.activeHotKey?.title ?? "none")" }
        return "shortcut \(settings.hotKey.title) taken, using \(settings.activeHotKey?.title ?? "none")"
    }

    // with no working shortcut the menu bar icon is the only way in, so it carries a dot and says why
    private func refreshStatusItem() {
        guard let button = statusItem?.button else { return }
        let settings = AppSettings.shared
        let none = settings.activeHotKey == nil
        button.image = none ? badged(statusBarImage()) : statusBarImage()
        if none {
            button.toolTip = "Q Calc: \(settings.hotKey.title) is taken, so no shortcut opens it. Pick another in Settings"
        } else if settings.hotKeyFailed, let active = settings.activeHotKey {
            button.toolTip = "Q Calc: \(active.title) (\(settings.hotKey.title) is taken)"
        } else {
            button.toolTip = "Q Calc" + (settings.activeHotKey.map { ": \($0.title)" } ?? "")
        }
    }

    // a dot in the top right corner, cut out of the icon so it reads at menu bar size; still a template
    private func badged(_ base: NSImage) -> NSImage {
        let image = NSImage(size: base.size, flipped: false) { rect in
            base.draw(in: rect)
            let d: CGFloat = 7
            let dot = NSRect(x: rect.maxX - d, y: rect.maxY - d, width: d, height: d)
            NSGraphicsContext.current?.compositingOperation = .clear
            NSBezierPath(ovalIn: dot.insetBy(dx: -1.5, dy: -1.5)).fill()
            NSGraphicsContext.current?.compositingOperation = .sourceOver
            NSColor.black.setFill()
            NSBezierPath(ovalIn: dot).fill()
            return true
        }
        image.isTemplate = true
        return image
    }

    private func setupStatusItem() {
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        if let button = item.button {
            button.image = statusBarImage()
            button.imageScaling = .scaleProportionallyDown
            button.toolTip = "Q Calc"
        }
        let menu = NSMenu()
        menu.delegate = self
        item.menu = menu
        statusItem = item
    }

    private func buildStatusMenu(_ menu: NSMenu) {
        let active = AppSettings.shared.activeHotKey
        if AppSettings.shared.hotKeyFailed {
            let taken = AppSettings.shared.hotKey.title
            let title = active.map { "\(taken) is taken, so \($0.title) opens Q Calc…" }
                ?? "\(taken) is taken, so no shortcut opens Q Calc…"
            let warning = NSMenuItem(title: title, action: #selector(showSettings), keyEquivalent: "")
            warning.target = self
            warning.image = NSImage(systemSymbolName: "exclamationmark.triangle", accessibilityDescription: nil)
            menu.addItem(warning)
            menu.addItem(.separator())
        }
        let quick = NSMenuItem(title: "Show Q Calc", action: #selector(showQuickCalc), keyEquivalent: active?.menuKey ?? "")
        quick.keyEquivalentModifierMask = active?.menuModifiers ?? []
        quick.target = self
        menu.addItem(quick)
        // the key the user chose for settings, shown beside the item
        let settingsKey = GlobalHotKey(KeyActions.chord("settings", in: AppSettings.shared.keybinds))
        let settings = NSMenuItem(title: "Settings…", action: #selector(showSettings), keyEquivalent: settingsKey?.menuKey ?? "")
        settings.keyEquivalentModifierMask = settingsKey?.menuModifiers ?? []
        settings.target = self
        menu.addItem(settings)
        let tips = NSMenuItem(title: "Tips…", action: #selector(showTips), keyEquivalent: "")
        tips.target = self
        menu.addItem(tips)
        let tutorial = NSMenuItem(title: "Tutorial", action: #selector(showTutorial), keyEquivalent: "")
        tutorial.target = self
        menu.addItem(tutorial)
        if Updates.shared.enabled {
            let updates = NSMenuItem(title: "Check for Updates…", action: #selector(Updates.checkForUpdates), keyEquivalent: "")
            updates.target = Updates.shared
            menu.addItem(updates)
        }
        menu.addItem(.separator())
        let quit = NSMenuItem(title: "Quit Q Calc", action: #selector(quitApp), keyEquivalent: "q")
        quit.target = self
        menu.addItem(quit)
    }

    @objc private func showQuickCalc() {
        overlay?.toggle(from: "menu")
    }

    @objc private func showTips() {
        overlay?.showTips()
    }

    @objc func showTutorial() {
        overlay?.showTutorial()
    }

    // false when macos or carbon refused the preset; the previous one stays bound
    @discardableResult
    func selectHotKey(_ next: GlobalHotKey) -> Bool {
        let previous = AppSettings.shared.activeHotKey
        if next == previous {
            if hotKeyRef == nil { _ = bindHotKey(next) }
            AppSettings.shared.setHotKey(next)
            AppSettings.shared.setHotKeyState(active: next, failed: false)
            return true
        }
        if SystemShortcuts.claims(next) { return false }
        unbindHotKey()
        if bindHotKey(next) {
            AppSettings.shared.setHotKey(next)
            AppSettings.shared.setHotKeyState(active: next, failed: false)
            return true
        }
        if let previous, !bindHotKey(previous) {
            AppSettings.shared.setHotKeyState(active: nil, failed: true)
        }
        return false
    }

    // while the settings window records a new show / hide key, the old one mustn't swallow the press
    func pauseHotKey(_ paused: Bool) {
        if paused {
            unbindHotKey()
        } else if hotKeyRef == nil, let active = AppSettings.shared.activeHotKey {
            _ = bindHotKey(active)
        }
    }

    @objc func showSettings() {
        overlay?.hide(because: "settings")
        let window = settingsWindow ?? SettingsWindowController()
        settingsWindow = window
        window.show()
    }

    @objc private func quitApp() {
        NSApp.terminate(nil)
    }

    private static let hotKeyID = EventHotKeyID(signature: hotKeySignature, id: 1)

    // at launch: the picked shortcut, else the default, else none, and the overlay's hint line says so
    private func registerHotKey() {
        installHotKeyHandler()
        let preferred = AppSettings.shared.hotKey
        if !SystemShortcuts.claims(preferred), bindHotKey(preferred) {
            AppSettings.shared.setHotKeyState(active: preferred, failed: false)
            return
        }
        NSLog("Q Calc: %@ is in use by macOS", preferred.title)
        OverlayLog.note("\(preferred.title) is taken")
        let fallback = GlobalHotKey.standard
        if preferred != fallback, !SystemShortcuts.claims(fallback), bindHotKey(fallback) {
            AppSettings.shared.setHotKeyState(active: fallback, failed: true)
            return
        }
        AppSettings.shared.setHotKeyState(active: nil, failed: true)
    }

    private func bindHotKey(_ preset: GlobalHotKey) -> Bool {
        var ref: EventHotKeyRef?
        let status = RegisterEventHotKey(
            preset.keyCode,
            preset.carbonModifiers,
            Self.hotKeyID,
            GetEventDispatcherTarget(),
            0,
            &ref
        )
        guard status == noErr, let ref else {
            NSLog("Q Calc: failed to register %@ (%d)", preset.title, status)
        OverlayLog.note("couldn't register \(preset.title) (\(status))")
            return false
        }
        hotKeyRef = ref
        return true
    }

    private func unbindHotKey() {
        guard let hotKeyRef else { return }
        UnregisterEventHotKey(hotKeyRef)
        self.hotKeyRef = nil
    }

    private func installHotKeyHandler() {
        guard !hotKeyHandlerInstalled else { return }
        hotKeyHandlerInstalled = true
        HotKeyBox.shared.onPress = { [weak self] in
            DispatchQueue.main.async { self?.toggleOverlay() }
        }
        var eventType = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        InstallEventHandler(
            GetEventDispatcherTarget(),
            qcalcHotKeyHandler,
            1,
            &eventType,
            nil,
            nil
        )
    }
}

private let hotKeySignature = OSType(0x51434C43) // "QCLC"

private struct HotKeyNudgeView: View {
    let hotKey: String
    let onDismiss: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Q Calc lives up here").font(.headline)
            Text("Press \(hotKey) in any app to open it.")
            Text("Type math, press Return.").foregroundStyle(.secondary)
        }
        .padding(12)
        .fixedSize()
        .contentShape(Rectangle())
        .onTapGesture(perform: onDismiss)
    }
}

final class HotKeyBox {
    static let shared = HotKeyBox()
    var onPress: (() -> Void)?
}

func qcalcHotKeyHandler(
    _ nextHandler: EventHandlerCallRef?,
    _ event: EventRef?,
    _ userData: UnsafeMutableRawPointer?
) -> OSStatus {
    var id = EventHotKeyID()
    GetEventParameter(
        event,
        EventParamName(kEventParamDirectObject),
        EventParamType(typeEventHotKeyID),
        nil,
        MemoryLayout<EventHotKeyID>.size,
        nil,
        &id
    )
    if id.signature == hotKeySignature {
        HotKeyBox.shared.onPress?()
    }
    return noErr
}
