import AppKit
import CoreImage
import QCalcCore
import QuartzCore

// The 420 smoke, 67 arms and 69 fold, native: a transparent, click-through panel over the bar (the room around
// it as the web view's had), drawn with Core Animation from the components' own numbers and keyframes
// (FourTwentySmoke.tsx, SixtySevenArms.tsx, SixtyNineFold.tsx and their rules in index.css). The puffs and how
// long each effect stays come from the core (smokePuffs, eggPlayMs). It tears itself down when the effect ends.

final class EggOverlay {
    private var panel: NSPanel?
    private var timer: Timer?
    /// the room around the bar, as the web view's panel had
    private let room: CGFloat = 200
    private static var puffImages: [String: CGImage] = [:]

    func play(_ kind: String, over bar: NSWindow) {
        stop()
        let frame = bar.frame.insetBy(dx: -room, dy: -room)
        let panel = NSPanel(contentRect: frame, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false
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
        let reduce = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        let dark = bar.effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
        let now = CACurrentMediaTime()
        switch kind {
        case "smoke": smoke(root, barRect, reduce, now, scale: bar.backingScaleFactor)
        case "arms": arms(root, barRect, reduce, now)
        default: fold(root, barRect, reduce, now, dark: dark)
        }
        panel.orderFrontRegardless()
        bar.addChildWindow(panel, ordered: .above)
        self.panel = panel
        let egg: EggKind = kind == "smoke" ? .smoke : kind == "arms" ? .arms : .fold
        timer = Timer.scheduledTimer(withTimeInterval: eggPlayMs(kind: egg) / 1000 + 0.3, repeats: false) { [weak self] _ in self?.stop() }
        OverlayLog.note("effect \(kind) (native)")
    }

    func stop() {
        timer?.invalidate()
        timer = nil
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
}
