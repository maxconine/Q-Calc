import AppKit

// QA item 90: with Increase Contrast every grey that carries text reads at WCAG AA (4.5:1) on the bar and the tape,
// and with it off the faded greys are exactly the page's (parity).
var failed = 0
func check(_ ok: Bool, _ what: String) {
    print((ok ? "ok   " : "FAIL ") + what)
    if !ok { failed += 1 }
}
func resolved(_ c: NSColor, dark: Bool) -> (r: CGFloat, g: CGFloat, b: CGFloat, a: CGFloat) {
    var out = (r: CGFloat(0), g: CGFloat(0), b: CGFloat(0), a: CGFloat(0))
    NSAppearance(named: dark ? .darkAqua : .aqua)!.performAsCurrentDrawingAppearance {
        let s = c.usingColorSpace(.sRGB)!
        out = (s.redComponent, s.greenComponent, s.blueComponent, s.alphaComponent)
    }
    return out
}
func lum(_ c: (r: CGFloat, g: CGFloat, b: CGFloat, a: CGFloat)) -> CGFloat {
    func l(_ v: CGFloat) -> CGFloat { v <= 0.04045 ? v / 12.92 : pow((v + 0.055) / 1.055, 2.4) }
    return 0.2126 * l(c.r) + 0.7152 * l(c.g) + 0.0722 * l(c.b)
}
func ratio(_ fg: NSColor, on bg: NSColor, dark: Bool) -> CGFloat {
    let f = resolved(fg, dark: dark), b = resolved(bg, dark: dark)
    let mixed = (r: f.r * f.a + b.r * (1 - f.a), g: f.g * f.a + b.g * (1 - f.a), b: f.b * f.a + b.b * (1 - f.a), a: CGFloat(1))
    let (x, y) = (lum(mixed), lum(b))
    return (max(x, y) + 0.05) / (min(x, y) + 0.05)
}

Theme.devContrast = true
for dark in [false, true] {
    for (name, bg) in [("bar", Theme.bg), ("tape", Theme.bgTape)] {
        for alpha: CGFloat in [0.5, 0.7, 0.72, 0.8] {
            let r = ratio(Theme.mutedFaded(alpha), on: bg, dark: dark)
            check(r >= 4.5, "contrast \(dark ? "dark" : "light") \(name): muted at \(alpha) reads \(String(format: "%.2f", r)):1")
        }
        let r = ratio(Theme.muted, on: bg, dark: dark)
        check(r >= 4.5, "contrast \(dark ? "dark" : "light") \(name): muted \(String(format: "%.2f", r)):1")
    }
}
Theme.devContrast = false
if !NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast {
    for dark in [false, true] {
        let a = resolved(Theme.mutedFaded(0.8), dark: dark), b = resolved(Theme.muted.withAlphaComponent(0.8), dark: dark)
        check(a == b, "normal \(dark ? "dark" : "light"): the faded grey is the page's muted at 80%")
    }
}
print(failed == 0 ? "all passed" : "\(failed) failed")
exit(failed == 0 ? 0 : 1)
