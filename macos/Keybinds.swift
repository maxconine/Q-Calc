import AppKit
import Carbon

// the same chords as src/lib/keybinds.ts: "ctrl+alt+shift+cmd+key", key names from the physical key
enum KeyNames {
    static let byCode: [Int: String] = {
        var names: [Int: String] = [
            kVK_ANSI_A: "a", kVK_ANSI_B: "b", kVK_ANSI_C: "c", kVK_ANSI_D: "d", kVK_ANSI_E: "e", kVK_ANSI_F: "f",
            kVK_ANSI_G: "g", kVK_ANSI_H: "h", kVK_ANSI_I: "i", kVK_ANSI_J: "j", kVK_ANSI_K: "k", kVK_ANSI_L: "l",
            kVK_ANSI_M: "m", kVK_ANSI_N: "n", kVK_ANSI_O: "o", kVK_ANSI_P: "p", kVK_ANSI_Q: "q", kVK_ANSI_R: "r",
            kVK_ANSI_S: "s", kVK_ANSI_T: "t", kVK_ANSI_U: "u", kVK_ANSI_V: "v", kVK_ANSI_W: "w", kVK_ANSI_X: "x",
            kVK_ANSI_Y: "y", kVK_ANSI_Z: "z",
            kVK_ANSI_0: "0", kVK_ANSI_1: "1", kVK_ANSI_2: "2", kVK_ANSI_3: "3", kVK_ANSI_4: "4",
            kVK_ANSI_5: "5", kVK_ANSI_6: "6", kVK_ANSI_7: "7", kVK_ANSI_8: "8", kVK_ANSI_9: "9",
            kVK_ANSI_Comma: ",", kVK_ANSI_Period: ".", kVK_ANSI_Slash: "/", kVK_ANSI_Backslash: "\\",
            kVK_ANSI_Semicolon: ";", kVK_ANSI_Quote: "'", kVK_ANSI_LeftBracket: "[", kVK_ANSI_RightBracket: "]",
            kVK_ANSI_Minus: "-", kVK_ANSI_Equal: "=", kVK_ANSI_Grave: "`",
            kVK_Space: "space", kVK_Delete: "backspace", kVK_ForwardDelete: "delete", kVK_Return: "enter",
            kVK_ANSI_KeypadEnter: "enter", kVK_Tab: "tab", kVK_Escape: "escape",
            kVK_UpArrow: "up", kVK_DownArrow: "down", kVK_LeftArrow: "left", kVK_RightArrow: "right",
            kVK_Home: "home", kVK_End: "end", kVK_PageUp: "pageup", kVK_PageDown: "pagedown",
        ]
        let fkeys = [kVK_F1, kVK_F2, kVK_F3, kVK_F4, kVK_F5, kVK_F6, kVK_F7, kVK_F8, kVK_F9, kVK_F10,
                     kVK_F11, kVK_F12, kVK_F13, kVK_F14, kVK_F15, kVK_F16, kVK_F17, kVK_F18, kVK_F19, kVK_F20]
        for (i, code) in fkeys.enumerated() { names[code] = "f\(i + 1)" }
        return names
    }()

    // keypad enter reads as enter too, but return is the one to register
    static let byName: [String: Int] = {
        var codes: [String: Int] = [:]
        for (code, name) in byCode where code != kVK_ANSI_KeypadEnter { codes[name] = code }
        return codes
    }()

    static let labels: [String: String] = [
        "space": "Space", "backspace": "⌫", "delete": "⌦", "enter": "↵", "tab": "⇥", "escape": "esc",
        "up": "↑", "down": "↓", "left": "←", "right": "→", "home": "↖", "end": "↘", "pageup": "⇞", "pagedown": "⇟",
        "plus": "+",
    ]

    // what an NSMenuItem wants as its key equivalent
    static func menuKey(_ name: String) -> String {
        let special: [String: Int] = [
            "up": NSUpArrowFunctionKey, "down": NSDownArrowFunctionKey, "left": NSLeftArrowFunctionKey,
            "right": NSRightArrowFunctionKey, "home": NSHomeFunctionKey, "end": NSEndFunctionKey,
            "pageup": NSPageUpFunctionKey, "pagedown": NSPageDownFunctionKey, "delete": NSDeleteFunctionKey,
        ]
        if let code = special[name], let scalar = UnicodeScalar(code) { return String(Character(scalar)) }
        if name.hasPrefix("f"), let n = Int(name.dropFirst()), (1...20).contains(n),
           let scalar = UnicodeScalar(NSF1FunctionKey + n - 1) {
            return String(Character(scalar))
        }
        switch name {
        case "space": return " "
        case "backspace": return "\u{8}"
        case "enter": return "\r"
        case "tab": return "\t"
        case "escape": return "\u{1b}"
        default: return name.count == 1 ? name : ""
        }
    }
}

struct Chord: Equatable {
    static let modifierOrder = ["ctrl", "alt", "shift", "cmd"]

    let modifiers: Set<String>
    let key: String

    init?(_ text: String) {
        var parts = text.lowercased().split(separator: "+", omittingEmptySubsequences: false).map(String.init)
        guard let key = parts.popLast(), !key.isEmpty, !Self.modifierOrder.contains(key),
              parts.allSatisfy(Self.modifierOrder.contains)
        else { return nil }
        self.modifiers = Set(parts)
        self.key = key
    }

    // nil for a key the table doesn't know, or while only modifiers are down
    init?(event: NSEvent) {
        guard let key = KeyNames.byCode[Int(event.keyCode)] else { return nil }
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        var mods = Set<String>()
        if flags.contains(.control) { mods.insert("ctrl") }
        if flags.contains(.option) { mods.insert("alt") }
        if flags.contains(.shift) { mods.insert("shift") }
        if flags.contains(.command) { mods.insert("cmd") }
        self.modifiers = mods
        self.key = key
    }

    var text: String {
        (Self.modifierOrder.filter(modifiers.contains) + [key]).joined(separator: "+")
    }

    // ⌃⌥⇧⌘ is the mac's own order
    var title: String {
        let glyphs = ["ctrl": "⌃", "alt": "⌥", "shift": "⇧", "cmd": "⌘"]
        let mods = Self.modifierOrder.filter(modifiers.contains).compactMap { glyphs[$0] }.joined()
        return mods + (KeyNames.labels[key] ?? key.uppercased())
    }

    var carbonModifiers: UInt32 {
        var out = 0
        if modifiers.contains("ctrl") { out |= controlKey }
        if modifiers.contains("alt") { out |= optionKey }
        if modifiers.contains("shift") { out |= shiftKey }
        if modifiers.contains("cmd") { out |= cmdKey }
        return UInt32(out)
    }

    var menuModifiers: NSEvent.ModifierFlags {
        var out: NSEvent.ModifierFlags = []
        if modifiers.contains("ctrl") { out.insert(.control) }
        if modifiers.contains("alt") { out.insert(.option) }
        if modifiers.contains("shift") { out.insert(.shift) }
        if modifiers.contains("cmd") { out.insert(.command) }
        return out
    }
}

// the show / hide shortcut: any chord carbon can register
struct GlobalHotKey: Equatable {
    let chord: Chord
    let keyCode: UInt32

    init?(_ text: String) {
        guard let chord = Chord(text), let code = KeyNames.byName[chord.key] else { return nil }
        self.chord = chord
        self.keyCode = UInt32(code)
    }

    var id: String { chord.text }
    var title: String { chord.title }
    var carbonModifiers: UInt32 { chord.carbonModifiers }
    var menuModifiers: NSEvent.ModifierFlags { chord.menuModifiers }
    var menuKey: String { KeyNames.menuKey(chord.key) }

    static let standard = GlobalHotKey("ctrl+alt+space")!

    // what 2.0 stored: one of four preset ids
    private static let presetIDs = [
        "ctrl-opt-space": "ctrl+alt+space",
        "cmd-opt-space": "alt+cmd+space",
        "ctrl-space": "ctrl+space",
        "opt-space": "alt+space",
    ]

    static func stored(_ raw: String?) -> GlobalHotKey {
        guard let raw else { return standard }
        return GlobalHotKey(presetIDs[raw] ?? raw) ?? standard
    }
}

// mirrors KEY_ACTIONS in src/lib/keybinds.ts; `show` is the global shortcut, the rest the page acts on
enum KeyActions {
    static let all: [(id: String, label: String)] = [
        ("show", "Show / hide Q Calc"),
        ("settings", "Open settings"),
        ("copyAnswer", "Copy answer"),
        ("copyLine", "Copy whole line"),
        ("angle", "Degrees / radians"),
        ("fraction", "Fractions"),
        ("sigFigs", "Sig figs from input"),
        ("clear", "Clear history"),
    ]

    static let defaults: [String: String] = [
        "show": GlobalHotKey.standard.id,
        "settings": "cmd+,",
        "copyAnswer": "cmd+c",
        "copyLine": "shift+cmd+c",
        "angle": "ctrl+d",
        "fraction": "ctrl+f",
        "sigFigs": "ctrl+s",
        "clear": "ctrl+c",
    ]

    static func label(_ id: String) -> String {
        all.first { $0.id == id }?.label ?? id
    }

    // the stored override, else the default; "" means no key
    static func chord(_ id: String, in binds: [String: String]) -> String {
        binds[id] ?? defaults[id] ?? ""
    }

    static func title(_ chord: String) -> String {
        Chord(chord)?.title ?? ""
    }

    // why a chord can't be used, or nil when it can. same rules as chordProblem in the page
    static func problem(_ chord: Chord, for action: String, binds: [String: String]) -> String? {
        let mods = chord.modifiers
        let fkey = chord.key.hasPrefix("f") && Int(chord.key.dropFirst()) != nil
        if !fkey && mods.isDisjoint(with: ["ctrl", "alt", "cmd"]) {
            return "Add ⌘, ⌃ or ⌥ so typing still works"
        }
        if mods == ["cmd"], let kept = ["v": "paste", "x": "cut", "a": "select all", "z": "undo"][chord.key] {
            return "\(chord.title) is kept for \(kept)"
        }
        if mods == ["cmd", "shift"] && chord.key == "z" {
            return "\(chord.title) is kept for redo"
        }
        for other in all where other.id != action && Self.chord(other.id, in: binds) == chord.text {
            return "\(chord.title) already does \(other.label.lowercased())"
        }
        return nil
    }

    // only real actions and real chords; "" stays, it means no key
    static func sanitize(_ raw: [String: Any]) -> [String: String] {
        var out: [String: String] = [:]
        for (id, value) in raw {
            guard id != "show", defaults[id] != nil, let text = value as? String else { continue }
            if text.isEmpty {
                out[id] = ""
            } else if let chord = Chord(text) {
                out[id] = chord.text
            }
        }
        return out
    }
}
