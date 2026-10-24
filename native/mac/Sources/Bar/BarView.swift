import AppKit
import SwiftUI
import QCalcCore

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

struct BarView: View {
    @ObservedObject var model: BarModel
    @ObservedObject private var settings = AppSettings.shared
    var onSize: (BarSize) -> Void

    var body: some View {
        VStack(spacing: 0) {
            above
                .reportHeight(AnchorKey.self)
            Composer(model: model)
            below
        }
        .frame(maxWidth: .infinity, alignment: .top)
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
        .animation(NSWorkspace.shared.accessibilityDisplayShouldReduceMotion ? nil : .easeOut(duration: 0.12), value: model.rows.count)
    }

    @State private var anchor: CGFloat = 0
    @State private var lastHeight: CGFloat = 0

    // what sits above the line: the cheat sheet, the games list, or the tape
    @ViewBuilder private var above: some View {
        if model.helpShown {
            HelpSheet()
            Divider().opacity(0.5)
        } else if model.gamesShown {
            GamesSheet(model: model)
            Divider().opacity(0.5)
        } else if !model.visibleRange.isEmpty {
            HistoryTape(model: model)
            Divider().opacity(0.5)
        }
    }

    @ViewBuilder private var below: some View {
        if let hint = model.hint, !hint.isEmpty {
            Text(hint)
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 18)
                .padding(.bottom, 8)
                .accessibilityLabel(hint)
        }
        if settings.typstPreview, model.graph == nil, !model.live.isBlank, MathPreview.useful(model.text) {
            MathPreview(expression: model.text, answer: model.shownAnswer, renderer: model.math)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 18)
                .padding(.bottom, 8)
        }
        if let graph = model.graph {
            Divider().opacity(0.5)
            GraphPanel(model: model, state: graph)
        }
        if !model.cards.isEmpty {
            MessageCards(model: model)
        }
    }
}

// MARK: - the line and its answer

private struct Composer: View {
    @ObservedObject var model: BarModel
    @ObservedObject private var settings = AppSettings.shared

    var body: some View {
        HStack(alignment: .center, spacing: 10) {
            HStack(spacing: 1) {
                // the faint `ans` a chained line continues from
                if model.chained {
                    Text("ans")
                        .font(.system(size: BarTextView.fontSize))
                        .foregroundStyle(.tertiary)
                        .accessibilityLabel("continues from the last answer")
                }
                ZStack(alignment: .leading) {
                    if model.text.isEmpty {
                        Text(model.rows.isEmpty ? "Type math, like 2^10 or sqrt(2)" : "")
                            .font(.system(size: BarTextView.fontSize))
                            .foregroundStyle(.tertiary)
                            .allowsHitTesting(false)
                            .accessibilityHidden(true)
                    }
                    InputField(model: model, placeholder: "")
                }
            }
            .frame(minWidth: 120, maxWidth: .infinity, minHeight: 34, maxHeight: 34)
            .layoutPriority(1)
            LiveAnswerView(model: model)
                .layoutPriority(2)
            EdgeTools(model: model)
        }
        .padding(.leading, 18)
        .padding(.trailing, 10)
        .frame(height: 64)
    }
}

struct LiveAnswerView: View {
    @ObservedObject var model: BarModel

    var body: some View {
        Group {
            if model.copied {
                Label("Copied", systemImage: "checkmark")
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(.secondary)
                    .transition(.opacity)
                    .accessibilityLabel("Copied")
            } else if model.live.isBlank, let steady = model.steady {
                Text(model.pretty(steady))
                    .font(answerFont)
                    .foregroundStyle(.tertiary)
                    .accessibilityHidden(true)
            } else if model.hasDual, let exact = model.live.exact {
                HStack(spacing: 6) {
                    part(Pretty.answer(exact), copy: exact, help: "Copy exact value")
                    Text("≈").font(answerFont).foregroundStyle(.secondary).accessibilityHidden(true)
                    part(Pretty.answer(model.live.display), copy: model.live.display, help: "Copy approximation")
                }
                .accessibilityElement(children: .contain)
                .accessibilityLabel("Answer")
            } else if !model.live.isBlank {
                part(model.pretty(model.shownAnswer), copy: model.shownAnswer, help: "Copy to clipboard · ⌘C also copies")
            }
        }
        .lineLimit(1)
        .minimumScaleFactor(0.45)
        .animation(.easeOut(duration: 0.12), value: model.copied)
    }

    private var answerFont: Font { .system(size: 24, weight: .regular).monospacedDigit() }

    private func part(_ shown: String, copy: String, help: String) -> some View {
        Button {
            model.copyAnswer(copy)
            model.focusField?()
        } label: {
            Text(shown)
                .font(answerFont)
                .foregroundStyle(Color.accentColor)
        }
        .buttonStyle(.plain)
        .help(help)
        .accessibilityLabel("Answer \(shown)")
        .accessibilityHint("Copies it")
        .onDrag { NSItemProvider(object: copy as NSString) }
    }
}

// the toggles at the bar's end (EdgeTools.tsx), each with its key in the tooltip
private struct EdgeTools: View {
    @ObservedObject var model: BarModel
    @ObservedObject private var settings = AppSettings.shared

    var body: some View {
        HStack(spacing: 2) {
            tool(model.angleRad ? "rad" : "deg", on: model.angleRad, action: "angle", label: model.angleRad ? "Radians" : "Degrees") {
                model.toggleAngle()
            }
            tool("a/b", on: model.fractions, action: "fraction", label: "Fractions") { model.toggleFractions() }
            tool("sf", on: model.sigFigMode, action: "sigFigs", label: "Sig figs from input") { model.toggleSigFigMode() }
            icon("trash", action: "clear", label: "Clear history") { model.clearHistory() }
                .disabled(model.rows.isEmpty)
            icon("gearshape", action: "settings", label: "Settings") { model.onSettings?() }
        }
    }

    private func keyTitle(_ action: String) -> String {
        KeyActions.title(KeyActions.chord(action, in: settings.keybinds))
    }

    private func tip(_ label: String, _ action: String) -> String {
        let key = keyTitle(action)
        return key.isEmpty ? label : "\(label) · \(key)"
    }

    private func tool(_ title: String, on: Bool, action: String, label: String, run: @escaping () -> Void) -> some View {
        Button(action: { run(); model.focusField?() }) {
            Text(title)
                .font(.system(size: 11, weight: .semibold, design: .rounded))
                .frame(minWidth: 26, minHeight: 22)
                .foregroundStyle(on ? Color.accentColor : Color.secondary)
                .background(RoundedRectangle(cornerRadius: 6).fill(on ? Color.accentColor.opacity(0.14) : Color.clear))
        }
        .buttonStyle(.plain)
        .help(tip(label, action))
        .accessibilityLabel(label)
        .accessibilityValue(on ? "on" : "off")
    }

    private func icon(_ symbol: String, action: String, label: String, run: @escaping () -> Void) -> some View {
        Button(action: { run(); model.focusField?() }) {
            Image(systemName: symbol)
                .font(.system(size: 12, weight: .medium))
                .frame(width: 24, height: 22)
                .foregroundStyle(.secondary)
        }
        .buttonStyle(.plain)
        .help(tip(label, action))
        .accessibilityLabel(label)
    }
}

// MARK: - the tape

private struct HistoryTape: View {
    @ObservedObject var model: BarModel

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView(.vertical, showsIndicators: false) {
                LazyVStack(spacing: 0) {
                    ForEach(Array(model.visibleRange), id: \.self) { i in
                        HistoryRowView(model: model, index: i, row: model.rows[i])
                            .id(i)
                    }
                }
                .padding(.vertical, 6)
            }
            .frame(maxHeight: CGFloat(min(model.visibleRange.count, 6)) * 34 + 12)
            .onChange(of: model.selected) { i in
                if let i { withAnimation(.easeOut(duration: 0.1)) { proxy.scrollTo(i) } }
            }
            .onAppear { proxy.scrollTo(model.rows.count - 1) }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("History")
    }
}

private struct HistoryRowView: View {
    @ObservedObject var model: BarModel
    let index: Int
    let row: HistoryRow
    @State private var hover = false

    var selected: Bool { model.selected == index }
    private var answer: String {
        let approx = Pretty.answer(row.display)
        guard AppSettings.shared.answerForm == "exact", row.hasDual, let exact = row.exact else { return approx }
        return Pretty.answer(exact) + " ≈ " + approx
    }

    var body: some View {
        HStack(spacing: 8) {
            Button { model.insertFromRow(index, answer: false) } label: {
                Text(row.expr)
                    .foregroundStyle(selected && !model.pickAnswer ? Color.primary : Color.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .padding(.horizontal, 4)
                    .background(RoundedRectangle(cornerRadius: 5).fill(selected && !model.pickAnswer ? Color.accentColor.opacity(0.18) : .clear))
            }
            .buttonStyle(.plain)
            .help("Insert the expression")
            .accessibilityLabel("Expression \(row.expr)")
            Spacer(minLength: 8)
            Text("=").foregroundStyle(.tertiary).accessibilityHidden(true)
            Button { model.insertFromRow(index, answer: true) } label: {
                Text(answer)
                    .foregroundStyle(selected && model.pickAnswer ? Color.primary : Color.accentColor.opacity(0.9))
                    .lineLimit(1)
                    .padding(.horizontal, 4)
                    .background(RoundedRectangle(cornerRadius: 5).fill(selected && model.pickAnswer ? Color.accentColor.opacity(0.18) : .clear))
            }
            .buttonStyle(.plain)
            .help("Insert the answer")
            .accessibilityLabel("Answer \(answer)")
            .onDrag { NSItemProvider(object: row.display as NSString) }
        }
        .font(.system(size: 15).monospacedDigit())
        .padding(.horizontal, 14)
        .frame(height: 34)
        .background(RoundedRectangle(cornerRadius: 8).fill(selected ? Color.primary.opacity(0.07) : hover ? Color.primary.opacity(0.04) : .clear))
        .padding(.horizontal, 6)
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
}

// MARK: - sheets above the bar

private struct GamesSheet: View {
    @ObservedObject var model: BarModel

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(BarCommand.gameList.enumerated()), id: \.offset) { i, game in
                Button { model.onGame?(game.kind) } label: {
                    HStack {
                        Text(game.name).font(.system(size: 14, weight: .medium))
                        Spacer()
                        Text(game.blurb).font(.system(size: 12)).foregroundStyle(.secondary)
                    }
                    .padding(.horizontal, 14)
                    .frame(height: 30)
                    .background(RoundedRectangle(cornerRadius: 7).fill(i == model.gamePick ? Color.accentColor.opacity(0.16) : .clear))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Play \(game.name), \(game.blurb)")
            }
        }
        .padding(6)
    }
}

private struct HelpSheet: View {
    private let rows: [(String, String)] = [
        ("2^10 · sqrt(2) · 5!", "arithmetic, powers, roots, factorials"),
        ("x = 4 · f(x) = x^2", "variables and functions, used by later lines"),
        ("* 3 at the start", "continues from the last answer"),
        ("graph sin(x)", "draws a curve under the bar"),
        ("periodic · identities", "reference sheets in their own windows"),
        ("games · pong", "games, in a window of their own"),
        ("message <name> hi", "messages to other Q Calc users"),
        ("↑ ↓ · ↵ · ⌥↵", "walk the history, insert a row or its other half"),
        ("Tab", "cycle the answer's forms"),
        ("⌘C · ⇧⌘C", "copy the answer, or the whole line"),
    ]

    var body: some View {
        Grid(alignment: .leading, horizontalSpacing: 18, verticalSpacing: 6) {
            ForEach(rows, id: \.0) { r in
                GridRow {
                    Text(r.0).font(.system(size: 13, design: .monospaced))
                    Text(r.1).font(.system(size: 13)).foregroundStyle(.secondary)
                }
            }
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

// MARK: - message cards (MessageNotices.tsx)

private struct MessageCards: View {
    @ObservedObject var model: BarModel

    var body: some View {
        VStack(spacing: 6) {
            ForEach(model.cards) { card in
                CardView(model: model, card: card)
            }
        }
        .padding(.horizontal, 10)
        .padding(.bottom, 10)
        .onHover { model.cardsHovered = $0 }
    }
}

private struct CardView: View {
    @ObservedObject var model: BarModel
    let card: BannerCard
    @FocusState private var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                Text(card.from).font(.system(size: 13, weight: .semibold))
                if card.ask {
                    Text("wants to message you").font(.system(size: 13)).foregroundStyle(.secondary)
                } else {
                    Text(card.text).font(.system(size: 13)).textSelection(.enabled).lineLimit(3)
                }
                Spacer()
                Button { model.dismissCard(card.id) } label: { Image(systemName: "xmark").font(.system(size: 10, weight: .bold)) }
                    .buttonStyle(.plain)
                    .foregroundStyle(.secondary)
                    .help("Dismiss")
                    .accessibilityLabel("Dismiss message from \(card.from)")
            }
            if card.ask {
                HStack {
                    Button("Allow") { model.answerAsk(card.id, allow: true) }
                    Button("Block") { model.answerAsk(card.id, allow: false) }
                }
                .controlSize(.small)
            } else {
                HStack {
                    TextField("Reply to \(card.from)", text: Binding(get: { card.draft }, set: { model.setDraft(card.id, $0) }))
                        .textFieldStyle(.roundedBorder)
                        .noWritingTools()
                        .focused($focused)
                        .onSubmit { model.sendReply(card.id) }
                        .disabled(card.reply == .sending || card.reply == .sent)
                        .accessibilityLabel("Reply to \(card.from)")
                    switch card.reply {
                    case .sending: ProgressView().controlSize(.small)
                    case .sent: Text("sent").font(.system(size: 12)).foregroundStyle(.secondary)
                    case let .failed(note): Text(note).font(.system(size: 11)).foregroundStyle(.red).lineLimit(1)
                    case .idle: EmptyView()
                    }
                }
            }
        }
        .padding(10)
        .background(RoundedRectangle(cornerRadius: 10).fill(Color.primary.opacity(0.06)))
        .onChange(of: focused) { on in
            if on { model.replyFocused = card.id } else if model.replyFocused == card.id { model.replyFocused = nil }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(card.ask ? "\(card.from) wants to message you" : "Message from \(card.from): \(card.text)")
    }
}
