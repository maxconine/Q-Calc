import AppKit
import Carbon
import Combine

extension Notification.Name {
    static let qcalcSettingsChanged = Notification.Name("QCalc.settingsChanged")
}

/// Global shortcut choices. Presets only: each is Space plus modifiers.
struct HotKeyPreset: Equatable {
    let id: String
    let title: String
    let carbonModifiers: UInt32
    let menuModifiers: NSEvent.ModifierFlags

    static let all: [HotKeyPreset] = [
        HotKeyPreset(id: "ctrl-opt-space", title: "⌃⌥Space", carbonModifiers: UInt32(controlKey | optionKey), menuModifiers: [.control, .option]),
        HotKeyPreset(id: "cmd-opt-space", title: "⌘⌥Space", carbonModifiers: UInt32(cmdKey | optionKey), menuModifiers: [.command, .option]),
        HotKeyPreset(id: "ctrl-space", title: "⌃Space", carbonModifiers: UInt32(controlKey), menuModifiers: [.control]),
        HotKeyPreset(id: "opt-space", title: "⌥Space", carbonModifiers: UInt32(optionKey), menuModifiers: [.option]),
    ]
    static let standard = all[0]

    static func named(_ id: String?) -> HotKeyPreset {
        all.first { $0.id == id } ?? standard
    }
}

/// macOS keeps its own Space shortcuts (Spotlight, Finder search, input sources) in com.apple.symbolichotkeys.
/// Carbon happily registers a combo the system already owns, and the system then wins — so ask first.
enum SystemShortcuts {
    private static let domain = "com.apple.symbolichotkeys" as CFString
    private static let modifierMask = NSEvent.ModifierFlags([.shift, .control, .option, .command]).rawValue
    /// Space shortcuts macOS ships enabled, by symbolic id: 60/61 input sources, 64 Spotlight, 65 Finder search.
    private static let spaceDefaults: [Int: NSEvent.ModifierFlags] = [
        60: [.control],
        61: [.control, .option],
        64: [.command],
        65: [.command, .option],
    ]
    private static let inputSourceIDs: Set<Int> = [60, 61]

    static func claims(_ preset: HotKeyPreset) -> Bool {
        CFPreferencesAppSynchronize(domain)
        let table = CFPreferencesCopyAppValue("AppleSymbolicHotKeys" as CFString, domain) as? [String: Any] ?? [:]
        let want = preset.menuModifiers.rawValue & modifierMask
        var owners: [Int] = []
        for (key, raw) in table {
            guard let id = Int(key), let entry = raw as? [String: Any],
                  (entry["enabled"] as? Bool) == true,
                  let value = entry["value"] as? [String: Any],
                  let params = value["parameters"] as? [Int], params.count >= 3,
                  params[1] == kVK_Space,
                  UInt(params[2]) & modifierMask == want
            else { continue }
            owners.append(id)
        }
        for (id, flags) in spaceDefaults where table[String(id)] == nil && flags.rawValue == want {
            owners.append(id)
        }
        // The input-source shortcuts only act (and only swallow the key) when there is more than one source.
        return owners.contains { !inputSourceIDs.contains($0) || switchesInputSources() }
    }

    private static func switchesInputSources() -> Bool {
        let filter = [
            kTISPropertyInputSourceCategory as String: kTISCategoryKeyboardInputSource as String,
            kTISPropertyInputSourceIsSelectCapable as String: true,
        ] as CFDictionary
        guard let list = TISCreateInputSourceList(filter, false)?.takeRetainedValue() as? [TISInputSource] else {
            return false
        }
        return list.count > 1
    }
}

final class AppSettings: ObservableObject {
    static let shared = AppSettings()
    static let sigFigsKey = "qcalc.sigFigs"
    static let draftSecondsKey = "qcalc.draftSeconds"
    static let defaultUnitsKey = "qcalc.defaultUnits"
    static let answerFormKey = "qcalc.answerForm"
    static let historyInsertKey = "qcalc.historyInsert"
    static let rationalizeKey = "qcalc.rationalize"
    static let sigFigModeKey = "qcalc.sigFigMode"
    static let themeKey = "qcalc.theme"
    static let hotKeyKey = "qcalc.hotkey"
    static let onboardingKey = "qcalc.onboarding"
    static let firstRunKey = "qcalc.firstRunDone"
    static let defaultSigFigs = 12
    static let minSigFigs = 2
    static let maxSigFigs = 16
    static let defaultDraftSeconds = 60
    static let minDraftSeconds = 0
    static let maxDraftSeconds = 3600
    static let defaultAnswerForm = "exact"
    static let defaultHistoryInsert = "expr"
    static let defaultRationalize = true
    static let defaultTheme = "system"

    @Published private(set) var significantFigures: Int
    @Published private(set) var draftSeconds: Int
    @Published private(set) var defaultUnits: [String: String]
    @Published private(set) var answerForm: String
    @Published private(set) var historyInsert: String
    @Published private(set) var rationalize: Bool
    @Published private(set) var sigFigMode: Bool
    @Published private(set) var theme: String
    /// The shortcut the user picked (persisted).
    @Published private(set) var hotKey: HotKeyPreset
    /// The shortcut actually registered right now; nil when none could be.
    @Published private(set) var activeHotKey: HotKeyPreset?
    /// The picked shortcut could not be registered at launch.
    @Published private(set) var hotKeyFailed = false
    /// Web onboarding progress (opens, commits, hints, done); the web view's own storage does not persist.
    private(set) var onboarding: [String: Int]

    private init() {
        let storedFigs = UserDefaults.standard.integer(forKey: Self.sigFigsKey)
        significantFigures = Self.clampSigFigs(storedFigs == 0 ? Self.defaultSigFigs : storedFigs)
        if UserDefaults.standard.object(forKey: Self.draftSecondsKey) == nil {
            draftSeconds = Self.defaultDraftSeconds
        } else {
            draftSeconds = Self.clampDraftSeconds(UserDefaults.standard.integer(forKey: Self.draftSecondsKey))
        }
        defaultUnits = Self.loadDefaultUnits()
        answerForm = Self.loadAnswerForm()
        historyInsert = Self.loadHistoryInsert()
        rationalize = Self.loadRationalize()
        sigFigMode = UserDefaults.standard.bool(forKey: Self.sigFigModeKey)
        theme = Self.loadTheme()
        hotKey = HotKeyPreset.named(UserDefaults.standard.string(forKey: Self.hotKeyKey))
        onboarding = Self.loadOnboarding()
    }

    private static let onboardingFields = ["opens", "commits", "hints", "done"]

    private static func loadOnboarding() -> [String: Int] {
        guard let stored = UserDefaults.standard.dictionary(forKey: onboardingKey) else { return [:] }
        var out: [String: Int] = [:]
        for key in onboardingFields {
            if let n = stored[key] as? Int, n > 0 { out[key] = n }
        }
        return out
    }

    /// Progress only moves forward, whichever side reports it.
    func mergeOnboarding(_ incoming: [String: Int]) {
        var next = onboarding
        for key in Self.onboardingFields {
            guard let n = incoming[key], n > 0 else { continue }
            next[key] = key == "hints" ? (next[key] ?? 0) | n : max(next[key] ?? 0, n)
        }
        guard next != onboarding else { return }
        onboarding = next
        UserDefaults.standard.set(next, forKey: Self.onboardingKey)
    }

    func onboardingJSON() -> String {
        let data = (try? JSONSerialization.data(withJSONObject: onboarding, options: [])) ?? Data("{}".utf8)
        return String(data: data, encoding: .utf8) ?? "{}"
    }

    /// True exactly once per install.
    func claimFirstRun() -> Bool {
        guard !UserDefaults.standard.bool(forKey: Self.firstRunKey) else { return false }
        UserDefaults.standard.set(true, forKey: Self.firstRunKey)
        return true
    }

    func setHotKey(_ preset: HotKeyPreset) {
        hotKey = preset
        UserDefaults.standard.set(preset.id, forKey: Self.hotKeyKey)
    }

    func setHotKeyState(active: HotKeyPreset?, failed: Bool) {
        guard active != activeHotKey || failed != hotKeyFailed else { return }
        activeHotKey = active
        hotKeyFailed = failed
        NotificationCenter.default.post(name: .qcalcSettingsChanged, object: nil)
    }

    private static func loadAnswerForm() -> String {
        let stored = UserDefaults.standard.string(forKey: answerFormKey) ?? defaultAnswerForm
        return stored == "approx" ? "approx" : defaultAnswerForm
    }

    private static func loadHistoryInsert() -> String {
        let stored = UserDefaults.standard.string(forKey: historyInsertKey) ?? defaultHistoryInsert
        return stored == "answer" ? "answer" : defaultHistoryInsert
    }

    private static func loadRationalize() -> Bool {
        if UserDefaults.standard.object(forKey: rationalizeKey) == nil {
            return defaultRationalize
        }
        return UserDefaults.standard.bool(forKey: rationalizeKey)
    }

    static func normalizeTheme(_ raw: String) -> String {
        raw == "light" || raw == "dark" ? raw : defaultTheme
    }

    private static func loadTheme() -> String {
        normalizeTheme(UserDefaults.standard.string(forKey: themeKey) ?? defaultTheme)
    }

    private static func loadDefaultUnits() -> [String: String] {
        guard let stored = UserDefaults.standard.dictionary(forKey: defaultUnitsKey) else { return [:] }
        var units: [String: String] = [:]
        for (dim, value) in stored {
            guard let id = value as? String, !id.isEmpty else { continue }
            units[dim] = id
        }
        return units
    }

    static func clampSigFigs(_ n: Int) -> Int {
        min(maxSigFigs, max(minSigFigs, n))
    }

    static func clampDraftSeconds(_ n: Int) -> Int {
        min(maxDraftSeconds, max(minDraftSeconds, n))
    }

    func setSignificantFigures(_ n: Int, notifyWeb: Bool) {
        let value = Self.clampSigFigs(n)
        guard value != significantFigures else { return }
        significantFigures = value
        UserDefaults.standard.set(value, forKey: Self.sigFigsKey)
        if notifyWeb {
            NotificationCenter.default.post(name: .qcalcSettingsChanged, object: nil)
        }
    }

    func setDraftSeconds(_ n: Int, notifyWeb: Bool) {
        let value = Self.clampDraftSeconds(n)
        guard value != draftSeconds else { return }
        draftSeconds = value
        UserDefaults.standard.set(value, forKey: Self.draftSecondsKey)
        if notifyWeb {
            NotificationCenter.default.post(name: .qcalcSettingsChanged, object: nil)
        }
    }

    func setAnswerForm(_ form: String, notifyWeb: Bool) {
        let value = form == "approx" ? "approx" : Self.defaultAnswerForm
        guard value != answerForm else { return }
        answerForm = value
        UserDefaults.standard.set(value, forKey: Self.answerFormKey)
        if notifyWeb {
            NotificationCenter.default.post(name: .qcalcSettingsChanged, object: nil)
        }
    }

    func setHistoryInsert(_ raw: String, notifyWeb: Bool) {
        let value = raw == "answer" ? "answer" : Self.defaultHistoryInsert
        guard value != historyInsert else { return }
        historyInsert = value
        UserDefaults.standard.set(value, forKey: Self.historyInsertKey)
        if notifyWeb {
            NotificationCenter.default.post(name: .qcalcSettingsChanged, object: nil)
        }
    }

    func setRationalize(_ value: Bool, notifyWeb: Bool) {
        guard value != rationalize else { return }
        rationalize = value
        UserDefaults.standard.set(value, forKey: Self.rationalizeKey)
        if notifyWeb {
            NotificationCenter.default.post(name: .qcalcSettingsChanged, object: nil)
        }
    }

    func setSigFigMode(_ value: Bool, notifyWeb: Bool) {
        guard value != sigFigMode else { return }
        sigFigMode = value
        UserDefaults.standard.set(value, forKey: Self.sigFigModeKey)
        if notifyWeb {
            NotificationCenter.default.post(name: .qcalcSettingsChanged, object: nil)
        }
    }

    func setTheme(_ raw: String, notifyWeb: Bool) {
        let value = Self.normalizeTheme(raw)
        guard value != theme else { return }
        theme = value
        UserDefaults.standard.set(value, forKey: Self.themeKey)
        if notifyWeb {
            NotificationCenter.default.post(name: .qcalcSettingsChanged, object: nil)
        }
    }

    /// Overlay and settings windows only — not `NSApp`, so the menu-bar icon can stay a template.
    var nsAppearance: NSAppearance? {
        switch theme {
        case "light":
            return NSAppearance(named: .aqua)
        case "dark":
            return NSAppearance(named: .darkAqua)
        default:
            return nil
        }
    }

    func setDefaultUnit(dim: String, unitId: String, notifyWeb: Bool) {
        var next = defaultUnits
        let id = unitId.trimmingCharacters(in: .whitespacesAndNewlines)
        if id.isEmpty {
            next.removeValue(forKey: dim)
        } else {
            next[dim] = id
        }
        applyDefaultUnits(next, notifyWeb: notifyWeb)
    }

    func resetDefaultUnits(notifyWeb: Bool) {
        applyDefaultUnits([:], notifyWeb: notifyWeb)
    }

    private func applyDefaultUnits(_ next: [String: String], notifyWeb: Bool) {
        guard next != defaultUnits else { return }
        defaultUnits = next
        UserDefaults.standard.set(next, forKey: Self.defaultUnitsKey)
        if notifyWeb {
            NotificationCenter.default.post(name: .qcalcSettingsChanged, object: nil)
        }
    }

    func defaultUnitsJSON() -> String {
        let data = (try? JSONSerialization.data(withJSONObject: defaultUnits, options: [])) ?? Data("{}".utf8)
        return String(data: data, encoding: .utf8) ?? "{}"
    }
}

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
    private var hotKeyRef: EventHotKeyRef?
    private var hotKeyHandlerInstalled = false
    /// A preset macOS refused; its menu title says so on the next menu build, then clears.
    private var refusedHotKey: String?
    private var statusItem: NSStatusItem?
    private var unitSettings: UnitSettingsWindowController?

    func applicationWillFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.prohibited)
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        setupStatusItem()
        // Before the web view boots, so its injected settings already carry the shortcut.
        registerHotKey()
        overlay = OverlayController()
        overlay?.preload()
        if AppSettings.shared.claimFirstRun() {
            overlay?.showFirstRun()
        }
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        overlay?.toggle()
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
        let quick = NSMenuItem(title: "Show Q Calc", action: #selector(showQuickCalc), keyEquivalent: active == nil ? "" : " ")
        quick.keyEquivalentModifierMask = active?.menuModifiers ?? []
        quick.target = self
        menu.addItem(quick)
        let tips = NSMenuItem(title: "Tips…", action: #selector(showTips), keyEquivalent: "")
        tips.target = self
        menu.addItem(tips)
        menu.addItem(.separator())

        let shortcut = NSMenuItem(title: "Shortcut", action: nil, keyEquivalent: "")
        shortcut.submenu = hotKeyMenu()
        menu.addItem(shortcut)

        let figs = NSMenuItem(title: "Significant figures", action: nil, keyEquivalent: "")
        figs.submenu = sigFigsMenu()
        menu.addItem(figs)

        let answers = NSMenuItem(title: "Answers", action: nil, keyEquivalent: "")
        answers.submenu = answerFormMenu()
        menu.addItem(answers)

        let history = NSMenuItem(title: "History", action: nil, keyEquivalent: "")
        history.submenu = historyInsertMenu()
        menu.addItem(history)

        let appearance = NSMenuItem(title: "Appearance", action: nil, keyEquivalent: "")
        appearance.submenu = appearanceMenu()
        menu.addItem(appearance)

        let draft = NSMenuItem(title: "Keep unfinished", action: nil, keyEquivalent: "")
        draft.submenu = draftMenu()
        menu.addItem(draft)

        let units = NSMenuItem(title: "Default units…", action: #selector(showUnitSettings), keyEquivalent: "")
        units.target = self
        menu.addItem(units)

        menu.addItem(.separator())
        let quit = NSMenuItem(title: "Quit Q Calc", action: #selector(quitApp), keyEquivalent: "q")
        quit.target = self
        menu.addItem(quit)
    }

    private func hotKeyMenu() -> NSMenu {
        let menu = NSMenu()
        let current = AppSettings.shared.activeHotKey
        let refused = refusedHotKey
        refusedHotKey = nil
        for preset in HotKeyPreset.all {
            let title = preset.id == refused ? "\(preset.title) — in use by macOS" : preset.title
            let item = NSMenuItem(title: title, action: #selector(setHotKey(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = preset.id
            item.state = preset == current ? .on : .off
            menu.addItem(item)
        }
        return menu
    }

    private func sigFigsMenu() -> NSMenu {
        let menu = NSMenu()
        let current = AppSettings.shared.significantFigures
        for n in AppSettings.minSigFigs...AppSettings.maxSigFigs {
            let item = NSMenuItem(title: "\(n)", action: #selector(setSigFigs(_:)), keyEquivalent: "")
            item.target = self
            item.tag = n
            item.state = n == current ? .on : .off
            menu.addItem(item)
        }
        menu.addItem(.separator())
        let propagate = NSMenuItem(title: "Propagate from input", action: #selector(toggleSigFigMode), keyEquivalent: "")
        propagate.target = self
        propagate.state = AppSettings.shared.sigFigMode ? .on : .off
        menu.addItem(propagate)
        return menu
    }

    private func answerFormMenu() -> NSMenu {
        let menu = NSMenu()
        let current = AppSettings.shared.answerForm
        let options: [(String, String)] = [
            ("Exact", "exact"),
            ("Approximate", "approx"),
        ]
        for (title, form) in options {
            let item = NSMenuItem(title: title, action: #selector(setAnswerForm(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = form
            item.state = form == current ? .on : .off
            menu.addItem(item)
        }
        menu.addItem(.separator())
        let rat = NSMenuItem(title: "Rationalize denominators", action: #selector(toggleRationalize), keyEquivalent: "")
        rat.target = self
        rat.state = AppSettings.shared.rationalize ? .on : .off
        menu.addItem(rat)
        return menu
    }

    private func historyInsertMenu() -> NSMenu {
        let menu = NSMenu()
        let current = AppSettings.shared.historyInsert
        let options: [(String, String)] = [
            ("Insert expression", "expr"),
            ("Insert answer", "answer"),
        ]
        for (title, value) in options {
            let item = NSMenuItem(title: title, action: #selector(setHistoryInsert(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = value
            item.state = value == current ? .on : .off
            menu.addItem(item)
        }
        return menu
    }

    private func appearanceMenu() -> NSMenu {
        let menu = NSMenu()
        let current = AppSettings.shared.theme
        let options: [(String, String)] = [
            ("System", "system"),
            ("Light", "light"),
            ("Dark", "dark"),
        ]
        for (title, theme) in options {
            let item = NSMenuItem(title: title, action: #selector(setTheme(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = theme
            item.state = theme == current ? .on : .off
            menu.addItem(item)
        }
        return menu
    }

    private func draftMenu() -> NSMenu {
        let menu = NSMenu()
        let current = AppSettings.shared.draftSeconds
        let options: [(String, Int)] = [
            ("Don't keep", 0),
            ("30 seconds", 30),
            ("1 minute", 60),
            ("2 minutes", 120),
            ("5 minutes", 300),
        ]
        for (title, seconds) in options {
            let item = NSMenuItem(title: title, action: #selector(setDraftSeconds(_:)), keyEquivalent: "")
            item.target = self
            item.tag = seconds
            item.state = seconds == current ? .on : .off
            menu.addItem(item)
        }
        return menu
    }

    @objc private func showQuickCalc() {
        overlay?.toggle()
    }

    @objc private func showTips() {
        overlay?.showTips()
    }

    @objc private func setHotKey(_ sender: NSMenuItem) {
        let next = HotKeyPreset.named(sender.representedObject as? String)
        let previous = AppSettings.shared.activeHotKey
        if next == previous {
            AppSettings.shared.setHotKey(next)
            AppSettings.shared.setHotKeyState(active: next, failed: false)
            return
        }
        refusedHotKey = next.id
        if SystemShortcuts.claims(next) { return }
        unbindHotKey()
        if bindHotKey(next) {
            refusedHotKey = nil
            AppSettings.shared.setHotKey(next)
            AppSettings.shared.setHotKeyState(active: next, failed: false)
            return
        }
        if let previous, !bindHotKey(previous) {
            AppSettings.shared.setHotKeyState(active: nil, failed: true)
        }
    }

    @objc private func setSigFigs(_ sender: NSMenuItem) {
        AppSettings.shared.setSignificantFigures(sender.tag, notifyWeb: true)
    }

    @objc private func setDraftSeconds(_ sender: NSMenuItem) {
        AppSettings.shared.setDraftSeconds(sender.tag, notifyWeb: true)
    }

    @objc private func setAnswerForm(_ sender: NSMenuItem) {
        let form = sender.representedObject as? String ?? AppSettings.defaultAnswerForm
        AppSettings.shared.setAnswerForm(form, notifyWeb: true)
    }

    @objc private func setHistoryInsert(_ sender: NSMenuItem) {
        let value = sender.representedObject as? String ?? AppSettings.defaultHistoryInsert
        AppSettings.shared.setHistoryInsert(value, notifyWeb: true)
    }

    @objc private func toggleRationalize() {
        AppSettings.shared.setRationalize(!AppSettings.shared.rationalize, notifyWeb: true)
    }

    @objc private func toggleSigFigMode() {
        AppSettings.shared.setSigFigMode(!AppSettings.shared.sigFigMode, notifyWeb: true)
    }

    @objc private func setTheme(_ sender: NSMenuItem) {
        let theme = sender.representedObject as? String ?? AppSettings.defaultTheme
        AppSettings.shared.setTheme(theme, notifyWeb: true)
    }

    @objc private func showUnitSettings() {
        if unitSettings == nil {
            unitSettings = UnitSettingsWindowController()
        }
        unitSettings?.show()
    }

    @objc private func quitApp() {
        NSApp.terminate(nil)
    }

    private static let hotKeyID = EventHotKeyID(signature: OSType(0x51434C43), id: 1) // QCLC

    /// Launch: the picked shortcut, else the default, else none — and the overlay's hint line says so.
    private func registerHotKey() {
        installHotKeyHandler()
        let preferred = AppSettings.shared.hotKey
        if !SystemShortcuts.claims(preferred), bindHotKey(preferred) {
            AppSettings.shared.setHotKeyState(active: preferred, failed: false)
            return
        }
        NSLog("Q Calc: %@ is in use by macOS", preferred.title)
        refusedHotKey = preferred.id
        let fallback = HotKeyPreset.standard
        if preferred != fallback, !SystemShortcuts.claims(fallback), bindHotKey(fallback) {
            AppSettings.shared.setHotKeyState(active: fallback, failed: true)
            return
        }
        AppSettings.shared.setHotKeyState(active: nil, failed: true)
    }

    private func bindHotKey(_ preset: HotKeyPreset) -> Bool {
        var ref: EventHotKeyRef?
        let status = RegisterEventHotKey(
            UInt32(kVK_Space),
            preset.carbonModifiers,
            Self.hotKeyID,
            GetEventDispatcherTarget(),
            0,
            &ref
        )
        guard status == noErr, let ref else {
            NSLog("Q Calc: failed to register %@ (%d)", preset.title, status)
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
    if id.signature == OSType(0x51434C43) {
        HotKeyBox.shared.onPress?()
    }
    return noErr
}
