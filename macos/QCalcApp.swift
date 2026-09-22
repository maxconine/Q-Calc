import AppKit
import Carbon
import Combine

extension Notification.Name {
    static let qcalcSettingsChanged = Notification.Name("QCalc.settingsChanged")
}

final class AppSettings: ObservableObject {
    static let shared = AppSettings()
    static let sigFigsKey = "qcalc.sigFigs"
    static let draftSecondsKey = "qcalc.draftSeconds"
    static let defaultUnitsKey = "qcalc.defaultUnits"
    static let answerFormKey = "qcalc.answerForm"
    static let historyInsertKey = "qcalc.historyInsert"
    static let rationalizeKey = "qcalc.rationalize"
    static let themeKey = "qcalc.theme"
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
    @Published private(set) var theme: String

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
        theme = Self.loadTheme()
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
    private var statusItem: NSStatusItem?
    private var unitSettings: UnitSettingsWindowController?

    func applicationWillFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.prohibited)
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        setupStatusItem()
        overlay = OverlayController()
        overlay?.preload()
        registerHotKey()
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
        let quick = NSMenuItem(title: "Show Q Calc", action: #selector(showQuickCalc), keyEquivalent: "")
        quick.target = self
        menu.addItem(quick)
        menu.addItem(.separator())

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

    private func registerHotKey() {
        HotKeyBox.shared.onPress = { [weak self] in
            DispatchQueue.main.async { self?.toggleOverlay() }
        }
        let hotKeyID = EventHotKeyID(signature: OSType(0x51434C43), id: 1) // QCLC
        let modifiers = UInt32(controlKey | optionKey)
        let status = RegisterEventHotKey(
            UInt32(kVK_Space),
            modifiers,
            hotKeyID,
            GetEventDispatcherTarget(),
            0,
            &hotKeyRef
        )
        if status != noErr {
            NSLog("Q Calc: failed to register Control+Option+Space (%d)", status)
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
