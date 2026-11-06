import AppKit
import QCalcCore
import SwiftUI

// Native only (native/MATRIX_PLAN.md, Phase 4). The matrix grid: `matrix 3x3` anywhere in a line opens rows ×
// columns of fields under the bar on ↵, written back into the line as `[a b; c d]` as they're typed, so the answer
// stays live (`det(matrix 3x3)`). ⌃G opens the matrix under the caret (else the last one, else a new 2×2) to edit.
// The steps: a line whose answer has row operations (rref, inv, solve, A \ b, det, rank) gets a chip under the
// preview; it (or ⌃R) opens a card of each operation with the matrix after it, typeset.

/// The grid's fields and where in the line it writes.
struct GridState: Equatable {
    var cells: [[String]]
    /// UTF-16 offsets of the matrix it wrote
    var start: Int
    var end: Int
    /// the line as the grid last left it; any other line closes it
    var written: String

    var rows: Int { cells.count }
    var cols: Int { cells.first?.count ?? 0 }
}

struct GridCell: Hashable {
    let r: Int
    let c: Int
}

/// One step as the card shows it: the operation, and the matrix after it (typeset once Typst has it).
struct StepRow: Identifiable, Equatable {
    let id: Int
    let op: String
    let display: String
    var paths: TypstPaths?
    /// a closing remark (`det = …`): words only
    let remark: Bool

    static func == (a: StepRow, b: StepRow) -> Bool {
        a.id == b.id && a.op == b.op && a.display == b.display && (a.paths == nil) == (b.paths == nil) && a.remark == b.remark
    }

    /// the typeset matrix's height in the card: the page, at most 72 pt as the preview row allows
    var height: CGFloat {
        guard let paths else { return 22 }
        return min(CGFloat(paths.heightPt), 72)
    }
}

extension BarModel {
    // MARK: the grid

    func openGrid(_ spot: GridSpot) {
        grid = GridState(cells: spot.cells, start: Int(spot.start), end: Int(spot.end), written: text)
        stepsShown = false
        writeGrid()
        gridFocus = GridCell(r: 0, c: 0)
        track("matrix.grid")
    }

    /// ⌃G: the matrix under the caret in the grid (else the last in the line, else a new 2×2 at the caret);
    /// again, the grid closes
    func toggleGrid() {
        if grid != nil {
            closeGrid()
            return
        }
        let caret = fieldCaret?() ?? (text as NSString).length
        if let spot = matrixGridAt(text: text, caret: UInt32(caret)) {
            openGrid(spot)
        } else if let spot = matrixGridCommand(text: text) {
            openGrid(spot)
        } else {
            openGrid(GridSpot(start: UInt32(caret), end: UInt32(caret), cells: [["", ""], ["", ""]]))
        }
    }

    func closeGrid() {
        grid = nil
        gridFocus = nil
        focusField?()
    }

    func setGridCell(_ cell: GridCell, _ value: String) {
        guard var g = grid, g.cells.indices.contains(cell.r), g.cells[cell.r].indices.contains(cell.c),
              g.cells[cell.r][cell.c] != value else { return }
        g.cells[cell.r][cell.c] = value
        grid = g
        writeGrid()
    }

    /// a row or column more (or fewer), from 1 to the most the grid has
    func resizeGrid(rows dr: Int = 0, cols dc: Int = 0) {
        guard var g = grid else { return }
        let most = Int(matrixGridMax())
        let rows = max(1, min(most, g.rows + dr)), cols = max(1, min(most, g.cols + dc))
        guard rows != g.rows || cols != g.cols else { return }
        g.cells = (0..<rows).map { r in
            (0..<cols).map { c in g.cells.indices.contains(r) && g.cells[r].indices.contains(c) ? g.cells[r][c] : "" }
        }
        grid = g
        if let f = gridFocus, f.r >= rows || f.c >= cols { gridFocus = GridCell(r: min(f.r, rows - 1), c: min(f.c, cols - 1)) }
        writeGrid()
    }

    /// the cells into the line, in the grid's place; the caret after the matrix
    func writeGrid() {
        guard var g = grid else { return }
        let before = text as NSString
        guard g.start <= g.end, g.end <= before.length else { closeGrid(); return }
        let tail = before.length - g.end
        let literal = matrixGridLiteral(cells: g.cells)
        let line = before.replacingCharacters(in: NSRange(location: g.start, length: g.end - g.start), with: literal)
        writingGrid = true
        if let fill = fillParensInField {
            fill(line, g.start + (literal as NSString).length)
        } else {
            text = line
        }
        writingGrid = false
        // the field may write the line its own way (symbols); the matrix ends where the rest begins
        g.end = (text as NSString).length - tail
        g.written = text
        grid = g
    }

    /// ↵ in a cell: down the column; from the last row the grid closes with the line ready
    func gridEnter(_ cell: GridCell) {
        guard let g = grid else { return }
        if cell.r + 1 < g.rows {
            gridFocus = GridCell(r: cell.r + 1, c: cell.c)
        } else {
            closeGrid()
        }
    }

    /// Tab / ⇧Tab: along the row, then on to the next
    func gridTab(_ cell: GridCell, back: Bool) {
        guard let g = grid else { return }
        let n = g.rows * g.cols, i = cell.r * g.cols + cell.c
        let j = (i + (back ? n - 1 : 1)) % n
        gridFocus = GridCell(r: j / g.cols, c: j % g.cols)
    }

    // MARK: the steps

    /// ⌃R or the chip: the row operations behind the answer, or put them away
    func toggleSteps() {
        guard let steps = live.matrix?.steps, !steps.isEmpty else {
            stepsShown = false
            return
        }
        stepsShown.toggle()
        if stepsShown { track("matrix.steps") }
        refreshSteps()
    }

    /// The card's rows for the answer shown: the operations at once, their matrices typeset after.
    func refreshSteps() {
        // a new line starts with the card shut
        if text.isEmpty { stepsShown = false }
        guard stepsShown, let steps = live.matrix?.steps, !steps.isEmpty else {
            stepsAsked = nil
            stepRows = []
            return
        }
        let docs = settings.typstPreview ? steps.map(\.matrixDocument) : steps.map { _ in "" }
        guard docs != stepsAsked else { return }
        stepsAsked = docs
        let rows = steps.enumerated().map { i, s in
            StepRow(id: i, op: s.op, display: s.display, paths: nil, remark: s.matrixDocument.isEmpty)
        }
        // text first only when there's nothing up yet, so a card being edited doesn't jump
        if stepRows.isEmpty || !settings.typstPreview { stepRows = rows }
        guard settings.typstPreview else { return }
        typesetter.render(docs) { [weak self] paths in
            guard let self, self.stepsAsked == docs else { return }
            self.stepRows = rows.map { r in
                var r = r
                r.paths = paths.indices.contains(r.id) ? paths[r.id] : nil
                return r
            }
        }
    }
}

private extension View {
    /// ↑ ↓ ← → between cells (← → only from the ends of a cell's text), Tab / ⇧Tab along the rows; key presses
    /// need macOS 14, so on 12 and 13 Tab moves by the key view loop instead
    @ViewBuilder func gridKeys(move: @escaping (Int, Int) -> Bool, tab: @escaping (Bool) -> Void) -> some View {
        if #available(macOS 14.0, *) {
            self.onKeyPress(.upArrow) { move(-1, 0) ? .handled : .ignored }
                .onKeyPress(.downArrow) { move(1, 0) ? .handled : .ignored }
                .onKeyPress(.leftArrow) { move(0, -1) ? .handled : .ignored }
                .onKeyPress(.rightArrow) { move(0, 1) ? .handled : .ignored }
                .onKeyPress(.tab, phases: .down) { press in tab(press.modifiers.contains(.shift)); return .handled }
                .onKeyPress(KeyEquivalent("\u{19}"), phases: .down) { _ in tab(true); return .handled }
        } else {
            self
        }
    }
}

/// The grid under the bar, in the system panel's card: cells on --bg, and a row of size buttons.
struct MatrixGridPanel: View {
    @ObservedObject var model: BarModel
    @FocusState private var focused: GridCell?

    private static let cellWidth: CGFloat = 64

    var body: some View {
        let g = model.grid ?? GridState(cells: [], start: 0, end: 0, written: "")
        VStack(alignment: .leading, spacing: 8) {
            ScrollView(.horizontal, showsIndicators: false) {
                VStack(spacing: 4) {
                    ForEach(0..<g.rows, id: \.self) { r in
                        HStack(spacing: 4) {
                            ForEach(0..<g.cols, id: \.self) { c in
                                cell(GridCell(r: r, c: c), g.cells[r][c])
                            }
                        }
                    }
                }
                .padding(.horizontal, 2)
                .overlay(alignment: .leading) { bracket(left: true) }
                .overlay(alignment: .trailing) { bracket(left: false) }
                .padding(.horizontal, 8)
            }
            .frame(height: CGFloat(g.rows) * 34 + CGFloat(max(0, g.rows - 1)) * 4)
            HStack(spacing: 14) {
                sizer("rows", count: g.rows, less: { model.resizeGrid(rows: -1) }, more: { model.resizeGrid(rows: 1) })
                sizer("columns", count: g.cols, less: { model.resizeGrid(cols: -1) }, more: { model.resizeGrid(cols: 1) })
                Spacer(minLength: 0)
                Text("tab next · ↵ down · esc done")
                    .font(.system(size: Theme.small))
                    .foregroundStyle(Color.qMuted)
                    .lineLimit(1)
            }
        }
        .padding(10)
        .background(RoundedRectangle(cornerRadius: Theme.radiusCard).fill(Color.qBgTape)
            .shadow(color: .black.opacity(0.06), radius: 1.5, y: 1))
        .overlay(RoundedRectangle(cornerRadius: Theme.radiusCard).strokeBorder(Color.black.opacity(0.1), lineWidth: 0.5))
        .padding(.horizontal, 12)
        .padding(.bottom, 12)
        .onAppear { focused = model.gridFocus ?? GridCell(r: 0, c: 0) }
        .onChange(of: model.gridFocus) { f in if let f { focused = f } }
        .onChange(of: focused) { f in if let f, f != model.gridFocus { model.gridFocus = f } }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Matrix, \(g.rows) by \(g.cols)")
    }

    private func cell(_ at: GridCell, _ value: String) -> some View {
        TextField("", text: Binding(get: { value }, set: { model.setGridCell(at, $0) }))
            .textFieldStyle(.plain)
            .multilineTextAlignment(.center)
            .font(.system(size: Theme.row))
            .mutedPrompt("0", shown: value.isEmpty, size: Theme.row, color: Color.qMuted.opacity(0.5),
                         kerning: Theme.displayTracking(Theme.row))
            .foregroundStyle(Color.qInk)
            .padding(.horizontal, 6)
            .frame(width: Self.cellWidth, height: 34)
            .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(Color.qBg))
            .overlay(RoundedRectangle(cornerRadius: Theme.radiusS)
                .strokeBorder(focused == at ? Color.qAccent.opacity(0.55) : .clear, lineWidth: 1))
            .noWritingTools()
            .tint(Color.qInk)
            .autocorrectionDisabled()
            .focused($focused, equals: at)
            .onSubmit { model.gridEnter(at) }
            .gridKeys(move: { dr, dc in move(from: at, value: value, dr: dr, dc: dc) },
                      tab: { model.gridTab(at, back: $0) })
            .accessibilityLabel("Row \(at.r + 1), column \(at.c + 1)")
    }

    /// arrows between cells; ← and → only once the caret is at that end of the cell's text
    private func move(from at: GridCell, value: String, dr: Int, dc: Int) -> Bool {
        guard let g = model.grid else { return false }
        if dc != 0, let editor = NSApp.keyWindow?.firstResponder as? NSTextView {
            let sel = editor.selectedRange(), length = (editor.string as NSString).length
            if sel.length > 0 || (dc < 0 && sel.location > 0) || (dc > 0 && sel.location < length) { return false }
        }
        let r = at.r + dr, c = at.c + dc
        guard (0..<g.rows).contains(r), (0..<g.cols).contains(c) else {
            // ↑ from the top row goes back to the line
            if dr < 0 { model.focusField?() }
            return dr < 0
        }
        model.gridFocus = GridCell(r: r, c: c)
        return true
    }

    /// the matrix's square brackets, drawn either side of the cells
    private func bracket(left: Bool) -> some View {
        GeometryReader { geo in
            Path { p in
                let w = geo.size.width, h = geo.size.height
                let inner: CGFloat = left ? w : 0, outer: CGFloat = left ? 0 : w
                p.move(to: CGPoint(x: inner, y: 0.6))
                p.addLine(to: CGPoint(x: outer, y: 0.6))
                p.addLine(to: CGPoint(x: outer, y: h - 0.6))
                p.addLine(to: CGPoint(x: inner, y: h - 0.6))
            }
            .stroke(Color.qMuted.opacity(0.7), lineWidth: 1.2)
        }
        .frame(width: 5)
        .offset(x: left ? -8 : 8)
        .accessibilityHidden(true)
    }

    private func sizer(_ name: String, count: Int, less: @escaping () -> Void, more: @escaping () -> Void) -> some View {
        HStack(spacing: 4) {
            Text("\(count) \(name)")
                .font(.system(size: Theme.small))
                .monospacedDigit()
                .foregroundStyle(Color.qMuted)
            SizeButton(symbol: "minus", help: "One \(name.dropLast()) fewer", action: less)
            SizeButton(symbol: "plus", help: "One \(name.dropLast()) more", action: more)
        }
    }
}

private struct SizeButton: View {
    let symbol: String
    let help: String
    let action: () -> Void
    @State private var hover = false

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 9, weight: .semibold))
                .frame(width: 18, height: 18)
                .background(Circle().fill(Color.qInk.opacity(hover ? 0.12 : 0.06)))
        }
        .buttonStyle(.plain)
        .foregroundStyle(Color.qMuted)
        .onHover { hover = $0 }
        .hoverTip(help)
        .accessibilityLabel(help)
    }
}

/// `steps · 4` under the preview, muted, as a link: opens the card (⌃R too)
struct StepsChip: View {
    @ObservedObject var model: BarModel
    let count: Int
    @State private var hover = false

    var body: some View {
        HStack(spacing: 6) {
            Button { model.toggleSteps() } label: {
                HStack(spacing: 4) {
                    Image(systemName: model.stepsShown ? "chevron.down" : "chevron.right")
                        .font(.system(size: 9, weight: .semibold))
                    Text(model.stepsShown ? "hide steps" : "show steps · \(count)")
                        .underline(hover)
                }
            }
            .buttonStyle(.plain)
            .onHover { hover = $0 }
            .hoverTip("The row operations behind this answer · ⌃R")
            .accessibilityLabel(model.stepsShown ? "Hide steps" : "Show \(count) steps")
            Spacer(minLength: 0)
        }
        .font(.system(size: Theme.small))
        .foregroundStyle(Color.qMuted)
        .padding(.horizontal, Theme.gutter)
        .padding(.bottom, model.stepsShown ? 6 : 10)
        .padding(.top, -4)
    }
}

/// Each row operation with the matrix after it: the operation on the left, muted; the matrix typeset beside it.
struct StepsPanel: View {
    @ObservedObject var model: BarModel

    private static let opWidth: CGFloat = 168
    private static let spacing: CGFloat = 6
    private static let most: CGFloat = 320

    var body: some View {
        let rows = model.stepRows
        let total = rows.reduce(0) { $0 + $1.height } + CGFloat(max(0, rows.count - 1)) * Self.spacing
        ScrollView(.vertical) {
            VStack(alignment: .leading, spacing: Self.spacing) {
                ForEach(rows) { row in
                    if row.remark {
                        Text(Pretty.matrix(row.op))
                            .font(.system(size: Theme.body))
                            .foregroundStyle(Color.qInk)
                            .frame(maxWidth: .infinity, minHeight: row.height, alignment: .leading)
                            .textSelection(.enabled)
                    } else {
                        HStack(alignment: .center, spacing: 10) {
                            Text(Pretty.matrix(row.op))
                                .font(.system(size: Theme.body))
                                .foregroundStyle(Color.qMuted)
                                .frame(width: Self.opWidth, alignment: .leading)
                                .lineLimit(2)
                                .fixedSize(horizontal: false, vertical: true)
                            if let paths = row.paths {
                                MathPreview(typeset: Typeset(paths: paths, doc: ""), leading: true)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                                    .frame(height: row.height)
                            } else {
                                Text(row.display)
                                    .font(.system(size: Theme.body).monospacedDigit())
                                    .foregroundStyle(Color.qInk)
                                    .lineLimit(1)
                                    .truncationMode(.middle)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                            }
                        }
                        .frame(height: row.height)
                        .accessibilityElement(children: .ignore)
                        .accessibilityLabel("\(row.op): \(row.display)")
                    }
                }
            }
            .padding(.vertical, 2)
        }
        .frame(height: min(total + 4, Self.most))
        .padding(10)
        .background(RoundedRectangle(cornerRadius: Theme.radiusCard).fill(Color.qBgTape)
            .shadow(color: .black.opacity(0.06), radius: 1.5, y: 1))
        .overlay(RoundedRectangle(cornerRadius: Theme.radiusCard).strokeBorder(Color.black.opacity(0.1), lineWidth: 0.5))
        .padding(.horizontal, 12)
        .padding(.bottom, 12)
        .onAppear { model.refreshSteps() }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Steps")
    }
}
