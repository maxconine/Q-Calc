// Headless checks for the bar's update line (native/mac/scripts/test-update.sh). Foundation only.

import Foundation

var failures = 0

func check<T: Equatable>(_ what: String, _ got: T, _ want: T) {
    let ok = got == want
    print(ok ? "ok   \(what)" : "FAIL \(what)\n     got  \(got)\n     want \(want)")
    if !ok { failures += 1 }
}

check("patch newer", UpdateRules.isNewer("3.0.4", than: "3.0.3"), true)
check("two digits", UpdateRules.isNewer("3.0.10", than: "3.0.9"), true)
check("same", UpdateRules.isNewer("3.0.3", than: "3.0.3"), false)
check("older", UpdateRules.isNewer("3.0.2", than: "3.0.3"), false)
check("leading v", UpdateRules.isNewer("v3.1.0", than: "3.0.3"), true)
check("short form", UpdateRules.isNewer("3.1", than: "3.1.0"), false)
check("pre-release tail", UpdateRules.isNewer("4.0.0-beta", than: "3.9.9"), true)
check("junk isn't newer", UpdateRules.isNewer("latest", than: "3.0.3"), false)
check("empty isn't newer", UpdateRules.isNewer("", than: "3.0.3"), false)
check("dev 0.0.0 sees any", UpdateRules.isNewer("3.0.3", than: "0.0.0"), true)

func text(_ l: (text: String, action: String)?) -> String { l.map { "\($0.text) | \($0.action)" } ?? "nil" }
check("available", text(UpdateRules.line(latest: "3.0.4", current: "3.0.3", ready: false, dismissed: nil)),
      "Update available: 3.0.4 | update")
check("ready", text(UpdateRules.line(latest: "3.0.4", current: "3.0.3", ready: true, dismissed: nil)),
      "Update ready: 3.0.4 | install now")
check("nothing found", text(UpdateRules.line(latest: nil, current: "3.0.3", ready: false, dismissed: nil)), "nil")
check("already on it", text(UpdateRules.line(latest: "3.0.3", current: "3.0.3", ready: false, dismissed: nil)), "nil")
check("dismissed this run", text(UpdateRules.line(latest: "3.0.4", current: "3.0.3", ready: false, dismissed: "3.0.4")), "nil")
check("a newer one after a dismiss", text(UpdateRules.line(latest: "3.0.5", current: "3.0.3", ready: false, dismissed: "3.0.4")),
      "Update available: 3.0.5 | update")

print(failures == 0 ? "all passed" : "\(failures) failed")
exit(failures == 0 ? 0 : 1)
