import AppKit
import WebKit

// The only web views in the app. The calculator never makes one; a game command (pong, ember, connect 4,
// trails, ultimate, crack, equal, numbers) builds one in its own window, loaded with the games-only bundle
// (games.html#<kind>, built by `npm run build` from src/games.tsx), and closing the window releases it, so the
// Safari WebContent process exits with it. The 420/67/69 effects do the same in a click-through panel over the bar
// (EffectOverlay) and tear themselves down when the animation ends.

enum GameBundle {
    /// Contents/Resources/web/games.html, or nil when this build has no game bundle (build.sh warns).
    static var url: URL? {
        guard let dir = Bundle.main.url(forResource: "web", withExtension: nil) else { return nil }
        let games = dir.appendingPathComponent("games.html")
        return FileManager.default.fileExists(atPath: games.path) ? games : nil
    }

    /// What the page reads at start, as the bar's page did: saves, nickname, messaging, theme.
    static func bootScript(kind: String) -> WKUserScript {
        let nickname = UserDefaults.standard.string(forKey: "qcalc.nickname").flatMap(jsonString) ?? "null"
        let source = """
        window.__QCALC_NATIVE = true;
        window.__QCALC_GAME_SAVES = \(AppSettings.shared.gameSavesJSON());
        window.__QCALC_NICKNAME = \(nickname);
        window.__QCALC_MESSAGING = \(Messages.shared.webJSON);
        window.__QCALC_SETTINGS = { theme: \(jsonString(AppSettings.shared.theme) ?? "\"system\"") };
        document.documentElement.dataset.theme = \(jsonString(AppSettings.shared.theme) ?? "\"system\"");
        """
        return WKUserScript(source: source, injectionTime: .atDocumentStart, forMainFrameOnly: true)
    }

    static func jsonString(_ s: String) -> String? {
        guard let data = try? JSONSerialization.data(withJSONObject: [s]), let json = String(data: data, encoding: .utf8) else { return nil }
        return String(json.dropFirst().dropLast())
    }

    static func configuration(kind: String, handler: WKScriptMessageHandler) -> WKWebViewConfiguration {
        let config = WKWebViewConfiguration()
        config.websiteDataStore = .nonPersistent()
        config.userContentController.add(handler, name: "qcalc")
        config.userContentController.addUserScript(bootScript(kind: kind))
        config.preferences.setValue(true, forKey: "allowFileAccessFromFileURLs")
        if #available(macOS 15.0, *) {
            config.setValue(NSWritingToolsBehavior.none.rawValue, forKey: "writingToolsBehavior")
        }
        return config
    }
}

// the script message handler holds its target weakly, so the configuration never keeps the controller (or the
// web view) alive after the window closes
private final class WeakHandler: NSObject, WKScriptMessageHandler {
    weak var target: WKScriptMessageHandler?
    init(_ target: WKScriptMessageHandler) { self.target = target }
    func userContentController(_ c: WKUserContentController, didReceive message: WKScriptMessage) {
        target?.userContentController(c, didReceive: message)
    }
}

final class GameWindowController: NSObject, NSWindowDelegate, WKScriptMessageHandler, WKNavigationDelegate {
    private var window: NSWindow?
    private var web: WKWebView?
    var onClose: (() -> Void)?

    func open(_ kind: String, on screen: NSScreen?) {
        close()
        guard let url = GameBundle.url else {
            let alert = NSAlert()
            alert.messageText = "This build has no games"
            alert.informativeText = "Run npm run build, then native/mac/build.sh, to bundle games.html."
            alert.runModal()
            onClose?()
            return
        }
        let config = GameBundle.configuration(kind: kind, handler: WeakHandler(self))
        let web = WKWebView(frame: NSRect(x: 0, y: 0, width: 900, height: 620), configuration: config)
        web.navigationDelegate = self
        web.setValue(false, forKey: "drawsBackground")
        web.autoresizingMask = [.width, .height]
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 900, height: 620),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        window.title = BarCommand.gameList.first { $0.kind == kind }?.name ?? "Q Calc"
        window.titlebarAppearsTransparent = true
        window.isReleasedWhenClosed = false
        window.delegate = self
        window.contentView = web
        window.backgroundColor = .windowBackgroundColor
        window.appearance = AppSettings.shared.nsAppearance
        window.minSize = NSSize(width: 480, height: 360)
        if let visible = (screen ?? NSScreen.main)?.visibleFrame {
            window.setFrame(GameWindow.frame(visible: visible, natural: CGSize(width: 900, height: 620), titlebar: 28), display: false)
        } else {
            window.center()
        }
        self.window = window
        self.web = web
        web.loadFileURL(url.appendingFragment(kind), allowingReadAccessTo: url.deletingLastPathComponent())
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(web)
        OverlayLog.note("game \(kind) opened (web view made)")
    }

    func close() {
        guard let window else { return }
        window.delegate = nil
        window.close()
        teardown()
    }

    func windowWillClose(_ notification: Notification) {
        teardown()
    }

    private func teardown() {
        web?.navigationDelegate = nil
        web?.configuration.userContentController.removeAllScriptMessageHandlers()
        web?.removeFromSuperview()
        web = nil
        window?.contentView = nil
        window = nil
        OverlayLog.note("game closed (web view released)")
        onClose?()
    }

    func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
        close()
    }

    func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
        guard let dict = message.body as? [String: Any] else { return }
        switch dict["type"] as? String {
        case "game":
            if dict["open"] as? Bool == false { DispatchQueue.main.async { [weak self] in self?.close() } }
        case "dismiss":
            DispatchQueue.main.async { [weak self] in self?.close() }
        case "gameSave":
            if let key = dict["key"] as? String { AppSettings.shared.saveGame(key, dict["value"] as? String) }
        case "nickname":
            if let name = dict["name"] as? String, !name.isEmpty, name.count <= 64 {
                UserDefaults.standard.set(name, forKey: "qcalc.nickname")
            }
        case "copy":
            if let text = dict["text"] as? String {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(text, forType: .string)
            }
        case "gameFullscreen":
            if let window, (dict["on"] as? Bool ?? false) != window.isZoomed { window.zoom(nil) }
        default:
            break
        }
    }
}

private extension URL {
    func appendingFragment(_ fragment: String) -> URL {
        var c = URLComponents(url: self, resolvingAgainstBaseURL: false)
        c?.fragment = fragment
        return c?.url ?? self
    }
}

/// The 420 smoke, 67 arms and 69 fold: a transparent, click-through panel over the bar with a web view of
/// games.html#smoke (or #arms, #fold), gone when the animation ends. No bundle, no effect.
final class EffectOverlay: NSObject, WKScriptMessageHandler {
    private var panel: NSPanel?
    private var web: WKWebView?
    private var timer: Timer?
    // the room around the bar the effects draw in (smokeRoom.ts keeps them within 240 px)
    private let room: CGFloat = 200

    func play(_ kind: String, over bar: NSWindow) {
        stop()
        guard let url = GameBundle.url else { return }
        let frame = bar.frame.insetBy(dx: -room, dy: -room)
        let panel = NSPanel(contentRect: frame, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false
        panel.ignoresMouseEvents = true
        panel.level = .statusBar
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
        let config = GameBundle.configuration(kind: kind, handler: WeakHandler(self))
        let web = WKWebView(frame: NSRect(origin: .zero, size: frame.size), configuration: config)
        web.setValue(false, forKey: "drawsBackground")
        web.autoresizingMask = [.width, .height]
        panel.contentView = web
        var c = URLComponents(url: url, resolvingAgainstBaseURL: false)
        c?.queryItems = [
            URLQueryItem(name: "x", value: "\(Int(room))"), URLQueryItem(name: "y", value: "\(Int(room))"),
            URLQueryItem(name: "w", value: "\(Int(bar.frame.width))"), URLQueryItem(name: "h", value: "\(Int(bar.frame.height))"),
        ]
        c?.fragment = kind
        if let u = c?.url { web.loadFileURL(u, allowingReadAccessTo: url.deletingLastPathComponent()) }
        panel.orderFrontRegardless()
        bar.addChildWindow(panel, ordered: .above)
        self.panel = panel
        self.web = web
        // the longest effect runs 2.7 s, plus the page's start-up
        timer = Timer.scheduledTimer(withTimeInterval: 3.6, repeats: false) { [weak self] _ in self?.stop() }
        OverlayLog.note("effect \(kind) (web view made)")
    }

    func stop() {
        timer?.invalidate()
        timer = nil
        guard let panel else { return }
        panel.parent?.removeChildWindow(panel)
        web?.configuration.userContentController.removeAllScriptMessageHandlers()
        web?.removeFromSuperview()
        web = nil
        panel.orderOut(nil)
        panel.contentView = nil
        self.panel = nil
        OverlayLog.note("effect over (web view released)")
    }

    func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
        if (message.body as? [String: Any])?["type"] as? String == "effectDone" {
            DispatchQueue.main.async { [weak self] in self?.stop() }
        }
    }
}
