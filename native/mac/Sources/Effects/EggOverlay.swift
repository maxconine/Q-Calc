import AppKit
import CoreImage
import QCalcCore
import QuartzCore

// The 420 smoke, 67 arms and 69 fold, native: a transparent, click-through panel over the bar (the room around
// it as the web view's had), drawn with Core Animation from the components' own numbers and keyframes
// (FourTwentySmoke.tsx, SixtySevenArms.tsx, SixtyNineFold.tsx and their rules in index.css). The puffs and how
// long each effect stays come from the core (smokePuffs, eggPlayMs). It tears itself down when the effect ends.
//
// Native-only (no web counterpart; native/core/src/games/eggs.rs has their gates): the black hole that swallows
// an x/0 answer (`void`), Euler's confetti and note (`euler`), Pi Day's digits (`pidigits`) and the barrel roll
// (`roll`). Each is under 2.5 s, never takes a click or a key, and with Reduce Motion is static or skipped.

final class EggOverlay {
    /// the effect panel's title (never shown; dev snapshots find it by this)
    static let title = "Q Calc effect"
    private var panel: NSPanel?
    private var timer: Timer?
    /// the room around the bar, as the web view's panel had
    private let room: CGFloat = 200
    private static var puffImages: [String: CGImage] = [:]
    /// the effect up now
    private var kind: String?
    /// the black hole hides the real answer while its copy falls in (BarModel.answerVeiled)
    var onVeil: ((Bool) -> Void)?
    private var veiled = false
    private var veilTimer: Timer?
    /// the bar the barrel roll hid while its picture spins
    private weak var rolled: NSWindow?

    /// the line changed under an effect tied to its answer: end it now
    func interrupt() {
        if let kind, ["void", "pidigits", "roll"].contains(kind) { stop() }
    }

    func play(_ kind: String, over bar: NSWindow, answer: (frame: CGRect, text: String)? = nil) {
        stop()
        let reduce = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        // the barrel roll is all motion: Reduce Motion skips it
        if kind == "roll" && reduce { return }
        var room = self.room
        if kind == "roll" {
            // the whole bar turning needs its half-diagonal of room on every side
            let w = bar.frame.width, h = bar.frame.height
            room = max(room, (w * w + h * h).squareRoot() / 2 - min(w, h) / 2 + 24)
        }
        let frame = bar.frame.insetBy(dx: -room, dy: -room)
        let panel = NSPanel(contentRect: frame, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false
        panel.title = Self.title
        panel.setAccessibilityElement(false)
        panel.ignoresMouseEvents = true
        panel.level = .statusBar
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
        let host = NSView(frame: NSRect(origin: .zero, size: frame.size))
        let root = CALayer()
        root.frame = host.bounds
        root.isGeometryFlipped = true
        host.layer = root
        host.wantsLayer = true
        root.contentsScale = bar.backingScaleFactor
        panel.contentView = host
        // the bar, where the page drew it: (room, room) from the top left, at the bar's size
        let barRect = CGRect(x: room, y: room, width: bar.frame.width, height: bar.frame.height)
        let dark = bar.effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
        let now = CACurrentMediaTime()
        // the answer, where the bar drew it, in this layer's terms
        let answerRect = (answer?.frame ?? .zero).offsetBy(dx: room, dy: room)
        let answerText = answer?.text ?? ""
        let ms: Double
        switch kind {
        case "smoke", "arms", "fold":
            switch kind {
            case "smoke": smoke(root, barRect, reduce, now, scale: bar.backingScaleFactor)
            case "arms": arms(root, barRect, reduce, now)
            default: fold(root, barRect, reduce, now, dark: dark)
            }
            ms = eggPlayMs(kind: kind == "smoke" ? .smoke : kind == "arms" ? .arms : .fold)
        case "void":
            guard answer?.frame.width ?? 0 > 0 else { return }
            void(root, answerRect, answerText, reduce, now, bar: bar)
            ms = lineEggPlayMs(kind: .void)
        case "euler":
            euler(root, barRect, answer?.frame.width ?? 0 > 0 ? answerRect : .zero, reduce, now, bar: bar)
            ms = lineEggPlayMs(kind: .euler)
        case "pidigits":
            guard answer?.frame.width ?? 0 > 0 else { return }
            piDigits(root, barRect, answerRect, reduce, now, bar: bar)
            ms = lineEggPlayMs(kind: .piDigits)
        case "roll":
            guard roll(root, barRect, now, bar: bar) else { return }
            ms = lineEggPlayMs(kind: .barrelRoll)
        default: return
        }
        panel.orderFrontRegardless()
        bar.addChildWindow(panel, ordered: .above)
        self.panel = panel
        self.kind = kind
        timer = Timer.scheduledTimer(withTimeInterval: ms / 1000 + (kind == "roll" ? 0 : 0.3), repeats: false) { [weak self] _ in self?.stop() }
        OverlayLog.note("effect \(kind) (native)")
    }

    func stop() {
        timer?.invalidate()
        timer = nil
        veilTimer?.invalidate()
        veilTimer = nil
        if veiled {
            veiled = false
            onVeil?(false)
        }
        if let rolled {
            rolled.alphaValue = 1
            self.rolled = nil
        }
        kind = nil
        // the puffs are made again for the next 420: kept, they held ~35 MB (Core Image's and Core Animation's
        // copies) for good after the first
        Self.puffImages.removeAll()
        guard let panel else { return }
        panel.parent?.removeChildWindow(panel)
        panel.orderOut(nil)
        panel.contentView = nil
        self.panel = nil
    }

    // ---- 420: the puffs (.four-twenty, .four-twenty-puff, four-twenty-rise)

    private static func puffImage(_ w: CGFloat, _ h: CGFloat, scale: CGFloat) -> CGImage? {
        let id = "\(w)x\(h)@\(scale)"
        if let img = puffImages[id] { return img }
        // the ellipse's radial gradient reaches the box's corners (farthest-corner), then blur(18px)
        let pad: CGFloat = 54
        let pw = Int((w + 2 * pad) * scale), ph = Int((h + 2 * pad) * scale)
        guard let c = Canvas2D(width: pw, height: ph) else { return nil }
        c.scale(Double(scale), Double(scale))
        c.translate(Double(pad + w / 2), Double(pad + h / 2))
        c.scale(Double(w / 2) * 2.0.squareRoot(), Double(h / 2) * 2.0.squareRoot())
        let g = CanvasGradient(.radial(.zero, 0, .zero, 1))
        g.addColorStop(0, Col(255, 255, 255, 0.62))
        g.addColorStop(0.28, Col(255, 255, 255, 0.5))
        g.addColorStop(0.5, Col(230, 230, 234, 0.26))
        g.addColorStop(0.64, Col(215, 215, 220, 0.08))
        g.addColorStop(0.72, Col(215, 215, 220, 0))
        c.fill(g)
        c.beginPath()
        c.arc(0, 0, 1, 0, .pi * 2)
        c.fill()
        // border-radius 50% clips it to the ellipse of its box
        guard let raw = c.image() else { return nil }
        let clipped = Canvas2D(width: pw, height: ph)
        clipped?.scale(Double(scale), Double(scale))
        clipped?.beginPath()
        clipped?.ellipse(Double(pad + w / 2), Double(pad + h / 2), Double(w / 2), Double(h / 2), 0, 0, .pi * 2)
        clipped?.clip()
        clipped?.resetTransform()
        clipped?.drawImage(raw, 0, 0, Double(pw), Double(ph))
        guard let shape = clipped?.image() else { return nil }
        let blur = CIFilter(name: "CIGaussianBlur")
        blur?.setValue(CIImage(cgImage: shape), forKey: kCIInputImageKey)
        blur?.setValue(18 * scale, forKey: kCIInputRadiusKey)
        guard let out = blur?.outputImage?.cropped(to: CGRect(x: 0, y: 0, width: pw, height: ph)),
              let img = CIContext().createCGImage(out, from: CGRect(x: 0, y: 0, width: pw, height: ph)) else { return shape }
        puffImages[id] = img
        return img
    }

    private func smoke(_ root: CALayer, _ bar: CGRect, _ reduce: Bool, _ now: CFTimeInterval, scale: CGFloat) {
        // the layer: --bar-x 56 px either side of the bar, puffs placed from --bar-top 128 below its top
        let barX: CGFloat = 56, barTop: CGFloat = 128
        let layerW = bar.width + 2 * barX
        let origin = CGPoint(x: bar.minX - barX, y: bar.minY)
        for p in smokePuffs() {
            let w = CGFloat(p.w), h = CGFloat(p.h)
            let left: CGFloat
            if p.at == -1 {
                left = barX + CGFloat(p.x)
            } else if p.at == -2 {
                left = layerW - barX + CGFloat(p.x)
            } else {
                left = barX + (layerW - 2 * barX) * CGFloat(p.at) + CGFloat(p.x)
            }
            let l = CALayer()
            let pad: CGFloat = 54
            l.bounds = CGRect(x: 0, y: 0, width: w + 2 * pad, height: h + 2 * pad)
            l.contents = EggOverlay.puffImage(w, h, scale: scale)
            l.contentsScale = scale
            // translate(-50%, -50%): the box's top-left at (left, top) is its centre
            let centre = CGPoint(x: origin.x + left, y: origin.y + barTop + CGFloat(p.y))
            l.position = centre
            l.opacity = 0
            root.addSublayer(l)
            let begin = now + p.delay
            let group = CAAnimationGroup()
            group.beginTime = begin
            group.fillMode = .both
            group.isRemovedOnCompletion = false
            if reduce {
                group.duration = 2.5
                l.transform = CATransform3DScale(CATransform3DMakeTranslation(CGFloat(p.sway), CGFloat(p.rise) * 0.55, 0), 1, 1, 1)
                let o = CAKeyframeAnimation(keyPath: "opacity")
                o.values = [0, 0.8, 0.8, 0]
                o.keyTimes = [0, 0.18, 0.68, 1]
                o.timingFunctions = Array(repeating: CAMediaTimingFunction(name: .default), count: 3)
                group.animations = [o]
            } else {
                group.duration = 2.15
                let t = CAKeyframeAnimation(keyPath: "transform")
                func tf(_ x: CGFloat, _ y: CGFloat, _ s: CGFloat) -> NSValue {
                    NSValue(caTransform3D: CATransform3DScale(CATransform3DMakeTranslation(x, y, 0), s, s, 1))
                }
                t.values = [tf(0, 16, 0.45), tf(CGFloat(p.sway), CGFloat(p.rise) * 0.55, 1.1), tf(CGFloat(p.drift), CGFloat(p.rise), 1.6)]
                t.keyTimes = [0, 0.48, 1]
                t.timingFunctions = Array(repeating: CAMediaTimingFunction(name: .easeOut), count: 2)
                let o = CAKeyframeAnimation(keyPath: "opacity")
                o.values = [0, 0.85, 0.55, 0]
                o.keyTimes = [0, 0.14, 0.48, 1]
                o.timingFunctions = Array(repeating: CAMediaTimingFunction(name: .easeOut), count: 3)
                group.animations = [t, o]
            }
            l.add(group, forKey: "rise")
        }
    }

    // ---- 67: the two hands (.sixty-seven-arm, sixty-seven-weigh)

    private static let palm = "M4 20.8C4 16.2 7.4 13.6 12 13.5L20.2 13.8C23 14 24.2 16.4 24.2 19.8C24.2 24.3 21 26.9 15.6 26.9H11C6.8 26.9 4 24.6 4 20.8Z"
    private static let fingers: [(String, CGFloat)] = [
        ("M20 15Q29 15.2 35.6 11.2", 3.4), ("M21 18.4Q31 19.2 38.8 15.9", 3.5), ("M21 21.9Q31 23 38.4 20.6", 3.3),
        ("M20 25.2Q28 26.5 34.2 25", 3), ("M10.5 14.5Q12.6 8.6 18 6", 4),
    ]

    private func hand(mirrored: Bool) -> CALayer {
        let l = CALayer()
        l.bounds = CGRect(x: 0, y: 0, width: 44, height: 30)
        let colour = CGColor(srgbRed: 0xe4 / 255, green: 0xe4 / 255, blue: 0xea / 255, alpha: 1)
        var m = mirrored ? CGAffineTransform(a: -1, b: 0, c: 0, d: 1, tx: 44, ty: 0) : .identity
        let palm = CAShapeLayer()
        palm.path = SVGPath.path(EggOverlay.palm).cgPath.copy(using: &m)
        palm.fillColor = colour
        l.addSublayer(palm)
        for (d, w) in EggOverlay.fingers {
            let f = CAShapeLayer()
            f.path = SVGPath.path(d).cgPath.copy(using: &m)
            f.fillColor = nil
            f.strokeColor = colour
            f.lineWidth = w
            f.lineCap = .round
            l.addSublayer(f)
        }
        // a dark rim, like the pointer, so it reads on any desktop
        l.shadowColor = CGColor(gray: 0, alpha: 1)
        l.shadowOpacity = 0.65
        l.shadowRadius = 1
        l.shadowOffset = .zero
        return l
    }

    private func arms(_ root: CALayer, _ bar: CGRect, _ reduce: Bool, _ now: CFTimeInterval) {
        // the container: the bar's top 72 px, clipped (overflow hidden)
        let box = CALayer()
        box.frame = CGRect(x: bar.minX, y: bar.minY, width: bar.width, height: 72)
        box.masksToBounds = true
        root.addSublayer(box)
        let sides: [(Bool, CGFloat, CGPoint)] = [(false, bar.width / 2 - 92, CGPoint(x: 0.2, y: 0.7)), (true, bar.width / 2 + 48, CGPoint(x: 0.8, y: 0.7))]
        for (right, left, origin) in sides {
            let outer = CALayer()
            outer.bounds = CGRect(x: 0, y: 0, width: 44, height: 30)
            outer.anchorPoint = origin
            outer.position = CGPoint(x: left + 44 * origin.x, y: 30 * origin.y)
            let h = hand(mirrored: right)
            h.position = CGPoint(x: 22, y: 15)
            outer.addSublayer(h)
            // a softer second shadow under it
            outer.shadowColor = CGColor(gray: 0, alpha: 1)
            outer.shadowOpacity = 0.3
            outer.shadowRadius = 3
            outer.shadowOffset = CGSize(width: 0, height: 1)
            box.addSublayer(outer)
            func tf(_ y: CGFloat, _ deg: CGFloat) -> NSValue {
                NSValue(caTransform3D: CATransform3DRotate(CATransform3DMakeTranslation(0, y, 0), deg * .pi / 180, 0, 0, 1))
            }
            let up: CGFloat = -33 // translateY(-110%) of its 30 px
            let anim: CAAnimation
            if reduce {
                outer.transform = right ? tf(8, 4).caTransform3DValue : tf(24, 5).caTransform3DValue
                let o = CAKeyframeAnimation(keyPath: "opacity")
                o.values = [0, 1, 1, 0]
                o.keyTimes = [0, 0.15, 0.85, 1]
                o.duration = 2.6
                anim = o
            } else {
                let k = CAKeyframeAnimation(keyPath: "transform")
                let steps: [(CGFloat, CGFloat)] = right
                    ? [(up, 0), (5, 6), (29, -7), (7, 5), (28, -6), (8, 5), (27, -5), (9, 4), (18, 0), (21, 0), (up, 0)]
                    : [(up, 0), (33, 9), (6, -6), (29, 7), (7, -5), (28, 6), (8, -5), (27, 5), (18, 0), (21, 0), (up, 0)]
                k.values = steps.map { tf($0.0, $0.1) }
                k.keyTimes = [0, 0.1, 0.21, 0.32, 0.43, 0.54, 0.65, 0.76, 0.86, 0.9, 1]
                let base = CAMediaTimingFunction(controlPoints: 0.38, 0, 0.26, 1.32)
                var fns = Array(repeating: base, count: 10)
                fns[0] = CAMediaTimingFunction(controlPoints: 0.2, 0.8, 0.35, 1.25)
                fns[7] = CAMediaTimingFunction(controlPoints: 0.3, 0, 0.3, 1)
                fns[8] = CAMediaTimingFunction(name: .easeInEaseOut)
                fns[9] = CAMediaTimingFunction(controlPoints: 0.5, 0, 0.8, 0.4)
                k.timingFunctions = fns
                k.duration = 2.6
                anim = k
                outer.transform = tf(up, 0).caTransform3DValue
            }
            anim.beginTime = now
            anim.fillMode = .both
            anim.isRemovedOnCompletion = false
            outer.add(anim, forKey: "weigh")
        }
    }

    // ---- 69: the digits (.sixty-nine-digit, sixty-nine-spin)

    private func fold(_ root: CALayer, _ bar: CGRect, _ reduce: Bool, _ now: CFTimeInterval, dark: Bool) {
        let font = NSFont.systemFont(ofSize: 28, weight: .medium)
        let colour = dark ? CGColor(srgbRed: 0xf5 / 255, green: 0xf5 / 255, blue: 0xf7 / 255, alpha: 1) : CGColor(srgbRed: 0x1d / 255, green: 0x1d / 255, blue: 0x1f / 255, alpha: 1)
        for (text, spin) in [("6", 1.0), ("9", -1.0)] {
            let t = CATextLayer()
            t.string = NSAttributedString(string: text, attributes: [.font: font, .foregroundColor: NSColor(cgColor: colour) ?? .labelColor])
            t.contentsScale = root.contentsScale
            let size = (text as NSString).size(withAttributes: [.font: font])
            // line-height 1: the box is the font size tall
            t.bounds = CGRect(x: 0, y: 0, width: ceil(size.width), height: 28)
            let x = spin > 0 ? bar.minX + bar.width * 0.14 : bar.maxX - bar.width * 0.14 - ceil(size.width)
            t.position = CGPoint(x: x + ceil(size.width) / 2, y: bar.minY + bar.height / 2 + 14)
            t.opacity = 0
            root.addSublayer(t)
            let group = CAAnimationGroup()
            let o = CAKeyframeAnimation(keyPath: "opacity")
            if reduce {
                o.values = [0, 1, 1, 0]
                o.keyTimes = [0, 0.2, 0.8, 1]
                group.animations = [o]
            } else {
                o.values = [0, 1, 1, 0]
                o.keyTimes = [0, 0.12, 0.88, 1]
                let r = CABasicAnimation(keyPath: "transform.rotation.z")
                r.fromValue = 0
                r.toValue = spin * 2 * .pi
                r.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
                r.duration = 2.2
                group.animations = [o, r]
            }
            o.duration = 2.2
            o.timingFunctions = Array(repeating: CAMediaTimingFunction(name: .easeInEaseOut), count: 3)
            group.duration = 2.2
            group.beginTime = now
            group.fillMode = .both
            group.isRemovedOnCompletion = false
            t.add(group, forKey: "spin")
        }
    }

    // ---- native-only: the black hole (x/0), Euler's identity, Pi Day's digits, the barrel roll

    /// a theme colour as the bar's appearance draws it
    private static func resolved(_ colour: NSColor, in window: NSWindow) -> CGColor {
        var out = colour.cgColor
        window.effectiveAppearance.performAsCurrentDrawingAppearance { out = colour.cgColor }
        return out
    }

    private static let answerFont = NSFont.monospacedDigitSystemFont(ofSize: Theme.display, weight: .regular)

    /// the answer's text as the slot draws it: right-aligned in its frame, centred on it
    private static func answerTextRect(_ text: String, in frame: CGRect) -> CGRect {
        let size = (text as NSString).size(withAttributes: [.font: answerFont])
        let w = min(ceil(size.width), frame.width), h = ceil(size.height)
        return CGRect(x: frame.maxX - w, y: frame.midY - h / 2, width: w, height: h)
    }

    private static func keyframes(_ path: String, _ values: [Any], _ times: [NSNumber],
                                  _ timing: CAMediaTimingFunctionName = .easeInEaseOut) -> CAKeyframeAnimation {
        let k = CAKeyframeAnimation(keyPath: path)
        k.values = values
        k.keyTimes = times
        k.timingFunctions = Array(repeating: CAMediaTimingFunction(name: timing), count: max(values.count - 1, 1))
        return k
    }

    private static func group(_ animations: [CAAnimation], begin: CFTimeInterval, duration: CFTimeInterval) -> CAAnimationGroup {
        let g = CAAnimationGroup()
        g.animations = animations
        g.beginTime = begin
        g.duration = duration
        g.fillMode = .both
        g.isRemovedOnCompletion = false
        return g
    }

    /// x/0: a small black hole opens on the answer, its copy spirals in, the hole closes, and the answer (the real
    /// one, never changed) fades back. Reduce Motion: a still hole beside the answer, fading in and out.
    private func void(_ root: CALayer, _ answer: CGRect, _ text: String, _ reduce: Bool, _ now: CFTimeInterval, bar: NSWindow) {
        let textRect = Self.answerTextRect(text, in: answer)
        let r: CGFloat = reduce ? 8 : 12
        let centre = reduce ? CGPoint(x: textRect.minX - r - 10, y: answer.midY) : CGPoint(x: textRect.midX, y: answer.midY)
        let hole = CALayer()
        hole.bounds = CGRect(x: 0, y: 0, width: 4 * r, height: 4 * r)
        hole.position = centre
        // the disk: a warm dashed ring that turns, round a black core with a violet glow (so it reads on dark too)
        let ring = CAShapeLayer()
        ring.frame = hole.bounds
        ring.path = CGPath(ellipseIn: hole.bounds.insetBy(dx: r * 0.55, dy: r * 0.55), transform: nil)
        ring.fillColor = nil
        ring.strokeColor = CGColor(srgbRed: 1, green: 0.62, blue: 0.25, alpha: 0.85)
        ring.lineWidth = 2
        ring.lineDashPattern = [5, 3, 2, 3]
        ring.shadowColor = CGColor(srgbRed: 1, green: 0.5, blue: 0.2, alpha: 1)
        ring.shadowOpacity = 0.8
        ring.shadowRadius = 3
        ring.shadowOffset = .zero
        hole.addSublayer(ring)
        let core = CALayer()
        core.bounds = CGRect(x: 0, y: 0, width: 2 * r, height: 2 * r)
        core.position = CGPoint(x: 2 * r, y: 2 * r)
        core.cornerRadius = r
        core.backgroundColor = CGColor(gray: 0, alpha: 1)
        core.shadowColor = CGColor(srgbRed: 0.55, green: 0.3, blue: 1, alpha: 1)
        core.shadowOpacity = 0.9
        core.shadowRadius = 5
        core.shadowOffset = .zero
        hole.addSublayer(core)
        root.addSublayer(hole)
        if reduce {
            hole.opacity = 0
            hole.add(Self.group([Self.keyframes("opacity", [0, 1, 1, 0], [0, 0.2, 0.8, 1])], begin: now, duration: 1.4), forKey: "void")
            return
        }
        // the answer's copy, where the slot drew it; the real one hides under it until the hole closes
        let copy = CATextLayer()
        copy.string = NSAttributedString(string: text, attributes: [
            .font: Self.answerFont, .foregroundColor: NSColor(cgColor: Self.resolved(Theme.answer, in: bar)) ?? .labelColor,
            .kern: Theme.displayTracking(Theme.display),
        ])
        copy.contentsScale = root.contentsScale
        copy.alignmentMode = .right
        copy.bounds = CGRect(origin: .zero, size: textRect.size)
        copy.position = CGPoint(x: textRect.midX, y: textRect.midY)
        root.insertSublayer(copy, below: hole)
        veiled = true
        onVeil?(true)
        veilTimer = Timer.scheduledTimer(withTimeInterval: 1.4, repeats: false) { [weak self] _ in
            guard let self, self.veiled else { return }
            self.veiled = false
            self.onVeil?(false)
        }
        let open = NSValue(caTransform3D: CATransform3DIdentity)
        let shut = NSValue(caTransform3D: CATransform3DMakeScale(0.01, 0.01, 1))
        hole.transform = CATransform3DMakeScale(0.01, 0.01, 1)
        hole.add(Self.group([
            Self.keyframes("transform", [shut, open, open, NSValue(caTransform3D: CATransform3DMakeScale(1.25, 1.25, 1)), shut], [0, 0.18, 0.66, 0.74, 0.86]),
            Self.keyframes("opacity", [1, 1, 0], [0, 0.86, 1]),
        ], begin: now, duration: 1.7), forKey: "void")
        let spin = CABasicAnimation(keyPath: "transform.rotation.z")
        spin.fromValue = 0
        spin.toValue = 4 * CGFloat.pi
        spin.duration = 1.7
        spin.beginTime = now
        spin.fillMode = .both
        spin.isRemovedOnCompletion = false
        ring.add(spin, forKey: "turn")
        // the copy: drawn in on a tightening spiral, smaller and fainter
        let fall = CAKeyframeAnimation(keyPath: "position")
        let start = copy.position
        let span = hypot(start.x - centre.x, start.y - centre.y) + 14
        fall.values = (0...12).map { i -> NSValue in
            let t = CGFloat(i) / 12
            let angle = t * 2.2 * .pi
            let reach = span * 0.35 * sin(t * .pi)
            return NSValue(point: NSPoint(x: start.x + (centre.x - start.x) * t * t + reach * cos(angle),
                                          y: start.y + (centre.y - start.y) * t * t + reach * sin(angle)))
        }
        fall.calculationMode = .cubic
        let small = CATransform3DRotate(CATransform3DMakeScale(0.05, 0.05, 1), 2.4 * .pi, 0, 0, 1)
        let shrink = Self.keyframes("transform", [open, NSValue(caTransform3D: small)], [0, 1], .easeIn)
        let fade = Self.keyframes("opacity", [1, 1, 0], [0, 0.6, 1], .easeIn)
        copy.add(Self.group([fall, shrink, fade], begin: now + 0.28, duration: 0.85), forKey: "fall")
    }

    private static func confettiImage(_ colour: CGColor) -> CGImage? {
        guard let ctx = CGContext(data: nil, width: 8, height: 4, bitsPerComponent: 8, bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(),
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
        ctx.setFillColor(colour)
        ctx.fill(CGRect(x: 0, y: 0, width: 8, height: 4))
        return ctx.makeImage()
    }

    /// a small note in a pill of the bar's own colour, just under the bar, so it reads on any desktop
    private func note(_ root: CALayer, _ text: String, under bar: CGRect, centreX: CGFloat, window: NSWindow,
                      begin: CFTimeInterval, duration: CFTimeInterval) {
        let font = NSFont.systemFont(ofSize: Theme.small)
        let size = (text as NSString).size(withAttributes: [.font: font])
        let pill = CALayer()
        pill.bounds = CGRect(x: 0, y: 0, width: ceil(size.width) + 16, height: ceil(size.height) + 6)
        let x = min(max(centreX, bar.minX + pill.bounds.width / 2), bar.maxX - pill.bounds.width / 2)
        pill.position = CGPoint(x: x, y: bar.maxY + 6 + pill.bounds.height / 2)
        pill.cornerRadius = pill.bounds.height / 2
        pill.backgroundColor = Self.resolved(Theme.bg, in: window).copy(alpha: 0.9)
        let label = CATextLayer()
        label.string = NSAttributedString(string: text, attributes: [
            .font: font, .foregroundColor: NSColor(cgColor: Self.resolved(Theme.muted, in: window)) ?? .secondaryLabelColor,
        ])
        label.contentsScale = root.contentsScale
        label.alignmentMode = .center
        label.frame = CGRect(x: 0, y: 3, width: pill.bounds.width, height: ceil(size.height))
        pill.addSublayer(label)
        pill.opacity = 0
        root.addSublayer(pill)
        pill.add(Self.group([Self.keyframes("opacity", [0, 0.9, 0.9, 0], [0, 0.15, 0.8, 1])], begin: begin, duration: duration), forKey: "note")
    }

    /// e^(iπ)+1: a short burst of confetti off the answer, and a faint note under the bar. Reduce Motion: the note.
    private func euler(_ root: CALayer, _ bar: CGRect, _ answer: CGRect, _ reduce: Bool, _ now: CFTimeInterval, bar window: NSWindow) {
        let from = answer.width > 0 ? CGPoint(x: answer.maxX - 24, y: answer.midY) : CGPoint(x: bar.midX, y: bar.minY + Theme.composerHeight / 2)
        note(root, "the most beautiful equation", under: bar, centreX: from.x, window: window,
             begin: now + (reduce ? 0 : 0.25), duration: reduce ? 2.2 : 2.1)
        guard !reduce else { return }
        let emitter = CAEmitterLayer()
        emitter.frame = root.bounds
        emitter.emitterPosition = from
        emitter.emitterShape = .point
        emitter.beginTime = now
        let colours: [CGColor] = [
            CGColor(srgbRed: 0.39, green: 0.83, blue: 0.57, alpha: 1), CGColor(srgbRed: 1, green: 0.78, blue: 0.2, alpha: 1),
            CGColor(srgbRed: 0.98, green: 0.4, blue: 0.45, alpha: 1), CGColor(srgbRed: 0.4, green: 0.6, blue: 1, alpha: 1),
            CGColor(srgbRed: 0.75, green: 0.5, blue: 1, alpha: 1),
        ]
        emitter.emitterCells = colours.map { c in
            let cell = CAEmitterCell()
            cell.contents = Self.confettiImage(c)
            cell.birthRate = 110
            cell.lifetime = 1.5
            cell.lifetimeRange = 0.3
            cell.velocity = 190
            cell.velocityRange = 70
            // up and out (the layer is flipped: -y is up), falling back under gravity
            cell.emissionLongitude = -.pi / 2
            cell.emissionRange = .pi * 0.8
            cell.yAcceleration = 330
            cell.spin = 3
            cell.spinRange = 8
            cell.scale = 0.9
            cell.scaleRange = 0.4
            cell.alphaSpeed = -0.6
            return cell
        }
        root.addSublayer(emitter)
        // a burst: the cells are born for a tenth of a second, then fall
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { emitter.birthRate = 0 }
    }

    /// `pi` on March 14: π's digits roll out under the answer, as many as fit, then fade. Reduce Motion: they
    /// fade in whole.
    private func piDigits(_ root: CALayer, _ bar: CGRect, _ answer: CGRect, _ reduce: Bool, _ now: CFTimeInterval, bar window: NSWindow) {
        let font = NSFont.monospacedDigitSystemFont(ofSize: Theme.caption, weight: .regular)
        let digits = Array(QCalcCore.piDigits())
        // 3.14159 26535 89793 …: grouped by five after the point
        var text = String(digits.prefix(2))
        for (i, d) in digits.dropFirst(2).enumerated() {
            if i > 0 && i % 5 == 0 { text += " " }
            text.append(d)
        }
        let charWidth = ("0" as NSString).size(withAttributes: [.font: font]).width
        let room = answer.maxX - bar.minX - Theme.gutter
        var shown = text
        while CGFloat(shown.count) * charWidth > room, let space = shown.lastIndex(of: " ") { shown = String(shown[..<space]) }
        let size = (shown as NSString).size(withAttributes: [.font: font])
        let label = CATextLayer()
        label.string = NSAttributedString(string: shown, attributes: [
            .font: font, .foregroundColor: NSColor(cgColor: Self.resolved(Theme.muted, in: window)) ?? .secondaryLabelColor,
        ])
        label.contentsScale = root.contentsScale
        let w = ceil(size.width), h = ceil(size.height)
        label.frame = CGRect(x: answer.maxX - w, y: min(answer.maxY + 1, bar.minY + Theme.composerHeight - h - 1), width: w, height: h)
        root.addSublayer(label)
        label.opacity = 0
        let fade = Self.keyframes("opacity", [0, 1, 1, 0], reduce ? [0, 0.15, 0.78, 1] : [0, 0.04, 0.78, 1])
        label.add(Self.group([fade], begin: now, duration: 2.3), forKey: "digits")
        guard !reduce else { return }
        // a mask that opens a digit at a time from the left: the first seven (3.14159, the answer) at once
        let mask = CALayer()
        mask.backgroundColor = CGColor(gray: 0, alpha: 1)
        mask.anchorPoint = .zero
        mask.bounds = CGRect(x: 0, y: 0, width: w, height: h)
        mask.position = .zero
        label.mask = mask
        let n = max(shown.count, 8)
        let open = CAKeyframeAnimation(keyPath: "bounds.size.width")
        open.values = (7...n).map { CGFloat($0) * charWidth as NSNumber }
        open.keyTimes = (7...n).map { NSNumber(value: Double($0 - 7) / Double(n - 7)) }
        open.calculationMode = .discrete
        open.beginTime = now + 0.1
        open.duration = min(1.2, 0.03 * Double(n))
        open.fillMode = .both
        open.isRemovedOnCompletion = false
        mask.add(open, forKey: "roll")
    }

    /// do a barrel roll: a picture of the bar turns once in the room around it while the bar itself hides (it
    /// still has the keys; typing ends the roll). Never under Reduce Motion.
    private func roll(_ root: CALayer, _ bar: CGRect, _ now: CFTimeInterval, bar window: NSWindow) -> Bool {
        guard let view = window.contentView, let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { return false }
        view.cacheDisplay(in: view.bounds, to: rep)
        guard let image = rep.cgImage else { return false }
        let picture = CALayer()
        picture.frame = bar
        picture.contents = image
        picture.contentsGravity = .resize
        picture.shadowColor = CGColor(gray: 0, alpha: 1)
        picture.shadowOpacity = 0.25
        picture.shadowRadius = 12
        picture.shadowOffset = CGSize(width: 0, height: 6)
        root.addSublayer(picture)
        window.alphaValue = 0
        rolled = window
        let turn = CAKeyframeAnimation(keyPath: "transform")
        turn.values = [0.0, 0.5, 1.0].map { t -> NSValue in
            let s: CGFloat = t == 0.5 ? 0.9 : 1
            return NSValue(caTransform3D: CATransform3DRotate(CATransform3DMakeScale(s, s, 1), CGFloat(t) * 2 * .pi * 0.999, 0, 0, 1))
        }
        turn.keyTimes = [0, 0.5, 1]
        turn.timingFunctions = [CAMediaTimingFunction(name: .easeIn), CAMediaTimingFunction(name: .easeOut)]
        picture.add(Self.group([turn], begin: now, duration: 1.0), forKey: "roll")
        return true
    }
}

/// The Konami code's look, for the run only: the bar in green phosphor on black, glowing, under scanlines. The
/// colours go through a Core Image filter on the bar's content (so the field and the answers turn with it); the
/// lines are a click-through layer on top. Nothing moves, so Reduce Motion needs nothing.
enum CRTLook {
    private final class Scanlines: NSView {
        override func hitTest(_ point: NSPoint) -> NSView? { nil }
        override var mouseDownCanMoveWindow: Bool { true }
    }

    static func apply(_ on: Bool, to view: NSView) {
        view.subviews.filter { $0 is Scanlines }.forEach { $0.removeFromSuperview() }
        guard on else {
            view.contentFilters = []
            return
        }
        view.wantsLayer = true
        view.layerUsesCoreImageFilters = true
        // the light in each pixel, as phosphor green on near black
        let tint = CIFilter(name: "CIColorMatrix")
        func row(_ k: CGFloat) -> CIVector { CIVector(x: 0.3 * k, y: 0.59 * k, z: 0.11 * k, w: 0) }
        tint?.setValue(row(0.3), forKey: "inputRVector")
        tint?.setValue(row(1.15), forKey: "inputGVector")
        tint?.setValue(row(0.5), forKey: "inputBVector")
        tint?.setValue(CIVector(x: -0.02, y: -0.07, z: -0.03, w: 0), forKey: "inputBiasVector")
        let glow = CIFilter(name: "CIBloom")
        glow?.setValue(3, forKey: kCIInputRadiusKey)
        glow?.setValue(0.6, forKey: kCIInputIntensityKey)
        view.contentFilters = [tint, glow].compactMap { $0 }
        OverlayLog.note("crt look: \(view.layer?.filters?.count ?? 0) filters on the bar")
        let lines = Scanlines(frame: view.bounds)
        lines.autoresizingMask = [.width, .height]
        lines.wantsLayer = true
        let rep = CAReplicatorLayer()
        rep.frame = CGRect(x: 0, y: 0, width: 4000, height: 2400)
        rep.instanceCount = 800
        rep.instanceTransform = CATransform3DMakeTranslation(0, 3, 0)
        let line = CALayer()
        line.frame = CGRect(x: 0, y: 0, width: 4000, height: 1)
        line.backgroundColor = CGColor(gray: 0, alpha: 0.28)
        rep.addSublayer(line)
        lines.layer?.addSublayer(rep)
        lines.layer?.masksToBounds = true
        lines.setAccessibilityElement(false)
        view.addSubview(lines)
    }
}
