import AppKit

/// Stacked fractions and powers in the line (StackedMath.swift): each slot's characters drawn small and raised or
/// lowered, the pen pulled back so a denominator sits under its numerator, and the bar, the empty slots' boxes and a
/// caret the height of its slot drawn over the text.
struct StackBar { var anchor: Int; var shift: CGFloat; var width: CGFloat; var axis: CGFloat }
struct StackHole { var anchor: Int; var dx: CGFloat; var offset: CGFloat; var size: CGFloat }
struct SlotLook { var content: NSRange; var size: CGFloat; var offset: CGFloat }

/// TextKit 1 with one addition: a character's `pull` moves the pen by that much after it, back as well as on
/// (a kern can't take an advance below nothing), and the first character's `lead` starts the line that much further
/// on (a fraction that starts the line has no character before it to kern). The bar's line is one paragraph on one line.
final class StackTypesetter: NSATSTypesetter {
    static let pull = NSAttributedString.Key("qcalc.stackPull")
    static let lead = NSAttributedString.Key("qcalc.stackLead")

    private func pull(_ glyph: Int) -> CGFloat {
        guard let lm = layoutManager, let s = attributedString else { return 0 }
        let c = lm.characterIndexForGlyph(at: glyph)
        guard c < s.length else { return 0 }
        return (s.attribute(Self.pull, at: c, effectiveRange: nil) as? CGFloat) ?? 0
    }

    override func setLocation(_ location: NSPoint, withAdvancements advancements: UnsafePointer<CGFloat>!, forStartOfGlyphRange glyphRange: NSRange) {
        guard let s = attributedString, s.length > 0,
              s.attribute(Self.pull, existsIn: NSRange(location: 0, length: s.length)) else {
            return super.setLocation(location, withAdvancements: advancements, forStartOfGlyphRange: glyphRange)
        }
        // every pull earlier in the line moves this run too
        let start = paragraphGlyphRange.location
        let first = layoutManager.map { $0.characterIndexForGlyph(at: start) } ?? 0
        var before: CGFloat = first < s.length ? (s.attribute(Self.lead, at: first, effectiveRange: nil) as? CGFloat) ?? 0 : 0
        if glyphRange.location > start { for g in start..<glyphRange.location { before += pull(g) } }
        let at = NSPoint(x: location.x + before, y: location.y)
        guard let lm = layoutManager else { return super.setLocation(at, withAdvancements: advancements, forStartOfGlyphRange: glyphRange) }
        // each glyph gets a location of its own: TextKit answers where a glyph inside a run is (the caret, a click)
        // from the run's start and the font's advances, which leave the pulls out
        var x = at.x
        for i in 0..<glyphRange.length {
            let g = glyphRange.location + i
            super.setLocation(NSPoint(x: x, y: at.y), withAdvancements: nil, forStartOfGlyphRange: NSRange(location: g, length: 1))
            let advance: CGFloat
            if let advancements {
                advance = advancements[i]
            } else {
                let c = lm.characterIndexForGlyph(at: g)
                let font = s.attribute(.font, at: c, effectiveRange: nil) as? NSFont ?? .systemFont(ofSize: 24)
                let kern = (s.attribute(.kern, at: c, effectiveRange: nil) as? NSNumber).map { CGFloat($0.doubleValue) } ?? 0
                advance = font.advancement(forCGGlyph: lm.cgGlyph(at: g)).width + kern
            }
            x += advance + pull(g)
        }
    }

    // the line's used width with the pulls and the lead taken in, so what TextKit draws by it (the selection's
    // highlight, the line's end, the scroll to the caret) ends where the text does
    override func setLineFragmentRect(_ fragmentRect: NSRect, forGlyphRange glyphRange: NSRange, usedRect: NSRect, baselineOffset: CGFloat) {
        guard let s = attributedString, s.length > 0, glyphRange.length > 0, let lm = layoutManager,
              s.attribute(Self.pull, existsIn: NSRange(location: 0, length: s.length)) else {
            return super.setLineFragmentRect(fragmentRect, forGlyphRange: glyphRange, usedRect: usedRect, baselineOffset: baselineOffset)
        }
        let first = lm.characterIndexForGlyph(at: glyphRange.location)
        var shift = first < s.length ? (s.attribute(Self.lead, at: first, effectiveRange: nil) as? CGFloat) ?? 0 : 0
        for g in glyphRange.location..<NSMaxRange(glyphRange) { shift += pull(g) }
        var used = usedRect
        used.size.width = max(0, used.width + shift)
        super.setLineFragmentRect(fragmentRect, forGlyphRange: glyphRange, usedRect: used, baselineOffset: baselineOffset)
    }
}

private extension NSAttributedString {
    func attribute(_ key: Key, existsIn range: NSRange) -> Bool {
        var found = false
        enumerateAttribute(key, in: range) { v, _, stop in if v != nil { found = true; stop.pointee = true } }
        return found
    }
}

extension BarTextView {
    static var stackedOn: Bool { AppSettings.shared.stackedMath }

    private static let hiddenLook: [NSAttributedString.Key: Any] = [.font: NSFont.systemFont(ofSize: 0.01), .foregroundColor: NSColor.clear, .kern: 0]

    /// Lays the line's fractions and powers out stacked (from restyle, on a freshly styled line)
    func styleStacks() {
        stackBars = []
        stackHoles = []
        slotLooks = []
        caretShift = [:]
        // TextKit grows the line for a raised numerator and pushes its baseline down: the inset takes it back up
        // (and back once the line has no stacks)
        defer {
            if Self.stackedOn { spaceOperators() }
            updateInset()
            // the caret goes where the spaced line puts it (the stacks' pass placed it before the room was added)
            refreshStackCaret()
        }
        guard Self.stackedOn, let storage = textStorage, storage.length > 0, string.contains("/") || string.contains("^") else {
            stacked = StackedMath.Parsed()
            refreshStackCaret()
            return
        }
        let exclude = limits.compactMap { b -> NSRange? in
            guard let a = (b["sign"] as? NSNumber)?.intValue, let e = (b["end"] as? NSNumber)?.intValue, e > a else { return nil }
            return NSRange(location: a, length: e - a)
        }
        let p = StackedMath.parse(string, exclude: exclude)
        stacked = p
        guard !p.stacks.isEmpty else { return refreshStackCaret() }
        let n = storage.length, full = Self.fontSize
        var size = [CGFloat](repeating: full, count: n), offset = [CGFloat](repeating: 0, count: n)
        // a graphing calculator's proportions: a fraction's slots nearly the line's size, a power's well under it
        func small(_ s: StackedMath.Stack) -> CGFloat { Self.slotSize(parent: size[s.op], fraction: s.isFraction) }
        // sizes and heights, outer stacks first: a slot's are its stack's, made smaller and moved
        for s in p.stacks {
            let parent = size[s.op], base = offset[s.op], f = small(s)
            func put(_ slot: StackedMath.Slot, _ o: CGFloat) {
                slotLooks.append(SlotLook(content: slot.content, size: f, offset: o))
                for k in slot.lo..<slot.hi { size[k] = f; offset[k] = o }
            }
            if s.isFraction, let num = s.num {
                let axis = base + parent * 0.25
                // a slot with a stack of its own in it stands as far off the bar as that stack is tall
                put(num, Self.numOffset(axis: axis, f, depth: Self.extent(p, num.content, size: f).depth))
                put(s.slot, Self.denOffset(axis: axis, f, height: Self.extent(p, s.slot.content, size: f).height))
            } else {
                put(s.slot, base + parent * Self.powerRaise)
            }
        }
        for k in 0..<n {
            let r = NSRange(location: k, length: 1)
            if p.hidden.contains(k) {
                storage.setAttributes(Self.hiddenLook, range: r)
            } else if size[k] != full || offset[k] != 0 {
                storage.setAttributes([.font: NSFont.systemFont(ofSize: size[k]), .foregroundColor: Theme.ink,
                                       .kern: Theme.displayTracking(size[k]), .baselineOffset: offset[k]], range: r)
            }
        }
        func kern(_ k: Int) -> CGFloat {
            let v = storage.attribute(.kern, at: k, effectiveRange: nil)
            return (v as? CGFloat) ?? CGFloat((v as? NSNumber)?.doubleValue ?? 0)
        }
        func addKern(_ k: Int, _ v: CGFloat) { storage.addAttribute(.kern, value: kern(k) + v, range: NSRange(location: k, length: 1)) }
        func width(_ r: NSRange) -> CGFloat { r.length == 0 ? 0 : storage.attributedSubstring(from: r).size().width }
        // the pen, inner stacks first, so an outer slot's width takes in the stacks inside it
        for s in p.stacks.sorted(by: { $0.end - $0.start < $1.end - $1.start }) {
            let f = small(s), hole = f * 0.62
            if s.isFraction, let num = s.num {
                let den = s.slot
                let nw = num.isEmpty ? hole : width(num.content), dw = den.isEmpty ? hole : width(den.content)
                let pad = f * Self.barOverhang, w = max(nw, dw) + 2 * pad
                let lead = (w - nw) / 2
                var used: CGFloat = 0
                if let o = num.open {
                    addKern(o, lead + (num.isEmpty ? hole : 0))
                    used = lead
                    if num.isEmpty { stackHoles.append(StackHole(anchor: o, dx: lead, offset: Self.numOffset(axis: offset[s.op] + size[s.op] * 0.25, f), size: f)) }
                } else if num.lo > 0 {
                    addKern(num.lo - 1, lead)
                    used = lead
                } else {
                    // the line starts with this fraction: the whole line starts `lead` further on
                    let had = (storage.attribute(StackTypesetter.lead, at: 0, effectiveRange: nil) as? CGFloat) ?? 0
                    storage.addAttribute(StackTypesetter.lead, value: had + lead, range: NSRange(location: 0, length: 1))
                    used = lead
                }
                // the `/` takes the pen back to where the denominator starts, centred under the numerator (a pull:
                // TextKit never lets a kern take a glyph's advance below nothing)
                storage.addAttribute(StackTypesetter.pull, value: (w - dw) / 2 - used - nw, range: NSRange(location: s.op, length: 1))
                if let o = den.open, den.isEmpty {
                    addKern(o, hole)
                    stackHoles.append(StackHole(anchor: o, dx: 0, offset: Self.denOffset(axis: offset[s.op] + size[s.op] * 0.25, f), size: f))
                }
                // and the denominator's end takes it on past the wider of the two
                addKern(den.close ?? (den.hi - 1), (w - dw) / 2)
                stackBars.append(StackBar(anchor: num.open ?? num.lo, shift: num.open == nil ? used : 0, width: w,
                                          axis: offset[s.op] + size[s.op] * 0.25))
            } else {
                if let o = s.slot.open, s.slot.isEmpty {
                    addKern(o, hole)
                    stackHoles.append(StackHole(anchor: o, dx: 0, offset: offset[s.op] + size[s.op] * Self.powerRaise, size: f))
                }
                if let c = s.slot.close { addKern(c, f * 0.1) }
            }
        }
        for d in p.dots { addKern(d, size[d] * 0.14) }
        refreshStackCaret()
    }

    // a graphing calculator's measures, also used by the history's rows (StackedText)
    static let fractionScale: CGFloat = 0.82
    static let powerScale: CGFloat = 0.64
    static let powerRaise: CGFloat = 0.42
    /// a slot's size: a fraction's nearly the line's, one inside a slot smaller again (TeX's script style), a power's
    /// well under it
    static func slotSize(parent: CGFloat, fraction: Bool) -> CGFloat {
        let scale = fraction ? (parent < fontSize - 0.01 ? 0.7 : fractionScale) : powerScale
        return max(8, parent * scale)
    }
    static let barOverhang: CGFloat = 0.2
    /// a numerator's baseline over the bar's axis: a clear gap, plus room for a letter's tail
    static func numOffset(axis: CGFloat, _ f: CGFloat, depth: CGFloat = 0) -> CGFloat { axis + 2.5 + max(f * 0.24, depth) }
    /// a denominator's baseline under it: the gap and a digit's height (or the height of a stack in it)
    static func denOffset(axis: CGFloat, _ f: CGFloat, height: CGFloat = 0) -> CGFloat { axis - 3 - max(f * 0.74, height) }

    /// how far what's in `r` reaches above its baseline and below it, at `size`, the stacks in it taken in
    static func extent(_ p: StackedMath.Parsed, _ r: NSRange, size: CGFloat) -> (height: CGFloat, depth: CGFloat) {
        var height = size * 0.74, depth: CGFloat = 0
        let inside = p.stacks.filter { $0.start >= r.location && $0.end <= NSMaxRange(r) && $0.end - $0.start < r.length + 2 }
        // only the stacks directly in r: one inside another's slot is taken in by that one
        let direct = inside.filter { s in !inside.contains { o in o != s && o.slots.contains { $0.lo <= s.start && s.end <= $0.hi } } }
        for s in direct {
            if s.isFraction, let num = s.num {
                let f = slotSize(parent: size, fraction: true), axis = size * 0.25
                let n = extent(p, num.content, size: f), d = extent(p, s.slot.content, size: f)
                height = max(height, numOffset(axis: axis, f, depth: n.depth) + n.height)
                depth = max(depth, -denOffset(axis: axis, f, height: d.height) + d.depth)
            } else {
                let f = slotSize(parent: size, fraction: false)
                height = max(height, size * powerRaise + extent(p, s.slot.content, size: f).height)
            }
        }
        return (height, depth)
    }

    /// `+ − = < >` between two terms, `; ,` before the next: room around them, as a graphing calculator spaces
    /// them (setting on only). A sign that starts a term (`-3`, `(−x`, `1e-5`) keeps tight.
    private func spaceOperators() {
        guard let storage = textStorage, storage.length > 1 else { return }
        let u = Array(string.utf16), n = u.count
        let hidden = stacked.hidden
        func size(_ k: Int) -> CGFloat { (storage.attribute(.font, at: k, effectiveRange: nil) as? NSFont)?.pointSize ?? Self.fontSize }
        func add(_ k: Int, _ v: CGFloat) {
            let had = storage.attribute(.kern, at: k, effectiveRange: nil)
            let was = (had as? CGFloat) ?? CGFloat((had as? NSNumber)?.doubleValue ?? 0)
            storage.addAttribute(.kern, value: was + v, range: NSRange(location: k, length: 1))
        }
        // the shown character before k
        func before(_ k: Int) -> Int? { var j = k - 1; while j >= 0, hidden.contains(j) { j -= 1 }; return j >= 0 ? j : nil }
        for k in 0..<n where !hidden.contains(k) {
            let c = u[k]
            let binary: Set<UInt16> = [0x2B, 0x2D, 0x2212, 0x3D, 0x3C, 0x3E, 0x2264, 0x2265, 0xB1, 0x2260]
            if binary.contains(c) {
                guard k + 1 < n, let b = before(k) else { continue }
                let p = u[b]
                // a sign that starts a term
                guard StackedMath.isWord(p) || p == 0x29 || p == 0x5D || p == 0x21 || p == 0x25 || p == 0xB2 || p == 0xB3 || p == 0x3C0 else { continue }
                if (c == 0x2D || c == 0x2B), (p == 0x65 || p == 0x45), b > 0, StackedMath.isDigit(u[b - 1]) { continue }
                // a hidden `)` before the sign belongs to a slot that ends there: the room goes after it
                let gap = size(k) * 0.2
                add(k - 1, gap)
                add(k, gap)
                // the caret either side of the sign stands in the middle of its room, never against the sign
                caretShift[k] = gap / 2
                caretShift[k + 1] = gap / 2
            } else if c == 0x3B || c == 0x2C {
                if k + 1 < n { add(k, size(k) * 0.25); caretShift[k + 1] = size(k) * 0.125 }
            }
        }
    }

    // ---- where things are

    /// x of the caret place before character `i` (or at the end of the line)
    func stackCaretX(_ i: Int) -> CGFloat? {
        guard let lm = layoutManager, let tc = textContainer else { return nil }
        lm.ensureLayout(for: tc)
        let n = (string as NSString).length
        guard n > 0 else { return textContainerInset.width }
        let at = min(i, n - 1)
        let g = lm.glyphIndexForCharacter(at: at)
        let frag = lm.lineFragmentRect(forGlyphAt: g, effectiveRange: nil)
        var x = frag.minX + lm.location(forGlyphAt: g).x + textContainerInset.width
        if i >= n, let storage = textStorage { x += storage.attributedSubstring(from: NSRange(location: n - 1, length: 1)).size().width }
        return x
    }

    /// the line's baseline, in the view
    func stackBaseline() -> CGFloat {
        guard let lm = layoutManager, let tc = textContainer, (string as NSString).length > 0 else { return textContainerInset.height + Self.fontSize }
        lm.ensureLayout(for: tc)
        // a hidden `/` or `^` sits on the line's own baseline
        // (the parse can be the line's before an edit, until restyle: never past its end)
        let at = min(stacked.stacks.first?.op ?? 0, (string as NSString).length - 1)
        let g = lm.glyphIndexForCharacter(at: at)
        let frag = lm.lineFragmentRect(forGlyphAt: g, effectiveRange: nil)
        return textContainerInset.height + frag.minY + lm.location(forGlyphAt: g).y
    }

    /// the size and height of the slot the caret is in
    func stackLook(at caret: Int) -> SlotLook? {
        guard let slot = stacked.slot(at: caret) else { return nil }
        return slotLooks.first { $0.content == slot.content }
    }

    /// the caret drawn by the line itself while it has stacks, the height of the slot it's in (full height outside
    /// them: TextKit's own caret goes by the line's used width, which the pulls leave wider than the text)
    func stackCaretRect() -> NSRect? {
        let sel = selectedRange()
        guard sel.length == 0 else { return nil }
        if let limit = limitCaretRect(sel.location) { return limit }
        guard !stacked.stacks.isEmpty, var x = stackCaretX(sel.location) else { return nil }
        x -= caretShift[sel.location] ?? 0
        let look = stackLook(at: sel.location) ?? SlotLook(content: NSRange(location: 0, length: 0), size: Self.fontSize, offset: 0)
        // in an empty slot, the middle of its box
        if let slot = stacked.slot(at: sel.location), slot.isEmpty, let o = slot.open,
           let hole = stackHoles.first(where: { $0.anchor == o }), let ox = stackCaretX(o) {
            x = ox + hole.dx + hole.size * 0.31
        }
        let base = stackBaseline()
        return NSRect(x: x - 0.75, y: base - look.offset - look.size * 0.92, width: 1.5, height: look.size * 1.15)
    }

    /// the system caret hides while the line draws its own
    func refreshStackCaret() {
        let rect = stackCaretRect()
        if let old = stackCaret { setNeedsDisplay(old.insetBy(dx: -2, dy: -2)) }
        stackCaret = rect
        stackCaretShown = true
        super.insertionPointColor = rect == nil ? caretInk : .clear
        stackBlink?.invalidate()
        stackBlink = nil
        if let rect {
            setNeedsDisplay(rect.insetBy(dx: -2, dy: -2))
            stackBlink = Timer.scheduledTimer(withTimeInterval: 0.53, repeats: true) { [weak self] _ in
                guard let self, let r = self.stackCaret else { return }
                self.stackCaretShown.toggle()
                self.setNeedsDisplay(r.insetBy(dx: -2, dy: -2))
            }
        }
    }

    func drawStacks() {
        guard !stackBars.isEmpty || !stackHoles.isEmpty || stackCaret != nil else { return }
        let base = stackBaseline()
        Theme.ink.setFill()
        for b in stackBars {
            guard let x = stackCaretX(b.anchor) else { continue }
            NSRect(x: x - b.shift, y: base - b.axis - 0.6, width: b.width, height: 1.2).fill()
        }
        for h in stackHoles {
            guard let x = stackCaretX(h.anchor) else { continue }
            let box = NSRect(x: x + h.dx, y: base - h.offset - h.size * 0.72, width: h.size * 0.62, height: h.size * 0.8)
            Theme.muted.withAlphaComponent(0.18).setFill()
            NSBezierPath(roundedRect: box, xRadius: h.size * 0.14, yRadius: h.size * 0.14).fill()
        }
        if let r = stackCaret, stackCaretShown, window?.firstResponder === self, window?.isKeyWindow == true {
            caretInk.setFill()
            r.fill()
        }
    }

    /// ↑ from a denominator to its numerator, ↓ back: the place nearest across from the caret
    func stackVertical(up: Bool) -> Int? {
        let c = selectedRange().location
        let fractions = stacked.stacks.filter { $0.isFraction && (up ? $0.slot.holds(c) : $0.num?.holds(c) == true) }
        guard let s = fractions.min(by: { $0.end - $0.start < $1.end - $1.start }), let to = up ? s.num : s.slot,
              let x = stackCaretX(c) else { return nil }
        return (to.lo...to.hi).filter { !stacked.dead.contains($0) }.min { abs((stackCaretX($0) ?? 0) - x) < abs((stackCaretX($1) ?? 0) - x) }
    }

    /// the caret place nearest a click, by its slot's height too
    func stackStop(near p: NSPoint) -> Int? {
        let n = (string as NSString).length
        let base = stackBaseline()
        var best: (Int, CGFloat)?
        for i in 0...n where !stacked.dead.contains(i) {
            guard let x = stackCaretX(i) else { continue }
            let look = stackLook(at: i)
            let y = base - (look?.offset ?? 0) - (look?.size ?? Self.fontSize) * 0.35
            let d = abs(x - p.x) + 1.5 * abs(y - p.y)
            if best == nil || d < best!.1 { best = (i, d) }
        }
        return best?.0
    }
}
