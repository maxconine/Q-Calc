import AppKit
import Combine
import ServiceManagement

// the shortcut only works while q calc runs; opening at login is off until someone turns it on in settings
final class LoginItem: ObservableObject {
    static let shared = LoginItem()

    var enabled: Bool {
        get { SMAppService.mainApp.status == .enabled }
        set {
            objectWillChange.send()
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
}
