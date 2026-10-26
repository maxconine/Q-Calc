import AppKit
import QCalcCore
import SwiftUI

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
        .background(Color.qBg.opacity(0.9))
        .clipShape(RoundedRectangle(cornerRadius: Theme.radius, style: .continuous))
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
            TapeBox { HelpSheet() }
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
        if let hint = model.hint, !hint.isEmpty {
            // .composer-hint: 12 pt muted, tucked 12 pt up into the composer's bottom padding
            Text(hint)
                .font(.system(size: Theme.small))
                .lineSpacing(2)
                .foregroundStyle(Color.qMuted)
                .lineLimit(1)
                .truncationMode(.tail)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, Theme.gutter)
                .padding(.bottom, 12)
                .padding(.top, -12)
                .accessibilityLabel(hint)
        }
        if settings.typstPreview, model.graph == nil, !model.live.isBlank, MathPreview.useful(model.text),
           model.math.render(expression: model.text, answer: model.shownAnswer, scale: 2, dark: false) != nil {
            MathPreview(expression: model.text, answer: model.shownAnswer, renderer: model.math)
                .frame(maxWidth: .infinity)
                .padding(.horizontal, Theme.gutter)
                .padding(.bottom, 8)
        }
        if let graph = model.graph {
            GraphPanel(model: model, state: graph)
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
            .background(Color.qBgTape.opacity(0.9))
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
            }
            .padding(.vertical, 8)
            .padding(.horizontal, Theme.gutter)
            .frame(width: geo.size.width, height: Theme.composerHeight)
        }
        .frame(height: Theme.composerHeight)
        .overlay { EdgeTools(model: model, shown: hover || model.edgeFocus) }
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
                    .padding(.trailing, 0.22 * Theme.display)
                    .accessibilityLabel("continues from the last answer")
            }
            ZStack(alignment: .leading) {
                if model.text.isEmpty {
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

struct LiveAnswerView: View {
    @ObservedObject var model: BarModel

    var body: some View {
        HStack(spacing: 0) {
            if model.copied { CopiedCheck() }
            content
        }
        .lineLimit(1)
    }

    @ViewBuilder private var content: some View {
        if let message = model.commandMessage, !message.isEmpty {
            // .live.message: what a command does, 13 pt muted, in the answer's place
            Text(message)
                .font(.system(size: Theme.body))
                .foregroundStyle(Color.qMuted)
                .truncationMode(.head)
                .padding(.vertical, 2).padding(.horizontal, 8).padding(.trailing, -8)
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
                AnswerButton(model: model, shown: model.pretty(model.live.display), copy: model.live.display, size: Theme.answerPart, help: "Copy approximation", trailingPad: -8)
            }
            .accessibilityElement(children: .contain)
            .accessibilityLabel("Answer")
        } else if !model.live.isBlank {
            AnswerButton(model: model, shown: model.pretty(model.shownAnswer), copy: model.shownAnswer, size: Theme.display,
                         help: "Copy to clipboard · ⌘C also copies", trailingPad: -8)
                .id(model.formIndex)
                .transition(.opacity)
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
            .help("Copy all the solutions")
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
        .help("Copy this solution")
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
        .help(locked ? "\(letter) is written with a dot, so it changes with time"
              : varying ? "Treat \(letter) as a constant" : "Treat \(letter) as changing with time")
        .accessibilityLabel("\(letter): \(varying ? "changing with time" : "constant")\(locked ? "" : ". Click to switch")")
    }
}

/// .live-check: "✓ copied", hanging left of the answer, in for 120 ms, held, gone by 1.2 s
private struct CopiedCheck: View {
    @State private var opacity: Double = reduceMotion ? 1 : 0
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
                guard !reduceMotion else { return }
                withAnimation(.easeOut(duration: 0.12)) { opacity = 1 }
                withAnimation(.easeOut(duration: 0.48).delay(0.72)) { opacity = 0 }
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
        .animation(.easeOut(duration: 0.12), value: hover)
        .help(help)
        .accessibilityLabel("Answer \(shown)")
        .accessibilityHint("Copies it")
        .onDrag { NSItemProvider(object: copy as NSString) }
    }
}

// MARK: - edge tools (EdgeTools.tsx): settings top left, deg · a/b · sf top right, clear bottom right; shown on hover

private struct EdgeTools: View {
    @ObservedObject var model: BarModel
    @ObservedObject private var settings = AppSettings.shared
    let shown: Bool

    var body: some View {
        ZStack {
            EdgeTool(label: "Settings", tip: tip("Open settings", "settings"), tipAbove: false, active: false, alignLeft: true, action: { model.onSettings?() }) {
                Image(systemName: "gearshape.fill").font(.system(size: 10.5, weight: .medium))
            }
            .padding(.horizontal, -3)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .padding(.leading, 6)

            HStack(spacing: 0) {
                EdgeTool(label: model.angleRad ? "Radians" : "Degrees",
                         tip: tip(model.angleRad ? "Radians · click for degrees" : "Degrees · click for radians", "angle"),
                         tipAbove: false, active: true, action: { model.toggleAngle() }) {
                    Text(model.angleRad ? "rad" : "deg")
                }
                EdgeTool(label: "Fraction results \(model.fractions ? "on" : "off")",
                         tip: tip("Answers as fractions: \(model.fractions ? "on" : "off")", "fraction"),
                         tipAbove: false, active: model.fractions, action: { model.toggleFractions() }) {
                    // .edge-frac-mark: a over b, 6.5 pt, with a 7 pt bar
                    VStack(spacing: 0.5) {
                        Text("a")
                        Rectangle().frame(width: 7, height: 1)
                        Text("b")
                    }
                    .font(.system(size: 6.5, weight: .semibold))
                    .padding(.vertical, -2)
                }
                EdgeTool(label: "Significant figures from input \(model.sigFigMode ? "on" : "off")",
                         tip: tip("Sig figs from what you type: \(model.sigFigMode ? "on" : "off")", "sigFigs"),
                         tipAbove: false, active: model.sigFigMode, action: { model.toggleSigFigMode() }) {
                    Text("sf")
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
        .animation(.easeInOut(duration: 0.15), value: shown)
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
                .tracking(0.02 * Theme.caption)
                .foregroundStyle(active || (hover && alignLeft) ? Color.qAnswer : hover ? Color.qMuted : Color.qMuted.opacity(0.72))
                .padding(.vertical, 4)
                .padding(.horizontal, alignLeft ? 6 : 9)
                .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(pressed ? Color.qFillSelected : hover ? Color.qFillHover : .clear))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { on in
            hover = on
            tipShown = false
            if on { DispatchQueue.main.asyncAfter(deadline: .now() + 0.45) { if hover { tipShown = true } } }
        }
        .animation(.easeOut(duration: 0.12), value: hover)
        .overlay(alignment: tipAbove ? .topTrailing : (alignLeft ? .bottomLeading : .bottomTrailing)) {
            if tipShown {
                // ::after: ink pill with --bg text, 4 pt off the button
                Text(tip)
                    .font(.system(size: Theme.caption, weight: .medium))
                    .foregroundStyle(Color.qBg)
                    .fixedSize()
                    .padding(.vertical, 4).padding(.horizontal, 7)
                    .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(Color.qInk))
                    .alignmentGuide(tipAbove ? .top : .bottom) { d in tipAbove ? d[.bottom] + 4 : d[.top] - 4 }
                    .allowsHitTesting(false)
                    .transition(.opacity)
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
                        HistoryRowView(model: model, index: i, row: model.rows[i]).id(i)
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

private struct HistoryRowView: View {
    @ObservedObject var model: BarModel
    let index: Int
    let row: HistoryRow
    @State private var hover = false
    @State private var exprHover = false

    private var selected: Bool { model.selected == index }
    // which part ← → picked: 0 the expression, 1 the first answer, 2 the second of a dual
    private var picked: Int? { selected ? model.pick : nil }

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
                .help("Insert the expression")
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

    @ViewBuilder private var answers: some View {
        if row.hasDual, let exact = row.exact {
            // .tape-a-dual: exact ≈ approx, 6 pt apart, the ≈ in 13 pt muted
            HStack(spacing: 6) {
                answer(Pretty.answer(exact), insert: true, picked: picked == 1)
                Text("≈").font(.system(size: Theme.body)).foregroundStyle(Color.qMuted).accessibilityHidden(true)
                answer(Pretty.answer(row.display), insert: true, picked: picked == 2)
            }
        } else {
            answer(Pretty.answer(row.display), insert: true, picked: picked == 1, fill: true)
        }
    }

    private func answer(_ text: String, insert: Bool, picked: Bool, fill: Bool = false) -> some View {
        AnswerCell(text: text, picked: picked, fill: fill) { model.insertFromRow(index, answer: true) }
            .onDrag { NSItemProvider(object: row.display as NSString) }
    }
}

/// .tape-a: 17 pt answer colour, -0.015 em, lighter on hover
private struct AnswerCell: View {
    let text: String
    let picked: Bool
    /// a lone answer is the whole column (display: block), so its pick ring is too
    var fill = false
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
        .help("Insert the answer")
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
            }
        }
    }
}

/// `?` / `help`: the cheat sheet (cheatSheet + hostCheats), its keys following the user's keybinds
private struct HelpSheet: View {
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

    private let right: [(String, String)] = [
        ("ft to cm", "units"), ("80 + 15%", "percent"), ("+/- -/+", "± ∓ uncertainty"), ("x = 5", "variables"),
        ("x^2 = 2", "solve"), ("sys 2", "systems"), ("graph x^2", "plot"), ("GCF LCM", "factors"),
        ("sum n, n=1..9", "Σ sums"), ("tutorial", "the walkthrough"), ("?", "this list"),
    ]

    var body: some View {
        // .cheats: two columns 24 pt apart; key columns at least 64 and 96 pt plus 8, 12 pt to the label
        HStack(alignment: .top, spacing: 24) {
            column(left, keyWidth: 64)
            column(right, keyWidth: 96)
        }
        .padding(.vertical, 8)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Shortcuts")
    }

    // a subgrid in the web: the key column is as wide as its widest key, and every label lines up after it
    private func column(_ rows: [(String, String)], keyWidth: CGFloat) -> some View {
        Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 0) {
            ForEach(Array(rows.enumerated()), id: \.offset) { _, r in
                GridRow {
                    Text(r.0).font(.system(size: Theme.body, weight: .medium)).tracking(0.02 * Theme.body)
                        .foregroundStyle(Color.qInk).lineLimit(1).fixedSize()
                        .frame(minWidth: keyWidth, alignment: .leading)
                    Text(r.1).font(.system(size: Theme.body)).foregroundStyle(Color.qMuted).lineLimit(1)
                }
                .frame(height: 28)
                .accessibilityElement(children: .combine)
            }
        }
        .padding(.horizontal, 8)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
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
            if !card.ask {
                Text(card.text)
                    .foregroundStyle(Color.qAnswer)
                    .lineLimit(4)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                HStack(spacing: 8) {
                    TextField("", text: Binding(get: { card.draft }, set: { model.setDraft(card.id, $0) }),
                              prompt: Text("Reply to \(card.from)").foregroundColor(.qMuted))
                        .textFieldStyle(.plain)
                        .font(.system(size: Theme.small))
                        .foregroundStyle(Color.qInk)
                        .padding(.vertical, 4)
                        .padding(.horizontal, 8)
                        .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(Self.control)
                            .shadow(color: .black.opacity(0.1), radius: 1, y: 1))
                        .overlay(RoundedRectangle(cornerRadius: Theme.radiusS)
                            .strokeBorder(focused ? Color.qAccent : Color.black.opacity(0.12), lineWidth: focused ? 2 : 0.5))
                        .noWritingTools()
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
        .accessibilityElement(children: .contain)
        .accessibilityLabel(card.ask ? "\(card.from) wants to message you" : "Message from \(card.from): \(card.text)")
    }
}
