import AppKit
import QCalcCore
import SwiftUI

// The typeset preview row (TypstPreview.tsx). The engine writes the Typst document (`typstSource`, the port of
// typstDocument) and `qcalc-typst` (`TypstRenderer`, ARCHITECTURE.md §6) lays it out as vector paths in page points,
// drawn here in the text colour, so light and dark need no second compile. Mid-edit input Typst can't compile keeps
// the last picture, dimmed, as the web preview does.

/// One typeset line: the paths, and whether they belong to an earlier text (the current one didn't compile).
struct Typeset {
    let paths: TypstPaths
    let doc: String
    var stale = false
}

/// What the row typesets, decided on the main thread from what the bar shows (QuickCalc.tsx `typstShown`).
struct TypesetRequest: Equatable {
    /// the line, chained through `ans` as it evaluates (`chainedExpr`)
    let expr: String
    /// the exact form beside the shown answer, when the answer slot shows the plain one
    let exact: String?
    let display: String
    /// a solve's letter (`x = ±2`); empty otherwise
    let solvedFor: String
    /// the slot says something that isn't an answer (no real solution, a command), so nothing follows `=`
    let message: Bool
}

/// Compiles on its own serial queue, so neither the main thread nor the engine thread waits on Typst.
final class MathTypesetter {
    private let queue = DispatchQueue(label: "qcalc.typst", qos: .userInitiated)
    private let engine: Engine
    private var renderer: TypstRenderer?
    /// only touched on `queue`
    private lazy var logic = AppLogic(engine: engine)

    init(engine: Engine) {
        self.engine = engine
        // fonts and Typst's library once, then `$ x^2 $`, so the first keystroke is fast (warmupTypst)
        queue.async { [weak self] in
            let r = TypstRenderer()
            self?.renderer = r
            _ = r.warmUp()
        }
    }

    enum Outcome {
        /// nothing worth typesetting: the row hides
        case hidden
        case drawn(TypstPaths, doc: String)
        /// Typst rejected the document (mid-edit): keep what's shown, dimmed
        case failed(doc: String)
    }

    private let lock = NSLock()
    private var latest: TypesetRequest?

    /// Typesets `r` unless a newer request comes before its turn (a burst of keystrokes compiles only the last).
    func typeset(_ r: TypesetRequest, done: @escaping (Outcome) -> Void) {
        lock.withLock { latest = r }
        queue.async { [weak self] in
            guard let self, let renderer = self.renderer, self.lock.withLock({ self.latest == r }) else { return }
            let outcome: Outcome
            if !self.logic.bool("lib/typstMath", "typstPreviewUseful", [r.expr]) {
                outcome = .hidden
            } else {
                let answer = r.message ? "" : self.logic.string("lib/typstMath", "typstAnswer", [r.exact ?? NSNull(), r.display]) ?? ""
                if let doc = self.engine.typstSource(expr: r.expr, answer: answer, solvedFor: r.solvedFor) {
                    do {
                        outcome = .drawn(try renderer.paths(source: doc), doc: doc)
                    } catch {
                        outcome = .failed(doc: doc)
                    }
                } else {
                    outcome = .hidden
                }
            }
            DispatchQueue.main.async { done(outcome) }
        }
    }
}

/// The preview row under the bar: the paths shrunk to fit (`max-width: 100%`, `max-height: 72px`), centred, in
/// `--ink`, at 40% while stale.
struct MathPreview: View {
    let typeset: Typeset

    var body: some View {
        let paths = typeset.paths
        let w = CGFloat(paths.widthPt), h = CGFloat(paths.heightPt)
        Group {
            // an AppKit view, not a Canvas: a Canvas renders into GPU surfaces that SwiftUI keeps per size, about
            // 100 MB of them after a session of typing
            if #available(macOS 13, *) {
                FitLayout(width: w, height: h) { PathsView(paths: paths, width: w) }
            } else {
                FitFrame(width: w, height: h) { PathsView(paths: paths, width: w) }
            }
        }
        .opacity(typeset.stale ? 0.4 : 1)
        .animation(reduceMotion ? nil : .easeOut(duration: 0.15), value: typeset.stale)
        .accessibilityHidden(true)
    }

    private let reduceMotion = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion

    /// The page at 1 pt a point, shrunk to the width offered and 72 pt high, centred at the top.
    @available(macOS 13, *)
    private struct FitLayout: Layout {
        let width: CGFloat
        let height: CGFloat

        private func scale(_ available: CGFloat?) -> CGFloat {
            min(1, (available ?? width) / max(width, 1), 72 / max(height, 1))
        }

        func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
            let s = scale(proposal.width)
            return CGSize(width: proposal.width ?? width * s, height: height * s)
        }

        func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
            let s = scale(bounds.width)
            subviews.first?.place(at: CGPoint(x: bounds.midX, y: bounds.minY), anchor: .top,
                                  proposal: ProposedViewSize(width: width * s, height: height * s))
        }
    }

    /// FitLayout on macOS 12 (no Layout protocol): the same scale from the width the row is given, measured a pass
    /// later, so a new width settles one frame after it arrives.
    private struct FitFrame<Content: View>: View {
        let width: CGFloat
        let height: CGFloat
        @ViewBuilder let content: () -> Content
        @State private var available: CGFloat?

        var body: some View {
            let s = min(1, (available ?? width) / max(width, 1), 72 / max(height, 1))
            content()
                .frame(width: width * s, height: height * s)
                .frame(maxWidth: .infinity, alignment: .top)
                .background(GeometryReader { g in
                    Color.clear.preference(key: AvailableWidth.self, value: g.size.width)
                })
                .onPreferenceChange(AvailableWidth.self) { available = $0 }
        }
    }

    private struct AvailableWidth: PreferenceKey {
        static var defaultValue: CGFloat = 0
        static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = nextValue() }
    }

    static func path(_ verbs: Data, _ points: [Float]) -> Path {
        var p = Path()
        var i = 0
        func next() -> CGPoint {
            defer { i += 2 }
            return CGPoint(x: CGFloat(points[i]), y: CGFloat(points[i + 1]))
        }
        for verb in verbs {
            switch verb {
            case 0: p.move(to: next())
            case 1: p.addLine(to: next())
            case 2: let c = next(); p.addQuadCurve(to: next(), control: c)
            case 3: let a = next(); let b = next(); p.addCurve(to: next(), control1: a, control2: b)
            default: p.closeSubpath()
            }
        }
        return p
    }

    /// The page's paths drawn at 1 pt a point, scaled to the view's width.
    private struct PathsView: NSViewRepresentable {
        let paths: TypstPaths
        let width: CGFloat

        func makeNSView(context: Context) -> PathsNSView { PathsNSView() }

        func updateNSView(_ view: PathsNSView, context: Context) {
            view.paths = paths
            view.pageWidth = width
            view.needsDisplay = true
        }
    }

    final class PathsNSView: NSView {
        var paths: TypstPaths?
        var pageWidth: CGFloat = 1

        override var isFlipped: Bool { true }
        // the row isn't interactive (the panel still drags by its background)
        override func hitTest(_ point: NSPoint) -> NSView? { nil }

        override func draw(_ dirtyRect: NSRect) {
            guard let paths, let cg = NSGraphicsContext.current?.cgContext else { return }
            let s = bounds.width / max(pageWidth, 1)
            cg.scaleBy(x: s, y: s)
            MathPreview.draw(paths, in: cg)
        }
    }

    /// `draw(_:in:)` for a GraphicsContext, on a CGContext: the same clips, paints, fills and strokes.
    static func draw(_ paths: TypstPaths, in cg: CGContext) {
        let clips = paths.clips.map { path($0.verbs, $0.points).cgPath }
        for item in paths.paths {
            cg.saveGState()
            defer { cg.restoreGState() }
            var chain: [Int] = []
            var at = item.clip.map(Int.init)
            while let c = at, clips.indices.contains(c), !chain.contains(c) {
                chain.append(c)
                at = paths.clips[c].parent.map(Int.init)
            }
            for c in chain.reversed() {
                cg.addPath(clips[c])
                cg.clip()
            }
            let color: CGColor
            switch item.paint {
            case .foreground: color = Theme.ink.cgColor
            case .color(let c):
                color = CGColor(srgbRed: CGFloat(c.r) / 255, green: CGFloat(c.g) / 255, blue: CGFloat(c.b) / 255, alpha: CGFloat(c.a) / 255)
            }
            cg.addPath(path(item.verbs, item.points).cgPath)
            switch item.style {
            case .fill(let evenOdd):
                cg.setFillColor(color)
                cg.fillPath(using: evenOdd ? .evenOdd : .winding)
            case let .stroke(width, cap, join, miterLimit, dash, dashPhase):
                cg.setStrokeColor(color)
                cg.setLineWidth(CGFloat(width))
                let lineCap: CGLineCap = switch cap { case .butt: .butt; case .round: .round; case .square: .square }
                let lineJoin: CGLineJoin = switch join { case .miter: .miter; case .round: .round; case .bevel: .bevel }
                cg.setLineCap(lineCap)
                cg.setLineJoin(lineJoin)
                cg.setMiterLimit(CGFloat(miterLimit))
                if !dash.isEmpty { cg.setLineDash(phase: CGFloat(dashPhase), lengths: dash.map { CGFloat($0) }) }
                cg.strokePath()
            }
        }
    }

    static func draw(_ paths: TypstPaths, in base: GraphicsContext) {
        let clips = paths.clips.map { path($0.verbs, $0.points) }
        for item in paths.paths {
            var ctx = base
            // a clip inside its parent's: apply from the outermost in
            var chain: [Int] = []
            var at = item.clip.map(Int.init)
            while let c = at, clips.indices.contains(c), !chain.contains(c) {
                chain.append(c)
                at = paths.clips[c].parent.map(Int.init)
            }
            for c in chain.reversed() { ctx.clip(to: clips[c]) }
            let shading: GraphicsContext.Shading
            switch item.paint {
            case .foreground: shading = .color(Color.qInk)
            case .color(let c):
                shading = .color(Color(.sRGB, red: Double(c.r) / 255, green: Double(c.g) / 255, blue: Double(c.b) / 255,
                                       opacity: Double(c.a) / 255))
            }
            let p = path(item.verbs, item.points)
            switch item.style {
            case .fill(let evenOdd):
                ctx.fill(p, with: shading, style: FillStyle(eoFill: evenOdd))
            case let .stroke(width, cap, join, miterLimit, dash, dashPhase):
                let lineCap: CGLineCap = switch cap { case .butt: .butt; case .round: .round; case .square: .square }
                let lineJoin: CGLineJoin = switch join { case .miter: .miter; case .round: .round; case .bevel: .bevel }
                ctx.stroke(p, with: shading, style: StrokeStyle(lineWidth: CGFloat(width), lineCap: lineCap, lineJoin: lineJoin,
                                                                miterLimit: CGFloat(miterLimit), dash: dash.map { CGFloat($0) },
                                                                dashPhase: CGFloat(dashPhase)))
            }
        }
    }
}
