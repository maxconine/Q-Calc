import AppKit
import Carbon
import Combine

extension Notification.Name {
    static let qcalcSettingsChanged = Notification.Name("QCalc.settingsChanged")
}

// presets only: each is space plus modifiers
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

// carbon happily registers a combo macos already owns (spotlight, finder search, input sources),
// and macos then wins, so check com.apple.symbolichotkeys first
enum SystemShortcuts {
    private static let domain = "com.apple.symbolichotkeys" as CFString
    private static let modifierMask = NSEvent.ModifierFlags([.shift, .control, .option, .command]).rawValue
    // space shortcuts macos ships enabled, by symbolic id: 60/61 input sources, 64 spotlight, 65 finder search
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
        // the input source shortcuts only swallow the key when there is more than one source
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
    static let historyShowKey = "qcalc.historyShow"
    static let rationalizeKey = "qcalc.rationalize"
    static let sigFigModeKey = "qcalc.sigFigMode"
    static let keepWordsKey = "qcalc.keepWords"
    static let themeKey = "qcalc.theme"
    static let angleModeKey = "qcalc.angleMode"
    static let fractionModeKey = "qcalc.fractionMode"
    static let typstPreviewKey = "qcalc.typstPreview"
    static let typstCopyKey = "qcalc.typstCopy"
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
    static let defaultHistoryShow = "recent"
    static let defaultRationalize = true
    static let defaultTheme = "system"

    @Published private(set) var significantFigures: Int
    @Published private(set) var draftSeconds: Int
    @Published private(set) var defaultUnits: [String: String]
    @Published private(set) var answerForm: String
    @Published private(set) var historyInsert: String
    @Published private(set) var historyShow: String
    @Published private(set) var rationalize: Bool
    @Published private(set) var sigFigMode: Bool
    @Published private(set) var keepWords: Bool
    @Published private(set) var theme: String
    @Published private(set) var angleMode: String
    @Published private(set) var fractionMode: Bool
    @Published private(set) var typstPreview: Bool
    @Published private(set) var typstCopy: Bool
    // what the user picked; activeHotKey is what actually got registered
    @Published private(set) var hotKey: HotKeyPreset
    @Published private(set) var activeHotKey: HotKeyPreset?
    @Published private(set) var hotKeyFailed = false
    // kept here because the web view's own storage does not persist
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
        historyShow = Self.normalizeHistoryShow(UserDefaults.standard.string(forKey: Self.historyShowKey) ?? Self.defaultHistoryShow)
        rationalize = Self.loadRationalize()
        sigFigMode = UserDefaults.standard.bool(forKey: Self.sigFigModeKey)
        keepWords = UserDefaults.standard.bool(forKey: Self.keepWordsKey)
        theme = Self.loadTheme()
        angleMode = UserDefaults.standard.string(forKey: Self.angleModeKey) == "rad" ? "rad" : "deg"
        fractionMode = UserDefaults.standard.bool(forKey: Self.fractionModeKey)
        typstPreview = UserDefaults.standard.bool(forKey: Self.typstPreviewKey)
        typstCopy = UserDefaults.standard.bool(forKey: Self.typstCopyKey)
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

    // progress only moves forward, whichever side reports it
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

    // true exactly once per install
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
        notifySettingsChanged()
    }

    private func notifySettingsChanged() {
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

    static func normalizeHistoryShow(_ raw: String) -> String {
        raw == "always" || raw == "arrow" ? raw : defaultHistoryShow
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
        if notifyWeb { notifySettingsChanged() }
    }

    func setDraftSeconds(_ n: Int, notifyWeb: Bool) {
        let value = Self.clampDraftSeconds(n)
        guard value != draftSeconds else { return }
        draftSeconds = value
        UserDefaults.standard.set(value, forKey: Self.draftSecondsKey)
        if notifyWeb { notifySettingsChanged() }
    }

    func setAnswerForm(_ form: String, notifyWeb: Bool) {
        let value = form == "approx" ? "approx" : Self.defaultAnswerForm
        guard value != answerForm else { return }
        answerForm = value
        UserDefaults.standard.set(value, forKey: Self.answerFormKey)
        if notifyWeb { notifySettingsChanged() }
    }

    func setHistoryInsert(_ raw: String, notifyWeb: Bool) {
        let value = raw == "answer" ? "answer" : Self.defaultHistoryInsert
        guard value != historyInsert else { return }
        historyInsert = value
        UserDefaults.standard.set(value, forKey: Self.historyInsertKey)
        if notifyWeb { notifySettingsChanged() }
    }

    func setHistoryShow(_ raw: String, notifyWeb: Bool) {
        let value = Self.normalizeHistoryShow(raw)
        guard value != historyShow else { return }
        historyShow = value
        UserDefaults.standard.set(value, forKey: Self.historyShowKey)
        if notifyWeb { notifySettingsChanged() }
    }

    func setRationalize(_ value: Bool, notifyWeb: Bool) {
        guard value != rationalize else { return }
        rationalize = value
        UserDefaults.standard.set(value, forKey: Self.rationalizeKey)
        if notifyWeb { notifySettingsChanged() }
    }

    func setSigFigMode(_ value: Bool, notifyWeb: Bool) {
        guard value != sigFigMode else { return }
        sigFigMode = value
        UserDefaults.standard.set(value, forKey: Self.sigFigModeKey)
        if notifyWeb { notifySettingsChanged() }
    }

    func setKeepWords(_ value: Bool, notifyWeb: Bool) {
        guard value != keepWords else { return }
        keepWords = value
        UserDefaults.standard.set(value, forKey: Self.keepWordsKey)
        if notifyWeb { notifySettingsChanged() }
    }

    func setTheme(_ raw: String, notifyWeb: Bool) {
        let value = Self.normalizeTheme(raw)
        guard value != theme else { return }
        theme = value
        UserDefaults.standard.set(value, forKey: Self.themeKey)
        if notifyWeb { notifySettingsChanged() }
    }

    func setAngleMode(_ raw: String, notifyWeb: Bool) {
        let value = raw == "rad" ? "rad" : "deg"
        guard value != angleMode else { return }
        angleMode = value
        UserDefaults.standard.set(value, forKey: Self.angleModeKey)
        if notifyWeb { notifySettingsChanged() }
    }

    func setFractionMode(_ value: Bool, notifyWeb: Bool) {
        guard value != fractionMode else { return }
        fractionMode = value
        UserDefaults.standard.set(value, forKey: Self.fractionModeKey)
        if notifyWeb { notifySettingsChanged() }
    }

    func setTypstPreview(_ value: Bool, notifyWeb: Bool) {
        guard value != typstPreview else { return }
        typstPreview = value
        UserDefaults.standard.set(value, forKey: Self.typstPreviewKey)
        if notifyWeb { notifySettingsChanged() }
    }

    func setTypstCopy(_ value: Bool, notifyWeb: Bool) {
        guard value != typstCopy else { return }
        typstCopy = value
        UserDefaults.standard.set(value, forKey: Self.typstCopyKey)
        if notifyWeb { notifySettingsChanged() }
    }

    // for the overlay and settings windows only, not NSApp, so the menu bar icon stays a template
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
        next[dim] = id.isEmpty ? nil : id
        applyDefaultUnits(next, notifyWeb: notifyWeb)
    }

    func resetDefaultUnits(notifyWeb: Bool) {
        applyDefaultUnits([:], notifyWeb: notifyWeb)
    }

    func replaceDefaultUnits(_ units: [String: String], notifyWeb: Bool) {
        applyDefaultUnits(units.filter { !$0.value.isEmpty }, notifyWeb: notifyWeb)
    }

    private func applyDefaultUnits(_ next: [String: String], notifyWeb: Bool) {
        guard next != defaultUnits else { return }
        defaultUnits = next
        UserDefaults.standard.set(next, forKey: Self.defaultUnitsKey)
        if notifyWeb { notifySettingsChanged() }
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
    private var statusItem: NSStatusItem?
    private var settingsWindow: SettingsWindowController?

    func applicationWillFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.prohibited)
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        setupStatusItem()
        // before the web view boots, so its injected settings already carry the shortcut
        registerHotKey()
        overlay = OverlayController()
        overlay?.onSettings = { [weak self] in self?.showSettings() }
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
        let settings = NSMenuItem(title: "Settings…", action: #selector(showSettings), keyEquivalent: ",")
        settings.target = self
        menu.addItem(settings)
        let tips = NSMenuItem(title: "Tips…", action: #selector(showTips), keyEquivalent: "")
        tips.target = self
        menu.addItem(tips)
        menu.addItem(.separator())
        let quit = NSMenuItem(title: "Quit Q Calc", action: #selector(quitApp), keyEquivalent: "q")
        quit.target = self
        menu.addItem(quit)
    }

    @objc private func showQuickCalc() {
        overlay?.toggle()
    }

    @objc private func showTips() {
        overlay?.showTips()
    }

    // false when macos or carbon refused the preset; the previous one stays bound
    @discardableResult
    func selectHotKey(_ next: HotKeyPreset) -> Bool {
        let previous = AppSettings.shared.activeHotKey
        if next == previous {
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

    @objc func showSettings() {
        overlay?.hide()
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

private let hotKeySignature = OSType(0x51434C43) // "QCLC"

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
