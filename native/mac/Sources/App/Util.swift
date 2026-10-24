import AppKit
import Carbon
import SwiftUI

// ⌘ and a letter by the layout's letter; a layout without latin letters falls back to the key where it sits on
// qwerty (Overlay.swift's rule, which PeriodicWindow.swift and IdentityWindow.swift use for ⌘W)
func isCommandKey(_ event: NSEvent, _ letter: String, _ code: Int) -> Bool {
    let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
    guard flags.contains(.command), !flags.contains(.option), !flags.contains(.control), !flags.contains(.shift) else {
        return false
    }
    guard let chars = event.charactersIgnoringModifiers?.lowercased(), !chars.isEmpty else { return false }
    if chars == letter { return true }
    return !chars.unicodeScalars.allSatisfy({ $0.isASCII }) && event.keyCode == UInt16(code)
}

func isCommandWCloseKey(_ event: NSEvent) -> Bool {
    isCommandKey(event, "w", kVK_ANSI_W)
}

func isPlainEnterKey(_ event: NSEvent) -> Bool {
    let flags = event.modifierFlags.intersection([.command, .option, .control, .shift])
    return flags.isEmpty && !event.isARepeat
        && (event.keyCode == UInt16(kVK_Return) || event.keyCode == UInt16(kVK_ANSI_KeypadEnter))
}

func uptime() -> TimeInterval { ProcessInfo.processInfo.systemUptime }

extension View {
    // no Writing Tools, nor the "Write with Siri" prompt macOS 26 puts beside a focused field
    @ViewBuilder func noWritingTools() -> some View {
        if #available(macOS 15.4, *) {
            writingToolsBehavior(.disabled).writingToolsAffordanceVisibility(.hidden)
        } else if #available(macOS 15.0, *) {
            writingToolsBehavior(.disabled)
        } else {
            self
        }
    }
}

// a short diary of shows and hides, as the web app's overlay keeps (Overlay.swift OverlayLog), in its own file
enum OverlayLog {
    private static let url = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent("Library/Logs/\(AppInfo.supportFolder)/overlay.log")
    private static let previous = url.deletingPathExtension().appendingPathExtension("log.1")
    private static let limit = 128_000
    private static let queue = DispatchQueue(label: "qcalc.overlaylog", qos: .utility)
    private static let stamp: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyy-MM-dd HH:mm:ss.SSS"
        return formatter
    }()

    static func note(_ line: String) {
        let text = "\(stamp.string(from: Date())) \(line)\n"
        queue.async {
            let files = FileManager.default
            try? files.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            if let size = try? files.attributesOfItem(atPath: url.path)[.size] as? Int, size > limit {
                try? files.removeItem(at: previous)
                try? files.moveItem(at: url, to: previous)
            }
            guard let handle = try? FileHandle(forWritingTo: url) else {
                try? Data(text.utf8).write(to: url)
                return
            }
            handle.seekToEndOfFile()
            handle.write(Data(text.utf8))
            try? handle.close()
        }
    }
}

enum AppInfo {
    /// "Q Calc" for a release, "Q Calc Dev" for the dev build, so its history and logs never mix with the
    /// installed app's (Info.plist QCSupportFolder)
    static let supportFolder = Bundle.main.object(forInfoDictionaryKey: "QCSupportFolder") as? String ?? "Q Calc"
    static let isDev = Bundle.main.object(forInfoDictionaryKey: "QCDevBuild") as? Bool ?? false

    static var supportDirectory: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Application Support")
        return base.appendingPathComponent(supportFolder, isDirectory: true)
    }

    /// seconds since this process started, for the launch timing
    static var sinceProcessStart: TimeInterval {
        var info = kinfo_proc()
        var size = MemoryLayout<kinfo_proc>.size
        var mib: [Int32] = [CTL_KERN, KERN_PROC, KERN_PROC_PID, getpid()]
        guard sysctl(&mib, 4, &info, &size, nil, 0) == 0 else { return 0 }
        let start = info.kp_proc.p_starttime
        let started = Double(start.tv_sec) + Double(start.tv_usec) / 1_000_000
        return Date().timeIntervalSince1970 - started
    }
}
