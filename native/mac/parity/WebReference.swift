// The current app's bar, rendered for parity pictures: dist/quick.html in a WKWebView with the flags the web-view
// app's boot script sets (Overlay.swift build()), driven by steps, and snapshotted at the height the page asks for.
//
//   swiftc -parse-as-library native/mac/parity/WebReference.swift -o /tmp/webref
//   /tmp/webref <dist dir> <light|dark> <out.png> step…
//   steps: insert:<text>  key:<Enter|ArrowUp|ArrowDown|Tab|Escape>  wait:<ms>  js:<script>  width:<px>
import AppKit
import WebKit

@main
final class WebReference: NSObject, WKScriptMessageHandler, WKNavigationDelegate {
    static var keep: WebReference?
    let web: WKWebView
    let window: NSWindow
    var height: CGFloat = 72
    var steps: [String]
    let out: String
    let dist: URL

    init(dist: URL, theme: String, out: String, steps: [String]) {
        self.dist = dist
        self.out = out
        self.steps = steps
        let config = WKWebViewConfiguration()
        config.websiteDataStore = .nonPersistent()
        config.preferences.setValue(true, forKey: "allowFileAccessFromFileURLs")
        let boot = """
        window.__QCALC_NATIVE = true;
        document.documentElement.classList.add('quick-resizable');
        document.documentElement.classList.add('quick-native');
        document.documentElement.style.setProperty('--bar-width', '680px');
        window.__QCALC_SETTINGS = { sigFigs: 6, theme: "\(theme)", angleMode: "deg", answerForm: "exact", historyShow: "recent", copyUnitless: true, rationalize: true, typstPreview: true, keybinds: {}, hotkey: "⌃⌥Space", hotkeyFailed: false };
        window.__QCALC_ONBOARDING = { opens: 9, commits: 9, hints: 2147483647, done: 1 };
        window.__QCALC_MESSAGING = { name: "", key: "", accepted: [], blocked: [] };
        window.__QCALC_GAME_SAVES = {};
        window.__QCALC_RATES = null;
        document.documentElement.dataset.theme = "\(theme)";
        document.documentElement.style.overflow = 'hidden';
        """
        config.userContentController.addUserScript(WKUserScript(source: boot, injectionTime: .atDocumentStart, forMainFrameOnly: true))
        // an offscreen web view has no display link, so a CSS transition would freeze at its first frame; pictures
        // show the settled state instead
        let settle = "var s=document.createElement('style');s.textContent='*,*::before,*::after{transition:none!important}';document.head.appendChild(s);"
        config.userContentController.addUserScript(WKUserScript(source: settle, injectionTime: .atDocumentEnd, forMainFrameOnly: true))
        web = WKWebView(frame: NSRect(x: 0, y: 0, width: 680, height: 900), configuration: config)
        web.setValue(false, forKey: "drawsBackground")
        web.appearance = NSAppearance(named: theme == "dark" ? .darkAqua : .aqua)
        window = NSWindow(contentRect: NSRect(x: -4000, y: -4000, width: 680, height: 900), styleMask: [.borderless], backing: .buffered, defer: false)
        window.contentView = web
        window.appearance = web.appearance
        super.init()
        config.userContentController.add(self, name: "qcalc")
        web.navigationDelegate = self
    }

    static func main() {
        let args = CommandLine.arguments
        guard args.count >= 4 else {
            print("usage: webref <dist> <light|dark> <out.png> steps…")
            exit(2)
        }
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        let ref = WebReference(dist: URL(fileURLWithPath: args[1]), theme: args[2], out: args[3], steps: Array(args.dropFirst(4)))
        keep = ref
        ref.window.orderFrontRegardless()
        let quick = ref.dist.appendingPathComponent("quick.html")
        ref.web.loadFileURL(quick, allowingReadAccessTo: ref.dist)
        DispatchQueue.main.asyncAfter(deadline: .now() + 20) { print("timeout"); exit(1) }
        app.run()
    }

    func userContentController(_ c: WKUserContentController, didReceive message: WKScriptMessage) {
        if let d = message.body as? [String: Any], d["type"] as? String == "size", let h = d["height"] as? NSNumber {
            height = CGFloat(truncating: h)
        }
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        web.evaluateJavaScript("var el=document.querySelector('.quick-plain'); if(el) el.focus(); if (window.__qcalcSize) window.__qcalcSize();")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.6) { self.next() }
    }

    private func js(_ s: String, then: @escaping () -> Void) {
        web.evaluateJavaScript(s) { _, err in
            if let err { print("js error: \(err)") }
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.15, execute: then)
        }
    }

    private static func lit(_ s: String) -> String {
        let data = try! JSONSerialization.data(withJSONObject: [s])
        return String(String(data: data, encoding: .utf8)!.dropFirst().dropLast())
    }

    private func next() {
        guard !steps.isEmpty else { return snap() }
        let step = steps.removeFirst()
        let (verb, arg) = step.firstIndex(of: ":").map { (String(step[..<$0]), String(step[step.index(after: $0)...])) } ?? (step, "")
        switch verb {
        case "insert":
            js("window.__qcalcInsert && window.__qcalcInsert(\(Self.lit(arg)));", then: next)
        case "key":
            js("""
            (function(){ var el=document.querySelector('.quick-plain')||document.activeElement;
              var o={key:\(Self.lit(arg)),bubbles:true,cancelable:true};
              el.dispatchEvent(new KeyboardEvent('keydown',o)); el.dispatchEvent(new KeyboardEvent('keyup',o)); })()
            """, then: next)
        case "wait":
            DispatchQueue.main.asyncAfter(deadline: .now() + Double(Int(arg) ?? 300) / 1000, execute: next)
        case "js":
            js(arg, then: next)
        case "print":
            web.evaluateJavaScript(arg) { value, err in
                print("print: \(value.map { "\($0)" } ?? "nil") \(err.map { "\($0)" } ?? "")")
                self.next()
            }
        default:
            next()
        }
    }

    private func snap() {
        web.evaluateJavaScript("if (window.__qcalcSize) window.__qcalcSize();") { _, _ in
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
                let config = WKSnapshotConfiguration()
                config.rect = NSRect(x: 0, y: 0, width: 680, height: min(900, max(72, self.height)))
                self.web.takeSnapshot(with: config) { image, error in
                    guard let image, let tiff = image.tiffRepresentation,
                          let png = NSBitmapImageRep(data: tiff)?.representation(using: .png, properties: [:]) else {
                        print("snapshot failed: \(String(describing: error))")
                        exit(1)
                    }
                    try? png.write(to: URL(fileURLWithPath: self.out))
                    print("wrote \(self.out) (\(Int(self.height)) pt)")
                    exit(0)
                }
            }
        }
    }
}
