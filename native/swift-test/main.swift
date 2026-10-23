// Links the Rust core's static library through its UniFFI bindings and checks a few answers.
// Built and run for each architecture by native/scripts/build-mac.sh.

import Foundation

var failures = 0

func check(_ what: String, _ got: String, _ want: String) {
    if got == want {
        print("ok   \(what) → \(got)")
    } else {
        print("FAIL \(what) → \(got), want \(want)")
        failures += 1
    }
}

#if arch(arm64)
let arch = "arm64"
#else
let arch = "x86_64"
#endif
print("qcalc-core \(coreVersion()) on \(arch)")

var options = defaultEvalOptions()
let lines = evaluate(lines: ["2+3*4", "0.1+0.2", "1/3", "x = 2", "x^10", "3!", "√8", "255 in hex"], options: options)
check("2+3*4", lines[0].display, "14")
check("0.1+0.2", lines[1].display, "0.3")
check("1/3 at 6 figures", lines[2].display, "0.333333")
check("x = 2 is an assignment", String(describing: lines[3].kind), "assignment")
check("x^10", lines[4].display, "1024")
check("3!", lines[5].display, "6")
check("√8 exact", lines[6].exact ?? "", "2sqrt(2)")
check("255 in hex", lines[7].display, "0xFF")

options.sigFigs = 16
options.fractionMode = true
check("1/3 + 1/6 as a fraction", evaluate(lines: ["1/3 + 1/6"], options: options)[0].display, "1/2")

do {
    let json = try evaluateJson(request: #"{"lines":["2^0.5"],"options":{"sigFigs":16}}"#)
    check("evaluate_json", json.contains(#""display":"1.414213562373095""#) ? "has display" : json, "has display")
    _ = try evaluateJson(request: "not json")
    check("bad JSON throws", "returned", "threw")
} catch CoreError.BadRequest {
    check("bad JSON throws", "threw", "threw")
} catch {
    check("bad JSON throws", "\(error)", "threw")
}

// a burst of keystrokes, timed: the per-keystroke cost the UI pays
let start = Date()
var typed = ""
for ch in "sqrt(2)*(3+4)/7-1.5^2" {
    typed.append(ch)
    _ = evaluate(lines: [typed], options: options)
}
print(String(format: "21 keystrokes evaluated in %.2f ms", Date().timeIntervalSince(start) * 1000))

if failures > 0 {
    print("\(failures) failed")
    exit(1)
}
print("all passed")
