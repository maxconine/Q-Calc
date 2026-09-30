import AppKit
import Carbon
import SwiftUI

struct GeneralSettingsView: View {
    @ObservedObject var settings: AppSettings
    @ObservedObject private var updates = Updates.shared
    @State private var choosingKeys = false

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
                LabeledContent {
                    Button("Choose Keybinds…") { choosingKeys = true }
                } label: {
                    Text("Keybinds")
                    if settings.hotKeyFailed {
                        Text("Your show / hide shortcut is in use by macOS")
                    } else {
                        Text("Show / hide with \((settings.activeHotKey ?? settings.hotKey).title)")
                    }
                }
                .sheet(isPresented: $choosingKeys) {
                    KeybindsSheet(settings: settings)
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
                toggle("Share anonymous usage", "Counts of which features get used, never what you type", \.shareUsage, AppSettings.setShareUsage(_:notifyWeb:))
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
                    if !keyTitle("angle").isEmpty {
                        Text("Switch with \(keyTitle("angle"))")
                    }
                }
                Picker("Answer form", selection: bind(\.answerForm, AppSettings.setAnswerForm(_:notifyWeb:))) {
                    Text("Exact").tag("exact")
                    Text("Approximate").tag("approx")
                }
                toggle("Fractions", withKey("Show answers as fractions", "fraction"), \.fractionMode, AppSettings.setFractionMode(_:notifyWeb:))
                toggle("Rationalize denominators", "5/√41 becomes 5√41/41", \.rationalize, AppSettings.setRationalize(_:notifyWeb:))
                toggle("Keep typed words as text", "sqrt stays sqrt, not √", \.keepWords, AppSettings.setKeepWords(_:notifyWeb:))
                toggle("Typst preview", "Show math like fractions, powers and integrals typeset under the bar", \.typstPreview, AppSettings.setTypstPreview(_:notifyWeb:))
                toggle("Copy Typst compatible", "Equations copied from the bar paste as Typst math", \.typstCopy, AppSettings.setTypstCopy(_:notifyWeb:))
            }

            Section("Significant figures") {
                Picker("Digits shown", selection: bind(\.significantFigures, AppSettings.setSignificantFigures(_:notifyWeb:))) {
                    ForEach(AppSettings.minSigFigs...AppSettings.maxSigFigs, id: \.self) { n in
                        Text("\(n)").tag(n)
                    }
                }
                toggle("Propagate from input", withKey("Match the precision you typed", "sigFigs"), \.sigFigMode, AppSettings.setSigFigMode(_:notifyWeb:))
            }
        }
        .formStyle(.grouped)
    }

    private var draftChoices: [(title: String, seconds: Int)] {
        let current = settings.draftSeconds
        if Self.draftChoices.contains(where: { $0.seconds == current }) { return Self.draftChoices }
        return Self.draftChoices + [("\(current) seconds", current)]
    }

    private func keyTitle(_ action: String) -> String {
        KeyActions.title(KeyActions.chord(action, in: settings.keybinds))
    }

    private func withKey(_ detail: String, _ action: String) -> String {
        let key = keyTitle(action)
        return key.isEmpty ? detail : "\(detail) · \(key)"
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

// a row per action; clicking a row's key listens for the next chord pressed. same rules as the page's KeybindSettings
struct KeybindsSheet: View {
    @ObservedObject var settings: AppSettings
    @Environment(\.dismiss) private var dismiss
    @State private var recording: String?
    @State private var note = ""
    @State private var monitor: Any?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Keybinds").font(.headline)
            Text(hint)
                .font(.callout)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            Form {
                ForEach(KeyActions.all, id: \.id) { action in
                    LabeledContent(action.label) {
                        HStack(spacing: 6) {
                            if chord(action.id) != KeyActions.defaults[action.id] {
                                Button {
                                    pick(action.id, KeyActions.defaults[action.id] ?? "")
                                } label: {
                                    Image(systemName: "arrow.uturn.backward")
                                }
                                .buttonStyle(.borderless)
                                .help("Back to \(KeyActions.title(KeyActions.defaults[action.id] ?? ""))")
                            }
                            Button {
                                toggleRecording(action.id)
                            } label: {
                                Text(buttonTitle(action.id))
                                    .frame(minWidth: 96)
                                    .foregroundStyle(recording == action.id ? Color.accentColor : .primary)
                            }
                        }
                    }
                }
            }
            .formStyle(.grouped)
            HStack {
                Button("Reset All", action: resetAll).disabled(!changed)
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(width: 460)
        .onDisappear { stopRecording() }
    }

    private var hint: String {
        if !note.isEmpty { return note }
        if recording != nil { return "Press the new keys. Esc cancels, ⌫ leaves it without a key." }
        return "Click a shortcut, then press the keys you want."
    }

    private var showChord: String { (settings.activeHotKey ?? settings.hotKey).id }

    // the global shortcut counts for conflicts too
    private var allBinds: [String: String] {
        var binds = settings.keybinds
        binds["show"] = showChord
        return binds
    }

    private var changed: Bool { !settings.keybinds.isEmpty || showChord != GlobalHotKey.standard.id }

    private func chord(_ action: String) -> String {
        action == "show" ? showChord : KeyActions.chord(action, in: settings.keybinds)
    }

    private func buttonTitle(_ action: String) -> String {
        if recording == action { return "Press keys…" }
        let title = KeyActions.title(chord(action))
        return title.isEmpty ? "None" : title
    }

    private func toggleRecording(_ action: String) {
        let again = recording == action
        stopRecording()
        note = ""
        guard !again else { return }
        recording = action
        if action == "show" { QCalc.delegate.pauseHotKey(true) }
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            handle(event)
        }
    }

    private func stopRecording() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
        if recording == "show" { QCalc.delegate.pauseHotKey(false) }
        recording = nil
    }

    private func handle(_ event: NSEvent) -> NSEvent? {
        guard let action = recording else { return event }
        let flags = event.modifierFlags.intersection([.command, .control, .option, .shift])
        if flags.isEmpty && event.keyCode == UInt16(kVK_Escape) {
            stopRecording()
            note = ""
            return nil
        }
        if flags.isEmpty && (event.keyCode == UInt16(kVK_Delete) || event.keyCode == UInt16(kVK_ForwardDelete)) {
            if action == "show" {
                note = "Show / hide needs a key"
            } else {
                pick(action, "")
            }
            return nil
        }
        guard let chord = Chord(event: event) else { return nil }
        if let why = KeyActions.problem(chord, for: action, binds: allBinds) {
            note = why
            return nil
        }
        pick(action, chord.text)
        return nil
    }

    private func pick(_ action: String, _ chord: String) {
        stopRecording()
        note = ""
        if action == "show" {
            guard let next = GlobalHotKey(chord) else { return }
            if !QCalc.delegate.selectHotKey(next) { note = "\(next.title) is in use by macOS" }
            return
        }
        var binds = settings.keybinds
        if chord == KeyActions.defaults[action] {
            binds.removeValue(forKey: action)
        } else {
            binds[action] = chord
        }
        settings.setKeybinds(binds, notifyWeb: true)
    }

    private func resetAll() {
        stopRecording()
        note = ""
        settings.setKeybinds([:], notifyWeb: true)
        if showChord != GlobalHotKey.standard.id, !QCalc.delegate.selectHotKey(.standard) {
            note = "\(GlobalHotKey.standard.title) is in use by macOS"
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
