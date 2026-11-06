import Foundation

/// "Stacked fractions and powers" (Settings, off by default; native only): the line is typed as a graphing
/// calculator's is. `/` puts what's before the caret over a bar and the caret under it; `^` puts the caret up in
/// a small raised power. The text stays plain maths the engine reads, `(5x)/(32+x^(3))`: each slot is a bracketed
/// group whose brackets, with the `/` and `^`, are hidden, and BarTextView lays the slots out stacked
/// (StackedLayout.swift). This is the text side: what's stacked, where the caret may stop, and the edits the keys make.
enum StackedMath {
    /// one place for typing: the characters between `open` and `close` (hidden brackets), or a bare run
    struct Slot: Equatable {
        var content: NSRange
        var open: Int?
        var close: Int?
        var lo: Int { content.location }
        var hi: Int { NSMaxRange(content) }
        var isEmpty: Bool { content.length == 0 }
        /// the caret is in the slot from its first place to its last, both ends counted
        func holds(_ caret: Int) -> Bool { caret >= lo && caret <= hi }
    }

    /// a fraction (`num` over `slot`) or a power (`slot` raised; its base is left as typed)
    struct Stack: Equatable {
        var isFraction: Bool
        /// the `/` or `^`
        var op: Int
        /// the first character the stack takes (a hidden `(` or the numerator's first), and the one after its last
        var start: Int
        var end: Int
        var num: Slot?
        var slot: Slot
        var slots: [Slot] { (num.map { [$0] } ?? []) + [slot] }
    }

    struct Parsed: Equatable {
        var stacks: [Stack] = []
        /// characters drawn as nothing: the `/` and `^`, the slots' brackets, and the `·` put after a stack
        var hidden: Set<Int> = []
        /// a `·` between a stack and what's typed after it (the engine reads `(1)/(2)x` as 1/(2x))
        var dots: Set<Int> = []
        /// caret places that look the same as the one beside them, so the caret never stops there
        var dead: Set<Int> = []
        /// the hidden `)` that ends a stack's last slot: → or `)` there steps out, backspace after it steps in
        var closers: Set<Int> = []

        /// the innermost slot the caret is in
        func slot(at caret: Int) -> Slot? {
            stacks.flatMap(\.slots).filter { $0.holds(caret) }.min { $0.content.length < $1.content.length }
        }
    }

    // ---- characters

    static func isDigit(_ c: UInt16) -> Bool { c >= 48 && c <= 57 || c == 46 }
    static func isLetter(_ c: UInt16) -> Bool {
        (c >= 65 && c <= 90) || (c >= 97 && c <= 122) || (c >= 0x391 && c <= 0x3C9) || c == 0x221E
    }
    static func isWord(_ c: UInt16) -> Bool { isDigit(c) || isLetter(c) }
    /// what starts a term: typed right after a stack it multiplies it
    static func startsTerm(_ c: UInt16) -> Bool { isWord(c) || c == 0x28 || c == 0x221A || c == 0x221B }
    /// what may come before a stacked numerator: a fraction is only drawn stacked where the engine reads it so
    static let boundary: Set<UInt16> = Set(" +-−×·*(,=<>±÷:[{|".utf16)
    /// what after a denominator would take it with it (`(a)/(b)c` is a/(bc), `(a)/(b)^2` is a/b²)
    static func binds(_ c: UInt16) -> Bool { startsTerm(c) || c == 0x5E || c == 0x21 || c == 0x25 || c == 0x5B }

    private static let open: UInt16 = 0x28, close: UInt16 = 0x29, slash: UInt16 = 0x2F, caret: UInt16 = 0x5E, dot: UInt16 = 0xB7

    static func pairs(_ u: [UInt16]) -> (closeOf: [Int: Int], openOf: [Int: Int]) {
        var closeOf: [Int: Int] = [:], openOf: [Int: Int] = [:], stack: [Int] = []
        for (i, c) in u.enumerated() {
            if c == open { stack.append(i) }
            if c == close, let o = stack.popLast() { closeOf[o] = i; openOf[i] = o }
        }
        return (closeOf, openOf)
    }

    private static func all(_ u: [UInt16], _ r: Range<Int>, _ test: (UInt16) -> Bool) -> Bool {
        !r.isEmpty && u[r].allSatisfy(test)
    }

    // ---- what's stacked

    /// The fractions and powers in `text`. `exclude`: the ∫ Σ Π limits, whose `^` are theirs.
    static func parse(_ text: String, exclude: [NSRange] = []) -> Parsed {
        let u = Array(text.utf16), n = u.count
        let (closeOf, openOf) = pairs(u)
        var p = Parsed()
        func boundaryOK(_ j: Int) -> Bool { j == 0 || boundary.contains(u[j - 1]) }
        func excluded(_ i: Int) -> Bool { exclude.contains { NSLocationInRange(i, $0) } }

        for i in 0..<n where u[i] == slash && !excluded(i) {
            guard i > 0, i + 1 < n else { continue }
            // the numerator: a bracketed group (its brackets hidden), a function's call, a number or a name
            var num: Slot
            if u[i - 1] == close, let o = openOf[i - 1] {
                var j = o
                while j > 0, isLetter(u[j - 1]) { j -= 1 }
                if j < o {
                    guard boundaryOK(j) else { continue }
                    num = Slot(content: NSRange(location: j, length: i - j))
                } else {
                    guard boundaryOK(o) else { continue }
                    num = Slot(content: NSRange(location: o + 1, length: i - 1 - (o + 1)), open: o, close: i - 1)
                }
            } else {
                var j = i
                while j > 0, isWord(u[j - 1]) { j -= 1 }
                guard all(u, j..<i, isDigit) || all(u, j..<i, isLetter), boundaryOK(j) else { continue }
                num = Slot(content: NSRange(location: j, length: i - j))
            }
            // the denominator: a bracketed group, or a number or a name
            var den: Slot
            let after: Int
            if u[i + 1] == open, let c = closeOf[i + 1] {
                den = Slot(content: NSRange(location: i + 2, length: c - (i + 2)), open: i + 1, close: c)
                after = c + 1
            } else {
                var j = i + 1
                while j < n, isWord(u[j]) { j += 1 }
                guard all(u, (i + 1)..<j, isDigit) || all(u, (i + 1)..<j, isLetter) else { continue }
                den = Slot(content: NSRange(location: i + 1, length: j - (i + 1)))
                after = j
            }
            if after < n, binds(u[after]) { continue }
            let s = Stack(isFraction: true, op: i, start: num.open ?? num.lo, end: after, num: num, slot: den)
            p.stacks.append(s)
            p.hidden.insert(i)
            for k in [num.open, num.close, den.open, den.close].compactMap({ $0 }) { p.hidden.insert(k) }
            if num.close != nil { p.dead.insert(i) }
            if den.open != nil { p.dead.insert(i + 1) }
            if let c = den.close { p.closers.insert(c) }
        }

        for i in 0..<n where u[i] == caret && !excluded(i) {
            guard i > 0, i + 1 < n, isWord(u[i - 1]) || u[i - 1] == close || u[i - 1] == 0x21 else { continue }
            var exp: Slot
            if u[i + 1] == open, let c = closeOf[i + 1] {
                exp = Slot(content: NSRange(location: i + 2, length: c - (i + 2)), open: i + 1, close: c)
            } else {
                var j = i + 1
                while j < n, isDigit(u[j]) { j += 1 }
                if j == i + 1, isLetter(u[i + 1]), i + 2 >= n || !isLetter(u[i + 2]) { j = i + 2 }
                guard j > i + 1 else { continue }
                exp = Slot(content: NSRange(location: i + 1, length: j - (i + 1)))
            }
            p.stacks.append(Stack(isFraction: false, op: i, start: i, end: exp.close.map { $0 + 1 } ?? exp.hi, num: nil, slot: exp))
            p.hidden.insert(i)
            if let o = exp.open, let c = exp.close {
                p.hidden.insert(o)
                p.hidden.insert(c)
                p.dead.insert(i + 1)
                p.closers.insert(c)
            }
        }

        // the `·` between a stack and the term typed after it is drawn as a small gap
        for s in p.stacks where s.slot.close != nil {
            let k = s.end
            if k + 1 < n, u[k] == dot, startsTerm(u[k + 1]) {
                p.dots.insert(k)
                p.hidden.insert(k)
                p.dead.insert(k + 1)
            }
        }
        p.stacks.sort { $0.start != $1.start ? $0.start < $1.start : $0.end > $1.end }
        return p
    }

    // ---- the keys

    struct Edit: Equatable { var text: String; var caret: Int }

    private static func splice(_ u: [UInt16], _ r: Range<Int>, _ with: String) -> String {
        String(utf16CodeUnits: Array(u[..<r.lowerBound]), count: r.lowerBound) + with
            + String(utf16CodeUnits: Array(u[r.upperBound...]), count: u.count - r.upperBound)
    }
    private static func str(_ u: [UInt16], _ r: Range<Int>) -> String { String(utf16CodeUnits: Array(u[r]), count: r.count) }

    /// Where the term that ends at `c` starts, as a graphing calculator takes it for `/` or `^`: a number, a name,
    /// a function's call, a bracketed group, a root, and the powers and fractions already made of them.
    static func termStart(_ u: [UInt16], _ c: Int) -> Int {
        let (_, openOf) = pairs(u)
        var j = c
        while j > 0 {
            let ch = u[j - 1]
            if ch == close, let o = openOf[j - 1] { j = o; continue }
            if isWord(ch) || ch == 0x221A || ch == 0x221B || ch == 0x21 { j -= 1; continue }
            if j < c, ch == caret, j >= 2 { j -= 1; continue }
            if j < c, ch == slash, j >= 2, u[j - 2] == close, u[j] == open { j -= 1; continue }
            break
        }
        return j
    }

    /// `/` typed: what's selected, or the term before the caret, goes over the bar, and the caret into the
    /// denominator. With nothing before it the caret goes into an empty numerator.
    static func slashKey(_ text: String, start: Int, end: Int) -> Edit {
        let u = Array(text.utf16)
        if end > start {
            let t = str(u, start..<end)
            return Edit(text: splice(u, start..<end, "(" + t + ")/()"), caret: end + 4)
        }
        let c = start, j = termStart(u, c)
        if j == c { return Edit(text: splice(u, c..<c, "()/()"), caret: c + 1) }
        let (closeOf, _) = pairs(u)
        let group = u[j] == open && closeOf[j] == c - 1
        let plain = all(u, j..<c, isDigit) || all(u, j..<c, isLetter)
        if group || (plain && (j == 0 || boundary.contains(u[j - 1]))) {
            return Edit(text: splice(u, c..<c, "/()"), caret: c + 2)
        }
        return Edit(text: splice(u, j..<c, "(" + str(u, j..<c) + ")/()"), caret: c + 4)
    }

    /// `^` typed: the caret goes up into an empty power. A fraction or a power before it is bracketed first
    /// (½ then ^ is (½)³, not 1/2³). Nil with nothing before the caret to raise.
    static func powerKey(_ text: String, caret c: Int) -> Edit? {
        let u = Array(text.utf16)
        guard c > 0 else { return nil }
        let j = termStart(u, c)
        guard j < c else { return nil }
        let p = parse(text)
        // the term ends with a stack (it isn't a bracketed group on its own): bracket it
        if p.stacks.contains(where: { $0.end == c && $0.start >= j }) {
            return Edit(text: splice(u, j..<c, "(" + str(u, j..<c) + ")^()"), caret: c + 4)
        }
        return Edit(text: splice(u, c..<c, "^()"), caret: c + 2)
    }

    /// `)` typed right before a slot's hidden `)`: the caret steps out instead
    static func closeKey(_ text: String, caret c: Int) -> Int? {
        let p = parse(text)
        if p.closers.contains(c) { return c + 1 }
        if p.stacks.contains(where: { $0.num?.close == c }) { return c + 2 } // out of the numerator, into the denominator
        return nil
    }

    /// A term typed right after a stack: a `·` goes first, so the engine multiplies it by the whole stack
    static func termAfterStack(_ text: String, caret c: Int, typed: String) -> Edit? {
        let u = Array(text.utf16)
        guard c > 0, let first = typed.utf16.first, startsTerm(first), parse(text).closers.contains(c - 1) else { return nil }
        return Edit(text: splice(u, c..<c, "·" + typed), caret: c + 1 + (typed as NSString).length)
    }

    /// Backspace: after a stack it steps in (the last slot's end); at the start of a slot it takes the stack
    /// apart, its slots' text left in the line. Nil leaves the key to the line.
    static func backspace(_ text: String, caret c: Int) -> Edit? {
        let u = Array(text.utf16), p = parse(text)
        guard c > 0 else { return nil }
        if p.closers.contains(c - 1) { return Edit(text: text, caret: c - 1) }
        // the one term typed after a stack: its `·` goes with it
        if c >= 3, u[c - 2] == dot, p.closers.contains(c - 3), startsTerm(u[c - 1]), c == u.count || !startsTerm(u[c]) {
            return Edit(text: splice(u, (c - 2)..<c, ""), caret: c - 2)
        }
        for s in p.stacks.reversed() {
            let groups = s.slots.filter { $0.open != nil }
            guard let hit = groups.first(where: { $0.lo == c }) else { continue }
            let end = p.dots.contains(s.end) ? s.end + 1 : s.end
            if s.isFraction, let num = s.num {
                let a = str(u, num.lo..<num.hi), b = str(u, s.slot.lo..<s.slot.hi)
                return Edit(text: splice(u, s.start..<end, a + b), caret: s.start + (hit == num ? 0 : (a as NSString).length))
            }
            return Edit(text: splice(u, s.op..<end, str(u, s.slot.lo..<s.slot.hi)), caret: s.op)
        }
        return nil
    }

    /// The caret moved onto a place it never stops: on, the way it was going
    static func snap(_ p: Parsed, _ caret: Int, from old: Int, length: Int) -> Int {
        var c = caret
        let step = caret >= old ? 1 : -1
        while p.dead.contains(c), c + step >= 0, c + step <= length { c += step }
        return c
    }

    // ---- selections and deletions that would cut a stack

    /// a selection grown so it never takes part of a stack: one inside a single slot stays, one that reaches
    /// past a slot takes the whole stack (and the hidden `·` after one it takes whole)
    static func expand(_ p: Parsed, _ r: NSRange, length: Int) -> NSRange {
        guard r.length > 0 else { return r }
        var lo = r.location, hi = NSMaxRange(r), changed = true
        while changed {
            changed = false
            for s in p.stacks where lo < s.end && hi > s.start {
                if lo <= s.start && hi >= s.end { continue }
                if s.slots.contains(where: { lo >= $0.lo && hi <= $0.hi }) { continue }
                lo = min(lo, s.start)
                hi = max(hi, s.end)
                changed = true
            }
        }
        if hi < length, p.dots.contains(hi), p.stacks.contains(where: { $0.end == hi && $0.start >= lo }) { hi += 1 }
        return NSRange(location: lo, length: hi - lo)
    }

    /// what ⌥⌫ / ⌘⌫ (and ⌥⌦ / ⌘⌦ going on) take: from the caret to `to`, kept inside the slot the caret is in, and
    /// grown to whole stacks outside one. nil when the caret is at its slot's edge (backspace there takes the
    /// stack apart instead)
    static func deletion(_ p: Parsed, caret: Int, to: Int, length: Int, next: ((Int) -> Bool)? = nil) -> NSRange? {
        var lo = min(caret, to), hi = max(caret, to)
        if let slot = p.slot(at: caret) {
            lo = max(lo, slot.lo)
            hi = min(hi, slot.hi)
        }
        guard hi > lo else { return nil }
        var r = expand(p, NSRange(location: lo, length: hi - lo), length: length)
        // the hidden `·` before what's taken goes too when no term is left after it to keep apart from the stack
        if r.location > 0, p.dots.contains(r.location - 1), !(next?(NSMaxRange(r)) ?? false) {
            r = NSRange(location: r.location - 1, length: r.length + 1)
        }
        return r
    }

    /// a deletion that would take all of a slot with no hidden brackets (`42/(12)`'s 42, `x^2`'s 2): the `/` or
    /// `^` would be left with nothing on that side and the stack would come apart, so the slot is left as an empty
    /// `()` instead. The slot's first place, or nil when `r` doesn't empty one.
    static func emptying(_ p: Parsed, _ r: NSRange) -> Int? {
        guard r.length > 0 else { return nil }
        return p.stacks.flatMap(\.slots).first { $0.open == nil && $0.content == r }?.lo
    }
}
