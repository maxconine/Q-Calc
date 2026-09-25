import AppKit
import SwiftUI

struct GeneralSettingsView: View {
    @ObservedObject var settings: AppSettings
    @ObservedObject private var updates = Updates.shared
    // the preset macos just refused, so the row can say why nothing changed
    @State private var refusedHotKey: HotKeyPreset?

    private static let draftChoices: [(title: String, seconds: Int)] = [
        ("Don’t keep", 0),
        ("30 seconds", 30),
        ("1 minute", 60),
        ("2 minutes", 120),
        ("5 minutes", 300),
    ]

    var body: some View {
        Form {
            Section {
                Picker(selection: hotKeyBinding) {
                    ForEach(HotKeyPreset.all, id: \.id) { preset in
                        Text(preset.title).tag(preset.id)
                    }
                } label: {
                    Text("Keyboard shortcut")
                    if let refusedHotKey {
                        Text("\(refusedHotKey.title) is in use by macOS")
                    } else if settings.hotKeyFailed {
                        Text("Your shortcut is in use by macOS")
                    }
                }
                Picker("Appearance", selection: bind(\.theme, AppSettings.setTheme(_:notifyWeb:))) {
                    Text("System").tag("system")
                    Text("Light").tag("light")
                    Text("Dark").tag("dark")
                }
                Picker(selection: bind(\.historyShow, AppSettings.setHistoryShow(_:notifyWeb:))) {
                    Text("Recent calculations").tag("recent")
                    Text("Always").tag("always")
                    Text("Only on ↑").tag("arrow")
                } label: {
                    Text("Show history")
                    Text("Recent means the last 5 minutes")
                }
                Picker("Enter on a history row inserts", selection: bind(\.historyInsert, AppSettings.setHistoryInsert(_:notifyWeb:))) {
                    Text("Expression").tag("expr")
                    Text("Answer").tag("answer")
                }
                Picker("Keep unfinished input", selection: bind(\.draftSeconds, AppSettings.setDraftSeconds(_:notifyWeb:))) {
                    ForEach(draftChoices, id: \.seconds) { choice in
                        Text(choice.title).tag(choice.seconds)
                    }
                }
                if updates.enabled {
                    Toggle(isOn: $updates.automatic) {
                        Text("Update automatically")
                        Text("Installs new versions while you’re away")
                    }
                }
            }

            Section("Answers") {
                Picker(selection: bind(\.angleMode, AppSettings.setAngleMode(_:notifyWeb:))) {
                    Text("Degrees").tag("deg")
                    Text("Radians").tag("rad")
                } label: {
                    Text("Angles")
                    Text("Switch with ⌃D")
                }
                Picker("Answer form", selection: bind(\.answerForm, AppSettings.setAnswerForm(_:notifyWeb:))) {
                    Text("Exact").tag("exact")
                    Text("Approximate").tag("approx")
                }
                toggle("Fractions", "Show answers as fractions · ⌃F", \.fractionMode, AppSettings.setFractionMode(_:notifyWeb:))
                toggle("Rationalize denominators", "5/√41 becomes 5√41/41", \.rationalize, AppSettings.setRationalize(_:notifyWeb:))
                toggle("Keep typed words as text", "sqrt stays sqrt, not √", \.keepWords, AppSettings.setKeepWords(_:notifyWeb:))
                toggle("Typst preview", "Show the calculation typeset under the bar", \.typstPreview, AppSettings.setTypstPreview(_:notifyWeb:))
                toggle("Copy Typst compatible", "Equations copied from the bar paste as Typst math", \.typstCopy, AppSettings.setTypstCopy(_:notifyWeb:))
            }

            Section("Significant figures") {
                Picker("Digits shown", selection: bind(\.significantFigures, AppSettings.setSignificantFigures(_:notifyWeb:))) {
                    ForEach(AppSettings.minSigFigs...AppSettings.maxSigFigs, id: \.self) { n in
                        Text("\(n)").tag(n)
                    }
                }
                toggle("Propagate from input", "Match the precision you typed · ⌃S", \.sigFigMode, AppSettings.setSigFigMode(_:notifyWeb:))
            }
        }
        .formStyle(.grouped)
    }

    private var draftChoices: [(title: String, seconds: Int)] {
        let current = settings.draftSeconds
        if Self.draftChoices.contains(where: { $0.seconds == current }) { return Self.draftChoices }
        return Self.draftChoices + [("\(current) seconds", current)]
    }

    private var hotKeyBinding: Binding<String> {
        Binding(
            get: { (settings.activeHotKey ?? settings.hotKey).id },
            set: { id in
                let preset = HotKeyPreset.named(id)
                refusedHotKey = QCalc.delegate.selectHotKey(preset) ? nil : preset
            }
        )
    }

    // every row writes through AppSettings, which tells the web view; the web view's own changes land here through @Published
    private func bind<T>(_ value: KeyPath<AppSettings, T>, _ set: @escaping (AppSettings) -> (T, Bool) -> Void) -> Binding<T> {
        Binding(
            get: { settings[keyPath: value] },
            set: { set(settings)($0, true) }
        )
    }

    private func toggle(
        _ title: String,
        _ detail: String,
        _ value: KeyPath<AppSettings, Bool>,
        _ set: @escaping (AppSettings) -> (Bool, Bool) -> Void
    ) -> some View {
        Toggle(isOn: bind(value, set)) {
            Text(title)
            Text(detail)
        }
    }
}

final class SettingsWindowController: NSObject, NSWindowDelegate {
    private var window: NSWindow?
    private var settingsObserver: NSObjectProtocol?

    deinit {
        if let settingsObserver {
            NotificationCenter.default.removeObserver(settingsObserver)
        }
    }

    func show() {
        if window == nil {
            let window = NSWindow(contentViewController: makeTabs())
            window.styleMask = [.titled, .closable, .miniaturizable]
            window.toolbarStyle = .preference
            window.isReleasedWhenClosed = false
            window.delegate = self
            window.center()
            self.window = window
            observeSettings()
        }
        applyAppearance()
        NSApp.mainMenu = Self.mainMenu()
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
    }

    func windowWillClose(_ notification: Notification) {
        window = nil
        NSApp.mainMenu = nil
        NSApp.setActivationPolicy(.accessory)
    }

    private func makeTabs() -> NSTabViewController {
        let tabs = NSTabViewController()
        tabs.tabStyle = .toolbar
        tabs.addTabViewItem(tab("General", symbol: "gearshape", size: NSSize(width: 500, height: 600)) {
            GeneralSettingsView(settings: AppSettings.shared)
        })
        tabs.addTabViewItem(tab("Units", symbol: "ruler", size: NSSize(width: 500, height: 600)) {
            UnitSettingsView(settings: AppSettings.shared)
        })
        return tabs
    }

    private func tab<Content: View>(_ title: String, symbol: String, size: NSSize, content: () -> Content) -> NSTabViewItem {
        let hosting = NSHostingController(rootView: content().frame(width: size.width, height: size.height))
        hosting.sizingOptions = [.preferredContentSize]
        hosting.title = title
        let item = NSTabViewItem(viewController: hosting)
        item.label = title
        item.image = NSImage(systemSymbolName: symbol, accessibilityDescription: title)
        return item
    }

    // only while the window is open: the overlay has no menu bar, so ⌘Q there must stay a no-op
    private static func mainMenu() -> NSMenu {
        let main = NSMenu()
        let app = NSMenu(title: "Q Calc")
        app.addItem(withTitle: "Settings…", action: #selector(AppDelegate.showSettings), keyEquivalent: ",").target = QCalc.delegate
        app.addItem(.separator())
        app.addItem(withTitle: "Quit Q Calc", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        let window = NSMenu(title: "Window")
        window.addItem(withTitle: "Close", action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w")
        window.addItem(withTitle: "Minimize", action: #selector(NSWindow.performMiniaturize(_:)), keyEquivalent: "m")
        for submenu in [app, window] {
            let item = NSMenuItem()
            item.submenu = submenu
            main.addItem(item)
        }
        NSApp.windowsMenu = window
        return main
    }

    private func observeSettings() {
        guard settingsObserver == nil else { return }
        settingsObserver = NotificationCenter.default.addObserver(
            forName: .qcalcSettingsChanged,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            self?.applyAppearance()
        }
    }

    private func applyAppearance() {
        window?.appearance = AppSettings.shared.nsAppearance
    }
}
