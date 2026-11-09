import QCalcCore
import SwiftUI

// Native only: the graph's settings (the grid, the x and y windows, steps and scales) and its export (labels on
// the axes and the key points), and the equations graphed beside the bar's.

/// The settings, under the head in the bar (a popover took the keys from the bar, which hid it). Each row says
/// what it does underneath. Only what changes the view is here: the bar has its own deg/rad switch.
struct GraphSettingsView: View {
    @ObservedObject var model: BarModel

    private var g: GraphState? { model.graph }
    private var p: GraphPrefs { model.graphPrefs }

    private func flip(_ key: WritableKeyPath<GraphPrefs, Bool>) { model.graphPrefs[keyPath: key].toggle() }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            row("Grid", "Lines behind the graph. Cartesian is squares; polar is rings a step apart with spokes every 15°.") {
                Cell("Grid", on: p.grid, tip: "Show gridlines") { flip(\.grid) }
                Choice(["Cartesian", "Polar"], at: p.polar ? 1 : 0) { model.graphPrefs.polar = $0 == 1 }
                    .disabled(!p.grid)
                Cell("Minor lines", on: p.minorGrid, tip: "Fainter lines between the numbered ones") { flip(\.minorGrid) }
                Cell("Numbers", on: p.axisNumbers, tip: "Numbers along the axes") { flip(\.axisNumbers) }
            }
            if let g {
                range("x", lo: g.xMin, hi: g.xMax, set: model.setGraphX, step: g.xStep,
                      setStep: { model.setGraphAxis(xStep: .some($0)) },
                      log: g.shows3D ? nil : g.xLog, setLog: { model.setGraphLog(x: $0) })
                if g.shows3D {
                    range("z", lo: g.zWindow.lowerBound, hi: g.zWindow.upperBound, set: model.setGraphZ, step: g.yStep,
                          setStep: { model.setGraphAxis(yStep: .some($0)) }, log: nil, setLog: { _ in })
                } else {
                    range("y", lo: g.yMin, hi: g.yMax, set: model.setGraphY, step: g.yStep,
                          setStep: { model.setGraphAxis(yStep: .some($0)) }, log: g.yLog, setLog: { model.setGraphLog(y: $0) })
                }
                if !g.shows3D {
                    HStack(spacing: 6) {
                        Cell("Zoom square", on: false, tip: "Make one unit up as long as one unit across") { model.zoomGraphSquare() }
                            .disabled(g.logX || g.logY)
                        Text("Reset (top right) goes back to the view the graph opened on.")
                            .font(.system(size: Theme.caption)).foregroundStyle(Color.qMuted)
                    }
                    .padding(.leading, Self.nameWidth + 8)
                }
            }
        }
        .padding(12)
        .background(RoundedRectangle(cornerRadius: Theme.radiusCard).fill(Color.qInk.opacity(0.04)))
        .overlay(RoundedRectangle(cornerRadius: Theme.radiusCard).strokeBorder(Color.qInk.opacity(0.1), lineWidth: 0.5))
        .padding(.vertical, 6)
    }

    static let nameWidth: CGFloat = 54

    /// A setting: its name, its controls (wrapping on a narrow bar) and a line saying what it does
    private func row<Content: View>(_ name: String, _ about: String, @ViewBuilder _ content: @escaping () -> Content) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(name).font(.system(size: Theme.small, weight: .semibold)).foregroundStyle(Color.qInk)
                .frame(width: Self.nameWidth, alignment: .leading)
            VStack(alignment: .leading, spacing: 4) {
                WrapRow(spacing: 6, leading: true) { content() }
                Text(about).font(.system(size: Theme.caption)).foregroundStyle(Color.qMuted)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    /// An axis's window `lo to hi`, its step and (2D) linear or log
    private func range(_ name: String, lo: Double, hi: Double, set: @escaping (Double, Double) -> Void,
                       step: Double?, setStep: @escaping (Double?) -> Void,
                       log: Bool?, setLog: @escaping (Bool) -> Void) -> some View {
        row("\(name) range", name == "x"
            ? "The window across. Type numbers or pi; Step is the gap between numbered lines (blank picks one)."
            : name == "z" ? "How high the box reaches. Step is the gap between numbered lines."
            : "The window up and down. Log spaces 1, 10, 100… evenly, for values that grow fast.") {
            NumberField(value: lo) { set($0 ?? lo, hi) }.frame(width: 96)
            Text("to").font(.system(size: Theme.small)).foregroundStyle(Color.qMuted)
            NumberField(value: hi) { set(lo, $0 ?? hi) }.frame(width: 96)
            Text("step").font(.system(size: Theme.small)).foregroundStyle(Color.qMuted).padding(.leading, 4)
            NumberField(value: step, placeholder: "auto") { setStep($0) }.frame(width: 56)
            if let log {
                Choice(["Linear", "Log"], at: log ? 1 : 0) { setLog($0 == 1) }
            }
        }
    }
}

/// The export, under the head when the camera is pressed: the axes' names and the key points' labels go on the
/// picture, which is saved to the Desktop and copied.
struct GraphExportView: View {
    @ObservedObject var model: BarModel
    @Binding var labelPoints: Bool
    let save: () -> Void

    var body: some View {
        if let g = model.graph {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text("Export").font(.system(size: Theme.small, weight: .semibold)).foregroundStyle(Color.qInk)
                    .frame(width: GraphSettingsView.nameWidth, alignment: .leading)
                VStack(alignment: .leading, spacing: 4) {
                    WrapRow(spacing: 6, leading: true) {
                        Cell("Label key points", on: labelPoints, tip: "Write each point's kind and (x, y) beside it") { labelPoints.toggle() }
                        Text("x axis").font(.system(size: Theme.small)).foregroundStyle(Color.qMuted).padding(.leading, 4)
                        FieldBox { TextField("x", text: Binding(get: { g.xLabel }, set: { model.setGraphAxis(xLabel: $0) })) }.frame(width: 90)
                        Text(g.shows3D ? "z axis" : "y axis").font(.system(size: Theme.small)).foregroundStyle(Color.qMuted)
                        FieldBox { TextField(g.shows3D ? "z" : "y", text: Binding(get: { g.yLabel }, set: { model.setGraphAxis(yLabel: $0) })) }.frame(width: 90)
                        Button(action: save) {
                            Label("Save picture", systemImage: "square.and.arrow.down")
                                .font(.system(size: Theme.small, weight: .semibold))
                                .foregroundStyle(Color.qBg)
                                .padding(.vertical, 3).padding(.horizontal, 10)
                                .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(Color.qAccent))
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .hoverTip("Save to the Desktop and copy")
                    }
                    Text("Saves a PNG of the graph and its equations to the Desktop and copies it. Axis names show on the graph too.")
                        .font(.system(size: Theme.caption)).foregroundStyle(Color.qMuted)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .padding(12)
            .background(RoundedRectangle(cornerRadius: Theme.radiusCard).fill(Color.qInk.opacity(0.04)))
            .overlay(RoundedRectangle(cornerRadius: Theme.radiusCard).strokeBorder(Color.qInk.opacity(0.1), lineWidth: 0.5))
            .padding(.vertical, 6)
        }
    }
}

/// A setting that's on or off: a rounded cell, filled and in ink when on, muted when off.
private struct Cell: View {
    let title: String
    let on: Bool
    let tip: String
    let action: () -> Void
    @State private var hover = false
    @Environment(\.isEnabled) private var enabled

    init(_ title: String, on: Bool, tip: String, action: @escaping () -> Void) {
        (self.title, self.on, self.tip, self.action) = (title, on, tip, action)
    }

    var body: some View {
        Button(action: action) {
            HStack(spacing: 4) {
                if on { Image(systemName: "checkmark").font(.system(size: 9, weight: .bold)) }
                Text(title).font(.system(size: Theme.small, weight: on ? .medium : .regular))
            }
            .foregroundStyle(on ? Color.qInk : hover ? Color.qInk.opacity(0.8) : Color.qMuted)
            .padding(.vertical, 3).padding(.horizontal, 9)
            .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(on ? Color.qInk.opacity(0.1) : hover ? Color.qFillHover : .clear))
            .overlay(RoundedRectangle(cornerRadius: Theme.radiusS).strokeBorder(Color.qInk.opacity(on ? 0.2 : 0.14), lineWidth: 0.75))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .opacity(enabled ? 1 : 0.4)
        .onHover { hover = $0 }
        .hoverTip(tip)
        .accessibilityLabel(tip)
        .accessibilityAddTraits(on ? .isSelected : [])
    }
}

/// One of a few choices, side by side in one rounded box, the chosen one filled
private struct Choice: View {
    let names: [String]
    let at: Int
    let pick: (Int) -> Void
    @Environment(\.isEnabled) private var enabled

    init(_ names: [String], at: Int, pick: @escaping (Int) -> Void) {
        (self.names, self.at, self.pick) = (names, at, pick)
    }

    var body: some View {
        HStack(spacing: 0) {
            ForEach(Array(names.enumerated()), id: \.offset) { i, name in
                Button { pick(i) } label: {
                    Text(name).font(.system(size: Theme.small, weight: i == at ? .medium : .regular))
                        .foregroundStyle(i == at ? Color.qInk : Color.qMuted)
                        .padding(.vertical, 3).padding(.horizontal, 9)
                        .background(RoundedRectangle(cornerRadius: Theme.radiusS - 1).fill(i == at ? Color.qInk.opacity(0.12) : .clear))
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(i == at ? .isSelected : [])
            }
        }
        .padding(1)
        .overlay(RoundedRectangle(cornerRadius: Theme.radiusS).strokeBorder(Color.qInk.opacity(0.14), lineWidth: 0.75))
        .opacity(enabled ? 1 : 0.4)
    }
}

/// A field in the bar's look: a rounded box with a hairline edge
struct FieldBox<Content: View>: View {
    @ViewBuilder let content: () -> Content
    var body: some View {
        content()
            .textFieldStyle(.plain)
            .font(.system(size: Theme.small))
            .padding(.vertical, 3).padding(.horizontal, 7)
            .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(Color.qBg.opacity(0.6)))
            .overlay(RoundedRectangle(cornerRadius: Theme.radiusS).strokeBorder(Color.qInk.opacity(0.18), lineWidth: 0.75))
    }
}

/// A number typed in: committed on Return or leaving the field (blank is none); `pi`, `π`, `e` and a leading
/// multiple of them (`2pi`) read too. It shows the value again when that changes elsewhere.
struct NumberField: View {
    let value: Double?
    var placeholder = ""
    let commit: (Double?) -> Void
    @State private var text = ""
    @FocusState private var focused: Bool

    static func show(_ v: Double?) -> String {
        guard let v else { return "" }
        let r = abs(v) < 1e-12 ? 0 : v
        return String(format: "%.6g", r)
    }

    static func parse(_ raw: String) -> Double? {
        var s = raw.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: "−", with: "-")
            .replacingOccurrences(of: "π", with: "pi").replacingOccurrences(of: " ", with: "")
        guard !s.isEmpty else { return nil }
        if let d = Double(s) { return d }
        for (name, k) in [("pi", Double.pi), ("e", M_E)] where s.hasSuffix(name) {
            s.removeLast(name.count)
            if s.hasSuffix("*") { s.removeLast() }
            if s.isEmpty { return k }
            if s == "-" { return -k }
            if let m = Double(s) { return m * k }
        }
        return nil
    }

    var body: some View {
        FieldBox { TextField(placeholder, text: $text) }
            .focused($focused)
            .onAppear { text = Self.show(value) }
            .onChange(of: value) { if !focused { text = Self.show($0) } }
            .onSubmit { done() }
            .onChange(of: focused) { if !$0 { done() } }
    }

    private func done() {
        let v = Self.parse(text)
        if text.trimmingCharacters(in: .whitespaces).isEmpty || v != nil { commit(v) }
        text = Self.show(v ?? value)
    }
}

/// The equations on the graph, each with its colour: the bar's first (and any after a `;` there), then those
/// added here, which are typed in place and can be taken off.
struct GraphEquations: View {
    @ObservedObject var model: BarModel
    let state: GraphState
    let label: String
    @FocusState private var focus: Int?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            row(color: GraphPanel.layerSwiftColor(0)) {
                Text(label).font(.system(size: Theme.small)).foregroundStyle(Color.qInk).lineLimit(1)
            }
            ForEach(Array(state.layers.enumerated()), id: \.element.id) { i, layer in
                row(color: GraphPanel.layerSwiftColor(i + 1)) {
                    if layer.fromBar {
                        Text(Pretty.radicals(layer.expression)).font(.system(size: Theme.small)).foregroundStyle(Color.qInk).lineLimit(1)
                            .help("After a ; in the bar: edit it there")
                    } else {
                        TextField("y = …  or  x^2 + y^2 = 9  or  z = …", text: Binding(
                            get: { layer.expression },
                            set: { model.setGraphLayer(layer.id, expression: $0) }))
                            .textFieldStyle(.plain)
                            .font(.system(size: Theme.small))
                            .focused($focus, equals: layer.id)
                    }
                    if let error = layer.error, !layer.isBlank {
                        Text(error).font(.system(size: Theme.caption)).foregroundStyle(Color.qMuted).lineLimit(1)
                    } else if state.shows3D, layer.surface == nil, !layer.isBlank {
                        Text("not a surface: shown in 2D").font(.system(size: Theme.caption)).foregroundStyle(Color.qMuted)
                    }
                    Spacer(minLength: 0)
                    if !layer.fromBar {
                        Button { model.removeGraphLayer(layer.id) } label: {
                            Image(systemName: "xmark").font(.system(size: 10, weight: .semibold)).foregroundStyle(Color.qMuted)
                                .frame(width: 18, height: 18).contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .hoverTip("Remove")
                    }
                }
            }
        }
        .onChange(of: state.layers.last?.id) { _ in
            // a row just added takes the typing
            if let last = state.layers.last, !last.fromBar, last.isBlank { focus = last.id }
        }
        .onAppear {
            if let last = state.layers.last, !last.fromBar, last.isBlank { focus = last.id }
        }
    }

    private func row<Content: View>(color: Color, @ViewBuilder _ content: () -> Content) -> some View {
        // a plain list: a colour bar, the equation in monospace, a hairline under each
        HStack(spacing: 8) {
            Rectangle().fill(color).frame(width: 3, height: 14)
            content()
        }
        .padding(.vertical, 3).padding(.horizontal, 4)
        .frame(maxWidth: .infinity, alignment: .leading)
        .overlay(alignment: .bottom) { Rectangle().fill(Color.qInk.opacity(0.1)).frame(height: 0.5) }
    }
}
