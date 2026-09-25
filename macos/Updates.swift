import AppKit
import Combine
import Sparkle

// sparkle checks daily and downloads in the background. q calc rarely quits, so a downloaded
// update installs on quit or, sooner, once the mac has sat idle with no q calc window up
final class Updates: NSObject, ObservableObject, SPUUpdaterDelegate, SPUStandardUserDriverDelegate {
    static let shared = Updates()
    // macos/test-update.sh shortens this through the defaults key
    private static var quietSeconds: TimeInterval {
        let override = UserDefaults.standard.double(forKey: "qcalc.updateQuietSeconds")
        return override > 0 ? override : 30 * 60
    }

    private var controller: SPUStandardUpdaterController?
    private var installNow: (() -> Void)?
    private var quietTimer: Timer?

    // builds without a real public key (dev builds) never start the updater
    var enabled: Bool { controller != nil }

    var automatic: Bool {
        get { controller?.updater.automaticallyDownloadsUpdates ?? false }
        set {
            objectWillChange.send()
            controller?.updater.automaticallyDownloadsUpdates = newValue
        }
    }

    func start() {
        guard controller == nil, Self.hasPublicKey else { return }
        controller = SPUStandardUpdaterController(startingUpdater: true, updaterDelegate: self, userDriverDelegate: self)
    }

    @objc func checkForUpdates() {
        controller?.checkForUpdates(nil)
    }

    private static var hasPublicKey: Bool {
        guard let key = Bundle.main.object(forInfoDictionaryKey: "SUPublicEDKey") as? String else { return false }
        return Data(base64Encoded: key)?.count == 32
    }

    func updater(_ updater: SPUUpdater, willInstallUpdateOnQuit item: SUAppcastItem, immediateInstallationBlock: @escaping () -> Void) -> Bool {
        installNow = immediateInstallationBlock
        quietTimer?.invalidate()
        let timer = Timer(timeInterval: min(60, Self.quietSeconds), repeats: true) { [weak self] _ in self?.installIfQuiet() }
        timer.tolerance = timer.timeInterval / 2
        RunLoop.main.add(timer, forMode: .common)
        quietTimer = timer
        return true
    }

    private func installIfQuiet() {
        guard let installNow, !Self.inUse, Self.idleSeconds >= Self.quietSeconds else { return }
        quietTimer?.invalidate()
        quietTimer = nil
        self.installNow = nil
        installNow()
    }

    // the status item's own window is always visible, so skip that level
    private static var inUse: Bool {
        NSApp.isActive || NSApp.windows.contains { $0.isVisible && $0.level != .statusBar }
    }

    private static var idleSeconds: TimeInterval {
        CGEventSource.secondsSinceLastEventType(.combinedSessionState, eventType: CGEventType(rawValue: ~0)!)
    }

    // menu bar apps get a log warning without this; sparkle still decides when its own alerts show
    var supportsGentleScheduledUpdateReminders: Bool { true }
}
