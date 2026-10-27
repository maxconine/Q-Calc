import AppKit
import SwiftUI

/// A key as the web page saw it (`KeyboardEvent`): `key` is the DOM's name (`ArrowUp`, `Enter`, `Backspace`,
/// `Tab`, `Escape`, `" "`, or the character typed, shift applied), `code` the qwerty position (`KeyW`,
/// `ArrowLeft`, `Space`, `Digit1`). Games and the lobby match on these, so the ported key handling reads as the TS.
struct GameKey: Equatable {
    var key: String
    var code: String
    var down: Bool = true
    var isRepeat: Bool = false
    var meta = false, ctrl = false, alt = false, shift = false

    /// `plainKey`: no ⌘, ⌃ or ⌥.
    var plain: Bool { !meta && !ctrl && !alt }

    init(key: String, code: String = "", down: Bool = true, isRepeat: Bool = false,
         meta: Bool = false, ctrl: Bool = false, alt: Bool = false, shift: Bool = false) {
        self.key = key; self.code = code; self.down = down; self.isRepeat = isRepeat
        self.meta = meta; self.ctrl = ctrl; self.alt = alt; self.shift = shift
    }

    init(event: NSEvent) {
        let flags = event.modifierFlags
        meta = flags.contains(.command)
        ctrl = flags.contains(.control)
        alt = flags.contains(.option)
        shift = flags.contains(.shift)
        down = event.type == .keyDown
        isRepeat = event.type == .keyDown && event.isARepeat
        code = GameKey.codes[event.keyCode] ?? ""
        if let named = GameKey.named[event.keyCode] {
            key = named
        } else if ctrl || meta {
            // the page saw the letter, not the control character ⌃F types
            key = event.charactersIgnoringModifiers ?? ""
        } else {
            key = event.characters ?? ""
        }
    }

    private static let named: [UInt16: String] = [
        123: "ArrowLeft", 124: "ArrowRight", 125: "ArrowDown", 126: "ArrowUp",
        36: "Enter", 76: "Enter", 51: "Backspace", 117: "Delete", 48: "Tab", 53: "Escape",
        115: "Home", 119: "End", 116: "PageUp", 121: "PageDown",
        122: "F1", 120: "F2", 99: "F3", 118: "F4", 96: "F5", 97: "F6", 98: "F7", 100: "F8", 101: "F9", 109: "F10",
    ]

    /// `KeyboardEvent.code` by the Mac's virtual key codes (ANSI layout positions).
    static let codes: [UInt16: String] = {
        var c: [UInt16: String] = [
            0: "KeyA", 11: "KeyB", 8: "KeyC", 2: "KeyD", 14: "KeyE", 3: "KeyF", 5: "KeyG", 4: "KeyH", 34: "KeyI",
            38: "KeyJ", 40: "KeyK", 37: "KeyL", 46: "KeyM", 45: "KeyN", 31: "KeyO", 35: "KeyP", 12: "KeyQ", 15: "KeyR",
            1: "KeyS", 17: "KeyT", 32: "KeyU", 9: "KeyV", 13: "KeyW", 7: "KeyX", 16: "KeyY", 6: "KeyZ",
            29: "Digit0", 18: "Digit1", 19: "Digit2", 20: "Digit3", 21: "Digit4", 23: "Digit5", 22: "Digit6",
            26: "Digit7", 28: "Digit8", 25: "Digit9",
            49: "Space", 36: "Enter", 76: "NumpadEnter", 51: "Backspace", 48: "Tab", 53: "Escape",
            123: "ArrowLeft", 124: "ArrowRight", 125: "ArrowDown", 126: "ArrowUp",
            27: "Minus", 24: "Equal", 33: "BracketLeft", 30: "BracketRight", 42: "Backslash", 41: "Semicolon",
            39: "Quote", 43: "Comma", 47: "Period", 44: "Slash", 50: "Backquote",
            56: "ShiftLeft", 60: "ShiftRight", 59: "ControlLeft", 62: "ControlRight", 58: "AltLeft", 61: "AltRight",
            55: "MetaLeft", 54: "MetaRight",
        ]
        c[117] = "Delete"
        return c
    }()

    /// A one-character key (`e.key.length === 1`, counted in UTF-16 like the page).
    var isCharacter: Bool { key.utf16.count == 1 }
}

/// Who gets a key, in order: the first handler that says true has it. The shell's own (⌃F, esc, the paused card)
/// run first, then the lobby, then whatever the open game pushed. A game pushes its handler when it appears and
/// pops it when it goes, as the page's panels added and removed their window listeners.
final class GameKeyRouter {
    typealias Handler = (GameKey) -> Bool
    private var handlers: [(id: Int, fn: Handler)] = []
    private var next = 0

    /// Adds a handler after the ones already there; returns its removal.
    @discardableResult
    func push(_ fn: @escaping Handler) -> () -> Void {
        next += 1
        let id = next
        handlers.append((id, fn))
        return { [weak self] in self?.handlers.removeAll { $0.id == id } }
    }

    /// Runs the handlers in order; true when one took the key.
    func dispatch(_ key: GameKey) -> Bool {
        for h in handlers where h.fn(key) { return true }
        return false
    }
}

// a handler's registration that lasts while a view is up: `.gameKeys(shell) { key in … }`

private struct GameKeysModifier: ViewModifier {
    let shell: GameShellModel
    let handler: (GameKey) -> Bool
    @State private var box = HandlerBox()

    func body(content: Content) -> some View {
        content
            .onAppear {
                box.fn = handler
                box.remove?()
                let b = box
                box.remove = shell.keys.push { b.fn?($0) ?? false }
            }
            .onDisappear {
                box.remove?()
                box.remove = nil
            }
            .background(HandlerRefresher(box: box, handler: handler))
    }
}

// keeps the registered closure the latest one, so it sees the view's current state
private struct HandlerRefresher: View {
    let box: HandlerBox
    let handler: (GameKey) -> Bool
    var body: some View {
        box.fn = handler
        return Color.clear.frame(width: 0, height: 0)
    }
}

final class HandlerBox {
    var fn: ((GameKey) -> Bool)?
    var remove: (() -> Void)?
}

extension View {
    /// Takes keys while this view is up, after the shell and the lobby. Return true to keep a key from the rest.
    func gameKeys(_ shell: GameShellModel, _ handler: @escaping (GameKey) -> Bool) -> some View {
        modifier(GameKeysModifier(shell: shell, handler: handler))
    }
}
