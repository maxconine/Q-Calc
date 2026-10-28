import AppKit
import QCalcCore
import SwiftUI
import UniformTypeIdentifiers

// The bar, laid out as the web app's .spotlight is (src/index.css): the tape above, the 72 pt composer, then the
// hint, the graph and the message cards, on one solid --bg card with 12 pt corners. Every size, colour and
// spacing here comes from that CSS (Theme.swift).

/// The bar's size as SwiftUI lays it out: the whole height, and how far down the composer starts (so history
/// grows up and panels grow down, as applySize did for the page).
struct BarSize: Equatable {
    var height: CGFloat = 0
    var anchorTop: CGFloat = 0
}

private struct HeightKey: PreferenceKey {
    static var defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = max(value, nextValue()) }
}

private struct AnchorKey: PreferenceKey {
    static var defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = max(value, nextValue()) }
}

private extension View {
    func reportHeight<K: PreferenceKey>(_ key: K.Type) -> some View where K.Value == CGFloat {
        background(GeometryReader { Color.clear.preference(key: key, value: $0.size.height) })
    }
}

private let reduceMotion = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion

struct BarView: View {
    @ObservedObject var model: BarModel
    @ObservedObject private var settings = AppSettings.shared
    var onSize: (BarSize) -> Void
    @State private var anchor: CGFloat = 0
    @State private var lastHeight: CGFloat = 0

    var body: some View {
        VStack(spacing: 0) {
            above
                .reportHeight(AnchorKey.self)
            Composer(model: model)
            below
        }
        .frame(maxWidth: .infinity, alignment: .top)
        .background(Color.qBg.opacity(Theme.panelOpacity))
        .clipShape(RoundedRectangle(cornerRadius: Theme.radius, style: .continuous))
        // a file dropped on the bar goes to whoever the line names (Messages/ShareCommands.swift); a thread
        // under it takes its own drops
        .onDrop(of: [.fileURL], isTargeted: Binding(get: { model.dragHint != nil }, set: { model.dragHover($0) })) {
            model.dropProviders($0, to: nil)
        }
        .fixedSize(horizontal: false, vertical: true)
        .reportHeight(HeightKey.self)
        .onPreferenceChange(HeightKey.self) { h in
            lastHeight = h
            onSize(BarSize(height: h, anchorTop: anchor))
        }
        .onPreferenceChange(AnchorKey.self) { a in
            anchor = a
            onSize(BarSize(height: lastHeight, anchorTop: a))
        }
    }

    // what sits above the line (.tape): the cheat sheet, the games list, or the history
    @ViewBuilder private var above: some View {
        if model.helpShown {
            TapeBox { HelpSheet(logic: model.logic) }
        } else if model.gamesShown {
            TapeBox { GamesSheet(model: model) }
        } else if !model.visibleRange.isEmpty {
            TapeBox { HistoryTape(model: model) }
        }
    }

    @ViewBuilder private var below: some View {
        if let letters = model.live.timeLetters, !letters.varying.isEmpty || !letters.constant.isEmpty, model.graph == nil {
            TimeLetterChips(model: model, letters: letters)
        }
        if model.hint == nil, let line = model.tourLine {
            // .tour-line: the step, `n/m` at 70%, and an underlined skip, clear of the clear button's corner
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                // native only: a narrow bar wraps the step to a second line rather than cutting off what to type
                Text(line.text).lineLimit(2).truncationMode(.tail).fixedSize(horizontal: false, vertical: true)
                if line.fillable { Text("· tab fills it").lineLimit(1).opacity(0.7) }
                Text("\(line.at)/\(line.of)").monospacedDigit().opacity(0.7)
                Spacer(minLength: 0)
                Button { model.skipTutorial() } label: { Text("skip").underline() }
                    .buttonStyle(.plain)
                    .hoverTip("skip the tutorial (or type skip)")
            }
            .font(.system(size: Theme.small))
            .foregroundStyle(Color.qMuted)
            .padding(.leading, Theme.gutter)
            .padding(.trailing, Theme.gutter + 44)
            .padding(.bottom, 12)
            .padding(.top, -12)
            .accessibilityElement(children: .contain)
            .accessibilityLabel("Tutorial step \(line.at) of \(line.of): \(line.text)")
        } else if let hint = model.hint, !hint.isEmpty {
            // .composer-hint: 12 pt muted, tucked 12 pt up into the composer's bottom padding
            Text(hint)
                .font(.system(size: Theme.small))
                .lineSpacing(2)
                .foregroundStyle(Color.qMuted)
                // native only: two lines on a narrow bar (the page cuts it at one), so a why or a how isn't lost
                .lineLimit(2)
                .fixedSize(horizontal: false, vertical: true)
                .truncationMode(.tail)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, Theme.gutter)
                .padding(.bottom, 12)
                .padding(.top, -12)
                .hoverTip(hint)
                .accessibilityLabel(hint)
        }
        // the one-time line on what's end-to-end encrypted (Messages/ShareViews.swift)
        PrivacyNoticeLine()
        if let typeset = model.typeset {
            MathPreview(typeset: typeset)
                .frame(maxWidth: .infinity)
                .padding(.horizontal, Theme.gutter)
                .padding(.bottom, 8)
        }
        if model.sysLines != nil {
            SystemPanel(model: model)
        }
        if let graph = model.graph {
            GraphPanel(model: model, state: graph)
        }
        if model.messagesPanel != nil {
            MessagesPanelView(model: model)
        }
        if !model.cards.isEmpty {
            MessageCards(model: model)
        }
    }
}

/// .tape: the tape's own colour, a hairline under it, 6 pt by 8 pt in
private struct TapeBox<Content: View>: View {
    @ViewBuilder var content: Content
    var body: some View {
        content
            .padding(.vertical, 6)
            .padding(.horizontal, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.qBgTape.opacity(Theme.panelOpacity))
            .overlay(alignment: .bottom) { Rectangle().fill(Color.qLine).frame(height: 1) }
    }
}

// MARK: - the line and its answer (.composer)

private struct Composer: View {
    @ObservedObject var model: BarModel
    @State private var hover = false

    var body: some View {
        // grid-template-columns: minmax(33%, 1fr) minmax(120px, max-content); gap 12; padding 8 16
        GeometryReader { geo in
            let inner = geo.size.width - Theme.gutter * 2
            HStack(alignment: .center, spacing: 12) {
                field
                    .frame(minWidth: inner * 0.33, maxWidth: .infinity, minHeight: 36, maxHeight: 36)
                    .layoutPriority(1)
                LiveAnswerView(model: model)
                    .frame(minWidth: 120, alignment: .trailing)
                    .layoutPriority(2)
                    // where the black hole and the π digits find the answer (window content, top-left origin)
                    .background(GeometryReader { g in
                        Color.clear
                            .onAppear { model.answerFrame = g.frame(in: .global) }
                            .onChange(of: g.frame(in: .global)) { model.answerFrame = $0 }
                    })
            }
            .padding(.vertical, 8)
            .padding(.horizontal, Theme.gutter)
            .frame(width: geo.size.width, height: Theme.composerHeight)
        }
        .frame(height: Theme.composerHeight)
        .overlay(alignment: .leading) {
            // Pi Day (native only): a small slice of pie in the gutter beside the line, all March 14
            if BarModel.piDay { PieSlice().padding(.leading, 3).offset(y: -1) }
        }
        .overlay {
            EdgeTools(model: model, angleRad: model.angleRad, fractions: model.fractions, sigFigMode: model.sigFigMode,
                      exponentMode: model.exponentMode, shown: hover || model.edgeFocus)
                .equatable()
        }
        .onHover { hover = $0 }
    }

    private var field: some View {
        HStack(spacing: 0) {
            // .quick-chain: the faint `ans` a chained line continues from
            if model.chained {
                Text("ans")
                    .font(.system(size: Theme.display))
                    .tracking(Theme.displayTracking(Theme.display))
                    .foregroundStyle(Color.qMuted)
                    .opacity(0.75)
                    // native only: no gap after it, so Tab / → typing it in (`ans*2`) moves nothing
                    .hoverTip(model.ansHover)
                    .accessibilityLabel("continues from the last answer")
            }
            ZStack(alignment: .leading) {
                if model.text.isEmpty, let example = model.example {
                    // .quick-example: the rotating example, muted, fading in and out over 2.5 s
                    ExampleFade(id: example.id) {
                        RadicalText(text: Symbols.pretty(example.expr, caret: (example.expr as NSString).length, keepWords: false).text,
                                    size: Theme.display, tracking: Theme.displayTracking(Theme.display))
                            .foregroundStyle(Color.qMuted)
                    }
                    .allowsHitTesting(false)
                    .accessibilityLabel("Example: \(example.expr)")
                } else if model.text.isEmpty {
                    // .quick-plain::placeholder: muted at 80%
                    Text("Calculate")
                        .font(.system(size: Theme.display))
                        .tracking(Theme.displayTracking(Theme.display))
                        .foregroundStyle(Color.qMuted.opacity(0.8))
                        .allowsHitTesting(false)
                        .accessibilityHidden(true)
                }
                InputField(model: model, placeholder: "")
            }
        }
    }
}

/// Pi Day's slice: a 60° wedge with its crust, muted, never in the way of a click
private struct PieSlice: View {
    var body: some View {
        ZStack {
            Path { p in
                p.move(to: CGPoint(x: 1, y: 8.5))
                p.addArc(center: CGPoint(x: 1, y: 8.5), radius: 8.5, startAngle: .degrees(-60), endAngle: .degrees(0), clockwise: false)
                p.closeSubpath()
            }
            .fill(Color(red: 0.93, green: 0.72, blue: 0.42))
            Path { p in
                p.addArc(center: CGPoint(x: 1, y: 8.5), radius: 7.8, startAngle: .degrees(-60), endAngle: .degrees(0), clockwise: false)
            }
            .stroke(Color(red: 0.72, green: 0.45, blue: 0.2), style: StrokeStyle(lineWidth: 1.4, lineCap: .round))
        }
        .frame(width: 10, height: 9)
        .opacity(0.75)
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

struct LiveAnswerView: View {
    @ObservedObject var model: BarModel

    var body: some View {
        HStack(spacing: 0) {
            if model.copied { CopiedCheck() }
            content
        }
        .lineLimit(1)
        // the black hole has it for a moment (EggOverlay `void`); the answer underneath never changes
        .opacity(model.answerVeiled ? 0 : 1)
    }

    @ViewBuilder private var content: some View {
        if let part = model.selectionAnswer, !model.text.isEmpty {
            // .live.live-selection: the selected part's answer, muted, not a button
            AnswerText(text: model.pretty(part), size: Theme.display)
                .foregroundStyle(Color.qMuted)
                .padding(.vertical, 2).padding(.horizontal, 8).padding(.trailing, -8)
                .accessibilityLabel("Selection: \(part)")
        } else if model.text.isEmpty, let example = model.example, !example.answer.isEmpty, !model.copied {
            // .live-example: the example's answer, muted at 70%, fading with it
            ExampleFade(id: example.id) {
                AnswerText(text: Pretty.answer(example.answer), size: Theme.display)
                    .foregroundStyle(Color.qMuted.opacity(0.7))
                    .padding(.vertical, 2).padding(.horizontal, 8).padding(.trailing, -8)
            }
            .accessibilityHidden(true)
        } else if model.sysLines != nil, let sys = model.sysAnswer {
            // the system's solution, or why there is none
            if sys.message {
                Text(sys.display).font(.system(size: Theme.body)).foregroundStyle(Color.qMuted)
                    .padding(.vertical, 2).padding(.horizontal, 8).padding(.trailing, -8)
            } else {
                AnswerButton(model: model, shown: Pretty.answer(sys.display), copy: sys.display, size: Theme.display,
                             help: "Copy to clipboard · ⌘C also copies", trailingPad: -8)
            }
        } else if let message = model.commandMessage, !message.isEmpty {
            // .live.message: what a command does, 13 pt muted, in the answer's place
            Text(message)
                .font(.system(size: Theme.body))
                .foregroundStyle(Color.qMuted)
                .truncationMode(.tail)  // .live: text-overflow: ellipsis, so the start (what is wrong) stays
                .padding(.vertical, 2).padding(.horizontal, 8).padding(.trailing, -8)
                // a narrow bar cuts its end off; the pointer shows it whole (native only)
                .hoverTip(message)
        } else if model.live.isBlank, let steady = model.steady {
            // .live-steady: never the answer's colour, gone after a pause
            AnswerText(text: model.pretty(steady), size: Theme.display)
                .foregroundStyle(Color.qMuted.opacity(0.5))
                .padding(.vertical, 2).padding(.horizontal, 8).padding(.trailing, -8)
                .accessibilityHidden(true)
        } else if let solve = model.live.solve, solve.outcome != .roots {
            // a solve with no roots says so, as a message
            Text(model.live.display)
                .font(.system(size: Theme.body))
                .foregroundStyle(Color.qMuted)
                .padding(.vertical, 2).padding(.horizontal, 8).padding(.trailing, -8)
        } else if let solve = model.live.solve, solve.roots.count > 1, !model.hasDual {
            SolveRoots(model: model, solve: solve)
        } else if model.live.improper {
            Text(model.live.display)
                .font(.system(size: Theme.body))
                .foregroundStyle(Color.qMuted)
        } else if model.hasDual, let exact = model.exact {
            // .live-dual: two 20 pt halves around a 17 pt ≈, each its own copy
            HStack(spacing: 0) {
                AnswerButton(model: model, shown: Pretty.answer(exact), copy: exact, size: Theme.answerPart, help: "Copy exact value", trailingPad: 8)
                Text("≈").font(.system(size: Theme.row)).foregroundStyle(Color.qMuted).accessibilityHidden(true)
                AnswerButton(model: model, shown: model.pretty(model.exponentShown(model.live.display)), copy: model.live.display, size: Theme.answerPart, help: "Copy approximation", trailingPad: -8)
            }
            .accessibilityElement(children: .contain)
            .accessibilityLabel("Answer")
        } else if !model.live.isBlank {
            AnswerButton(model: model, shown: model.pretty(model.shownAnswer), copy: model.shownAnswer, size: Theme.display,
                         help: "Copy to clipboard · ⌘C also copies", trailingPad: -8)
                .id(model.formIndex)
        }
    }
}

/// .live-roots: the letter (copies them all), then each root its own copy at full precision, `, …` when there are more
private struct SolveRoots: View {
    @ObservedObject var model: BarModel
    let solve: SolveInfo

    var body: some View {
        let figs = AppSettings.shared.significantFigures
        let shown = model.live.display.components(separatedBy: ", ").filter { $0 != "…" }
        let labels = shown.count == solve.roots.count ? shown : solve.roots.map { String(format: "%.\(figs)g", $0) }
        HStack(alignment: .firstTextBaseline, spacing: 0) {
            Button {
                model.copyAnswer(model.live.display)
            } label: {
                Text("\(solve.variable) =").font(.system(size: Theme.answerPart)).foregroundStyle(Color.qMuted)
                    .padding(.horizontal, 2).padding(.trailing, 0.2 * Theme.answerPart)
            }
            .buttonStyle(.plain)
            .hoverTip("Copy all the solutions")
            ForEach(Array(solve.roots.enumerated()), id: \.offset) { k, root in
                if k > 0 { Text(", ").font(.system(size: Theme.display)).foregroundStyle(Color.qAnswer) }
                RootButton(model: model, shown: Pretty.answer(labels[k]), copy: String(format: "%.17g", root))
            }
            if solve.more { Text(", …").font(.system(size: Theme.display)).foregroundStyle(Color.qAnswer) }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Solutions")
    }
}

private struct RootButton: View {
    @ObservedObject var model: BarModel
    let shown: String
    let copy: String
    @State private var hover = false
    var body: some View {
        Button { model.copyAnswer(copy) } label: {
            AnswerText(text: shown, size: Theme.display)
                .foregroundStyle(hover ? Color.qAnswerHover : Color.qAnswer)
                .padding(.vertical, 2).padding(.horizontal, 3)
                .background(RoundedRectangle(cornerRadius: 4).fill(hover ? Color.qAnswer.opacity(0.12) : .clear))
        }
        .buttonStyle(.plain)
        .onHover { hover = $0 }
        .hoverTip("Copy this solution")
        .accessibilityLabel("Solution \(shown)")
    }
}

/// .time-letters: under a d/dt, which letters changed with time and which stayed constant; a chip moves one across
struct TimeLetterChips: View {
    @ObservedObject var model: BarModel
    let letters: TimeLetters

    var body: some View {
        HStack(spacing: 4) {
            if !letters.varying.isEmpty {
                Text("changing with time").foregroundStyle(Color.qMuted).padding(.trailing, 2)
                ForEach(letters.varying, id: \.self) { chip($0, varying: true) }
            }
            if !letters.varying.isEmpty, !letters.constant.isEmpty { Text("·").foregroundStyle(Color.qMuted) }
            if !letters.constant.isEmpty {
                Text("constant").foregroundStyle(Color.qMuted).padding(.trailing, 2)
                ForEach(letters.constant, id: \.self) { chip($0, varying: false) }
            }
            Spacer(minLength: 0)
        }
        .font(.system(size: Theme.small))
        .frame(minHeight: 18)
        .padding(.horizontal, Theme.gutter)
        .padding(.bottom, 12)
        .padding(.top, -12)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Which letters change with time")
    }

    private func chip(_ letter: String, varying: Bool) -> some View {
        let locked = letters.locked.contains(letter)
        return Button { model.setTimeVarying(letter, !varying) } label: {
            Text(letter)
                .foregroundStyle(varying ? Color.qAccent : Color.qMuted)
                .frame(minWidth: 22 - 14)
                .padding(.horizontal, 7)
                .frame(height: 20)
                .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(varying ? Color.qAccent.opacity(0.14) : Color.qFillHover))
                .opacity(locked ? 0.75 : 1)
        }
        .buttonStyle(.plain)
        .disabled(locked)
        .hoverTip(locked ? "\(letter) is written with a dot, so it changes with time"
              : varying ? "Treat \(letter) as a constant" : "Treat \(letter) as changing with time")
        .accessibilityLabel("\(letter): \(varying ? "changing with time" : "constant")\(locked ? "" : ". Click to switch")")
    }
}

/// qcalc-example: the rotating example, shown at once (native only: no fade in or out, it just changes)
private struct ExampleFade<Content: View>: View {
    let id: Int
    @ViewBuilder var content: Content

    var body: some View { content }
}

/// .live-check: "✓ copied", hanging left of the answer, there at once and gone at once by 1.2 s (native only: no fade)
private struct CopiedCheck: View {
    @State private var opacity: Double = 1
    var body: some View {
        Text("✓ copied")
            .font(.system(size: Theme.caption, weight: .medium))
            .tracking(0.02 * Theme.caption)
            .foregroundStyle(Color.qAccentHover)
            .padding(.vertical, 2).padding(.horizontal, 7)
            .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(Color.qBg))
            .padding(.trailing, 6)
            .opacity(opacity)
            .onAppear {
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { opacity = 0 }
            }
            .accessibilityLabel("Copied")
    }
}

private struct AnswerText: View {
    let text: String
    let size: CGFloat
    var body: some View {
        RadicalText(text: text, size: size, tracking: Theme.displayTracking(size), monospaced: true)
            .truncationMode(.tail)
            // useFitFont: a long answer shrinks to fit before it is cut
            .minimumScaleFactor(0.5)
    }
}

/// .live: the answer as a button — answer colour, hover lightens it and fills behind, 6 pt corners
private struct AnswerButton: View {
    @ObservedObject var model: BarModel
    let shown: String
    let copy: String
    let size: CGFloat
    let help: String
    let trailingPad: CGFloat
    @State private var hover = false

    var body: some View {
        Button {
            model.copyAnswer(copy)
            model.focusField?()
        } label: {
            AnswerText(text: shown, size: size)
                .foregroundStyle(model.copied || hover ? Color.qAnswerHover : Color.qAnswer)
                .padding(.vertical, 2)
                .padding(.horizontal, 8)
                .frame(minWidth: size == Theme.display ? 88 : 0, minHeight: 32, alignment: .trailing)
                .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(hover ? Color.qAnswer.opacity(0.09) : .clear))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.trailing, trailingPad < 0 ? trailingPad : 0)
        .onHover { hover = $0 }
        .hoverTip(help)
        .accessibilityLabel("Answer \(shown)")
        .accessibilityHint("Copies it")
        .onDrag { NSItemProvider(object: copy as NSString) }
    }
}

// MARK: - edge tools (EdgeTools.tsx): settings top left, deg · a/b · sf top right, clear bottom right; shown on hover

/// Not observing the model: the tools redraw when a toggle or the shortcuts change, not on every keystroke.
private struct EdgeTools: View, Equatable {
    let model: BarModel
    let angleRad: Bool
    let fractions: Bool
    let sigFigMode: Bool
    let exponentMode: Bool
    @ObservedObject private var settings = AppSettings.shared
    let shown: Bool

    static func == (a: Self, b: Self) -> Bool {
        a.angleRad == b.angleRad && a.fractions == b.fractions && a.sigFigMode == b.sigFigMode && a.exponentMode == b.exponentMode && a.shown == b.shown
    }

    var body: some View {
        ZStack {
            EdgeTool(label: "Settings", tip: tip("Open settings", "settings"), tipAbove: false, active: false, alignLeft: true, action: { model.onSettings?() }) {
                Image(systemName: "gearshape.fill").font(.system(size: 10.5, weight: .medium))
            }
            .padding(.horizontal, -3)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .padding(.leading, 6)

            HStack(spacing: 0) {
                EdgeTool(label: angleRad ? "Radians" : "Degrees",
                         tip: tip(angleRad ? "Radians · click for degrees" : "Degrees · click for radians", "angle"),
                         tipAbove: false, active: true, action: { model.toggleAngle() }) {
                    Text(angleRad ? "rad" : "deg")
                }
                EdgeTool(label: "Fraction results \(fractions ? "on" : "off")",
                         tip: tip("Answers as fractions: \(fractions ? "on" : "off")", "fraction"),
                         tipAbove: false, active: fractions, action: { model.toggleFractions() }) {
                    // .edge-frac-mark: a over b, 6.5 pt, with a 7 pt bar
                    VStack(spacing: 0.5) {
                        Text("a")
                        Rectangle().frame(width: 7, height: 1)
                        Text("b")
                    }
                    .font(.system(size: 6.5, weight: .semibold))
                    .padding(.vertical, -2)
                }
                EdgeTool(label: "Significant figures from input \(sigFigMode ? "on" : "off")",
                         tip: tip("Sig figs from what you type: \(sigFigMode ? "on" : "off")", "sigFigs"),
                         tipAbove: false, active: sigFigMode, action: { model.toggleSigFigMode() }) {
                    Text("sf")
                }
                // native only: ⌃E's exponent form
                EdgeTool(label: "Exponent form \(exponentMode ? "on" : "off")",
                         tip: tip("Answers in exponent form: \(exponentMode ? "on" : "off")", "exponent"),
                         tipAbove: false, active: exponentMode, action: { model.toggleExponent() }) {
                    Text("×10ⁿ")
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing)
            .padding(.trailing, 6)

            EdgeTool(label: "Clear history and variables", tip: tip("Clear history and variables", "clear"), tipAbove: true, active: false, action: { model.clearHistory() }) {
                Text("clear")
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)
            .padding(.trailing, 6)
        }
        .opacity(shown ? 1 : 0)
    }

    private func tip(_ text: String, _ action: String) -> String {
        let key = KeyActions.title(KeyActions.chord(action, in: settings.keybinds))
        return key.isEmpty ? text : "\(text) · \(key)"
    }
}

/// .edge-tool: 11 pt medium, 0.02 em tracking, padding 4 × 9, muted at 72% (an active toggle the answer colour),
/// with its tip after a 450 ms rest
private struct EdgeTool<Label: View>: View {
    let label: String
    let tip: String
    let tipAbove: Bool
    let active: Bool
    var alignLeft = false
    let action: () -> Void
    @ViewBuilder var content: Label
    @State private var hover = false
    @State private var tipShown = false
    @State private var pressed = false

    var body: some View {
        Button(action: action) {
            content
                .font(.system(size: Theme.caption, weight: .medium))
                .viewTracking(0.02 * Theme.caption)
                .foregroundStyle(active || (hover && alignLeft) ? Color.qAnswer : hover ? Color.qMuted : Color.qMuted.opacity(0.72))
                .padding(.vertical, 4)
                .padding(.horizontal, alignLeft ? 6 : 9)
                .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(pressed ? Color.qFillSelected : hover ? Color.qFillHover : .clear))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { on in
            hover = on
            if tipShown { HoverTip.onScreen() }
            tipShown = false
            guard on else { return }
            let wait = HoverTip.wait
            if wait == 0 {
                tipShown = true
                HoverTip.onScreen()
            } else {
                DispatchQueue.main.asyncAfter(deadline: .now() + wait) {
                    if hover { tipShown = true; HoverTip.onScreen() }
                }
            }
        }
        .overlay(alignment: tipAbove ? .topTrailing : (alignLeft ? .bottomLeading : .bottomTrailing)) {
            if tipShown {
                // ::after: ink pill with --bg text, 4 pt off the button
                Text(tip)
                    .font(.system(size: Theme.caption, weight: .medium))
                    .foregroundStyle(Color.qBg)
                    .fixedSize()
                    .padding(.vertical, 4).padding(.horizontal, 7)
                    .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(Color.qInk)
                        .shadow(color: .black.opacity(0.25), radius: 3, y: 1))
                    .compositingGroup()
                    .alignmentGuide(tipAbove ? .top : .bottom) { d in tipAbove ? d[.bottom] + 4 : d[.top] - 4 }
                    .allowsHitTesting(false)
                    .zIndex(10)
            }
        }
        .accessibilityLabel(label)
        .accessibilityHint(tip)
        .accessibilityAddTraits(active ? .isSelected : [])
    }
}

// MARK: - the tape (HistoryTape.tsx)

private struct HistoryTape: View {
    @ObservedObject var model: BarModel

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView(.vertical, showsIndicators: model.visibleRange.count > Theme.tapeVisible) {
                LazyVStack(spacing: 0) {
                    ForEach(Array(model.visibleRange), id: \.self) { i in
                        HistoryRowView(model: model, index: i, row: model.rows[i], selected: model.selected == i,
                                       picked: model.selected == i ? model.pick : nil)
                            .equatable()
                            .id(i)
                    }
                }
            }
            .frame(height: CGFloat(min(model.visibleRange.count, Theme.tapeVisible)) * Theme.tapeRow)
            .onChange(of: model.selected) { i in
                if let i { proxy.scrollTo(i) }
            }
            .onAppear { proxy.scrollTo(model.rows.count - 1, anchor: .bottom) }
            .onChange(of: model.rows.count) { _ in proxy.scrollTo(model.rows.count - 1, anchor: .bottom) }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Calculation history")
    }
}

/// Not observing the model: a row redraws only when what it shows changes (`.equatable()`), not on every keystroke.
private struct HistoryRowView: View, Equatable {
    let model: BarModel
    let index: Int
    let row: HistoryRow
    let selected: Bool
    // which part ← → picked: 0 the expression, 1 the first answer, 2 the second of a dual
    let picked: Int?
    @State private var hover = false
    @State private var exprHover = false

    static func == (a: Self, b: Self) -> Bool {
        a.index == b.index && a.row == b.row && a.selected == b.selected && a.picked == b.picked
    }

    var body: some View {
        // grid: 1fr | minmax(120px, 46%), gap 12, padding 0 8, 36 pt rows with 6 pt corners
        GeometryReader { geo in
            HStack(spacing: 12) {
                Button { model.insertFromRow(index, answer: false) } label: {
                    RadicalText(text: row.expr.replacingOccurrences(of: "sqrt(", with: "√("), size: Theme.body)
                        .foregroundStyle(exprHover || picked == 0 ? Color.qInk : Color.qMuted)
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .padding(.vertical, 4)
                        .padding(.horizontal, picked == 0 ? 4 : 0)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(PickedMark(on: picked == 0))
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .onHover { exprHover = $0 }
                // HistoryTape.tsx's titles; the pointer also reads an expression cut off at the row's end in full
                .hoverTip(exprTip)
                .accessibilityLabel("Expression \(row.expr)")

                answers
                    .frame(minWidth: 120, maxWidth: max(120, (geo.size.width - 16) * 0.46), alignment: .trailing)
            }
            .padding(.horizontal, 8)
            .frame(height: Theme.tapeRow)
            .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(selected ? Color.qFillSelected : hover ? Color.qFillHover : .clear))
        }
        .frame(height: Theme.tapeRow)
        .onHover { hover = $0 }
        .contextMenu {
            Button("Copy Expression") { model.copyRow(row, answer: false) }
            Button("Copy Answer") { model.copyRow(row, answer: true) }
            Divider()
            Button("Delete") {
                model.select(index)
                _ = model.deleteSelected()
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("\(row.expr) equals \(row.display)")
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    private var exprTip: String {
        let what = row.kind == .system ? "Edit equations" : row.kind == .definition ? "Insert word at the cursor"
            : "Insert expression at the cursor"
        return row.expr.count > 40 ? "\(what) · \(row.expr)" : what
    }

    @ViewBuilder private var answers: some View {
        if row.hasDual, let exact = row.exact {
            // the page's titles: exact value, then approximation
            // .tape-a-dual: exact ≈ approx, 6 pt apart, the ≈ in 13 pt muted
            HStack(spacing: 6) {
                answer(Pretty.answer(exact), insert: true, picked: picked == 1, tip: "Insert exact value at the cursor")
                Text("≈").font(.system(size: Theme.body)).foregroundStyle(Color.qMuted).accessibilityHidden(true)
                answer(Pretty.answer(row.display), insert: true, picked: picked == 2, tip: "Insert approximation at the cursor")
            }
        } else {
            answer(Pretty.answer(row.display), insert: true, picked: picked == 1, fill: true,
                   tip: row.kind == .definition ? "Insert word at the cursor"
                       : AppSettings.shared.answerForm == "exact" && row.exact != nil ? "Insert exact value at the cursor"
                       : "Insert approximation at the cursor")
        }
    }

    private func answer(_ text: String, insert: Bool, picked: Bool, fill: Bool = false,
                        tip: String = "Insert the answer at the cursor") -> some View {
        AnswerCell(text: text, picked: picked, fill: fill, tip: tip) { model.insertFromRow(index, answer: true) }
            .onDrag { NSItemProvider(object: row.display as NSString) }
    }
}

/// .tape-a: 17 pt answer colour, -0.015 em, lighter on hover
private struct AnswerCell: View {
    let text: String
    let picked: Bool
    /// a lone answer is the whole column (display: block), so its pick ring is too
    var fill = false
    var tip = "Insert the answer at the cursor"
    let action: () -> Void
    @State private var hover = false
    var body: some View {
        Button(action: action) {
            RadicalText(text: text, size: Theme.row, tracking: -0.015 * Theme.row, monospaced: true)
                .foregroundStyle(hover ? Color.qAnswerHover : Color.qAnswer)
                .lineLimit(1)
                .truncationMode(.tail)
                .padding(.vertical, 4)
                .padding(.horizontal, picked ? 4 : 0)
                .frame(maxWidth: fill ? .infinity : nil, alignment: .trailing)
                .background(PickedMark(on: picked))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hover = $0 }
        .hoverTip(tip)
        .accessibilityLabel("Answer \(text)")
    }
}

/// .tape-row.selected .picked: a 1 pt ring and a tint, both the answer colour
private struct PickedMark: View {
    let on: Bool
    var body: some View {
        if on {
            RoundedRectangle(cornerRadius: 4)
                .fill(Color.qAnswer.opacity(0.14))
                .overlay(RoundedRectangle(cornerRadius: 4).strokeBorder(Color.qAnswer.opacity(0.55), lineWidth: 1))
        }
    }
}

// MARK: - sheets above the bar (.cheats)

private struct GamesSheet: View {
    @ObservedObject var model: BarModel

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(BarCommand.gameList.enumerated()), id: \.offset) { i, game in
                Button { model.onGame?(game.kind) } label: {
                    // .cheat-row.game-row: 28 pt, the name column as wide as the longest name
                    HStack(spacing: 12) {
                        Text(game.name).font(.system(size: Theme.body, weight: .medium)).tracking(0.02 * Theme.body)
                            .foregroundStyle(Color.qInk).lineLimit(1).fixedSize()
                            .frame(width: 126, alignment: .leading)
                        Text(game.blurb).font(.system(size: Theme.body)).foregroundStyle(Color.qMuted).lineLimit(1)
                        Spacer(minLength: 0)
                    }
                    .padding(.horizontal, 8)
                    .frame(height: 28)
                    .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(i == model.gamePick ? Color.qFillHover : .clear))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Play \(game.name), \(game.blurb)")
                .accessibilityAddTraits(i == model.gamePick ? .isSelected : [])
            }
        }
        // aria-label="Games", each row aria-selected as ↑ ↓ pick it
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Games")
    }
}

/// `?` / `help`: the cheat sheet (cheatSheet + hostCheats), its keys following the user's keybinds
private struct HelpSheet: View {
    let logic: AppLogic
    @ObservedObject private var settings = AppSettings.shared

    private func key(_ action: String) -> String { KeyActions.title(KeyActions.chord(action, in: settings.keybinds)) }

    private var left: [(String, String)] {
        [
            ((settings.activeHotKey ?? settings.hotKey).title, "show / hide"),
            ("↵", "save"), ("↑", "history"),
            ([key("copyAnswer"), key("copyLine")].filter { !$0.isEmpty }.joined(separator: " "), "copy answer / line"),
            ("⌥↑ ⌥↓", "unit prefix"),
            (key("angle"), "degrees / radians"), (key("fraction"), "fractions"), (key("sigFigs"), "sig figs from input"),
            (key("clear"), "clear history"), (key("settings"), "settings"), ("esc", "hide"),
        ]
    }

    /// the web's things to type with the Mac app's own (messages, sharing, konami) before `?` (core `nativeCheats`)
    private var right: [(String, String)] {
        if let rows = logic.call("lib/onboarding", "nativeCheats", []) as? [[String]], !rows.isEmpty {
            // the page collapses the core's double spaces (`GCF  LCM`), so this does too
            return rows.compactMap { $0.count == 2 ? ($0[0].replacingOccurrences(of: "  ", with: " "), $0[1]) : nil }
        }
        return [
            ("ft to cm", "units"), ("80 + 15%", "percent"), ("+/- -/+", "± ∓ uncertainty"), ("x = 5", "variables"),
            ("x^2 = 2", "solve"), ("sys 2", "systems"), ("graph x^2", "plot"), ("GCF LCM", "factors"),
            ("sum n, n=1..9", "Σ sums"), ("tutorial", "the walkthrough"), ("?", "this list"),
        ]
    }

    var body: some View {
        Group {
            if #available(macOS 13, *) {
                // native only: a bar too narrow for both columns' labels (they'd end in "…" at 340 pt) lists them
                // in one column, the keys first, scrolling past 12 rows
                ViewThatFits(in: .horizontal) {
                    twoColumns
                    ScrollView(.vertical) { column(left + right, keyWidth: 96).padding(.vertical, 8) }
                        // past 12 rows, half of the 12th shows, so it reads as more below
                        .frame(height: left.count + right.count > 12 ? 11.5 * 28 + 8 : CGFloat(left.count + right.count) * 28 + 16)
                }
            } else {
                twoColumns
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Shortcuts")
    }

    private var twoColumns: some View {
        // .cheats: two columns 24 pt apart; key columns at least 64 and 96 pt plus 8, 12 pt to the label
        HStack(alignment: .top, spacing: 24) {
            column(left, keyWidth: 64)
            column(right, keyWidth: 96)
        }
        .padding(.vertical, 8)
    }

    // a subgrid in the web: the key column is as wide as its widest key, and every label lines up after it
    private func column(_ rows: [(String, String)], keyWidth: CGFloat) -> some View {
        Group {
            if #available(macOS 13, *) {
                Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 0) {
                    ForEach(Array(rows.enumerated()), id: \.offset) { _, r in
                        GridRow {
                            key(r.0).frame(minWidth: keyWidth, alignment: .leading)
                            label(r.1)
                        }
                        .frame(height: 28)
                        .accessibilityElement(children: .combine)
                    }
                }
            } else {
                KeyColumn(rows: rows, keyWidth: keyWidth, key: key, label: label)
            }
        }
        .padding(.horizontal, 8)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func key(_ s: String) -> some View {
        Text(s).font(.system(size: Theme.body, weight: .medium)).tracking(0.02 * Theme.body)
            .foregroundStyle(Color.qInk).lineLimit(1).fixedSize()
    }

    private func label(_ s: String) -> some View {
        Text(s).font(.system(size: Theme.body)).foregroundStyle(Color.qMuted).lineLimit(1)
    }
}

/// The shortcuts Grid on macOS 12 (no Grid): rows of key and label, the key column as wide as its widest key
/// (measured a pass later) and at least `keyWidth`.
private struct KeyColumn<Key: View, Label: View>: View {
    let rows: [(String, String)]
    let keyWidth: CGFloat
    let key: (String) -> Key
    let label: (String) -> Label
    @State private var widest: CGFloat = 0

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(rows.enumerated()), id: \.offset) { _, r in
                HStack(spacing: 12) {
                    key(r.0)
                        .background(GeometryReader { g in Color.clear.preference(key: WidestKey.self, value: g.size.width) })
                        .frame(minWidth: max(keyWidth, widest), alignment: .leading)
                    label(r.1)
                }
                .frame(height: 28)
                .accessibilityElement(children: .combine)
            }
        }
        .onPreferenceChange(WidestKey.self) { widest = $0 }
    }
}

private struct WidestKey: PreferenceKey {
    static var defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = max(value, nextValue()) }
}

// MARK: - message cards (MessageNotices.tsx, .message-banner)

private struct MessageCards: View {
    @ObservedObject var model: BarModel

    var body: some View {
        VStack(spacing: 0) {
            ForEach(Array(model.cards.enumerated()), id: \.element.id) { i, card in
                if i > 0 { Rectangle().fill(Color.qLine).frame(height: 1) }
                CardView(model: model, card: card)
            }
        }
        .font(.system(size: Theme.body))
        .background(Color.qBgTape)
        .overlay(alignment: .top) { Rectangle().fill(Color.qLine).frame(height: 1) }
        .onHover { model.cardsHovered = $0 }
    }
}

private struct CardAct: View {
    let title: String
    let action: () -> Void
    @State private var hover = false
    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: Theme.small))
                .foregroundStyle(hover ? Color.qAnswer : Color.qMuted)
                .padding(.horizontal, 8)
                .frame(height: 22)
                .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(hover ? Color.qFillSelected : .clear))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hover = $0 }
    }
}

private struct CardView: View {
    @ObservedObject var model: BarModel
    let card: BannerCard
    @FocusState private var focused: Bool

    private static let control = Color(nsColor: NSColor(name: nil) { a in
        a.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua ? NSColor(srgbRed: 0x58 / 255, green: 0x58 / 255, blue: 0x5c / 255, alpha: 1) : .white
    })

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Text(card.from).fontWeight(.semibold).foregroundStyle(Color.qInk).lineLimit(1)
                if card.ask {
                    Text("wants to message you").foregroundStyle(Color.qMuted).lineLimit(1)
                    Spacer(minLength: 0)
                    CardAct(title: "Allow") { model.answerAsk(card.id, allow: true) }
                    CardAct(title: "Block") { model.answerAsk(card.id, allow: false) }
                } else {
                    Spacer(minLength: 0)
                    CardAct(title: "×") { model.dismissCard(card.id) }.accessibilityLabel("Dismiss")
                }
            }
            .frame(minHeight: 22)
            // native only: a click on a message card's name or words opens the thread with them
            // (Messages/MessageThreads.swift); the buttons and the reply line keep their own clicks
            .contentShape(Rectangle())
            .onTapGesture { if !card.ask { model.openThread(card.from) } }
            if card.ask {
                // a request's files show, without Save until they're allowed (MessageFiles.swift)
                ForEach(MessageFiles.shared.heldFiles(from: card.from), id: \.id) { file in
                    FileBlock(file: file, mine: false, who: card.from)
                }
                if MessageFiles.shared.heldClips(from: card.from) > 0 {
                    Text("shared their clipboard").font(.system(size: Theme.small)).foregroundStyle(Color.qMuted)
                }
            }
            if !card.ask {
                if let clip = card.clip {
                    ClipBlock(clip: clip, who: card.from, lines: 4)
                } else if let file = card.file {
                    FileBlock(file: file, mine: false, who: card.from)
                } else {
                    Text(card.text)
                        .foregroundStyle(Color.qAnswer)
                        .lineLimit(4)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
                        .onTapGesture { model.openThread(card.from) }
                        .hoverTip("Open messages with \(card.from)")
                }
                HStack(spacing: 8) {
                    TextField("", text: Binding(get: { card.draft }, set: { model.setDraft(card.id, $0) }))
                        .textFieldStyle(.plain)
                        .font(.system(size: Theme.small))
                        .mutedPrompt("Reply to \(card.from)", shown: card.draft.isEmpty, size: Theme.small)
                        .foregroundStyle(Color.qInk)
                        .padding(.vertical, 4)
                        .padding(.horizontal, 8)
                        .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(Self.control)
                            .shadow(color: .black.opacity(0.1), radius: 1, y: 1))
                        .overlay(RoundedRectangle(cornerRadius: Theme.radiusS)
                            .strokeBorder(focused ? Color.qAccent : Color.black.opacity(0.12), lineWidth: focused ? 2 : 0.5))
                        .noWritingTools()
                        .tint(Color.qInk)
                        .focused($focused)
                        .onSubmit { model.sendReply(card.id) }
                        .disabled(card.reply == .sent)
                        .accessibilityLabel("Reply to \(card.from)")
                    switch card.reply {
                    case .sending: Text("sending…").font(.system(size: Theme.small)).foregroundStyle(Color.qMuted)
                    case .sent: Text("sent").font(.system(size: Theme.small)).foregroundStyle(Color.qAccent)
                    case let .failed(note): Text(note).font(.system(size: Theme.small)).foregroundStyle(Color.qMuted).lineLimit(1)
                    case .idle: EmptyView()
                    }
                }
                .padding(.top, 4)
            }
        }
        .padding(.top, 8)
        .padding(.horizontal, Theme.gutter)
        .padding(.bottom, card.ask ? 8 : 12)
        .onChange(of: focused) { on in
            if on { model.replyFocused = card.id } else if model.replyFocused == card.id { model.replyFocused = nil }
        }
        // Tab on an empty line (BarModel.tabToCard)
        .onChange(of: model.cardFocusRequest) { id in
            guard id == card.id else { return }
            focused = true
            model.cardFocusRequest = nil
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(card.ask ? "\(card.from) wants to message you" : "Message from \(card.from): \(card.text)")
        // the click on the name or words that opens the thread, for VoiceOver and Full Keyboard Access
        .accessibilityAction(named: "Open messages with \(card.from)") { if !card.ask { model.openThread(card.from) } }
    }
}
