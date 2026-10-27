import SwiftUI

/// The games drawn in Swift, by kind. A kind listed here opens natively (Shell/GameWindow.swift); any other still
/// opens the games web bundle (GameWindowController). Each game adds its one line here.
enum NativeGames {
    static func view(_ kind: String) -> AnyView? {
        switch kind {
        default: return nil
        }
    }
}
