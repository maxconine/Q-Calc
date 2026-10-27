import AppKit
import Combine
import ServiceManagement

// the shortcut only works while q calc runs; opening at login is off until someone turns it on in settings
final class LoginItem: ObservableObject {
    static let shared = LoginItem()

    var enabled: Bool {
        get {
            if #available(macOS 13, *) {
                return SMAppService.mainApp.status == .enabled
            }
            return FileManager.default.fileExists(atPath: Self.agentURL.path)
        }
        set {
            objectWillChange.send()
            guard #available(macOS 13, *) else {
                Self.setAgent(newValue)
                return
            }
            do {
                if newValue {
                    try SMAppService.mainApp.register()
                } else {
                    try SMAppService.mainApp.unregister()
                }
            } catch {
                NSLog("Q Calc: couldn't change the login item (%@)", error.localizedDescription)
            }
            // switched off in system settings earlier: only the user can turn it back on there
            if newValue, SMAppService.mainApp.status == .requiresApproval {
                SMAppService.openSystemSettingsLoginItems()
            }
        }
    }

    // system settings can change it while q calc runs
    func refresh() {
        objectWillChange.send()
    }

    // macOS 12 has no SMAppService.mainApp (and SMLoginItemSetEnabled needs a helper app inside the bundle): a
    // per-user launch agent that opens this app at login, the way login items worked before 13. Written on, removed
    // off; launchd reads it at the next login, so turning it on doesn't launch a second copy now
    private static var agentURL: URL {
        let id = Bundle.main.bundleIdentifier ?? "com.maxconine.qcalc"
        return FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/LaunchAgents/\(id).login.plist")
    }

    private static func setAgent(_ on: Bool) {
        let url = agentURL
        do {
            if on {
                let id = Bundle.main.bundleIdentifier ?? "com.maxconine.qcalc"
                let plist: [String: Any] = [
                    "Label": "\(id).login",
                    "ProgramArguments": ["/usr/bin/open", "-a", Bundle.main.bundlePath],
                    "RunAtLoad": true,
                    "LimitLoadToSessionType": "Aqua",
                ]
                let data = try PropertyListSerialization.data(fromPropertyList: plist, format: .xml, options: 0)
                try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
                try data.write(to: url, options: .atomic)
            } else if FileManager.default.fileExists(atPath: url.path) {
                try FileManager.default.removeItem(at: url)
            }
        } catch {
            NSLog("Q Calc: couldn't change the login item (%@)", error.localizedDescription)
        }
    }
}
