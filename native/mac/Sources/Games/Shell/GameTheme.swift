import SwiftUI

// The games' look, value for value from src/index.css (:root and its dark block) and the games' CSS, with
// color-mix(in srgb, …) worked out the way the browser does it. Views read it as `@Environment(\.gameColors)`;
// GameShellView sets it from the window's appearance.

struct RGBA: Equatable {
    var r: Double, g: Double, b: Double, a: Double

    init(_ hex: UInt32, _ a: Double = 1) {
        r = Double((hex >> 16) & 0xff) / 255
        g = Double((hex >> 8) & 0xff) / 255
        b = Double(hex & 0xff) / 255
        self.a = a
    }

    init(r: Double, g: Double, b: Double, a: Double) {
        self.r = r; self.g = g; self.b = b; self.a = a
    }

    var color: Color { Color(.sRGB, red: r, green: g, blue: b, opacity: a) }
    var cg: CGColor { CGColor(srgbRed: r, green: g, blue: b, alpha: a) }

    /// `color-mix(in srgb, self p%, other)`: premultiplied, as CSS mixes.
    func mix(_ p: Double, _ other: RGBA) -> RGBA {
        let q = 1 - p
        let a = self.a * p + other.a * q
        guard a > 0 else { return RGBA(r: 0, g: 0, b: 0, a: 0) }
        return RGBA(r: (r * self.a * p + other.r * other.a * q) / a,
                    g: (g * self.a * p + other.g * other.a * q) / a,
                    b: (b * self.a * p + other.b * other.a * q) / a, a: a)
    }

    func opacity(_ k: Double) -> RGBA { RGBA(r: r, g: g, b: b, a: a * k) }
    static let clear = RGBA(r: 0, g: 0, b: 0, a: 0)
}

struct GameColors: Equatable {
    let dark: Bool
    let bg, bgTape, ink, muted, line, accent, control, page: RGBA

    init(dark: Bool) {
        self.dark = dark
        bg = dark ? RGBA(0x2c2c2e) : RGBA(0xffffff)
        bgTape = dark ? RGBA(0x252527) : RGBA(0xf5f5f7)
        ink = dark ? RGBA(0xf5f5f7) : RGBA(0x1d1d1f)
        // Increase Contrast (native only) takes muted and the hairlines to the bar's stronger values (Theme.swift)
        let high = Theme.increaseContrast
        muted = high ? (dark ? RGBA(0xaeaeb2) : RGBA(0x636366)) : dark ? RGBA(0x98989d) : RGBA(0x86868b)
        line = high ? (dark ? RGBA(0xffffff, 0.3) : RGBA(0x000000, 0.3)) : dark ? RGBA(0xffffff, 0.09) : RGBA(0x000000, 0.08)
        accent = dark ? RGBA(0x63d392) : RGBA(0x1d7a4c)
        control = dark ? RGBA(0x58585c) : RGBA(0xffffff)
        page = dark ? RGBA(0x1c1c1e) : RGBA(0xd8d8d4)
    }

    /// --fill-hover, --fill-selected, --fill-track: ink at 5, 8 and 6%.
    var fillHover: RGBA { ink.mix(0.05, .clear) }
    var fillSelected: RGBA { ink.mix(0.08, .clear) }
    var fillTrack: RGBA { ink.mix(0.06, .clear) }
    /// color-mix(in srgb, var(--bg) 70%, var(--bg-tape)): the court, the board's frame, the dial's bezel.
    var court: RGBA { bg.mix(0.7, bgTape) }
}

/// A CSS box-shadow layer.
struct ShadowLayer {
    var color: RGBA
    var x: CGFloat = 0, y: CGFloat = 0, blur: CGFloat = 0, spread: CGFloat = 0
}

extension GameColors {
    /// --control-shadow
    var controlShadow: [ShadowLayer] {
        dark ? [ShadowLayer(color: RGBA(0, 0.3), spread: 0.5), ShadowLayer(color: RGBA(0, 0.25), y: 1, blur: 2)]
             : [ShadowLayer(color: RGBA(0, 0.12), spread: 0.5), ShadowLayer(color: RGBA(0, 0.1), y: 1, blur: 2)]
    }

    /// --shadow-card
    var cardShadow: [ShadowLayer] {
        dark ? [ShadowLayer(color: RGBA(0xffffff, 0.1), spread: 0.5), ShadowLayer(color: RGBA(0, 0.3), y: 1, blur: 3)]
             : [ShadowLayer(color: RGBA(0, 0.1), spread: 0.5), ShadowLayer(color: RGBA(0, 0.06), y: 1, blur: 3)]
    }

    /// --shadow, the card's own
    var shadow: [ShadowLayer] {
        dark ? [ShadowLayer(color: RGBA(0, 0.55), y: 18, blur: 50), ShadowLayer(color: RGBA(0, 0.3), y: 2, blur: 8)]
             : [ShadowLayer(color: RGBA(0, 0.28), y: 18, blur: 50), ShadowLayer(color: RGBA(0, 0.12), y: 2, blur: 8)]
    }
}

private struct GameColorsKey: EnvironmentKey {
    static let defaultValue = GameColors(dark: false)
}

extension EnvironmentValues {
    var gameColors: GameColors {
        get { self[GameColorsKey.self] }
        set { self[GameColorsKey.self] = newValue }
    }
}

/// The CSS spacing and type scale (--sp-*, --fs-*, --radius*).
enum GameMetrics {
    static let sp1: CGFloat = 4, sp2: CGFloat = 8, sp3: CGFloat = 12, sp4: CGFloat = 16, sp5: CGFloat = 24
    static let gutter: CGFloat = 16
    static let fsDisplay: CGFloat = 24, fsRow: CGFloat = 17, fsBody: CGFloat = 13, fsSmall: CGFloat = 12, fsCaption: CGFloat = 11
    static let radius: CGFloat = 12, radiusS: CGFloat = 6
    static let tapeRow: CGFloat = 36
    /// --track-display: -0.021em
    static func displayTracking(_ size: CGFloat) -> CGFloat { -0.021 * size }
    /// .spotlight's width: min(680px, 100vw - 24px)
    static let panelWidth: CGFloat = 680
}

extension View {
    /// Box shadows, outermost last as CSS lists them. A spread-only layer (a hairline ring) is drawn as a stroke.
    func boxShadow(_ layers: [ShadowLayer], radius: CGFloat) -> some View {
        var v = AnyView(self)
        for layer in layers.reversed() {
            if layer.blur == 0 && layer.spread > 0 {
                v = AnyView(v.overlay(RoundedRectangle(cornerRadius: radius + layer.spread)
                    .strokeBorder(layer.color.color, lineWidth: layer.spread)
                    .padding(-layer.spread)
                    .allowsHitTesting(false)))
            } else {
                v = AnyView(v.shadow(color: layer.color.color, radius: layer.blur / 2, x: layer.x, y: layer.y))
            }
        }
        return v
    }

    /// An inset ring (`box-shadow: inset 0 0 0 w color`).
    func insetRing(_ color: RGBA, width: CGFloat, radius: CGFloat) -> some View {
        overlay(RoundedRectangle(cornerRadius: radius).strokeBorder(color.color, lineWidth: width).allowsHitTesting(false))
    }
}

extension Font {
    /// The page's font (-apple-system) at a CSS px size.
    static func game(_ size: CGFloat, _ weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight)
    }

    /// With tabular figures (font-variant-numeric: tabular-nums).
    static func gameDigits(_ size: CGFloat, _ weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight).monospacedDigit()
    }
}
