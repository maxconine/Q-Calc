import Foundation
import QCalcCore
import QuartzCore

/// The clock games run on (`gameClock.ts`, in the core): `now()` is `gameNow()` and `date()` `gameDateNow()`, with
/// the paused stretches taken out. The shell pauses it; games only read it.
enum GameTime {
    static let clock = GameClock()

    static var mono: Double { CACurrentMediaTime() * 1000 }
    static var wall: Double { Date().timeIntervalSince1970 * 1000 }

    /// performance.now on the game clock, in ms.
    static func now() -> Double { clock.now(mono: mono) }
    /// Date.now on the game clock, in ms.
    static func date() -> Double { clock.date(mono: mono, wall: wall) }
    static var paused: Bool { clock.paused() }
    static func pause() { clock.pause(mono: mono) }
    static func resume() { clock.resume(mono: mono) }
}

/// What a game keeps between launches, by the page's local-storage keys (`GAME_SAVE_KEYS`): the same values the
/// web page wrote, held by AppSettings, so a save from the web-view build carries over.
enum GameSaves {
    static func load(_ key: String) -> String? {
        guard let data = AppSettings.shared.gameSavesJSON().data(using: .utf8),
              let dict = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return nil }
        return dict[key] as? String
    }

    /// nil removes it. Over `game_save_max()` characters it isn't kept, as the page's mirror dropped it.
    static func store(_ key: String, _ value: String?) {
        guard gameSaveKeys().contains(key) else { return }
        if let value, value.utf16.count > Int(gameSaveMax()) { return }
        AppSettings.shared.saveGame(key, value)
    }
}

/// The name a room list shows for you (`nickname.ts`): random once ("teal otter"), then kept in `qcalc.nickname`.
enum GameNickname {
    private static let key = "qcalc.nickname"
    private static var current: String?

    static func load() -> String {
        if let current { return current }
        let stored = cleanNickname(raw: UserDefaults.standard.string(forKey: key) ?? "")
        let name = stored.isEmpty ? randomNickname() : stored
        current = name
        if stored.isEmpty { UserDefaults.standard.set(name, forKey: key) }
        return name
    }

    @discardableResult
    static func save(_ raw: String) -> String {
        let cleaned = cleanNickname(raw: raw)
        let name = cleaned.isEmpty ? load() : cleaned
        current = name
        UserDefaults.standard.set(name, forKey: key)
        return name
    }
}
