import SwiftUI

/// The games drawn in Swift, by kind. A kind listed here opens natively (Shell/GameWindow.swift); any other still
/// opens the games web bundle (GameWindowController). Each game adds its one line here.
enum NativeGames {
    static func view(_ kind: String) -> AnyView? {
        switch kind {
        case "pong": return AnyView(PongPanel())
        case "crack": return AnyView(CrackPanel())
        case "connect4": return AnyView(Connect4Panel())
        case "ultimate": return AnyView(UltimatePanel())
        case "equal": return AnyView(EqualPanel())
        case "numbers": return AnyView(NumbersPanel())
        case "ember": return AnyView(EmberPanel())
        case "trails": return AnyView(TrailsPanel())
        default: return nil
        }
    }
}
