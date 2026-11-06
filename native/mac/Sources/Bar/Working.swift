import AppKit
import QCalcCore
import SwiftUI

// Native only: the working for the line, so an answer can be checked by hand. The core says the steps
// (`lib/steps` `steps`: derivatives by their rules, the integral table's form, by parts or substitution, a linear
// or quadratic equation, a unit conversion's factors, a differential equation's method), each its Typst maths, its
// plain text and why; ⌃W (rebindable, `working`) shows them in a panel under the bar, typeset by the preview's
// renderer. A line with working says so under its answer; a line without has nothing.

/// One step: its maths as the core wrote it, and its picture once typeset.
struct WorkingStep: Identifiable, Equatable {
    let id: Int
    let math: String
    let plain: String
    let why: String
    var paths: TypstPaths?
}

/// Asks the core for the working on a queue of its own, so neither a keystroke nor the preview waits on it.
final class WorkingMaker {
    private let queue = DispatchQueue(label: "qcalc.working", qos: .userInitiated)
    private let engine: Engine
    /// only touched on `queue`
    private lazy var logic = AppLogic(engine: engine)
    private let lock = NSLock()
    private var latest: String?

    init(engine: Engine) {
        self.engine = engine
    }

    /// The steps for `text` (nil when there are none), unless a newer line is asked for before its turn.
    func steps(for text: String, options: [String: Any], done: @escaping ([WorkingStep]?) -> Void) {
        lock.withLock { latest = text }
        queue.async { [weak self] in
            guard let self, self.lock.withLock({ self.latest == text }) else { return }
            let raw = self.logic.call("lib/steps", "steps", [text, options]) as? [[String: Any]]
            let steps = raw?.enumerated().compactMap { i, s -> WorkingStep? in
                guard let math = s["math"] as? String, let plain = s["plain"] as? String, let why = s["why"] as? String else { return nil }
                return WorkingStep(id: i, math: math, plain: plain, why: why)
            }
            DispatchQueue.main.async { done(steps.flatMap { $0.isEmpty ? nil : $0 }) }
        }
    }

    /// Each step's document (`lib/steps` `stepDocument`), worked out on this queue.
    func documents(_ steps: [WorkingStep], done: @escaping ([String]) -> Void) {
        queue.async { [weak self] in
            guard let self else { return }
            let docs = steps.map { self.logic.string("lib/steps", "stepDocument", [$0.math]) ?? "" }
            DispatchQueue.main.async { done(docs) }
        }
    }
}

extension BarModel {
    /// The working for the line just answered: none for a chained line, a blank one, or one whose last step
    /// doesn't end in what the slot shows.
    func findWorking(_ typed: String, answer: LiveAnswer, chained: Bool, context: EvalContext) {
        guard !chained, !answer.isBlank, !answer.improper else {
            working = nil
            return
        }
        var options: [String: Any] = [
            "angleMode": angleRad ? "rad" : "deg",
            "sigFigs": Int(self.options.sigFigs),
            "fractionMode": fractions,
            "spaceThousands": settings.spaceThousands,
            "variables": context.variables,
            "functions": context.functions.mapValues { ["params": $0.params, "body": $0.body] },
        ]
        if let ans = context.ans { options["ans"] = ans }
        let shown = answer.display
        workingMaker.steps(for: typed, options: options) { [weak self] steps in
            guard let self, self.text == typed else { return }
            // never working to another answer than the one the bar shows
            guard let steps, let last = steps.last, last.plain.hasSuffix(shown) else {
                self.working = nil
                return
            }
            if self.working?.map(\.plain) == steps.map(\.plain) { return }
            self.working = steps
            if self.workingShown { self.typesetWorking() }
        }
    }

    /// ⌃W: the panel up or down. On a line with no working it waits, and comes up with the next line that has some.
    func toggleWorking() {
        workingShown.toggle()
        if workingShown { typesetWorking() }
    }

    /// The steps' pictures, once, for the steps now shown.
    func typesetWorking() {
        guard let steps = working, steps.contains(where: { $0.paths == nil }) else { return }
        workingMaker.documents(steps) { [weak self] docs in
            self?.typesetter.paths(docs) { [weak self] pictures in
                guard let self, let now = self.working, now.map(\.plain) == steps.map(\.plain) else { return }
                self.working = now.enumerated().map { i, s in
                    var s = s
                    s.paths = pictures.indices.contains(i) ? pictures[i] : nil
                    return s
                }
            }
        }
    }

    /// A step's maths as plain text on the clipboard, with the "✓ copied" flash.
    func copyStep(_ step: WorkingStep) {
        let board = NSPasteboard.general
        board.clearContents()
        board.setString(step.plain, forType: .string)
        flashCopied()
    }
}

/// The working under the bar, in the graph and systems panels' place and look: a head, then the steps, numbered,
/// typeset, each with why beside it, muted. A click on a step copies its maths.
struct WorkingPanel: View {
    @ObservedObject var model: BarModel
    let steps: [WorkingStep]
    @ObservedObject private var settings = AppSettings.shared

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline, spacing: 12) {
                Text("Working").font(.system(size: Theme.body, weight: .semibold)).foregroundStyle(Color.qInk)
                Text("click a step to copy it").font(.system(size: Theme.small)).foregroundStyle(Color.qMuted)
                Spacer(minLength: 0)
                let key = KeyActions.title(KeyActions.chord("working", in: settings.keybinds))
                if !key.isEmpty {
                    Text("\(key) hides").font(.system(size: Theme.small)).foregroundStyle(Color.qMuted)
                }
            }
            ScrollView(.vertical) {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(steps) { step in
                        WorkingRow(number: step.id + 1, step: step) { model.copyStep(step) }
                    }
                }
            }
            .frame(maxHeight: 320)
            .fixedSize(horizontal: false, vertical: true)
        }
        .padding(.top, 12)
        .padding(.horizontal, Theme.gutter)
        .padding(.bottom, 16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.qBgTape)
        .overlay(alignment: .top) { Rectangle().fill(Color.qLine).frame(height: 1) }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Working")
    }
}

/// One numbered step: the maths (its picture, or the plain text until it's typeset) and why beside it.
private struct WorkingRow: View {
    let number: Int
    let step: WorkingStep
    let copy: () -> Void
    @State private var hover = false

    var body: some View {
        Button(action: copy) {
            HStack(alignment: .center, spacing: 12) {
                Text("\(number)")
                    .font(.system(size: Theme.small).monospacedDigit())
                    .foregroundStyle(Color.qMuted)
                    .frame(minWidth: 14, alignment: .trailing)
                Group {
                    if let paths = step.paths {
                        StepMath(paths: paths)
                    } else {
                        Text(step.plain).font(.system(size: Theme.body)).foregroundStyle(Color.qInk)
                    }
                }
                .layoutPriority(1)
                Spacer(minLength: 8)
                Text(step.why)
                    .font(.system(size: Theme.small))
                    .foregroundStyle(Color.qMuted)
                    .multilineTextAlignment(.trailing)
                    .lineLimit(3)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: 320, alignment: .trailing)
            }
            .padding(.vertical, 4).padding(.horizontal, 8)
            .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(hover ? Color.qFillHover : .clear))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hover = $0 }
        .hoverTip("Copy \(step.plain)")
        .accessibilityLabel("Step \(number): \(step.plain), \(step.why)")
        .accessibilityHint("copies the step")
    }
}

/// A step's picture at the size Typst laid it out, shrunk only when the row is narrower.
private struct StepMath: View {
    let paths: TypstPaths

    var body: some View {
        let w = CGFloat(paths.widthPt), h = CGFloat(paths.heightPt)
        ViewThatFitsOrScale(width: w, height: h) {
            Canvas { ctx, size in
                let s = size.width / max(w, 1)
                ctx.scaleBy(x: s, y: s)
                MathPreview.draw(paths, in: ctx)
            }
        }
        .accessibilityHidden(true)
    }
}

/// The picture at 1 pt a point, or narrower in proportion when the row hasn't the room.
private struct ViewThatFitsOrScale<Content: View>: View {
    let width: CGFloat
    let height: CGFloat
    @ViewBuilder let content: () -> Content
    @State private var available: CGFloat?

    var body: some View {
        let s = min(1, (available ?? width) / max(width, 1))
        content()
            .frame(width: width * s, height: height * s)
            .frame(maxWidth: width, alignment: .leading)
            .background(GeometryReader { g in
                Color.clear.preference(key: StepWidth.self, value: g.size.width)
            })
            .onPreferenceChange(StepWidth.self) { available = $0 }
    }
}

private struct StepWidth: PreferenceKey {
    static var defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = nextValue() }
}

/// Under the answer, when the line has working: what shows it (the key), and a click does the same.
struct WorkingHint: View {
    @ObservedObject var model: BarModel
    @ObservedObject private var settings = AppSettings.shared

    var body: some View {
        let key = KeyActions.title(KeyActions.chord("working", in: settings.keybinds))
        Button { model.toggleWorking() } label: {
            Text(model.workingShown ? "hide working" : "show working\(key.isEmpty ? "" : " · \(key)")")
                .font(.system(size: Theme.caption))
                .foregroundStyle(Color.qMuted)
        }
        .buttonStyle(.plain)
        .hoverTip(model.workingShown ? "Hide the working" : "Show the working, to check it by hand")
        .accessibilityLabel(model.workingShown ? "Hide working" : "Show working")
    }
}
