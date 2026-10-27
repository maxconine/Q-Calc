import SwiftUI

/// An svg path's `d` (absolute M, L, H, V, C, Q, Z, as the games' TS wrote them) as a SwiftUI Path.
enum SVGPath {
    private static var cache: [String: Path] = [:]

    static func path(_ d: String) -> Path {
        if let p = cache[d] { return p }
        let p = parse(d)
        if cache.count > 4000 { cache.removeAll() }
        cache[d] = p
        return p
    }

    static func parse(_ d: String) -> Path {
        var path = Path()
        var nums: [Double] = []
        var cmd: Character = "M"
        var cur = CGPoint.zero
        func flush() {
            var i = 0
            func take(_ n: Int) -> [CGFloat]? {
                guard i + n <= nums.count else { return nil }
                defer { i += n }
                return nums[i..<i + n].map { CGFloat($0) }
            }
            switch cmd {
            case "M", "L":
                var first = cmd == "M"
                while let p = take(2) {
                    cur = CGPoint(x: p[0], y: p[1])
                    if first { path.move(to: cur); first = false } else { path.addLine(to: cur) }
                }
            case "H":
                while let p = take(1) { cur = CGPoint(x: p[0], y: cur.y); path.addLine(to: cur) }
            case "V":
                while let p = take(1) { cur = CGPoint(x: cur.x, y: p[0]); path.addLine(to: cur) }
            case "C":
                while let p = take(6) {
                    cur = CGPoint(x: p[4], y: p[5])
                    path.addCurve(to: cur, control1: CGPoint(x: p[0], y: p[1]), control2: CGPoint(x: p[2], y: p[3]))
                }
            case "Q":
                while let p = take(4) {
                    cur = CGPoint(x: p[2], y: p[3])
                    path.addQuadCurve(to: cur, control: CGPoint(x: p[0], y: p[1]))
                }
            case "Z", "z":
                path.closeSubpath()
            default:
                break
            }
            nums.removeAll()
        }
        var token = ""
        func endNumber() {
            if !token.isEmpty, let v = Double(token) { nums.append(v) }
            token = ""
        }
        for ch in d {
            if ch.isLetter && ch != "e" && ch != "E" {
                endNumber()
                flush()
                cmd = ch
            } else if ch == " " || ch == "," {
                endNumber()
            } else if ch == "-" && !token.isEmpty && !token.hasSuffix("e") && !token.hasSuffix("E") {
                endNumber()
                token = "-"
            } else {
                token.append(ch)
            }
        }
        endNumber()
        flush()
        return path
    }
}
