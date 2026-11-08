import QCalcCore
import SwiftUI

// Native only: the graph's settings (Desmos's graph settings: text size, contrast, grid, arrows, numbers, axes'
// labels, windows and steps, log axes, lock, radians or degrees) and the equations graphed beside the bar's.

/// The settings, under the head in the bar's own look: monospaced, hairline cells that read in ink when on, and
/// plain fields. (A popover took the keys from the bar, which hid it.)
struct GraphSettingsView: View {
    @ObservedObject var model: BarModel

    private var g: GraphState? { model.graph }
    private var p: GraphPrefs { model.graphPrefs }

    private func flip(_ key: WritableKeyPath<GraphPrefs, Bool>) { model.graphPrefs[keyPath: key].toggle() }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            row("view") {
                Cell("A", on: !p.largeText, tip: "Small text") { model.graphPrefs.largeText = false }
                Cell("A+", on: p.largeText, tip: "Large text") { model.graphPrefs.largeText = true }
                Gap()
                Cell("invert", on: p.reverseContrast, tip: "Reverse contrast") { flip(\.reverseContrast) }
                Gap()
                Cell("grid", on: p.grid, tip: "Gridlines") { flip(\.grid) }
                Cell("xy", on: !p.polar, tip: "Cartesian grid") { model.graphPrefs.polar = false }.disabled(!p.grid)
                Cell("rθ", on: p.polar, tip: "Polar grid") { model.graphPrefs.polar = true }.disabled(!p.grid)
                Cell("minor", on: p.minorGrid, tip: "Minor gridlines") { flip(\.minorGrid) }
                Gap()
                Cell("numbers", on: p.axisNumbers, tip: "Axis numbers") { flip(\.axisNumbers) }
                Cell("arrows", on: p.arrows, tip: "Arrows on the axes") { flip(\.arrows) }
            }
            if let g {
                axis("x", show: \.showXAxis, label: g.xLabel, setLabel: { model.setGraphAxis(xLabel: $0) },
                     lo: g.xMin, hi: g.xMax, set: model.setGraphX,
                     step: g.xStep, setStep: { model.setGraphAxis(xStep: .some($0)) })
                if g.shows3D {
                    axis("z", show: \.showYAxis, label: g.yLabel, setLabel: { model.setGraphAxis(yLabel: $0) },
                         lo: g.zWindow.lowerBound, hi: g.zWindow.upperBound, set: model.setGraphZ,
                         step: g.yStep, setStep: { model.setGraphAxis(yStep: .some($0)) })
                } else {
                    axis("y", show: \.showYAxis, label: g.yLabel, setLabel: { model.setGraphAxis(yLabel: $0) },
                         lo: g.yMin, hi: g.yMax, set: model.setGraphY,
                         step: g.yStep, setStep: { model.setGraphAxis(yStep: .some($0)) })
                }
                row("scale") {
                    if !g.shows3D {
                        Cell("x lin", on: !g.xLog, tip: "Linear x axis") { model.setGraphLog(x: false) }
                        Cell("x log", on: g.xLog, tip: "Logarithmic x axis") { model.setGraphLog(x: true) }
                        Gap()
                        Cell("y lin", on: !g.yLog, tip: "Linear y axis") { model.setGraphLog(y: false) }
                        Cell("y log", on: g.yLog, tip: "Logarithmic y axis") { model.setGraphLog(y: true) }
                        Gap()
                        Cell("square", on: false, tip: "Zoom square: as many units per point up as across") { model.zoomGraphSquare() }
                            .disabled(g.logX || g.logY)
                    }
                    Cell("lock", on: p.lockViewport, tip: "Lock the viewport: dragging and zooming don't move it") { flip(\.lockViewport) }
                    Gap()
                    let degrees = p.degrees ?? !model.options.angleRadians
                    Cell("rad", on: !degrees, tip: "Radians") { model.graphPrefs.degrees = false }
                    Cell("deg", on: degrees, tip: "Degrees") { model.graphPrefs.degrees = true }
                }
            }
        }
        .padding(.vertical, 8)
        .overlay(alignment: .bottom) { Rectangle().fill(Color.qInk.opacity(0.1)).frame(height: 0.5) }
    }

    /// A row: its name in muted capitals, then cells that wrap on a narrow bar
    private func row<Content: View>(_ name: String, @ViewBuilder _ content: @escaping () -> Content) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(name.uppercased()).font(.system(size: Theme.caption, weight: .medium, design: .monospaced)).tracking(0.6)
                .foregroundStyle(Color.qMuted).frame(width: 46, alignment: .leading)
            WrapRow(spacing: 4, leading: true) { content() }
        }
    }

    /// An axis: shown or not, `lo ≤ name ≤ hi`, its step and its label (on the 3D view the second axis is z)
    private func axis(_ name: String, show: WritableKeyPath<GraphPrefs, Bool>, label: String, setLabel: @escaping (String) -> Void,
                      lo: Double, hi: Double, set: @escaping (Double, Double) -> Void,
                      step: Double?, setStep: @escaping (Double?) -> Void) -> some View {
        row("\(name) axis") {
            Cell("show", on: p[keyPath: show], tip: "Show the \(name) axis") { flip(show) }
            NumberField(value: lo) { set($0 ?? lo, hi) }.frame(width: 76)
            Text("≤ \(name) ≤").font(.system(size: Theme.small, design: .monospaced)).foregroundStyle(Color.qMuted).fixedSize()
            NumberField(value: hi) { set(lo, $0 ?? hi) }.frame(width: 76)
            Text("step").font(.system(size: Theme.caption, design: .monospaced)).foregroundStyle(Color.qMuted).padding(.leading, 4)
            NumberField(value: step, placeholder: "auto") { setStep($0) }.frame(width: 52)
            Text("label").font(.system(size: Theme.caption, design: .monospaced)).foregroundStyle(Color.qMuted).padding(.leading, 4)
            FieldBox { TextField(name, text: Binding(get: { label }, set: setLabel)) }.frame(width: 84)
        }
    }
}

/// A setting's cell: monospaced, a hairline square edge, in ink with a faint fill when on and muted when off.
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
            Text(title)
                .font(.system(size: Theme.small, weight: on ? .semibold : .regular, design: .monospaced))
                .foregroundStyle(on ? Color.qInk : hover ? Color.qInk.opacity(0.8) : Color.qMuted)
                .padding(.vertical, 2).padding(.horizontal, 7)
                .background(Rectangle().fill(on ? Color.qInk.opacity(0.08) : hover ? Color.qFillHover : .clear))
                .overlay(Rectangle().strokeBorder(Color.qInk.opacity(on ? 0.3 : 0.14), lineWidth: 0.75))
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

/// A little space between groups of cells
private struct Gap: View {
    var body: some View { Color.clear.frame(width: 6, height: 1) }
}

/// A field's frame in the bar's look: plain monospaced text in a hairline box
struct FieldBox<Content: View>: View {
    @ViewBuilder let content: () -> Content
    var body: some View {
        content()
            .textFieldStyle(.plain)
            .font(.system(size: Theme.small, design: .monospaced))
            .padding(.vertical, 2).padding(.horizontal, 5)
            .overlay(Rectangle().strokeBorder(Color.qInk.opacity(0.18), lineWidth: 0.75))
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
                Text(label).font(.system(size: Theme.small, design: .monospaced)).foregroundStyle(Color.qInk).lineLimit(1)
            }
            ForEach(Array(state.layers.enumerated()), id: \.element.id) { i, layer in
                row(color: GraphPanel.layerSwiftColor(i + 1)) {
                    if layer.fromBar {
                        Text(Pretty.radicals(layer.expression)).font(.system(size: Theme.small, design: .monospaced)).foregroundStyle(Color.qInk).lineLimit(1)
                            .help("After a ; in the bar: edit it there")
                    } else {
                        TextField("y = …  or  x^2 + y^2 = 9  or  z = …", text: Binding(
                            get: { layer.expression },
                            set: { model.setGraphLayer(layer.id, expression: $0) }))
                            .textFieldStyle(.plain)
                            .font(.system(size: Theme.small, design: .monospaced))
                            .focused($focus, equals: layer.id)
                    }
                    if let error = layer.error, !layer.isBlank {
                        Text(error).font(.system(size: Theme.caption, design: .monospaced)).foregroundStyle(Color.qMuted).lineLimit(1)
                    } else if state.shows3D, layer.surface == nil, !layer.isBlank {
                        Text("not a surface: shown in 2D").font(.system(size: Theme.caption, design: .monospaced)).foregroundStyle(Color.qMuted)
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
