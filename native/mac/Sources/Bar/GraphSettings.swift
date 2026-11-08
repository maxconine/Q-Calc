import QCalcCore
import SwiftUI

// Native only: the graph's settings (Desmos's graph settings: text size, contrast, grid, arrows, numbers, axes'
// labels, windows and steps, log axes, lock, radians or degrees) and the equations graphed beside the bar's.

/// The settings, in a popover from the head's gear.
struct GraphSettingsView: View {
    @ObservedObject var model: BarModel
    @State private var more = false

    private var g: GraphState? { model.graph }

    private func pref<T>(_ key: WritableKeyPath<GraphPrefs, T>) -> Binding<T> {
        Binding(get: { model.graphPrefs[keyPath: key] }, set: { model.graphPrefs[keyPath: key] = $0 })
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Picker("Text size", selection: pref(\.largeText)) {
                Text("A").font(.system(size: 11)).tag(false)
                Text("A").font(.system(size: 17)).tag(true)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .help("Text size")
            Toggle("Reverse contrast", isOn: pref(\.reverseContrast))
            Divider()
            HStack(alignment: .top, spacing: 16) {
                VStack(alignment: .leading, spacing: 8) {
                    Toggle("Grid", isOn: pref(\.grid))
                    Picker("Grid", selection: pref(\.polar)) {
                        Image(systemName: "squareshape.split.3x3").tag(false).help("Cartesian grid")
                        Image(systemName: "circle.circle").tag(true).help("Polar grid")
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                    .frame(width: 84)
                    .disabled(!model.graphPrefs.grid)
                    Toggle("Arrows", isOn: pref(\.arrows))
                }
                VStack(alignment: .leading, spacing: 8) {
                    Toggle("Axis numbers", isOn: pref(\.axisNumbers))
                    Toggle("Minor gridlines", isOn: pref(\.minorGrid))
                    Button("Zoom Square") { model.zoomGraphSquare() }
                        .disabled(g.map { $0.logX || $0.logY } ?? true)
                }
            }
            Divider()
            if let g {
                axis("X-Axis", show: pref(\.showXAxis), label: g.xLabel, setLabel: { model.setGraphAxis(xLabel: $0) },
                     name: "x", lo: g.xMin, hi: g.xMax, set: model.setGraphX,
                     step: g.xStep, setStep: { model.setGraphAxis(xStep: .some($0)) })
                if g.shows3D {
                    axis("Y-Axis", show: pref(\.showYAxis), label: g.yLabel, setLabel: { model.setGraphAxis(yLabel: $0) },
                         name: "z", lo: g.zWindow.lowerBound, hi: g.zWindow.upperBound, set: model.setGraphZ,
                         step: g.yStep, setStep: { model.setGraphAxis(yStep: .some($0)) })
                } else {
                    axis("Y-Axis", show: pref(\.showYAxis), label: g.yLabel, setLabel: { model.setGraphAxis(yLabel: $0) },
                         name: "y", lo: g.yMin, hi: g.yMax, set: model.setGraphY,
                         step: g.yStep, setStep: { model.setGraphAxis(yStep: .some($0)) })
                }
                DisclosureGroup("More Options", isExpanded: $more) {
                    VStack(alignment: .leading, spacing: 8) {
                        if !g.shows3D {
                            scale("X-Axis:", log: g.xLog) { model.setGraphLog(x: $0) }
                            scale("Y-Axis:", log: g.yLog) { model.setGraphLog(y: $0) }
                        }
                        Toggle("Lock Viewport", isOn: pref(\.lockViewport))
                            .help("Keep the window still: dragging and zooming don't move it")
                    }
                    .padding(.top, 6)
                    .padding(.leading, 4)
                }
            }
            Divider()
            Picker("Angles", selection: Binding(
                get: { model.graphPrefs.degrees ?? !model.options.angleRadians },
                set: { model.graphPrefs.degrees = $0 })) {
                Text("Radians").tag(false)
                Text("Degrees").tag(true)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
        }
        .toggleStyle(.checkbox)
        .font(.system(size: 13))
        .padding(14)
        .frame(width: 320)
    }

    /// An axis: shown or not, its label, `lo ≤ name ≤ hi` and its step (on the 3D view the second axis's window is z)
    private func axis(_ title: String, show: Binding<Bool>, label: String, setLabel: @escaping (String) -> Void, name: String,
                      lo: Double, hi: Double, set: @escaping (Double, Double) -> Void,
                      step: Double?, setStep: @escaping (Double?) -> Void) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Toggle(title, isOn: show)
                Spacer()
                TextField("Label, e.g. “\(name)”", text: Binding(get: { label }, set: setLabel))
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 130)
            }
            HStack(spacing: 4) {
                NumberField(value: lo) { set($0 ?? lo, hi) }
                Text("≤ \(name) ≤").font(.system(size: 13, design: .serif).italic()).fixedSize()
                NumberField(value: hi) { set(lo, $0 ?? hi) }
                Text("Step:").foregroundStyle(.secondary).padding(.leading, 6)
                NumberField(value: step, placeholder: "auto") { setStep($0) }.frame(width: 52)
            }
        }
    }

    private func scale(_ title: String, log: Bool, set: @escaping (Bool) -> Void) -> some View {
        HStack {
            Text(title).frame(width: 52, alignment: .leading)
            Picker(title, selection: Binding(get: { log }, set: set)) {
                Text("Linear").tag(false)
                Text("Logarithmic").tag(true)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .fixedSize()
        }
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
        TextField(placeholder, text: $text)
            .textFieldStyle(.roundedBorder)
            .monospacedDigit()
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
